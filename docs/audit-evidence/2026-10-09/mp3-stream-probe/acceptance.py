#!/usr/bin/env python3
# ABOUTME: Runs bounded, direct MP3 hardware acceptance against an already flashed board.
# ABOUTME: Retains private raw captures and distinguishes encoded history from lossy PCM.
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import socket
import subprocess
import time

FRAME_BYTES = 960
FRAME_SAMPLES = 1152


def connect(host, port, path, small=False):
    peer = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    peer.settimeout(10)
    if small:
        peer.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 2048)
    try:
        peer.connect((host, port))
        peer.sendall(f'GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n'.encode())
        header = bytearray()
        header_deadline = time.monotonic() + 10
        # Read only the header: slow-reader receives no stream payload until its drain.
        while not header.endswith(b'\r\n\r\n'):
            remaining = header_deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError('HTTP header deadline exceeded')
            peer.settimeout(remaining)
            byte = peer.recv(1)
            if not byte:
                raise ValueError('EOF in HTTP header')
            header += byte
            if len(header) > 8192:
                raise ValueError('oversized HTTP header')
        text = header.decode('ascii')
        if not text.startswith('HTTP/1.1 200 OK\r\n') or 'Content-Type: audio/mpeg\r\n' not in text:
            raise ValueError(f'unexpected MP3 response: {text}')
        if 'transfer-encoding:' in text.lower():
            raise ValueError('unexpected HTTP transfer encoding')
        return peer, time.monotonic()
    except BaseException:
        peer.close()
        raise


def receive(peer, frames, deadline):
    data = bytearray()
    while len(data) < frames * FRAME_BYTES:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('capture deadline exceeded')
        peer.settimeout(min(10, remaining))
        chunk = peer.recv(min(65536, frames * FRAME_BYTES - len(data)))
        if not chunk:
            raise ValueError(f'premature EOF after {len(data)} bytes')
        data += chunk
    return bytes(data)


def check(data, out, label):
    if not data or len(data) % FRAME_BYTES:
        raise ValueError('incomplete MP3 frames')
    for offset in range(0, len(data), FRAME_BYTES):
        frame = data[offset:offset + FRAME_BYTES]
        header = int.from_bytes(frame[:4], 'big')
        # MPEG1, Layer III, 320 kb/s, 48 kHz, no padding, stereo/joint stereo.
        if (header >> 21 != 0x7ff or (header >> 19) & 3 != 3
                or (header >> 17) & 3 != 1 or (header >> 12) & 15 != 14
                or (header >> 10) & 3 != 1 or (header >> 9) & 1
                or (header >> 6) & 3 not in (0, 1)):
            raise ValueError(f'invalid fixed MP3 header at byte {offset}: {header:08x}')
        side = 4 if (header >> 16) & 1 else 6
        reservoir = (frame[side] << 1) | (frame[side + 1] >> 7)
        if reservoir:
            raise ValueError(f'MP3 reservoir dependency at byte {offset}: {reservoir}')
    raw = out / f'{label}.mp3'
    raw.write_bytes(data)
    hashes = out / f'{label}.framehash'
    result = subprocess.run(['ffmpeg', '-nostdin', '-nostats', '-v', 'level+warning', '-xerror',
                             '-f', 'mp3', '-i', str(raw), '-c:a', 'pcm_s32le', '-f', 'framehash',
                             '-y', str(hashes)], capture_output=True, text=True, timeout=20)
    (out / f'{label}.ffmpeg.log').write_text(result.stderr)
    if result.returncode:
        raise ValueError(f'{label} decode failed: {result.stderr}')
    for line in result.stderr.splitlines():
        if line.strip() and '[warning] Estimating duration from bitrate, this may be inaccurate' not in line:
            raise ValueError(f'{label}: unexpected decoder diagnostic: {line}')
    samples = sum(int(line.split(',')[4]) // 8 for line in hashes.read_text().splitlines()
                  if line and not line.startswith('#'))
    expected = len(data) // FRAME_BYTES * FRAME_SAMPLES
    if samples != expected:
        raise ValueError(f'{label}: decoded {samples}, expected {expected}')
    return dict(frames=len(data) // FRAME_BYTES, decoded_samples=samples, raw=str(raw),
                decoder_warnings=result.stderr.splitlines())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('host')
    parser.add_argument('--port', type=int, default=8000)
    parser.add_argument('--out', required=True)
    parser.add_argument('--single-frames', type=int, default=2500)
    args = parser.parse_args()
    os.umask(0o077)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    result = dict(passed=False, host=args.host, captures={}, history={})
    try:
        # Allow the already running encoder to retain High's eight seconds.
        time.sleep(9)
        peer, admitted = connect(args.host, args.port, '/stream.mp3')
        try:
            data = receive(peer, args.single_frames, admitted + args.single_frames * .024 + 10)
        finally:
            peer.close()
        result['captures']['single'] = check(data, out, 'single')
        time.sleep(3)
        for preset, expected in [('low', 42), ('med', 167), ('high', 334)]:
            # Dispatch both requests before waiting for either response head.
            # Header delivery time is not the server's cursor-admission time.
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as admission:
                live_future = admission.submit(connect, args.host, args.port, '/stream.mp3')
                buffered_future = admission.submit(connect, args.host, args.port, f'/stream.mp3?buffer={preset}')
                live, first = live_future.result()
                buffered, second = buffered_future.result()
            try:
                with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
                    a = pool.submit(receive, live, 500, first + 24)
                    b = pool.submit(receive, buffered, 500, second + 24)
                    live_bytes, buffered_bytes = a.result(), b.result()
            finally:
                live.close()
                if buffered:
                    buffered.close()
            result['captures'][f'{preset}-live'] = check(live_bytes, out, f'{preset}-live')
            result['captures'][f'{preset}-buffered'] = check(buffered_bytes, out, f'{preset}-buffered')
            needle = live_bytes[:10 * FRAME_BYTES]
            matches = [i for i in range(491)
                       if buffered_bytes[i*FRAME_BYTES:(i+10)*FRAME_BYTES] == needle]
            if len(matches) != 1 or abs(matches[0] - expected) > 2:
                raise ValueError(f'{preset}: encoded overlap offsets {matches}, expected {expected} +/-2')
            offset = matches[0]
            overlap = 500 - offset
            if live_bytes[:overlap * FRAME_BYTES] != buffered_bytes[offset * FRAME_BYTES:]:
                raise ValueError(f'{preset}: encoded content differs after initial unique overlap')
            result['history'][preset] = dict(overlapping_frames=overlap, expected_frames=expected, matched_frames=offset,
                                            admission_gap_seconds=second-first)
            time.sleep(3)
        slow, slow_at = connect(args.host, args.port, '/stream.mp3', small=True)
        fast = None
        try:
            fast, fast_at = connect(args.host, args.port, '/stream.mp3')
            with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
                future = pool.submit(receive, fast, 1667, fast_at + 55)
                time.sleep(max(0, slow_at + 25 - time.monotonic()))
                # A third listener must be admitted while fast still owns its slot.
                # Remote EOF/RST delivery is separate from board-side reclamation.
                if future.done():
                    raise ValueError('fast listener ended before reclamation probe')
                probe, probe_at = connect(args.host, args.port, '/stream.mp3')
                try:
                    probe_bytes = receive(probe, 10, probe_at + 5)
                finally:
                    probe.close()
                result['captures']['reclaimed-slot-probe'] = check(probe_bytes, out, 'reclaimed-slot-probe')
                result['slow_reader'] = dict(no_read_seconds=25, slot_reclaimed=True,
                                             probe_admitted_after_seconds=probe_at-slow_at)
                drained = bytearray()
                drain_deadline = time.monotonic() + 10
                outcome = 'timeout'
                while True:
                    remaining = drain_deadline - time.monotonic()
                    if remaining <= 0:
                        break
                    slow.settimeout(remaining)
                    try:
                        chunk = slow.recv(65536)
                    except socket.timeout:
                        break
                    except ConnectionResetError:
                        outcome = 'reset'
                        break
                    if not chunk:
                        outcome = 'eof'
                        break
                    drained += chunk
                    if len(drained) > 2 * 1024 * 1024:
                        outcome = 'byte-limit'
                        break
                (out / 'slow-drain.mp3').write_bytes(drained)
                result['slow_reader'].update(drained_bytes=len(drained), drain_outcome=outcome)
                fast_bytes = future.result()
            result['captures']['fast-isolation'] = check(fast_bytes, out, 'fast-isolation')
        finally:
            slow.close()
            if fast:
                fast.close()
        result['passed'] = True
    except Exception as error:
        result['error'] = f'{type(error).__name__}: {error}'
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
