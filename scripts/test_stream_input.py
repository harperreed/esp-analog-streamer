# ABOUTME: Tests known endpoint decoding with real paced localhost HTTP and production FLAC.
# ABOUTME: Checks bounded startup, exact stereo samples, WAV and wrong-container rejection.
import contextlib
import socket
import struct
import subprocess
import tempfile
import threading
import time
import unittest
from pathlib import Path

from stream_format import input_format_args


@contextlib.contextmanager
def paced_http(header, blocks, interval):
    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen(1)
    listener.settimeout(2)
    stopped = threading.Event()
    errors = []

    def send():
        try:
            peer, _ = listener.accept()
            with peer:
                peer.settimeout(2)
                request = b''
                while b'\r\n\r\n' not in request:
                    chunk = peer.recv(4096)
                    if not chunk:
                        return
                    request += chunk
                origin = time.monotonic()
                peer.sendall(b'HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n' + header)
                for index, block in enumerate(blocks):
                    if stopped.wait(max(0, origin + (index + 1) * interval - time.monotonic())):
                        break
                    peer.sendall(block)
                stopped.wait(8)
        except (BrokenPipeError, ConnectionResetError):
            pass  # A bounded output decoder deliberately closes before this endless source.
        except Exception as error:
            errors.append(error)

    thread = threading.Thread(target=send, daemon=True)
    thread.start()
    try:
        yield f'http://127.0.0.1:{listener.getsockname()[1]}/stream'
    finally:
        stopped.set()
        listener.close()
        thread.join(3)
        if thread.is_alive():
            raise AssertionError('loopback server did not stop')
        if errors:
            raise errors[0]


class StreamInputTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with tempfile.TemporaryDirectory() as directory:
            generator = Path(directory) / 'flac-fixture'
            source = Path(__file__).with_suffix('.rs')
            subprocess.run(['rustc', '--edition', '2021', '-O', str(source), '-o', str(generator)],
                           check=True, capture_output=True, timeout=30)
            data = subprocess.run([str(generator)], check=True, capture_output=True, timeout=10).stdout
        cls.header = data[:42]
        cls.blocks = []
        offset = 42
        while offset < len(data):
            size = struct.unpack_from('<I', data, offset)[0]
            offset += 4
            cls.blocks.append(data[offset:offset + size])
            offset += size

    def decode(self, source, path):
        try:
            return subprocess.run(
                ['ffmpeg', '-nostdin', '-nostats', '-v', 'level+warning', '-xerror',
                 '-rw_timeout', '10000000', '-err_detect', 'crccheck',
                 *input_format_args(path), '-i', source, '-t', '2',
                 '-c:a', 'pcm_s32le', '-f', 's32le', '-'],
                capture_output=True, timeout=6)
        except subprocess.TimeoutExpired:
            self.fail('two decoded seconds did not arrive within the six-second startup budget')

    def test_constant_flac_decodes_two_seconds_before_startup_budget(self):
        # Removing known-format selection restores the >12 s auto-probe delay.
        with paced_http(self.header, self.blocks, 4096 / 48000) as source:
            result = self.decode(source, '/stream.flac')
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertEqual(result.stderr, b'')
        self.assertEqual(len(result.stdout), 96000 * 8)
        self.assertTrue(result.stdout == struct.pack('<ii', 100 << 16, -200 << 16) * 96000,
                        'decoded stereo differs from the literal channel reference')

    def test_wav_endpoint_decodes_exact_stereo(self):
        samples = (int(100 * 256).to_bytes(3, 'little', signed=True)
                   + int(-200 * 256).to_bytes(3, 'little', signed=True)) * 384000
        header = (b'RIFF' + struct.pack('<I', 36 + len(samples)) + b'WAVEfmt '
                  + struct.pack('<IHHIIHH', 16, 1, 2, 48000, 288000, 6, 24)
                  + b'data' + struct.pack('<I', len(samples)))
        with paced_http(header, [samples], 0) as source:
            result = self.decode(source, '/stream.wav')
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        lines = result.stderr.decode().splitlines()
        self.assertEqual(len(lines), 1, result.stderr.decode())
        self.assertIn('Guessed Channel Layout: stereo', lines[0])
        self.assertEqual(len(result.stdout), 96000 * 8)
        self.assertTrue(result.stdout == struct.pack('<ii', 100 << 16, -200 << 16) * 96000,
                        'decoded stereo differs from the literal channel reference')

    def test_flac_header_cannot_be_decoded_as_wav(self):
        with paced_http(self.header, self.blocks[:2], 0) as source:
            result = self.decode(source, '/stream.wav')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, b'')
        self.assertIn(b'Invalid', result.stderr)

    def test_unknown_endpoint_is_rejected(self):
        with self.assertRaises(ValueError):
            input_format_args('/other')


if __name__ == '__main__':
    unittest.main()
