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
    rows[0] = (rows[0][0], rows[0][1] + ' telemetry_epoch=0 telemetry_window=0 event_us=1 baseline_us=1')
    rows[1] = (rows[1][0], rows[1][1] + ' capture_begin_us=0 capture_end_us=0 settling_discarded_frames=0')
    for i in range(24):
        metadata = f' telemetry_epoch=0 telemetry_window={i+1} window_start_us={i*10000000+1} window_end_us={(i+1)*10000000+1}'
        for index in (2+2*i, 3+2*i):
            rows[index] = (rows[index][0], rows[index][1] + metadata + (' sequence=1' if index % 2 == 0 else ''))
    rows[-1] = (rows[-1][0], rows[-1][1] + ' capture_begin_us=241000001 capture_end_us=241000002 settling_discarded_frames=0')
    rows.insert(-1, (240.4, 'encoder-boundary: prior_epoch=0 reason=Startup frames=0 resets=0 raw_overruns=0 raw_wait_us=0 raw_attempts=0 telemetry_epoch=0 telemetry_window=0 event_us=1 baseline_us=1 format=Mp3'))
    rows.insert(-1, (240.5, 'encoder-telemetry: dropped=0 loss_occurred=false observed_event_range_valid=true observed_event_min_us=1 observed_event_max_us=240000001 record_bytes=400 queue_bytes=840 metrics_stack_free_bytes=1000 export_us=240500001 dropped_full=0 dropped_contended=0 dropped_poisoned=0'))
    return rows


def startup_records():
    """Current real startup shape: FLAC owner then persisted MP3 selection."""
    rows = records()
    mp3 = rows[0][1].replace('telemetry_epoch=0', 'telemetry_epoch=1').replace('event_us=1 baseline_us=1', 'event_us=2 baseline_us=2')
    flac = rows[0][1].split(' mp3=')[0].replace('sequence=1', 'sequence=0').replace('output_capacity=960 output_required=960', 'output_capacity=16399 output_required=16399') + ' mp3=None telemetry_epoch=0 telemetry_window=0 event_us=1 baseline_us=1'
    prefix = [
        (0, 'encoder-boundary: prior_epoch=0 reason=Startup frames=0 resets=0 raw_overruns=0 raw_wait_us=0 raw_attempts=0 telemetry_epoch=0 telemetry_window=0 event_us=1 baseline_us=1 format=Flac'),
        (0, flac),
        (0, 'encoder-boundary: prior_epoch=0 reason=Switched frames=0 resets=0 raw_overruns=0 raw_wait_us=0 raw_attempts=0 telemetry_epoch=1 telemetry_window=0 event_us=2 baseline_us=2 format=Mp3'),
        (0, 'encoder-probe: invalidated position=(0, "INACTIVE", 1) frames=0 resets=0 raw_overruns=0 raw_wait_us=0 raw_attempts=0 reason=Switched telemetry_epoch=0 telemetry_window=0 event_us=2'),
        (0, mp3),
        (0, 'stream: active format Mp3 telemetry_epoch=1 telemetry_window=0 event_us=2 baseline_us=2'),
    ]
    import re
    for stamp, line in rows[1:]:
        if 'encoder-boundary:' in line: continue
        line = line.replace('telemetry_epoch=0', 'telemetry_epoch=1')
        line = re.sub(r'(window_(?:start|end)_us=)([0-9]+)', lambda match: match[1]+str(int(match[2])+1), line)
        line = line.replace('observed_event_max_us=240000001', 'observed_event_max_us=240000002')
        prefix.append((stamp, line))
    return prefix


def analyze_file(rows):
    """Replay complete UART bytes and arrival receipts through the real file reader."""
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory); raw = bytearray(); arrivals = []
        for stamp, line in rows:
            data = (line.rstrip('\n') + '\n').encode()
            arrivals.append(dict(offset=len(raw), bytes=len(data), host_monotonic=stamp))
            raw.extend(data)
        (root/'uart.log').write_bytes(raw)
        (root/'uart-arrival.jsonl').write_text(''.join(json.dumps(row)+'\n' for row in arrivals))
        return probe.analyze(probe.load_uart(root))


class Routing(unittest.TestCase):
    # Actual amixer readbacks from listener-telemetry-device/trial-3.
    READBACKS = {
        'Headphone': (
            "Simple mixer control 'Headphone',0\n"
            '  Capabilities: pvolume pswitch\n'
            '  Playback channels: Front Left - Front Right\n'
            '  Limits: Playback 0 - 87\n'
            '  Mono:\n'
            '  Front Left: Playback 87 [100%] [0.00dB] [on]\n'
            '  Front Right: Playback 87 [100%] [0.00dB] [on]\n'
        ),
        'Line Out': (
            "Simple mixer control 'Line Out',0\n"
            '  Capabilities: pvolume pswitch\n'
            '  Playback channels: Front Left - Front Right\n'
            '  Limits: Playback 0 - 87\n'
            '  Mono:\n'
            '  Front Left: Playback 87 [100%] [0.00dB] [off]\n'
            '  Front Right: Playback 87 [100%] [0.00dB] [off]\n'
        ),
        'Master': (
            "Simple mixer control 'Master',0\n"
            '  Capabilities: pvolume pvolume-joined pswitch pswitch-joined\n'
            '  Playback channels: Mono\n'
            '  Limits: Playback 0 - 87\n'
            '  Mono: Playback 87 [100%] [0.00dB] [on]\n'
        ),
        'PCM': (
            "Simple mixer control 'PCM',0\n"
            '  Capabilities: pvolume\n'
            '  Playback channels: Front Left - Front Right\n'
            '  Limits: Playback 0 - 255\n'
            '  Mono:\n'
            '  Front Left: Playback 255 [100%] [0.00dB]\n'
            '  Front Right: Playback 255 [100%] [0.00dB]\n'
        ),
    }

    def test_declared_mono_and_stereo_routing_readbacks(self):
        for control, expected in (('Headphone', '[on]'), ('Line Out', '[off]'),
                                  ('Master', '[0.00dB]'), ('PCM', '[0.00dB]')):
            with self.subTest(control=control):
                probe.require_routing(control, expected, 0, self.READBACKS[control])

    def test_muted_master_rejects_zero_db_readback(self):
        muted = self.READBACKS['Master'].replace('[on]', '[off]')
        with self.assertRaises(ValueError):
            probe.require_routing('Master', '[0.00dB]', 0, muted)

    def test_each_declared_channel_requires_the_expected_value(self):
        for control, expected in (('Headphone', '[on]'), ('Line Out', '[off]'),
                                  ('Master', '[0.00dB]'), ('PCM', '[0.00dB]')):
            text = self.READBACKS[control]
            invalid = [text.replace(expected, '[wrong]', 1),
                       text.replace('Playback channels:', 'Unknown channels:')]
            if control != 'Master':
                missing_channel = '\n'.join(line for line in text.splitlines()
                                            if not line.strip().startswith('Front Right: Playback '))
                invalid.append(missing_channel.replace(expected, expected + ' ' + expected))
            for bad in invalid:
                with self.subTest(control=control, text=bad):
                    with self.assertRaises(ValueError):
                        probe.require_routing(control, expected, 0, bad)
            with self.assertRaises(ValueError):
                probe.require_routing(control, expected, 1, text)


class Availability(unittest.TestCase):
    def test_module_available(self):
        self.assertIsNotNone(SPEC, 'importable bounded encoder probe harness is missing')


@unittest.skipUnless(SPEC, 'module not implemented yet')
class Accounting(unittest.TestCase):
    def test_phase_sums_use_consecutive_deltas(self):
        value = analyze_file(records())
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
        value = analyze_file(rows)
        self.assertFalse(value['valid_sequence'])
        self.assertTrue(value['errors'])
        self.assertEqual(len(value['sequences'][0]['windows']), 24)

    def test_rejects_missing_duplicate_reordered_and_malformed_rows(self):
        for change in ('missing', 'duplicate', 'negative', 'runtime', 'listener', 'arithmetic', 'retained', 'first-predecessor', 'capture'):
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
                self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_unpaired_tail_and_new_sequence_fail_independently(self):
        rows = records()
        rows.append((242, 'encoder-probe-storage: sequence=2 baseline_pending=true'))
        value = analyze_file(rows)
        self.assertFalse(value['valid_sequence'])
        self.assertTrue(any('metadata' in error for error in value['errors']))
        rows = records(); rows.append(rows[-3])
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_state_matches_real_saved_receipts(self):
        saved = Path(__file__).resolve().parent/'fixtures/encoder-probe/state.json'
        receipt = json.loads(saved.read_text())
        probe.require_state(receipt['sonos'], receipt['app_state'])
        for key, replacement in [('shazam_enabled', True), ('shazam_enabled', None)]:
            app = dict(receipt['app_state']); app[key] = replacement
            with self.assertRaises(ValueError): probe.require_state(receipt['sonos'], app)

    def test_actual_switch_announces_mp3_after_matching_initial_boundary(self):
        self.assertTrue(analyze_file(startup_records())['valid_sequence'])
        rows = records(); rows.insert(1, (0, 'stream: active format Mp3 telemetry_epoch=0 telemetry_window=0 event_us=1 baseline_us=1'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records(); rows.insert(10, (40, 'stream: active format Mp3 telemetry_epoch=0 telemetry_window=0 event_us=40000001 baseline_us=40000001'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records(); rows.insert(1, (0, 'stream: active format Flac'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_complete_pairs_cannot_hide_early_or_invalid_windows(self):
        rows = records()
        metric, completed = rows[4], rows[5]
        completed = (completed[0], completed[1].replace('phase=OFF1 interval=2', 'phase=COMPLETE interval=1'))
        rows[4:4] = [metric, completed]
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records()
        metric = (242, next(line for _, line in reversed(rows) if 'stream-encode:' in line).replace('encoded_frames=9600', 'encoded_frames=10000').replace('telemetry_window=24', 'telemetry_window=25').replace('window_start_us=230000001 window_end_us=240000001', 'window_start_us=240000001 window_end_us=250000001'))
        completed = (242, next(line for _, line in reversed(rows) if 'encoder-probe: sequence=' in line).replace('telemetry_window=24', 'telemetry_window=25').replace('window_start_us=230000001 window_end_us=240000001', 'window_start_us=240000001 window_end_us=250000001').replace('phase=OFF2 interval=8 retained=true completed=true',
                                             'phase=COMPLETE interval=1 retained=false completed=false'))
        rows.extend([metric, completed, (243, rows[-2][1].replace('observed_event_max_us=240000001', 'observed_event_max_us=250000001').replace('export_us=240500001', 'export_us=251000001'))])
        self.assertTrue(analyze_file(rows)['valid_sequence'])
        rows[-2] = (242, completed[1].replace('valid=true', 'valid=false'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_shared_arrival_stamp_uses_line_order_for_capture_coverage(self):
        value = analyze_file([(1, row[1]) for row in records()])
        self.assertTrue(value['valid_sequence'])
        self.assertEqual(value['sequences'][0]['capture_coverage']['start']['line'], 2)
        self.assertEqual(value['sequences'][0]['capture_coverage']['end']['line'], 53)

    def test_storage_missing_or_invalid_is_separate_failure(self):
        for original, replacement in [('output_capacity=960', 'output_capacity=959'),
                                      ('address: 40000', 'address: 0'),
                                      ('bytes: 4096', 'bytes: 0'),
                                      ('mp3=Some(', 'mp3=None(')]:
            rows = records(); rows[0] = (0, rows[0][1].replace(original, replacement))
            value = analyze_file(rows)
            self.assertFalse(value['valid_sequence'])
            self.assertFalse(value['sequences'][0]['storage_valid'])
            self.assertTrue(value['sequences'][0]['phase_valid'])

    def test_wav_join_leave_contaminates_zero_listener_endpoints(self):
        rows = records(); rows.insert(10, (40, 'stream: 192.0.2.1:123 joined /stream.wav (1 listeners)'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

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
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        self.assertFalse(analyze_file(records()[:-1])['valid_sequence'])

    def test_real_saved_production_metric_slice(self):
        path = Path(__file__).resolve().parent/'fixtures/encoder-probe/normal-uart.log'
        lines = probe.read_uart_lines(path)
        self.assertEqual(len(lines), 2)
        first, second = [probe.fields(line) for line in lines]
        self.assertEqual(second['encoded_frames'] - first['encoded_frames'], 417)
        self.assertEqual(second['resets'] - first['resets'], 0)
        self.assertEqual(second['elapsed_us'], 10007713)
        self.assertFalse(probe.analyze([(1, lines[0]), (2, lines[1])])['valid_sequence'])

    def test_saved_device_rejection_replays_actual_order_and_accounting(self):
        saved = Path(__file__).resolve().parent/'fixtures/encoder-probe/device-rejected'
        provenance = json.loads((saved/'provenance.json').read_text())
        events = [json.loads(line) for line in (saved/'events.jsonl').read_text().splitlines()]
        self.assertEqual([event['kind'] for event in events], [
            'before_reset', 'reset_asserted', 'reset_released', 'after_boot',
            'final_before_source_cleanup', 'source_cleanup_begin', 'source_cleanup_end'])
        for event in events:
            if 'sonos' in event:
                probe.require_state(event['sonos'], event['app_state'])
        reset = events[2]['host_monotonic']
        self.assertIsNone(events[-2]['returncode'])
        self.assertEqual(events[-1]['returncode'], 0)
        self.assertFalse(provenance['completed'])
        self.assertFalse(provenance['acquisition_complete'])
        self.assertEqual(provenance['budget_seconds'], 360)
        self.assertIn('fixed global collection budget exhausted', provenance['errors'][0]['error'])
        rows = probe.load_uart(saved)
        self.assertTrue(all(reset <= stamp < events[-2]['host_monotonic'] for stamp, _ in rows))
        import hashlib
        raw = (saved/'uart.log').read_bytes()
        for mapping, line in zip(provenance['uart_mapping'], raw.splitlines(keepends=True), strict=True):
            self.assertEqual(hashlib.sha256(line).hexdigest(), mapping['sha256'])
            self.assertEqual(raw[mapping['fixture_offset']:mapping['fixture_offset']+mapping['bytes']], line)
            self.assertLessEqual(mapping['source_offset']+mapping['bytes'],
                                 mapping['source_arrival_offset']+mapping['source_arrival_bytes'])
        value = analyze_file(rows)
        self.assertFalse(value['valid_sequence'])
        self.assertEqual(value['performance_acceptance'], 'not assessed')
        self.assertTrue(any('raw-loss' in error for error in value['errors']), value['errors'])
        self.assertTrue(any('metadata' in error for error in value['errors']), value['errors'])
        # Archived evidence has no producer metadata and cannot be recertified.
        # Keep direct raw counter facts independent of the current acceptance parser.
        self.assertEqual([probe.fields(line)['sequence'] for _, line in rows if 'encoder-probe-storage:' in line], [1, 2, 3])
        losses = [line for _, line in rows if 'invalidated' in line]
        self.assertEqual(len(losses), 2)
        for line, device_ms, frames, resets in zip(losses, (166116, 346626), (6597, 14025), (1, 2)):
            self.assertIn(f'({device_ms})', line)
            self.assertIn(f'frames={frames} resets={resets} raw_overruns=1', line)

    def test_delayed_arrivals_pair_by_exact_owner_identity(self):
        rows = records()
        # Complete pairs and their members arrive out of order; original provenance survives.
        rows[2], rows[8] = rows[8], rows[2]
        rows[3], rows[17] = rows[17], rows[3]
        rows = [(999, line) for _, line in rows]
        value = analyze_file(rows)
        self.assertTrue(value['valid_sequence'], value)
        self.assertEqual(value['sequences'][0]['windows'][0]['metric_line'], 9)

    def test_strict_metadata_capture_bounds_and_export_tail(self):
        for old, replacement in [
            ('window_start_us=1 ', ''), ('telemetry_epoch=0 ', 'telemetry_epoch=0 telemetry_epoch=0 '),
            ('window_end_us=10000001', 'window_end_us=oops'),
            ('baseline_us=1', 'baseline_us=2'),
            ('capture_end_us=0', 'capture_end_us=30000001'),
            ('capture_begin_us=241000001', 'capture_begin_us=1'),
            ('settling_discarded_frames=0', 'settling_discarded_frames=1'),
            ('loss_occurred=false', 'loss_occurred=true'),
            ('dropped_full=0', 'dropped_full=1'),
        ]:
            with self.subTest(replacement=replacement):
                rows = records()
                index = next(i for i, (_, line) in enumerate(rows) if old in line)
                if old == 'settling_discarded_frames=0': index = len(rows)-1
                rows[index] = (rows[index][0], rows[index][1].replace(old, replacement, 1))
                self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records(); del rows[-2]
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records(); rows[-2] = (241, rows[-2][1].replace('export_us=240500001', 'export_us=bad'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_malformed_duplicate_core_fields_and_intervening_capture_never_disappear(self):
        for old, replacement in [('encoded_frames=400', 'encoded_frames=400 encoded_frames=bad'),
                                 ('listeners_start=0', 'listeners_start=false'),
                                 ('interval=1', 'interval=1 interval=bad'),
                                 ('frames=0 ', 'frames=0 frames=bad '),
                                 ('capture_end_us=0', 'capture_end_us=false')]:
            with self.subTest(replacement=replacement):
                rows = records(); index = next(i for i, (_, line) in enumerate(rows) if old in line)
                rows[index] = (rows[index][0], rows[index][1].replace(old, replacement, 1))
                self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records()
        middle = rows[1][1].replace('frames=0', 'frames=100').replace('capture_begin_us=0 capture_end_us=0', 'capture_begin_us=100000000 capture_end_us=100000002').replace('read_errors=0', 'read_errors=1')
        rows.insert(-2, (1000, middle))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_incomplete_metadata_valid_replacement_cannot_certify_prior_sequence(self):
        rows = records()
        rows.append((242, rows[0][1].replace('sequence=1', 'sequence=2').replace('telemetry_window=0', 'telemetry_window=24').replace('event_us=1 baseline_us=1', 'event_us=242000001 baseline_us=242000001')))
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records(); rows.insert(0, (0, 'rst:0x1 boot reset'))
        self.assertTrue(analyze_file(rows)['valid_sequence'])
        rows = records(); rows.append((242, 'rst:0x1 unexpected reset'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_combined_boundary_cannot_hide_loss_stop_or_malformed_lifecycle(self):
        for reason, counters in [('Overrun', 'resets=1 raw_overruns=1'), ('Stopped', 'resets=0 raw_overruns=0'),
                                 ('Switched', 'resets=0 raw_overruns=0')]:
            rows = records()
            rows.insert(-2, (240.1, f'encoder-boundary: prior_epoch=0 reason={reason} frames=9600 {counters} raw_wait_us=9 raw_attempts=9 telemetry_epoch=0 telemetry_window=24 event_us=240000002 baseline_us=240000002 format=Mp3'))
            value = analyze_file(rows)
            self.assertFalse(value['valid_sequence'])
            self.assertTrue(any('boundary' in error or 'raw-loss' in error for error in value['errors']))
        rows = records(); rows.insert(-2, (240.1, 'encoder-boundary: reason=garbage'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = [(stamp, line) for stamp, line in records() if 'encoder-boundary:' not in line]
        self.assertFalse(analyze_file(rows)['valid_sequence'])
        rows = records(); rows.insert(1, (0, 'stream: active format Mp3 telemetry_epoch=999 telemetry_window=999 event_us=2 baseline_us=2'))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_storage_raw_duplicate_tokens_and_malformed_values_never_hide(self):
        for suffix in [' baseline_pending=garbage', ' output_capacity=garbage', ' raw_payload=(0, 0)',
                       ' raw_stamps=garbage', ' chunk=(1, 2, 3)', ' mp3=garbage',
                       ' shine_policy=garbage']:
            with self.subTest(suffix=suffix):
                rows = records(); rows[0] = (0, rows[0][1] + suffix)
                self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_full_actual_startup_is_valid_and_every_lifecycle_partner_is_required(self):
        rows = startup_records()
        self.assertTrue(analyze_file(rows)['valid_sequence'], analyze_file(rows)['errors'])
        for index in range(6):
            with self.subTest(omitted=index):
                rows = startup_records(); del rows[index]
                self.assertFalse(analyze_file(rows)['valid_sequence'])
        for original, replacement in [('position=(0, "INACTIVE", 1)', 'position=(1, "OFF1", 1)'),
                                      ('reason=Switched', 'reason=Overrun'),
                                      ('telemetry_epoch=1', 'telemetry_epoch=99'),
                                      ('baseline_us=2', 'baseline_us=3')]:
            rows = startup_records(); rows[3 if 'position' in original else 2] = (0, rows[3 if 'position' in original else 2][1].replace(original, replacement))
            self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_storage_receipt_cannot_match_another_keys_suffix(self):
        rows = records()
        rows[0] = (0, rows[0][1].replace('mp3=Some(', 'mp3=garbage xmp3=Some('))
        self.assertFalse(analyze_file(rows)['valid_sequence'])

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
        self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_export_loss_and_delayed_listener_activity_reject_probe_acceptance(self):
        for line in ['listener-telemetry: dropped=1 observed_event_range_valid=false',
                     'stream: 127.0.0.1:9 left after 1.0 s: stopped reading event_us=1250000']:
            rows = records() + [(1000, line)]
            self.assertFalse(analyze_file(rows)['valid_sequence'])

    def test_probe_rejects_incomplete_or_ambiguous_listener_batch_receipts(self):
        receipt=('listener-telemetry: dropped=0 loss_occurred=false observed_event_range_valid=true '
                 'observed_event_min_us=1000 observed_event_max_us=2000 record_bytes=248 '
                 'queue_bytes=2016 metrics_stack_free_bytes=512 export_us=3000')
        self.assertTrue(analyze_file(records()+[(1000,receipt)])['valid_sequence'])
        for invalid in [receipt.replace('loss_occurred=false ', ''),
                        receipt.replace('observed_event_range_valid=true','observed_event_range_valid=trueBAD'),
                        receipt+' observed_event_min_us=0',
                        receipt.replace('observed_event_min_us=1000','observed_event_min_us=2001'),
                        receipt.replace('observed_event_max_us=2000','observed_event_max_us=18446744073709551616')]:
            with self.subTest(receipt=invalid):
                self.assertFalse(analyze_file(records()+[(1000,invalid)])['valid_sequence'])

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
