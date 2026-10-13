# ABOUTME: Exercises preset admission and MP3 decoding against retained and malformed evidence.
# ABOUTME: Uses real parsers and a saved production MP3 prefix without board access.
import importlib.util
import http.client
import json
from pathlib import Path
import tempfile
import socket
from types import SimpleNamespace
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('preset_driver', HERE / 'preset-driver.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
PEER = '192.168.23.49:12345'


def receipts():
    return ('buffer-admission: peer=' + PEER + ' format=Mp3 preset=low event_us=200 '
            'head_generation=0 head_sequence=900 cursor=Cursor { sequence: 858, generation: 0, offset: 0 }\n'
            'esp_shazam_firmware::stream: stream: ' + PEER + ' joined /stream.mp3 (1 of 2) event_us=190 owner_id=7 sonos_intent=0 role=diagnostic\n'
            'stream-listener: peer=' + PEER + ' frames_sent=70 bytes_sent=67200 event_us=300\n'
            'stream-media: peer=' + PEER + ' valid=true first_raw_generation=0 last_raw_generation=0 source_start=988416 source_end=1069056 first_encoded_generation=0 last_encoded_generation=0 first_encoded_sequence=859 last_encoded_sequence=928 event_us=300\n'
            'stream: ' + PEER + ' socket closed in 1 ms; releasing slot (1 held) event_us=301 owner_id=7 sonos_intent=0\n'
            'listener-telemetry: dropped=0 loss_occurred=false export_us=305 observed_event_range_valid=true observed_event_min_us=300 observed_event_max_us=301\n')


class AdmissionTests(unittest.TestCase):
    def test_actual_cursor_handles_admission_skew_and_inflight_frames(self):
        row = driver.validate_owner(receipts(), PEER, 'low', 64)
        self.assertIsNotNone(row)
        self.assertEqual(row['retained_frames'], 42)
        self.assertEqual(row['first_encoded_sequence'], 859)
        self.assertEqual(row['frames_sent'], 70)

    def test_wrong_preset_generation_first_sequence_skew_and_incomplete_rejected(self):
        for old, new in [('preset=low', 'preset=med'), ('head_generation=0', 'head_generation=1'),
                         ('first_encoded_sequence=859', 'first_encoded_sequence=860'),
                         ('head_sequence=900', 'head_sequence=901'),
                         ('last_raw_generation=0', 'last_raw_generation=1'),
                         ('offset: 0', 'offset: 1'), ('owner_id=7 sonos_intent=0\n', 'owner_id=8 sonos_intent=0\n'),
                         ('valid=true', 'valid=false')]:
            with self.subTest(new=new), self.assertRaises(ValueError):
                driver.validate_owner(receipts().replace(old, new), PEER, 'low', 64)
        with self.assertRaises(ValueError):
            driver.validate_owner(receipts().split('listener-telemetry:')[0], PEER, 'low', 64)

    def test_lost_malformed_and_wrong_range_export_rejected(self):
        for old, new in [('dropped=0', 'dropped=1'), ('loss_occurred=false', 'loss_occurred=true'),
                         ('export_us=305', 'export_us=oops'),
                         ('observed_event_max_us=301', 'observed_event_max_us=300')]:
            with self.subTest(new=new), self.assertRaises(ValueError):
                driver.validate_owner(receipts().replace(old, new), PEER, 'low', 64)

    def test_unrelated_later_event_does_not_require_later_export(self):
        self.assertTrue(driver.validate_owner(receipts() + 'other event_us=999\n', PEER, 'low', 64))

    def test_duplicate_peer_admission_rejected(self):
        with self.assertRaises(ValueError):
            driver.validate_owner(receipts() + receipts().splitlines()[0] + '\n', PEER, 'low', 64)

    def test_contradictory_media_boolean_rejected(self):
        with self.assertRaises(ValueError):
            driver.validate_owner(receipts().replace('valid=true', 'valid=true valid=false'), PEER, 'low', 64)

    def test_archived_close_reports_pre_release_slot_count(self):
        archive = driver.ROOT / 'target/kata-hardening/oct13-control-fault/cancel-uart.log'
        close = next(line for line in archive.read_text().splitlines() if 'socket closed' in line)
        self.assertIn('releasing slot (1 held)', close)
        self.assertTrue(driver.validate_owner(receipts(), PEER, 'low', 64))


class Mp3Tests(unittest.TestCase):
    def test_real_retained_prefix_decodes_actual_sample_count(self):
        source = driver.ROOT / 'target/kata-hardening/media-progress/controlled-baseline/0-mp3/listener-1-whole-prefix.mp3'
        with tempfile.TemporaryDirectory() as directory:
            row = driver.decode_prefix(source.read_bytes()[:64 * 960], Path(directory), 'fixture', 10)
            self.assertIsNotNone(row)
            self.assertEqual(row['decoded_audio_frames'], 73728)
            self.assertEqual(row['mp3_frames'], 64)

    def test_incomplete_header_and_reservoir_rejected(self):
        frame = bytearray.fromhex('fffbe400') + bytearray(956)
        for data in [frame[:-1], bytes(960), frame[:4] + bytes([1]) + frame[5:]]:
            with self.subTest(size=len(data)), self.assertRaises(ValueError):
                driver.validate_mp3(data)


class UartTests(unittest.TestCase):
    def test_offending_uart_tail_is_retained_before_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            uart, image = base / 'uart.log', base / 'image.elf'
            uart.write_bytes(b'malformed boot\n')
            image.write_bytes(b'fixture')
            trial = driver.PresetTrial(SimpleNamespace(out=base / 'out', uart=uart, image=image, boot_token='b' * 32))
            with self.assertRaises(ValueError):
                trial.uart()
            self.assertEqual((base / 'out/uart-observed.log').read_bytes(), uart.read_bytes())
    def test_retained_actual_boot_passes_startup_and_steady_gate(self):
        archive = driver.ROOT / 'target/kata-hardening/oct13-control-fault/retention-two-uart.log'
        self.assertIn('stream-encode:', driver.checked_uart(archive.read_bytes(), final=True))

    def test_actual_encoder_failure_is_terminal(self):
        archive = driver.ROOT / 'target/kata-hardening/oct13-control-fault/retention-two-uart.log'
        with self.assertRaises(ValueError):
            driver.checked_uart(archive.read_bytes() + b'stream: encoder failed; output remains invalid (kind=Other raw_os_error=None) telemetry_epoch=1 telemetry_window=3 event_us=50000000\n')
    def test_raw_loss_and_second_reset_rejected(self):
        raw = (b'ets Jul 29 2019 12:21:46\r\n'
               b'rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)\r\n'
               b'encoder-boundary: prior_epoch=0 reason=Startup frames=0 resets=0 raw_overruns=0 telemetry_epoch=0 format=Flac event_us=1\n'
               b'encoder-boundary: prior_epoch=0 reason=Switched frames=3 resets=0 raw_overruns=0 telemetry_epoch=1 format=Mp3 event_us=2\n'
               b'stream-encode: encoded_frames=416 resets=0 raw_read_overruns=0 telemetry_epoch=1 window_end_us=1000\n')
        self.assertEqual(driver.checked_uart(raw), raw.decode())
        for invalid in [raw + raw, raw.replace(b'raw_read_overruns=0', b'raw_read_overruns=1'),
                        raw.replace(b'reason=Switched', b'reason=Overrun'),
                        raw.replace(b'resets=0', b'resets=oops'), raw + b'Guru Meditation\n']:
            with self.subTest(invalid=invalid[-30:]), self.assertRaises(ValueError):
                driver.checked_uart(invalid)

    def test_partial_tail_is_not_evidence(self):
        raw = (b'ets Jul 29 2019 12:21:46\r\n'
               b'rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)\r\n'
               b'listener-telemetry: dropped=oops')
        text = driver.checked_uart(raw)
        self.assertIsInstance(text, str)
        self.assertNotIn('listener-telemetry', text)


class HttpTests(unittest.TestCase):
    def response(self, wire, cap=128 * 1024):
        left, right = socket.socketpair()
        with left, right:
            right.sendall(wire)
            right.shutdown(socket.SHUT_WR)
            retained = bytearray()
            result = driver.read_response(left, retained, cap)
            return result, retained

    def test_actual_http_retains_wire_and_bounds_body(self):
        wire = b'HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}'
        result, retained = self.response(wire)
        self.assertEqual(result, b'{}')
        self.assertEqual(retained, wire)
        with self.assertRaises(ValueError):
            self.response(wire, 20)

    def test_redirect_and_incomplete_response_rejected(self):
        for wire in [b'HTTP/1.1 302 Found\r\nLocation: http://elsewhere/\r\nContent-Length: 0\r\n\r\n',
                     b'HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{}']:
            with self.subTest(wire=wire), self.assertRaises((ValueError, http.client.IncompleteRead)):
                self.response(wire)


class LeaseTests(unittest.TestCase):
    def test_retained_actual_initial_metrics_have_no_listener_loss_receipt(self):
        archive = driver.ROOT / 'target/kata-hardening/oct13-control-fault/retention-two/0005-body'
        value = driver.validate_metrics(json.loads(archive.read_bytes()))
        self.assertIsNone(value['listener_loss'])
        self.assertTrue(driver.snapshot_check(value, value['build_elf_sha256'], value['boot_token'], idle=True))
    def snapshot(self):
        return dict(build_elf_sha256='a' * 64, boot_token='b' * 32, served_device_us=10_000_000,
                    network=dict(connected=True, generation=2, disconnects=0),
                    settings=dict(active_format='mp3', next_play_format='mp3', next_play_buffer='med',
                                  autoplay=False, recognition_enabled=False, buffer_available=True, pga_gain=0),
                    listeners=[dict(state='idle'), dict(state='idle')],
                    listener_loss=dict(dropped=0, loss_occurred=False), encoder_loss=dict(dropped=0, loss_occurred=False),
                    media_progress=dict(health='healthy', encoded=dict(generation=0, raw_generation=0)))

    def test_optional_metadata_gap_has_no_cached_substitution(self):
        value = self.snapshot()
        settings, network = value['settings'].copy(), value['network'].copy()
        value['network'] = None
        self.assertFalse(driver.snapshot_check(value, 'a' * 64, 'b' * 32, settings, network))
        value['settings']['pga_gain'] = 1
        with self.assertRaises(ValueError):
            driver.snapshot_check(value, 'a' * 64, 'b' * 32, settings, network)

    def test_initial_unobserved_loss_families_are_not_invented(self):
        value = self.snapshot()
        value['listener_loss'] = value['encoder_loss'] = None
        self.assertTrue(driver.snapshot_check(value, 'a' * 64, 'b' * 32, idle=True))
        self.assertIsNone(value['listener_loss'])

    def test_identity_loss_lease_and_nonidle_cleanup_rejected(self):
        value = self.snapshot()
        settings, network = value['settings'].copy(), value['network'].copy()
        self.assertTrue(driver.snapshot_check(value, 'a' * 64, 'b' * 32, settings, network, idle=True))
        for key, field, replacement in [('network', 'generation', 3), ('listener_loss', 'dropped', 1),
                                        ('encoder_loss', 'loss_occurred', True), ('settings', 'autoplay', True),
                                        ('media_progress', 'health', 'reset_requested')]:
            invalid = self.snapshot()
            invalid[key][field] = replacement
            with self.subTest(key=key), self.assertRaises(ValueError):
                driver.snapshot_check(invalid, 'a' * 64, 'b' * 32, settings, network, idle=True)


if __name__ == '__main__':
    unittest.main()
