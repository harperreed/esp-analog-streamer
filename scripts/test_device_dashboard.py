# ABOUTME: Validates the device dashboard's useful queries, units and missing-data behavior.
# ABOUTME: Reads the real import artifact without snapshots or external services.
import json
from pathlib import Path
import unittest

PATH = Path(__file__).resolve().parents[1] / '.agent/grafana/device-dashboard.json'


class DeviceDashboard(unittest.TestCase):
    def setUp(self):
        self.assertTrue(PATH.exists(), 'separate device dashboard must exist')
        self.dashboard = json.loads(PATH.read_text())
        self.panels = self.dashboard['panels']
        self.queries = {target['expr']: panel for panel in self.panels
                        for target in panel.get('targets', [])}

    def test_import_schema_and_datasource(self):
        self.assertIsNone(self.dashboard['id'])
        self.assertEqual(self.dashboard['uid'], 'esp32-device-health')
        self.assertEqual(self.dashboard['schemaVersion'], 39)
        ids = [panel['id'] for panel in self.panels]
        self.assertEqual(len(ids), len(set(ids)))
        for panel in self.panels:
            self.assertLessEqual(panel['gridPos']['x'] + panel['gridPos']['w'], 24)
            if panel.get('targets'):
                self.assertEqual(panel['datasource'], {'type': 'prometheus', 'uid': 'prometheus'})
                self.assertTrue(panel['description'])
                self.assertEqual(panel['fieldConfig']['defaults']['noValue'], 'Unavailable')

    def test_source_units_and_window_rate(self):
        required = {
            'device_capture_read_pcm_frames': 'short',
            'device_capture_queue_overflows': 'short',
            'device_capture_software_dropped_pcm_frames': 'short',
            'device_capture_read_errors': 'short',
            'device_capture_restarts': 'short',
            'device_capture_internal_free_bytes': 'bytes',
            'device_capture_internal_largest_bytes': 'bytes',
            'device_capture_psram_free_bytes': 'bytes',
            'device_capture_internal_minimum_sum_bytes': 'bytes',
            'device_capture_psram_minimum_sum_bytes': 'bytes',
            'device_encoder_stack_free_bytes': 'bytes',
            'device_metrics_owner_stack_free_bytes': 'bytes',
            'device_encoder_window_raw_pcm_frames': 'short',
            'device_encoder_window_raw_overruns': 'short',
            'device_encoder_window_resets': 'short',
            'device_encoder_window_codec_frames': 'short',
            'device_encoder_window_seconds': 's',
            'device_encoder_window_codec_frames / (device_encoder_window_seconds > 0)': 'hertz',
            'device_encoder_encode_max_seconds': 's',
            'device_encoder_task_runtime_seconds': 's',
            'device_encoder_runtime_valid': 'short',
            'device_telemetry_dropped': 'short',
            'device_telemetry_loss_occurred': 'short',
        }
        for expr, unit in required.items():
            with self.subTest(expr=expr):
                self.assertTrue(expr in self.queries, expr)
                self.assertEqual(self.queries[expr]['fieldConfig']['defaults']['unit'], unit)

    def test_media_and_completed_listener_observations(self):
        required = {
            'device_media_accepted_pcm_frames': 'short',
            'device_media_published_pcm_frames': 'short',
            'device_media_encoded_sequence': 'short',
            'device_media_encoded_age_seconds': 's',
            'device_media_backlog_pcm_frames': 'short',
            'device_last_listener_bytes_sent': 'bytes',
            'device_last_listener_frame_write_max_seconds': 's',
            'device_last_listener_media_age_seconds': 's',
            'device_last_listener_stack_free_bytes': 'bytes',
            'device_network_connected': 'short',
            'device_network_disconnects': 'short',
            'device_network_generation': 'short',
        }
        for expr, unit in required.items():
            with self.subTest(expr=expr):
                self.assertTrue(expr in self.queries, expr)
                panel = self.queries[expr]
                self.assertEqual(panel['fieldConfig']['defaults']['unit'], unit)
                if expr.startswith('device_last_listener_'):
                    self.assertIn('completed', panel['description'].lower())
                    self.assertIn('not live', panel['description'].lower())
        introduction = self.panels[0]['options']['content']
        self.assertIn('media_progress', introduction)
        self.assertIn('last_listener_departure', introduction)
        self.assertNotIn('Unavailable: publication head', introduction)

    def test_identity_and_availability_colors(self):
        for expr in ('device_metrics_response_available', 'device_metrics_family_available',
                     'device_metrics_identity_available'):
            with self.subTest(expr=expr):
                self.assertTrue(expr in self.queries, expr)
                panel = self.queries[expr]
                self.assertTrue(panel['targets'][0]['instant'])
                defaults = panel['fieldConfig']['defaults']
                self.assertEqual(defaults['color']['mode'], 'thresholds')
                self.assertEqual(defaults['thresholds']['steps'], [
                    {'color': 'red', 'value': None}, {'color': 'green', 'value': 1}])
        if 'device_metrics_identity_available' in self.queries:
            self.assertIn('{{kind}}', self.queries['device_metrics_identity_available']['targets'][0]['legendFormat'])

    def test_freshness_and_no_fabricated_rates(self):
        for expr in ('device_metrics_response_available', 'device_metrics_family_available',
                     'device_metrics_response_age_seconds', 'device_metrics_family_age_seconds'):
            self.assertTrue(expr in self.queries, expr)
            for target in self.queries[expr]['targets']:
                self.assertTrue(target['instant'])
        for expr in self.queries:
            self.assertNotRegex(expr, r'\b(rate|irate|increase|delta|idelta)\s*\(')
            self.assertNotIn('vector(0)', expr)
        text = json.dumps(self.dashboard).lower()
        for limitation in ('boot', 'publication', 'frame age', 'listener', 'wi-fi', 'provenance'):
            self.assertIn(limitation, text)
        for panel in self.panels:
            if panel['type'] == 'timeseries':
                self.assertFalse(panel['fieldConfig']['defaults']['custom']['spanNulls'])


if __name__ == '__main__':
    unittest.main()
