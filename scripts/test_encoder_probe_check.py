# ABOUTME: Checks encoder probe accounting and evidence handling without board access.
# ABOUTME: Synthetic probe records test contracts; saved normal UART remains separate evidence.
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import wave

SPEC = importlib.util.find_spec('encoder_probe_check')
if SPEC:
    import encoder_probe_check as probe


def records():
    """Synthetic contract data, never device/HIL evidence."""
    rows = [(0.0, 'encoder-probe-storage: sequence=1 baseline_pending=true raw_payload=(0x1000, 4096) raw_stamps=(0x3000, 128) chunk=(16384, 1920, 1920) output_address=20000 output_len=0 output_capacity=960 output_required=960 shine_policy=MALLOC_CAP_SPIRAM|MALLOC_CAP_8BIT mp3=Some((30000, 4608, EncoderReceipt { config: AllocationReceipt { address: 40000, bytes: 1000 }, bitstream: AllocationReceipt { address: 50000, bytes: 4096 } }))')]
    rows.append((0.0, 'audio: frames=0 ' + ' '.join(f'{k}=0' for k in (
        'queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts',
        'supervisor_restarts', 'overflow_restarts', 'discontinuities', 'health_restart_requests'))))
    for i in range(24):
        phase = ('OFF1', 'ON', 'OFF2')[i // 8]
        n = i % 8 + 1
        on = phase == 'ON'
        rows.append(((i+1)*10.0, f'I ({(i+1)*10000}) stream-encode: elapsed_us=10000000 task_runtime_us=3000000 runtime_valid=true encoded_frames={(i+1)*400} resets=0 listeners_start=0 listeners_end=0 block_samples=1152 raw_read_overruns=0 raw_wait_total_us=5000000 encode_wall_total_us=2000000 encoded_lock_wait_wall_total_us=100 encoded_push_held_wall_total_us=200'))
        rows.append(((i+1)*10.0, f'encoder-probe: sequence=1 phase={phase} interval={n} retained={str(n>2).lower()} completed={str(i==23).lower()} valid=true success_attempt_us={100 if on else 0} rejected_attempt_us={20 if on else 0} pcm_push_us={300 if on else 0} delay_us={400 if on else 0} delay_calls={50 if on else 0} raw_remainder_us=Some({4999880 if on else 5000000}) residual_us=Some({2999000 if on else 2999700})'))
    rows.append((241.0, 'audio: frames=12000000 ' + ' '.join(f'{k}=0' for k in (
        'queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts',
        'supervisor_restarts', 'overflow_restarts', 'discontinuities', 'health_restart_requests'))))
    return rows


class Availability(unittest.TestCase):
    def test_module_available(self):
        self.assertIsNotNone(SPEC, 'importable bounded encoder probe harness is missing')


@unittest.skipUnless(SPEC, 'module not implemented yet')
class Accounting(unittest.TestCase):
    def test_phase_sums_use_consecutive_deltas(self):
        value = probe.analyze(records())
        self.assertTrue(value['valid_sequence'], value)
        phase = value['sequences'][0]['phases']['ON']
        self.assertEqual(phase['frame_delta'], 2400)
        self.assertEqual(phase['elapsed_us'], 60000000)
        self.assertAlmostEqual(phase['generation_ratio'], .96)
        self.assertEqual(phase['codec_wall_us_per_frame'], 5000)
        self.assertEqual(phase['delay_us'], 2400)
        self.assertEqual(value['performance_acceptance'], 'not assessed')

    def test_invalid_sequences_never_disappear(self):
        rows = records()
        rows.insert(8, (31, 'encoder-probe: invalidated position=(1, "OFF1", 4) frames=1200 resets=0'))
        value = probe.analyze(rows)
        self.assertFalse(value['valid_sequence'])
        self.assertTrue(value['sequences'][0]['errors'])
        self.assertEqual(len(value['sequences'][0]['windows']), 24)

    def test_rejects_missing_duplicate_reordered_and_malformed_rows(self):
        for change in ('missing', 'duplicate', 'reorder', 'negative', 'runtime', 'listener', 'arithmetic', 'retained', 'first-predecessor', 'capture'):
            with self.subTest(change=change):
                rows = records()
                if change == 'missing': del rows[6]
                if change == 'duplicate': rows.insert(7, rows[6])
                if change == 'reorder': rows[6], rows[8] = rows[8], rows[6]
                edits = {
                    'negative': ('encoded_frames=1600', 'encoded_frames=1'),
                    'runtime': ('runtime_valid=true', 'runtime_valid=false'),
                    'listener': ('listeners_end=0', 'listeners_end=1'),
                    'arithmetic': ('raw_remainder_us=Some(5000000)', 'raw_remainder_us=None'),
                    'retained': ('interval=3 retained=true', 'interval=3 retained=false'),
                    'capture': ('read_errors=0', 'read_errors=1'),
                }
                if change in edits:
                    a, b = edits[change]
                    index = len(rows)-1 if change == 'capture' else 7 if change == 'retained' else 9 if change == 'arithmetic' else 8
                    rows[index] = (rows[index][0], rows[index][1].replace(a, b))
                if change == 'first-predecessor': del rows[4:6]
                self.assertFalse(probe.analyze(rows)['valid_sequence'])

    def test_unpaired_tail_and_new_sequence_fail_independently(self):
        rows = records()
        rows.append((242, 'encoder-probe-storage: sequence=2 baseline_pending=true'))
        value = probe.analyze(rows)
        self.assertTrue(value['valid_sequence'])
        self.assertFalse(value['sequences'][1]['valid'])
        rows = records(); rows.append(rows[-3])
        self.assertFalse(probe.analyze(rows)['valid_sequence'])

    def test_state_matches_real_saved_receipts(self):
        saved = Path(__file__).resolve().parent/'fixtures/encoder-probe/state.json'
        receipt = json.loads(saved.read_text())
        probe.require_state(receipt['sonos'], receipt['app_state'])
        for key, replacement in [('shazam_enabled', True), ('shazam_enabled', None)]:
            app = dict(receipt['app_state']); app[key] = replacement
            with self.assertRaises(ValueError): probe.require_state(receipt['sonos'], app)

    def test_actual_switch_announces_mp3_after_fresh_storage_before_windows(self):
        rows = records()
        rows.insert(1, (0, 'stream: active format Mp3'))
        self.assertTrue(probe.analyze(rows)['valid_sequence'])
        rows = records(); rows.insert(10, (40, 'stream: active format Mp3'))
        self.assertFalse(probe.analyze(rows)['valid_sequence'])
        rows = records(); rows.insert(1, (0, 'stream: active format Flac'))
        self.assertFalse(probe.analyze(rows)['valid_sequence'])

    def test_complete_pairs_cannot_hide_early_or_invalid_windows(self):
        rows = records()
        metric, completed = rows[4], rows[5]
        completed = (completed[0], completed[1].replace('phase=OFF1 interval=2', 'phase=COMPLETE interval=1'))
        rows[4:4] = [metric, completed]
        self.assertFalse(probe.analyze(rows)['valid_sequence'])
        rows = records()
        metric = (242, rows[-3][1].replace('encoded_frames=9600', 'encoded_frames=10000'))
        completed = (242, rows[-2][1].replace('phase=OFF2 interval=8 retained=true completed=true',
                                             'phase=COMPLETE interval=1 retained=false completed=false'))
        rows.extend([metric, completed])
        self.assertTrue(probe.analyze(rows)['valid_sequence'])
        rows[-1] = (242, completed[1].replace('valid=true', 'valid=false'))
        self.assertFalse(probe.analyze(rows)['valid_sequence'])

    def test_shared_arrival_stamp_uses_line_order_for_capture_coverage(self):
        value = probe.analyze([(1, row[1]) for row in records()])
        self.assertTrue(value['valid_sequence'])
        self.assertEqual(value['sequences'][0]['capture_coverage']['start']['line'], 2)
        self.assertEqual(value['sequences'][0]['capture_coverage']['end']['line'], 51)

    def test_storage_missing_or_invalid_is_separate_failure(self):
        for original, replacement in [('output_capacity=960', 'output_capacity=959'),
                                      ('address: 40000', 'address: 0'),
                                      ('bytes: 4096', 'bytes: 0'),
                                      ('mp3=Some(', 'mp3=None(')]:
            rows = records(); rows[0] = (0, rows[0][1].replace(original, replacement))
            value = probe.analyze(rows)
            self.assertFalse(value['valid_sequence'])
            self.assertFalse(value['sequences'][0]['storage_valid'])
            self.assertTrue(value['sequences'][0]['phase_valid'])

    def test_wav_join_leave_contaminates_zero_listener_endpoints(self):
        rows = records(); rows.insert(10, (40, 'stream: 192.0.2.1:123 joined /stream.wav (1 listeners)'))
        self.assertFalse(probe.analyze(rows)['valid_sequence'])

    def test_import_and_wrapper_help_are_inert(self):
        root = Path(__file__).resolve().parent
        code = """
import sys
sys.path.insert(0, sys.argv[1])
def audit(event, args):
    if event.startswith(('socket.', 'subprocess.')) or event in ('os.mkdir', 'os.system'):
        raise RuntimeError('unexpected import action: ' + event)
sys.addaudithook(audit)
import encoder_probe_check
"""
        with tempfile.TemporaryDirectory() as directory:
            command = subprocess.run([sys.executable, '-B', '-c', code, str(root)], cwd=directory,
                                     capture_output=True, text=True, timeout=5)
            self.assertEqual(command.returncode, 0, command.stderr)
            command = subprocess.run([str(root/'encoder-probe-check'), '--help'], cwd=directory,
                                     capture_output=True, text=True, timeout=5)
            self.assertEqual(command.returncode, 0, command.stderr)
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_missing_capture_progress_or_coverage_is_invalid(self):
        rows = records()
        rows[-1] = (241, rows[-1][1].replace('frames=12000000', 'frames=0'))
        self.assertFalse(probe.analyze(rows)['valid_sequence'])
        self.assertFalse(probe.analyze(records()[:-1])['valid_sequence'])

    def test_real_saved_production_metric_slice(self):
        path = Path(__file__).resolve().parent/'fixtures/encoder-probe/normal-uart.log'
        lines = probe.read_uart_lines(path)
        self.assertEqual(len(lines), 2)
        first, second = [probe.fields(line) for line in lines]
        self.assertEqual(second['encoded_frames'] - first['encoded_frames'], 417)
        self.assertEqual(second['resets'] - first['resets'], 0)
        self.assertEqual(second['elapsed_us'], 10007713)
        self.assertFalse(probe.analyze([(1, lines[0]), (2, lines[1])])['valid_sequence'])

    def test_global_deadline_does_not_restart(self):
        self.assertEqual(probe.remaining_timeout(100, now=92), 8)
        self.assertEqual(probe.remaining_timeout(100, now=99), 1)
        with self.assertRaises(TimeoutError): probe.remaining_timeout(100, now=100)
        with self.assertRaises(TimeoutError): probe.remaining_timeout(100, now=105)

    def test_request_absolute_timeout_kills_child_and_retains_error(self):
        import http.server
        import threading
        import time
        class Slow(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                time.sleep(1)
                self.send_response(200)
                self.end_headers()
            def log_message(self, *args): pass
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Slow)
        worker = threading.Thread(target=server.serve_forever, daemon=True); worker.start()
        try:
            start = time.monotonic()
            value = probe.fetch_status(f'http://127.0.0.1:{server.server_port}/', .1)
            self.assertIn('error', value)
            self.assertLess(time.monotonic()-start, .8)
        finally:
            server.shutdown(); server.server_close(); worker.join(2)

    def test_listener_logs_contaminate_even_with_zero_endpoint_counts(self):
        rows = records()
        rows.insert(10, (40, 'stream-listener: peer=redacted elapsed_us=1'))
        self.assertFalse(probe.analyze(rows)['valid_sequence'])

    def test_offline_arrivals_reconstruct_complete_records(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            (out/'uart.log').write_bytes(b'one\npart\xffial\ntrailing')
            (out/'uart-arrival.jsonl').write_text('\n'.join(json.dumps(row) for row in [
                {'host_monotonic': 1, 'offset': 0, 'bytes': 8},
                {'host_monotonic': 2, 'offset': 8, 'bytes': 13}])+'\n')
            self.assertEqual(probe.load_uart(out), [(1, 'one'), (2, 'part\ufffdial')])

    def test_cleanup_receipt_failure_cannot_leave_real_source_running(self):
        process = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        attempted = []
        def broken_receipt(kind, **values):
            attempted.append(kind)
            raise OSError('receipt write failed: ' + kind)
        try:
            with self.assertRaisesRegex(OSError, 'receipt write failed: source_cleanup_begin'):
                probe.stop_source(process, broken_receipt, Path('/nonexistent'))
            self.assertIsNotNone(process.poll())
            self.assertEqual(attempted, ['source_cleanup_begin', 'source_cleanup_end'])
        finally:
            if process.poll() is None: process.kill(); process.wait(3)

    def test_unstarted_thread_cleanup_still_terminates_real_source(self):
        import threading
        process = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        unstarted = threading.Thread(target=lambda: None)
        events = []
        try:
            probe.join_started(unstarted, 1)
            probe.stop_source(process, lambda kind, **kw: events.append((kind, kw)), Path('/nonexistent'))
            self.assertIsNotNone(process.poll())
            self.assertEqual([row[0] for row in events], ['source_cleanup_begin', 'source_cleanup_end'])
        finally:
            if process.poll() is None:
                process.kill(); process.wait(3)

    def test_real_process_exit_and_cleanup(self):
        process = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        events = []
        probe.stop_source(process, lambda kind, **kw: events.append((kind, kw)), Path('/nonexistent'))
        self.assertIsNotNone(process.poll())
        self.assertEqual([row[0] for row in events], ['source_cleanup_begin', 'source_cleanup_end'])
        with self.assertRaises(RuntimeError): probe.require_source(process)

    def test_short_fixture_and_help_have_no_output_side_effects(self):
        root = Path(__file__).resolve().parent
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            wav = out/'short.wav'
            with wave.open(str(wav), 'wb') as writer:
                writer.setnchannels(2); writer.setsampwidth(2); writer.setframerate(48000)
                writer.writeframes(b'\0'*192000)
            elf = out/'image.elf'; elf.write_bytes(b'operator provenance')
            for argv in (['--help'], ['--unknown'], ['127.0.0.1', '--elf', str(elf), '--fixture', str(wav), '--out', str(out/'run')]):
                command = subprocess.run([sys.executable, str(root/'encoder_probe_check.py'), *argv], capture_output=True, text=True, timeout=5)
                self.assertEqual(command.returncode, 0 if argv == ['--help'] else 2, command.stderr)
                self.assertFalse((out/'run').exists())


if __name__ == '__main__':
    unittest.main()
