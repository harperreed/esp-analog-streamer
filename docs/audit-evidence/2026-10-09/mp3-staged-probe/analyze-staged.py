#!/usr/bin/env python3
# ABOUTME: Correlates completed staged receiver telemetry with timestamped UART numeric records.
# ABOUTME: Keeps phase alignment approximate and publishes no peers, paths or raw messages.
import argparse
import bisect
import hashlib
import json
from pathlib import Path
import re
import tempfile

FAULTS = ['queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts',
          'supervisor_restarts', 'overflow_restarts', 'discontinuities', 'health_restart_requests']
PAIRS = re.compile(r'\b([a-z][a-z0-9_]*)=(-?\d+(?:\.\d+)?|true|false)\b')
DROP = re.compile(r'peer|host|path|(?:^|_)ip(?:_|$)|mac|uuid|identity', re.I)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read_json(path):
    return json.loads(path.read_text())

def uart_records(directory, origin, ports=None):
    uart = directory / 'uart.log'
    arrival = directory / 'uart-arrival.jsonl'
    chunks = [json.loads(line) for line in arrival.read_text().splitlines() if line]
    expected_offset = 0
    for item in chunks:
        if item['offset'] != expected_offset or item['bytes'] <= 0:
            raise ValueError('UART arrival metadata has a gap or invalid chunk')
        expected_offset += item['bytes']
    if expected_offset != uart.stat().st_size:
        raise ValueError('UART arrival byte total does not match log')
    ends = [item['offset'] + item['bytes'] for item in chunks]
    records = {'encoder': [], 'listener_departure': [], 'capture': []}
    byte_offset = 0
    uart_hash = digest(uart)
    for raw in uart.read_bytes().splitlines(keepends=True):
        byte_offset += len(raw)
        line = raw.decode(errors='replace')
        marker = next((item for item in ['stream-encode:', 'stream-listener:', 'audio: frames='] if item in line), None)
        if marker is None:
            continue
        stamp = re.search(r'\b[IWE] \((\d+)\)', line)
        if not stamp:
            continue
        chunk_index = bisect.bisect_left(ends, byte_offset)
        if chunk_index >= len(chunks):
            raise ValueError('UART record extends beyond arrival metadata')
        chunk = chunks[chunk_index]
        fields = {key: json.loads(value) for key, value in PAIRS.findall(line.split(marker.split(' frames=')[0], 1)[1]) if not DROP.search(key)}
        record = {'boot_ms': int(stamp[1]),
                  'host_arrival_seconds_from_receiver_origin': chunk['host_monotonic'] - origin,
                  'private_uart_sha256': uart_hash, **fields}
        kind = {'stream-encode:': 'encoder', 'stream-listener:': 'listener_departure', 'audio: frames=': 'capture'}[marker]
        if kind == 'listener_departure' and ports:
            peer = re.search(r'\bpeer=\S+:(\d+)\b', line)
            if peer:
                record['listener_index'] = ports.get(int(peer[1]))
        records[kind].append(record)
    if ports:
        for line in uart.read_text(errors='replace').splitlines():
            reason = re.search(r':(\d+) left after .*: fell a ring behind', line)
            if reason:
                index = ports.get(int(reason[1]))
                for row in records['listener_departure']:
                    if index is not None and row.get('listener_index') == index:
                        row['departure_reason'] = 'encoded_ring_overrun'
    return records

def baseline_delta(records, start, end, following_end=False):
    before = [item for item in records if item['host_arrival_seconds_from_receiver_origin'] <= start]
    after = [item for item in records if item['host_arrival_seconds_from_receiver_origin'] <= end]
    if not before or not after:
        return {'status': 'unavailable'}
    baseline, final = before[-1], after[-1]
    following = [item for item in records if item['host_arrival_seconds_from_receiver_origin'] >= end]
    if following_end and following:
        final = following[0]
    return {'status': 'observed',
            'final_selection': 'first_at_or_after_end' if following_end and following else 'last_at_or_before_end',
            'baseline_arrival_seconds': baseline['host_arrival_seconds_from_receiver_origin'],
            'final_arrival_seconds': final['host_arrival_seconds_from_receiver_origin'],
            'baseline_boot_ms': baseline['boot_ms'], 'final_boot_ms': final['boot_ms'],
            'baseline': {key: baseline.get(key) for key in FAULTS},
            'final': {key: final.get(key) for key in FAULTS},
            'deltas': {key: final[key] - baseline[key] for key in FAULTS if key in baseline and key in final}}

def classify_encoder(record, boundaries):
    end = record['host_arrival_seconds_from_receiver_origin']
    start = end - record.get('elapsed_us', 0) / 1_000_000
    record['approximate_window_start_arrival_seconds'] = start
    record['phase'] = next((name for name, low, high in boundaries if start >= low and end <= high), 'cross_boundary_or_outside_trial')
    return record

def numbers(value):
    if isinstance(value, (int, float, bool)) or value is None:
        return value
    if isinstance(value, dict):
        return {key: cleaned for key, child in value.items()
                if not DROP.search(key) and key not in ('local_port', 'decoder_warnings')
                and (cleaned := numbers(child)) is not None}
    if isinstance(value, list):
        return [cleaned for child in value if (cleaned := numbers(child)) is not None]
    return None

def sampled_rates(telemetry, low, high):
    rows = [row for row in telemetry if low <= row['seconds'] <= high]
    if len(rows) < 2:
        return {'status': 'unavailable'}
    first, final = rows[0], rows[-1]
    wall = final['seconds'] - first['seconds']
    result = {'status': 'sampled', 'requested_start_seconds': low, 'requested_end_seconds': high,
              'sample_start_seconds': first['seconds'], 'sample_end_seconds': final['seconds'],
              'start_boundary_delay_seconds': first['seconds'] - low,
              'end_boundary_gap_seconds': high - final['seconds'], 'wall_seconds': wall, 'listeners': []}
    for index, start in enumerate(first['listeners']):
        end = next((row for row in final['listeners'] if row['label'] == start['label']), None)
        if end:
            frames = end['app_bytes'] // 960 - start['app_bytes'] // 960
            coded_bytes = end['app_bytes'] - start['app_bytes']
            result['listeners'].append({'listener_index': index + 1,
                'coded_byte_delta': coded_bytes,
                'completed_frame_delta': frames, 'coded_sample_frame_equivalent': frames * 1152,
                'coded_audio_seconds_from_byte_delta': coded_bytes / 40_000,
                'audio_seconds_per_wall_second': coded_bytes / 40_000 / wall,
                'completed_frame_audio_seconds_per_wall_second': frames * .024 / wall})
    return result


def analyze(directory):
    receiver_path = directory / 'tcp/result.json'
    receiver = read_json(receiver_path)
    origin = receiver['origin_monotonic']
    before = receiver['before_second_connect']['seconds']
    after = receiver['after_second_admission']['seconds']
    end = receiver['capture_end']['seconds']
    listeners = receiver.get('listeners', [])
    eof_times = [item['eof_seconds'] for item in listeners if item.get('eof') and 'eof_seconds' in item]
    joint_end = min(eof_times) if eof_times else end
    boundaries = [('single', 0, before), ('joining', before, after), ('joint_active', after, joint_end)]
    if eof_times:
        boundaries.append(('survivor_catchup', joint_end, end))
    ports = {item['local_port']: index + 1 for index, item in enumerate(listeners)}
    records = uart_records(directory, origin, ports)
    previous = None
    for row in records['encoder']:
        classify_encoder(row, boundaries)
        if previous and row['boot_ms'] >= previous['boot_ms']:
            frames = row.get('encoded_frames', 0) - previous.get('encoded_frames', 0)
            resets = row.get('resets', 0) - previous.get('resets', 0)
            row['encoded_frame_counter_delta'] = frames
            row['reset_counter_delta'] = resets
            if row.get('elapsed_us', 0) > 0:
                row['encoded_audio_seconds_per_window_second'] = frames * .024 / (row['elapsed_us'] / 1_000_000)
        previous = row
    phases = []
    for item in receiver.get('phase_rates', []):
        label = item.get('label', '')
        phases.append({'phase': 'planned_overlap' if item['phase'] == 'overlap' else item['phase'],
                       'listener_index': int(label.split('-')[-1]),
                       **numbers(item)})
    summaries = []
    for index, item in enumerate(listeners):
        lifetime = item.get('eof_seconds', end) - item['admitted_seconds']
        coded_seconds = item['bytes'] // 960 * .024
        summaries.append({'listener_index': index + 1, **numbers(item),
                          'observed_lifetime_wall_seconds': lifetime,
                          'coded_audio_seconds_received': coded_seconds,
                          'audio_seconds_per_observed_lifetime_second': coded_seconds / lifetime,
                          'receive_error': bool(item.get('error')),
                          'decode_error': bool(item.get('decode_error')),
                          'decoder_warning_count': len(item.get('prefix_decode', {}).get('decoder_warnings', [])),
                          'duration_estimation_warning_count': sum('Estimating duration from bitrate' in warning for warning in item.get('prefix_decode', {}).get('decoder_warnings', []))})
    eof_times = [item['eof_seconds'] for item in listeners if item.get('eof') and 'eof_seconds' in item]
    joint_end = min(eof_times) if eof_times else end
    active = {'planned_overlap_seconds': end - after,
              'overlap_until_first_observed_eof_seconds': joint_end - after}
    telemetry_path = directory / 'tcp/tcp-info.jsonl'
    if telemetry_path.exists():
        if telemetry_path.stat().st_size > 4 * 1024 * 1024:
            raise ValueError('Telemetry exceeds the probe limit')
        telemetry = [json.loads(line) for line in telemetry_path.read_text().splitlines() if line]
        active['joint_active_sampled'] = sampled_rates(telemetry, after, joint_end)
        if eof_times:
            active['after_first_eof_sampled'] = sampled_rates(telemetry, joint_end, end)
    hashes = {'receiver_result_sha256': digest(receiver_path),
              'uart_sha256': digest(directory / 'uart.log'),
              'uart_arrival_sha256': digest(directory / 'uart-arrival.jsonl')}
    runner_path = directory / 'result.json'
    restoration = {'status': 'not_recorded'}
    if runner_path.exists():
        runner = read_json(runner_path)
        hashes['runner_result_sha256'] = digest(runner_path)
        restoration = {'status': 'restored' if runner.get('production_restored') and runner.get('config_restored') else 'not_established',
                       'production_restored': runner.get('production_restored'),
                       'config_restored': runner.get('config_restored'),
                       'runner_error': bool(runner.get('error'))}
        if re.fullmatch('[0-9a-f]{7,40}', runner.get('source_commit', '')):
            hashes['source_commit'] = runner['source_commit']
        if re.fullmatch('[0-9a-f]{64}', runner.get('input_fixture_sha256', '')):
            hashes['input_fixture_sha256'] = runner['input_fixture_sha256']
        for key in ['production_elf_sha256', 'mp3_elf_sha256']:
            if re.fullmatch('[0-9a-f]{64}', runner.get(key, '')):
                restoration[key] = runner[key]
    offsets = [item['host_arrival_seconds_from_receiver_origin'] - item['boot_ms'] / 1000
               for rows in records.values() for item in rows]
    return {'schema_version': 1, 'receiver_capture_passed': receiver.get('passed', False),
            'real_time_reliability_established': False,
            'phase_boundaries_seconds': {'single_start': 0, 'second_connect_start': before,
                                         'second_admission': after, 'capture_end': end},
            'phase_rates': phases, 'active_overlap': active, 'listeners': summaries, 'uart': records,
            'capture_fault_deltas': {name: baseline_delta(records['capture'], low, high) for name, low, high in boundaries},
            'capture_fault_delta_whole_trial': baseline_delta(records['capture'], 0, end, following_end=True),
            'restoration': restoration, 'private_evidence_hashes': hashes,
            'alignment': {'serial_read_timeout_seconds': .2,
                'arrival_minus_boot_seconds_minimum': min(offsets) if offsets else None,
                'arrival_minus_boot_seconds_maximum': max(offsets) if offsets else None,
                'observed_arrival_minus_boot_span_seconds': max(offsets) - min(offsets) if offsets else None,
                'limits': ['Host timestamps mark completed serial read chunks, not device event time.',
                           'The 200 ms serial read timeout, UART transmission and host/device scheduling add delay; 200 ms is not a strict bound.',
                           'Encoder phase tags use approximate arrival-window endpoints; crossing windows stay explicit. The measured arrival-minus-boot span documents observed alignment variation, not a strict clock-error bound.',
                           'Connection start and HTTP admission bracket the listener transition; admission is after server cursor selection.',
                           'Whole-trial fault deltas prefer the first capture record after capture end; this also includes the stated post-trial sampling delay.',
                           'Task runtime includes charged interrupts; write wall time includes scheduling and TCP waits. Correlation does not locate a radio/TCP root cause.',
                           'Joint-active and survivor rates use the first/last available 100 ms telemetry samples inside each requested interval; actual sample endpoints and gaps are retained.',
                           'Sampled interval delivery rates use CBR bytes / 40000 bytes per audio second; complete-frame boundary counts and their separate rates are also retained.',
                           'Coded phase sample equivalents do not prove decoded source continuity or receiver playout.']}}


def self_test():
    assert bisect.bisect_left([10, 20], 10) == 0
    assert classify_encoder({'host_arrival_seconds_from_receiver_origin': 31, 'elapsed_us': 10_000_000}, [('solo', 0, 30), ('overlap', 30, 90)])['phase'] == 'cross_boundary_or_outside_trial'
    rows = [{'host_arrival_seconds_from_receiver_origin': -1, 'boot_ms': 100, 'restarts': 2},
            {'host_arrival_seconds_from_receiver_origin': 20, 'boot_ms': 21100, 'restarts': 3}]
    assert baseline_delta(rows, 0, 30)['deltas']['restarts'] == 1
    with tempfile.TemporaryDirectory() as scratch:
        directory = Path(scratch)
        raw = b'I (100) stream: stream-listener: peer=example.invalid:1234 elapsed_us=42 frames_sent=3\n'
        (directory / 'uart.log').write_bytes(raw)
        (directory / 'uart-arrival.jsonl').write_text(json.dumps({'host_monotonic': 1001, 'offset': 0, 'bytes': len(raw)}) + '\n')
        parsed = uart_records(directory, 1000, {1234: 1})['listener_departure'][0]
        assert parsed['listener_index'] == 1 and parsed['host_arrival_seconds_from_receiver_origin'] == 1
        assert 'peer' not in parsed
        assert 'example.invalid' not in json.dumps(parsed)
    print('staged analysis self-tests passed')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__ or 'Analyze completed staged receiver/UART evidence; no hardware access.')
    parser.add_argument('--trial', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if args.trial is None or args.output is None:
            parser.error('--trial and --output are required')
        output = analyze(args.trial)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(output, indent=2) + '\n')
        print('Sanitized staged numeric analysis written; alignment limits retained.')
