# ABOUTME: Exercises replay artifact accounting and real FFmpeg FLAC integrity failures.
# ABOUTME: Generates local codec fixtures independently of the production Rust replay producer.
import copy
import json
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path

try:
    from replay_stream import validate
except ImportError:
    def validate(path):
        raise AssertionError('replay artifact validator is not implemented')


class ReplayTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.manifest = dict(schema_version=1, transport='transport.bin', http_header='http.bin',
                             codec_header='codec.bin', expected_pcm='expected.pcm',
                             complete_audio_bytes=0, expected_frames=0, terminal_reason='finished',
                             incomplete_tail='tail.bin', phase='payload')
        self.http = b'HTTP/1.1 200 OK\r\nContent-Type: audio/flac\r\n\r\n'
        for name, data in [('http.bin', self.http), ('codec.bin', b'fLaC'),
                           ('expected.pcm', b''), ('tail.bin', b'')]:
            (self.root / name).write_bytes(data)

    def run_manifest(self):
        path = self.root / 'manifest.json'
        path.write_text(json.dumps(self.manifest))
        return validate(path)

    def fixture(self):
        samples = [((i * 113) % 30001 - 15000, (i * 317) % 28001 - 14000)
                   for i in range(18432)]
        raw = b''.join(struct.pack('<hh', *pair) for pair in samples)
        (self.root / 'source.pcm').write_bytes(raw)
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-f', 's16le', '-ar', '48000',
                        '-ac', '2', '-i', str(self.root / 'source.pcm'), '-c:a', 'flac',
                        str(self.root / 'audio.flac')], check=True, capture_output=True, timeout=10)
        packets = json.loads(subprocess.run(['ffprobe', '-v', 'error', '-show_packets', '-of', 'json',
                            str(self.root / 'audio.flac')], check=True, capture_output=True,
                            timeout=10).stdout)['packets']
        blob = (self.root / 'audio.flac').read_bytes()
        header = blob[:int(packets[0]['pos'])]
        blocks = [blob[int(p['pos']):int(p['pos']) + int(p['size'])] for p in packets]
        (self.root / 'codec.bin').write_bytes(header)
        (self.root / 'expected.pcm').write_bytes(b''.join(struct.pack('<ii', a << 16, b << 16)
                                                       for a, b in samples))
        self.manifest.update(complete_audio_bytes=len(blob), expected_frames=18432)
        (self.root / 'transport.bin').write_bytes(self.http + blob)
        return header, blocks

    def test_missing_fields_fail_before_decoder(self):
        self.manifest.pop('expected_frames')
        with self.assertRaises(ValueError):
            self.run_manifest()

    def test_paths_cannot_escape_artifact_directory(self):
        self.manifest['transport'] = '../outside'
        with self.assertRaises(ValueError):
            self.run_manifest()

    def test_header_only_is_explicit_byte_validation(self):
        self.manifest['complete_audio_bytes'] = 4
        (self.root / 'transport.bin').write_bytes(self.http + b'fLaC')
        self.assertEqual(self.run_manifest()['status'], 'zero_pcm')

    def test_partial_http_header_is_explicitly_bounded(self):
        (self.root / 'initial.bin').write_bytes(self.http + b'fLaC')
        (self.root / 'tail.bin').write_bytes(self.http[:12])
        (self.root / 'transport.bin').write_bytes(self.http[:12])
        self.manifest.update(terminal_reason='write_timeout', pending_write='initial.bin', phase='header')
        self.assertEqual(self.run_manifest()['status'], 'zero_pcm')
        (self.root / 'tail.bin').write_bytes(self.http + b'fLaC')
        (self.root / 'transport.bin').write_bytes(self.http + b'fLaC')
        with self.assertRaises(ValueError):
            self.run_manifest()

    def test_partial_codec_header_and_empty_initial_write_are_bounded(self):
        initial = self.http + b'fLaC'
        (self.root / 'initial.bin').write_bytes(initial)
        self.manifest.update(terminal_reason='write_error', pending_write='initial.bin', phase='header')
        for accepted in [b'', initial[:-1]]:
            with self.subTest(accepted=len(accepted)):
                (self.root / 'tail.bin').write_bytes(accepted)
                (self.root / 'transport.bin').write_bytes(accepted)
                self.assertEqual(self.run_manifest()['status'], 'zero_pcm')

    def test_schema_rejects_unbounded_and_noninteger_counts(self):
        for key, value in [('expected_frames', 1000001), ('expected_frames', True),
                           ('complete_audio_bytes', -1), ('schema_version', 2), ('phase', 'unknown')]:
            with self.subTest(key=key, value=value):
                original = copy.deepcopy(self.manifest)
                self.manifest[key] = value
                with self.assertRaises(ValueError):
                    self.run_manifest()
                self.manifest = original

    def test_real_flac_matches_independent_stereo_samples(self):
        self.fixture()
        result = self.run_manifest()
        self.assertEqual(result['decoded_audio_frames'], 18432)
        self.assertEqual(result['status'], 'decoded')

    def test_real_flac_negative_controls(self):
        header, blocks = self.fixture()
        original = copy.deepcopy(self.manifest)
        corrupt = bytearray(blocks[1]); corrupt[-1] ^= 1
        cases = [blocks[:1] + blocks[2:], blocks[:1] + [blocks[0]] + blocks[1:],
                 blocks[:1] + [bytes(corrupt)] + blocks[2:], blocks[:-1],
                 blocks[:-1] + [blocks[-1][:-7]]]
        for parts in cases:
            with self.subTest(lengths=[len(p) for p in parts]):
                self.manifest = copy.deepcopy(original)
                blob = header + b''.join(parts)
                self.manifest['complete_audio_bytes'] = len(blob)
                (self.root / 'transport.bin').write_bytes(self.http + blob)
                with self.assertRaises(ValueError):
                    self.run_manifest()

    def test_partial_final_write_uses_only_declared_complete_prefix(self):
        header, blocks = self.fixture()
        complete = header + b''.join(blocks[:-1])
        self.manifest.update(complete_audio_bytes=len(complete), expected_frames=13824,
                             terminal_reason='write_timeout', pending_write='pending.bin')
        pcm = self.root / 'expected.pcm'; pcm.write_bytes(pcm.read_bytes()[:13824 * 8])
        (self.root / 'pending.bin').write_bytes(blocks[-1])
        (self.root / 'tail.bin').write_bytes(blocks[-1][:13])
        (self.root / 'transport.bin').write_bytes(self.http + complete + blocks[-1][:13])
        self.assertEqual(self.run_manifest()['decoded_audio_frames'], 13824)
        (self.root / 'tail.bin').write_bytes(b'wrong')
        with self.assertRaises(ValueError):
            self.run_manifest()

    def test_declared_complete_bytes_cannot_exceed_transport(self):
        self.fixture()
        self.manifest['complete_audio_bytes'] += 1
        with self.assertRaises(ValueError):
            self.run_manifest()

    def test_repeated_decodes_preserve_prior_evidence(self):
        self.fixture()
        first = self.run_manifest()['evidence']
        second = self.run_manifest()['evidence']
        self.assertNotEqual(first, second)
        self.assertTrue((Path(first) / 'ffmpeg.log').exists())

    def test_channel_swap_fails_content_check(self):
        self.fixture()
        path = self.root / 'expected.pcm'
        data = path.read_bytes()
        path.write_bytes(b''.join(data[i + 4:i + 8] + data[i:i + 4] for i in range(0, len(data), 8)))
        with self.assertRaises(ValueError):
            self.run_manifest()


if __name__ == '__main__':
    unittest.main()
