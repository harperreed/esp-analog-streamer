# ABOUTME: Validates real Encoder left-side FLAC through the transport capture checker.
# ABOUTME: Uses FFprobe/FFmpeg and rejects CRC-valid unsupported channel assignments.
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    'stereo_capture', ROOT / 'docs/audit-evidence/2026-10-09/runtime-codec-probe/staged-probe.py')
capture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(capture)


def crc8(data):
    value = 0
    for byte in data:
        value ^= byte
        for _ in range(8):
            value = ((value << 1) ^ (0x07 if value & 0x80 else 0)) & 0xff
    return value


class StereoCaptureTests(unittest.TestCase):
    def setUp(self):
        # Retained from core/tests/flac_stereo.rs identical fixture, number zero.
        # Claxon and FFmpeg independently verified its exact quantized PCM.
        self.data = (ROOT / 'scripts/fixtures/flac-left-side.flac').read_bytes()
        self.assertEqual(self.data[42:46], b'\xff\xf8\xca\x88')

    def test_real_left_side_capture_passes_whole_frame_decoder(self):
        with tempfile.TemporaryDirectory() as directory:
            summary, ends = capture.validate_capture(
                self.data, 'flac', Path(directory), 'left-side')
            self.assertEqual(summary['decoded_audio_frames'], 4096)
            self.assertEqual(summary['complete_coded_frames'], 1)
            self.assertEqual(summary['excluded_tail_bytes'], 0)
            self.assertEqual(ends, [len(self.data)])

    def test_corrupt_left_side_frame_is_rejected(self):
        data = bytearray(self.data)
        data[-1] ^= 1
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                capture.validate_capture(data, 'flac', Path(directory), 'bad-crc')

    def test_crc_valid_unsupported_assignments_are_rejected(self):
        for assignment in (0, 2, 7, 9, 10, 11, 15):
            with self.subTest(assignment=assignment), tempfile.TemporaryDirectory() as directory:
                data = bytearray(self.data)
                data[45] = (assignment << 4) | 8
                data[47] = crc8(data[42:47])
                data[-2:] = capture.crc16(data[42:-2]).to_bytes(2, 'big')
                self.assertEqual(crc8(data[42:48]), 0)
                self.assertEqual(capture.crc16(data[42:]), 0)
                with self.assertRaises(ValueError):
                    capture.validate_capture(data, 'flac', Path(directory), 'unsupported')


if __name__ == '__main__':
    unittest.main()
