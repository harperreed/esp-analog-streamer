# ABOUTME: Injects receiver-side packet loss and rate limits with tc netem while real decoders listen to the board.
# ABOUTME: qdisc planning, qdisc-show parsing and trial verdicts are pure; the device driver runs only from main.
import argparse
import hashlib
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
from stream_soak import (DECODER_PROGRESS_TIMEOUT_S, DECODER_RW_TIMEOUT_S, LOSS_KEYS, Tail, decoder_command, stop,
                         update_decoder)


class Failure(Exception):
    pass


# MP3 is 320 kbit/s fixed: 960 bytes per 1152-sample frame at 48 kHz.
MP3_BITRATE_BIT_PER_S = 320_000
MP3_FRAME_SAMPLES = 1152
# Verdict policies. `live`: a lagging listener skips forward on its socket and is never terminated for lag
# (docs/superpowers/plans/2026-10-15-live-stream-robustness.md, "The contract"). `exact`: the pre-skip
# verdicts, a MediaAge departure with a checked close and a fresh admission, kept byte-for-byte.
LIVE, EXACT = 'live', 'exact'
POLICIES = (LIVE, EXACT)
# A steady deficit skips about once per MEDIA_AGE_BASE_S / (1 - fraction); a gap beyond this multiple is not steady.
SKIP_GAP_TOLERANCE = 2.0
# Under live policy the per-frame write deadline is the encoded ring depth: the harness streams MP3, whose ring holds
# 500 frames of 1152 samples at 48 kHz = 12.0 s (FLAC: 12.03 s, 2.05 s with recognition on; decision 2026-10-15).
# A stall longer than this blocks the write past its budget, so the board ends the socket (`stopped reading`).
LIVE_WRITE_DEADLINE_S = 500 * 1152 / 48000
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
          'stall-4-1', 'stall-4-2', 'stall-8-1', 'stall-8-2', 'stall-15-1', 'stall-15-2', 'deficit-0.8-1', 'deficit-0.5-1',
          'delayed-reader-1']
# Band 4 of a four-band prio: the default priomap only names bands 0-2, so nothing but the u32 filter lands there.
EGRESS_NETEM_PARENT = '1:4'
IFB_NETEM_PARENT = 'root'
DEFAULT_ROOT_KINDS = ('mq', 'fq_codel', 'pfifo_fast')
# Every trial field trial_verdict reads; trial_setup must supply each one for every kind.
VERDICT_TRIAL_KEYS = ('kind', 'listeners', 'age_bound_us', 'largest_tolerance_bytes', 'largest_before',
                      'predicted_end_s', 'end_tolerance_s', 'policy', 'profile')


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
    """Upper bound on the time to a MediaAge end: age grows at (1 - fraction) s/s until the bound.

    A TCP flow through netem `rate` gets at most the nominal rate (ACK clocking, retransmits
    and burst compression only lose throughput), so the real deficit is at least the nominal one
    and the end comes no later than this. At or above full rate it never ends.
    """
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


def stall_seconds(spec):
    return spec['profile']['seconds'] if spec['kind'] == 'stall' else 0.0


def beyond_ring_depth(stall_s):
    """Whether a live listener is expected to end the socket on its write deadline during a stall of stall_s.

    Shorter stalls (up to the ring depth) resync on the same socket with zero departures; longer ones end with one
    checked `stopped reading` departure per listener followed by a fresh admission.
    """
    return stall_s > LIVE_WRITE_DEADLINE_S


def budget_seconds(names):
    # Startup after the serial reset, then each trial with its fresh listener, receipt wait and margin. A live
    # listener keeps its socket through a stall, so its decoder's wall time grows by the stall as well.
    return 180 + sum(trial_spec(name)['seconds'] + stall_seconds(trial_spec(name)) + FRESH_LISTENER_SECONDS + 30 for name in names)


def decoder_timeouts(spec):
    """ffmpeg's socket read timeout and the harness's progress watchdog, both stretched by the trial's stall so the
    receiver side never ends a listener the board kept."""
    extra = stall_seconds(spec)
    return dict(rw_timeout_s=DECODER_RW_TIMEOUT_S + extra, progress_timeout_s=DECODER_PROGRESS_TIMEOUT_S + extra)


def trial_setup(spec, before, tolerances, installed, policy):
    """Everything trial_verdict reads, from the spec, the pre-trial snapshot, null-* tolerances, the install record and the policy."""
    if policy not in POLICIES:
        raise ValueError(f'policy must be one of {", ".join(POLICIES)}')
    trial = dict(spec, expected_frames=round(spec['seconds'] * 48000), policy=policy,
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
# Same production close line encoder_fault_check pairs with departures; the receipt that certifies it comes later.
CLOSED = re.compile(r'stream: (\S+) socket closed in \d+ ms; releasing slot \(\d+ held\)')
# Per-listener skip counters the firmware adds to the departure line (plan, "Skip rule"); position unspecified, absence tolerated.
SKIP_KEYS = ('skips', 'frames_skipped', 'last_skip_us')
SKIP_TOKEN = re.compile(r'\s*\b(' + '|'.join(SKIP_KEYS) + r')=(\d+)\b')


def skip_fields(text):
    found = {key: int(value) for key, value in SKIP_TOKEN.findall(text)}
    return {key: found.get(key) for key in SKIP_KEYS}


def without_skip_tokens(text):
    return SKIP_TOKEN.sub('', text).strip()
# SO_LINGER 2 s + the lwIP 500 ms poll + margin: a close line later than this is not coming.
CLOSE_TIMEOUT_S = 3.0


def replacement_trigger(departed_peer, uart_lines_since, waited_s):
    """When to start a replacement for a departed peer: its close line, else a timeout fallback.

    Decoder EOF is the board shutting the socket; the slot frees only at the close line about
    0.5 s later, and with a survivor holding the other slot an earlier join is refused (503).
    Returns None, ('uart_close', close_device_us) or ('close_timeout', None).
    """
    for line in uart_lines_since:
        closed = CLOSED.search(line)
        if closed and closed.group(1) == departed_peer:
            return 'uart_close', listener_event_us(line)
    if waited_s >= CLOSE_TIMEOUT_S:
        return 'close_timeout', None
    return None


def replacement_decisions(departed, uart_lines, now_s, fresh_count, limit):
    """Replacements to start this poll: (pending, trigger, close_device_us), never beyond `limit` fresh listeners."""
    decisions = []
    for pending in departed:
        if pending['replaced'] or fresh_count + len(decisions) >= limit:
            continue
        trigger = replacement_trigger(pending['peer'], uart_lines[pending['index'] + 1:], now_s - pending['at_s'])
        if trigger is not None:
            decisions.append((pending, *trigger))
    return decisions


def may_exit(job_ends, fresh_ends, departed, uart_lines, now_s, injection_done):
    """Whether the trial loop may stop: every listener ended, the injection removed, and no departed
    peer still waiting for its close line (or the close timeout), so replacements are never skipped."""
    if not injection_done or any(end is None for end in job_ends + fresh_ends):
        return False
    return not any(not pending['replaced']
                   and replacement_trigger(pending['peer'], uart_lines[pending['index'] + 1:], now_s - pending['at_s']) is None
                   for pending in departed)


def refused_status(log_text):
    """HTTP status ffmpeg logged for a refused join, or 'unknown' when the log names none."""
    found = re.search(r'HTTP error (\d{3})', log_text)
    return int(found.group(1)) if found else 'unknown'


def uart_events(lines, at_s):
    """Admissions, departures and checked closes from production UART lines, in line order."""
    events = []
    for line, at in zip(lines, at_s):
        joined = JOINED.search(line)
        if joined:
            events.append(dict(kind='admission', peer=joined.group(1), device_us=int(joined.group(2)), at_s=at))
        left = LEFT.search(line)
        if left:
            events.append(dict(kind='departure', peer=left.group(1), end=without_skip_tokens(left.group(2)), device_us=int(left.group(3)),
                               at_s=at, **skip_fields(line)))
    for index, close in checked_listener_closes(lines):
        events.append(dict(kind='close', peer=close.group(1), device_us=listener_event_us(lines[index]), at_s=at_s[index]))
    return events


def metrics_sample(value, at_s):
    # Ready carries the age recorded after every successful write (firmware/src/listener_observation.rs:185).
    active = [item for item in value['listeners'] if item['state'] in ('writing', 'ready')]
    ages = [item['media_age_us'] for item in active]
    capture = value.get('capture')

    def counted(key):
        # Per-slot skip counters are absent until the skip lands in firmware; None is unreported, not zero.
        values = [item.get(key) for item in active]
        return None if any(item is None for item in values) else sum(values)
    return dict(kind='metrics', at_s=at_s, served_device_us=value['served_device_us'],
                media_age_us=max(ages) if ages else None, skips=counted('skips'), frames_skipped=counted('frames_skipped'),
                internal_largest=capture['internal_largest'] if capture else None,
                psram_largest=capture['psram_largest'] if capture else None,
                capture={key: capture[key] for key in LOSS_KEYS} if capture else None,
                read_frames=capture['read_frames'] if capture else None,
                settings=value.get('settings'), network=value.get('network'),
                boot_token=value.get('boot_token'), build_elf_sha256=value.get('build_elf_sha256'))


def attribute_skips(events):
    """Decoder events with the board's skip counters for their own listener.

    The harness never learns a decoder's local port, so decoders (start order, by label) pair with the trial's
    first admissions (device order) and each admission with the departure of that peer that follows it; the
    departure line carries the counters. Decoders already attributed (a retained result.json) are kept, and
    a decoder whose line carries no counters stays None.
    """
    decoders = sorted((event for event in events if event['kind'] == 'decoder'), key=lambda item: item['label'])
    admissions = sorted((event for event in events if event['kind'] == 'admission'), key=lambda item: item['device_us'])
    departures = [event for event in events if event['kind'] == 'departure']
    counters = {}
    for decoder, admission in zip(decoders, admissions):
        if decoder.get('skips') is not None:
            continue
        departure = next((item for item in departures if item['peer'] == admission['peer'] and item['device_us'] >= admission['device_us']), None)
        if departure is not None and departure.get('skips') is not None:
            counters[decoder['label']] = dict(skips=departure['skips'], frames_skipped=departure.get('frames_skipped') or 0, skip_peer=admission['peer'])
    return [dict(event, **counters[event['label']]) if event['kind'] == 'decoder' and event['label'] in counters else event for event in events]


def trial_skips(events):
    """Board skips for the whole trial: the sum over departure lines, else the last metrics sample that reports them, else zero."""
    departures = [event for event in events if event['kind'] == 'departure' and event.get('skips') is not None]
    if departures:
        return dict(skips=sum(event['skips'] for event in departures),
                    frames_skipped=sum(event.get('frames_skipped') or 0 for event in departures), source='departures')
    samples = [event for event in events if event['kind'] == 'metrics' and event.get('skips') is not None]
    if samples:
        last = max(samples, key=lambda item: item['at_s'])
        return dict(skips=last['skips'], frames_skipped=last.get('frames_skipped') or 0, source='metrics')
    return dict(skips=0, frames_skipped=0, source='none')


def skip_steadiness(samples, installed_s, removed_s, period_s):
    """Skip counter progress over [installed_s, removed_s] from the metrics samples that report it.

    Returns dict(skips, skip_times_s, max_skip_gap_s, monotonic); skips is None when no sample reports the counter.
    The gap list starts at the install, so a first skip that never comes is a gap too.
    """
    window = sorted((item for item in samples if item.get('skips') is not None and installed_s <= item['at_s'] <= removed_s),
                    key=lambda item: item['at_s'])
    if not window:
        return dict(skips=None, skip_times_s=[], max_skip_gap_s=None, monotonic=True)
    times, previous, monotonic = [], window[0]['skips'], True
    for item in window[1:]:
        if item['skips'] < previous:
            monotonic = False
        times += [item['at_s']] * max(0, item['skips'] - previous)
        previous = item['skips']
    edges = [installed_s] + times + [removed_s]
    return dict(skips=window[-1]['skips'] - window[0]['skips'], skip_times_s=times,
                max_skip_gap_s=max(b - a for a, b in zip(edges, edges[1:])), monotonic=monotonic)


def settings_problem(settings):
    if settings is None:
        return 'settings snapshot unavailable'
    if settings['autoplay'] is not False:
        return 'autoplay is enabled; the harness needs it off'
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
    events = attribute_skips(events)
    by_kind = {}
    for event in events:
        by_kind.setdefault(event['kind'], []).append(event)
    decoders = by_kind.get('decoder', [])
    closes = by_kind.get('close', [])
    admissions = by_kind.get('admission', [])
    samples = by_kind.get('metrics', [])
    injections = {event['phase']: event for event in by_kind.get('injection', [])}
    # Sockets the harness closed itself: completed or stopped decoders, fresh listeners, the WAV reader. Under live
    # policy the board never closes for lag, so a socket error at any decoder end is the receiver's own close
    # (ffmpeg -t can end a decode short after a skip); the decoder record still has to justify that end.
    harness_ends = [event['ended_s'] for event in decoders + by_kind.get('fresh', [])
                    if (trial['policy'] == LIVE or event['ended'] in ('completed', 'stopped')) and event['ended_s'] is not None]
    harness_ends += [event['closed_s'] for event in by_kind.get('wav', []) if event['closed_s'] is not None]

    def harness_owned(departure):
        # UART is read before decoders are polled, so the board's line can precede ended_s by up to one poll.
        return (listener_end_category(departure['end']) == 'socket_error'
                and any(-POLL_INTERVAL_S <= departure['at_s'] - at <= HARNESS_CLOSE_WINDOW_S for at in harness_ends))

    departures = [event for event in by_kind.get('departure', []) if not harness_owned(event)]
    harness_closes = len(by_kind.get('departure', [])) - len(departures)

    def fail(reason, **detail):
        return dict(passed=False, reason=reason, **detail)

    def continuous(decoder):
        return decoder['frames'] == decoder['expected_frames'] and decoder['discontinuities'] == 0 and decoder['exit_code'] == 0

    def close_of(departure):
        return next((close for close in closes if close['peer'] == departure['peer'] and close['device_us'] >= departure['device_us']), None)

    unclaimed = sorted(admissions, key=lambda item: item['device_us'])

    def admitted_within(departure):
        # Device time from the departure: with a free second slot the fresh join lands before the old
        # close completes, and the checked close is required separately. Each join answers one departure.
        for item in unclaimed:
            if departure['device_us'] < item['device_us'] <= departure['device_us'] + ADMISSION_WINDOW_S * 1e6:
                unclaimed.remove(item)
                return True
        return False

    def missing_join(departure):
        refusals = [str(event['refused_status']) for event in by_kind.get('fresh', []) if event.get('refused_status') is not None]
        return (f"no fresh join line within {ADMISSION_WINDOW_S:g} s of {departure['peer']} departure (device_us {departure['device_us']})"
                + (f"; fresh listener refused: {', '.join(refusals)}" if refusals else ''))

    def counters_flat():
        counted = [sample['capture'] for sample in samples if sample['capture'] is not None]
        return len(counted) >= 2 and counted[0] == counted[-1]

    def checked_departure(departure, *categories):
        if listener_end_category(departure['end']) not in categories:
            return f"departure {departure['peer']} ended {departure['end']!r}, not {'/'.join(categories)}"
        if close_of(departure) is None:
            return f"departure {departure['peer']} has no checked close"
        return None

    total_skips = trial_skips(events)

    def live_decoder(decoder):
        """The decoder's record with its skip bound and the reason it is not continuous modulo skips, if any.

        Counters come from the decoder's own departure line when attribute_skips paired it, else the trial
        total bounds every decoder. A board skip shows to an MP3 decoder as late audio, not as a timestamp
        gap, so discontinuities may fall short of the skips but never exceed them; ffmpeg -t ends at the
        jumped timeline, so the decode may stop the skipped audio short, within one MP3 frame.
        """
        attributed = decoder.get('skips') is not None
        skips = decoder['skips'] if attributed else total_skips['skips']
        frames_skipped = (decoder.get('frames_skipped') or 0) if attributed else total_skips['frames_skipped']
        record = dict(label=decoder['label'], frames=decoder['frames'], expected_frames=decoder['expected_frames'],
                      discontinuities=decoder['discontinuities'], exit_code=decoder['exit_code'], ended=decoder['ended'],
                      skips=skips, frames_skipped=frames_skipped, attributed=attributed)
        floor = decoder['expected_frames'] - (frames_skipped + 1) * MP3_FRAME_SAMPLES
        if decoder['exit_code'] != 0:
            problem = f"exit code {decoder['exit_code']}"
        elif decoder['discontinuities'] > skips:
            problem = f"{decoder['discontinuities']} discontinuities exceed {skips} board skips"
        elif not floor <= decoder['frames'] <= decoder['expected_frames']:
            problem = f"{decoder['frames']} frames outside {floor}..{decoder['expected_frames']} (expected minus {frames_skipped} skipped)"
        else:
            problem = None
        return record, problem

    def live_verdict(records, reason, **detail):
        return dict(passed=True, reason=reason, departed=0, decoders=records, skips=total_skips['skips'],
                    frames_skipped=total_skips['frames_skipped'], skips_source=total_skips['source'], **detail)

    def live_stall_beyond_ring_depth():
        # Every listener's write blocked past the ring depth: the board ends each socket on its write deadline, the
        # close is checked, and the slot is refilled by the harness's replacement within the admission window.
        detail = dict(beyond_ring_depth=True, skips=total_skips['skips'], frames_skipped=total_skips['frames_skipped'],
                      skips_source=total_skips['source'])
        if 'removed' not in injections:
            return fail('injection install/removal not recorded', **detail)
        if len(departures) != trial['listeners']:
            return fail(f"expected {trial['listeners']} frame_timeout departure(s) beyond the {LIVE_WRITE_DEADLINE_S:g} s ring depth, "
                        f"found {len(departures)}", departures=departures, **detail)
        for departure in sorted(departures, key=lambda item: item['device_us']):
            problem = checked_departure(departure, 'frame_timeout')
            if problem:
                return fail(problem, **detail)
            if not admitted_within(departure):
                return fail(missing_join(departure), **detail)
        return dict(passed=True, reason=f"{len(departures)} frame_timeout departure(s) beyond the ring depth with checked close and fresh admission",
                    departed=len(departures), **detail)

    def live_stall():
        if trial['age_bound_us'] is None:
            return fail('no media age bound; run null-* first')
        if beyond_ring_depth(trial['profile']['seconds']):
            return live_stall_beyond_ring_depth()
        if departures:
            return fail(f'{len(departures)} listener(s) departed under live policy', departures=departures)
        if 'removed' not in injections:
            return fail('injection install/removal not recorded')
        records = []
        for decoder in decoders:
            record, problem = live_decoder(decoder)
            records.append(record)
            if problem:
                return fail(f"{decoder['label']}: {problem}", decoders=records)
        active_until = max((decoder['ended_s'] for decoder in decoders if decoder['ended_s'] is not None), default=None)
        active = [sample['media_age_us'] for sample in samples if sample['media_age_us'] is not None
                  and injections['removed']['at_s'] <= sample['at_s'] <= (active_until if active_until is not None else float('inf'))]
        ages = dict(age_peak_after_removal_us=max(active) if active else None, age_last_active_us=active[-1] if active else None)
        if not active or active[-1] > trial['age_bound_us']:
            return fail('media age did not return under the null bound', decoders=records, **ages)
        return live_verdict(records, f"continuous modulo {total_skips['skips']} skip(s), no departures", beyond_ring_depth=False, **ages)

    def live_deficit():
        if departures:
            return fail(f'{len(departures)} listener(s) departed under live policy', departures=departures)
        if 'installed' not in injections or 'removed' not in injections:
            return fail('injection install/removal not recorded')
        installed_s = injections['installed']['at_s'] + injections['installed']['apply_seconds']
        removed_s = injections['removed']['at_s']
        records = []
        for decoder in decoders:
            record, problem = live_decoder(decoder)
            records.append(record)
            if problem:
                return fail(f"{decoder['label']}: {problem}", decoders=records)
            if decoder['ended_s'] is not None and decoder['ended_s'] < removed_s:
                return fail(f"{decoder['label']} ended {removed_s - decoder['ended_s']:.1f} s before the injection was removed", decoders=records)
        period = MEDIA_AGE_BASE_S / (1 - trial['profile']['fraction'])
        steady = skip_steadiness(samples, installed_s, removed_s, period)
        detail = dict(predicted_skip_period_s=period, max_skip_gap_s=steady['max_skip_gap_s'], skip_times_s=steady['skip_times_s'])
        if steady['skips'] is None:
            return fail('no metrics sample reports skip counters during the deficit', decoders=records, **detail)
        if steady['skips'] < 1:
            return fail('no skip during the deficit', decoders=records, **detail)
        if not steady['monotonic']:
            return fail('skip counter fell during the deficit; not a steady rate', decoders=records, **detail)
        if steady['max_skip_gap_s'] > SKIP_GAP_TOLERANCE * period:
            return fail(f"skip gap {steady['max_skip_gap_s']:.1f} s exceeds {SKIP_GAP_TOLERANCE:g} x the predicted {period:.1f} s period; not steady",
                        decoders=records, **detail)
        return live_verdict(records, f"listener kept its socket with {steady['skips']} skips at a steady rate, no departures", **detail)

    def judge():
        if len(decoders) != trial['listeners']:
            return fail(f"expected {trial['listeners']} decoder records, found {len(decoders)}")
        cut = [decoder for decoder in decoders if not continuous(decoder)]
        kind = trial['kind']
        if trial['policy'] == LIVE and kind == 'stall':
            return live_stall()
        if trial['policy'] == LIVE and kind == 'deficit':
            return live_deficit()
        if kind == 'null':
            if cut:
                return fail('decoder not continuous', decoders=cut, departures=departures)
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
            ages = dict(age_peak_after_removal_us=None, age_last_active_us=None)
            if len(cut) < len(decoders):
                # Judge survival on ages sampled after removal while an original decoder still ran;
                # the post-trial sample is idle and carries no age.
                if 'removed' not in injections:
                    return fail('injection install/removal not recorded', **ages)
                active_until = max((decoder['ended_s'] for decoder in decoders if decoder['ended_s'] is not None), default=None)
                active = [sample['media_age_us'] for sample in samples if sample['media_age_us'] is not None
                          and injections['removed']['at_s'] <= sample['at_s'] <= (active_until if active_until is not None else float('inf'))]
                if active:
                    ages = dict(age_peak_after_removal_us=max(active), age_last_active_us=active[-1])
                if not active or active[-1] > trial['age_bound_us']:
                    return fail('media age did not return under the null bound', **ages)
            for departure in sorted(departures, key=lambda item: item['device_us']):
                problem = checked_departure(departure, 'media_age')
                if problem:
                    return fail(problem, **ages)
                if not admitted_within(departure):
                    return fail(missing_join(departure), **ages)
            return dict(passed=True, reason='continuous' if not cut else f'{len(cut)} MediaAge departure(s) with checked close and fresh admission',
                        departed=len(cut), **ages)
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
            # Age grows at most 1 s/s from at most the null bound, so no end can precede this floor.
            earliest = injections['installed']['apply_seconds'] + MEDIA_AGE_BASE_S - trial['age_bound_us'] / 1e6
            if observed < earliest:
                return fail(f"departure after {observed:.2f} s, earlier than the age floor {earliest:.2f} s",
                            observed_end_s=observed, earliest_end_s=earliest)
            if observed > trial['predicted_end_s'] + trial['end_tolerance_s']:
                return fail(f"departure after {observed:.2f} s, later than the predicted bound {trial['predicted_end_s']:.2f} s + {trial['end_tolerance_s']:.2f}",
                            observed_end_s=observed, earliest_end_s=earliest)
            if not admitted_within(departure):
                return fail(missing_join(departure), observed_end_s=observed, earliest_end_s=earliest)
            last = next((sample for sample in reversed(samples) if sample['internal_largest'] is not None), None)
            if last is None:
                return fail('no post-trial largest-free sample', observed_end_s=observed)
            for key, tolerance in trial['largest_tolerance_bytes'].items():
                if abs(last[key] - trial['largest_before'][key]) > tolerance:
                    return fail(f"{key} {last[key]} is outside {trial['largest_before'][key]} ± {tolerance}", observed_end_s=observed)
            return dict(passed=True, reason='MediaAge departure near prediction, fresh admission, largest-free returned',
                        observed_end_s=observed, earliest_end_s=earliest)
        # delayed-reader: the starved WAV reader ends on its own frame write limit (ring capacity / 48 kHz, 'stopped
        # reading') or, when that is later than the 4 s age bound, on media age (stream.rs:729). Either is its deadline.
        wav_end = listener_end_category(departures[0]['end']) if len(departures) == 1 else None
        if cut:
            return fail('MP3 decoder not continuous', decoders=cut, wav_end_category=wav_end)
        reader = next(iter(by_kind.get('wav', [])), None)
        if reader is not None and reader.get('connected_s') is None and (reader.get('error') is not None or reader.get('phase') is not None):
            return fail(f"WAV reader never connected (phase {reader.get('phase')}): {reader.get('error')}", wav_reader=reader, wav_end_category=wav_end)
        if len(departures) != 1:
            return fail(f'expected one WAV reader departure, found {len(departures)}', departures=departures, wav_end_category=wav_end)
        problem = checked_departure(departures[0], 'frame_timeout', 'media_age')
        if problem:
            return fail(problem, wav_end_category=wav_end)
        if not counters_flat():
            return fail('capture loss counters changed or missing', wav_end_category=wav_end)
        return dict(passed=True, reason=f'MP3 continuous; WAV reader ended on its deadline ({wav_end}) with checked close', wav_end_category=wav_end)

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
                self.record('metrics', **{key: sample[key] for key in ('at_s', 'media_age_us', 'skips', 'frames_skipped', 'internal_largest',
                                                                        'psram_largest', 'capture')})
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
        self.result = dict(passed=False, error=None, host=args.host, interface=args.interface, ifb=args.ifb, policy=args.policy,
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

    def start_decoder(self, label, seconds, spec):
        decoder_args = SimpleNamespace(path='/stream.mp3', stream_port=STREAM_PORT, seconds=seconds, **decoder_timeouts(spec))
        errors_path = self.out / f'{label}.ffmpeg.log'
        error_handle = errors_path.open('wb')
        self.handles.append(error_handle)
        progress, hashes = self.out / f'{label}.progress', self.out / f'{label}.framehash'
        process = subprocess.Popen(decoder_command(decoder_args, self.args.host, progress, hashes),
                                   stdout=subprocess.DEVNULL, stderr=error_handle, start_new_session=True)
        self.processes.append(process)
        # Under live policy a flagged timestamp gap is evidence to count against the board's skips, not a reason to stop.
        job = dict(label=label, process=process, args=decoder_args, tail=Tail(progress), errors=errors_path, log_tail=Tail(errors_path),
                   accounting=FrameHash(hashes, tolerate_discontinuities=self.args.policy == LIVE), decoded_audio_frames=0,
                   decoded_seconds=0, timeline_seconds=0, advanced_at=time.monotonic(), started_s=self.elapsed(), ended=None)
        self.record('decoder_started', label=label, seconds=seconds, **decoder_timeouts(spec))
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
        return dict(kind='fresh', label=job['label'], ended=job['ended'], ended_s=job.get('ended_s'), trigger=job['trigger'],
                    close_device_us=job['close_device_us'], refused_status=job.get('refused_status'))

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
            report['phase'] = 'connect'
            sock.connect((self.args.host, STREAM_PORT))
            report['phase'] = 'request'
            sock.sendall(f'GET /stream.wav HTTP/1.1\r\nHost: {self.args.host}\r\n\r\n'.encode('ascii'))
            report.update(connected_s=self.elapsed(), local_address=sock.getsockname(), bytes_read=0)
            report['phase'] = 'reading'
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
        fields = dict(phase=phase, at_s=started, apply_seconds=round(self.elapsed() - started, 3))
        self.record('injection', **fields)
        return dict(kind='injection', **fields)

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
        jobs = [self.start_decoder(f'{name}-listener-{index}', spec['seconds'], spec) for index in range(1, spec['listeners'] + 1)]
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
        departed = []  # product departures awaiting a replacement: dict(peer, index, at_s, replaced)
        scanned = uart_start
        try:
            while True:
                self.check_deadline()
                self.read_uart()
                if self.serial.poll() is not None:
                    raise Failure('UART collector exited; see serial.log')
                for index, line in enumerate(self.uart_lines[scanned:], scanned):
                    left = LEFT.search(line)
                    if left and listener_end_category(left.group(2)) != 'socket_error':
                        departed.append(dict(peer=left.group(1), index=index, at_s=self.uart_at[index], replaced=False))
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
                    deficit_over = spec['kind'] == 'deficit' and (departed or cut or held >= DEFICIT_SECONDS)
                    if stall_over or deficit_over:
                        removed = self.apply('removed', remove)
                        injections.append(removed)
                        if spec['kind'] == 'stall':
                            trial['stall_actual_s'] = round(removed['at_s'] + removed['apply_seconds'] - installed['at_s'], 3)
                # A departed listener is replaced once the path is clean and its slot is freed (its close
                # line, or the timeout that says the line never came), so the admission is fresh, not refused.
                if spec['kind'] in ('stall', 'deficit') and removed is not None:
                    for pending, trigger, close_device_us in replacement_decisions(departed, self.uart_lines, self.elapsed(), len(fresh), len(jobs)):
                        pending['replaced'] = True
                        job = self.start_decoder(f'{name}-fresh-{len(fresh) + 1}', FRESH_LISTENER_SECONDS, spec)
                        job['trigger'], job['close_device_us'] = trigger, close_device_us
                        self.record('fresh_listener', label=job['label'], trigger=trigger, close_device_us=close_device_us, replaces=pending['peer'])
                        fresh.append(job)
                # Pending departures hold the loop until their close lines (or the timeout) so a replacement can start.
                replacing = departed if spec['kind'] in ('stall', 'deficit') else []
                if may_exit([job['ended'] for job in jobs], [job['ended'] for job in fresh], replacing, self.uart_lines, self.elapsed(),
                            injection_done=not install or removed is not None):
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
        trial.update(trial_setup(spec, before, self.tolerances, installed, self.args.policy))
        samples = [sample for sample in self.poller.since(trial['started_s'])]
        window = self.uart_lines[uart_start:]
        events = [self.decoder_event(job) for job in jobs] + [self.fresh_event(job) for job in fresh] + injections + samples
        if wav_thread is not None:
            events.append(dict(kind='wav', closed_s=wav.get('closed_s'), connected_s=wav.get('connected_s'), phase=wav.get('phase'),
                               error=wav.get('error'), error_s=wav.get('error_s')))
        uart = uart_events(window, self.uart_at[uart_start:])
        # A fresh listener with no join line after its start was refused or never connected; say which.
        for job, event in zip(fresh, (event for event in events if event['kind'] == 'fresh')):
            if not any(item['kind'] == 'admission' and item['at_s'] >= job['started_s'] for item in uart):
                job['refused_status'] = event['refused_status'] = refused_status(job['errors'].read_text(errors='replace')[-16384:])
        events = attribute_skips(events + uart)
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


def reverdict(directory, repo, policy):
    """Recompute every trial verdict of a retained run from its result.json and events.jsonl under `policy`; writes reverdict.json only."""
    directory = Path(directory)
    source = (directory / 'result.json').read_bytes()
    result = json.loads(source)
    with (directory / 'events.jsonl').open() as handle:
        samples = [item for item in (json.loads(line) for line in handle) if item['kind'] == 'metrics']

    def git(*argv):
        try:
            return subprocess.run(['git', *argv], cwd=repo, capture_output=True, text=True, timeout=10, check=True).stdout
        except (OSError, subprocess.SubprocessError):
            return None
    commit = git('rev-parse', 'HEAD')
    status = git('status', '--porcelain', '--', 'scripts/')
    # HEAD alone is ambiguous while the tree is dirty; the file hashes say which harness judged.
    hashes = {name: hashlib.sha256((Path(repo) / 'scripts' / name).read_bytes()).hexdigest() for name in ('network_fault.py', 'encoder_fault_check.py')}
    tolerances, sources = None, []
    written = dict(policy=policy, source_result_sha256=hashlib.sha256(source).hexdigest(), harness_commit=commit.strip() if commit else None,
                   harness_sha256=hashes, harness_dirty=bool(status.strip()) if status is not None else None,
                   tolerances=None, trials={}, original={})
    for name in result['trial_order']:
        trial = result['trials'].get(name)
        written['original'][name] = trial['passed'] if trial else None
        if trial is None:
            written['trials'][name] = dict(passed=False, reason='trial not recorded')
            continue
        if 'events' not in trial:
            written['trials'][name] = dict(passed=False, reason=trial['reason'])
            continue
        spec = trial_spec(name)
        events = trial['events']
        if trial.get('wav_reader') is not None:
            # Older records kept the connect outcome only on wav_reader; the verdict reads it from the wav event.
            record = trial['wav_reader']
            events = [dict(event, **{key: record.get(key) for key in ('connected_s', 'phase', 'error', 'error_s')}) if event['kind'] == 'wav' else event
                      for event in events]
        installed = next((event for event in events if event['kind'] == 'injection' and event['phase'] == 'installed'), None)
        window = [item for item in samples if trial['started_s'] <= item['at_s'] <= trial['ended_s']]
        setup = trial_setup(spec, trial['before'], tolerances, installed, policy)
        verdict = trial_verdict(setup, events + window)
        written['trials'][name] = dict(verdict, age_bound_us=setup['age_bound_us'], largest_tolerance_bytes=setup['largest_tolerance_bytes'],
                                       predicted_end_s=setup['predicted_end_s'], end_tolerance_s=setup['end_tolerance_s'])
        if spec['kind'] == 'null' and verdict['passed']:
            measured = null_tolerances(window, trial['started_s'] + STEADY_SECONDS)
            if measured is not None:
                tolerances = merge_tolerances(tolerances, measured)
                sources.append(name)
                written['tolerances'] = dict(tolerances, source=list(sources))
    (directory / 'reverdict.json').write_text(json.dumps(written, indent=2) + '\n')
    lines = [f"{name:18} {'PASS' if verdict['passed'] else 'FAIL'} (originally {'PASS' if written['original'][name] else 'FAIL'}): {verdict['reason']}"
             for name, verdict in written['trials'].items()]
    lines.append(f"wrote {directory / 'reverdict.json'}")
    return written, lines


def argument_parser():
    parser = argparse.ArgumentParser(
        prog='scripts/network-fault',
        description='Install a tc prio/netem tree on the receiver interface, then run the interruption-recovery trials '
                    'against the board with real MP3 decoders while polling /api/metrics every second. Never runs as root: '
                    'cache credentials with `sudo -v` first; every tc/ip call uses `sudo -n`. The serial open resets the board once. '
                    'UART logs are private. The qdisc tree is torn down in a finally block and verified against the pre-run listing.')
    parser.add_argument('host', nargs='?', type=board_address, help='board IPv4 address (live run)')
    parser.add_argument('--reverdict', type=Path, metavar='DIR',
                        help='recompute verdicts of a retained run from DIR/result.json and DIR/events.jsonl into DIR/reverdict.json; '
                             'no board, root or serial; exits 0 once written even when verdicts fail; takes only --policy')
    parser.add_argument('--policy', choices=POLICIES, default=LIVE,
                        help='verdict policy: live (default) never accepts a lag departure and allows decoder discontinuities up to the '
                             'board-reported skips; exact keeps the MediaAge-departure-plus-fresh-admission verdicts')
    parser.add_argument('--interface', type=interface_name, help='receiver interface that carries board traffic (live run)')
    parser.add_argument('--ifb', default='ifb0', type=interface_name, help='ifb device for ingress shaping (default: ifb0)')
    parser.add_argument('--trials', type=trial_names, help='comma-separated trial names (default: all)')
    parser.add_argument('--serial', help='UART device for scripts/serial-log (live run)')
    parser.add_argument('--out', type=Path, help='new output directory; must not exist (live run)')
    parser.add_argument('--dry-run', action='store_true', help='print every privileged argv and create nothing')
    return parser


def main(argv=None):
    parser = argument_parser()
    args = parser.parse_args(argv)
    repo = Path(__file__).resolve().parent.parent
    if args.reverdict is not None:
        if any(value not in (None, False) for value in (args.host, args.interface, args.serial, args.out, args.trials, args.dry_run)):
            parser.error('--reverdict takes only --policy')
        for name in ('result.json', 'events.jsonl'):
            if not (args.reverdict / name).is_file():
                parser.error(f'{args.reverdict / name} is missing')
        # Analysis succeeded once the file is written; the verdicts themselves are in it.
        for line in reverdict(args.reverdict, repo, args.policy)[1]:
            print(line)
        return 0
    if args.trials is None:
        args.trials = list(TRIALS)
    missing = [flag for flag, value in (('host', args.host), ('--interface', args.interface), ('--serial', args.serial), ('--out', args.out)) if value is None]
    if missing:
        parser.error('a live run needs ' + ', '.join(missing))
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
    return Driver(args, repo).run()


if __name__ == '__main__':
    sys.exit(main())
