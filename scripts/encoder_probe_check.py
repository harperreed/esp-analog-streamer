# ABOUTME: Collects bounded same-ELF OFF/ON/OFF trials without media listeners or settings writes.
# ABOUTME: Preserves raw evidence and separates sequence validity from performance acceptance.
import argparse
import hashlib
import ipaddress
import json
import os
import sys
from pathlib import Path
import re
import signal
import subprocess
import threading
import time
import wave

from encoder_fault_check import check_budget, read_uart_lines, validate_listener_export, listener_event_ms, validate_encoder_export, encoder_export_receipt

FAULTS = ('queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts',
          'supervisor_restarts', 'overflow_restarts', 'settling_discarded_frames', 'discontinuities', 'health_restart_requests')
PHASES = ('OFF1', 'ON', 'OFF2')
METRICS = ('elapsed_us', 'task_runtime_us', 'runtime_valid', 'encoded_frames', 'resets',
           'listeners_start', 'listeners_end', 'block_samples', 'raw_read_overruns',
           'raw_wait_total_us', 'encode_wall_total_us', 'encoded_lock_wait_wall_total_us',
           'encoded_push_held_wall_total_us')
PROBE = ('sequence', 'phase', 'interval', 'retained', 'completed', 'valid',
         'success_attempt_us', 'rejected_attempt_us', 'pcm_push_us', 'delay_us',
         'delay_calls', 'raw_remainder_us', 'residual_us')


def fields(line):
    values = {}
    for key, value in re.findall(r'\b([a-z_]+)=(Some\(-?\d+\)|None|true|false|-?\d+|[A-Z][A-Z0-9]*)(?=\s|$)', line):
        if key in values:
            raise ValueError('duplicate field: ' + key)
        if value in ('true', 'false'):
            values[key] = value == 'true'
        elif value == 'None':
            values[key] = None
        elif value.startswith('Some('):
            values[key] = int(value[5:-1])
        elif re.fullmatch(r'-?\d+', value):
            values[key] = int(value)
        else:
            values[key] = value
    return values


def require_state(sonos, app, expected_format='mp3'):
    if expected_format not in ('mp3', 'flac'):
        raise ValueError('Unsupported expected format: ' + str(expected_format))
    config = sonos['config']
    if not (sonos['state'] == 'stopped' and sonos['active_format'] == expected_format
            and config['output_format'] == expected_format and config['buffer_preset'] == 'med'
            and config['autoplay'] is False and app.get('shazam_enabled') is False):
        raise ValueError(f'Requires stopped {expected_format.upper()}/med, autoplay false and shazam_enabled false')


def storage_receipt(line, output_format='Mp3'):
    """Read frozen Rust Debug receipt layout without inferring a memory class."""
    result = {}
    for key in ('baseline_pending', 'raw_payload', 'raw_stamps', 'chunk', 'output_address',
                'output_len', 'output_capacity', 'output_required', 'shine_policy', 'mp3'):
        if len(re.findall(r'\b'+key+'=', line)) != 1:
            raise ValueError('missing or duplicate storage field: '+key)
    if strict_token(line, 'baseline_pending') != 'true':
        raise ValueError('invalid storage baseline_pending')
    if strict_token(line, 'shine_policy') != 'MALLOC_CAP_SPIRAM|MALLOC_CAP_8BIT':
        raise ValueError('unexpected allocator policy receipt')
    strict_numbers(line, ('output_address', 'output_len', 'output_capacity', 'output_required'))
    for name, count in [('raw_payload', 2), ('raw_stamps', 2), ('chunk', 3)]:
        match = re.search(r'\b' + name + r'=\(([^)]+)\)(?=\s|$)', line)
        if not match:
            raise ValueError('missing storage ' + name)
        tokens = [value.strip() for value in match[1].split(',')]
        if any(not re.fullmatch(r'(?:0x[0-9a-fA-F]+|[0-9]+)', token) for token in tokens):
            raise ValueError('invalid storage tuple '+name)
        numbers = [int(token, 0) for token in tokens]
        if len(numbers) != count or any(not 0 < value <= 2**64-1 for value in numbers):
            raise ValueError('invalid storage ' + name)
        result[name] = numbers
    numeric = fields(line)
    for key in ('output_address', 'output_len', 'output_capacity', 'output_required'):
        result[key] = numeric[key]
    if (result['chunk'][1] != 1920 or result['chunk'][2] < result['chunk'][1]
            or result['output_address'] <= 0 or result['output_len'] < 0
            or result['output_required'] != (960 if output_format == 'Mp3' else 16399)
            or result['output_capacity'] < result['output_required']
            or result['output_len'] > result['output_capacity']):
        raise ValueError('invalid chunk/output reservation')
    if output_format == 'Flac':
        if strict_token(line, 'mp3') != 'None': raise ValueError('unexpected FLAC MP3 receipt')
        return result
    match = re.search(r'\bmp3=Some\(\((\d+),\s*(\d+),\s*EncoderReceipt\s*\{\s*config:\s*AllocationReceipt\s*\{\s*address:\s*(\d+),\s*bytes:\s*(\d+)\s*\},\s*bitstream:\s*AllocationReceipt\s*\{\s*address:\s*(\d+),\s*bytes:\s*(\d+)\s*\}\s*\}\)\)(?=\s|$)', line)
    if not match:
        raise ValueError('missing MP3 live allocation receipt')
    pcm, pcm_bytes, config, config_bytes, bitstream, bitstream_bytes = map(int, match.groups())
    if (any(not 0 < value <= 2**64-1 for value in (pcm, pcm_bytes, config, config_bytes, bitstream, bitstream_bytes))
            or pcm_bytes != 4608 or bitstream_bytes != 4096):
        raise ValueError('invalid PCM/Shine receipt')
    result.update(pcm_address=pcm, pcm_bytes=pcm_bytes, config_address=config,
                  config_bytes=config_bytes, bitstream_address=bitstream, bitstream_bytes=bitstream_bytes)
    if 'shine_policy=MALLOC_CAP_SPIRAM|MALLOC_CAP_8BIT' not in line:
        raise ValueError('unexpected allocator policy receipt')
    return result


def phase_summary(windows):
    totals = {key: sum(row['metrics'][key] for row in windows) for key in METRICS
              if key.endswith('_us')}
    totals['frame_delta'] = sum(row['frame_delta'] for row in windows)
    totals['reset_delta'] = sum(row['reset_delta'] for row in windows)
    totals['generated_seconds'] = sum(row['frame_delta'] * row['metrics']['block_samples'] / 48000
                                     for row in windows)
    totals['generation_ratio'] = totals['generated_seconds'] / (totals['elapsed_us'] / 1e6)
    totals['codec_wall_us_per_frame'] = totals['encode_wall_total_us'] / totals['frame_delta']
    for key in ('success_attempt_us', 'rejected_attempt_us', 'pcm_push_us', 'delay_us', 'delay_calls',
                'raw_remainder_us', 'residual_us'):
        totals[key] = sum(row['probe'][key] for row in windows)
    return totals


OWNER = ('telemetry_epoch', 'telemetry_window')
WINDOW = (*OWNER, 'window_start_us', 'window_end_us', 'sequence')


def strict_token(line, key):
    tokens = re.findall(r'\b'+key+r'=([^\s]+)', line)
    if len(tokens) != 1 or len(re.findall(r'\b'+key+'=', line)) != 1:
        raise ValueError('missing or duplicate field: '+key)
    return tokens[0]


def strict_numbers(line, keys):
    """Require each complete bounded token exactly once; partial regex matches are not fields."""
    values = {}
    for key in keys:
        tokens = re.findall(r'\b' + key + r'=([^\s]+)', line)
        if len(tokens) != 1 or len(re.findall(r'\b' + key + '=', line)) != 1:
            raise ValueError('metadata missing or duplicate field: ' + key)
        token = tokens[0]
        if not re.fullmatch(r'[0-9]+', token) or len(token) > 20 or int(token) > 2**64-1:
            raise ValueError('metadata invalid bounded number: ' + key)
        values[key] = int(token)
    return values


def strict_report_fields(line, keys, booleans=(), optional=(), phases=()):
    numeric = [key for key in keys if key not in (*booleans, *optional, *phases)]
    strict_numbers(line, numeric)
    for key in (*booleans, *optional, *phases):
        tokens = re.findall(r'\b' + key + r'=([^\s]+)', line)
        if len(tokens) != 1 or len(re.findall(r'\b' + key + '=', line)) != 1:
            raise ValueError('missing or duplicate report field: ' + key)
        token = tokens[0]
        if key in booleans:
            valid = token in ('true', 'false')
        elif key in phases:
            valid = token in (*PHASES, 'COMPLETE', 'INACTIVE')
        else:
            match = re.fullmatch(r'Some\(([0-9]+)\)', token)
            valid = token == 'None' or (match and len(match[1]) <= 20 and int(match[1]) <= 2**64-1)
        if not valid:
            raise ValueError('invalid report field: ' + key)


def analyze(lines):
    """Pair current records by owner identity; retain original arrival and line provenance."""
    ordered, errors, pairs, receipts = [], [], {}, []
    boundaries, storages, announcements, invalidations = {}, {}, [], []
    saw_owner = False
    def event_key(value):
        return tuple(value[key] for key in (*OWNER, 'event_us'))
    def add_error(index, error):
        errors.append(f'line {index+1}: {error}')
    for index, (stamp, raw) in enumerate(lines):
        line = re.sub(r'\x1b\[[0-9;]*m', '', raw)
        try:
            raw_fields = fields(line) if any(prefix in line for prefix in ('encoder-boundary:', 'encoder-probe:', 'stream-encode:')) else {}
            if ('stream: encoder fell behind capture' in line
                    or raw_fields.get('raw_read_overruns', 0) > 0
                    or raw_fields.get('raw_overruns', 0) > 0
                    or ('stream-encode:' in line and raw_fields.get('resets', 0) > 0)):
                errors.append(f'raw-loss/reset/invalidation evidence at line {index+1}')
            if ('stream-listener:' in line or 'stream-sender:' in line
                    or re.search(r'stream: .* (joined |left after |socket closed )', line)):
                errors.append(f'listener activity has ambiguous probe window at line {index+1}')
            if ('rst:' in line and saw_owner) or 'stream: encoder failed' in line:
                errors.append(f'epoch interrupted at line {index+1}')
            if any(prefix in line for prefix in ('encoder-boundary:', 'encoder-telemetry:', 'encoder-probe-storage:', 'stream-encode:', 'encoder-probe: sequence=')):
                saw_owner = True
            if 'encoder-telemetry:' in line:
                validate_encoder_export([line])
                receipts.append((index+1, encoder_export_receipt(line)))
                continue
            if 'stream-encode:' in line or 'encoder-probe: sequence=' in line:
                value = strict_numbers(line, WINDOW)
                if 'stream-encode:' in line:
                    strict_report_fields(line, METRICS, booleans=('runtime_valid',))
                else:
                    strict_report_fields(line, PROBE, booleans=('retained', 'completed', 'valid'),
                        optional=('raw_remainder_us', 'residual_us'), phases=('phase',))
                if value['window_end_us'] <= value['window_start_us']:
                    raise ValueError('metadata reversed/empty encoder window')
                identity = tuple(value[key] for key in WINDOW)
                kind = 'metric' if 'stream-encode:' in line else 'probe'
                pair = pairs.setdefault(identity, {})
                if kind in pair:
                    raise ValueError('duplicate window partner identity')
                pair[kind] = (stamp, raw, index)
                continue
            if 'encoder-boundary:' in line:
                value = strict_numbers(line, (*OWNER, 'event_us', 'baseline_us', 'prior_epoch',
                    'frames', 'resets', 'raw_overruns', 'raw_wait_us', 'raw_attempts'))
                value['reason'] = strict_token(line, 'reason')
                value['format'] = strict_token(line, 'format')
                if value['reason'] not in ('Startup', 'Overrun', 'Switched', 'Stopped') or value['format'] not in ('Flac', 'Mp3'):
                    raise ValueError('invalid encoder boundary reason/format')
                if value['baseline_us'] > value['event_us'] or (value['reason'] != 'Stopped' and value['baseline_us'] != value['event_us']):
                    raise ValueError('metadata encoder boundary baseline differs from event')
                if value['reason'] == 'Switched':
                    if value['prior_epoch'] + 1 != value['telemetry_epoch']:
                        raise ValueError('metadata encoder boundary switched epoch')
                elif value['prior_epoch'] != value['telemetry_epoch']:
                    raise ValueError('metadata encoder boundary prior epoch')
                key = event_key(value)
                if key in boundaries:
                    raise ValueError('duplicate encoder boundary identity')
                boundaries[key] = (value, stamp, raw, index)
                if value['reason'] == 'Overrun' or value['resets'] or value['raw_overruns']:
                    errors.append(f'raw-loss encoder boundary at line {index+1}')
                if value['reason'] == 'Stopped':
                    errors.append(f'encoder boundary stopped at line {index+1}')
                continue
            if 'encoder-probe-storage:' in line:
                value = strict_numbers(line, (*OWNER, 'sequence', 'event_us', 'baseline_us'))
                if value['baseline_us'] != value['event_us']:
                    raise ValueError('metadata storage baseline differs from owner event')
                key = event_key(value)
                if key in storages:
                    raise ValueError('duplicate storage lifecycle identity')
                storages[key] = (value, stamp, raw, index)
                continue
            if 'encoder-probe: invalidated' in line:
                value = strict_numbers(line, (*OWNER, 'event_us', 'frames', 'resets', 'raw_overruns', 'raw_wait_us', 'raw_attempts'))
                value['reason'] = strict_token(line, 'reason')
                if len(re.findall(r'\bposition=', line)) != 1:
                    raise ValueError('missing or duplicate invalidation position')
                position = re.search(r'\bposition=\(([0-9]+), "([A-Z0-9]+)", ([0-9]+)\)(?=\s|$)', line)
                if not position or len(position[1]) > 20 or int(position[1]) > 2**64-1:
                    raise ValueError('malformed invalidation position')
                value['position'] = (int(position[1]), position[2], int(position[3]))
                invalidations.append((value, index))
                continue
            if 'stream: active format ' in line:
                value = strict_numbers(line, (*OWNER, 'event_us', 'baseline_us'))
                match = re.search(r'stream: active format (Flac|Mp3)(?=\s|$)', line)
                if not match or value['event_us'] != value['baseline_us']:
                    raise ValueError('metadata malformed format announcement/baseline')
                value['format'] = match[1]
                announcements.append((value, stamp, raw, index))
                continue
            if 'audio: frames=' in line:
                value = strict_numbers(line, ('capture_begin_us', 'capture_end_us', 'frames', *FAULTS))
                if value['capture_end_us'] < value['capture_begin_us']:
                    raise ValueError('metadata reversed capture bound')
                at, rank = value['capture_begin_us'], 4
            else:
                at, rank = 0, 5
            ordered.append((at, rank, index, stamp, raw))
        except (ValueError, TypeError, RuntimeError) as error:
            add_error(index, error)
    def covered(event, index):
        return any(receipt_index > index+1 and receipt['observed_event_range_valid']
            and receipt['observed_event_min_us'] <= event <= receipt['observed_event_max_us']
            for receipt_index, receipt in receipts)
    for key, (value, stamp, raw, index) in storages.items():
        boundary = boundaries.get(key)
        if boundary is None:
            add_error(index, 'missing encoder boundary for storage metadata')
            continue
        b = boundary[0]
        try:
            storage_receipt(raw, b['format'])
            if b['reason'] == 'Stopped':
                raise ValueError('stopped boundary cannot replace storage')
            if b['reason'] == 'Startup' and (key[:2] != (0, 0) or value['sequence'] != (1 if b['format'] == 'Mp3' else 0)):
                raise ValueError('metadata startup epoch/window/sequence')
            if b['format'] == 'Flac' and value['sequence'] != 0:
                raise ValueError('metadata initial FLAC sequence')
        except (ValueError, KeyError, TypeError) as error:
            add_error(index, error)
        if b['format'] == 'Mp3':
            ordered.append((value['baseline_us'], 1, index, stamp, raw))
        if not covered(value['event_us'], index):
            add_error(index, 'missing final encoder export receipt for storage')
    startups = [(key, row) for key, row in boundaries.items() if row[0]['reason'] == 'Startup']
    if len(startups) != 1:
        errors.append('missing/duplicate encoder startup boundary')
    for key, (b, stamp, raw, index) in boundaries.items():
        if b['reason'] != 'Stopped' and key not in storages:
            add_error(index, 'missing storage partner for encoder boundary')
        if not covered(b['event_us'], index):
            add_error(index, 'missing final encoder export receipt for boundary')
        matching_announcements = [row for row in announcements if event_key(row[0]) == key and row[0]['baseline_us'] == b['baseline_us'] and row[0]['format'] == b['format']]
        matching_invalidations = [row for row in invalidations if row[0]['telemetry_epoch'] == b['prior_epoch'] and row[0]['telemetry_window'] == b['telemetry_window'] and row[0]['event_us'] == b['event_us'] and row[0]['reason'] == b['reason']]
        if b['reason'] == 'Startup':
            if matching_announcements or matching_invalidations:
                add_error(index, 'startup boundary has unexpected format/invalidation partner')
        elif b['reason'] == 'Switched':
            clean_initial = (len(startups) == 1 and startups[0][0][:2] == (0, 0)
                and startups[0][1][0]['format'] == 'Flac' and startups[0][0] in storages
                and startups[0][1][0]['event_us'] < b['event_us']
                and key[:2] == (1, 0) and b['prior_epoch'] == 0 and b['format'] == 'Mp3'
                and key in storages and storages[key][0]['sequence'] == 1
                and len(matching_announcements) == 1 and len(matching_invalidations) == 1
                and matching_invalidations[0][0]['position'] == (0, 'INACTIVE', 1)
                and all(b[name] == matching_invalidations[0][0][name] for name in ('frames', 'resets', 'raw_overruns', 'raw_wait_us', 'raw_attempts'))
                and b['resets'] == b['raw_overruns'] == 0)
            if not clean_initial:
                add_error(index, 'encoder boundary lacks clean initial inactive switch lifecycle')
        elif len(matching_invalidations) != 1:
            add_error(index, 'missing/duplicate invalidation partner for encoder boundary')
    for value, stamp, raw, index in announcements:
        key = event_key(value)
        boundary = boundaries.get(key)
        if not boundary or boundary[0]['reason'] != 'Switched' or boundary[0]['format'] != value['format'] or key not in storages or value['baseline_us'] != storages[key][0]['baseline_us']:
            add_error(index, 'metadata format announcement lacks matching switch/storage boundary')
        else:
            ordered.append((value['event_us'], 2, index, stamp, raw))
        if not covered(value['event_us'], index):
            add_error(index, 'missing final encoder export receipt for format announcement')
    for value, index in invalidations:
        matches = [b for b, _, _, _ in boundaries.values() if b['reason'] == 'Switched' and b['prior_epoch'] == value['telemetry_epoch'] and b['telemetry_window'] == value['telemetry_window'] and b['event_us'] == value['event_us']]
        if value['reason'] != 'Switched' or value['position'] != (0, 'INACTIVE', 1) or value['resets'] or value['raw_overruns'] or len(matches) != 1:
            add_error(index, 'raw-loss/active invalidation lacks clean initial boundary')
        if not covered(value['event_us'], index):
            add_error(index, 'missing final encoder export receipt for invalidation')
    for identity, pair in pairs.items():
        if set(pair) != {'metric', 'probe'}:
            errors.append('missing or conflicting encoder/probe pairing metadata: ' + str(identity))
            continue
        epoch, window, start, end, sequence = identity
        metric = fields(pair['metric'][1])
        if metric.get('elapsed_us') != end-start:
            errors.append('metadata elapsed differs from producer window')
        for rank, kind in ((3, 'metric'), (3.5, 'probe')):
            stamp, raw, index = pair[kind]
            ordered.append((end, rank, index, stamp, raw))
        if not covered(end, max(pair['metric'][2], pair['probe'][2])):
            errors.append(f'missing final encoder export receipt for epoch/window {epoch}/{window}')
    ordered.sort(key=lambda row: row[:3])
    result = analyze_owner_order([(stamp, raw, index) for _, _, index, stamp, raw in ordered])
    result['errors'] = errors + result['errors']
    result['valid_sequence'] &= not errors and all(row['valid'] for row in result['sequences'])
    return result


def analyze_owner_order(lines):
    """Analyze validated owner order; unknown first deltas stay unknown."""
    sequences, captures, errors = [], [], []
    current = pending = previous = None
    for stamp, raw, index in lines:
        line = re.sub(r'\x1b\[[0-9;]*m', '', raw)
        row = dict(line=index + 1, host_monotonic=stamp, raw=raw)
        try:
            if 'listener-telemetry:' in line:
                try:
                    validate_listener_export([line])
                except RuntimeError as error:
                    errors.append(f'line {index+1}: {error}')
            if ('event_us=' in line and ('stream-listener:' in line or 'stream-sender:' in line
                    or re.search(r'stream: .* (joined |left after |socket closed )', line))):
                row['event_ms'] = listener_event_ms(line)
                # No common clock receipt ties deferred listener events to probe windows.
                # Reject the acquisition rather than place the event at its UART arrival.
                errors.append(f'delayed listener activity has ambiguous probe window at line {index+1}')
            if 'audio: frames=' in line:
                row['counters'] = fields(line)
                captures.append(row)
            if 'encoder-probe-storage:' in line:
                value = fields(line)
                if pending is not None:
                    current['errors'].append('unpaired encoder report before storage')
                sequence = value['sequence']
                current = dict(sequence=sequence, storage=row, windows=[], errors=[], phases={}, valid=False, storage_valid=False, storage_errors=[])
                try:
                    current['storage_receipt'] = storage_receipt(line)
                    current['storage_valid'] = True
                except (ValueError, KeyError) as error:
                    current['storage_errors'].append(str(error))
                if any(item['sequence'] == sequence for item in sequences):
                    current['errors'].append('duplicate sequence storage')
                current['metadata'] = value
                if value.get('baseline_pending') is not True:
                    current['errors'].append('missing fresh baseline receipt')
                sequences.append(current)
                pending = previous = None
            elif 'encoder-probe: invalidated' in line:
                if current:
                    current['errors'].append(f'invalidated at line {index+1}')
                pending = previous = None
            elif 'rst:' in line or 'stream: active format ' in line or 'stream: encoder failed' in line:
                expected_announcement = (current is not None and not current['windows']
                                         and pending is None and current['storage_valid']
                                         and not current.get('format_announcement')
                                         and re.search(r'stream: active format Mp3(?:\s|$)', line))
                if expected_announcement:
                    current['format_announcement'] = row
                elif current:
                    current['errors'].append(f'epoch interrupted at line {index+1}')
                pending = previous = None
            elif ('stream-listener:' in line or 'stream-sender:' in line
                  or re.search(r'stream: .* (joined /stream\.|left after )', line)) and current:
                current['errors'].append(f'listener activity at line {index+1}')
            elif 'stream-encode:' in line and current:
                if pending is not None:
                    current['errors'].append('unpaired/duplicate encoder report')
                row['metrics'] = fields(line)
                pending = row
            elif 'encoder-probe: sequence=' in line:
                if current is None:
                    errors.append(f'probe report without storage at line {index+1}')
                    continue
                probe = fields(line)
                if pending is None:
                    current['errors'].append(f'unpaired probe report at line {index+1}')
                    continue
                metric = pending['metrics']
                row.update(probe=probe, metrics=metric, metric_line=pending['line'],
                           frame_delta=None, reset_delta=None)
                pending = None
                position = len(current['windows'])
                post_complete = probe.get('phase') == 'COMPLETE' and position == 24
                if post_complete:
                    current.setdefault('post_completion', []).append(row)
                else:
                    current['windows'].append(row)
                problem = []
                if position == 0:
                    baseline = current['metadata']
                    if (metric['window_start_us'] != baseline['baseline_us']
                            or metric['telemetry_epoch'] != baseline['telemetry_epoch']
                            or metric['telemetry_window'] != baseline['telemetry_window'] + 1):
                        problem.append('metadata first window differs from storage baseline')
                elif previous and (metric['telemetry_epoch'] != previous['metrics']['telemetry_epoch']
                        or metric['telemetry_window'] != previous['metrics']['telemetry_window'] + 1
                        or metric['window_start_us'] != previous['metrics']['window_end_us']):
                    problem.append('metadata discontinuous window identity or bounds')
                if any(key not in metric for key in METRICS) or any(key not in probe for key in PROBE):
                    raise ValueError('missing report field')
                if any(type(value) is int and not 0 <= value <= 2**64-1
                       for source in (metric, probe) for value in source.values()):
                    problem.append('negative or overflowing metric')
                expected_phase = 'COMPLETE' if post_complete else PHASES[min(position // 8, 2)]
                if (position >= 24 and not post_complete) or (probe['sequence'], probe['phase'], probe['interval']) != (
                        current['sequence'], expected_phase, position % 8 + 1):
                    problem.append('nonconsecutive sequence/phase/interval')
                if probe['retained'] is not (position < 24 and position % 8 >= 2) or probe['completed'] is not (position == 23):
                    problem.append('incorrect retained/completed label')
                if probe['valid'] is not True or metric['runtime_valid'] is not True:
                    problem.append('invalid probe/runtime')
                if metric['elapsed_us'] <= 0 or metric['block_samples'] != 1152:
                    problem.append('invalid MP3 elapsed/block samples')
                if metric['listeners_start'] != 0 or metric['listeners_end'] != 0 or metric['raw_read_overruns'] != 0:
                    problem.append('listener/raw-loss contamination')
                if previous:
                    row['frame_delta'] = metric['encoded_frames'] - previous['metrics']['encoded_frames']
                    row['reset_delta'] = metric['resets'] - previous['metrics']['resets']
                    if row['frame_delta'] <= 0 or row['reset_delta'] != 0:
                        problem.append('nonpositive frame delta or reset')
                if probe['retained'] and previous is None:
                    problem.append('missing consecutive predecessor')
                raw_remainder = metric['raw_wait_total_us'] - probe['success_attempt_us'] - probe['rejected_attempt_us']
                accounted = sum(metric[key] for key in ('raw_wait_total_us', 'encode_wall_total_us',
                                  'encoded_lock_wait_wall_total_us', 'encoded_push_held_wall_total_us'))
                accounted += probe['pcm_push_us'] + probe['delay_us']
                residual = metric['elapsed_us'] - accounted
                if raw_remainder < 0 or residual < 0 or probe['raw_remainder_us'] != raw_remainder or probe['residual_us'] != residual:
                    problem.append('invalid checked remainder/residual')
                if probe['phase'] != 'ON' and any(probe[key] != 0 for key in (
                        'success_attempt_us', 'rejected_attempt_us', 'pcm_push_us', 'delay_us', 'delay_calls')):
                    problem.append('ON-only timing present in OFF')
                row['errors'] = problem
                current['errors'].extend(f'line {index+1}: {item}' for item in problem)
                previous = row
        except (ValueError, KeyError, TypeError, IndexError, RuntimeError) as error:
            (current['errors'] if current else errors).append(f'line {index+1}: {error}')
            pending = previous = None
    if pending is not None:
        current['errors'].append('unfinished encoder/probe pair')
    for sequence in sequences:
        windows = sequence['windows']
        if len(windows) != 24:
            sequence['errors'].append(f'expected 24 windows, found {len(windows)}')
        sequence['phase_valid'] = not sequence['errors']
        capture_errors_start = len(sequence['errors'])
        retained = [row for row in windows if row['probe'].get('retained') is True]
        if retained:
            start = retained[0]['metrics']['window_start_us']
            end = retained[-1]['metrics']['window_end_us']
            before = [row for row in captures if row['counters'].get('capture_end_us', end+1) <= start]
            after = [row for row in captures if row['counters'].get('capture_begin_us', 0) >= end]
            if not before or not after:
                sequence['errors'].append('capture reports do not bracket retained producer span')
            else:
                left = max(before, key=lambda row: row['counters']['capture_end_us'])
                right = min(after, key=lambda row: row['counters']['capture_begin_us'])
                covered = [row for row in captures if left['counters']['capture_begin_us'] <= row['counters']['capture_begin_us'] <= right['counters']['capture_begin_us']]
                covered.sort(key=lambda row: row['counters']['capture_begin_us'])
                if any(a['counters']['capture_end_us'] > b['counters']['capture_begin_us'] for a, b in zip(covered, covered[1:])):
                    sequence['errors'].append('overlapping capture bounds')
                sequence['capture_coverage'] = dict(start=left, end=right, fault_deltas={})
                frames = [row['counters'].get('frames') for row in covered]
                if (any(type(value) is not int or value < 0 for value in frames)
                        or frames[-1] <= frames[0]
                        or any(b < a for a, b in zip(frames, frames[1:]))):
                    sequence['errors'].append('missing/nonpositive capture progress')
                for key in FAULTS:
                    values = [row['counters'].get(key) for row in covered]
                    if any(type(value) is not int or value < 0 for value in values):
                        sequence['errors'].append('missing/invalid capture counter ' + key)
                    elif any(value != values[0] for value in values):
                        sequence['errors'].append('capture fault/discontinuity ' + key)
                    else:
                        sequence['capture_coverage']['fault_deltas'][key] = 0
        sequence['capture_errors'] = sequence['errors'][capture_errors_start:]
        sequence['capture_valid'] = bool(retained) and not sequence['capture_errors']
        sequence['errors'].extend(sequence['storage_errors'])
        if not sequence['errors']:
            try:
                for phase in PHASES:
                    selected = [row for row in retained if row['probe']['phase'] == phase]
                    if len(selected) != 6:
                        raise ValueError('phase missing retained windows')
                    sequence['phases'][phase] = phase_summary(selected)
                sequence['valid'] = True
            except (ValueError, KeyError, TypeError, ZeroDivisionError) as error:
                sequence['errors'].append(str(error))
    return dict(valid_sequence=not errors and any(row['valid'] for row in sequences),
                sequences=sequences, errors=errors, capture_reports=captures,
                performance_acceptance='not assessed')


def load_uart(out):
    """Use existing LF framing and match each line end to its original read arrival."""
    raw = (out/'uart.log').read_bytes()
    arrivals = [json.loads(line) for line in (out/'uart-arrival.jsonl').read_text().splitlines()]
    cursor = 0
    for arrival in arrivals:
        if arrival['offset'] != cursor or arrival['bytes'] <= 0:
            raise ValueError('nonconsecutive UART arrivals')
        cursor += arrival['bytes']
    if cursor != len(raw):
        raise ValueError('UART arrival coverage differs from raw bytes')
    lines, offset, chunk = [], 0, 0
    for line in read_uart_lines(out/'uart.log'):
        offset = raw.index(b'\n', offset) + 1
        while arrivals[chunk]['offset'] + arrivals[chunk]['bytes'] < offset:
            chunk += 1
        lines.append((arrivals[chunk]['host_monotonic'], line))
    return lines


def analyze_acquisition(out, reset_stamp):
    """Anchor the owned acquisition at its first ROM reset, preserving raw offsets."""
    rows = load_uart(out)
    raw_lines = (out/'uart.log').read_bytes().split(b'\n')
    arrivals = [json.loads(line) for line in (out/'uart-arrival.jsonl').read_text().splitlines()]
    offset = 0
    for index, ((stamp, line), raw) in enumerate(zip(rows, raw_lines)):
        if reset_stamp is not None and stamp >= reset_stamp and 'rst:' in line:
            if line.rstrip('\r') != 'rst:0x1 (POWERON_RESET),boot:0x1f (SPI_FAST_FLASH_BOOT)':
                raise ValueError('unexpected first ROM reset after owned reset release')
            arrival = next(row for row in arrivals if row['offset'] <= offset+len(raw) < row['offset']+row['bytes'])
            selection = dict(byte_offset=offset, line=index+1, host_monotonic=stamp,
                             reset_released=reset_stamp, arrival_offset=arrival['offset'],
                             arrival_bytes=arrival['bytes'], rom_reset=line)
            analysis = analyze(rows[index:])
            if any('rst:' in later for _, later in rows[index+1:]):
                analysis['valid_sequence'] = False
                analysis['errors'].append('epoch interrupted by later ROM reset')
            return analysis, selection
        offset += len(raw)+1
    raise ValueError('missing ROM reset after owned reset release')


def require_source(player):
    if player.poll() is not None:
        raise RuntimeError(f'Analog source exited unexpectedly: {player.returncode}')


def join_started(thread, seconds):
    if thread is not None and thread.ident is not None:
        thread.join(seconds)


def stop_source(player, event, log):
    size = lambda: log.stat().st_size if log.exists() else 0
    first_error = None
    try:
        event('source_cleanup_begin', returncode=player.poll(), aplay_log_bytes=size())
    except BaseException as error:
        first_error = error
    try:
        if player.poll() is None:
            player.terminate()
            try:
                player.wait(5)
            except subprocess.TimeoutExpired:
                try:
                    event('source_cleanup_kill')
                except BaseException as error:
                    if first_error is None:
                        first_error = error
                finally:
                    player.kill()
                    player.wait(5)
    except BaseException as error:
        if first_error is None:
            first_error = error
    finally:
        try:
            event('source_cleanup_end', returncode=player.poll(), aplay_log_bytes=size())
        except BaseException as error:
            if first_error is None:
                first_error = error
    if first_error is not None:
        raise first_error


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description='Bounded same-ELF probe. Owns UART/reset and analog source. GET only; no media listener, flash, mixer writes or Sonos Play. Parent must release UART/source first.')
    parser.add_argument('host', type=ipaddress.IPv4Address)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--budget-seconds', type=float, default=360)
    args = parser.parse_args(argv)
    if not 240 <= args.budget_seconds <= 600:
        parser.error('--budget-seconds must be 240..600')
    try:
        args.image = args.elf.read_bytes()
        args.fixture = args.fixture.resolve()
        with wave.open(str(args.fixture)) as wav:
            args.fixture_info = dict(seconds=wav.getnframes()/wav.getframerate(), rate=wav.getframerate(),
                                     channels=wav.getnchannels(), sample_bytes=wav.getsampwidth(), frames=wav.getnframes())
        required = 12 + args.budget_seconds + 16
        if args.fixture_info['seconds'] < required:
            raise ValueError(f'fixture must cover at least {required:g}s without looping')
        if args.fixture_info['rate'] != 48000 or args.fixture_info['channels'] != 2 or args.fixture_info['sample_bytes'] != 2:
            raise ValueError('requires approved 48 kHz stereo 16-bit PCM WAV')
        args.fixture_hash = hashlib.sha256(args.fixture.read_bytes()).hexdigest()
        if args.out.exists():
            raise ValueError('output directory must be fresh')
    except (OSError, ValueError, wave.Error) as error:
        parser.error(str(error))
    return args


def require_routing(control, expected, returncode, stdout):
    """Check each declared playback channel in a read-only amixer receipt."""
    if 'Playback channels: Mono' in stdout:
        channels = ('Mono',)
    elif 'Playback channels: Front Left - Front Right' in stdout:
        channels = ('Front Left', 'Front Right')
    else:
        raise ValueError('unknown playback channels: ' + control)
    rows = [line.strip() for line in stdout.splitlines()]
    if returncode or any(not any(row.startswith(channel + ': Playback ') and expected in row
                                and (control != 'Master' or '[on]' in row)
                                for row in rows) for channel in channels):
        raise ValueError('unexpected routing/gain: ' + control)


def remaining_timeout(deadline, now=None, maximum=8):
    remaining = deadline - (time.monotonic() if now is None else now)
    if remaining <= 0:
        raise TimeoutError('fixed global deadline exhausted')
    return min(maximum, remaining)


# A child bounds DNS/header/body reads by an absolute parent timeout, including
# responses that keep supplying bytes without ever hitting a socket timeout.
STATUS_REQUEST = r"""
import json, sys, urllib.request, urllib.error
row = {}
try:
    with urllib.request.urlopen(sys.argv[1], timeout=float(sys.argv[2])) as response:
        row['http_status'] = response.status
        raw = response.read(131073)
        row['raw'] = raw.decode(errors='replace')
        if len(raw) > 131072:
            raise ValueError('status exceeds 128 KiB')
        row['value'] = json.loads(raw)
except BaseException as error:
    row['error'] = repr(error)
    if isinstance(error, urllib.error.HTTPError):
        row['http_status'] = error.code
        row['raw'] = error.read(131073).decode(errors='replace')
print(json.dumps(row))
"""


def fetch_status(url, timeout):
    try:
        command = subprocess.run([sys.executable, '-c', STATUS_REQUEST, url, str(timeout)],
                                 capture_output=True, text=True, timeout=timeout)
        if command.returncode:
            return dict(error=f'status child exited {command.returncode}', stdout=command.stdout, stderr=command.stderr)
        return json.loads(command.stdout)
    except subprocess.TimeoutExpired as error:
        return dict(error=f'absolute status timeout ({timeout}s)',
                    partial_stdout=(error.stdout or b'').decode(errors='replace'),
                    partial_stderr=(error.stderr or b'').decode(errors='replace'))


def main(argv=None):
    args = parse_args(argv)
    os.umask(0o077)
    args.out.mkdir(parents=True, exist_ok=False)
    (args.out/'unified.elf').write_bytes(args.image)
    result = dict(completed=False, acquisition_complete=False, errors=[], events=[], status_receipts=[],
                  image_provenance=dict(path=str(args.elf.resolve()), sha256=hashlib.sha256(args.image).hexdigest(), device_attestation=False),
                  fixture_sha256=args.fixture_hash, fixture_info=args.fixture_info, budget_seconds=args.budget_seconds,
                  limitations=['RTS reset is not a physical power cycle.', 'Source liveness/routing do not prove samples at ADC.',
                               'Arrival stamps are host read times, not device execution.', 'Zero listener endpoints cannot exclude brief churn.',
                               'No overhead tolerance or release performance acceptance is inferred.'])
    port = reader = player = None
    stopping = threading.Event()
    reader_errors = []
    uart_lock = threading.Lock()
    source_stopping = threading.Event()
    source_monitor = None
    source_errors = []
    deadline = None
    final_deadline = None
    reset_asserted = False
    previous_sigterm = signal.getsignal(signal.SIGTERM)

    def interrupted(number, frame):
        raise SystemExit(number)

    signal.signal(signal.SIGTERM, interrupted)

    def event(kind, **values):
        row = dict(kind=kind, host_monotonic=time.monotonic(), utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), **values)
        result['events'].append(row)
        with (args.out/'events.jsonl').open('a') as output:
            output.write(json.dumps(row)+'\n')
        return row['host_monotonic']

    def failure(error):
        row = dict(error=repr(error), host_monotonic=time.monotonic())
        result['errors'].append(row)
        # Append-only receipts survive any later failure or finalization.
        with (args.out/'errors.jsonl').open('a') as output:
            output.write(json.dumps(row)+'\n')

    def get(path, final=False):
        timeout = (remaining_timeout(final_deadline, maximum=6) if final else
                   8 if deadline is None else remaining_timeout(deadline))
        row = dict(path=path, started_monotonic=time.monotonic())
        row.update(fetch_status('http://' + str(args.host) + path, timeout))
        row['ended_monotonic'] = time.monotonic()
        result['status_receipts'].append(row)
        with (args.out/'status.jsonl').open('a') as output:
            output.write(json.dumps(row)+'\n')
        if 'error' in row:
            raise RuntimeError(f'{path}: {row["error"]}')
        return row['value']

    def snapshot(label, final=False):
        values = []
        for path in ('/api/sonos', '/api/state'):
            try:
                values.append(get(path, final))
            except BaseException as error:
                if not final:
                    raise
                failure(error)
                values.append(None)
        sonos, app = values
        if sonos is None or app is None:
            raise RuntimeError('incomplete final state snapshot; request errors retained')
        event(label, sonos=sonos, app_state=app, source_returncode=None if player is None else player.poll())
        require_state(sonos, app)
        if 'before' in result and sonos['config'] != result['before']['config']:
            raise ValueError('saved config changed')
        return sonos

    def routing(label, final=False):
        reads = {}
        for control, expected in [('Headphone', '[on]'), ('Line Out', '[off]'), ('Master', '[0.00dB]'), ('PCM', '[0.00dB]')]:
            timeout = (remaining_timeout(final_deadline, maximum=1) if final else
                       5 if deadline is None else remaining_timeout(deadline, maximum=5))
            if timeout <= 0:
                raise TimeoutError('routing deadline reached')
            command = subprocess.run(['amixer', '-c', 'PCH', 'sget', control], capture_output=True, text=True, timeout=timeout)
            reads[control] = dict(stdout=command.stdout, stderr=command.stderr, returncode=command.returncode)
            event('routing_readback', label=label, control=control, **reads[control])
            require_routing(control, expected, command.returncode, command.stdout)
        return reads

    def collect():
        offset = 0
        try:
            with (args.out/'uart.log').open('wb') as log, (args.out/'uart-arrival.jsonl').open('w') as arrivals:
                while not stopping.is_set():
                    data = port.read(4096)
                    if data:
                        stamp = time.monotonic()
                        with uart_lock:
                            log.write(data); log.flush()
                            arrivals.write(json.dumps(dict(host_monotonic=stamp, offset=offset, bytes=len(data)))+'\n'); arrivals.flush()
                            offset += len(data)
        except BaseException as error:
            reader_errors.append(repr(error))

    def monitor_source():
        with (args.out/'source-liveness.jsonl').open('w') as output:
            while not source_stopping.is_set():
                row = dict(host_monotonic=time.monotonic(), returncode=player.poll())
                output.write(json.dumps(row)+'\n'); output.flush()
                if row['returncode'] is not None:
                    source_errors.append(row)
                    break
                source_stopping.wait(.2)

    def watch():
        require_source(player)
        if source_errors:
            raise RuntimeError(source_errors)
        if reader_errors:
            raise RuntimeError(reader_errors)
        check_budget(args.out)
        if deadline is not None and time.monotonic() >= deadline:
            raise TimeoutError('fixed global collection budget exhausted; invalid sequences retained')

    def pause(seconds):
        end = time.monotonic()+seconds
        while time.monotonic() < end:
            watch()
            time.sleep(min(.2, end-time.monotonic()))

    reset_stamp = None
    try:
        result['before'] = snapshot('before_reset')
        result['routing_before'] = routing('before_source')
        import serial
        port = serial.Serial('/dev/ttyUSB0', 115200, timeout=.2)
        reader = threading.Thread(target=collect, daemon=True)
        reader.start()
        with (args.out/'aplay.log').open('wb') as log:
            argv = ['aplay', '-q', '-D', 'plughw:CARD=PCH,DEV=0', str(args.fixture)]
            launch = event('source_launch', argv=argv)
            deadline = launch + 12 + args.budget_seconds
            player = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT)
            event('source_started', pid=player.pid, returncode=player.poll())
            source_monitor = threading.Thread(target=monitor_source, daemon=True)
            source_monitor.start()
            pause(12)
            port.dtr = False
            reset_asserted = True
            port.rts = True
            event('reset_asserted')
            pause(.1)
            port.rts = False
            reset_asserted = False
            reset_stamp = event('reset_released')
            pause(25)
            snapshot('after_boot')
            while True:
                watch()
                # Reader owns writes; analyze only bytes covered by complete arrival records.
                try:
                    with uart_lock:
                        analysis, result['boot_selection'] = analyze_acquisition(args.out, reset_stamp)
                except (ValueError, json.JSONDecodeError):
                    pause(.2)
                    continue
                if analysis['valid_sequence']:
                    result['analysis'] = analysis
                    result['acquisition_complete'] = True
                    break
                pause(.2)
            result['completed'] = True
    except BaseException as error:
        failure(error)
    finally:
        if port is not None and reset_asserted:
            try:
                port.rts = False
                event('reset_released_during_cleanup')
            except BaseException as error:
                failure(error)
        if player is not None:
            final_deadline = min(deadline + 16, time.monotonic() + 16)
            # Each attempt preserves its failure and does not suppress the other.
            try:
                result['routing_after'] = routing('final_before_source_cleanup', final=True)
            except BaseException as error:
                failure(error)
            try:
                snapshot('final_before_source_cleanup', final=True)
            except BaseException as error:
                failure(error)
            try:
                require_source(player)
            except BaseException as error:
                failure(error)
        source_stopping.set()
        try:
            join_started(source_monitor, 1)
        except BaseException as error:
            failure(error)
        for error in source_errors:
            failure(RuntimeError(error))
        if player is not None:
            try:
                stop_source(player, event, args.out/'aplay.log')
            except BaseException as error:
                failure(error)
        stopping.set()
        try:
            join_started(reader, 3)
        except BaseException as error:
            failure(error)
        if port:
            try:
                port.close()
            except BaseException as error:
                failure(error)
        if reader and reader.is_alive():
            failure(RuntimeError('UART reader did not stop within three seconds'))
        for error in reader_errors:
            failure(RuntimeError(error))
        if (args.out/'uart.log').exists():
            try:
                result['analysis'], result['boot_selection'] = analyze_acquisition(args.out, reset_stamp)
            except BaseException as error:
                failure(error)
        result['completed'] = result['completed'] and not result['errors'] and result.get('analysis', {}).get('valid_sequence', False)
        (args.out/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        signal.signal(signal.SIGTERM, previous_sigterm)
    print(json.dumps(dict(out=str(args.out), completed=result['completed'], errors=result['errors'])))
    return 0 if result['completed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
