#!/usr/bin/env python3
# ABOUTME: Measures matched MP3/FLAC staged transport using bounded direct receivers and host TCP_INFO.
# ABOUTME: Separates actual connection overlap from recovery and decodes complete captured frames exactly.
import argparse
from bisect import bisect_right
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'scripts'))
from decoder_integrity import FrameHash, validate_decoder_log
from stream_format import validate_flac_header, validate_stream_decoder_log

spec = importlib.util.spec_from_file_location('tcp_probe', Path(__file__).resolve().parents[1] / 'mp3-stream-probe/tcp-stall-probe.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
RAW_LIMIT = 32 * 1024 * 1024  # 90 seconds of 48 kHz stereo FLAC16 can exceed the MP3 helper's 8 MiB.
ARRIVAL_LIMIT = 200000


def crc16_table():
    table = []
    for byte in range(256):
        crc = byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ (0x8005 if crc & 0x8000 else 0)) & 0xffff
        table.append(crc)
    return table


CRC16 = crc16_table()


def crc16(data):
    value = 0
    for byte in data:
        value = ((value << 8) & 0xffff) ^ CRC16[(value >> 8) ^ byte]
    return value


def validate_capture(data, format, out, label):
    raw = out / f'{label}-raw.{format}'
    raw.write_bytes(data)
    ends = []
    tail_note = 'timed capture may stop within a frame; excluded bytes are not validated'
    if format == 'mp3':
        ends = list(range(960, len(data) + 1, 960))
        if not ends:
            raise ValueError('no complete MP3 frame')
        # Retain the established fixed-header/reservoir and exact PCM checks.
        base.helper.check(memoryview(data)[:ends[-1]], out, f'{label}-mp3-check')
        samples_per_frame = 1152
    else:
        validate_flac_header(data[:42])
        if data[4] != 0x80 or data[8:12] != b'\x10\x00\x10\x00':
            raise ValueError('expected sole STREAMINFO and fixed 4096-sample blocks')
        probe = subprocess.run(['ffprobe', '-v', 'error', '-f', 'flac', '-show_packets',
                                '-show_entries', 'packet=pos,size', '-of', 'json', str(raw)],
                               capture_output=True, timeout=20)
        (out / f'{label}.ffprobe.log').write_bytes(probe.stderr)
        if len(probe.stdout) > 2 * 1024 * 1024 or len(probe.stderr) > 65536:
            raise ValueError('ffprobe evidence exceeded bounded size')
        packets = json.loads(probe.stdout).get('packets', [])
        cursor = 42
        for index, packet in enumerate(packets):
            start, size = int(packet['pos']), int(packet['size'])
            end = start + size
            if start != cursor or size <= 0 or end > len(data):
                raise ValueError('FLAC packet offsets do not cover a contiguous capture')
            frame = memoryview(data)[start:end]
            if size < 14 or bytes(frame[:4]) != b'\xff\xf8\xca\x18' or crc16(frame):
                if index == len(packets) - 1 and end == len(data):
                    # A CRC-invalid final packet can be truncated or corrupt. Do not claim which.
                    tail_note = 'final packet lacks a valid whole-frame CRC; truncation versus corruption is unresolved'
                    break
                raise ValueError(f'invalid interior FLAC frame at byte {start}')
            ends.append(end)
            cursor = end
        if not ends:
            raise ValueError('no complete CRC-valid FLAC frame')
        if probe.returncode and ends[-1] == len(data):
            raise ValueError(f'ffprobe failed despite complete packet coverage: {probe.returncode}')
        samples_per_frame = 4096
    prefix = out / f'{label}-whole-prefix.{format}'
    prefix.write_bytes(memoryview(data)[:ends[-1]])
    errors = out / f'{label}.ffmpeg.log'
    hashes = out / f'{label}.framehash'
    accounting = FrameHash(hashes)
    process = None
    try:
        with errors.open('wb') as log:
            process = subprocess.Popen(['ffmpeg', '-nostdin', '-nostats', '-v', 'level+info', '-xerror',
                                        '-err_detect', 'crccheck', '-f', format, '-i', str(prefix),
                                        '-c:a', 'pcm_s32le', '-f', 'framehash', '-flush_packets', '1',
                                        '-y', str(hashes)], stdout=subprocess.DEVNULL, stderr=log)
            deadline = time.monotonic() + 25
            while process.poll() is None:
                if errors.stat().st_size > 65536 or (hashes.exists() and hashes.stat().st_size > 2 * 1024 * 1024):
                    raise ValueError('decoder evidence exceeded bounded size')
                if time.monotonic() >= deadline:
                    raise TimeoutError('prefix decoder exceeded 25 seconds')
                time.sleep(.025)
            if errors.stat().st_size > 65536 or hashes.stat().st_size > 2 * 1024 * 1024:
                raise ValueError('decoder evidence exceeded bounded size')
            # FrameHash.update bounds a live backlog to 128 KiB. This completed local
            # artifact can be larger; reuse its record validator one bounded line at a time.
            with hashes.open('r') as records:
                while line := records.readline(16385):
                    if len(line) > 16384 or not line.endswith('\n'):
                        raise ValueError('oversized or incomplete decoded framehash record')
                    accounting._record(line.rstrip('\n'))
            if not accounting.frames:
                raise ValueError('no decoded framehash samples')
        validate_decoder_log(errors)
        validate_stream_decoder_log(errors.read_text(), f'/stream.{format}')
        if process.returncode:
            raise ValueError(f'prefix decoder exited {process.returncode}; see {errors}')
        summary = accounting.summary()
        expected = len(ends) * samples_per_frame
        if summary['decoded_audio_frames'] != expected:
            raise ValueError(f'decoded {summary["decoded_audio_frames"]}, expected {expected} sample frames')
        summary.update(complete_coded_frames=len(ends), expected_sample_frames=expected,
                       excluded_tail_bytes=len(data) - ends[-1], excluded_tail_note=tail_note,
                       whole_prefix=str(prefix), complete_frame_bytes=ends[-1] - (42 if format == 'flac' else 0))
        return summary, ends
    finally:
        if process is not None and process.poll() is None:
            process.kill()
            process.wait(timeout=3)
        accounting.close()


def phase_rates(jobs, origin, connect_start, admitted, ended, samples):
    second = next((job for job in jobs if job['label'] == 'listener-2'), None)
    stop_times = [value for job in jobs for value in (job.get('eof_monotonic'), job.get('error_monotonic')) if value is not None]
    joint_end = min([ended] + stop_times)
    windows = [('solo_before_second_connect', origin, connect_start),
               ('second_handshake', connect_start, admitted),
               ('both_connected', admitted, max(admitted, joint_end)),
               ('planned_postjoin_including_recovery', admitted, ended)]
    if second:
        second_end = second.get('eof_monotonic') or second.get('error_monotonic')
        if second_end is not None:
            windows.append(('after_second_end', second_end, ended))
    output = []
    for name, start, end in windows:
        if end <= start:
            continue
        for job in jobs:
            if job['admitted_monotonic'] > start:
                continue
            arrival_times = [row[0] for row in job['arrivals']]
            def count(stamp):
                index = bisect_right(arrival_times, stamp)
                app_bytes = job['arrivals'][index - 1][1] if index else 0
                return app_bytes, bisect_right(job['frame_ends'], app_bytes)
            before_bytes, before_frames = count(start)
            after_bytes, after_frames = count(end)
            frames = after_frames - before_frames
            output.append(dict(phase=name, label=job['label'], start_seconds=start-origin,
                               end_seconds=end-origin, wall_seconds=end-start,
                               received_byte_delta=after_bytes-before_bytes, completed_frame_delta=frames,
                               coded_sample_frames=frames*samples, audio_seconds_per_wall_second=frames*samples/48000/(end-start)))
    return output


def main():
    parser = argparse.ArgumentParser(description='Throwaway matched transport observer. Parent selects the active codec. One direct receiver, second starts at 30 s, finish at 90 s. No settings, UART, flashing, Play or audio output.')
    parser.add_argument('host', help='numeric IPv4 address; DNS is excluded to keep admission bounded')
    parser.add_argument('--format', required=True, choices=['mp3', 'flac'])
    parser.add_argument('--port', type=int, default=8000)
    parser.add_argument('--seconds', type=float, default=90, help='total after first HTTP admission, at most 90 (default: 90)')
    parser.add_argument('--join-after', type=float, default=30, help='second connect start after first HTTP admission (default: 30)')
    parser.add_argument('--out', type=Path, required=True, help='new private evidence directory; must not exist')
    args = parser.parse_args()
    try:
        ipaddress.IPv4Address(args.host)
    except ipaddress.AddressValueError:
        parser.error('host must be a numeric IPv4 address')
    if not 0 < args.join_after < args.seconds <= 90:
        parser.error('require 0 < join-after < seconds <= 90')
    os.umask(0o077)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(passed=False, format=args.format, path=f'/stream.{args.format}', requested_seconds=args.seconds,
                  join_after_seconds=args.join_after, origin_monotonic=None, events=[], connections=[],
                  limitations=['This is throwaway host transport evidence; no device settings or source state are changed or verified.',
                               'TCP_INFO describes receiving sockets; it cannot locate the failing hop or show sender retransmissions.',
                               'Complete frame ends use host recv completion timestamps; arrival within a chunk and server admission precede observation.',
                               'Phase counts measure received complete coded frames, not speaker playback or sample-exact analog source continuity.',
                               'Only whole prefixes are decoded; excluded final bytes remain unvalidated. FLAC continuity is decoder/frame-number continuity.',
                               'passed means two bounded captures without natural EOF/error and valid whole prefixes, not real-time delivery success.'])
    jobs, workers, peers = [], [], []
    lock = threading.RLock()
    stopping = threading.Event()
    observer = None
    origin = deadline = connect_start = admitted_second = None
    telemetry_bytes = 0

    def event(kind, **fields):
        stamp = time.monotonic()
        with lock:
            result['events'].append(dict(kind=kind, monotonic=stamp, seconds=None if origin is None else stamp-origin, **fields))

    def connect(label):
        nonlocal origin, deadline
        started = time.monotonic()
        connection = dict(label=label, connect_start_monotonic=started)
        result['connections'].append(connection)
        event('connect_start', label=label)
        peer = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        peers.append(peer)
        # Resolve/connect/header each stay bounded by one overall admission deadline.
        connect_deadline = min(started + 10, deadline) if deadline else started + 10
        try:
            peer.settimeout(max(.001, connect_deadline-time.monotonic()))
            peer.connect((args.host, args.port))
            peer.settimeout(max(.001, connect_deadline-time.monotonic()))
            peer.sendall(f'GET /stream.{args.format} HTTP/1.1\r\nHost: {args.host}\r\nConnection: close\r\n\r\n'.encode())
            header = bytearray()
            while not header.endswith(b'\r\n\r\n'):
                remaining = connect_deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('overall HTTP admission deadline exceeded')
                peer.settimeout(remaining)
                byte = peer.recv(1)
                if not byte:
                    raise ValueError('EOF in HTTP header')
                header += byte
                if len(header) > 8192:
                    raise ValueError('HTTP header exceeded 8192 bytes')
            text = header.decode('ascii')
            connection.update(response_header=text, response_status=text.split('\r\n', 1)[0])
            lines = text.lower().split('\r\n')
            mime = 'audio/mpeg' if args.format == 'mp3' else 'audio/flac'
            if not text.startswith('HTTP/1.1 200 OK\r\n') or f'content-type: {mime}' not in lines:
                raise ValueError(f'unexpected active-route response: {text}')
            if any(line.startswith('transfer-encoding:') for line in lines):
                raise ValueError('unexpected HTTP transfer encoding')
            admitted = time.monotonic()
            if origin is None:
                origin, deadline = admitted, admitted + args.seconds
                result['origin_monotonic'] = origin
            peer.settimeout(.25)
            job = dict(label=label, peer=peer, data=bytearray(), arrivals=[], frame_ends=[], app_bytes=0,
                       admitted_monotonic=admitted, local_port=peer.getsockname()[1],
                       receiver_buffer_bytes=peer.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF),
                       last_recv_monotonic=admitted, max_recv_gap=0, recv_calls=0,
                       eof_monotonic=None, error_monotonic=None, error=None)
            with lock:
                jobs.append(job)
            connection.update(admitted_monotonic=admitted, admitted_seconds=admitted-origin, local_port=job['local_port'])
            worker = threading.Thread(target=drain, args=(job,), daemon=True)
            workers.append(worker)
            worker.start()
            event('admitted', label=label, admitted_monotonic=admitted, local_port=job['local_port'])
            return admitted
        except BaseException as error:
            connection['error'] = f'{type(error).__name__}: {error}'
            connection['partial_response_header'] = bytes(locals().get('header', b'')).decode('ascii', errors='replace')
            event('admission_error', label=label, error=connection['error'])
            peer.close()
            raise

    def drain(job):
        try:
            while not stopping.is_set() and time.monotonic() < deadline:
                try:
                    chunk = job['peer'].recv(65536)
                except socket.timeout:
                    continue
                stamp = time.monotonic()
                if stopping.is_set() or stamp > deadline:
                    break
                with lock:
                    if not chunk:
                        job['eof_monotonic'] = stamp
                        event('eof', label=job['label'], observed_monotonic=stamp)
                        break
                    if len(job['data']) + len(chunk) > RAW_LIMIT or len(job['arrivals']) >= ARRIVAL_LIMIT:
                        raise ValueError('raw capture/arrival record budget exceeded')
                    gap = stamp-job['last_recv_monotonic']
                    if gap >= .5:
                        event('receive_gap', label=job['label'], gap_seconds=gap, app_bytes_before=job['app_bytes'])
                    job['max_recv_gap'] = max(job['max_recv_gap'], gap)
                    job['last_recv_monotonic'] = stamp
                    job['recv_calls'] += 1
                    job['data'] += chunk
                    job['app_bytes'] += len(chunk)
                    job['arrivals'].append((stamp, job['app_bytes']))
        except Exception as error:
            if not stopping.is_set():
                with lock:
                    job.update(error=f'{type(error).__name__}: {error}', error_monotonic=time.monotonic())
                    event('receive_error', label=job['label'], error=job['error'])
        finally:
            event('drain_stopped', label=job['label'])

    def sample(fields):
        nonlocal telemetry_bytes
        next_sample = origin
        try:
            with (args.out/'tcp-info.jsonl').open('w') as output:
                while not stopping.is_set() and time.monotonic() <= deadline:
                    stamp = time.monotonic()
                    values = []
                    with lock:
                        for job in jobs:
                            raw = job['peer'].getsockopt(socket.IPPROTO_TCP, socket.TCP_INFO, 512)
                            counters = {name: int.from_bytes(raw[offset:offset+size], os.sys.byteorder)
                                        if offset+size <= len(raw) else None for name, (offset, size) in fields.items()}
                            values.append(dict(label=job['label'], local_port=job['local_port'], tcp_info_length=len(raw),
                                               counters=counters, app_bytes=job['app_bytes'], recv_calls=job['recv_calls'],
                                               seconds_since_recv=stamp-job['last_recv_monotonic'], max_recv_gap=job['max_recv_gap'],
                                               eof=job['eof_monotonic'] is not None, error=job['error']))
                    line = json.dumps(dict(monotonic=stamp, seconds=stamp-origin, listeners=values), separators=(',', ':'))+'\n'
                    telemetry_bytes += len(line.encode())
                    if telemetry_bytes > base.TELEMETRY_LIMIT:
                        raise ValueError('TCP_INFO exceeded 4 MiB')
                    output.write(line)
                    output.flush()
                    next_sample += .1
                    stopping.wait(max(0, next_sample-time.monotonic()))
        except Exception as error:
            result['telemetry_error'] = f'{type(error).__name__}: {error}'
            event('telemetry_error', error=result['telemetry_error'])
            stopping.set()

    try:
        fields = base.layout()
        result['tcp_info_layout'] = {name: dict(offset=offset, size=size) for name, (offset, size) in fields.items()}
        connect('listener-1')
        observer = threading.Thread(target=sample, args=(fields,), daemon=True)
        observer.start()
        stopping.wait(max(0, origin+args.join_after-time.monotonic()))
        if not stopping.is_set():
            connect_start = time.monotonic()
            admitted_second = connect('listener-2')
            stopping.wait(max(0, deadline-time.monotonic()))
    except BaseException as error:
        result['error'] = f'{type(error).__name__}: {error}'
        event('probe_error', error=result['error'])
    finally:
        ended = min(time.monotonic(), deadline) if deadline else time.monotonic()
        stopping.set()
        # End observer before closing its sockets; shutdown interrupts any receiver wait.
        if observer:
            observer.join(2)
            if observer.is_alive():
                result['worker_error'] = 'TCP_INFO observer did not stop within 2 seconds'
        for peer in peers:
            try:
                peer.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            peer.close()
        for worker in workers:
            worker.join(2)
            if worker.is_alive():
                result['worker_error'] = 'receiver did not stop within 2 seconds'
        result.update(end_monotonic=ended, telemetry_bytes=telemetry_bytes, listeners=[])
        for job in jobs:
            label = job['label']
            try:
                summary, job['frame_ends'] = validate_capture(job['data'], args.format, args.out, label)
                job['prefix_decode'] = summary
            except Exception as error:
                job['decode_error'] = f'{type(error).__name__}: {error}'
                event('decode_error', label=label, error=job['decode_error'])
            arrival_path = args.out/f'{label}-arrivals.jsonl'
            with arrival_path.open('w') as output:
                for stamp, count in job['arrivals']:
                    output.write(json.dumps(dict(monotonic=stamp, seconds=stamp-origin, app_bytes=count))+'\n')
            state = {key: value for key, value in job.items() if key not in ('peer', 'data', 'arrivals', 'frame_ends')}
            state.update(arrivals=str(arrival_path), frame_ends=job['frame_ends'],
                         final_no_receive_seconds=ended-job['last_recv_monotonic'])
            result['listeners'].append(state)
        if origin is not None and admitted_second is not None:
            result['phase_rates'] = phase_rates(jobs, origin, connect_start, admitted_second, ended,
                                                1152 if args.format == 'mp3' else 4096)
        result['passed'] = len(jobs) == 2 and not any(result.get(key) for key in ('error', 'worker_error', 'telemetry_error')) and all(
            not row['eof_monotonic'] and not row['error'] and not row.get('decode_error') for row in result['listeners'])
        (args.out/'result.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(dict(passed=result['passed'], result=str(args.out/'result.json'),
                          phase_rates=result.get('phase_rates', []), error=result.get('error')), indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
