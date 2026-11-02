# ABOUTME: Tests numeric metrics validation, freshness and bounded real HTTP transport.
# ABOUTME: Uses only loopback sockets and never contacts a device.
import json
import contextlib
import io
import http.client
import socket
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import device_metrics as dm


def snapshot():
    return dict(schema_version=1, served_device_us=20_000_000, boot_token=None,
                build_attestation=None, capture=None, encoder=None,
                listeners=[dict(generation=0, state="unavailable", sample_us=0, media_age_us=None, write_us=None,
                                skips=None, frames_skipped=None, last_skip_us=None) for _ in range(2)],
                listener_loss=dict(observed_device_us=20_000_000, dropped=0,
                                   loss_occurred=False, metrics_stack_free_bytes=None), encoder_loss=None)


def stage_snapshot():
    value = snapshot()
    value['media_progress'] = dict(observed_device_us=20_000_000, operator_epoch=3, health='healthy',
        accepted=dict(generation=2, frames=100, observed_device_us=19_000_000),
        raw=dict(generation=2, committed_frames=90, publication_device_us=19_100_000),
        encoded=dict(operator_epoch=3, generation=4, sequence=5, raw_generation=2,
                     source_start=70, source_end=80, oldest_accepted_us=18_000_000,
                     raw_publication_us=18_100_000, production_device_us=19_000_000), backlog_frames=10)
    value['last_listener_departure'] = dict(observed_device_us=20_000_000, listener_stack_free_bytes=0,
        stats=dict(elapsed_us=5_000_000, payload_elapsed_us=4_000_000, frames_sent=2,
                   bytes_sent=1000, frame_write_wall_total_us=50, frame_write_wall_max_us=30,
                   frame_read_wait_wall_total_us=20, task_runtime_us=0, runtime_valid=True,
                   block_samples=1152, skips=1, frames_skipped=166, last_skip_us=19_000_000,
                   media=dict(last_age_us=100, max_age_us=200, queue_frames_max=2)))
    value['network'] = dict(generation=5, connected=True, disconnects=0)
    return value


# The largest body the firmware can serve: every counter at its type's maximum, both slots
# writing. core/tests/device_metrics.rs `maximal_response_fits` measures the firmware's own
# serialization at 4383 bytes; this fixture carries the same fields and values except
# backlog_frames, which the validator's raw-minus-encoded rule pins to zero at the maximum.
MEASURED_MAXIMAL_BODY_BYTES = 4383


def maximal_snapshot():
    item = snapshot(); maximum = (1 << 64) - 1
    item['served_device_us'] = maximum
    item.update(boot_token='fe' * 16, build_elf_sha256='fe' * 32,
                settings=dict(active_format='flac', next_play_format='flac', next_play_buffer='high',
                              buffer_available=False, autoplay=False, recognition_enabled=False, pga_gain=255))
    item['http_stack_free_bytes'] = (1 << 32) - 1
    item['capture'] = dict.fromkeys(('read_frames callbacks queue_overflows software_dropped_frames read_errors restarts supervisor_restarts overflow_restarts settling_discarded_frames discontinuities health_restart_requests read_service_delay_estimate_max_us raw_work_max_us meter_work_max_us store_work_max_us raw_read_max_us').split(), (1 << 32) - 1)
    item['capture'].update(dict.fromkeys('internal_free internal_largest internal_min_sum psram_free psram_largest psram_min_sum'.split(), maximum))
    item['capture'].update(capture_begin_us=maximum - 1, capture_end_us=maximum)
    item['encoder'] = dict.fromkeys('window_start_us window_end_us telemetry_epoch telemetry_window encoded_frames resets encode_max_us raw_read_frames raw_read_overruns raw_read_retries encode_wall_total_us raw_wait_total_us raw_wait_max_us encoded_lock_wait_wall_total_us encoded_push_held_wall_total_us'.split(), maximum)
    item['encoder'].update(kind='window', window_start_us=maximum - 1, block_samples=maximum, listeners_start=maximum, listeners_end=maximum, stack_free_bytes=(1 << 32) - 1, task_runtime_us=(1 << 32) - 1, runtime_valid=True)
    loss = dict(observed_device_us=maximum, dropped=(1 << 32) - 1, loss_occurred=True, metrics_stack_free_bytes=(1 << 32) - 1)
    item.update(listener_loss=loss, encoder_loss=dict(loss))
    extra = stage_snapshot()
    progress = extra['media_progress']; progress['observed_device_us'] = maximum
    progress['operator_epoch'] = (1 << 32) - 1
    for name in ('accepted', 'raw', 'encoded'):
        for key in progress[name]: progress[name][key] = maximum
    progress['encoded']['operator_epoch'] = (1 << 32) - 1
    progress['backlog_frames'] = 0
    departure = extra['last_listener_departure']; departure['observed_device_us'] = maximum
    departure['listener_stack_free_bytes'] = (1 << 32) - 1
    for key in departure['stats']:
        if key not in ('runtime_valid', 'task_runtime_us', 'media'): departure['stats'][key] = maximum
    departure['stats']['task_runtime_us'] = (1 << 32) - 1
    departure['stats']['media'] = dict(last_age_us=maximum, max_age_us=maximum, queue_frames_max=maximum)
    item.update(media_progress=progress, last_listener_departure=departure, network=dict(generation=(1 << 32) - 1, connected=True, disconnects=(1 << 32) - 1))
    item['listeners'] = [dict(generation=(1 << 32) - 1, state=state, sample_us=maximum, media_age_us=maximum, write_us=maximum,
                              skips=maximum, frames_skipped=maximum, last_skip_us=maximum) for state in ('writing', 'ready')]
    return item


class LiveListeners(unittest.TestCase):
    def test_live_values_fixed_slots_and_independent_source_freshness(self):
        item = snapshot()
        item['listeners'] = [dict(generation=9, state='writing', sample_us=19_000_000, media_age_us=500_000, write_us=300_000, skips=0, frames_skipped=0, last_skip_us=0),
                             dict(generation=12, state='ready', sample_us=1_000_000, media_age_us=700_000, write_us=400_000, skips=0, frames_skipped=0, last_skip_us=0)]
        item['listeners'][0]['secret'] = {'peer': ['x' * 4096]}
        cache = dm.Collector(stale=5); cache.accept(item, 10)
        text = cache.render(11)
        self.assertIn('device_listener_available{slot="1"} 1', text)
        self.assertIn('device_listener_available{slot="2"} 0', text)
        self.assertIn('device_listener_media_age_seconds{slot="1",state="writing"} 0.5', text)
        self.assertNotIn('state="ready"', text)
        item['listeners'][0]['media_age_us'] = 999
        item['listeners'][0]['secret'] = 'peer'
        self.assertIn('state="writing"} 0.5', cache.render(12))
        self.assertNotIn('secret', repr(cache.value))
        self.assertNotIn('generation=', text)
        self.assertIn('device_listener_generation{slot="1"} 9', text)
        self.assertNotIn('device_listener_write_seconds', cache.render(16))

    def test_ready_does_not_extrapolate_and_idle_differs_from_unknown(self):
        item = snapshot()
        item['listeners'][0].update(generation=1, state='ready', sample_us=20_000_000, media_age_us=100, write_us=0, skips=0, frames_skipped=0, last_skip_us=0)
        item['listeners'][1].update(state='idle', sample_us=20_000_000)
        cache = dm.Collector(); cache.accept(item, 10)
        text = cache.render(12)
        self.assertIn('device_listener_media_age_seconds{slot="1",state="ready"} 0.0001', text)
        self.assertIn('device_listener_active{slot="2"} 0', text)
        self.assertIn('device_listener_available{slot="2"} 1', text)
        item['served_device_us'] += 1; item['listeners'][1]['state'] = 'unavailable'
        cache.accept(item, 13)
        self.assertIn('device_listener_available{slot="2"} 0', cache.render(13))
        self.assertNotIn('device_listener_active{slot="2"}', cache.render(13))

    def test_http_stack_watermark_is_optional_bounded_and_fresh(self):
        for number in (None, 0, (1 << 32) - 1):
            item = snapshot(); item['http_stack_free_bytes'] = number
            cache = dm.Collector(); cache.accept(item, 10)
            item['http_stack_free_bytes'] = 99
            text = cache.render(10)
            if number is None: self.assertNotIn('device_http_stack_free_bytes', text)
            else: self.assertIn('device_http_stack_free_bytes ' + str(number), text)
            self.assertNotIn('device_http_stack_free_bytes', cache.render(41))
        for number in (True, -1, 1 << 32, '1'):
            item = snapshot(); item['http_stack_free_bytes'] = number
            with self.assertRaises(ValueError): dm.validate(item)

    def test_skip_counters_validated_projected_and_rendered(self):
        # The shape firmware/src/listener_observation.rs publishes: counters on every owner
        # publication (waiting carries zeros), null once the slot is idle.
        item = snapshot()
        item['listeners'][0].update(generation=4, state='writing', sample_us=19_000_000, media_age_us=500_000, write_us=300_000,
                                    skips=2, frames_skipped=333, last_skip_us=18_500_000)
        item['listeners'][1].update(generation=3, state='waiting', sample_us=19_000_000, skips=0, frames_skipped=0, last_skip_us=0)
        cache = dm.Collector(stale=5); cache.accept(item, 10)
        text = cache.render(11)
        self.assertIn('device_listener_skips{slot="1"} 2', text)
        self.assertIn('device_listener_skipped_frames{slot="1"} 333', text)
        self.assertIn('device_listener_last_skip_device_seconds{slot="1"} 18.5', text)
        self.assertIn('device_listener_skips{slot="2"} 0', text)
        # The owned copy keeps the counters it validated, never the caller's later mutations.
        item['listeners'][0]['skips'] = 999
        self.assertIn('device_listener_skips{slot="1"} 2', cache.render(11))
        self.assertEqual(cache.value['listeners'][0]['frames_skipped'], 333)
        waiting = snapshot(); waiting['listeners'][1].update(generation=3, state='waiting', sample_us=1)
        dm.validate(waiting)
        idle = snapshot(); idle['listeners'][1].update(state='idle', sample_us=1)
        dm.validate(idle)
        idle['served_device_us'] += 1
        cache.accept(idle, 12)
        self.assertNotIn('device_listener_skips', cache.render(12))
        departure = stage_snapshot(); cache = dm.Collector(); cache.accept(departure, 10)
        self.assertEqual(cache.value['last_listener_departure']['stats']['frames_skipped'], 166)
        for key, bad in [('skips', True), ('frames_skipped', -1), ('last_skip_us', None)]:
            item = stage_snapshot(); item['last_listener_departure']['stats'][key] = bad
            with self.subTest(key=key), self.assertRaises(ValueError): dm.validate(item)
        item = stage_snapshot(); del item['last_listener_departure']['stats']['skips']
        with self.assertRaises(ValueError): dm.validate(item)

    def test_invalid_listener_wire_rejected(self):
        for mutation in [lambda v: v.pop('listeners'), lambda v: v['listeners'].pop(),
                         lambda v: v.update(listeners={}),
                         lambda v: v['listeners'].__setitem__(0, None),
                         lambda v: v['listeners'][0].update(state='peer'),
                         lambda v: v['listeners'][0].update(generation=True),
                         lambda v: v['listeners'][0].update(generation=1 << 32),
                         lambda v: v['listeners'][0].update(sample_us=20_000_001),
                         lambda v: v['listeners'][0].update(media_age_us=0),
                         lambda v: v['listeners'][0].update(state='writing', generation=1, write_us=0),
                         lambda v: v['listeners'][0].update(state='ready', generation=1, write_us=0, media_age_us=-1),
                         # Skip counters mirror media_age_us: integers while writing or ready, null while idle.
                         lambda v: v['listeners'][0].update(state='writing', generation=1, write_us=0, media_age_us=0),
                         lambda v: v['listeners'][0].update(state='ready', generation=1, write_us=0, media_age_us=0, skips=0, frames_skipped=0),
                         lambda v: v['listeners'][0].update(state='ready', generation=1, write_us=0, media_age_us=0, skips=True, frames_skipped=0, last_skip_us=0),
                         lambda v: v['listeners'][0].update(state='ready', generation=1, write_us=0, media_age_us=0, skips=0, frames_skipped=-1, last_skip_us=0),
                         lambda v: v['listeners'][0].update(state='ready', generation=1, write_us=0, media_age_us=0, skips=0, frames_skipped=0, last_skip_us=1 << 64),
                         lambda v: v['listeners'][0].update(state='idle', skips=0),
                         lambda v: v['listeners'][0].pop('last_skip_us')]:
            item = snapshot(); mutation(item)
            with self.subTest(item=item), self.assertRaises(ValueError): dm.validate(item)


class Validation(unittest.TestCase):
    def test_good_zero_and_missing(self):
        cache = dm.Collector()
        cache.accept(snapshot(), 10)
        text = cache.render(10)
        self.assertIn('device_telemetry_dropped{family="listener_loss"} 0', text)
        self.assertIn('device_metrics_family_available{family="capture"} 0', text)

    def test_stale_repeated_poll_and_host_age(self):
        cache = dm.Collector()
        cache.accept(snapshot(), 10)
        self.assertNotIn('device_telemetry_dropped', cache.render(41))
        value = snapshot()
        value['served_device_us'] = 60_000_000
        cache.accept(value, 41)
        self.assertNotIn('device_telemetry_dropped', cache.render(41))

    def test_bad_types_future_and_reset(self):
        for field, value in [('schema_version', True), ('served_device_us', -1)]:
            item = snapshot(); item[field] = value
            with self.assertRaises(ValueError): dm.validate(item)
        item = snapshot(); item['listener_loss']['observed_device_us'] = 21_000_000
        with self.assertRaises(ValueError): dm.validate(item)
        cache = dm.Collector(); cache.accept(snapshot(), 1)
        item = snapshot(); item['served_device_us'] = 1; item['listener_loss'] = None
        cache.accept(item, 2)
        self.assertNotIn('device_telemetry_dropped', cache.render(2))

    def test_capture_encoder_maximum_and_units(self):
        item = snapshot()
        item['capture'] = dict.fromkeys('read_frames callbacks queue_overflows software_dropped_frames read_errors restarts supervisor_restarts overflow_restarts settling_discarded_frames discontinuities health_restart_requests read_service_delay_estimate_max_us raw_work_max_us meter_work_max_us store_work_max_us raw_read_max_us'.split(), (1 << 32) - 1)
        item['capture'].update(dict.fromkeys('internal_free internal_largest internal_min_sum psram_free psram_largest psram_min_sum'.split(), (1 << 32) - 1))
        item['capture'].update(capture_begin_us=1_000_000, capture_end_us=19_000_000)
        item['encoder'] = dict.fromkeys('window_start_us window_end_us telemetry_epoch telemetry_window encoded_frames resets encode_max_us raw_read_frames raw_read_overruns raw_read_retries encode_wall_total_us raw_wait_total_us raw_wait_max_us encoded_lock_wait_wall_total_us encoded_push_held_wall_total_us'.split(), 0)
        item['encoder'].update(kind='window', window_start_us=10_000_000,
                               window_end_us=19_000_000, encoded_frames=5,
                               block_samples=1152, listeners_start=0, listeners_end=1,
                               stack_free_bytes=0, task_runtime_us=0, runtime_valid=True)
        cache = dm.Collector(); cache.accept(item, 10)
        text = cache.render(10)
        self.assertIn('device_capture_read_pcm_frames 4294967295', text)
        self.assertIn('device_encoder_window_seconds 9', text)
        self.assertIn('device_encoder_task_runtime_seconds 0', text)
        self.assertIn('device_encoder_stack_free_bytes 0', text)
        self.assertIn('device_encoder_window_raw_pcm_frames 0', text)
        self.assertIn('device_encoder_window_raw_overruns 0', text)
        self.assertIn('device_encoder_window_resets 0', text)
        self.assertIn('device_capture_internal_minimum_sum_bytes 4294967295', text)
        self.assertIn('device_capture_psram_minimum_sum_bytes 4294967295', text)
        self.assertNotIn('device_metrics_owner_stack_free_bytes', text)
        item['encoder']['stack_free_bytes'] = None
        item['listener_loss']['metrics_stack_free_bytes'] = 0
        item['served_device_us'] += 1
        cache.accept(item, 10)
        self.assertNotIn('device_encoder_stack_free_bytes', cache.render(10))
        self.assertIn('device_metrics_owner_stack_free_bytes{family="listener_loss"} 0', cache.render(10))
        item = json.loads(json.dumps(item))
        item['served_device_us'] += 1
        item['encoder'] = dict(kind='boundary', event_us=19_000_000,
                               telemetry_epoch=0, telemetry_window=1, reason='stopped')
        cache.accept(item, 11)
        self.assertNotIn('device_encoder_window_codec_frames', cache.render(11))
        item = json.loads(json.dumps(item))
        item['served_device_us'] += 1
        item['encoder']['kind'] = 'failure'
        cache.accept(item, 12)
        self.assertNotIn('device_encoder_window_codec_frames', cache.render(12))

    def test_frozen_response_expires(self):
        cache = dm.Collector()
        cache.accept(snapshot(), 10)
        for now in [15, 20, 30, 40]: cache.accept(snapshot(), now)
        self.assertNotIn('device_telemetry_dropped', cache.render(41))
        self.assertIn('device_metrics_response_available 0', cache.render(41))
        advancing = snapshot(); advancing['served_device_us'] = 60_000_000
        cache.accept(advancing, 42)
        self.assertIn('device_metrics_response_available 1', cache.render(42))
        self.assertNotIn('device_telemetry_dropped', cache.render(42))

    def test_identity_metadata(self):
        item = snapshot(); item.update(boot_token='a' * 32, build_elf_sha256='b' * 64)
        dm.validate(item)
        for name, value in [('boot_token', 'x' * 32), ('build_elf_sha256', 'b' * 63)]:
            bad = dict(item); bad[name] = value
            with self.assertRaises(ValueError): dm.validate(bad)

    def test_accepted_snapshot_is_owned(self):
        item = snapshot(); cache = dm.Collector(); cache.accept(item, 10)
        item['listener_loss']['dropped'] = 999
        item['served_device_us'] = 999
        self.assertIn('device_telemetry_dropped{family="listener_loss"} 0', cache.render(10))

    def test_settings_and_device_id(self):
        settings = dict(active_format='mp3', next_play_format='flac', next_play_buffer='low',
                        buffer_available=True, autoplay=False, recognition_enabled=True, pga_gain=0)
        item = snapshot(); item['settings'] = settings
        dm.validate(item)
        for field, value in [('active_format', 'wav'), ('next_play_buffer', 'huge'),
                             ('pga_gain', 256), ('autoplay', 1)]:
            bad = dict(item); bad['settings'] = dict(settings); bad['settings'][field] = value
            with self.assertRaises(ValueError): dm.validate(bad)
        self.assertEqual(dm.device_id('line-in_1.x'), 'line-in_1.x')
        for name in ['', 'x' * 33, 'private host', 'é']:
            with self.assertRaises(ValueError): dm.device_id(name)

    def test_stage_departure_and_network(self):
        item = stage_snapshot(); cache = dm.Collector(); cache.accept(item, 10)
        text = cache.render(10)
        for metric in ['device_media_accepted_pcm_frames 100', 'device_media_published_pcm_frames 90',
                       'device_media_backlog_pcm_frames 10', 'device_media_encoded_sequence 5',
                       'device_media_encoded_age_seconds 1.0', 'device_last_listener_bytes_sent 1000',
                       'device_last_listener_stack_free_bytes 0', 'device_network_disconnects 0']:
            self.assertIn(metric, text)
        item['media_progress']['encoded']['sequence'] = 999
        item['last_listener_departure']['stats']['bytes_sent'] = 999
        self.assertNotIn(' 999', cache.render(10))
        item = stage_snapshot(); item['served_device_us'] += 1
        item['last_listener_departure']['stats'] = None
        item['media_progress'].update(accepted=None, raw=None, encoded=None, backlog_frames=None)
        cache.accept(item, 11); text = cache.render(11)
        self.assertNotIn('device_last_listener_bytes_sent', text)
        self.assertNotIn('device_media_accepted_pcm_frames', text)
        self.assertNotIn('device_network_disconnects', cache.render(42))

    def test_stage_source_null_and_stale(self):
        item = stage_snapshot(); item['media_progress']['accepted']['observed_device_us'] = None
        item['media_progress']['raw']['publication_device_us'] = None
        cache = dm.Collector(); cache.accept(item, 10)
        self.assertNotIn('device_media_accepted_pcm_frames', cache.render(10))
        self.assertNotIn('device_media_published_pcm_frames', cache.render(10))
        item = stage_snapshot(); item['served_device_us'] = 60_000_000
        item['media_progress']['observed_device_us'] = 60_000_000
        cache.accept(item, 11)
        self.assertNotIn('device_media_accepted_pcm_frames', cache.render(11))
        self.assertNotIn('device_media_encoded_sequence', cache.render(11))

    def test_accepted_and_raw_are_independent_source_samples(self):
        item = stage_snapshot()
        # Firmware reads accepted_progress before a separate raw.progress call.
        # The producer can accept and commit more frames between those reads.
        item['media_progress']['accepted']['frames'] = 80
        item['media_progress']['raw']['committed_frames'] = 90
        cache = dm.Collector(); cache.accept(item, 10)
        self.assertIn('device_media_accepted_pcm_frames 80', cache.render(10))
        self.assertIn('device_media_published_pcm_frames 90', cache.render(10))

    def test_stage_invalid_relations(self):
        for group, key, value in [('encoded', 'source_start', 81), ('encoded', 'production_device_us', 21_000_000),
                                  ('encoded', 'raw_generation', 999), ('encoded', 'source_end', 91),
                                  ('raw', 'publication_device_us', 21_000_000),
                                  ('accepted', 'observed_device_us', 21_000_000)]:
            item = stage_snapshot(); item['media_progress'][group][key] = value
            with self.assertRaises(ValueError): dm.validate(item)
        item = stage_snapshot(); item['last_listener_departure']['stats']['media']['last_age_us'] = 201
        with self.assertRaises(ValueError): dm.validate(item)
        item = stage_snapshot(); item['media_progress']['encoded']['raw_generation'] = 99
        item['media_progress']['backlog_frames'] = None
        cache = dm.Collector(); cache.accept(item, 10)
        self.assertNotIn('device_media_encoded_age_seconds', cache.render(10))
        self.assertNotIn('device_media_backlog_pcm_frames', cache.render(10))
        item = stage_snapshot(); item['network']['disconnects'] = True
        with self.assertRaises(ValueError): dm.validate(item)

    def test_target(self):
        for url in ['https://127.0.0.1/api/metrics', 'http://localhost/api/metrics',
                    'http://a:b@127.0.0.1/api/metrics', 'http://127.0.0.1/foo']:
            with self.assertRaises(ValueError): dm.target(url)


class Transport(unittest.TestCase):
    def serve(self, mode):
        self_test = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_GET(self):
                value = snapshot()
                if mode == 'identity':
                    value = stage_snapshot()
                    value['media_progress']['accepted']['observed_device_us'] = value['served_device_us']
                    value['media_progress']['raw']['publication_device_us'] = value['served_device_us']
                    value['media_progress']['encoded']['production_device_us'] = value['served_device_us']
                    value.update(boot_token='a' * 32, build_elf_sha256='b' * 64,
                                 private_song='SECRET', settings=None)
                body = json.dumps(value).encode()
                if mode == 'measured':
                    value = maximal_snapshot(); body = json.dumps(value, separators=(',', ':')).encode()
                    self_test.assertLessEqual(len(body), MEASURED_MAXIMAL_BODY_BYTES)
                    body += b' ' * (MEASURED_MAXIMAL_BODY_BYTES - len(body))
                if mode == 'maximum': body += b' ' * (dm.MAX_BODY_BYTES - len(body))
                if mode == 'oversize': body = b' ' * (dm.MAX_BODY_BYTES + 1)
                if mode == 'malformed': body = b'no'
                if mode == 'busy': body = b'{"error":"metrics_unavailable"}'
                if mode == 'busy-other': body = b'{"error":"something else"}'
                self.send_response(302 if mode == 'redirect' else 503 if mode.startswith('busy') else 200)
                self.send_header('Location', '/api/metrics')
                self.end_headers()
                try:
                    if mode == 'slow':
                        for byte in body:
                            self.wfile.write(bytes([byte])); self.wfile.flush(); time.sleep(.02)
                    else: self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError): pass
        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        self.addCleanup(server.server_close); self.addCleanup(server.shutdown)
        return f'http://127.0.0.1:{server.server_port}/api/metrics'

    def test_good_measured_and_maximum(self):
        # The cap is the firmware's MAX_BODY_BYTES (firmware/src/device_metrics.rs); the measured
        # maximal body sits inside it with room to spare.
        self.assertEqual(dm.MAX_BODY_BYTES, 4608)
        self.assertLessEqual(MEASURED_MAXIMAL_BODY_BYTES + 128, dm.MAX_BODY_BYTES)
        for mode in ['good', 'measured', 'maximum']:
            value = dm.fetch(self.serve(mode), .5)
            self.assertEqual(value['schema_version'], 1)
            if mode == 'measured': self.assertEqual(value['listeners'][1]['skips'], (1 << 64) - 1)

    def test_the_boards_503_is_unavailable_not_a_fault(self):
        # firmware/src/web.rs answers 503 {"error":"metrics_unavailable"} while a publisher holds the cache.
        with self.assertRaises(dm.Unavailable):
            dm.fetch(self.serve('busy'), .5)
        with self.assertRaises(ValueError):
            dm.fetch(self.serve('busy-other'), .5)

    def test_rejects_wire_failures(self):
        for mode in ['oversize', 'malformed', 'redirect', 'slow']:
            start = time.monotonic()
            with self.assertRaises((ValueError, OSError, TimeoutError)):
                dm.fetch(self.serve(mode), .12)
            if mode == 'slow': self.assertLess(time.monotonic() - start, .4)


class Exporter(unittest.TestCase):
    def test_slow_request_then_followup(self):
        errors = io.StringIO()
        self.enterContext(contextlib.redirect_stderr(errors))
        self.addCleanup(lambda: self.assertIn('metrics scrape unavailable', errors.getvalue()))
        cache = dm.Collector()
        server = dm.MetricsServer(cache, 0, request_seconds=.12)
        worker = threading.Thread(target=server.serve_forever); worker.start()
        self.addCleanup(server.server_close); self.addCleanup(worker.join)
        self.addCleanup(server.shutdown)
        slow = socket.create_connection(server.server_address); self.addCleanup(slow.close)
        start = time.monotonic()
        for _ in range(15):
            try: slow.sendall(b'G')
            except OSError: break
            time.sleep(.02)
        client = http.client.HTTPConnection(*server.server_address, timeout=.5)
        self.addCleanup(client.close)
        client.request('GET', '/metrics')
        self.assertEqual(client.getresponse().status, 200)
        self.assertLess(time.monotonic() - start, .6)

    def test_oversize_exporter_response_is_503(self):
        class OversizedCollector(dm.Collector):
            def render(self, now):
                return super().render(now) + '# ' + 'x' * 12288 + '\n'
        cache = OversizedCollector(); cache.accept(snapshot(), time.monotonic())
        server = dm.MetricsServer(cache, 0)
        worker = threading.Thread(target=server.serve_forever); worker.start()
        self.addCleanup(server.server_close); self.addCleanup(worker.join); self.addCleanup(server.shutdown)
        client = http.client.HTTPConnection(*server.server_address, timeout=.5); self.addCleanup(client.close)
        client.request('GET', '/metrics')
        self.assertEqual(client.getresponse().status, 503)

    def test_maximum_prometheus_response(self):
        item = maximal_snapshot()
        cache = dm.Collector(); cache.accept(item, time.monotonic() - .123456789)
        self.assertLessEqual(len(cache.render(time.monotonic()).encode()), 12288)
        server = dm.MetricsServer(cache, 0)
        worker = threading.Thread(target=server.serve_forever); worker.start()
        self.addCleanup(server.server_close); self.addCleanup(worker.join); self.addCleanup(server.shutdown)
        client = http.client.HTTPConnection(*server.server_address, timeout=.5); self.addCleanup(client.close)
        client.request('GET', '/metrics'); response = client.getresponse(); body = response.read()
        self.assertEqual(response.status, 200)
        self.assertGreater(len(body), 4096); self.assertLessEqual(len(body), 12288)

    def test_identity_whitelist_missing_and_stale(self):
        cache = dm.Collector(stale=.1, device_id='line-in')
        server = dm.MetricsServer(cache, 0)
        worker = threading.Thread(target=server.serve_forever); worker.start()
        self.addCleanup(server.server_close); self.addCleanup(worker.join); self.addCleanup(server.shutdown)
        def identity():
            client = http.client.HTTPConnection(*server.server_address, timeout=.5)
            try:
                client.request('GET', '/identity')
                response = client.getresponse(); body = response.read()
                self.assertLessEqual(len(body), 4096)
                self.assertEqual(response.status, 200)
                return json.loads(body)
            finally: client.close()
        empty = identity()
        self.assertFalse(empty['available']); self.assertIsNone(empty['boot_token'])
        item = snapshot(); item.update(boot_token='a' * 32, build_elf_sha256='b' * 64,
            settings=dict(active_format='mp3', next_play_format='flac', next_play_buffer='low',
                          buffer_available=True, autoplay=False, recognition_enabled=True, pga_gain=7),
            private_song='SECRET')
        cache.accept(item, time.monotonic())
        fresh = identity()
        self.assertTrue(fresh['available']); self.assertEqual(fresh['device_id'], 'line-in')
        self.assertEqual(fresh['settings']['pga_gain'], 7)
        self.assertNotIn('SECRET', json.dumps(fresh))
        self.assertIn('device_metrics_identity_available{kind="settings"} 1', cache.render(time.monotonic()))
        time.sleep(.15)
        self.assertFalse(identity()['available'])
        self.assertIn('device_metrics_identity_available{kind="settings"} 0', cache.render(time.monotonic()))

    def test_production_poll_scrape_stale_shutdown(self):
        fixture = Transport(); fixture._cleanups = []
        url = fixture.serve('identity')
        self.addCleanup(fixture.doCleanups)
        cache = dm.Collector(stale=.12)
        server = dm.MetricsServer(cache, 0, request_seconds=.2)
        stop = threading.Event()
        worker = threading.Thread(target=dm.run, args=(url, cache, server, .03, .1, stop))
        worker.start()
        def cleanup():
            stop.set(); worker.join(2)
            self.assertFalse(worker.is_alive())
        self.addCleanup(cleanup)
        def scrape(path='/metrics'):
            client = http.client.HTTPConnection(*server.server_address, timeout=.5)
            try:
                client.request('GET', path)
                return client.getresponse().read().decode()
            finally: client.close()
        deadline = time.monotonic() + 1
        while cache.received is None and time.monotonic() < deadline: time.sleep(.01)
        self.assertIn('device_telemetry_dropped{family="listener_loss"} 0', scrape())
        self.assertIn('device_media_accepted_pcm_frames 100', scrape())
        self.assertIn('device_last_listener_bytes_sent 1000', scrape())
        identity = json.loads(scrape('/identity'))
        self.assertEqual(identity['boot_token'], 'a' * 32)
        self.assertTrue(identity['available'])
        self.assertNotIn('SECRET', json.dumps(identity))
        time.sleep(.17)
        self.assertNotIn('device_telemetry_dropped', scrape())
        self.assertIn('device_metrics_response_available 0', scrape())
        self.assertFalse(json.loads(scrape('/identity'))['available'])


if __name__ == '__main__': unittest.main()
