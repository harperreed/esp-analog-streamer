# ABOUTME: Plays the board's MP3 and FLAC streams through real players, one at a time, across one receiver-side stall.
# ABOUTME: Adapters, progress readers and verdicts are pure; tc via sudo -n, the UART and /api/metrics run only from main.
import argparse
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

from decoder_integrity import FrameHash
from device_metrics import fetch
from network_fault import (JOINED, LEFT, STREAM_PORT, Failure, board_address, injection_commands, interface_name, metrics_sample,
                           preinstall_problem, privileged, qdisc_plan, qdisc_teardown, require_unprivileged, restore_matches,
                           route_interface, skip_fields, stall_profile)
from sonos_http_check import CleanupDeadline, cleanup_attempt
from stream_soak import DECODER_RW_TIMEOUT_S, Tail, stop

FORMATS = ('mp3', 'flac')
# Progress must advance inside this window before the stop for the player to count as still receiving.
RECEIVING_WINDOW_S = 5.0
POLL_S = 1.0
# Per pair: the player's run plus its stall, then the UART settle for the departure line and the stop.
PAIR_MARGIN_S = 10.0
SETTLE_S = 3.0
# Self-bounded players (ffmpeg -t, curl --max-time) outlive the driver's stop by this much, never less.
PLAYER_BOUND_MARGIN_S = 5.0
DEFAULT_SECONDS = 30.0
DEFAULT_STALL_S = 4.0
DEFAULT_STEADY_S = 10.0


def positive_seconds(value):
    seconds = float(value)
    if not seconds > 0:
        raise ValueError('seconds must be positive')
    return seconds


class Adapter:
    """One player: how to run it headless against a URL and how to read its progress and its connection count."""
    name = ''
    formats = FORMATS
    unit = ''

    def pipeline(self, url, fmt, out, label, seconds, stall_s, scratch):
        """Ordered argv lists; each process's stdout feeds the next one's stdin, stderr goes to <label>.<argv0>.stderr."""
        raise NotImplementedError

    def reader(self, fmt, out, label, scratch):
        """Object with progress() -> (value or None, detail dict) and close(), reading the player's real output."""
        raise NotImplementedError

    def connections(self, log_text):
        """TCP connections the player's own log admits to, or None when the player keeps no such log."""
        return None

    def connection_log(self, label):
        return None


def bound_seconds(seconds, stall_s):
    return seconds + stall_s + PLAYER_BOUND_MARGIN_S


class FrameHashReader:
    def __init__(self, path):
        self.accounting = FrameHash(path, tolerate_discontinuities=True)

    def progress(self):
        try:
            self.accounting.update()
        except ValueError as error:
            return self.accounting.frames, dict(error=str(error))
        return self.accounting.frames, dict(discontinuities=self.accounting.discontinuities)

    def close(self):
        self.accounting.close()


class FfmpegAdapter(Adapter):
    name = 'ffmpeg'
    unit = 'samples'

    def pipeline(self, url, fmt, out, label, seconds, stall_s, scratch):
        # The soak harness's decode-to-framehash shape, at verbose level so the tcp open lines are in the log.
        return [['ffmpeg', '-nostdin', '-nostats', '-v', 'level+verbose', '-xerror',
                 '-rw_timeout', str(round((DECODER_RW_TIMEOUT_S + stall_s) * 1_000_000)), '-err_detect', 'crccheck',
                 '-f', fmt, '-i', url, '-t', f'{bound_seconds(seconds, stall_s):g}',
                 '-c:a', 'pcm_s32le', '-f', 'framehash', '-flush_packets', '1', str(out / f'{label}.framehash')]]

    def reader(self, fmt, out, label, scratch):
        return FrameHashReader(out / f'{label}.framehash')

    def connections(self, log_text):
        return log_text.count('Starting connection attempt')

    def connection_log(self, label):
        return f'{label}.ffmpeg.stderr'


def mpv_reply(line):
    """The data of one mpv JSON IPC reply, None for an error reply or an unsolicited event."""
    reply = json.loads(line)
    if reply.get('error') != 'success':
        return None
    return reply.get('data')


class MpvIpcReader:
    """Asks mpv over its IPC socket; the socket appears after mpv starts, so connecting is retried on every poll."""
    def __init__(self, path):
        self.path = path
        self.sock = None
        self.file = None
        self.request_id = 0

    def ask(self, prop):
        self.request_id += 1
        self.sock.sendall(json.dumps(dict(command=['get_property', prop], request_id=self.request_id)).encode() + b'\n')
        for _ in range(64):
            line = self.file.readline()
            if not line:
                raise OSError('mpv ipc closed')
            if json.loads(line).get('request_id') == self.request_id:
                return mpv_reply(line)
        raise OSError('mpv ipc flooded with events')

    def progress(self):
        try:
            if self.sock is None:
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.settimeout(1.0)
                try:
                    sock.connect(str(self.path))
                except OSError:
                    # mpv has not created its IPC socket yet; a failed connect must not leak the fd.
                    sock.close()
                    raise
                self.sock, self.file = sock, sock.makefile('r')
            played = self.ask('playback-time')
            detail = dict(stream_pos=self.ask('stream-pos'), eof=self.ask('eof-reached'))
            return played, detail
        except (OSError, ValueError) as error:
            self.close()
            return None, dict(error=repr(error))

    def close(self):
        if self.file is not None:
            self.file.close()
        if self.sock is not None:
            self.sock.close()
        self.sock = self.file = None


class MpvAdapter(Adapter):
    name = 'mpv'
    unit = 'playback_seconds'

    def pipeline(self, url, fmt, out, label, seconds, stall_s, scratch):
        # AF_UNIX paths are short (108 bytes), so the socket lives in the short scratch directory, not under --out.
        return [['mpv', '--no-video', '--ao=null', '--really-quiet', f'--log-file={out / f"{label}.mpv.log"}',
                 f'--input-ipc-server={scratch / f"{label}.sock"}', url]]

    def reader(self, fmt, out, label, scratch):
        return MpvIpcReader(scratch / f'{label}.sock')

    def connections(self, log_text):
        return log_text.count('tcp: Starting connection attempt')

    def connection_log(self, label):
        return f'{label}.mpv.log'


def wav_samples(path):
    # mpg123 -w writes 16-bit stereo: four bytes per sample frame after the 44-byte header it rewrites at the end.
    try:
        return max(0, path.stat().st_size - 44) // 4
    except FileNotFoundError:
        return 0


class FileSizeReader:
    def __init__(self, path, measure):
        self.path, self.measure = path, measure

    def progress(self):
        return self.measure(self.path), {}

    def close(self):
        pass


class Mpg123Adapter(Adapter):
    name = 'mpg123'
    formats = ('mp3',)
    unit = 'samples'

    def pipeline(self, url, fmt, out, label, seconds, stall_s, scratch):
        return [['mpg123', '-q', '--no-control', '-w', str(out / f'{label}.wav'), url]]

    def reader(self, fmt, out, label, scratch):
        return FileSizeReader(out / f'{label}.wav', wav_samples)


SOX_OUT = re.compile(r'Out:(\d+(?:\.\d+)?)([kMG]?)')


def sox_samples(text):
    """Sample frames from the last `Out:` field of sox -S progress (carriage-return separated)."""
    found = SOX_OUT.findall(text)
    if not found:
        return 0
    value, scale = found[-1]
    return round(float(value) * {'': 1, 'k': 1_000, 'M': 1_000_000, 'G': 1_000_000_000}[scale])


class TailTextReader:
    def __init__(self, path, measure, limit=4096):
        self.path, self.measure, self.limit = path, measure, limit

    def progress(self):
        try:
            with self.path.open('rb') as handle:
                handle.seek(max(0, handle.seek(0, os.SEEK_END) - self.limit))
                return self.measure(handle.read().decode('utf-8', 'replace')), {}
        except FileNotFoundError:
            return 0, {}

    def close(self):
        pass


def curl_connections(log_text):
    return sum(1 for line in log_text.splitlines() if line.startswith('> GET '))


def curl_argv(url, seconds, stall_s):
    # -v for the request trace (one `> GET` per connection); -N for no buffering; --max-time bounds the run.
    return ['curl', '-sS', '-v', '-N', '--max-time', f'{bound_seconds(seconds, stall_s):g}', url]


class SoxAdapter(Adapter):
    name = 'sox'
    unit = 'samples'

    def pipeline(self, url, fmt, out, label, seconds, stall_s, scratch):
        # `play` needs an audio device (this build has oss only); sox with the null output is the same decode headless.
        return [curl_argv(url, seconds, stall_s), ['sox', '-S', '-t', fmt, '-', '-n']]

    def reader(self, fmt, out, label, scratch):
        return TailTextReader(out / f'{label}.sox.stderr', sox_samples)

    def connections(self, log_text):
        return curl_connections(log_text)

    def connection_log(self, label):
        return f'{label}.curl.stderr'


class CurlAdapter(Adapter):
    name = 'curl'
    unit = 'bytes'

    def pipeline(self, url, fmt, out, label, seconds, stall_s, scratch):
        return [[*curl_argv(url, seconds, stall_s)[:-1], '-o', str(out / f'{label}.{fmt}'), url]]

    def reader(self, fmt, out, label, scratch):
        return FileSizeReader(out / f'{label}.{fmt}', lambda path: path.stat().st_size if path.exists() else 0)

    def connections(self, log_text):
        return curl_connections(log_text)

    def connection_log(self, label):
        return f'{label}.curl.stderr'


class FirefoxAdapter(Adapter):
    """Browser case, out of scope for the zoo today and deliberately not in PLAYERS.

    The plan's browser `<audio>` acceptance needs: a page under --out with `<audio autoplay src=URL>`, `firefox --headless
    --profile <fresh profile>` on it, progress from the page (`audio.currentTime`) through the remote protocol or a
    `navigator.sendBeacon` to a local listener, and connections from the board's UART only. None of that is wired.
    """
    name = 'firefox'
    unit = 'playback_seconds'

    def pipeline(self, url, fmt, out, label, seconds, stall_s, scratch):
        raise NotImplementedError('firefox is not wired into the client zoo; see FirefoxAdapter')

    def reader(self, fmt, out, label, scratch):
        raise NotImplementedError('firefox is not wired into the client zoo; see FirefoxAdapter')


PLAYERS = {adapter.name: adapter for adapter in (FfmpegAdapter(), MpvAdapter(), Mpg123Adapter(), SoxAdapter(), CurlAdapter())}


def player_names(text):
    if text == 'all':
        return list(PLAYERS)
    names = text.split(',')
    for name in names:
        if name not in PLAYERS:
            raise ValueError(f'unknown player {name!r}; choose from {", ".join(PLAYERS)}')
    if len(set(names)) != len(names):
        raise ValueError('duplicate player name')
    return [name for name in PLAYERS if name in names]


def format_names(text):
    names = text.split(',')
    for name in names:
        if name not in FORMATS:
            raise ValueError(f'unknown format {name!r}; choose from {", ".join(FORMATS)}')
    if len(set(names)) != len(names):
        raise ValueError('duplicate format name')
    return [name for name in FORMATS if name in names]


def pairs(players, formats):
    """(player, format) in PLAYERS order, formats in FORMATS order, only where the player decodes the format."""
    return [(name, fmt) for name in PLAYERS if name in players for fmt in FORMATS if fmt in formats and fmt in PLAYERS[name].formats]


def budget_seconds(pair_count, seconds, stall_s):
    return 180 + pair_count * (seconds + stall_s + PAIR_MARGIN_S)


def stream_url(host, fmt):
    return f'http://{host}:{STREAM_PORT}/stream.{fmt}'


def receiving_at_end(samples, window_s):
    """Whether progress advanced over the final window: the last reading beats the newest reading at least window_s older
    (or the first reading when the run is shorter than the window)."""
    valid = [sample for sample in samples if sample['progress'] is not None]
    if len(valid) < 2:
        return False
    last = valid[-1]
    earlier = [sample for sample in valid if sample['at_s'] <= last['at_s'] - window_s]
    reference = earlier[-1] if earlier else valid[0]
    return last['progress'] > reference['progress']


def pair_verdict(record):
    """Pass iff the player never reconnected and was still receiving at the end; skips are evidence, not a gate."""
    joins = record.get('board_joins')
    board_reconnects = joins - 1 if joins else None
    opened = record.get('player_connections')
    player_reconnects = opened - 1 if opened else None
    known = [count for count in (board_reconnects, player_reconnects) if count is not None]
    reconnects = max(known) if known else None
    receiving = receiving_at_end(record['samples'], RECEIVING_WINDOW_S)
    verdict = dict(reconnects=reconnects, board_reconnects=board_reconnects, player_reconnects=player_reconnects, receiving_at_end=receiving,
                   skips=dict(before=record.get('skips_before'), during=record.get('skips_during'), departure=record.get('departure_skips')))
    if record.get('ended_early'):
        return dict(passed=False, reason=f"player ended before the run did (exit codes {record.get('exit_codes')})", **verdict)
    if reconnects is None:
        return dict(passed=False, reason='no join line on the UART and no connection count in the player log', **verdict)
    if reconnects:
        return dict(passed=False, reason=f'{reconnects} reconnect(s)', **verdict)
    if not receiving:
        return dict(passed=False, reason=f'no progress in the final {RECEIVING_WINDOW_S:g} s', **verdict)
    return dict(passed=True, reason='no reconnect, still receiving at the end', **verdict)


def run_pair(adapter, fmt, url, out, label, seconds, steady_s, stall_s, inject, remove, scratch, elapsed, at_end=None):
    """Run one player for `seconds` of wall time with one stall from steady_s to steady_s + stall_s; sample progress every second.

    inject/remove apply and lift the stall (tc on the device run, the loopback server's pause in tests). at_end runs after
    the loop while the player is still connected. Processes are stopped and the stall lifted in finally, whatever happens.
    """
    pipeline = adapter.pipeline(url, fmt, out, label, seconds, stall_s, scratch)
    record = dict(player=adapter.name, format=fmt, url=url, label=label, unit=adapter.unit, argv=pipeline, seconds=seconds,
                  steady_s=steady_s, stall_s=stall_s, started_s=elapsed(), samples=[], stall=None, exit_codes=[], ended_early=False,
                  ended_s=None, player_connections=None, at_end=None, error=None)
    processes, handles = [], []
    reader = None
    installed = removed = None
    try:
        upstream = None
        for index, argv in enumerate(pipeline):
            log = (out / f'{label}.{argv[0]}.stderr').open('wb')
            handles.append(log)
            last = index == len(pipeline) - 1
            process = subprocess.Popen(argv, stdin=upstream, stdout=subprocess.DEVNULL if last else subprocess.PIPE, stderr=log,
                                       start_new_session=True)
            if upstream is not None:
                upstream.close()
            upstream = process.stdout
            processes.append(process)
        reader = adapter.reader(fmt, out, label, scratch)
        deadline = record['started_s'] + seconds
        while True:
            now = elapsed()
            value, detail = reader.progress()
            record['samples'].append(dict(at_s=now, progress=value, detail=detail))
            if any(process.poll() is not None for process in processes):
                record['ended_early'] = True
                record['ended_s'] = now
                break
            if installed is None and now - record['started_s'] >= steady_s:
                inject()
                installed = elapsed()
            if installed is not None and removed is None and elapsed() - installed >= stall_s:
                remove()
                removed = elapsed()
            if now >= deadline:
                break
            tick = POLL_S
            if installed is not None and removed is None:
                tick = min(tick, max(0.0, installed + stall_s - elapsed()))
            time.sleep(tick)
        if at_end is not None and not record['ended_early']:
            record['at_end'] = at_end()
    finally:
        if installed is not None and removed is None:
            try:
                remove()
                removed = elapsed()
            except Exception as error:
                record['error'] = 'remove: ' + repr(error)
        for process in reversed(processes):
            stop(process)
        if reader is not None:
            reader.close()
        for handle in handles:
            handle.close()
    record['exit_codes'] = [process.poll() for process in processes]
    record['stall'] = dict(installed_s=installed, removed_s=removed) if installed is not None else None
    if record['ended_s'] is None:
        record['ended_s'] = elapsed()
    log_name = adapter.connection_log(label)
    if log_name is not None:
        record['player_connections'] = adapter.connections((out / log_name).read_text(errors='replace') if (out / log_name).exists() else '')
    return record


def dry_run_lines(interface, board, ifb, chosen, formats, seconds, stall_s, out):
    lines = ['# install'] + [' '.join(privileged(argv)) for argv in qdisc_plan(interface, board, ifb)]
    install, remove = injection_commands(stall_profile(stall_s), interface, ifb)
    for name, fmt in pairs(chosen, formats):
        label = f'{name}-{fmt}'
        pipeline = PLAYERS[name].pipeline(stream_url(board, fmt), fmt, out, label, seconds, stall_s, Path('<scratch>'))
        lines.append(f'# player {name} {fmt}: ' + ' | '.join(' '.join(argv) for argv in pipeline))
        lines.append(f'# stall {stall_s:g} s')
        lines += [' '.join(privileged(argv)) for argv in install + remove]
    lines.append('# teardown')
    lines += [' '.join(privileged(argv)) for argv in qdisc_teardown(interface, board, ifb)]
    return lines


class Driver:
    def __init__(self, args, repo):
        self.args, self.repo = args, repo
        self.out = args.out
        self.started = time.monotonic()
        self.plan = pairs(args.players, args.formats)
        self.deadline = self.started + budget_seconds(len(self.plan), args.seconds, args.stall)
        self.metrics_url = f'http://{args.host}/api/metrics'
        self.events = None
        self.processes, self.handles = [], []
        self.uart = None
        self.uart_lines = []
        self.boots = 0
        self.installed_steps = 0
        self.cleanup_errors = []
        self.scratch = None
        self.result = dict(passed=False, error=None, host=args.host, interface=args.interface, ifb=args.ifb, seconds=args.seconds,
                           stall_s=args.stall, steady_s=args.steady, started_utc=datetime.now(timezone.utc).isoformat(),
                           pairs=[f'{name}-{fmt}' for name, fmt in self.plan], runs={}, board_reset_by_serial_open=True,
                           restore_verified=None, cleanup_errors=self.cleanup_errors,
                           budget_seconds=budget_seconds(len(self.plan), args.seconds, args.stall),
                           note='Players run one at a time (the board has two listener slots). Skips are /api/metrics totals and the '
                                'departure line counters; reconnects come from UART join lines and each player\'s own log.')

    def elapsed(self):
        return time.monotonic() - self.started

    def record(self, kind, **fields):
        value = dict(kind=kind, elapsed_seconds=round(self.elapsed(), 3), **fields)
        self.events.write(json.dumps(value) + '\n')
        self.events.flush()
        print(json.dumps(value), flush=True)

    def write_result(self):
        self.result['elapsed_seconds'] = round(self.elapsed(), 3)
        (self.out / 'result.pending.json').write_text(json.dumps(self.result, indent=2) + '\n')

    def check_deadline(self):
        if time.monotonic() >= self.deadline:
            raise Failure('overall deadline exceeded')

    def sh(self, argv, timeout=15):
        started = time.monotonic()
        completed = subprocess.run(privileged(argv), capture_output=True, text=True, timeout=timeout)
        self.record('command', argv=privileged(argv), returncode=completed.returncode,
                    seconds=round(time.monotonic() - started, 3), stderr=completed.stderr[-512:])
        if completed.returncode != 0:
            raise Failure(f"{' '.join(argv)} failed ({completed.returncode}): {completed.stderr.strip()[-200:]}")
        return completed.stdout

    def unprivileged(self, argv):
        return subprocess.run(argv, capture_output=True, text=True, timeout=15, check=True).stdout

    def environment(self):
        route = self.unprivileged(['ip', 'route', 'get', self.args.host])
        versions = {name: self.unprivileged([name, '--version']).splitlines()[0] for name in ('mpv', 'mpg123', 'sox', 'curl')}
        versions['ffmpeg'] = self.unprivileged(['ffmpeg', '-version']).splitlines()[0]
        return dict(route=route, route_interface=route_interface(route), tc_version=self.unprivileged(['tc', '-V']).strip(),
                    kernel=self.unprivileged(['uname', '-r']).strip(), players=versions,
                    qdisc_before=self.unprivileged(['tc', 'qdisc', 'show', 'dev', self.args.interface]))

    def read_uart(self):
        for line in self.uart.lines():
            self.uart_lines.append(line)
            if 'rst:0x' in line:
                self.boots += 1
                if self.boots > 1:
                    raise Failure('board reset during the run; see private uart.log')
            if 'Guru Meditation' in line or 'panicked at' in line or 'Task watchdog got triggered' in line:
                raise Failure('board reported panic/watchdog; see private uart.log')

    def snapshot(self):
        return metrics_sample(fetch(self.metrics_url, 3), self.elapsed())

    def wait_ready(self):
        deadline = time.monotonic() + 120
        previous = None
        while True:
            self.read_uart()
            if self.serial.poll() is not None:
                raise Failure('UART collector exited during startup; see serial.log')
            if time.monotonic() >= deadline:
                raise Failure('board not ready with steady capture within 120 s of the serial reset')
            try:
                sample = self.snapshot()
            except Exception:
                sample = None  # Wi-Fi may still be connecting; the deadline bounds this.
            if sample is not None and sample['network'] is not None and sample['network']['connected'] and sample['capture'] is not None:
                if previous is not None and sample['capture'] == previous['capture'] and sample['read_frames'] > previous['read_frames']:
                    self.result.update(boot_token=sample['boot_token'], build_elf_sha256=sample['build_elf_sha256'])
                    self.record('ready', boot_token=sample['boot_token'], build_elf_sha256=sample['build_elf_sha256'])
                    return
                previous = sample
            time.sleep(1)

    def apply(self, phase, commands):
        started = self.elapsed()
        for argv in commands:
            self.sh(argv)
        self.record('injection', phase=phase, at_s=started, apply_seconds=round(self.elapsed() - started, 3))

    def run_zoo_pair(self, name, fmt):
        self.check_deadline()
        self.read_uart()
        mark = len(self.uart_lines)
        label = f'{name}-{fmt}'
        install, remove = injection_commands(stall_profile(self.args.stall), self.args.interface, self.args.ifb)
        before = self.snapshot()
        self.record('run_started', label=label)
        record = run_pair(PLAYERS[name], fmt, stream_url(self.args.host, fmt), self.out, label, self.args.seconds, self.args.steady,
                          self.args.stall, inject=lambda: self.apply('installed', install), remove=lambda: self.apply('removed', remove),
                          scratch=self.scratch, elapsed=self.elapsed, at_end=self.snapshot)
        # The departure line with the listener's counters follows the stop; give the UART a moment for it.
        settle = time.monotonic() + SETTLE_S
        while time.monotonic() < settle:
            self.read_uart()
            time.sleep(0.2)
        window = self.uart_lines[mark:]
        departures = [dict(peer=found.group(1), end=found.group(2), **skip_fields(line)) for line in window for found in [LEFT.search(line)] if found]
        during = record.pop('at_end')
        record.update(board_joins=sum(1 for line in window if JOINED.search(line)), departures=departures,
                      skips_before=before['skips'], skips_during=during['skips'] if during else None,
                      departure_skips=departures[-1]['skips'] if departures else None, before=before, during=during)
        record.update(pair_verdict(record))
        self.record('run', label=label, passed=record['passed'], reason=record['reason'], reconnects=record['reconnects'], skips=record['skips'])
        return record

    def run(self):
        os.umask(0o077)
        self.out.mkdir(parents=True, exist_ok=False)
        self.events = (self.out / 'events.jsonl').open('w')
        self.write_result()
        failure = None
        try:
            # Probe with a command the operator's sudo rule covers; a tc-only NOPASSWD rule refuses `sudo -n true`.
            self.sh(['tc', '-V'])
            self.result['environment'] = self.environment()
            if self.result['environment']['route_interface'] != self.args.interface:
                raise Failure(f"route to {self.args.host} uses {self.result['environment']['route_interface']}, not {self.args.interface}")
            try:
                socket.if_nametoindex(self.args.ifb)
                ifb_exists = True
            except OSError:
                ifb_exists = False
            problem = preinstall_problem(self.result['environment']['qdisc_before'], ifb_exists)
            if problem:
                raise Failure('refusing to install: ' + problem)
            self.write_result()
            self.scratch = Path(tempfile.mkdtemp(prefix='client-zoo-'))
            raw = (self.out / 'uart.log').open('wb')
            serial_errors = (self.out / 'serial.log').open('wb')
            self.handles += [raw, serial_errors]
            self.serial = subprocess.Popen([str(self.repo / 'scripts/serial-log'), str(int(self.deadline - time.monotonic()) + 60)],
                                           stdout=raw, stderr=serial_errors, start_new_session=True,
                                           env=dict(os.environ, ESPFLASH_PORT=self.args.serial))
            self.processes.append(self.serial)
            self.uart = Tail(self.out / 'uart.log')
            self.wait_ready()
            for argv in qdisc_plan(self.args.interface, self.args.host, self.args.ifb):
                self.sh(argv)
                self.installed_steps += 1
            self.result['qdisc_installed'] = self.unprivileged(['tc', 'qdisc', 'show', 'dev', self.args.interface])
            for name, fmt in self.plan:
                self.result['runs'][f'{name}-{fmt}'] = self.run_zoo_pair(name, fmt)
                self.write_result()
        except KeyboardInterrupt:
            failure = 'interrupted'
        except BaseException as error:
            failure = f'{type(error).__name__}: {error}'
        finally:
            previous = {number: signal.signal(number, signal.SIG_IGN) for number in (signal.SIGINT, signal.SIGTERM)}
            if failure is not None:
                self.result['error'] = failure
                self.record('failure', error=failure)
            end = time.monotonic() + 60
            try:
                if self.installed_steps:
                    for argv in qdisc_teardown(self.args.interface, self.args.host, self.args.ifb):
                        cleanup_attempt(self.cleanup_errors, ' '.join(argv), end, lambda argv=argv: self.sh(argv))
                after = self.unprivileged(['tc', 'qdisc', 'show', 'dev', self.args.interface])
                self.result['qdisc_after'] = after
                before = self.result.get('environment', {}).get('qdisc_before')
                if before is None:
                    self.cleanup_errors.append('restore-check: no pre-run qdisc listing')
                else:
                    self.result['restore_verified'] = restore_matches(before, after)
            except (CleanupDeadline, InterruptedError) as error:
                self.cleanup_errors.append('cleanup-interrupted: ' + repr(error))
            except Exception as error:
                self.cleanup_errors.append('restore-check: ' + repr(error))
            for process in reversed(self.processes):
                try:
                    stop(process)
                except Exception as error:
                    self.cleanup_errors.append('process-stop: ' + repr(error))
            if self.uart is not None:
                self.uart.close()
            for handle in self.handles:
                handle.close()
            if self.scratch is not None:
                shutil.rmtree(self.scratch, ignore_errors=True)
            self.result['passed'] = (failure is None and not self.cleanup_errors and self.result['restore_verified'] is True
                                     and all(run['passed'] for run in self.result['runs'].values())
                                     and len(self.result['runs']) == len(self.plan))
            self.write_result()
            os.replace(self.out / 'result.pending.json', self.out / 'result.json')
            self.events.close()
            print(f"{'PASS' if self.result['passed'] else 'FAIL'} {self.out / 'result.json'}", flush=True)
            for number, handler in previous.items():
                signal.signal(number, handler)
        return 0 if self.result['passed'] else 1


def argument_parser():
    parser = argparse.ArgumentParser(
        prog='scripts/client-zoo',
        description='Play /stream.mp3 and /stream.flac through real players (ffmpeg, mpv, mpg123, sox, curl), one at a time, each '
                    'across one receiver-side stall shaped with tc netem on the receiver interface. Records bytes or samples '
                    'received, reconnects (UART join lines and the player log) and the board\'s skip counters; a player passes '
                    'with zero reconnects while still receiving at the end. Never runs as root: cache credentials with `sudo -v` '
                    'first; every tc/ip call uses `sudo -n`. The serial open resets the board once. UART logs are private.')
    parser.add_argument('host', nargs='?', type=board_address, help='board IPv4 address')
    parser.add_argument('--interface', type=interface_name, help='receiver interface that carries board traffic')
    parser.add_argument('--ifb', default='ifb0', type=interface_name, help='ifb device for ingress shaping (default: ifb0)')
    parser.add_argument('--serial', help='UART device for scripts/serial-log')
    parser.add_argument('--out', type=Path, help='new output directory; must not exist')
    parser.add_argument('--seconds', type=positive_seconds, default=DEFAULT_SECONDS, help=f'wall seconds per player and format (default: {DEFAULT_SECONDS:g})')
    parser.add_argument('--stall', type=positive_seconds, default=DEFAULT_STALL_S, help=f'receiver-side stall seconds (default: {DEFAULT_STALL_S:g})')
    parser.add_argument('--steady', type=positive_seconds, default=DEFAULT_STEADY_S, help=f'seconds of play before the stall (default: {DEFAULT_STEADY_S:g})')
    parser.add_argument('--players', type=player_names, default=list(PLAYERS), help=f'comma-separated players (default: all of {", ".join(PLAYERS)})')
    parser.add_argument('--formats', type=format_names, default=list(FORMATS), help='comma-separated formats (default: mp3,flac)')
    parser.add_argument('--dry-run', action='store_true', help='print every privileged argv and player pipeline; create nothing')
    return parser


def main(argv=None, euid=None):
    parser = argument_parser()
    args = parser.parse_args(argv)
    repo = Path(__file__).resolve().parent.parent
    missing = [flag for flag, value in (('host', args.host), ('--interface', args.interface), ('--serial', args.serial), ('--out', args.out)) if value is None]
    if missing:
        parser.error('a run needs ' + ', '.join(missing))
    if args.steady + args.stall >= args.seconds:
        parser.error('--seconds must exceed --steady plus --stall')
    if args.out.exists():
        parser.error(f'{args.out} exists; choose a new output directory')
    if not pairs(args.players, args.formats):
        parser.error('no player decodes the chosen formats')
    require_unprivileged(os.geteuid() if euid is None else euid)
    if args.dry_run:
        for line in dry_run_lines(args.interface, args.host, args.ifb, args.players, args.formats, args.seconds, args.stall, args.out):
            print(line)
        return 0

    def interrupted(_number, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    return Driver(args, repo).run()


if __name__ == '__main__':
    sys.exit(main())
