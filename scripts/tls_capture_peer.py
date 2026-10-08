# ABOUTME: Provides a bounded real TLS/WebSocket peer for receive-only capture diagnostics.
# ABOUTME: Sends a protocol baseline and reports counts and timing without logging client data.

import argparse
import base64
import binascii
import hashlib
import ipaddress
import json
import math
from pathlib import Path
import socket
import ssl
import sys
import time

PROTOCOL = 'v1.api.smartspeaker.audio'
BASELINE = b'[{"namespace":"fixture:1"},{}]'


class Rejected(Exception):
    pass


def integer(value, maximum):
    if not value.isascii() or not value.isdecimal() or len(value) > 5:
        raise argparse.ArgumentTypeError('expected an ASCII positive integer')
    result = int(value)
    if not 1 <= result <= maximum:
        raise argparse.ArgumentTypeError(f'expected 1..{maximum}')
    return result


def seconds(value):
    try:
        result = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError('expected positive finite seconds') from error
    if not math.isfinite(result) or not 0 < result <= 600:
        raise argparse.ArgumentTypeError('expected seconds greater than zero and at most 600')
    return result


def address(value):
    try:
        return str(ipaddress.ip_address(value))
    except ValueError as error:
        raise argparse.ArgumentTypeError('bind must be an explicit IPv4 or IPv6 address') from error


def arguments():
    parser = argparse.ArgumentParser(description='Bounded receive-only TLS WebSocket diagnostic. JSONL counts/timing; no client headers are logged.')
    parser.add_argument('--bind', required=True, type=address, help='explicit local listening IP; tests use 127.0.0.1')
    parser.add_argument('--port', type=lambda value: integer(value, 65535), default=9443, help='listening TCP port (default: 9443)')
    parser.add_argument('--cert', required=True, type=Path, help='PEM certificate chain with the client-trusted SAN')
    parser.add_argument('--key', required=True, type=Path, help='PEM private key matching --cert')
    parser.add_argument('--connections', type=lambda value: integer(value, 100), default=3, help='required complete TLS/WebSocket connections (default: 3)')
    parser.add_argument('--handshake-timeout', type=seconds, default=5.0, help='combined TLS and upgrade deadline in seconds (default: 5)')
    parser.add_argument('--connection-timeout', type=seconds, default=10.0, help='upgraded connection deadline in seconds (default: 10)')
    parser.add_argument('--run-timeout', type=seconds, default=90.0, help='absolute server lifetime in seconds (default: 90, maximum: 600)')
    return parser.parse_args()


def record(event, **fields):
    print(json.dumps(dict(event=event, **fields), sort_keys=True), flush=True)


def remaining(deadline):
    duration = deadline - time.monotonic()
    if duration <= 0:
        raise TimeoutError('deadline')
    return duration


def read_exact(stream, size, deadline, allow_eof=False):
    result = bytearray()
    while len(result) < size:
        try:
            stream.settimeout(remaining(deadline))
            chunk = stream.recv(size - len(result))
        except TimeoutError as error:
            if not allow_eof or result:
                raise Rejected('partial_timeout') from error
            raise
        if not chunk:
            if allow_eof and not result:
                raise EOFError('peer_eof')
            raise Rejected('truncated')
        result.extend(chunk)
    return bytes(result)


def send(stream, data, deadline):
    stream.settimeout(remaining(deadline))
    stream.sendall(data)


def upgrade(stream, deadline):
    request = bytearray()
    while not request.endswith(b'\r\n\r\n'):
        if len(request) >= 4096:
            raise Rejected('upgrade')
        request.extend(read_exact(stream, 1, deadline))
    try:
        lines = request.decode('ascii').split('\r\n')
        if lines[0] != 'GET /websocket/api HTTP/1.1':
            raise Rejected('upgrade')
        headers = {}
        for line in lines[1:-2]:
            name, value = line.split(':', 1)
            if not name or any(character not in "!#$%&'*+-.^_`|~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ" for character in name):
                raise Rejected('upgrade')
            name = name.lower()
            if name in headers:
                raise Rejected('upgrade')
            headers[name] = value.strip()
        key = headers.get('sec-websocket-key', '')
        if (not headers.get('host') or headers.get('upgrade', '').lower() != 'websocket'
                or 'upgrade' not in [item.strip().lower() for item in headers.get('connection', '').split(',')]
                or headers.get('sec-websocket-version') != '13'
                or PROTOCOL not in [item.strip() for item in headers.get('sec-websocket-protocol', '').split(',')]
                or len(base64.b64decode(key, validate=True)) != 16):
            raise Rejected('upgrade')
    except (UnicodeError, ValueError, binascii.Error) as error:
        raise Rejected('upgrade') from error
    accept = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode('ascii')).digest()).decode('ascii')
    response = ('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n'
                f'Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n'
                f'Sec-WebSocket-Protocol: {PROTOCOL}\r\n\r\n')
    send(stream, response.encode('ascii'), deadline)


def frame(stream, opcode, payload, deadline):
    send(stream, bytes([0x80 | opcode, len(payload)]) + payload, deadline)


def receive_control(stream, deadline):
    header = read_exact(stream, 2, deadline, allow_eof=True)
    opcode, size = header[0] & 0x0f, header[1] & 0x7f
    if (header[0] & 0xf0 != 0x80 or not header[1] & 0x80
            or opcode not in (8, 9, 10) or size > 125):
        raise Rejected('frame')
    mask = read_exact(stream, 4, deadline)
    wire = read_exact(stream, size, deadline)
    payload = bytes(value ^ mask[index % 4] for index, value in enumerate(wire))
    if opcode == 8:
        if len(payload) == 1:
            raise Rejected('frame')
        if len(payload) >= 2:
            code = int.from_bytes(payload[:2], 'big')
            if code not in (1000, 1001, 1002, 1003, 1007, 1008, 1009, 1010, 1011, 1012, 1013, 1014) and not 3000 <= code <= 4999:
                raise Rejected('frame')
            try:
                payload[2:].decode('utf-8')
            except UnicodeError as error:
                raise Rejected('frame') from error
    return opcode, payload


def connection(raw, context, args, deadline, started):
    handshake_deadline = min(deadline, time.monotonic() + args.handshake_timeout)
    raw.settimeout(remaining(handshake_deadline))
    with context.wrap_socket(raw, server_side=True, do_handshake_on_connect=False) as stream:
        stream.settimeout(remaining(handshake_deadline))
        stream.do_handshake()
        upgrade(stream, handshake_deadline)
        upgraded = time.monotonic()
        active_deadline = min(deadline, upgraded + args.connection_timeout)
        frame(stream, 1, BASELINE, active_deadline)
        pings = 0
        while True:
            try:
                opcode, payload = receive_control(stream, active_deadline)
            except EOFError:
                reason = 'peer_eof'
                break
            except TimeoutError:
                if time.monotonic() >= deadline:
                    raise TimeoutError('run_deadline')
                # Give the final close write a small, bounded grace within the total deadline.
                frame(stream, 8, b'\x03\xe8', min(deadline, time.monotonic() + 0.1))
                reason = 'connection_deadline'
                break
            if opcode == 9:
                frame(stream, 10, payload, active_deadline)
                pings += 1
            elif opcode == 8:
                frame(stream, 8, payload, active_deadline)
                reason = 'peer_close'
                break
        record('connection', reason=reason, pings=pings,
               handshake_ms=round((upgraded - started) * 1000),
               active_ms=round((time.monotonic() - upgraded) * 1000))
        return pings


def run(args):
    started = time.monotonic()
    deadline = started + args.run_timeout
    completed, pings = 0, 0
    try:
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.minimum_version = ssl.TLSVersion.TLSv1_2
        context.load_cert_chain(args.cert, args.key)
        family = socket.AF_INET6 if ':' in args.bind else socket.AF_INET
        with socket.socket(family) as listener:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind((args.bind, args.port))
            listener.listen(1)
            record('ready', expected_connections=args.connections, run_timeout_ms=round(args.run_timeout * 1000))
            while completed < args.connections:
                listener.settimeout(remaining(deadline))
                raw, _ = listener.accept()
                with raw:
                    pings += connection(raw, context, args, deadline, time.monotonic())
                completed += 1
    except (OSError, EOFError, Rejected, ValueError) as error:
        reason = ('timeout' if isinstance(error, TimeoutError) else
                  'invalid_upgrade_or_frame' if isinstance(error, Rejected) else
                  'tls' if isinstance(error, ssl.SSLError) else
                  'peer_eof' if isinstance(error, EOFError) else 'setup_or_io')
        record('error', reason=reason)
    success = completed == args.connections
    record('summary', success=success, connections=completed, expected_connections=args.connections,
           pings=pings, elapsed_ms=round((time.monotonic() - started) * 1000))
    return 0 if success else 1


if __name__ == '__main__':
    sys.exit(run(arguments()))
