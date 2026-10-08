# ABOUTME: Tests the bounded diagnostic peer through real loopback TLS and WebSocket exchanges.
# ABOUTME: Generates disposable certificates and checks trust, frame validation and deadlines.

import json
from pathlib import Path
import socket
import ssl
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent
PEER = ROOT / 'scripts/tls-capture-peer'


class PeerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.directory = Path(cls.temporary.name)
        cls.cert = cls.directory / 'cert.pem'
        cls.key = cls.directory / 'key.pem'
        subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
                        '-keyout', str(cls.key), '-out', str(cls.cert), '-days', '1',
                        '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost'],
                       check=True, capture_output=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def start_peer(self, *extra):
        self.assertTrue(PEER.is_file(), 'diagnostic TLS peer executable is missing')
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            port = reservation.getsockname()[1]
        process = subprocess.Popen([str(PEER), '--bind', '127.0.0.1', '--port', str(port),
                                    '--cert', str(self.cert), '--key', str(self.key),
                                    '--run-timeout', '3', '--handshake-timeout', '0.4',
                                    '--connection-timeout', '0.5', *extra],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.stop_peer, process)
        ready = process.stdout.readline()
        if not ready:
            self.fail(f'no readiness record: {process.stderr.read()}')
        self.assertEqual(json.loads(ready)['event'], 'ready')
        return process, port

    @staticmethod
    def stop_peer(process):
        if process.poll() is None:
            process.kill()
        process.communicate(timeout=3)

    def client(self, port, hostname='localhost', trusted=True):
        context = ssl.create_default_context(cafile=str(self.cert) if trusted else None)
        stream = context.wrap_socket(socket.create_connection(('127.0.0.1', port), timeout=2),
                                     server_hostname=hostname)
        self.addCleanup(stream.close)
        return stream

    @staticmethod
    def upgrade(stream, change=''):
        request = ('GET /websocket/api HTTP/1.1\r\nHost: localhost\r\n'
                   'Upgrade: websocket\r\nConnection: keep-alive, Upgrade\r\n'
                   'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n'
                   'Sec-WebSocket-Version: 13\r\n'
                   'Sec-WebSocket-Protocol: v1.api.smartspeaker.audio\r\n'
                   'X-Sonos-Api-Key: test-secret-never-log\r\n\r\n')
        if change:
            request = request.replace(*change)
        stream.sendall(request.encode('ascii'))
        response = bytearray()
        while not response.endswith(b'\r\n\r\n'):
            chunk = stream.recv(1)
            if not chunk:
                break
            response.extend(chunk)
        return bytes(response)

    @staticmethod
    def read_frame(stream):
        def exact(size):
            output = bytearray()
            while len(output) < size:
                chunk = stream.recv(size - len(output))
                if not chunk:
                    raise AssertionError('frame ended early')
                output.extend(chunk)
            return bytes(output)
        header = exact(2)
        if header[1] & 0x80 or header[1] >= 126:
            raise AssertionError('unexpected server mask or oversized fixture frame')
        return header[0], exact(header[1])

    @staticmethod
    def send_frame(stream, opcode, payload=b''):
        mask = b'\x12\x34\x56\x78'
        body = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
        stream.sendall(bytes([0x80 | opcode, 0x80 | len(payload)]) + mask + body)

    def finish(self, process, success):
        output, errors = process.communicate(timeout=4)
        self.assertEqual(process.returncode, 0 if success else 1, errors + output)
        self.assertEqual(errors, '')
        self.assertNotIn('test-secret', output)
        records = [json.loads(line) for line in output.splitlines()]
        self.assertEqual(records[-1]['event'], 'summary')
        self.assertEqual(records[-1]['success'], success)
        return records

    def test_three_verified_tls_upgrades_deliver_baseline_and_answer_ping(self):
        # Missing upgrade checks, payload or pong, or wrong counts break real clients.
        process, port = self.start_peer('--connections', '3')
        for _ in range(3):
            with self.client(port) as stream:
                response = self.upgrade(stream)
                self.assertTrue(response.startswith(b'HTTP/1.1 101 Switching Protocols\r\n'))
                self.assertIn(b'Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n', response)
                self.assertIn(b'Sec-WebSocket-Protocol: v1.api.smartspeaker.audio\r\n', response)
                opcode, payload = self.read_frame(stream)
                self.assertEqual(opcode, 0x81)
                self.assertEqual(json.loads(payload), [{'namespace': 'fixture:1'}, {}])
                self.send_frame(stream, 9, b'health')
                self.assertEqual(self.read_frame(stream), (0x8a, b'health'))
                self.send_frame(stream, 8, b'\x03\xe8')
                self.assertEqual(self.read_frame(stream), (0x88, b'\x03\xe8'))
        records = self.finish(process, True)
        self.assertEqual(records[-1]['connections'], 3)
        self.assertEqual(records[-1]['pings'], 3)

    def test_invalid_websocket_upgrade_is_rejected(self):
        # Accepting missing/wrong required headers would count a non-WebSocket client.
        changes = [('GET /websocket/api', 'POST /websocket/api'),
                   ('/websocket/api', '/wrong'), ('Upgrade: websocket', 'Upgrade: other'),
                   ('Connection: keep-alive, Upgrade', 'Connection: keep-alive'),
                   ('dGhlIHNhbXBsZSBub25jZQ==', 'invalid'),
                   ('Sec-WebSocket-Version: 13', 'Sec-WebSocket-Version: 12'),
                   ('v1.api.smartspeaker.audio', 'other'),
                   ('Host: localhost', 'Host: localhost\r\nHost: duplicate')]
        for change in changes:
            with self.subTest(change=change):
                process, port = self.start_peer('--connections', '1')
                with self.client(port) as stream:
                    response = self.upgrade(stream, change)
                    self.assertFalse(response.startswith(b'HTTP/1.1 101'))
                self.finish(process, False)

    def test_unmasked_and_oversized_control_frames_fail(self):
        # Trusting client frames without masks or control-frame bounds is invalid RFC6455.
        for frame in (b'\x89\x01x', b'\x89\xfe\x00\x7e', b'\x09\x80', b'\xc9\x80'):
            with self.subTest(frame=frame):
                process, port = self.start_peer('--connections', '1')
                with self.client(port) as stream:
                    self.upgrade(stream)
                    self.read_frame(stream)
                    stream.sendall(frame)
                    self.assertEqual(stream.recv(1), b'')
                self.finish(process, False)

    def test_truncated_control_frames_do_not_count_as_clean_disconnect(self):
        # EOF midway through a frame must not turn malformed input into success.
        for wire in (b'\x89', b'\x89\x80\x12', b'\x89\x81\x12\x34\x56\x78'):
            with self.subTest(wire=wire):
                process, port = self.start_peer('--connections', '1')
                with self.client(port) as stream:
                    self.upgrade(stream)
                    self.read_frame(stream)
                    stream.sendall(wire)
                self.finish(process, False)

    def test_incomplete_control_frame_timeout_is_not_success(self):
        # A partially received frame cannot pass as an idle healthy connection.
        process, port = self.start_peer('--connections', '1')
        with self.client(port) as stream:
            self.upgrade(stream)
            self.read_frame(stream)
            stream.sendall(b'\x89\x81\x12')
            self.assertEqual(stream.recv(1), b'')
        self.finish(process, False)

    def test_clean_tls_disconnect_counts_after_baseline(self):
        # Device destruction may close TLS without a WebSocket close frame.
        process, port = self.start_peer('--connections', '1')
        with self.client(port) as stream:
            self.upgrade(stream)
            self.read_frame(stream)
        records = self.finish(process, True)
        self.assertEqual(records[0]['reason'], 'peer_eof')

    def test_untrusted_or_wrong_hostname_certificate_never_upgrades(self):
        # The exercised client must verify both trust chain and name.
        for hostname, trusted in [('wrong.invalid', True), ('localhost', False)]:
            with self.subTest(hostname=hostname):
                process, port = self.start_peer('--connections', '1')
                with self.assertRaises(ssl.SSLCertVerificationError):
                    self.client(port, hostname, trusted)
                self.finish(process, False)

    def test_stalled_tls_and_partial_upgrade_are_bounded(self):
        # Socket deadlines must bound silent raw TCP and incomplete headers.
        for tls in (False, True):
            with self.subTest(tls=tls):
                started = time.monotonic()
                process, port = self.start_peer('--connections', '1')
                with self.client(port) if tls else socket.create_connection(('127.0.0.1', port)) as stream:
                    if tls:
                        stream.sendall(b'GET /websocket/api HTTP/1.1\r\n')
                    self.finish(process, False)
                self.assertLess(time.monotonic() - started, 2)

    def test_absent_client_exits_at_total_deadline(self):
        # Accept must have the same absolute total deadline as active connections.
        started = time.monotonic()
        process, _ = self.start_peer('--connections', '1', '--run-timeout', '0.25')
        self.finish(process, False)
        self.assertLess(time.monotonic() - started, 2)

    def test_idle_upgraded_client_closes_at_connection_deadline(self):
        # A client missing cancellation cannot retain a fixture process indefinitely.
        process, port = self.start_peer('--connections', '1')
        with self.client(port) as stream:
            self.upgrade(stream)
            self.read_frame(stream)
            self.assertEqual(self.read_frame(stream), (0x88, b'\x03\xe8'))
        records = self.finish(process, True)
        self.assertEqual(records[0]['reason'], 'connection_deadline')

    def test_cli_rejects_invalid_values_before_listening(self):
        self.assertTrue(PEER.is_file(), 'diagnostic TLS peer executable is missing')
        baseline = [str(PEER), '--bind', '127.0.0.1', '--cert', str(self.cert), '--key', str(self.key)]
        for option, value in [('--port', '0'), ('--port', '65536'), ('--port', '１２'),
                              ('--connections', '0'), ('--connections', '101'),
                              ('--run-timeout', 'nan'), ('--handshake-timeout', 'inf'),
                              ('--connection-timeout', '-1'), ('--bind', 'localhost'),
                              ('--cert', str(self.directory / 'absent'))]:
            with self.subTest(option=option, value=value):
                result = subprocess.run(baseline + [option, value], capture_output=True, text=True, timeout=3)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('"event": "ready"', result.stdout)


if __name__ == '__main__':
    unittest.main()
