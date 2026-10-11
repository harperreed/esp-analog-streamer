# ABOUTME: Checks HTTP acceptance guards using retained protocol shapes and UART receipts.
# ABOUTME: Keeps ownership observations, deferred settings and payload evidence fail closed.
import unittest
import time
import signal
import subprocess
import sys
import tempfile
import json
from pathlib import Path
from types import SimpleNamespace
from sonos_http_check import stream_identity, payload_receipt, same_peer, verify_attestation, cleanup_attempt, confirmed_cleanup_uri, stop_owned_source, Trial, confirmed_observation_uri


class Guards(unittest.TestCase):
    def test_stream_identity_binds_format_buffer_generation_and_intent(self):
        uri = 'http://192.168.23.47:8000/stream.flac?buffer=med&network_generation=7&sonos_intent=41'
        self.assertEqual(stream_identity(uri, '192.168.23.47', 'flac', 'med'), 41)
        for value in (uri.replace('sonos_intent=41', 'sonos_intent=0'), uri + '&sonos_intent=42', uri.replace('23.47', '23.48')):
            with self.assertRaises(ValueError):
                stream_identity(value, '192.168.23.47', 'flac', 'med')

    def test_external_service_url_rejected(self):
        self.assertEqual(same_peer('http://192.168.23.205:1400/xml/device_description.xml', '/MediaRenderer/AVTransport/Control'), 'http://192.168.23.205:1400/MediaRenderer/AVTransport/Control')
        with self.assertRaises(ValueError):
            same_peer('http://192.168.23.205:1400/xml/device_description.xml', 'http://192.168.23.206:1400/control')

    def test_uart_requires_matching_sonos_owner_and_nonzero_delivery(self):
        raw = ('stream: 192.168.23.205:50000 joined /stream.flac (1 of 2) event_us=10 owner_id=3 sonos_intent=41\n'
               'stream-listener: peer=192.168.23.205:50000 frames_sent=2 bytes_sent=300 event_us=20\n'
               'stream: 192.168.23.205:50000 socket closed in 1 ms; releasing slot (1 held) event_us=25 owner_id=3 sonos_intent=41\n')
        self.assertEqual(payload_receipt(raw, '192.168.23.205', 'flac', 41)['bytes_sent'], 300)
        with self.assertRaises(ValueError):
            payload_receipt(raw.replace('bytes_sent=300', 'bytes_sent=0'), '192.168.23.205', 'flac', 41)
        with self.assertRaises(ValueError):
            payload_receipt(raw, '192.168.23.205', 'flac', 42)
        for altered in (raw.replace('event_us=20', 'event_us=9'),
                        raw.replace('event_us=20', 'event_us=26'),
                        raw.replace('event_us=25 owner_id=3', 'event_us=25 owner_id=4'),
                        raw + 'stream: 192.168.23.205:50000 joined /stream.flac (1 of 2) event_us=30 owner_id=4 sonos_intent=42\n'):
            with self.assertRaises(ValueError):
                payload_receipt(altered, '192.168.23.205', 'flac', 41)

    def test_failed_observation_never_establishes_cleanup_ownership(self):
        candidate = 'http://192.168.23.47:8000/stream.flac?buffer=med&network_generation=7&sonos_intent=41'
        for transport, track, source_live in [('STOPPED', candidate, True),
                                              ('PLAYING', 'x-rincon-queue:external', True),
                                              ('PLAYING', candidate, False)]:
            owned = None
            with self.assertRaises(ValueError):
                owned = confirmed_observation_uri(owned, candidate, transport, track, source_live)
            self.assertIsNone(owned)
            with self.assertRaises(ValueError):
                confirmed_cleanup_uri(owned, candidate)
        owned = confirmed_observation_uri(None, candidate, 'PLAYING', candidate, True)
        self.assertEqual(confirmed_cleanup_uri(owned, candidate), candidate)

    def test_uncertain_play_never_adopts_another_matching_board_uri(self):
        candidate = 'http://192.168.23.47:8000/stream.flac?buffer=med&network_generation=7&sonos_intent=42'
        with self.assertRaises(ValueError):
            confirmed_cleanup_uri(None, candidate)
        with self.assertRaises(ValueError):
            confirmed_cleanup_uri(candidate, candidate.replace('intent=42', 'intent=43'))
        self.assertEqual(confirmed_cleanup_uri(candidate, candidate), candidate)

    def test_cleanup_interrupt_propagates_and_expired_budget_runs_no_step(self):
        errors = []
        def interrupted():
            raise InterruptedError('cleanup alarm')
        with self.assertRaises(InterruptedError):
            cleanup_attempt(errors, 'interrupt', time.monotonic() + 10, interrupted)
        self.assertEqual(errors, [])
        called = []
        with self.assertRaises(TimeoutError):
            cleanup_attempt(errors, 'expired', time.monotonic() - 1, lambda: called.append(True))
        self.assertEqual(called, [])

    def test_real_cleanup_alarm_escapes_step(self):
        previous = signal.getsignal(signal.SIGALRM)
        def alarm(signum, frame):
            raise InterruptedError('real cleanup alarm')
        signal.signal(signal.SIGALRM, alarm)
        try:
            signal.setitimer(signal.ITIMER_REAL, .01)
            with self.assertRaises(InterruptedError):
                cleanup_attempt([], 'alarm', time.monotonic() + 1, signal.pause)
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, previous)

    def test_owned_source_stops_even_with_expired_control_budget(self):
        source = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        try:
            with self.assertRaises(TimeoutError):
                stop_owned_source(source, time.monotonic() - 1)
            source.wait(timeout=2)
            self.assertIsNotNone(source.returncode)
        finally:
            if source.poll() is None:
                source.kill()
                source.wait(timeout=2)

    def test_real_cleanup_stops_only_its_child_and_saves_receipt(self):
        previous = signal.getsignal(signal.SIGALRM)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / 'image.elf'
            image.write_bytes(b'host cleanup test image')
            trial = Trial(SimpleNamespace(out=root / 'evidence', image=image))
            trial.source = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
            try:
                trial.cleanup()
                self.assertIsNotNone(trial.source.poll())
                receipt = json.loads((root / 'evidence/result.json').read_text())
                self.assertTrue(receipt['cleanup_complete'])
                self.assertEqual(receipt['cleanup_errors'], [])
                self.assertLess(receipt['cleanup_elapsed_seconds'], 45)
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                signal.signal(signal.SIGALRM, previous)
                if trial.source.poll() is None:
                    trial.source.kill()
                    trial.source.wait(timeout=2)

    def test_boot_or_image_change_rejected(self):
        record = dict(build_elf_sha256='a' * 64, boot_token='b' * 32)
        verify_attestation(record, 'a' * 64, 'b' * 32)
        for image, boot in [('c' * 64, 'b' * 32), ('a' * 64, 'c' * 32)]:
            with self.assertRaises(ValueError):
                verify_attestation(record, image, boot)


if __name__ == '__main__':
    unittest.main()
