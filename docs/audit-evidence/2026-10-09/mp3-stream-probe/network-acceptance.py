#!/usr/bin/env python3
# ABOUTME: Verifies real MP3 network fault recovery using parent-owned UART collection.
# ABOUTME: Requires before-injection PCM, actual disconnect/reconnect and unchanged capture faults.
import argparse
import concurrent.futures
import importlib.util
import json
import os
from pathlib import Path
import re
import time

spec = importlib.util.spec_from_file_location('mp3_acceptance', Path(__file__).with_name('acceptance.py'))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
FAULTS = ['queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts', 'discontinuities']
STAMP = re.compile(r'\b[IEWD] \((\d+)\)')


def rows(path):
    data = path.read_bytes() if path.exists() else b''
    # An in-progress UART line is not evidence.
    return data.split(b'\n')[:-1]


def observations(path):
    result = []
    for index, raw in enumerate(rows(path)):
        text = raw.decode('utf-8', errors='replace')
        match = STAMP.search(text)
        if match:
            result.append(dict(line=index, seconds=int(match[1])/1000, text=text))
    return result


def wait_for(function, seconds, label):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = function()
        if value:
            return value
        time.sleep(0.1)
    raise TimeoutError(label)


def capture_pair(args, out, label, count):
    peers = []
    try:
        for _ in range(2):
            peers.append(helper.connect(args.host, args.port, '/stream.mp3'))
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            futures = [pool.submit(helper.receive, peer, count, admitted+count*.024+10)
                       for peer, admitted in peers]
            data = [future.result() for future in futures]
        return [helper.check(value, out, f'{label}-{i+1}') for i, value in enumerate(data)]
    finally:
        for peer, _ in peers:
            peer.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('host')
    parser.add_argument('--port', type=int, default=8000)
    parser.add_argument('--uart', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    args.out.mkdir(parents=True, exist_ok=True)
    result = dict(passed=False, host=args.host, uart=str(args.uart))
    try:
        initial = rows(args.uart)
        initial_count = len(initial)
        # Bind startup to a newly committed UART record, not helper launch time or stale file mtime.
        def ready():
            records = observations(args.uart)
            if any('marker=DISCONNECT_REQUEST' in row['text'] for row in records):
                raise ValueError('network injection already occurred before baseline')
            if records and records[-1]['line'] >= initial_count and records[-1]['seconds'] >= 10:
                if records[-1]['seconds'] > 22:
                    raise ValueError('baseline startup missed board 10..22 second window')
                return records[-1]
        start = wait_for(ready, 15, 'fresh boot-relative UART startup record missing')
        result['baseline_start'] = start
        wall_start = time.monotonic()
        result['baseline_streams'] = capture_pair(args, args.out, 'baseline', 210)
        elapsed = time.monotonic()-wall_start
        baseline_end_bound = start['seconds'] + elapsed
        result['baseline_end_boot_seconds_estimate'] = baseline_end_bound
        records = observations(args.uart)
        if baseline_end_bound >= 29 or any('marker=DISCONNECT_REQUEST' in row['text'] for row in records):
            raise ValueError('baseline pair did not complete before injection')
        def recovered():
            records = observations(args.uart)
            request = next((row for row in records if 'network-faults: marker=DISCONNECT_REQUEST' in row['text']), None)
            if not request:
                return None
            response = next((row for row in records if row['line'] > request['line']
                             and 'network-faults: marker=DISCONNECT_RESULT' in row['text']), None)
            if response and not re.search(r'\bresult=0\b', response['text']):
                raise ValueError('esp_wifi_disconnect did not return ESP_OK')
            disconnected = next((row for row in records if row['line'] > request['line']
                                 and 'wifi: disconnected' in row['text']), None)
            reconnected = next((row for row in records if disconnected and row['line'] > disconnected['line']
                                and 'wifi: reconnected' in row['text']), None)
            if response and reconnected and records[-1]['seconds'] >= 45:
                return dict(request=request, response=response, disconnected=disconnected, reconnected=reconnected)
        markers = wait_for(recovered, 65, 'real network injection/reconnect or post-45-second UART observation missing')
        result['network'] = markers
        result['recovered_streams'] = capture_pair(args, args.out, 'recovered', 417)
        final_line = len(rows(args.uart))
        wait_for(lambda: any(row['line'] >= final_line and 'audio: frames=' in row['text']
                             for row in observations(args.uart)), 15, 'post-stream capture telemetry missing')
        records = observations(args.uart)
        request_line = markers['request']['line']
        captures = []
        for row in records:
            if 'audio: frames=' in row['text'] and row['seconds'] >= 10:
                fields = {key: int(value) for key, value in re.findall(r'\b([a-z_]+)=(-?\d+)',row['text'])}
                if any(key not in fields for key in ['frames']+FAULTS):
                    raise ValueError('capture telemetry fields missing')
                captures.append(dict(**row, counters=fields))
        before = [row for row in captures if row['line'] < request_line]
        after = [row for row in captures if row['line'] > request_line]
        if not before or len(after) < 2:
            raise ValueError('pre-injection >=10s baseline or recovery capture observations missing')
        baseline = before[-1]
        for row in after:
            if any(row['counters'][key] != baseline['counters'][key] for key in FAULTS):
                raise ValueError('capture fault counter changed during network test')
            if row['counters']['frames'] <= baseline['counters']['frames']:
                raise ValueError('capture did not advance after injection')
        window = [row for row in records if row['line'] >= baseline['line']]
        raw_window = [line.decode('utf-8', errors='replace') for line in rows(args.uart)[baseline['line']:]]
        if any(any(term in line for term in ['requesting restart','rebooting','rst:0x']) for line in raw_window):
            raise ValueError('reboot or capture supervisor trip during network test')
        result['capture_baseline'] = baseline
        result['capture_after'] = after
        result['passed'] = True
    except Exception as error:
        result['error'] = f'{type(error).__name__}: {error}'
    (args.out/'result.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
