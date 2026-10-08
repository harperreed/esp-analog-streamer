# ABOUTME: Tests actual harness decoder paths with deterministic FFmpeg FLAC and WAV fixtures.
# ABOUTME: Retains fixtures in target/task1-validation and uses real subprocesses, never network APIs.
import ast
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
from stream_format import input_format_args, validate_flac_decoder_log
from decoder_integrity import FrameHash

OUT = Path('target/task1-validation').resolve()
OUT.mkdir(parents=True, exist_ok=True)


def tree(name):
    text = Path('scripts', name).read_text().split("<<'PY'\n", 1)[1].rsplit('\nPY', 1)[0]
    return ast.parse(text)


def load(name, names, namespace):
    nodes = [node for node in ast.walk(tree(name))
             if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in names]
    assert len(nodes) == len(names)
    exec(compile(ast.Module(body=nodes, type_ignores=[]), name, 'exec'), namespace)
    return namespace


def command(name, namespace):
    if name == 'stream-soak':
        call = next(node for node in ast.walk(tree(name)) if isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Attribute) and node.func.attr == 'Popen'
                    and node.args and isinstance(node.args[0], ast.List)
                    and isinstance(node.args[0].elts[0], ast.Constant)
                    and node.args[0].elts[0].value == 'ffmpeg')
        return eval(compile(ast.Expression(call.args[0]), name, 'eval'), namespace)
    decoder = next(node for node in ast.walk(tree(name))
                   if isinstance(node, ast.FunctionDef) and node.name == 'decoder')
    start = next(i for i, node in enumerate(decoder.body) if isinstance(node, ast.Assign)
                 and any(isinstance(target, ast.Name) and target.id == 'command' for target in node.targets))
    end = next(i for i, node in enumerate(decoder.body) if isinstance(node, ast.Assign)
               and any(isinstance(target, ast.Name) and target.id == 'process' for target in node.targets))
    exec(compile(ast.Module(body=decoder.body[start:end], type_ignores=[]), name, 'exec'), namespace)
    return namespace['command']


class IntegrityTests(unittest.TestCase):
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
        ns = dict(subprocess=subprocess, Path=Path, time=time, re=__import__('re'), json=json,
                  ip='unused', args=SimpleNamespace(seconds=requested, stream_port=8000, path=f'/stream.{suffix}'),
                  progress=progress, hashes=hashes, seconds=requested,
                  validate_flac_decoder_log=validate_flac_decoder_log)
        # Import only helpers the production script itself imports, to exercise shared paths.
        for node in tree(harness).body:
            if isinstance(node, ast.ImportFrom) and node.module in ('stream_format', 'decoder_integrity'):
                exec(compile(ast.Module(body=[node], type_ignores=[]), harness, 'exec'), ns)
        argv = command(harness, ns)
        argv[argv.index('-i') + 1] = str(OUT / f'{fixture}.{suffix}')
        with errors.open('wb') as log:
            process = subprocess.Popen(argv, stdout=subprocess.DEVNULL, stderr=log)
            try:
                process.wait(timeout=10)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=3)
        ns.update(uart=OUT / 'absent-uart', started=time.monotonic())
        job = dict(label=fixture, process=process, progress=progress, hashes=hashes,
                   errors=errors, seconds=requested, decoded_seconds=0, decoded_audio_frames=0,
                   timeline_seconds=0, advanced_at=time.monotonic())
        load(harness, ['Failure'], ns)
        if harness == 'stream-soak':
            load(harness, ['Tail', 'decoder_updates'], ns)
            job.update(tail=ns['Tail'](progress), log_tail=ns['Tail'](errors))
            if 'FrameHash' in ns:
                job['accounting'] = ns['FrameHash'](hashes)
            try:
                ns['decoder_updates'](job)
            finally:
                job['tail'].close()
                job['log_tail'].close()
                if job.get('accounting'):
                    job['accounting'].close()
            return job
        with (OUT / f'{harness}-{fixture}.events.jsonl').open('w') as events:
            ns.update(event_file=events)
            load(harness, ['lines', 'wait', 'record', 'duration', 'completed'], ns)
            if 'FrameHash' in ns:
                job['accounting'] = ns['FrameHash'](hashes)
            try:
                ns['completed'](job)
            finally:
                if job.get('accounting'):
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
        ns = dict(np=np, subprocess=subprocess, RATE=48000, TOLERANCE_FRAMES=4800)
        for node in tree('stream-check').body:
            if isinstance(node, ast.ImportFrom) and node.module in ('stream_format', 'decoder_integrity'):
                exec(compile(ast.Module(body=[node], type_ignores=[]), 'stream-check', 'exec'), ns)
        load('stream-check', ['decode', 'check_length'], ns)
        capture = OUT / 'packet-copy.flac'
        call = next(node for node in ast.walk(tree('stream-check')) if isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Attribute) and node.func.attr == 'run'
                    and node.args and isinstance(node.args[0], ast.List)
                    and node.args[0].elts[0].value == 'ffmpeg' and '-t' in [
                        value.value for value in node.args[0].elts if isinstance(value, ast.Constant)])
        args = dict(STALL_TIMEOUT_US=10000000, args=SimpleNamespace(seconds=1.03),
                    url=str(OUT / 'complete.flac'), capture_path=capture)
        argv = eval(compile(ast.Expression(call.args[0]), 'stream-check', 'eval'), args)
        subprocess.run(argv, check=True, capture_output=True, timeout=10)
        complete = ns['decode'](capture)
        self.assertEqual(len(complete), 50688)
        self.assertTrue(ns['check_length'](complete, 1.03)[0])
        tail = ns['decode'](OUT / 'tail-omitted.flac')
        self.assertEqual(len(tail), 142848)
        self.assertFalse(ns['check_length'](tail, 3)[0])

    def test_encoder_fault_marker_wait_covers_lookup_hold_and_delivery(self):
        # Run the shipped matcher and its actual SUSPEND call against a UART file.
        # Advance only the clock; no hardware or real 30-second wait is needed.
        call = next(node.value for node in ast.walk(tree('encoder-fault-check'))
                    if isinstance(node, ast.Assign) and isinstance(node.value, ast.Call)
                    and isinstance(node.value.func, ast.Name) and node.value.func.id == 'match'
                    and isinstance(node.value.args[0], ast.Constant)
                    and 'marker=SUSPEND' in node.value.args[0].value)
        for delivered_at, succeeds in [(29.5, True), (30.5, False)]:
            with tempfile.TemporaryDirectory() as directory:
                uart = Path(directory) / 'uart.log'
                elapsed = [0.0]
                def sleep(seconds):
                    elapsed[0] += seconds
                    if elapsed[0] >= delivered_at:
                        uart.write_text('encoder-faults: marker=SUSPEND ms=27000\n')
                ns = dict(time=SimpleNamespace(monotonic=lambda: elapsed[0], sleep=sleep),
                          uart=uart, out=Path(directory), OUTPUT_BUDGET=1024,
                          re=__import__('re'), armed=-1)
                load('encoder-fault-check', ['match', 'wait', 'lines', 'check_output_budget'], ns)
                expression = compile(ast.Expression(call), 'encoder-fault-check', 'eval')
                if succeeds:
                    try:
                        index, found = eval(expression, ns)
                    except RuntimeError as error:
                        self.fail(f'valid deferred marker exceeded its wait: {error}')
                    self.assertEqual(index, 0)
                    self.assertEqual(found.group(1), '27000')
                else:
                    with self.assertRaisesRegex(RuntimeError, 'missing UART evidence'):
                        eval(expression, ns)

    def test_encoder_fault_eof_allowance_covers_history_but_remains_bounded(self):
        ns = load('encoder-fault-check', ['eof_within_allowance'], {})
        within = ns['eof_within_allowance']
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
        ns = dict(progress=progress, hashes=hashes, ip='unused', a=SimpleNamespace(stream_port=8000),
                  subprocess=subprocess, re=__import__('re'), FrameHash=FrameHash)
        call = next(node for node in ast.walk(tree('encoder-fault-check')) if isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Attribute) and node.func.attr == 'Popen'
                    and node.args and isinstance(node.args[0], ast.List)
                    and isinstance(node.args[0].elts[0], ast.Constant)
                    and node.args[0].elts[0].value == 'ffmpeg')
        argv = eval(compile(ast.Expression(call.args[0]), 'encoder-fault-check', 'eval'), ns)
        argv[argv.index('-i') + 1] = str(OUT / 'omitted.flac')
        argv[argv.index('-t') + 1] = '3'
        with error.open('wb') as log:
            subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=log, timeout=10)
        ns['accounting'] = {progress: FrameHash(hashes)}
        load('encoder-fault-check', ['duration'], ns)
        try:
            with self.assertRaises(ValueError):
                ns['duration'](progress)
        finally:
            ns['accounting'][progress].close()

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
        ns = dict(np=np, subprocess=subprocess, FULL_SCALE=2**23)
        for node in tree('stream-check').body:
            if isinstance(node, ast.ImportFrom) and node.module in ('stream_format', 'decoder_integrity'):
                exec(compile(ast.Module(body=[node], type_ignores=[]), 'stream-check', 'exec'), ns)
        load('stream-check', ['decode'], ns)
        self.assertEqual(len(ns['decode'](OUT / 'complete.flac')), 144000)
        with self.assertRaises(Exception) as caught:
            ns['decode'](OUT / 'corrupt.flac')
        self.assertRegex(str(caught.exception).lower(), 'padding|crc|decode|invalid|qlevel')

    def test_stream_check_rejects_all_integrity_negative_controls(self):
        ns = dict(np=np, subprocess=subprocess, FULL_SCALE=2**23)
        for node in tree('stream-check').body:
            if isinstance(node, ast.ImportFrom) and node.module in ('stream_format', 'decoder_integrity'):
                exec(compile(ast.Module(body=[node], type_ignores=[]), 'stream-check', 'exec'), ns)
        load('stream-check', ['decode'], ns)
        for fixture in ('omitted', 'duplicate', 'crc'):
            with self.subTest(fixture=fixture):
                with self.assertRaises(ValueError) as caught:
                    ns['decode'](OUT / f'{fixture}.flac')
                self.assertRegex(str(caught.exception).lower(), 'crc|mismatch|dts')
                self.assertTrue(Path(str(OUT / f'{fixture}.flac') + '.decode.log').stat().st_size)
        wav = ns['decode'](OUT / 'complete.wav')
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
        ns = dict(subprocess=subprocess, Path=Path, time=time, re=__import__('re'),
                  progress=progress, hashes=hashes, ip='unused',
                  args=SimpleNamespace(seconds=0.25, stream_port=8000),
                  FrameHash=FrameHash, validate_flac_decoder_log=validate_flac_decoder_log,
                  input_format_args=input_format_args)
        for node in tree('stream-soak').body:
            if isinstance(node, ast.ImportFrom) and node.module == 'decoder_integrity':
                exec(compile(ast.Module(body=[node], type_ignores=[]), 'stream-soak', 'exec'), ns)
        load('stream-soak', ['Failure', 'Tail', 'decoder_updates'], ns)
        argv = command('stream-soak', ns)
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
                       tail=ns['Tail'](progress), log_tail=ns['Tail'](errors), errors=errors,
                       decoded_audio_frames=0, decoded_seconds=0, timeline_seconds=0,
                       advanced_at=time.monotonic())
            try:
                if exit_between_polls:
                    self.assertIsNone(process.poll())
                    ns['decoder_updates'](job)
                else:
                    process.wait(timeout=10)
                    main = next(node for node in tree('stream-soak').body
                                if isinstance(node, ast.FunctionDef) and node.name == 'main')
                    completion = next(node for node in ast.walk(main) if isinstance(node, ast.If)
                                      and isinstance(node.test, ast.Call)
                                      and isinstance(node.test.func, ast.Name) and node.test.func.id == 'all'
                                      and isinstance(node.test.args[0], ast.GeneratorExp)
                                      and isinstance(node.test.args[0].generators[0].target, ast.Name)
                                      and node.test.args[0].generators[0].target.id == 'job')
                    loop = ast.While(test=ast.Constant(True), body=[completion, ast.Break()], orelse=[])
                    module = ast.fix_missing_locations(ast.Module(body=[loop], type_ignores=[]))
                    ns['jobs'] = [job]
                    exec(compile(module, 'stream-soak completion branch', 'exec'), ns)
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
