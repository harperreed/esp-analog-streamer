# ABOUTME: Checks deterministic offline noise generation using real WAV files.
# ABOUTME: Covers the recipe, stereo samples, duration bounds, and exclusive output.
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import wave

import numpy as np

COMMAND = Path(__file__).with_name('noise_fixture.py')


class NoiseFixtureTests(unittest.TestCase):
    def invoke(self, output, *args):
        return subprocess.run(
            [sys.executable, str(COMMAND), '--output', str(output), *args],
            capture_output=True, text=True, check=False,
        )

    def test_repeatable_stereo_recipe_and_duration_prefix(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ('a.wav', 'b.wav', 'c.wav')]
            results = [self.invoke(path, '--duration', duration)
                       for path, duration in zip(paths, ('1', '1', '2'))]
            for result in results:
                self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(paths[0].read_bytes(), paths[1].read_bytes())
            with wave.open(str(paths[0])) as wav:
                self.assertEqual((wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()),
                                 (2, 2, 48000, 48000))
                pcm = wav.readframes(wav.getnframes())
            with wave.open(str(paths[2])) as wav:
                self.assertEqual(wav.getnframes(), 96000)
                self.assertEqual(wav.readframes(48000), pcm)
            samples = np.frombuffer(pcm, dtype='<i2').reshape(-1, 2)
            # Frozen synthetic PCG64 recipe: adjacent values must remain L/R pairs.
            self.assertEqual(samples[:4].tolist(),
                             [[-1871, 2440], [-1384, -3076], [-2528, -3083], [-84, -1368]])
            self.assertLessEqual(int(np.abs(samples).max()), 3277)
            self.assertGreater(float(samples.std()), 1800)
            self.assertLess(abs(float(np.corrcoef(samples.T)[0, 1])), 0.02)
            self.assertGreater(float(np.mean(samples[:, 0] != samples[:, 1])), 0.99)
            receipt = json.loads(results[0].stdout)
            self.assertEqual(receipt['sha256'], hashlib.sha256(paths[0].read_bytes()).hexdigest())
            self.assertEqual(receipt['frames'], 48000)
            self.assertEqual(receipt['seed'], 20261010)
            self.assertEqual(receipt['algorithm'], 'PCG64')
            self.assertEqual(receipt['numpy_version'], np.__version__)
            self.assertEqual(receipt['peak_amplitude'], 3277)
            self.assertEqual(receipt['duration_seconds'], 1)
            self.assertEqual(receipt['format'], {'sample_rate': 48000, 'channels': 2, 'sample_width_bytes': 2, 'encoding': 'PCM signed 16-bit little-endian'})

    def test_invalid_arguments_create_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'noise.wav'
            for args in (('--duration', '0'), ('--duration', '601'),
                         ('--duration', '1.5'), ('--duration', '1', '--seed', '-1'),
                         ('--duration', '1', '--seed', 'oops')):
                with self.subTest(args=args):
                    result = self.invoke(output, *args)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(output.exists())
                    self.assertEqual(result.stdout, '')
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'noise.wav'
            output.write_bytes(b'preserve me')
            result = self.invoke(output, '--duration', '1')
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output.read_bytes(), b'preserve me')
            self.assertEqual(result.stdout, '')

    def test_seed_changes_pcm(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ('a.wav', 'b.wav')]
            for path, seed in zip(paths, ('1', '2')):
                result = self.invoke(path, '--duration', '1', '--seed', seed)
                self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotEqual(paths[0].read_bytes(), paths[1].read_bytes())


if __name__ == '__main__':
    unittest.main()
