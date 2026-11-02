#!/usr/bin/env python3
# ABOUTME: Acquires bounded direct MP3 preset source-history receipts without playback writes.
# ABOUTME: Retains raw HTTP, UART and independent decoded sample hashes in a private run directory.
import argparse
import hashlib
import http.client
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts'))
from decoder_integrity import FrameHash
from device_metrics import validate as validate_metrics
from sonos_http_check import require, verify_attestation

spec = importlib.util.spec_from_file_location('preset_control_helpers', ROOT / 'docs/audit-evidence/2026-10-13/control-check.py')
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)
TARGETS = {'low': 42, 'med': 167, 'high': 334}
FRAMES = 64


def snapshot_check(value, image, boot, settings=None, network=None, idle=False):
    verify_attestation(value, image, boot)
    current_settings, current_network = value['settings'], value['network']
    if current_settings is not None:
        require(current_settings['active_format'] == current_settings['next_play_format'] == 'mp3'
                and current_settings['next_play_buffer'] == 'med' and current_settings['buffer_available'] is True
                and current_settings['autoplay'] is False and current_settings['recognition_enabled'] is False,
                'requires MP3/Medium, buffers, autoplay and recognition off')
        require(settings is None or current_settings == settings, 'capture/encoder settings changed')
    if current_network is not None:
        require(current_network['connected'] is True and (network is None or current_network == network),
                'network lease changed')
    slots = value['listeners']
    require(len(slots) == 2 and all(row['state'] != 'unavailable' for row in slots)
            and sum(row['state'] != 'idle' for row in slots) <= (0 if idle else 1), 'unexpected listener slots')
    for family in ('listener_loss', 'encoder_loss'):
        loss = value[family]
        require(loss is None or (loss['dropped'] == 0 and loss['loss_occurred'] is False), 'lost ' + family)
    progress = value['media_progress']
    require(progress is None or progress['health'] == 'healthy', 'encoder health changed')
    return current_settings is not None and current_network is not None


def checked_uart(raw, final=False):
    control.markers(raw)  # Exact owned reset, malformed reset rejection and UART size cap.
    text = raw[:raw.rfind(b'\n') + 1].decode('utf-8', errors='replace')
    require(not any(value in text for value in ('Guru Meditation', 'panicked at', 'Task watchdog got triggered')),
            'panic/watchdog in UART')
    control.listener_exports(text)
    admissions(text)
    boundaries, windows = [], []
    for line in text.splitlines():
        if 'encoder-boundary:' in line:
            require(number(line, 'resets') == number(line, 'raw_overruns') == 0, 'encoder reset/raw loss')
            reason = re.findall(r'\breason=(\S+)', line)
            fmt = re.findall(r'\bformat=(\S+)', line)
            require(len(reason) == len(fmt) == 1, 'malformed encoder boundary')
            boundaries.append((reason[0], fmt[0], number(line, 'telemetry_epoch')))
            require(boundaries == [('Startup', 'Flac', 0)][:len(boundaries)]
                    or boundaries == [('Startup', 'Flac', 0), ('Switched', 'Mp3', 1)],
                    'unexpected encoder boundary or active reset')
        if 'stream-encode:' in line:
            require(number(line, 'resets') == number(line, 'raw_read_overruns') == 0, 'encoder reset/raw loss')
            number(line, 'encoded_frames')
            require(number(line, 'telemetry_epoch') == 1, 'unexpected active encoder epoch')
            windows.append(line)
        require(not any(value in line for value in ('encoder fell behind capture', 'encoder failed; output remains invalid')),
                'encoder failure/loss')
        if 'encoder-telemetry:' in line:
            require(number(line, 'dropped') == 0 and re.search(r'\bloss_occurred=false(?:\s|$)', line),
                    'lost encoder telemetry')
    if final:
        require(boundaries == [('Startup', 'Flac', 0), ('Switched', 'Mp3', 1)] and windows,
                'incomplete startup/encoder evidence')
        control.capture_gate(text)
    return text


def read_response(sock, wire, cap):
    class RecordingFile:
        def __init__(self, handle):
            self.handle = handle

        def retain(self, data):
            require(len(wire) + len(data) <= cap, 'HTTP response exceeds retained byte cap')
            wire.extend(data)
            return data

        def read(self, size=-1):
            return self.retain(self.handle.read(min(cap + 1, size) if size >= 0 else cap + 1))

        def readline(self, size=-1):
            return self.retain(self.handle.readline(min(cap + 1, size) if size >= 0 else cap + 1))

        def close(self):
            self.handle.close()

        def flush(self):
            self.handle.flush()

    class WireSocket:
        def makefile(self, mode):
            return RecordingFile(sock.makefile(mode))

    with http.client.HTTPResponse(WireSocket()) as response:
        response.begin()
        require(response.status == 200, 'non-200 HTTP response (redirects disabled)')
        body = response.read(cap + 1)
        require(len(body) <= cap, 'HTTP body exceeds cap')
        # HTTPResponse.read(amount) permits EOF before Content-Length; explicitly reject it.
        require(response.length in (None, 0), 'incomplete HTTP response')
        return body


class PendingEvidence(ValueError):
    """Complete receipt has not reached the growing UART file yet."""


def number(line, name):
    values = re.findall(r'\b' + re.escape(name) + r'=(\S+)', line)
    require(len(values) == 1 and re.fullmatch('[0-9]+', values[0]), 'malformed numeric ' + name)
    value = int(values[0])
    require(value <= 2**64 - 1, 'numeric field out of range: ' + name)
    return value


def one(lines, label):
    require(len(lines) <= 1, 'duplicate ' + label)
    if not lines:
        raise PendingEvidence('missing ' + label)
    return lines[0]


def admissions(text):
    rows = []
    pattern = (r'buffer-admission: peer=([0-9.]+:[0-9]+) format=(Mp3|Flac) preset=(low|med|high) '
               r'event_us=([0-9]+) head_generation=([0-9]+) head_sequence=([0-9]+) '
               r'cursor=Cursor \{ sequence: ([0-9]+), generation: ([0-9]+), offset: ([0-9]+) \}\r?$')
    for line in text.splitlines():
        if 'buffer-admission:' not in line:
            continue
        match = re.search(pattern, line)
        require(match is not None, 'malformed buffer-admission')
        peer, fmt, preset, *values = match.groups()
        rows.append(dict(peer=peer, format=fmt, preset=preset, **dict(zip(
            ('event_us', 'head_generation', 'head_sequence', 'cursor_sequence', 'cursor_generation', 'offset'),
            map(int, values)))))
    return rows


def validate_owner(text, peer, preset, received_frames):
    exports = control.listener_exports(text)  # Loss and malformed exports are terminal, even while waiting.
    admission = one([row for row in admissions(text) if row['peer'] == peer], 'peer admission')
    require(admission['format'] == 'Mp3' and admission['preset'] == preset, 'actual preset/format differs')
    require(admission['head_generation'] == admission['cursor_generation'] and admission['offset'] == 0,
            'admission generation/offset differs')
    retained = admission['head_sequence'] - admission['cursor_sequence']
    require(retained == TARGETS[preset], 'actual selected history differs from preset')
    token = re.escape(peer)
    join = one(re.findall(r'\bstream: ' + token + r' joined [^\n]*', text), 'peer join')
    require(re.search(r'joined /stream\.mp3 \(1 of 2\)', join), 'unexpected format or listener count')
    require(number(join, 'sonos_intent') == 0, 'debug listener acquired Sonos intent')
    owner = number(join, 'owner_id')
    require(owner > 0, 'zero listener owner')
    close = one(re.findall(r'\bstream: ' + token + r' socket closed[^\n]*', text), 'checked close')
    require(number(close, 'owner_id') == owner and number(close, 'sonos_intent') == 0,
            'checked close belongs to another owner')
    # The close receipt records occupancy before SlotGuard releases this owner's slot.
    require(re.search(r'releasing slot \(1 held\)', close), 'unexpected pre-release listener count')
    delivery = one(re.findall(r'\bstream-listener: peer=' + token + r'\b[^\n]*', text), 'positive delivery')
    sent, byte_count = number(delivery, 'frames_sent'), number(delivery, 'bytes_sent')
    require(sent >= received_frames > 0 and byte_count == sent * 960, 'incomplete delivery accounting')
    media = one(re.findall(r'\bstream-media: peer=' + token + r'\b[^\n]*', text), 'stream media')
    require(re.findall(r'\bvalid=(\S+)', media) == ['true'], 'invalid/contradictory stream media')
    first, last = [number(media, name + '_encoded_sequence') for name in ('first', 'last')]
    require(first == admission['cursor_sequence'] + 1 and last - first + 1 == sent,
            'first sequence differs from selected cursor or delivered range has gaps')
    require(number(media, 'first_encoded_generation') == number(media, 'last_encoded_generation')
            == admission['head_generation'], 'encoded generation changed')
    raw_generation = number(media, 'first_raw_generation')
    require(raw_generation == number(media, 'last_raw_generation'), 'raw generation changed')
    require(number(media, 'source_end') - number(media, 'source_start') == sent * 1152,
            'source range differs from delivered frames')
    joined, delivered, closed = number(join, 'event_us'), number(delivery, 'event_us'), number(close, 'event_us')
    require(joined <= admission['event_us'] <= delivered <= closed and number(media, 'event_us') == delivered,
            'owner event order differs')
    if not control.close_batch_exported(exports, closed):
        raise PendingEvidence('checked close lacks loss-free covering export')
    return dict(admission, owner_id=owner, retained_frames=retained, history_seconds=retained * 1152 / 48000,
                first_encoded_sequence=first, last_encoded_sequence=last, raw_generation=raw_generation,
                frames_sent=sent, bytes_sent=byte_count, close_us=closed, received_prefix_frames=received_frames)


def validate_mp3(data):
    require(data and len(data) % 960 == 0, 'incomplete MP3 frames')
    for offset in range(0, len(data), 960):
        frame = data[offset:offset + 960]
        header = int.from_bytes(frame[:4], 'big')
        require(header >> 21 == 0x7ff and (header >> 19) & 3 == 3 and (header >> 17) & 3 == 1
                and (header >> 12) & 15 == 14 and (header >> 10) & 3 == 1 and not (header >> 9) & 1
                and (header >> 6) & 3 in (0, 1) and header & 3 != 2,
                'invalid 320 kbps 48 kHz stereo MP3 header at byte ' + str(offset))
        side = 4 if (header >> 16) & 1 else 6
        require(((frame[side] << 1) | (frame[side + 1] >> 7)) == 0,
                'MP3 reservoir dependency at byte ' + str(offset))
    return len(data) // 960


def decode_prefix(data, out, label, timeout):
    frames = validate_mp3(data)
    raw, hashes, log = [out / (label + suffix) for suffix in ('.mp3', '.framehash', '.ffmpeg.log')]
    raw.write_bytes(data)
    command = ['ffmpeg', '-nostdin', '-nostats', '-v', 'level+warning', '-xerror', '-f', 'mp3',
               '-i', str(raw), '-c:a', 'pcm_s32le', '-f', 'framehash', str(hashes)]
    with log.open('wb') as errors:
        result = subprocess.run(command, stdout=subprocess.DEVNULL, stderr=errors, timeout=timeout)
    require(result.returncode == 0, 'independent MP3 decoder failed; see ' + str(log))
    for line in log.read_text().splitlines():
        require(not line.strip() or '[warning] Estimating duration from bitrate, this may be inaccurate' in line,
                'unexpected decoder diagnostic; see ' + str(log))
    accounting = FrameHash(hashes)
    try:
        accounting.update(final=True)
        require(accounting.frames == frames * 1152, 'decoded actual sample count differs')
        return dict(accounting.summary(), mp3_frames=frames, command=command,
                    mp3_sha256=hashlib.sha256(data).hexdigest(), framehash_sha256=hashlib.sha256(hashes.read_bytes()).hexdigest())
    finally:
        accounting.close()


class PresetTrial:
    def __init__(self, args):
        self.args = args
        args.out.mkdir(parents=True, exist_ok=False)
        self.deadline = time.monotonic() + 120
        self.image = hashlib.sha256(args.image.read_bytes()).hexdigest()
        self.settings = self.network = self.config = self.encoded_identity = None
        self.last_metrics = 0
        self.sequence = 0
        self.sockets = []
        self.requests = []
        self.result = dict(passed=False, cleanup_errors=[], captures=[], image_sha256=self.image,
                           boot_token=args.boot_token, limitations=[
                               'Quiet ADC source history only; no analog calibration or receiver playout protection.',
                               'Three sequential debug listeners; no Sonos playback or settings writes.',
                               '64-frame decoded prefixes do not establish long-term delivery or network recovery.',
                               'ELF hash and boot token are device reports, not secure attestation.'])
        self.source = None

    def event(self, kind, **fields):
        self.sequence += 1
        with (self.args.out / 'events.jsonl').open('a') as handle:
            handle.write(json.dumps(dict(sequence=self.sequence, monotonic=time.monotonic(), kind=kind, **fields)) + '\n')
        return self.sequence

    def remaining(self, cap):
        value = self.deadline - time.monotonic()
        if value <= 0:
            raise TimeoutError('bounded acquisition/cleanup deadline exhausted')
        return min(cap, value)

    def route(self):
        command = ['ip', '-j', 'route', 'get', self.args.host]
        routes = json.loads(subprocess.check_output(command, text=True, timeout=self.remaining(3)))
        require(len(routes) == 1 and routes[0]['dev'] == self.args.interface, 'actual route interface differs')
        source = str(ipaddress.IPv4Address(routes[0]['prefsrc']))
        require(self.source is None or source == self.source, 'actual route source changed')
        self.source = source
        self.event('route', command=command, routes=routes, interface=self.args.interface, source=source)

    def connect(self, port, cap):
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_BINDTODEVICE, self.args.interface.encode() + b'\0')
            sock.bind((self.source, 0))
            sock.settimeout(self.remaining(cap))
            sock.connect((self.args.host, port))
            require(sock.getsockname()[0] == self.source, 'actual socket source differs from route')
            self.sockets.append(sock)
            return sock
        except BaseException:
            sock.close()
            raise

    def timer(self, sock, cap):
        def expire():
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            sock.close()
        timer = threading.Timer(self.remaining(cap), expire)
        timer.daemon = True
        timer.start()
        return timer

    def close(self, sock, timer):
        timer.cancel()
        timer.join()
        sock.close()
        self.sockets.remove(sock)

    def api(self, path):
        require(path in ('/api/metrics', '/api/sonos', '/api/state'), 'read-only API path required')
        sequence = self.event('http-start', path=path)
        sock = self.connect(80, 2)
        timer = self.timer(sock, 2)
        wire = bytearray()
        try:
            sock.sendall(f'GET {path} HTTP/1.1\r\nHost: {self.args.host}\r\nConnection: close\r\n\r\n'.encode())
            return json.loads(read_response(sock, wire, 128 * 1024))
        finally:
            (self.args.out / f'{sequence:04d}-http.response').write_bytes(wire)
            self.close(sock, timer)

    def uart(self, final=False):
        with self.args.uart.open('rb') as handle:
            raw = handle.read(2 * 1024 * 1024 + 1)
        (self.args.out / 'uart-observed.log').write_bytes(raw)
        return checked_uart(raw, final)

    def attest(self, idle=False):
        outer_deadline = self.deadline
        try:
            while True:
                delay = max(0, self.last_metrics + .5 - time.monotonic())
                require(delay < self.remaining(120), 'metrics pacing exceeds deadline')
                time.sleep(delay)
                self.last_metrics = time.monotonic()
                value = validate_metrics(self.api('/api/metrics'))
                available = snapshot_check(value, self.image, self.args.boot_token, self.settings, self.network, idle)
                self.uart()
                progress = value['media_progress']
                if progress is not None and progress['encoded'] is not None:
                    encoded = progress['encoded']
                    identity = (encoded['generation'], encoded['raw_generation'])
                    require(self.encoded_identity is None or self.encoded_identity == identity,
                            'active encoded/raw generation changed')
                    self.encoded_identity = identity
                for family in ('capture', 'encoder'):
                    observation = value[family]
                    if observation is None:
                        self.event('metrics-observation-gap', family=family, served_device_us=value['served_device_us'])
                        continue
                    stamp = (observation['capture_end_us'] if family == 'capture' else
                             observation['window_end_us'] if observation['kind'] == 'window' else observation['event_us'])
                    require(0 <= value['served_device_us'] - stamp <= 15_000_000, 'stale ' + family + ' observation')
                if available:
                    if self.settings is None:
                        self.settings, self.network = value['settings'], value['network']
                    self.event('attestation', served_device_us=value['served_device_us'], idle=idle)
                    for family in ('listener_loss', 'encoder_loss'):
                        if value[family] is None:
                            self.event('metrics-observation-gap', family=family, served_device_us=value['served_device_us'])
                    return value
                self.event('metadata-gap', missing=[key for key in ('network', 'settings') if value[key] is None],
                           served_device_us=value['served_device_us'])
                self.deadline = min(self.deadline, time.monotonic() + 2)
        finally:
            self.deadline = outer_deadline

    def readback(self):
        value = self.attest(idle=True)
        status = self.api('/api/sonos')
        require(status['state'] == 'stopped' and status['active_format'] == 'mp3'
                and status['buffer_available'] is True, 'requires stopped MP3 with buffers')
        config = status['config']
        require(config['output_format'] == 'mp3' and config['buffer_preset'] == 'med'
                and config['autoplay'] is False, 'saved playback configuration differs')
        require(self.config is None or config == self.config, 'playback configuration changed')
        self.config = config
        require(self.api('/api/state')['shazam_enabled'] is False, 'recognition changed')
        return value

    def capture(self, preset):
        self.route()
        before = self.attest(idle=True)
        sock = self.connect(8000, 2)
        timer = self.timer(sock, 8)
        peer = ':'.join(map(str, sock.getsockname()))
        require(peer not in [row['peer'] for row in self.requests], 'local port reused in trial')
        request = dict(peer=peer, preset=preset, received_frames=FRAMES)
        self.requests.append(request)
        header, data = bytearray(), bytearray()
        self.event('stream-start', **request, served_device_us=before['served_device_us'])
        try:
            sock.sendall(f'GET /stream.mp3?buffer={preset} HTTP/1.1\r\nHost: {self.args.host}:8000\r\nConnection: close\r\n\r\n'.encode())
            while not header.endswith(b'\r\n\r\n'):
                require(len(header) < 8192, 'stream HTTP header exceeds cap')
                byte = sock.recv(1)
                require(byte, 'EOF in stream HTTP head')
                header.extend(byte)
            lines = header.decode('ascii').split('\r\n')
            require(lines[0] == 'HTTP/1.1 200 OK', 'stream HTTP status differs')
            fields = [line.lower() for line in lines[1:] if line]
            require(fields.count('content-type: audio/mpeg') == 1 and
                    not any(line.startswith(('transfer-encoding:', 'content-encoding:', 'location:')) for line in fields),
                    'unexpected stream response encoding/type')
            while len(data) < FRAMES * 960:
                chunk = sock.recv(FRAMES * 960 - len(data))
                require(chunk, 'premature EOF in MP3 prefix')
                data.extend(chunk)
        finally:
            (self.args.out / (preset + '-stream.http')).write_bytes(header + data)
            (self.args.out / (preset + '-received.mp3')).write_bytes(data)
            self.close(sock, timer)
            self.event('stream-owned-socket-closed', peer=peer, bytes_received=len(data))
        # Drain only missing complete receipts; malformed/lost evidence is terminal.
        until = time.monotonic() + self.remaining(3)
        while True:
            try:
                owner = validate_owner(self.uart(), peer, preset, FRAMES)
                break
            except PendingEvidence:
                if time.monotonic() >= until:
                    raise
                time.sleep(min(.1, until - time.monotonic()))
        require(self.encoded_identity is None or
                (owner['head_generation'], owner['raw_generation']) == self.encoded_identity,
                'admission/media generation differs from attested active generation')
        self.attest(idle=True)
        decoded = decode_prefix(bytes(data), self.args.out, preset, self.remaining(8))
        self.result['captures'].append(dict(preset=preset, owner=owner, decoded=decoded))

    def run(self):
        self.route()
        self.readback()
        while True:
            value = self.attest(idle=True)
            progress = value['media_progress']
            encoded = progress['encoded'] if progress is not None else None
            if encoded is not None and encoded['sequence'] >= 334:
                break
            time.sleep(min(.5, self.remaining(.5)))
        for preset in TARGETS:
            self.capture(preset)
        # At least two real capture windows are required, even on an unusually quick run.
        while sum('audio: frames=' in line for line in self.uart().splitlines()) < 2:
            self.attest(idle=True)
        self.result['capture_gate'] = control.capture_gate(self.uart(final=True))
        require(len(admissions(self.uart())) == 3, 'unexpected extra preset admission')
        self.readback()
        self.route()

    def cleanup(self):
        self.deadline = time.monotonic() + 15
        for sock in self.sockets[:]:
            sock.close()
            self.sockets.remove(sock)
        try:
            # No Play was issued, so cleanup grants no authority to send Stop or change settings.
            self.readback()
            text = self.uart(final=self.result.get('acquisition_passed', False))
            for row in self.requests:
                validate_owner(text, row['peer'], row['preset'], row['received_frames'])
        except Exception as error:
            self.result['cleanup_errors'].append(repr(error))


def main():
    parser = argparse.ArgumentParser(description='Bounded read-only MP3 preset gate; parent owns boot/UART. No Play or settings writes.')
    parser.add_argument('host', type=lambda value: str(ipaddress.IPv4Address(value)))
    parser.add_argument('--image', type=Path, required=True, help='exact flashed diagnostic ELF')
    parser.add_argument('--boot-token', required=True, help='expected 32 lowercase hex boot token')
    parser.add_argument('--interface', required=True, help='actual route interface, e.g. wlp6s0')
    parser.add_argument('--uart', type=Path, required=True, help='growing parent-owned complete boot log')
    parser.add_argument('--out', type=Path, required=True, help='new private run directory; must not exist')
    args = parser.parse_args()
    require(re.fullmatch('[0-9a-f]{32}', args.boot_token), 'invalid expected boot token')
    require(args.image.is_file() and args.uart.is_file(), 'image/UART file unavailable')
    os.umask(0o077)
    trial = PresetTrial(args)
    def interrupted(signum, frame):
        raise InterruptedError('signal ' + str(signum))
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, interrupted)
    try:
        trial.run()
        trial.result['acquisition_passed'] = True
    except Exception as error:
        trial.result['error'] = repr(error)
        trial.event('failure', error=repr(error))
    finally:
        trial.cleanup()
        trial.result['passed'] = trial.result.get('acquisition_passed', False) and not trial.result['cleanup_errors']
        trial.result['driver_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        (args.out / 'result.json').write_text(json.dumps(trial.result, indent=2) + '\n')
    print(json.dumps(dict(passed=trial.result['passed'], result=str(args.out / 'result.json'))))
    return 0 if trial.result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
