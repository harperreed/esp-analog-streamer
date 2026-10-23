# ABOUTME: Tests control-fault ownership and evidence boundaries without board or speaker access.
# ABOUTME: Exercises the real event writer, UART parser and fail-closed acceptance predicates.
import importlib.util
import ast
import json
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
import copy
import contextlib
import http.server
import threading

spec = importlib.util.spec_from_file_location('control_check', Path(__file__).with_name('control-check.py'))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
URI = 'http://192.168.23.217:8000/stream.mp3?buffer=med&network_generation=2&sonos_intent=1'
RESET = b'ets Jul 29 2019 12:21:46\r\n\r\nrst:0x1 (POWERON_RESET),boot:0x1f (SPI_FAST_FLASH_BOOT)\r\n'


def uart(phases):
    return RESET + b''.join((f'I (20) module: control-fault: phase={phase} event_us={stamp} command_generation=3 sonos_intent=1 network_generation=2' + (' queue_item_id=item-1' if phase == 'identity' else ' error_kind=TimedOut' if phase == 'observation-error' else '') + '\r\n').encode() for phase, stamp in phases)


RETENTION = [('armed', 15000100), ('identity', 15000150), ('stall-start', 15000200), ('stall-end', 20050200), ('observation-error', 20050250), ('Degraded', 20050300), ('Recovering', 21500000), ('Playing', 21600000)]


class EvidenceTests(unittest.TestCase):
    def test_actual_cancel_close_batch_export_can_precede_stall_end(self):
        root = Path(__file__).parent
        result = json.loads((root / 'cancel/result.json').read_text())
        raw = (root / 'cancel/uart-observed.log').read_bytes()
        end = next(row['event_us'] for row in driver.markers(raw) if row['phase'] == 'stall-end')
        close_export = next(row for row in driver.listener_exports(raw.decode()) if row['last_us'] == 49310738)
        self.assertLess(close_export['export_us'], end)
        self.assertTrue(driver.cleanup_receipts_ready(raw, result['selected_identity']['speaker'], result['intent']))

    def test_actual_cancel_requires_export_covering_matching_close(self):
        root = Path(__file__).parent
        result = json.loads((root / 'cancel/result.json').read_text())
        raw = (root / 'cancel/uart-observed.log').read_bytes()
        batch = next(line for line in raw.splitlines(keepends=True) if b'listener-telemetry:' in line and b'observed_event_max_us=49310738 ' in line)
        missing = raw.replace(batch, b'')
        unrelated = raw.replace(batch, batch.replace(b'observed_event_min_us=49307623', b'observed_event_min_us=49310739')
                               .replace(b'observed_event_max_us=49310738', b'observed_event_max_us=49310740')
                               .replace(b'export_us=49790040', b'export_us=60000000'))
        incomplete = raw[:raw.index(batch)] + batch.rstrip(b'\r\n')
        for candidate in (missing, unrelated, incomplete):
            self.assertFalse(driver.cleanup_receipts_ready(candidate, result['selected_identity']['speaker'], result['intent']))

    def test_actual_cleanup_prefix_waits_for_trailing_loss_export(self):
        root = Path(__file__).parent
        result = json.loads((root / 'retention-two/result.json').read_text())
        prefix = (root / 'retention-two/uart-observed.log').read_bytes()
        full = (root / 'retention-two-uart.log').read_bytes()
        speaker = result['selected_identity']['speaker']
        intent = result['intent']
        self.assertFalse(driver.cleanup_receipts_ready(prefix, speaker, intent))
        self.assertTrue(driver.cleanup_receipts_ready(full, speaker, intent))

    def test_cleanup_drain_rejects_actual_malformed_or_lost_exports(self):
        root = Path(__file__).parent
        result = json.loads((root / 'retention-two/result.json').read_text())
        full = (root / 'retention-two-uart.log').read_bytes()
        for replacement in (b'listener-telemetry: dropped=1', b'listener-telemetry: dropped=invalid'):
            damaged = full.replace(b'listener-telemetry: dropped=0', replacement, 1)
            with self.assertRaises(ValueError):
                driver.cleanup_receipts_ready(damaged, result['selected_identity']['speaker'], result['intent'])

    def test_retention_requires_real_ordered_states_and_same_queue(self):
        rows = driver.markers(uart(RETENTION))
        self.assertTrue(driver.validate_recovery(rows, 1, 2, 'item-1', 'retention'))

    def test_missing_fault_is_failure(self):
        with self.assertRaises(ValueError):
            driver.validate_recovery([], 1, 2, 'item-1', 'retention')

    def test_changed_generation_or_queue_is_failure(self):
        rows = driver.markers(uart(RETENTION))
        for intent, generation, queue in [(2, 2, 'item-1'), (1, 3, 'item-1'), (1, 2, 'other')]:
            with self.assertRaises(ValueError):
                driver.validate_recovery(rows, intent, generation, queue, 'retention')

    def test_malformed_or_duplicate_uart_marker_fails_closed(self):
        for raw in [uart(RETENTION).replace(b'sonos_intent=1', b'sonos_intent=nope', 1), uart(RETENTION) + RESET]:
            with self.assertRaises(ValueError):
                driver.markers(raw)
        with self.assertRaises(ValueError):
            driver.validate_recovery(driver.markers(uart(RETENTION + [('stall-start', 30000000)])), 1, 2, 'item-1', 'retention')

    def test_partial_uart_line_is_held_until_complete(self):
        raw = uart(RETENTION)
        self.assertEqual(len(driver.markers(raw[:-2])), 7)

    def test_cancel_rejects_late_restore(self):
        phases = RETENTION[:5] + [('Stopped', 21000000), ('Playing', 22000000)]
        with self.assertRaises(ValueError):
            driver.validate_recovery(driver.markers(uart(phases).replace(b'error_kind=TimedOut', b'error_kind=Interrupted')), 1, 2, 'item-1', 'cancel')

    def test_actual_event_writer_serializes_snapshot_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            trial = driver.Trial.__new__(driver.Trial)
            trial.args = SimpleNamespace(out=Path(directory))
            trial.sequence = 0
            tree = ast.parse(Path(__file__).with_name('control-check.py').read_text())
            cls = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'ControlTrial')
            method = next(node for node in cls.body if isinstance(node, ast.FunctionDef) and node.name == 'snapshot')
            assignment = next(node for node in method.body if isinstance(node, ast.Assign))
            row = eval(compile(ast.Expression(assignment.value), 'control-check.py', 'eval'),
                       {'time': time, 'play_started': time.monotonic()})
            trial.event('snapshot', **row)
            receipt = json.loads((Path(directory) / 'events.jsonl').read_text())
            self.assertEqual(receipt['kind'], 'snapshot')
            self.assertEqual(receipt['sample_started_monotonic'], row['sample_started_monotonic'])
            self.assertGreaterEqual(receipt['monotonic'], row['sample_started_monotonic'])

    def test_cancel_accepts_actual_interrupted_error_and_stopped(self):
        raw = uart(RETENTION[:5] + [('Stopped', 21000000)]).replace(b'error_kind=TimedOut', b'error_kind=Interrupted')
        self.assertTrue(driver.validate_recovery(driver.markers(raw), 1, 2, 'item-1', 'cancel'))

    def test_retention_rejects_non_timeout_error(self):
        with self.assertRaises(ValueError):
            driver.validate_recovery(driver.markers(uart(RETENTION).replace(b'error_kind=TimedOut', b'error_kind=Other')), 1, 2, 'item-1', 'retention')

    def test_fresh_boot_command_generation_zero_is_valid(self):
        rows = driver.markers(uart(RETENTION).replace(b'command_generation=3', b'command_generation=0'))
        self.assertTrue(driver.validate_recovery(rows, 1, 2, 'item-1', 'retention'))

    def test_stall_uses_remaining_original_deadline_after_actual_get(self):
        phases = RETENTION[:3] + [('stall-end', 15050200), ('observation-error', 15050250),
                                 ('Degraded', 15050300), ('Recovering', 16500000), ('Playing', 16600000)]
        self.assertTrue(driver.validate_recovery(driver.markers(uart(phases)), 1, 2, 'item-1', 'retention'))

    def test_retention_observations_end_before_authorized_cleanup_stop(self):
        rows = driver.markers(uart(RETENTION + [('Stopped', 25000000)]))
        self.assertTrue(driver.validate_recovery(rows, 1, 2, 'item-1', 'retention', observation_end_us=24000000))

    def test_original_byte_offsets_survive_non_utf8_uart(self):
        raw = uart(RETENTION)
        insertion = raw.index(b'I (20)')
        altered = raw[:insertion] + b'\xff\n' + raw[insertion:]
        self.assertEqual(driver.markers(altered)[0]['offset'], insertion + 2)

    def test_actual_archived_metrics_shape_accepts_nullable_listener_loss(self):
        records = Path(__file__).parents[1] / 'oct13-passive/v2/trial/events.jsonl'
        values = [json.loads(line) for line in records.read_text().splitlines()]
        metrics = next(row['response'] for row in values if row.get('kind') == 'api' and row.get('path') == '/api/metrics')
        status = next(row['response'] for row in values if row.get('kind') == 'api' and row.get('path') == '/api/sonos')
        trial = driver.ControlTrial.__new__(driver.ControlTrial)
        trial.initial_config = status['config']
        trial.network_generation = metrics['network']['generation']
        trial.initial_settings = metrics['settings']
        trial.validate(status, metrics)

    def test_capture_gate_rejects_poststartup_dma_loss(self):
        clean = 'audio: frames=48000 queue_overflows=8 software_dropped_frames=0 read_errors=0 restarts=0 supervisor_restarts=0 overflow_restarts=0 discontinuities=0 health_restart_requests=0 internal_min_sum=1 psram_min_sum=1'
        with self.assertRaises(Exception):
            driver.capture_gate(clean + '\n' + clean.replace('frames=48000', 'frames=96000').replace('queue_overflows=8', 'queue_overflows=9'))
        self.assertEqual(driver.capture_gate(clean + '\n' + clean.replace('frames=48000', 'frames=96000'))['samples'], 2)

    def test_original_owner_must_survive_entire_retention_observation(self):
        trial = driver.ControlTrial.__new__(driver.ControlTrial)
        trial.args = SimpleNamespace(mode='retention')
        trial.intent = 1
        trial.network_generation = 2
        trial.queue = {'queueVersion': 'item-1'}
        trial.speaker = '192.168.23.99'
        trial.result = {'observation_end_us': 50_000_000, 'preplay_device_us': 100}
        trial.samples = [
            {'metrics_before': {'served_device_us': 10_000_000}, 'metrics_after': {'served_device_us': 11_000_000},
             'position_seconds': 1, 'board': {'state': 'playing'}, 'transport': {'CurrentTransportState': 'PLAYING'}},
            {'metrics_before': {'served_device_us': 22_000_000}, 'metrics_after': {'served_device_us': 23_000_000},
             'position_seconds': 2, 'board': {'state': 'playing'}, 'transport': {'CurrentTransportState': 'PLAYING'}},
        ]
        clean = 'audio: frames=48000 queue_overflows=8 software_dropped_frames=0 read_errors=0 restarts=0 supervisor_restarts=0 overflow_restarts=0 discontinuities=0 health_restart_requests=0 internal_min_sum=1 psram_min_sum=1\n'
        tail = (clean + clean.replace('frames=48000', 'frames=96000') +
                'listener-telemetry: dropped=0 loss_occurred=false observed_event_range_valid=true observed_event_min_us=27000000 observed_event_max_us=27000000 export_us=52000000\n' +
                'stream: 192.168.23.99:1234 joined /stream.mp3 (1 of 2) event_us=200 owner_id=1 sonos_intent=1\n' +
                'stream-listener: peer=192.168.23.99:1234 frames_sent=12 bytes_sent=120 event_us=26000000\n' +
                'stream: 192.168.23.99:1234 socket closed event_us=27000000 owner_id=1 sonos_intent=1\n')
        raw = uart(RETENTION) + tail.encode()
        with self.assertRaisesRegex(ValueError, 'original media owner'):
            trial.evaluate(raw, driver.markers(raw))
        passing = raw.replace(b'event_us=26000000', b'event_us=50000000').replace(b'27000000', b'51000000')
        trial.evaluate(passing, driver.markers(passing))


class MetadataTests(unittest.TestCase):
    @contextlib.contextmanager
    def serve(self, missing):
        directory = Path(__file__).parent
        failed = json.loads((directory / 'retention/failed-snapshot.json').read_text())['metrics_before']
        events = [json.loads(line) for line in (directory / 'retention/events.jsonl').read_text().splitlines()]
        good = next(row['response'] for row in events if row.get('kind') == 'api'
                    and row.get('path') == '/api/metrics' and row['response']['network'] is not None
                    and row['response']['settings'] is not None)
        first = copy.deepcopy(failed if missing == 'network' else good)
        first[missing] = None
        replies = [first, good]
        requests = []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                requests.append(self.path)
                body = json.dumps(replies[min(len(requests) - 1, len(replies) - 1)]).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_):
                pass

        with tempfile.TemporaryDirectory() as temporary:
            with http.server.HTTPServer(('127.0.0.1', 0), Handler) as server:
                thread = threading.Thread(target=server.serve_forever)
                thread.start()
                try:
                    args = SimpleNamespace(out=Path(temporary) / 'receipt', image=directory / 'diagnostic.elf',
                                           host=f'127.0.0.1:{server.server_port}', mode='retention',
                                           boot_token=good['boot_token'])
                    trial = driver.ControlTrial(args)
                    yield trial, replies, requests, good
                finally:
                    server.shutdown()
                    thread.join()

    def test_actual_null_network_response_requires_fresh_metadata(self):
        with self.serve('network') as (trial, replies, requests, good):
            self.assertEqual(trial.attest()['network'], good['network'])
            self.assertEqual(requests, ['/api/metrics', '/api/metrics'])
            rows = [json.loads(line) for line in (trial.args.out / 'events.jsonl').read_text().splitlines()]
            self.assertTrue(any(row.get('kind') == 'metadata-gap' and row['missing'] == ['network'] for row in rows))

    def test_null_settings_response_requires_fresh_metadata(self):
        with self.serve('settings') as (trial, replies, requests, good):
            self.assertEqual(trial.attest()['settings'], good['settings'])
            self.assertEqual(len(requests), 2)

    def test_persistent_null_fails_within_existing_deadline(self):
        with self.serve('network') as (trial, replies, requests, good):
            replies[:] = [replies[0]]
            trial.deadline = time.monotonic() + .15
            deadline = trial.deadline
            with self.assertRaisesRegex(TimeoutError, 'metadata'):
                trial.attest()
            self.assertEqual(trial.deadline, deadline)

    def test_changed_network_is_rejected_without_retry(self):
        with self.serve('network') as (trial, replies, requests, good):
            replies[:] = [good]
            trial.network_generation = good['network']['generation'] + 1
            with self.assertRaisesRegex(ValueError, 'network lease'):
                trial.attest()
            self.assertEqual(len(requests), 1)


class OwnershipTests(unittest.TestCase):
    def test_same_uri_from_previous_boot_is_owned_after_current_boot_play(self):
        self.assertEqual(driver.current_boot_uri(URI, '192.168.23.217', 'PLAYING', URI, True, True), (URI, 1))

    def test_old_uri_without_accepted_current_boot_play_never_grants_stop(self):
        self.assertIsNone(driver.current_boot_uri(URI, '192.168.23.217', 'PLAYING', URI, False, True))
        self.assertIsNone(driver.current_boot_uri(URI, '192.168.23.217', 'PLAYING', URI, True, False))

    def test_stopped_or_unpublished_track_does_not_establish_ownership(self):
        self.assertIsNone(driver.current_boot_uri(URI, '192.168.23.217', 'STOPPED', URI, True, True))
        self.assertIsNone(driver.current_boot_uri(URI, '192.168.23.217', 'PLAYING', 'foreign', True, True))


if __name__ == '__main__':
    unittest.main()
