# ABOUTME: Implements the public stream-soak hardware harness.
# ABOUTME: Exposes host-testable helpers while keeping runtime work behind main.
import argparse
import json
import math
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace

from stream_format import input_format_args, validate_stream_decoder_log
from decoder_integrity import FrameHash, validate_decoder_line
from device_metrics import fetch
from failure_observation import observation_seconds, observe_failure
from encoder_fault_check import validate_encoder_export


class Failure(Exception):
    pass


def fault_tools():
    """network_fault imports this module's decoder helpers, so its qdisc helpers load on demand to avoid the cycle."""
    import network_fault
    return network_fault


# The ifb device the stall tree mirrors the board's packets through; network_fault's default.
IFB = 'ifb0'


# A framehash decoder writes about 4.7 KB/s (280,239 bytes per 60 s measured on oct16-interruption-1), so six
# hours of two listeners is about 202 MB; 384 MiB leaves headroom for the polling overshoot.
OUTPUT_BUDGET_BYTES = 384 * 1024 * 1024
# ffmpeg's socket read timeout and the no-progress watchdog; a harness may stretch both on its decoder args.
DECODER_RW_TIMEOUT_S = 10
# A slow or dropped /api/state answer is evidence, not a failure, until the board has been silent this long: the same
# bound as the UART telemetry rule. oct18-soak-1 died at 74 of 360 minutes on a single poll past the 3 s timeout while
# both streams and the UART stayed healthy.
STATE_UNANSWERED_LIMIT_S = 25
DECODER_PROGRESS_TIMEOUT_S = 20
LOSS_KEYS = ['queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts', 'discontinuities']
CLEAN_STARTUP_KEYS = ['software_dropped_frames', 'read_errors', 'restarts',
                      'supervisor_restarts', 'overflow_restarts', 'discontinuities',
                      'health_restart_requests']


def parse_capture_sample(line, require_clean_startup=False):
    """Validate the UART sample before it can establish a soak baseline."""
    if require_clean_startup:
        for key in CLEAN_STARTUP_KEYS:
            values = re.findall(r'\b' + key + r'=(\S+)', line)
            if len(values) != 1 or re.fullmatch(r'[0-9]+', values[0]) is None:
                raise Failure(f'clean startup requires one valid capture counter: {key}')
            if int(values[0]) != 0:
                raise Failure(f'clean startup capture counter is nonzero: {key}={values[0]}')
    sample = {key: int(value) for key, value in re.findall(r'\b([a-z_]+)=(-?\d+)', line)}
    required = ['frames', *LOSS_KEYS, 'internal_min_sum', 'psram_min_sum']
    if any(key not in sample for key in required):
        raise Failure('capture telemetry lacks required counters/window minima')
    return sample


def seconds(value):
    result = float(value)
    if not math.isfinite(result) or result < 1:
        raise argparse.ArgumentTypeError('must be finite and at least 1 second')
    return result


class Tail:
    """Consume disk logs incrementally, retaining at most one unfinished line."""
    def __init__(self, path):
        self.path = path
        self.handle = None
        self.pending = b''

    def lines(self):
        if self.handle is None:
            if not self.path.exists():
                return
            self.handle = self.path.open('rb')
        # Limit work per poll as well as memory; excessive UART output is a failure below.
        for _ in range(16):
            chunk = self.handle.read(8192)
            if not chunk:
                return
            pieces = (self.pending + chunk).split(b'\n')
            self.pending = pieces.pop()
            if len(self.pending) > 16384:
                raise Failure(f'oversized log line in {self.path.name}')
            for line in pieces:
                yield line.decode('utf-8', 'replace').strip()
        raise Failure(f'log reader cannot keep up with {self.path.name}')

    def close(self):
        if self.handle is not None:
            self.handle.close()


def port(value):
    if not 1 <= len(value) <= 5 or not value.isascii() or not value.isdecimal() or not 1 <= int(value) <= 65535:
        raise argparse.ArgumentTypeError('port must be an integer from 1 to 65535')
    return int(value)


def interface(value):
    try:
        return fault_tools().interface_name(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError(str(error)) from None


def stall_options_problem(args):
    """Why the stall flags are inconsistent, or None; the flags come as a set or not at all."""
    on = args.stall_every is not None or args.stall_seconds is not None
    if (args.stall_every is None) != (args.stall_seconds is None):
        return '--stall-every and --stall-seconds are required together'
    if on and args.interface is None:
        return '--interface is required with --stall-every/--stall-seconds'
    if not on and args.interface is not None:
        return '--interface applies only with --stall-every/--stall-seconds'
    if on and args.stall_seconds >= args.stall_every:
        return '--stall-seconds must be shorter than --stall-every'
    if on and args.path != '/stream.mp3':
        return 'stalls are judged against MP3 skips; use --path /stream.mp3'
    return None


def stall_schedule(run_seconds, every, stall_seconds):
    """Wall offsets from decoder start at which a stall begins; a stall that would outlast the run is left out."""
    starts = [float(every) * k for k in range(1, int(run_seconds // every) + 1)]
    return [start for start in starts if start + stall_seconds <= run_seconds]


def decoder_arguments(args):
    """The decoder's args: unchanged without stalls, else with both timeouts stretched by the stall like network_fault's trials."""
    if args.stall_seconds is None:
        return args
    tools = fault_tools()
    spec = dict(kind='stall', profile=tools.stall_profile(args.stall_seconds))
    return SimpleNamespace(**vars(args), **tools.decoder_timeouts(spec), frames_short_allowed=0)


def continuity_problem(decoder, expected_frames, skips, frames_skipped):
    """Why a decoder is not continuous modulo the board's skips, or None: network_fault's live rule.

    A board skip shows to an MP3 decoder as late audio, not always as a timestamp gap, so discontinuities may fall
    short of the skips but never exceed them; ffmpeg -t ends at the jumped timeline, so the decode may stop the
    skipped audio short, within one MP3 frame.
    """
    floor = expected_frames - (frames_skipped + 1) * fault_tools().MP3_FRAME_SAMPLES
    if decoder['decoded_discontinuities'] > skips:
        return f"{decoder['decoded_discontinuities']} discontinuities exceed {skips} board skips"
    if not floor <= decoder['decoded_audio_frames'] <= expected_frames:
        return f"{decoder['decoded_audio_frames']} frames outside {floor}..{expected_frames} (expected minus {frames_skipped} skipped)"
    return None


def restore_tree(run, show, interface, board_ip, before, ifb=IFB):
    """Tear the stall tree down with every step attempted, then compare the listing with the pre-install one.

    Returns (restore_verified, errors); `run` executes one privileged argv, `show` returns the interface's qdisc listing.
    """
    tools = fault_tools()
    errors = []
    for argv in tools.qdisc_teardown(interface, board_ip, ifb):
        try:
            run(argv)
        except Exception as error:
            errors.append(f"{' '.join(argv)}: {error}")
    try:
        verified = tools.restore_matches(before, show())
    except Exception as error:
        errors.append('restore-check: ' + repr(error))
        verified = None
    return verified, errors


def update_decoder(job, args):
    job['accounting'].update(final=job['process'].poll() is not None)
    before = job['decoded_audio_frames']
    job.update(job['accounting'].summary())
    if job['decoded_audio_frames'] > before:
        job['advanced_at'] = time.monotonic()
    for line in job['tail'].lines():
        if line.startswith('out_time_us='):
            value = line.split('=', 1)[1]
            if value.isdigit():
                duration = int(value) / 1_000_000
                job['timeline_seconds'] = duration
    for line in job['log_tail'].lines():
        validate_decoder_line(line)
    code = job['process'].poll()
    if code is not None:
        # Exit can race the earlier read: drain FFmpeg's final committed PCM records.
        job['accounting'].update(final=True)
        job.update(job['accounting'].summary())
        with job['errors'].open('r') as log:
            validate_stream_decoder_log(log.read(16384), args.path)
        # The process may flush its last progress update immediately before exit.
        for line in job['tail'].lines():
            if line.startswith('out_time_us=') and line.split('=', 1)[1].isdigit():
                job['timeline_seconds'] = int(line.split('=', 1)[1]) / 1_000_000
        # A stalled soak lets the decode stop short by the board's skipped frames; otherwise the count is exact.
        expected = round(args.seconds * 48000)
        if code != 0 or not expected - getattr(args, 'frames_short_allowed', 0) <= job['decoded_audio_frames'] <= expected:
            raise Failure(f"{job['label']}: exit={code}, decoded={job['decoded_audio_frames']} actual frames, "
                          f"timeline={job['timeline_seconds']:.3f}s, requested={args.seconds:g}s; see {job['errors']}")
    elif time.monotonic() - job['advanced_at'] > getattr(args, 'progress_timeout_s', DECODER_PROGRESS_TIMEOUT_S):
        raise Failure(f"{job['label']}: no decoder progress for {getattr(args, 'progress_timeout_s', DECODER_PROGRESS_TIMEOUT_S):g} seconds")


def stop(process):
    if process.poll() is None:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=3)


def decoder_command(args, ip, progress, hashes):
    return ['ffmpeg', '-nostdin', '-nostats', '-v', 'level+info', '-xerror', '-stats_period', '1',
                 '-progress', str(progress), '-rw_timeout', str(round(getattr(args, 'rw_timeout_s', DECODER_RW_TIMEOUT_S) * 1_000_000)),
                 '-err_detect', 'crccheck',
                 *input_format_args(args.path), '-i',
                 f'http://{ip}:{args.stream_port}{args.path}', '-t', str(round(args.seconds * 48000) / 48000),
                 '-c:a', 'pcm_s32le', '-f', 'framehash', '-flush_packets', '1', str(hashes)]


def decoders_completed(jobs, args):
    if not all(job['process'].poll() is not None for job in jobs):
        return False
    # A decoder can exit after its preceding update. Validate every final drain.
    for job in jobs:
        update_decoder(job, args)
    return True


class StatePoll:
    """One /api/state request per call. Any OSError (timeout, refusal, reset) is recorded as `state_unanswered` and
    tolerated until STATE_UNANSWERED_LIMIT_S has passed since the last answer; `required=True` (the pre-reset gate)
    fails on the first miss; `starting=True` (post-reset startup, bounded by its own deadline) never fails on silence.
    An answer with recognition enabled always fails. Returns True when the board answered."""

    def __init__(self, url, record, timeout=3, clock=time.monotonic, limit_seconds=STATE_UNANSWERED_LIMIT_S):
        self.url = url
        self.record = record
        self.timeout = timeout
        self.clock = clock
        self.limit_seconds = limit_seconds
        self.answered_at = clock()
        self.polls = 0
        self.unanswered = 0

    def __call__(self, required=False, starting=False):
        began = self.clock()
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        try:
            with opener.open(self.url, timeout=self.timeout) as response:
                value = json.load(response)
        except OSError as error:  # urllib.error.URLError is an OSError too
            now = self.clock()
            unanswered = round(now - self.answered_at, 3)
            self.unanswered += 1
            self.record('state_unanswered', error=str(error), seconds=round(now - began, 3), unanswered_seconds=unanswered)
            if required:
                raise Failure(f'/api/state did not answer: {error}') from error
            if unanswered > self.limit_seconds and not starting:
                raise Failure(f'/api/state unanswered for {unanswered} s (limit {self.limit_seconds} s): {error}') from error
            return False
        if value.get('shazam_enabled') is not False:
            raise Failure('/api/state must report shazam_enabled=false')
        self.polls += 1
        self.answered_at = self.clock()
        self.record('state', shazam_enabled=False, level_dbfs=value.get('level_dbfs'), seconds=round(self.answered_at - began, 3))
        return True


def state_url(ip, args):
    return f'http://{ip}:{args.state_port}/api/state'


def stream_address(ip, args):
    return ip, args.stream_port


def listener_indices(args):
    return range(1, args.listeners + 1)


def resolve_output(args):
    return Path(args.out or Path(__file__).resolve().parent.parent / 'target/stream-soak').resolve()


def output_bytes(out):
    return sum(path.stat().st_size for path in out.iterdir() if path.is_file())


def check_budget(out):
    # Collectors write directly to disk. This checked budget can overshoot between polls
    # and during cleanup; it is a stop condition, not a filesystem-enforced quota.
    total = output_bytes(out)
    if total > OUTPUT_BUDGET_BYTES:
        raise Failure(f'output budget exceeded: {total} bytes > {OUTPUT_BUDGET_BYTES}; collectors stopped, logs retained')


def argument_parser():
    parser = argparse.ArgumentParser(
        prog='scripts/stream-soak',
        description='Reset the board once, require Shazam off, then decode one or two real FLAC or MP3 streams '
        'to null for six hours by default. Check UART capture counters against a stable startup '
        'baseline and poll /api/state. UART logs are private. Run under caffeinate -is to prevent '
        'host sleep; keep continuous LINE IN audio playing. No flashing occurs. Decoder success '
        'and counters do not detect every omitted DMA interrupt. Runtime-stat wrap is not a capture fault. '
        'A checked 384 MiB total output budget stops the run on overflow; logs are retained, not rotated. '
        'With --stall-every/--stall-seconds/--interface the receiver side blackholes the board (network_fault\'s tc '
        'tree, sudo -n) on that schedule after the baseline and judges decoders continuous modulo the board\'s skips.')
    parser.add_argument('host', nargs='?', default='esp-shazam.local')
    parser.add_argument('--seconds', type=seconds, default=21600.0)
    parser.add_argument('--listeners', type=int, choices=[1, 2], default=2, help='simultaneous real decoders (default: 2)')
    parser.add_argument('--path', choices=['/stream.flac', '/stream.mp3'], default='/stream.flac',
                        help='compressed endpoint (default: /stream.flac)')
    parser.add_argument('--stream-port', type=port, default=8000, help='stream TCP port (default: 8000)')
    parser.add_argument('--state-port', type=port, default=80, help='state HTTP port (default: 80)')
    parser.add_argument('--out', help='parent output directory (default: target/stream-soak)')
    parser.add_argument('--require-clean-startup', action='store_true',
                        help='require every capture sample, including before baseline/decoder startup, to have zero loss/recovery counters; expected queue_overflows and settling discards are allowed before baseline (default: steady-state gate only)')
    parser.add_argument('--failure-observe-seconds', type=observation_seconds, default=0.0,
                        help='retain existing raw UART and decoders after failure for 0..120 seconds (default: 0); counters remain pre-failure snapshots')
    parser.add_argument('--stall-every', type=seconds, help='blackhole the board every N seconds of decoding (default: no stalls); needs --stall-seconds and --interface')
    parser.add_argument('--stall-seconds', type=seconds, help='length of each stall, shorter than --stall-every; stretches the decoder timeouts by as much')
    parser.add_argument('--interface', type=interface, help='workstation interface that routes to the board, shaped with sudo -n tc')
    return parser


def parse_args(argv=None):
    parser = argument_parser()
    args = parser.parse_args(argv)
    problem = stall_options_problem(args)
    if problem:
        parser.error(problem)
    return args


def main(argv=None):
    args = parse_args(argv)

    def interrupted(_number, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)
    repo = Path(__file__).resolve().parent.parent
    if shutil.which('ffmpeg') is None:
        argument_parser().error('ffmpeg is required')
    os.umask(0o077)
    out = resolve_output(args) / datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    out.mkdir(parents=True)
    started = time.monotonic()
    events = (out / 'events.jsonl').open('w')
    processes, handles, tails, jobs = [], [], [], []
    failure = None
    failure_at = None
    failure_observation = None
    serial = None
    baseline = None
    latest = None
    latest_at = started
    captured = 0
    samples = 0
    boots = 0
    runtime_invalid_windows = 0
    minimums = {}
    state = None
    ip = None
    loss_keys = LOSS_KEYS
    capture_gate = 'clean_startup_and_steady_state' if args.require_clean_startup else 'steady_state_only'
    decoder_args = decoder_arguments(args)
    stalls_on = args.stall_seconds is not None
    schedule = stall_schedule(args.seconds, args.stall_every, args.stall_seconds) if stalls_on else []
    # A live listener keeps its socket through a stall, so wall time may grow by every stall (network_fault's budget rule).
    stall_wall_seconds = len(schedule) * (args.stall_seconds or 0)
    qdisc_before = qdisc_after = None
    installed_steps = 0
    stall_installed = None
    stalls_applied = []
    cleanup_errors = []
    restore_verified = None
    # Board skip counters, from the last metrics sample with every soak listener active; absent counters leave it exact.
    skip_totals = None
    departure_seen = None
    departures = 0
    metrics_polls = 0

    def check_output_budget():
        return check_budget(out)

    def record(kind, **fields):
        value = dict(kind=kind, elapsed_seconds=round(time.monotonic() - started, 3), **fields)
        events.write(json.dumps(value) + '\n')
        events.flush()
        print(json.dumps(value), flush=True)

    def sh(argv, timeout=15):
        # Same privileged runner and command record as network_fault's driver.
        tools = fault_tools()
        began = time.monotonic()
        completed = subprocess.run(tools.privileged(argv), capture_output=True, text=True, timeout=timeout)
        record('command', argv=tools.privileged(argv), returncode=completed.returncode,
               seconds=round(time.monotonic() - began, 3), stderr=completed.stderr[-512:])
        if completed.returncode != 0:
            raise Failure(f"{' '.join(argv)} failed ({completed.returncode}): {completed.stderr.strip()[-200:]}")
        return completed.stdout

    def unprivileged(argv):
        return subprocess.run(argv, capture_output=True, text=True, timeout=15, check=True).stdout

    def qdisc_show():
        return unprivileged(['tc', 'qdisc', 'show', 'dev', args.interface])

    def apply(phase, commands):
        began = time.monotonic() - started
        for argv in commands:
            sh(argv)
        fields = dict(phase=phase, at_s=round(began, 3), apply_seconds=round(time.monotonic() - started - began, 3))
        record('injection', **fields)
        return dict(kind='injection', **fields)

    def metrics():
        """Read the board's skip counters and last departure; a departure while every decoder still runs fails the soak."""
        nonlocal skip_totals, departure_seen, departures, metrics_polls
        value = fetch(f'http://{ip}:{args.state_port}/api/metrics', 3)
        sample = fault_tools().metrics_sample(value, round(time.monotonic() - started, 3))
        active = sum(1 for item in value['listeners'] if item['state'] in ('writing', 'ready'))
        departure = value.get('last_listener_departure')
        observed = departure['observed_device_us'] if departure else None
        running = all(job['process'].poll() is None for job in jobs)
        metrics_polls += 1
        record('metrics', skips=sample['skips'], frames_skipped=sample['frames_skipped'], media_age_us=sample['media_age_us'],
               listeners_active=active, departure_device_us=observed)
        if metrics_polls > 1 and observed != departure_seen and running:
            # The harness's own closes after a decoder exits are not departures of the run.
            departures += 1
            raise Failure('listener departed during the soak (last_listener_departure moved); see private uart.log')
        departure_seen = observed
        if active == args.listeners and sample['skips'] is not None and sample['frames_skipped'] is not None:
            skip_totals = dict(skips=sample['skips'], frames_skipped=sample['frames_skipped'], source='metrics')
            decoder_args.frames_short_allowed = (sample['frames_skipped'] + 1) * fault_tools().MP3_FRAME_SAMPLES

    def uart_updates():
        nonlocal latest, latest_at, samples, boots, captured, runtime_invalid_windows
        check_output_budget()
        for line in uart_tail.lines():
            try:
                validate_encoder_export([line])
            except RuntimeError as error:
                raise Failure(str(error)) from error
            if 'rst:0x' in line:
                boots += 1
                if boots > 1:
                    raise Failure('board reset during soak; see private uart.log')
            if 'Guru Meditation' in line or 'panicked at' in line or 'Task watchdog got triggered' in line:
                raise Failure('board reported panic/watchdog; see private uart.log')
            if 'stream-encode:' in line and 'runtime_valid=false' in line:
                runtime_invalid_windows += 1
            if 'audio: frames=' not in line:
                continue
            sample = parse_capture_sample(line, require_clean_startup=args.require_clean_startup)
            if baseline is not None:
                for key in loss_keys:
                    if sample[key] != baseline[key]:
                        raise Failure(f'capture counter changed: {key} {baseline[key]} -> {sample[key]}')
                delta = (sample['frames'] - latest['frames']) & 0xffffffff
                if delta == 0 or delta >= 0x80000000:
                    raise Failure('capture frames stopped or moved backward')
                captured += delta
                for key in ['internal_min_sum', 'psram_min_sum']:
                    minimums[key] = min(minimums.get(key, sample[key]), sample[key])
            latest, latest_at = sample, time.monotonic()
            samples += 1
            record('capture', counters=sample)

    def decoder_updates(job):
        return update_decoder(job, decoder_args)


    try:
        ip = socket.getaddrinfo(args.host, args.stream_port, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
        state = StatePoll(state_url(ip, args), record)
        # Refuse a reset of firmware running with recognition enabled, or of a board that does not answer.
        state(required=True)
        if stalls_on:
            # Fail here, before the serial reset, on a lapsed sudo timestamp or a tree that is not the default one.
            # Probe with a command the operator's sudo rule covers: a NOPASSWD entry for tc alone refuses `sudo -n true`.
            tools = fault_tools()
            sh(['tc', '-V'])
            route = unprivileged(['ip', 'route', 'get', ip])
            if tools.route_interface(route) != args.interface:
                raise Failure(f'route to {ip} uses {tools.route_interface(route)}, not {args.interface}')
            qdisc_before = qdisc_show()
            try:
                socket.if_nametoindex(IFB)
                ifb_exists = True
            except OSError:
                ifb_exists = False
            problem = tools.preinstall_problem(qdisc_before, ifb_exists)
            if problem:
                raise Failure('refusing to install: ' + problem)
        raw = (out / 'uart.log').open('wb')
        serial_errors = (out / 'serial.log').open('wb')
        handles += [raw, serial_errors]
        serial = subprocess.Popen([str(repo / 'scripts/serial-log'), str(args.seconds + 180 + args.failure_observe_seconds + stall_wall_seconds)],
                                  stdout=raw, stderr=serial_errors, start_new_session=True)
        processes.append(serial)
        uart_tail = Tail(out / 'uart.log')
        tails.append(uart_tail)
        record('start', host=args.host, path=args.path, stream_port=args.stream_port, state_port=args.state_port, ip=ip, requested_seconds=args.seconds, listeners=args.listeners, capture_gate=capture_gate)
        if stalls_on:
            record('stall_plan', interface=args.interface, ifb=IFB, route=route.strip(), every_seconds=args.stall_every, stall_seconds=args.stall_seconds,
                   scheduled=len(schedule), rw_timeout_s=decoder_args.rw_timeout_s, progress_timeout_s=decoder_args.progress_timeout_s)
        startup_deadline = time.monotonic() + 60
        state_seen = False
        next_state = 0
        previous_sample = None
        while baseline is None:
            if serial.poll() is not None:
                raise Failure('UART collector exited during startup; see serial.log')
            uart_updates()
            if time.monotonic() >= next_state:
                # Startup may still be connecting Wi-Fi; the 60 s deadline bounds the silence, not the poll's limit.
                state_seen = state(starting=True) or state_seen
                next_state = time.monotonic() + 3
            if state_seen and boots == 1 and latest is not None:
                if previous_sample is not None and latest is not previous_sample:
                    if latest['frames'] > previous_sample['frames'] and all(latest[key] == previous_sample[key] for key in loss_keys):
                        baseline = dict(latest)
                        record('baseline', counters=baseline)
                previous_sample = latest
            if time.monotonic() >= startup_deadline:
                raise Failure('no stable post-startup capture/UART/state baseline within 60 seconds')
            time.sleep(0.2)
        if stalls_on:
            # The tree shapes nothing until a stall is applied; install it only on a clean baseline.
            for argv in fault_tools().qdisc_plan(args.interface, ip, IFB):
                sh(argv)
                installed_steps += 1
            record('qdisc_installed', listing=qdisc_show())
            stall_install, stall_remove = fault_tools().injection_commands(fault_tools().stall_profile(args.stall_seconds), args.interface, IFB)
        for index in listener_indices(args):
            label = f'listener-{index}'
            errors_path = out / f'{label}.ffmpeg.log'
            error_handle = errors_path.open('wb')
            handles.append(error_handle)
            progress = out / f'{label}.progress'
            hashes = out / f'{label}.framehash'
            process = subprocess.Popen(
                decoder_command(decoder_args, ip, progress, hashes),
                stdout=subprocess.DEVNULL, stderr=error_handle, start_new_session=True)
            processes.append(process)
            tail = Tail(progress)
            tails.append(tail)
            log_tail = Tail(errors_path)
            tails.append(log_tail)
            # Under stalls a flagged timestamp gap is evidence to count against the board's skips, not a reason to stop.
            jobs.append(dict(label=label, process=process, tail=tail, errors=errors_path,
                             log_tail=log_tail,
                             accounting=FrameHash(hashes, tolerate_discontinuities=stalls_on), decoded_audio_frames=0,
                             decoded_seconds=0, timeline_seconds=0, advanced_at=time.monotonic()))
        decode_started = time.monotonic()
        deadline = decode_started + args.seconds + 60 + stall_wall_seconds
        next_state = 0
        next_progress = 0
        sleep_for = 0.5
        while True:
            uart_updates()
            if serial.poll() is not None:
                raise Failure('UART collector exited before soak completed; see serial.log')
            if time.monotonic() - latest_at > 25:
                raise Failure('capture UART telemetry missing for 25 seconds')
            for job in jobs:
                decoder_updates(job)
            if stalls_on:
                since_decode = time.monotonic() - decode_started
                if stall_installed is None and len(stalls_applied) < len(schedule) and since_decode >= schedule[len(stalls_applied)]:
                    stall_installed = apply('installed', stall_install)
                if stall_installed is not None:
                    due = stall_installed['at_s'] + stall_installed['apply_seconds'] + args.stall_seconds
                    if time.monotonic() - started >= due:
                        removed = apply('removed', stall_remove)
                        stalls_applied.append(dict(scheduled_s=schedule[len(stalls_applied)], installed=stall_installed, removed=removed,
                                                   actual_seconds=round(removed['at_s'] + removed['apply_seconds'] - stall_installed['at_s'], 3)))
                        stall_installed = None
                # Hold a stall for its requested seconds, not the poll interval rounded up.
                sleep_for = max(0.0, min(0.5, due - (time.monotonic() - started))) if stall_installed is not None else 0.5
            # HTTP to the board cannot answer through a blackhole; the polls resume once the stall is removed.
            if time.monotonic() >= next_state and stall_installed is None:
                state()
                if stalls_on:
                    metrics()
                next_state = time.monotonic() + 5
            if time.monotonic() >= next_progress:
                record('progress', listeners=[dict(label=job['label'], timeline_seconds=job['timeline_seconds'],
                                                  **job['accounting'].summary()) for job in jobs],
                       capture_samples=samples, minimums=minimums,
                       runtime_invalid_windows=runtime_invalid_windows)
                next_progress = time.monotonic() + 60
            if decoders_completed(jobs, decoder_args):
                break
            if time.monotonic() >= deadline:
                raise Failure('decoders exceeded requested duration plus 60 seconds of wall time')
            time.sleep(sleep_for)
        finished_at = time.monotonic()
        # Require a UART sample after all decoders finish, covering their final capture window.
        final_deadline = finished_at + 20
        while latest_at <= finished_at:
            uart_updates()
            if serial.poll() is not None or time.monotonic() >= final_deadline:
                raise Failure('missing final capture telemetry after decoder completion')
            if time.monotonic() >= next_state:
                state()
                next_state = time.monotonic() + 5
            time.sleep(0.2)
        if stalls_on:
            # Zero departures were enforced at every poll; decoders must be continuous modulo the board's skips.
            if len(stalls_applied) != len(schedule):
                raise Failure(f'{len(stalls_applied)} of {len(schedule)} scheduled stalls applied')
            bound = skip_totals or dict(skips=0, frames_skipped=0, source='absent')
            for job in jobs:
                problem = continuity_problem(job['accounting'].summary(), round(args.seconds * 48000), bound['skips'], bound['frames_skipped'])
                if problem:
                    raise Failure(f"{job['label']}: {problem}")
        record('assertions_passed', listeners=[job['accounting'].summary() for job in jobs])
        check_output_budget()
    except KeyboardInterrupt:
        failure_at = time.monotonic()
        failure = 'interrupted'
        record('failure', error=failure)
    except Exception as error:
        failure_at = time.monotonic()
        failure = str(error)
        record('failure', error=failure)
        # Preserve raw evidence without re-running assertions against the failed workload.
        # Decoder exit is expected here; only UART loss, budget or interrupt ends grace early.
        if args.failure_observe_seconds:
            observation_started = time.monotonic()
            failure_observation = observe_failure(args.failure_observe_seconds, serial, check_output_budget, record)
            failure_observation.update(started_elapsed_seconds=observation_started - started,
                                       ended_elapsed_seconds=time.monotonic() - started)
    finally:
        if installed_steps:
            # A second interrupt must not skip the remaining tc calls; the tree comes down on every exit path.
            previous = {number: signal.signal(number, signal.SIG_IGN) for number in (signal.SIGINT, signal.SIGTERM)}
            try:
                restore_verified, cleanup_errors = restore_tree(sh, qdisc_show, args.interface, ip, qdisc_before, IFB)
                qdisc_after = qdisc_show()
            except Exception as error:
                cleanup_errors.append('teardown: ' + repr(error))
            finally:
                for number, handler in previous.items():
                    signal.signal(number, handler)
            record('restore', verified=restore_verified, errors=cleanup_errors)
            if restore_verified is not True or cleanup_errors:
                failure = failure or f'qdisc restore not verified: {cleanup_errors}'
        for process in reversed(processes):
            try:
                stop(process)
            except Exception as error:
                failure = failure or f'subprocess cleanup failed: {error}'
        for tail in tails:
            tail.close()
        for job in jobs:
            job['accounting'].close()
        for handle in handles:
            handle.close()
        try:
            check_output_budget()
        except Exception as error:
            failure = failure or str(error)
        summary = dict(passed=failure is None, error=failure, host=args.host, path=args.path, stream_port=args.stream_port, state_port=args.state_port, ip=ip,
                       requested_seconds=args.seconds, listeners=args.listeners, elapsed_seconds=round(time.monotonic() - started, 3),
                       decoded_seconds=[job['decoded_seconds'] for job in jobs], state_polls=state.polls if state else 0, state_unanswered=state.unanswered if state else 0,
                       decoder_accounting=[dict(timeline_seconds=job['timeline_seconds'], **job['accounting'].summary()) for job in jobs],
                       capture_gate=capture_gate, require_clean_startup=args.require_clean_startup,
                       baseline=baseline, last_capture=latest, captured_frames_observed=captured,
                       last_capture_phase='pre_failure' if failure_at is not None else 'completion',
                       last_capture_age_seconds_at_failure=max(0, failure_at - latest_at) if failure_at is not None and latest is not None else None,
                       last_capture_age_seconds_at_summary=max(0, time.monotonic() - latest_at) if latest is not None else None,
                       failure_observation=failure_observation,
                       stalls=dict(interface=args.interface, ifb=IFB, every_seconds=args.stall_every, seconds=args.stall_seconds,
                                   scheduled=len(schedule), count=len(stalls_applied), times=[stall['installed']['at_s'] for stall in stalls_applied],
                                   actual_seconds=[stall['actual_seconds'] for stall in stalls_applied], applied=stalls_applied,
                                   incomplete=stall_installed, qdisc_before=qdisc_before, qdisc_after=qdisc_after,
                                   cleanup_errors=cleanup_errors) if stalls_on else None,
                       restore_verified=restore_verified, skips=skip_totals if stalls_on else None,
                       departures_while_running=departures if stalls_on else None, metrics_polls=metrics_polls if stalls_on else None,
                       capture_samples=samples, minimums=minimums, runtime_invalid_windows=runtime_invalid_windows,
                       output_bytes_before_summary=output_bytes(out), output_budget_bytes=OUTPUT_BUDGET_BYTES,
                       counter_deltas={key: ((latest[key] - baseline[key]) & 0xffffffff) for key in loss_keys} if baseline and latest else None,
                       log_directory=str(out), uart_log=str(out / 'uart.log'),
                       note='Failure observation retains raw logs only; last_capture, counter_deltas and decoder accounting remain pre-failure snapshots. '
                       'Zero pre-failure deltas do not prove continuity through failure or observation. '
                       'Minimum sums are conservative per-region lows; largest blocks are snapshots. '
                       'Runtime-stat wrap is excluded from capture faults. Counters cannot detect every omitted DMA interrupt.')
        (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
        events.close()
        print(f"{'PASS' if failure is None else 'FAIL'} {out / 'summary.json'}", flush=True)
    return 0 if failure is None else 1


if __name__ == '__main__':
    sys.exit(main())
