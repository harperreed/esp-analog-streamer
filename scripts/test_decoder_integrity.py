# ABOUTME: Tests actual harness decoder paths with deterministic FFmpeg FLAC and WAV fixtures.
# ABOUTME: Retains fixtures in target/task1-validation and uses real subprocesses, never network APIs.
import functools
import importlib
import hashlib
import json
import subprocess
import sys
import time
import unittest
import tempfile
from pathlib import Path
from types import SimpleNamespace

import numpy as np

sys.path.insert(0, str(Path('scripts').resolve()))
from decoder_integrity import FrameHash, validate_decoder_line

OUT = Path('target/task1-validation').resolve()
OUT.mkdir(parents=True, exist_ok=True)


class IntegrityTests(unittest.TestCase):
    def test_interleaved_crc_error_rejected_without_error_prefix(self):
        line = '[info]       encoder         : Lavc62.11.100 pcm_s32leCRC error at PTS -9223372036854775808'
        with self.assertRaisesRegex(ValueError, 'ffmpeg integrity failure'):
            validate_decoder_line(line)

    def test_benign_decoder_metadata_and_zero_error_count_accepted(self):
        for line in ('[info]       encoder         : Lavc62.11.100 pcm_s32le',
                     '[info] 144000 samples decoded; 0 errors'):
            with self.subTest(line=line):
                validate_decoder_line(line)

    @classmethod
    def setUpClass(cls):
        # Distinct channel signals catch channel swaps; exact PCM is the digital reference.
        frames = np.column_stack((np.arange(144000, dtype=np.int32) % 30001 - 15000,
                                  np.arange(144000, dtype=np.int32) * 13 % 28001 - 14000)).astype('<i2')
        cls.reference = frames.astype('<i4') << 16
        raw = OUT / 'reference.s16le'
        raw.write_bytes(frames.tobytes())
        valid = OUT / 'complete.flac'
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-f', 's16le', '-ar', '48000',
                        '-ac', '2', '-i', str(raw), '-c:a', 'flac', str(valid)],
                       check=True, capture_output=True, timeout=10)
        packets = json.loads(subprocess.run(
            ['ffprobe', '-v', 'error', '-show_packets', '-of', 'json', str(valid)],
            check=True, capture_output=True, timeout=10).stdout)['packets']
        blob = valid.read_bytes()
        blocks = [blob[int(p['pos']):int(p['pos']) + int(p['size'])] for p in packets]
        header = blob[:int(packets[0]['pos'])]
        (OUT / 'omitted.flac').write_bytes(header + b''.join(blocks[:10] + blocks[20:]))
        (OUT / 'duplicate.flac').write_bytes(header + b''.join(blocks[:10] + [blocks[9]] + blocks[10:]))
        (OUT / 'tail-omitted.flac').write_bytes(header + b''.join(blocks[:-1]))
        corrupt = bytearray(blob)
        corrupt[int(packets[10]['pos']) + 6] ^= 1
        (OUT / 'corrupt.flac').write_bytes(corrupt)
        crc = bytearray(blob)
        crc[int(packets[10]['pos']) + int(packets[10]['size']) - 1] ^= 1
        (OUT / 'crc.flac').write_bytes(crc)
        wav_samples = np.column_stack((np.arange(144000, dtype=np.int32) * 19 % 7000001 - 3500000,
                                      np.arange(144000, dtype=np.int32) * 31 % 6000001 - 3000000))
        cls.wav_reference = wav_samples.astype('<i4') << 8
        wav_raw = OUT / 'reference24.s32le'
        wav_raw.write_bytes(cls.wav_reference.tobytes())
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-f', 's32le', '-ar', '48000',
                        '-ac', '2', '-i', str(wav_raw), '-c:a', 'pcm_s24le', str(OUT / 'complete.wav')],
                       check=True, capture_output=True, timeout=10)

    def run_harness(self, harness, fixture, suffix='flac', requested=3):
        progress = OUT / f'{harness}-{fixture}-{suffix}.progress'
        hashes = OUT / f'{harness}-{fixture}-{suffix}.framehash'
        errors = OUT / f'{harness}-{fixture}-{suffix}.ffmpeg.log'
        for path in (progress, hashes, errors):
            path.unlink(missing_ok=True)
        module = importlib.import_module(harness.replace('-', '_'))
        args = SimpleNamespace(seconds=requested, stream_port=8000, path=f'/stream.{suffix}')
        argv = (module.decoder_command(args, 'unused', progress, hashes, requested)
                if harness == 'fault-check' else module.decoder_command(args, 'unused', progress, hashes))
        argv[argv.index('-i') + 1] = str(OUT / f'{fixture}.{suffix}')
        with errors.open('wb') as log:
            process = subprocess.Popen(argv, stdout=subprocess.DEVNULL, stderr=log)
            try:
                process.wait(timeout=10)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=3)
        job = dict(label=fixture, process=process, progress=progress, hashes=hashes,
                   errors=errors, seconds=requested, decoded_seconds=0, decoded_audio_frames=0,
                   timeline_seconds=0, advanced_at=time.monotonic(), accounting=FrameHash(hashes))
        try:
            if harness == 'stream-soak':
                job.update(tail=module.Tail(progress), log_tail=module.Tail(errors))
                try:
                    module.update_decoder(job, args)
                finally:
                    job['tail'].close()
                    job['log_tail'].close()
            else:
                wait = functools.partial(module.wait_for, read_lines=lambda: [])
                with (OUT / f'{harness}-{fixture}.events.jsonl').open('w') as events:
                    def record(kind, **fields):
                        events.write(json.dumps(dict(kind=kind, **fields)) + '\n')
                    module.complete_decoder(job, wait, record)
        finally:
            job['accounting'].close()
        return job

    def test_complete_samples_counted_in_both_harnesses(self):
        for harness in ('stream-soak', 'fault-check'):
            with self.subTest(harness=harness):
                job = self.run_harness(harness, 'complete')
                self.assertEqual(job.get('decoded_audio_frames'), 144000,
                                 'must report samples actually emitted, not timestamp-derived samples')
                self.assert_hashes(job['hashes'], self.reference)

    def assert_hashes(self, path, reference):
        # Normalize packet boundaries against the contiguous digital source, without resampling.
        cursor = 0
        for line in path.read_text().splitlines():
            if not line or line.startswith('#'):
                continue
            fields = [part.strip() for part in line.split(',')]
            count = int(fields[4]) // 8
            self.assertEqual(fields[5], hashlib.sha256(reference[cursor:cursor + count].tobytes()).hexdigest())
            cursor += count
        self.assertEqual(cursor, 144000)

    def test_wav24_hashes_preserve_low_bits_and_channel_order(self):
        job = self.run_harness('fault-check', 'complete', 'wav')
        self.assertEqual(job['decoded_audio_frames'], 144000)
        self.assert_hashes(job['hashes'], self.wav_reference)

    def test_requested_duration_clips_final_packet_to_exact_samples(self):
        for harness in ('stream-soak', 'fault-check'):
            for requested, expected in [(1.12345, 53926), (1.00003125, 48002)]:
                with self.subTest(harness=harness, requested=requested):
                    job = self.run_harness(harness, 'complete', requested=requested)
                    self.assertEqual(job['decoded_audio_frames'], expected)

    def test_stream_check_capture_packet_boundary_allows_extra_audio_but_no_underflow(self):
        import stream_check
        capture = OUT / 'packet-copy.flac'
        argv = stream_check.capture_command(SimpleNamespace(seconds=1.03), str(OUT / 'complete.flac'), capture)
        subprocess.run(argv, check=True, capture_output=True, timeout=10)
        complete = stream_check.decode(capture)
        self.assertEqual(len(complete), 50688)
        self.assertTrue(stream_check.check_length(complete, 1.03)[0])
        tail = stream_check.decode(OUT / 'tail-omitted.flac')
        self.assertEqual(len(tail), 142848)
        self.assertFalse(stream_check.check_length(tail, 3)[0])

    def test_encoder_fault_marker_wait_covers_lookup_hold_and_delivery(self):
        import encoder_fault_check as encoder
        import threading
        self.assertEqual(encoder.SUSPEND_MARKER_WAIT_SECONDS, 30)
        with tempfile.TemporaryDirectory() as directory:
            uart = Path(directory) / 'uart.log'
            read_lines = functools.partial(encoder.read_uart_lines, uart)
            wait = functools.partial(encoder.wait_for, read_lines=read_lines,
                                     check_output_budget=lambda: encoder.check_budget(Path(directory)))
            # No writer exists during the timeout case, regardless of host scheduling.
            with self.assertRaisesRegex(RuntimeError, 'missing UART evidence'):
                encoder.match_uart('marker=SUSPEND', read_lines, wait, seconds=0)

            ready = threading.Event()
            release = threading.Event()
            def deliver():
                ready.set()
                if release.wait(timeout=5):
                    uart.write_text('encoder-faults: marker=SUSPEND ms=27000\n')
            worker = threading.Thread(target=deliver)
            worker.start()
            try:
                self.assertTrue(ready.wait(timeout=5), 'UART writer did not start')
                release.set()
                index, found = encoder.match_uart(r'encoder-faults: marker=SUSPEND ms=(\d+)',
                                                   read_lines, wait, seconds=5)
                self.assertEqual(index, 0)
                self.assertEqual(found.group(1), '27000')
            finally:
                release.set()
                worker.join(timeout=5)
            self.assertFalse(worker.is_alive(), 'UART writer did not finish')

    def test_encoder_fault_eof_allowance_covers_history_but_remains_bounded(self):
        from encoder_fault_check import eof_within_allowance as within
        self.assertTrue(within(115.0, 100.0))
        self.assertTrue(within(116.532, 100.0))
        self.assertFalse(within(116.533, 100.0))
        self.assertFalse(within(99.899, 100.0))
        self.assertFalse(within(None, 100.0))

    def test_encoder_fault_actual_decoder_rejects_missing_audio(self):
        progress = OUT / 'encoder-fault-omitted.progress'
        hashes = OUT / 'encoder-fault-omitted.framehash'
        error = OUT / 'encoder-fault-omitted.log'
        progress.unlink(missing_ok=True)
        hashes.unlink(missing_ok=True)
        import encoder_fault_check as encoder
        argv = encoder.decoder_command(SimpleNamespace(stream_port=8000), 'unused', progress, hashes)
        argv[argv.index('-i') + 1] = str(OUT / 'omitted.flac')
        argv[argv.index('-t') + 1] = '3'
        with error.open('wb') as log:
            subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=log, timeout=10)
        accounting = {progress: FrameHash(hashes)}
        try:
            with self.assertRaises(ValueError):
                encoder.decoder_duration(progress, accounting)
        finally:
            accounting[progress].close()

    def test_missing_duplicate_corrupt_and_crc_rejected_in_both_harnesses(self):
        for harness in ('stream-soak', 'fault-check'):
            for fixture in ('omitted', 'duplicate', 'corrupt', 'crc', 'tail-omitted'):
                with self.subTest(harness=harness, fixture=fixture):
                    with self.assertRaises(Exception) as caught:
                        self.run_harness(harness, fixture)
                    self.assertIn(type(caught.exception).__name__, ('ValueError', 'Failure'))
                    self.assertRegex(str(caught.exception).lower(),
                                     'ffmpeg|decoder|decode|discontinu|frame|crc|mismatch')

    def test_stream_check_retains_corruption_diagnostics(self):
        import stream_check
        self.assertEqual(len(stream_check.decode(OUT / 'complete.flac')), 144000)
        with self.assertRaises(Exception) as caught:
            stream_check.decode(OUT / 'corrupt.flac')
        self.assertRegex(str(caught.exception).lower(), 'padding|crc|decode|invalid|qlevel')

    def test_stream_check_rejects_all_integrity_negative_controls(self):
        import stream_check
        for fixture in ('omitted', 'duplicate', 'crc'):
            with self.subTest(fixture=fixture):
                with self.assertRaises(ValueError) as caught:
                    stream_check.decode(OUT / f'{fixture}.flac')
                self.assertRegex(str(caught.exception).lower(), 'crc|mismatch|dts')
                self.assertTrue(Path(str(OUT / f'{fixture}.flac') + '.decode.log').stat().st_size)
        wav = stream_check.decode(OUT / 'complete.wav')
        np.testing.assert_array_equal(wav, self.wav_reference >> 8)

    def test_incremental_framehash_ignores_partial_record_then_counts_it_once(self):
        job = self.run_harness('fault-check', 'complete')
        rows = job['hashes'].read_bytes().splitlines(keepends=True)
        first = next(i for i, row in enumerate(rows) if not row.startswith(b'#'))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'growing.framehash'
            path.write_bytes(b''.join(rows[:first]) + rows[first][:-1])
            counter = FrameHash(path)
            try:
                counter.update()
                self.assertEqual(counter.frames, 0)
                with path.open('ab') as output:
                    output.write(b'\n')
                counter.update()
                self.assertEqual(counter.frames, 4608)
                counter.update()
                self.assertEqual(counter.frames, 4608)
                with path.open('ab') as output:
                    output.write(b''.join(rows[first + 1:]))
                counter.update(final=True)
                self.assertEqual(counter.frames, 144000)
            finally:
                counter.close()

    def test_fault_decoder_budget_exact_boundary_retains_logs(self):
        import fault_check as fault
        job = self.run_harness('fault-check', 'complete')
        job['accounting'] = FrameHash(job['hashes'])
        try:
            budget = 128*1024*1024
            other = job['errors'].stat().st_size + job['hashes'].stat().st_size
            with job['progress'].open('r+b') as output:
                output.truncate(budget-other)
            self.assertEqual(fault.duration(job), 3)
            with job['progress'].open('r+b') as output:
                output.truncate(budget-other+1)
            with self.assertRaisesRegex(fault.Failure, 'decoder output exceeded 128 MiB; logs retained'):
                fault.duration(job)
            self.assertTrue(all(job[key].exists() for key in ('progress', 'errors', 'hashes')))
        finally:
            job['accounting'].close()
            # Keep later fixture runs small while preserving the other retained decoder evidence.
            job['progress'].unlink()

    def test_truncated_final_record_and_oversized_line_fail(self):
        job = self.run_harness('fault-check', 'complete')
        blob = job['hashes'].read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            for name, data in [('partial', blob[:-1]), ('oversized', b'x' * 17000), ('empty', b'')]:
                path = Path(directory) / name
                path.write_bytes(data)
                counter = FrameHash(path)
                try:
                    with self.assertRaises(ValueError):
                        counter.update(final=True)
                    self.assertLessEqual(len(counter.pending), 24576)
                finally:
                    counter.close()

    def test_tolerated_discontinuity_is_counted_and_the_read_continues(self):
        job = self.run_harness('fault-check', 'complete')
        rows = job['hashes'].read_bytes().splitlines(keepends=True)
        first = next(i for i, row in enumerate(rows) if not row.startswith(b'#'))
        # Drop one record from the middle: the next pts no longer follows the previous one.
        gapped = b''.join(rows[:first + 3] + rows[first + 4:])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'gapped.framehash'
            path.write_bytes(gapped)
            strict = FrameHash(path)
            try:
                with self.assertRaises(ValueError):
                    strict.update(final=True)
            finally:
                strict.close()
            counter = FrameHash(path, tolerate_discontinuities=True)
            try:
                counter.update(final=True)
                summary = counter.summary()
                self.assertEqual(summary['decoded_discontinuities'], 1)
                self.assertEqual(summary['decoded_audio_frames'], 144000 - 4608)
                self.assertEqual(summary['skipped_source_audio_frames'], 4608)
            finally:
                counter.close()

    def test_soak_final_flush_between_process_polls_is_counted(self):
        self.check_soak_completion_race(exit_between_polls=True)

    def test_soak_outer_completion_branch_performs_final_assertions(self):
        self.check_soak_completion_race(exit_between_polls=False)

    def check_soak_completion_race(self, exit_between_polls):
        progress = OUT / 'final-flush.progress'
        hashes = OUT / 'final-flush.framehash'
        errors = OUT / 'final-flush.ffmpeg.log'
        for path in (progress, hashes, errors):
            path.unlink(missing_ok=True)
        import stream_soak as soak
        args = SimpleNamespace(seconds=0.25, stream_port=8000, path='/stream.flac')
        argv = soak.decoder_command(args, 'unused', progress, hashes)
        argv[argv.index('-i') + 1] = str(OUT / 'complete.flac')
        argv.insert(argv.index('-i'), '-re')
        with errors.open('wb') as log:
            process = subprocess.Popen(argv, stdout=subprocess.DEVNULL, stderr=log)
            counter = FrameHash(hashes)
            if exit_between_polls:
                # Schedule the real decoder's final flush after the first real accounting read.
                # No poll/read result is replaced; wait creates the precise exit interleaving.
                class FinalFlush(FrameHash):
                    def update(self, final=False):
                        super().update(final=final)
                        if not final:
                            process.wait(timeout=10)
                counter = FinalFlush(hashes)
            job = dict(label='final-flush', process=process, accounting=counter,
                       tail=soak.Tail(progress), log_tail=soak.Tail(errors), errors=errors,
                       decoded_audio_frames=0, decoded_seconds=0, timeline_seconds=0,
                       advanced_at=time.monotonic())
            try:
                if exit_between_polls:
                    self.assertIsNone(process.poll())
                    soak.update_decoder(job, args)
                else:
                    process.wait(timeout=10)
                    self.assertTrue(soak.decoders_completed([job], args))
                self.assertEqual(job['decoded_audio_frames'], 12000)
            finally:
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=3)
                job['tail'].close()
                job['log_tail'].close()
                counter.close()


if __name__ == '__main__':
    unittest.main()
