# ABOUTME: Injects receiver-side packet loss and rate limits with tc netem while real decoders listen to the board.
# ABOUTME: qdisc planning, qdisc-show parsing and trial verdicts are pure; the device driver runs only from main.
import argparse
import ipaddress
import json
import math
import os
import re
import signal
import socket
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace

from decoder_integrity import FrameHash
from device_metrics import fetch
from encoder_fault_check import checked_listener_closes, listener_end_category, listener_event_us
from sonos_http_check import CleanupDeadline, cleanup_attempt
from stream_soak import LOSS_KEYS, Tail, decoder_command, stop, update_decoder


class Failure(Exception):
    pass


# MP3 is 320 kbit/s fixed: 960 bytes per 1152-sample frame at 48 kHz.
MP3_BITRATE_BIT_PER_S = 320_000
# Plain /stream.mp3 is Route::Stream with no preset (core/src/stream_http.rs:637, firmware/src/stream.rs:427),
# so MediaAge::for_stream(None, Mp3) bounds the age at four seconds (core/src/media_age.rs).
MEDIA_AGE_BASE_S = 4.0
ADMISSION_WINDOW_S = 10.0
# The board logs the harness's own listener close as a ConnectionReset departure; it follows the close closely.
HARNESS_CLOSE_WINDOW_S = 2.0
POLL_INTERVAL_S = 1.0
STEADY_SECONDS = 20.0
NULL_SECONDS = 60.0
DEFICIT_SECONDS = 60.0
DELAYED_READER_SECONDS = 40.0
FRESH_LISTENER_SECONDS = 10.0
STREAM_PORT = 8000
TRIALS = ['null-1', 'null-2', 'stall-0.5-1', 'stall-0.5-2', 'stall-1-1', 'stall-1-2', 'stall-2.5-1', 'stall-2.5-2',
          'stall-4-1', 'stall-4-2', 'deficit-0.8-1', 'deficit-0.5-1', 'delayed-reader-1']
# Band 4 of a four-band prio: the default priomap only names bands 0-2, so nothing but the u32 filter lands there.
EGRESS_NETEM_PARENT = '1:4'
IFB_NETEM_PARENT = 'root'
DEFAULT_ROOT_KINDS = ('mq', 'fq_codel', 'pfifo_fast')
# Every trial field trial_verdict reads; trial_setup must supply each one for every kind.
VERDICT_TRIAL_KEYS = ('kind', 'listeners', 'age_bound_us', 'largest_tolerance_bytes', 'largest_before',
                      'predicted_end_s', 'end_tolerance_s')


def privileged(argv):
    # Non-interactive sudo: a lapsed timestamp fails instead of hanging the trial.
    return ['sudo', '-n', *argv]


def board_address(value):
    address = ipaddress.ip_address(value)
    if address.version != 4:
        raise ValueError('board must be a numeric IPv4 address')
    return str(address)


def interface_name(value):
    if not re.fullmatch(r'[A-Za-z0-9_.][A-Za-z0-9_.-]{0,14}', value):
        raise ValueError('invalid interface name')
    return value


def qdisc_plan(interface, board_ip, ifb='ifb0'):
    """Ordered install argv: egress prio+netem+u32 for packets to the board, then ifb ingress netem.

    prio's default priomap sends Linux priorities 6-7 to band 0, so the netem sits in a fourth band
    that only the filter selects. Acceptance of `bands 4` without an explicit priomap is verified at
    the device run.
    """
    interface, board, ifb = interface_name(interface), board_address(board_ip), interface_name(ifb)
    return [
        ['tc', 'qdisc', 'replace', 'dev', interface, 'root', 'handle', '1:', 'prio', 'bands', '4'],
        ['tc', 'qdisc', 'add', 'dev', interface, 'parent', EGRESS_NETEM_PARENT, 'handle', '10:', 'netem'],
        ['tc', 'filter', 'add', 'dev', interface, 'parent', '1:', 'protocol', 'ip', 'prio', '1', 'u32',
         'match', 'ip', 'dst', board + '/32', 'flowid', EGRESS_NETEM_PARENT],
        ['ip', 'link', 'add', ifb, 'type', 'ifb'],
        ['ip', 'link', 'set', ifb, 'up'],
        ['tc', 'qdisc', 'add', 'dev', interface, 'handle', 'ffff:', 'ingress'],
        ['tc', 'filter', 'add', 'dev', interface, 'parent', 'ffff:', 'protocol', 'ip', 'prio', '1', 'u32',
         'match', 'ip', 'src', board + '/32', 'action', 'mirred', 'egress', 'redirect', 'dev', ifb],
        ['tc', 'qdisc', 'add', 'dev', ifb, 'root', 'handle', '1:', 'netem'],
    ]


def qdisc_teardown(interface, board_ip, ifb='ifb0'):
    """Deleting the egress root lets the kernel re-attach its default qdisc tree."""
    interface, ifb = interface_name(interface), interface_name(ifb)
    board_address(board_ip)
    return [
        ['tc', 'qdisc', 'del', 'dev', ifb, 'root'],
        ['tc', 'qdisc', 'del', 'dev', interface, 'ingress'],
        ['tc', 'qdisc', 'del', 'dev', interface, 'root'],
        ['ip', 'link', 'del', ifb],
    ]


def netem_change(dev, parent, *, loss_percent=None, rate_bit_per_s=None):
    """`tc qdisc change` on the netem under `parent` ('root' or MAJ:MIN).

    Removal says `rate 0bit` because `change` is expected to keep an unspecified rate
    (verify at the device run).
    """
    if parent == 'root':
        location = ['root']
    elif re.fullmatch(r'[0-9a-fA-F]{1,4}:[0-9a-fA-F]{0,4}', parent):
        location = ['parent', parent]
    else:
        raise ValueError('parent must be root or MAJ:MIN')
    options = []
    if loss_percent is not None:
        if not 0 <= loss_percent <= 100:
            raise ValueError('loss must be 0..100 percent')
        options += ['loss', f'{loss_percent:g}%']
    if rate_bit_per_s is not None:
        if rate_bit_per_s < 0 or int(rate_bit_per_s) != rate_bit_per_s:
            raise ValueError('rate must be a non-negative whole number of bits per second')
        options += ['rate', f'{int(rate_bit_per_s)}bit']
    if not options:
        raise ValueError('netem change needs loss or rate')
    return ['tc', 'qdisc', 'change', 'dev', interface_name(dev), *location, 'netem', *options]


def parse_qdisc_show(text):
    """Keep qdisc lines with their parameters; `-s` counter lines are indented and dropped."""
    return [' '.join(line.split()) for line in text.splitlines() if line.startswith('qdisc ')]


def restore_matches(before, after):
    # Listing order follows kernel hash iteration, which a re-attached default tree need not repeat.
    return sorted(parse_qdisc_show(before)) == sorted(parse_qdisc_show(after))


def preinstall_problem(qdisc_before, ifb_exists):
    """Refuse to shape anything but a default tree; teardown would otherwise delete someone else's state."""
    if ifb_exists:
        return 'ifb device already exists; remove it or pass another --ifb'
    lines = parse_qdisc_show(qdisc_before)
    kinds = [line.split()[1] for line in lines]
    for kind in ('ingress', 'clsact'):
        if kind in kinds:
            return f'interface already has a {kind} qdisc'
    roots = [line for line in lines if ' root' in line]
    if len(roots) != 1:
        return 'interface listing has no single root qdisc'
    root_kind = roots[0].split()[1]
    if root_kind not in DEFAULT_ROOT_KINDS:
        return f'root qdisc is {root_kind}, not a default tree ({", ".join(DEFAULT_ROOT_KINDS)})'
    return None


def stall_profile(seconds):
    if not isinstance(seconds, (int, float)) or not math.isfinite(seconds) or seconds <= 0:
        raise ValueError('stall seconds must be finite and positive')
    return dict(kind='stall', seconds=seconds, egress=dict(loss_percent=100), ingress=dict(loss_percent=100))


def deficit_profile(fraction, bitrate_bit_per_s=MP3_BITRATE_BIT_PER_S):
    if not isinstance(fraction, (int, float)) or not 0 < fraction < 1:
        raise ValueError('deficit fraction must lie strictly between 0 and 1')
    return dict(kind='deficit', fraction=fraction, egress=None, ingress=dict(rate_bit_per_s=round(fraction * bitrate_bit_per_s)))


def predicted_media_age_end_s(preset_seconds, fraction):
    """Age grows at (1 - fraction) s/s until the bound; at or above full rate it never ends."""
    if fraction >= 1:
        return None
    return (MEDIA_AGE_BASE_S + preset_seconds) / (1 - fraction)


def end_tolerance_s(age_bound_us, fraction, apply_seconds):
    # The run starts from an unknown age up to the null-trial bound, which shortens it by
    # bound/(1-f); applying the rate and one poll interval delay the observation.
    return age_bound_us / 1e6 / (1 - fraction) + apply_seconds + POLL_INTERVAL_S


def injection_commands(profile, interface, ifb):
    """(install, remove) argv lists from the profile's ingress (ifb, media) and egress (ACK) fields."""
    install, remove = [], []
    if profile is None:
        return install, remove
    for side, dev, parent in (('ingress', ifb, IFB_NETEM_PARENT), ('egress', interface, EGRESS_NETEM_PARENT)):
        fields = profile[side]
        if fields:
            install.append(netem_change(dev, parent, **fields))
            remove.append(netem_change(dev, parent, **{key: 0 for key in fields}))
    return install, remove


def trial_spec(name):
    if name not in TRIALS:
        raise ValueError(f'unknown trial {name!r}; choose from {", ".join(TRIALS)}')
    parts = name.split('-')
    listeners = int(parts[-1])
    if parts[0] == 'null':
        return dict(name=name, kind='null', listeners=listeners, seconds=NULL_SECONDS, profile=None)
    if parts[0] == 'stall':
        seconds = float(parts[1])
        return dict(name=name, kind='stall', listeners=listeners, seconds=STEADY_SECONDS + seconds + STEADY_SECONDS,
                    profile=stall_profile(seconds))
    if parts[0] == 'deficit':
        return dict(name=name, kind='deficit', listeners=listeners, seconds=DEFICIT_SECONDS, profile=deficit_profile(float(parts[1])))
    return dict(name=name, kind='delayed-reader', listeners=listeners, seconds=DELAYED_READER_SECONDS, profile=None)


def trial_names(text):
    if text == 'all':
        return list(TRIALS)
    names = text.split(',')
    for name in names:
        trial_spec(name)
    if len(set(names)) != len(names):
        raise ValueError('duplicate trial name')
    return [name for name in TRIALS if name in names]


def budget_seconds(names):
    # Startup after the serial reset, then each trial with its fresh listener, receipt wait and margin.
    return 180 + sum(trial_spec(name)['seconds'] + FRESH_LISTENER_SECONDS + 30 for name in names)


def trial_setup(spec, before, tolerances, installed):
    """Everything trial_verdict reads, from the spec, the pre-trial snapshot, null-* tolerances and the install record."""
    trial = dict(spec, expected_frames=round(spec['seconds'] * 48000),
                 largest_before=dict(internal_largest=before['internal_largest'], psram_largest=before['psram_largest']),
                 age_bound_us=tolerances['age_bound_us'] if tolerances else None,
                 largest_tolerance_bytes=tolerances['largest_tolerance_bytes'] if tolerances else None,
                 media_age_bound_s=MEDIA_AGE_BASE_S, predicted_end_s=None, end_tolerance_s=None)
    if spec['kind'] == 'deficit':
        fraction = spec['profile']['fraction']
        trial['predicted_end_s'] = predicted_media_age_end_s(0, fraction)
        if installed is not None and trial['age_bound_us'] is not None:
            trial['end_tolerance_s'] = end_tolerance_s(trial['age_bound_us'], fraction, installed['apply_seconds'])
    return trial


JOINED = re.compile(r'stream: (unknown peer|\S+) joined \S+ \(\d+ of \d+\) event_us=(\d+)')
LEFT = re.compile(r'stream: (unknown peer|\S+) left after [0-9.]+ s: (.*?) listener_stack_free_bytes=\d+ event_us=(\d+)')


def uart_events(lines, at_s):
    """Admissions, departures and checked closes from production UART lines, in line order."""
    events = []
    for line, at in zip(lines, at_s):
        joined = JOINED.search(line)
        if joined:
            events.append(dict(kind='admission', peer=joined.group(1), device_us=int(joined.group(2)), at_s=at))
        left = LEFT.search(line)
        if left:
            events.append(dict(kind='departure', peer=left.group(1), end=left.group(2), device_us=int(left.group(3)), at_s=at))
    for index, close in checked_listener_closes(lines):
        events.append(dict(kind='close', peer=close.group(1), device_us=listener_event_us(lines[index]), at_s=at_s[index]))
    return events


def metrics_sample(value, at_s):
    # Ready carries the age recorded after every successful write (firmware/src/listener_observation.rs:185).
    ages = [item['media_age_us'] for item in value['listeners'] if item['state'] in ('writing', 'ready')]
    capture = value.get('capture')
    return dict(kind='metrics', at_s=at_s, served_device_us=value['served_device_us'],
                media_age_us=max(ages) if ages else None,
                internal_largest=capture['internal_largest'] if capture else None,
                psram_largest=capture['psram_largest'] if capture else None,
                capture={key: capture[key] for key in LOSS_KEYS} if capture else None,
                read_frames=capture['read_frames'] if capture else None,
                settings=value.get('settings'), network=value.get('network'),
                boot_token=value.get('boot_token'), build_elf_sha256=value.get('build_elf_sha256'))


def settings_problem(settings):
    if settings is None:
        return 'settings snapshot unavailable'
    if settings['autoplay'] is not False:
        return 'autoplay is enabled; refusing to inject faults'
    if settings['active_format'] != 'mp3':
        return f"active format is {settings['active_format']}, not mp3"
    return None


def null_tolerances(samples, steady_after_s):
    """Bounds a null trial measured: steady-state media age and the whole-trial largest-free swing."""
    steady = [sample['media_age_us'] for sample in samples if sample['at_s'] >= steady_after_s and sample['media_age_us'] is not None]
    if not steady:
        return None
    swing = {}
    for key in ('internal_largest', 'psram_largest'):
        values = [sample[key] for sample in samples if sample[key] is not None]
        if not values:
            return None
        swing[key] = max(values) - min(values)
    return dict(age_bound_us=max(steady), largest_tolerance_bytes=swing)


def merge_tolerances(current, measured):
    if current is None or measured is None:
        return current or measured
    return dict(age_bound_us=max(current['age_bound_us'], measured['age_bound_us']),
                largest_tolerance_bytes={key: max(current['largest_tolerance_bytes'][key], measured['largest_tolerance_bytes'][key])
                                         for key in current['largest_tolerance_bytes']})


def trial_verdict(trial, events):
    by_kind = {}
    for event in events:
        by_kind.setdefault(event['kind'], []).append(event)
    decoders = by_kind.get('decoder', [])
    closes = by_kind.get('close', [])
    admissions = by_kind.get('admission', [])
    samples = by_kind.get('metrics', [])
    injections = {event['phase']: event for event in by_kind.get('injection', [])}
    # Sockets the harness closed itself: completed or stopped decoders, fresh listeners, the WAV reader.
    harness_ends = [event['ended_s'] for event in decoders + by_kind.get('fresh', [])
                    if event['ended'] in ('completed', 'stopped') and event['ended_s'] is not None]
    harness_ends += [event['closed_s'] for event in by_kind.get('wav', []) if event['closed_s'] is not None]

    def harness_owned(departure):
        return (listener_end_category(departure['end']) == 'socket_error'
                and any(0 <= departure['at_s'] - at <= HARNESS_CLOSE_WINDOW_S for at in harness_ends))

    departures = [event for event in by_kind.get('departure', []) if not harness_owned(event)]
    harness_closes = len(by_kind.get('departure', [])) - len(departures)

    def fail(reason, **detail):
        return dict(passed=False, reason=reason, **detail)

    def continuous(decoder):
        return decoder['frames'] == decoder['expected_frames'] and decoder['discontinuities'] == 0 and decoder['exit_code'] == 0

    def close_of(departure):
        return next((close for close in closes if close['peer'] == departure['peer'] and close['device_us'] >= departure['device_us']), None)

    def admitted_within(close):
        # Measured on device time: close receipt to join line, not harness reaction.
        return any(close['device_us'] < item['device_us'] <= close['device_us'] + ADMISSION_WINDOW_S * 1e6 for item in admissions)

    def counters_flat():
        counted = [sample['capture'] for sample in samples if sample['capture'] is not None]
        return len(counted) >= 2 and counted[0] == counted[-1]

    def checked_departure(departure, category):
        if listener_end_category(departure['end']) != category:
            return f"departure {departure['peer']} ended {departure['end']!r}, not {category}"
        if close_of(departure) is None:
            return f"departure {departure['peer']} has no checked close"
        return None

    def judge():
        if len(decoders) != trial['listeners']:
            return fail(f"expected {trial['listeners']} decoder records, found {len(decoders)}")
        cut = [decoder for decoder in decoders if not continuous(decoder)]
        kind = trial['kind']
        if kind == 'null':
            if cut:
                return fail('decoder not continuous', decoders=cut)
            if departures:
                return fail('unexpected departure', departures=departures)
            if not counters_flat():
                return fail('capture loss counters changed or missing')
            return dict(passed=True, reason='exact frames, no departures, counters flat')
        if kind == 'stall':
            if trial['age_bound_us'] is None:
                return fail('no media age bound; run null-* first')
            if len(cut) != len(departures):
                return fail(f'{len(cut)} listeners stopped decoding but {len(departures)} departed', departures=departures)
            if len(cut) < len(decoders):
                last = samples[-1] if samples else None
                if last is None or last['media_age_us'] is None or last['media_age_us'] > trial['age_bound_us']:
                    return fail('media age did not return under the null bound', last_sample=last)
            for departure in departures:
                problem = checked_departure(departure, 'media_age')
                if problem:
                    return fail(problem)
                if not admitted_within(close_of(departure)):
                    return fail(f"no fresh admission within {ADMISSION_WINDOW_S:g} s of {departure['peer']} close")
            return dict(passed=True, reason='continuous' if not cut else f'{len(cut)} MediaAge departure(s) with checked close and fresh admission',
                        departed=len(cut))
        if kind == 'deficit':
            if trial['largest_tolerance_bytes'] is None or trial['age_bound_us'] is None:
                return fail('no largest-free tolerance; run null-* first')
            if trial['end_tolerance_s'] is None or trial['predicted_end_s'] is None:
                return fail('no end tolerance; injection install not recorded')
            if len(departures) != 1 or len(cut) != 1:
                return fail(f'expected one MediaAge departure, found {len(departures)} departures and {len(cut)} cut decoders')
            departure = departures[0]
            problem = checked_departure(departure, 'media_age')
            if problem:
                return fail(problem)
            if 'installed' not in injections or 'removed' not in injections:
                return fail('injection install/removal not recorded')
            observed = departure['at_s'] - injections['installed']['at_s']
            if abs(observed - trial['predicted_end_s']) > trial['end_tolerance_s']:
                return fail(f"departure after {observed:.2f} s, predicted {trial['predicted_end_s']:.2f} s ± {trial['end_tolerance_s']:.2f}",
                            observed_end_s=observed)
            if not admitted_within(close_of(departure)):
                return fail(f'no fresh admission within {ADMISSION_WINDOW_S:g} s of the close', observed_end_s=observed)
            last = next((sample for sample in reversed(samples) if sample['internal_largest'] is not None), None)
            if last is None:
                return fail('no post-trial largest-free sample', observed_end_s=observed)
            for key, tolerance in trial['largest_tolerance_bytes'].items():
                if abs(last[key] - trial['largest_before'][key]) > tolerance:
                    return fail(f"{key} {last[key]} is outside {trial['largest_before'][key]} ± {tolerance}", observed_end_s=observed)
            return dict(passed=True, reason='MediaAge departure near prediction, fresh admission, largest-free returned', observed_end_s=observed)
        # delayed-reader: the starved WAV reader ends on media age (stream.rs:729 checks age before mapping the write error).
        wav_end = listener_end_category(departures[0]['end']) if len(departures) == 1 else None
        if cut:
            return fail('MP3 decoder not continuous', decoders=cut, wav_end_category=wav_end)
        if len(departures) != 1:
            return fail(f'expected one WAV reader departure, found {len(departures)}', departures=departures, wav_end_category=wav_end)
        problem = checked_departure(departures[0], 'media_age')
        if problem:
            return fail(problem, wav_end_category=wav_end)
        if not counters_flat():
            return fail('capture loss counters changed or missing', wav_end_category=wav_end)
        return dict(passed=True, reason='MP3 continuous; WAV reader ended on media age with checked close', wav_end_category=wav_end)

    return dict(judge(), harness_closes=harness_closes)


def route_interface(text):
    found = re.search(r'\bdev (\S+)', text)
    if not found:
        raise ValueError('ip route get output names no device')
    return found.group(1)


def bind_to_device(sock, interface):
    # SO_BINDTODEVICE takes a NUL-terminated name at SOL_SOCKET; fresh sockets bind unprivileged.
    device = os.fsencode(interface) + b'\0'
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_BINDTODEVICE, device)
    if sock.getsockopt(socket.SOL_SOCKET, socket.SO_BINDTODEVICE, 256) != device:
        raise OSError('SO_BINDTODEVICE verification failed')


def require_unprivileged(euid):
    if euid == 0:
        sys.stderr.write('run as your normal user with a cached `sudo -v`; this harness invokes sudo -n itself\n')
        raise SystemExit(2)


def dry_run_lines(interface, board, ifb, names):
    lines = ['# install'] + [' '.join(privileged(argv)) for argv in qdisc_plan(interface, board, ifb)]
    for name in names:
        install, remove = injection_commands(trial_spec(name)['profile'], interface, ifb)
        lines.append(f'# trial {name}' + ('' if install else ' (no injection)'))
        lines += [' '.join(privileged(argv)) for argv in install + remove]
    lines.append('# teardown')
    lines += [' '.join(privileged(argv)) for argv in qdisc_teardown(interface, board, ifb)]
    return lines


class Poller(threading.Thread):
    """Fetches /api/metrics once per second; failures during a stall are recorded, not fatal."""
    def __init__(self, url, clock, record):
        super().__init__(daemon=True)
        self.url, self.clock, self.record = url, clock, record
        self.stopping = threading.Event()
        self.lock = threading.Lock()
        self.samples = []
        self.unavailable = 0

    def run(self):
        while not self.stopping.is_set():
            started = time.monotonic()
            try:
                sample = metrics_sample(fetch(self.url, POLL_INTERVAL_S), self.clock())
                with self.lock:
                    self.samples.append(sample)
                self.record('metrics', **{key: sample[key] for key in ('at_s', 'media_age_us', 'internal_largest', 'psram_largest', 'capture')})
            except Exception as error:
                with self.lock:
                    self.unavailable += 1
                self.record('metrics_unavailable', error=repr(error))
            self.stopping.wait(max(0.0, POLL_INTERVAL_S - (time.monotonic() - started)))

    def since(self, at_s):
        with self.lock:
            return [sample for sample in self.samples if sample['at_s'] >= at_s]


class Driver:
    def __init__(self, args, repo):
        self.args, self.repo = args, repo
        self.out = args.out
        self.started = time.monotonic()
        self.deadline = self.started + budget_seconds(args.trials)
        self.metrics_url = f'http://{args.host}/api/metrics'
        self.events = None
        self.events_lock = threading.Lock()
        self.processes, self.handles = [], []
        self.uart = None
        self.uart_lines, self.uart_at = [], []
        self.boots = 0
        self.poller = None
        self.installed_steps = 0
        self.cleanup_errors = []
        self.tolerances = None
        self.tolerance_sources = []
        self.result = dict(passed=False, error=None, host=args.host, interface=args.interface, ifb=args.ifb,
                           started_utc=datetime.now(timezone.utc).isoformat(), trials={}, trial_order=list(args.trials),
                           board_reset_by_serial_open=True, restore_verified=None, cleanup_errors=self.cleanup_errors,
                           budget_seconds=budget_seconds(args.trials), tolerances=None,
                           note='Ages and largest-free are /api/metrics snapshots; departure reasons come from the private UART log. '
                                'ffmpeg sockets follow the recorded route; only in-process sockets are bound to the interface.')

    def elapsed(self):
        return time.monotonic() - self.started

    def record(self, kind, **fields):
        # The poller thread records too; one lock keeps each line whole.
        value = dict(kind=kind, elapsed_seconds=round(self.elapsed(), 3), **fields)
        with self.events_lock:
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
        return dict(route=route, route_interface=route_interface(route), tc_version=self.unprivileged(['tc', '-V']).strip(),
                    kernel=self.unprivileged(['uname', '-r']).strip(),
                    qdisc_before=self.unprivileged(['tc', 'qdisc', 'show', 'dev', self.args.interface]))

    def read_uart(self):
        for line in self.uart.lines():
            self.uart_lines.append(line)
            self.uart_at.append(self.elapsed())
            if 'rst:0x' in line:
                self.boots += 1
                if self.boots > 1:
                    raise Failure('board reset during the run; see private uart.log')
            if 'Guru Meditation' in line or 'panicked at' in line or 'Task watchdog got triggered' in line:
                raise Failure('board reported panic/watchdog; see private uart.log')

    def snapshot(self):
        # Settings are an optional nonblocking snapshot; null is unavailable evidence, so ask again.
        last = None
        for _ in range(3):
            last = metrics_sample(fetch(self.metrics_url, 3), self.elapsed())
            if last['settings'] is not None:
                break
        return last

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

    def start_decoder(self, label, seconds):
        decoder_args = SimpleNamespace(path='/stream.mp3', stream_port=STREAM_PORT, seconds=seconds)
        errors_path = self.out / f'{label}.ffmpeg.log'
        error_handle = errors_path.open('wb')
        self.handles.append(error_handle)
        progress, hashes = self.out / f'{label}.progress', self.out / f'{label}.framehash'
        process = subprocess.Popen(decoder_command(decoder_args, self.args.host, progress, hashes),
                                   stdout=subprocess.DEVNULL, stderr=error_handle, start_new_session=True)
        self.processes.append(process)
        job = dict(label=label, process=process, args=decoder_args, tail=Tail(progress), errors=errors_path, log_tail=Tail(errors_path),
                   accounting=FrameHash(hashes), decoded_audio_frames=0, decoded_seconds=0, timeline_seconds=0,
                   advanced_at=time.monotonic(), started_s=self.elapsed(), ended=None)
        self.record('decoder_started', label=label, seconds=seconds)
        return job

    def poll_decoder(self, job):
        if job['ended'] is not None:
            return
        try:
            update_decoder(job, job['args'])
        except Exception as error:
            # An early exit, a decoded discontinuity or stalled progress ends this listener's record.
            job['ended'] = str(error)
            job['ended_s'] = self.elapsed()
            self.record('decoder_ended', label=job['label'], error=job['ended'])
            stop(job['process'])
            return
        if job['process'].poll() is not None:
            job['ended'] = 'completed'
            job['ended_s'] = self.elapsed()
            self.record('decoder_ended', label=job['label'], error=None)

    def decoder_event(self, job):
        summary = job['accounting'].summary()
        return dict(kind='decoder', label=job['label'], frames=summary['decoded_audio_frames'],
                    expected_frames=round(job['args'].seconds * 48000), discontinuities=summary['decoded_discontinuities'],
                    exit_code=job['process'].poll(), ended=job['ended'], ended_s=job.get('ended_s'))

    def fresh_event(self, job):
        return dict(kind='fresh', label=job['label'], ended=job['ended'], ended_s=job.get('ended_s'), trigger=job['trigger'])

    def stop_job(self, job):
        if job['ended'] is None:
            job['ended'] = 'stopped'
            job['ended_s'] = self.elapsed()
        stop(job['process'])
        job['tail'].close()
        job['log_tail'].close()
        job['accounting'].close()

    def trickle_reader(self, report, stopping):
        """Reads one WAV byte per half second with a small receive window, like stream-churn's retained peer."""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            bind_to_device(sock, self.args.interface)
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4096)
            sock.settimeout(10)
            sock.connect((self.args.host, STREAM_PORT))
            sock.sendall(f'GET /stream.wav HTTP/1.1\r\nHost: {self.args.host}\r\n\r\n'.encode('ascii'))
            report.update(connected_s=self.elapsed(), local_address=sock.getsockname(), bytes_read=0)
            sock.settimeout(0.5)
            while not stopping.wait(0.5):
                try:
                    data = sock.recv(1)
                except socket.timeout:
                    continue
                if not data:
                    report['eof_s'] = self.elapsed()
                    return
                report['bytes_read'] += len(data)
        except OSError as error:
            report['error'] = repr(error)
            report['error_s'] = self.elapsed()
        finally:
            sock.close()

    def apply(self, phase, commands):
        started = self.elapsed()
        for argv in commands:
            self.sh(argv)
        event = dict(kind='injection', phase=phase, at_s=started, apply_seconds=round(self.elapsed() - started, 3))
        self.record('injection', **event)
        return event

    def run_trial(self, name):
        spec = trial_spec(name)
        trial = dict(spec, started_s=self.elapsed(), passed=False, reason=None)
        self.check_deadline()
        self.read_uart()
        uart_start = len(self.uart_lines)
        before = self.snapshot()
        trial['before'] = before
        problem = settings_problem(before['settings'])
        if problem:
            trial['reason'] = problem
            self.record('trial_aborted', name=name, reason=problem)
            return trial
        install, remove = injection_commands(spec['profile'], self.args.interface, self.args.ifb)
        jobs = [self.start_decoder(f'{name}-listener-{index}', spec['seconds']) for index in range(1, spec['listeners'] + 1)]
        fresh = []
        wav = dict(label=f'{name}-wav')
        wav_stop = threading.Event()
        wav_thread = None
        if spec['kind'] == 'delayed-reader':
            wav_thread = threading.Thread(target=self.trickle_reader, args=(wav, wav_stop), daemon=True)
            wav_thread.start()
        injections = []
        installed = removed = None
        unavailable_before = self.poller.unavailable
        product_departures = []
        scanned = uart_start
        try:
            while True:
                self.check_deadline()
                self.read_uart()
                if self.serial.poll() is not None:
                    raise Failure('UART collector exited; see serial.log')
                # Product departures on UART are one replacement trigger; decoder EOF is the other.
                for line in self.uart_lines[scanned:]:
                    left = LEFT.search(line)
                    if left and listener_end_category(left.group(2)) != 'socket_error':
                        product_departures.append(left.group(1))
                scanned = len(self.uart_lines)
                for job in jobs + fresh:
                    self.poll_decoder(job)
                since_start = self.elapsed() - trial['started_s']
                cut = [job for job in jobs if job['ended'] is not None and job['ended'] != 'completed']
                if install and installed is None and since_start >= STEADY_SECONDS:
                    installed = self.apply('installed', install)
                    injections.append(installed)
                if installed is not None and removed is None:
                    held = self.elapsed() - installed['at_s'] - installed['apply_seconds']
                    stall_over = spec['kind'] == 'stall' and held >= spec['profile']['seconds']
                    deficit_over = spec['kind'] == 'deficit' and (product_departures or cut or held >= DEFICIT_SECONDS)
                    if stall_over or deficit_over:
                        removed = self.apply('removed', remove)
                        injections.append(removed)
                        if spec['kind'] == 'stall':
                            trial['stall_actual_s'] = round(removed['at_s'] + removed['apply_seconds'] - installed['at_s'], 3)
                # A departed listener is replaced once the path is clean, so admission is fresh.
                if spec['kind'] in ('stall', 'deficit') and removed is not None:
                    while len(fresh) < max(len(product_departures), len(cut)):
                        trigger = 'uart_departure' if len(product_departures) > len(fresh) else 'decoder_eof'
                        job = self.start_decoder(f'{name}-fresh-{len(fresh) + 1}', FRESH_LISTENER_SECONDS)
                        job['trigger'] = trigger
                        self.record('fresh_listener', label=job['label'], trigger=trigger)
                        fresh.append(job)
                if all(job['ended'] is not None for job in jobs + fresh) and (not install or removed is not None):
                    break
                # Hold a stall for its requested seconds, not the poll interval rounded up.
                sleep_for = 0.5
                if spec['kind'] == 'stall' and installed is not None and removed is None:
                    due = installed['at_s'] + installed['apply_seconds'] + spec['profile']['seconds']
                    sleep_for = max(0.0, min(0.5, due - self.elapsed()))
                time.sleep(sleep_for)
            # Checked closes need the listener receipt that follows the close on UART.
            settle = time.monotonic() + 5
            while time.monotonic() < settle:
                self.read_uart()
                time.sleep(0.2)
        finally:
            if wav_thread is not None and wav_thread.is_alive() and 'eof_s' not in wav and 'error_s' not in wav:
                wav['closed_s'] = self.elapsed()
            wav_stop.set()
            if wav_thread is not None:
                wav_thread.join(3)
            for job in jobs + fresh:
                self.stop_job(job)
        trial['after'] = self.snapshot()
        trial.update(trial_setup(spec, before, self.tolerances, installed))
        samples = [sample for sample in self.poller.since(trial['started_s'])]
        window = self.uart_lines[uart_start:]
        events = [self.decoder_event(job) for job in jobs] + [self.fresh_event(job) for job in fresh] + injections + samples
        if wav_thread is not None:
            events.append(dict(kind='wav', closed_s=wav.get('closed_s')))
        events += uart_events(window, self.uart_at[uart_start:])
        trial.update(trial_verdict(trial, events), wav_reader=wav if wav_thread is not None else None,
                     metrics_unavailable=self.poller.unavailable - unavailable_before,
                     events=[event for event in events if event['kind'] != 'metrics'], samples=len(samples), ended_s=self.elapsed())
        if spec['kind'] == 'null' and trial['passed']:
            measured = null_tolerances(samples, trial['started_s'] + STEADY_SECONDS)
            if measured is not None:
                self.tolerances = merge_tolerances(self.tolerances, measured)
                self.tolerance_sources.append(name)
                self.result['tolerances'] = dict(self.tolerances, source=self.tolerance_sources)
        self.record('trial', name=name, passed=trial['passed'], reason=trial['reason'])
        return trial

    def run(self):
        os.umask(0o077)
        self.out.mkdir(parents=True, exist_ok=False)
        self.events = (self.out / 'events.jsonl').open('w')
        self.write_result()
        failure = None
        try:
            # A lapsed sudo timestamp fails here in a second, before the serial reset.
            self.sh(['true'])
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
            raw = (self.out / 'uart.log').open('wb')
            serial_errors = (self.out / 'serial.log').open('wb')
            self.handles += [raw, serial_errors]
            self.serial = subprocess.Popen([str(self.repo / 'scripts/serial-log'), str(int(self.deadline - time.monotonic()) + 60)],
                                           stdout=raw, stderr=serial_errors, start_new_session=True,
                                           env=dict(os.environ, ESPFLASH_PORT=self.args.serial))
            self.processes.append(self.serial)
            self.uart = Tail(self.out / 'uart.log')
            self.wait_ready()
            self.poller = Poller(self.metrics_url, self.elapsed, self.record)
            self.poller.start()
            for argv in qdisc_plan(self.args.interface, self.args.host, self.args.ifb):
                self.sh(argv)
                self.installed_steps += 1
            self.result['qdisc_installed'] = self.unprivileged(['tc', 'qdisc', 'show', 'dev', self.args.interface])
            for name in self.args.trials:
                self.result['trials'][name] = self.run_trial(name)
                self.write_result()
        except KeyboardInterrupt:
            failure = 'interrupted'
        except BaseException as error:
            failure = f'{type(error).__name__}: {error}'
        finally:
            # A second interrupt must not skip the remaining tc calls or the result rename.
            previous = {number: signal.signal(number, signal.SIG_IGN) for number in (signal.SIGINT, signal.SIGTERM)}
            if failure is not None:
                self.result['error'] = failure
                self.record('failure', error=failure)
            end = time.monotonic() + 60
            if self.poller is not None:
                self.poller.stopping.set()
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
            if self.poller is not None:
                self.poller.join(3)
            if self.uart is not None:
                self.uart.close()
            for handle in self.handles:
                handle.close()
            self.result['passed'] = (failure is None and not self.cleanup_errors and self.result['restore_verified'] is True
                                     and all(trial['passed'] for trial in self.result['trials'].values())
                                     and len(self.result['trials']) == len(self.args.trials))
            self.write_result()
            os.replace(self.out / 'result.pending.json', self.out / 'result.json')
            self.events.close()
            print(f"{'PASS' if self.result['passed'] else 'FAIL'} {self.out / 'result.json'}", flush=True)
            for number, handler in previous.items():
                signal.signal(number, handler)
        return 0 if self.result['passed'] else 1


def argument_parser():
    parser = argparse.ArgumentParser(
        prog='scripts/network-fault',
        description='Install a tc prio/netem tree on the receiver interface, then run the interruption-recovery trials '
                    'against the board with real MP3 decoders while polling /api/metrics every second. Never runs as root: '
                    'cache credentials with `sudo -v` first; every tc/ip call uses `sudo -n`. The serial open resets the board once. '
                    'UART logs are private. The qdisc tree is torn down in a finally block and verified against the pre-run listing.')
    parser.add_argument('host', type=board_address, help='board IPv4 address')
    parser.add_argument('--interface', required=True, type=interface_name, help='receiver interface that carries board traffic')
    parser.add_argument('--ifb', default='ifb0', type=interface_name, help='ifb device for ingress shaping (default: ifb0)')
    parser.add_argument('--trials', type=trial_names, default=list(TRIALS), help='comma-separated trial names (default: all)')
    parser.add_argument('--serial', required=True, help='UART device for scripts/serial-log')
    parser.add_argument('--out', required=True, type=Path, help='new output directory; must not exist')
    parser.add_argument('--dry-run', action='store_true', help='print every privileged argv and create nothing')
    return parser


def main(argv=None):
    parser = argument_parser()
    args = parser.parse_args(argv)
    if args.out.exists():
        parser.error(f'{args.out} exists; choose a new output directory')
    require_unprivileged(os.geteuid())
    if args.dry_run:
        for line in dry_run_lines(args.interface, args.host, args.ifb, args.trials):
            print(line)
        return 0

    def interrupted(_number, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    return Driver(args, Path(__file__).resolve().parent.parent).run()


if __name__ == '__main__':
    sys.exit(main())
