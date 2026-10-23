# ABOUTME: Tests stream formats and analysis against real local audio fixtures.
# ABOUTME: Calls the same importable helpers used by the public harnesses.
import importlib
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

import numpy as np

sys.path.insert(0, str(Path('scripts').resolve()))
from stream_format import input_format_args, probe_format, validate_flac_header, validate_flac_decoder_log
from decoder_integrity import validate_decoder_log
from test_stream_input import StreamInputTests
import stream_check
import stream_soak


class FormatTests(unittest.TestCase):
    def test_soak_format_validation_preserves_decode_duration(self):
        with tempfile.TemporaryDirectory() as directory:
            source_path = Path(directory) / 'source.flac'
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                            'anoisesrc=sample_rate=48000:duration=3:seed=42', '-ac', '2',
                            '-c:a', 'flac', '-sample_fmt', 's16', str(source_path)],
                           check=True, capture_output=True)
            for duration in (1, 2):
                progress = Path(directory) / f'progress-{duration}'
                hashes = Path(directory) / f'hashes-{duration}'
                environment = dict(progress=progress, hashes=hashes, ip='unused',
                                   input_format_args=input_format_args,
                                   args=SimpleNamespace(seconds=duration, stream_port=8000, path='/stream.flac'))
                argv = stream_soak.decoder_command(environment['args'], 'unused', progress, hashes)
                # Only replace the network input; run the harness's actual output options.
                argv[argv.index('-i') + 1] = str(source_path)
                result = subprocess.run(argv, check=True, capture_output=True, text=True, timeout=10)
                times = [int(line.split('=')[1]) for line in progress.read_text().splitlines()
                         if line.startswith('out_time_us=') and line.split('=')[1].isdigit()]
                self.assertEqual(max(times), duration * 1000000)
                validate_flac_decoder_log(result.stderr)
                self.assertLess(len(result.stderr), 16384)
                # Independently count decoded samples, rather than trusting progress alone.
                samples = sum(int(line.split(',')[3]) for line in hashes.read_text().splitlines()
                              if line and not line.startswith('#'))
                self.assertEqual(samples, duration * 48000)
            print('Soak actual command: full 1s/2s progress, decoded 48000/96000 samples; format log <16KiB')

    def test_mp3_actual_soak_command_counts_samples_and_rejects_short_input(self):
        from decoder_integrity import FrameHash
        from stream_format import validate_stream_decoder_log
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'source.mp3'
            subprocess.run(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i',
                            'anoisesrc=sample_rate=48000:duration=3:seed=42', '-ac', '2',
                            '-c:a', 'libmp3lame', '-b:a', '320k', str(path)], check=True, capture_output=True)
            self.assertEqual(input_format_args('/stream.mp3'), ['-f', 'mp3'])
            self.assertEqual(probe_format(path, '/stream.mp3')['channels'], 2)
            for short in (False, True):
                fixture = Path(directory) / ('short.mp3' if short else 'source.mp3')
                if short:
                    fixture.write_bytes(path.read_bytes()[:12000])
                progress = Path(directory) / f'progress-{short}'
                hashes = Path(directory) / f'hashes-{short}'
                argv = stream_soak.decoder_command(SimpleNamespace(seconds=2, stream_port=8000, path='/stream.mp3'), 'unused', progress, hashes)
                argv[argv.index('-i') + 1] = str(fixture)
                result = subprocess.run(argv, capture_output=True, text=True, timeout=10)
                validate_stream_decoder_log(result.stderr, '/stream.mp3')
                accounting = FrameHash(hashes)
                accounting.update(final=True)
                self.assertEqual(accounting.frames == 96000, not short,
                                 'a valid short MP3 prefix must fail exact requested sample accounting')
                accounting.close()
                if not short:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    for invalid in (result.stderr.replace('48000 Hz', '44100 Hz'),
                                    result.stderr.replace('stereo', 'mono')):
                        with self.assertRaises(ValueError):
                            validate_stream_decoder_log(invalid, '/stream.mp3')

    def test_stereo_energy_matches_board_meter(self):
        wave = np.tile([2**21, -2**21], 24000)
        for right, offset in [(wave, (0, 0)), (-wave, (0, 0)),
                              (-wave, (1000000, -2000000)), (wave // 2, (0, 0))]:
            frames = np.column_stack((wave, right)) + offset
            expected = 10 * np.log10((np.mean(wave.astype(float)**2)
                                     + np.mean(right.astype(float)**2)) / 2 / (2**23)**2)
            with self.subTest(offset=offset, right=right[0]):
                self.assertAlmostEqual(stream_check.dbfs(frames), expected, places=9)
                ok, message = stream_check.check_level(frames, [(0, 0, expected, False, '')])
                self.assertTrue(ok, message)
                wrong, _ = stream_check.check_level(frames, [(0, 0, expected + 6, False, '')])
                self.assertFalse(wrong)
        silent = np.tile([1000000, -2000000], (48000, 1))
        self.assertEqual(stream_check.dbfs(silent), -120.0)
        self.assertTrue(stream_check.check_level(silent, [(0, 0, -120.0, False, '')])[0])

    def test_harnesses_use_shared_format_validation(self):
        for name, function in [('stream_check', 'probe_format'), ('stream_soak', 'validate_stream_decoder_log'),
                               ('encoder_fault_check', 'validate_flac_header')]:
            self.assertIs(getattr(importlib.import_module(name), function),
                          getattr(importlib.import_module('stream_format'), function))

    def test_real_metadata_and_analysis_scale(self):
        with tempfile.TemporaryDirectory() as directory:
            levels = []
            for suffix, codec, sample_fmt, rate, channels, accepted in [
                ('flac', 'flac', 's16', 48000, 2, True),
                ('wav', 'pcm_s24le', 's32', 48000, 2, True),
                ('flac', 'flac', 's32', 48000, 2, False),
                ('wav', 'pcm_s16le', 's16', 48000, 2, False),
                ('flac', 'flac', 's16', 44100, 2, False),
                ('flac', 'flac', 's16', 48000, 1, False),
            ]:
                with self.subTest(codec=codec, sample_fmt=sample_fmt, rate=rate, channels=channels):
                    path = Path(directory) / f'capture.{suffix}'
                    subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-f', 'lavfi',
                                    '-i', f'sine=frequency=1000:sample_rate={rate}:duration=0.1',
                                    '-ac', str(channels), '-c:a', codec, '-sample_fmt', sample_fmt,
                                    str(path)], check=True, capture_output=True)
                    if accepted:
                        probe_format(path, f'/stream.{suffix}')
                        levels.append(stream_check.dbfs(stream_check.decode(path)))

                    else:
                        with self.assertRaises(ValueError):
                            probe_format(path, f'/stream.{suffix}')
                    decoded = subprocess.run(['ffmpeg', '-nostdin', '-nostats', '-v', 'level+info',
                                              '-i', str(path), '-f', 'null', '-'],
                                             check=True, capture_output=True, text=True)
                    if accepted and suffix == 'flac':
                        validate_flac_decoder_log(decoded.stderr)
                        for depth in (8, 12):
                            with self.assertRaises(ValueError):
                                validate_flac_decoder_log(decoded.stderr.replace('s16', f's16 ({depth} bit)'))
                    else:
                        with self.assertRaises(ValueError):
                            validate_flac_decoder_log(decoded.stderr)
                    if suffix == 'flac':
                        header = path.read_bytes()[:42]
                        if accepted:
                            validate_flac_header(header)
                            for broken in [header[:41], b'xxxx' + header[4:], header[:7] + b'\x21' + header[8:]]:
                                with self.assertRaises(ValueError):
                                    validate_flac_header(broken)
                        else:
                            with self.assertRaises(ValueError):
                                validate_flac_header(header)
            self.assertLess(abs(levels[0] - levels[1]), 0.01)


unittest.main()
