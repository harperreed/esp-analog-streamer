# ABOUTME: Checks HTTP acceptance guards using retained protocol shapes and UART receipts.
# ABOUTME: Keeps ownership observations, deferred settings and payload evidence fail closed.
import unittest
import time
import signal
import subprocess
import sys
import tempfile
import json
import threading
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from types import SimpleNamespace
from sonos_http_check import stream_identity, payload_receipt, same_peer, verify_attestation, cleanup_attempt, confirmed_cleanup_uri, stop_owned_source, Trial, confirmed_observation_uri


@contextmanager
def discovery_protocol_fixture(root, rows, config):
    # Exercise production HTTP parsing/guards using loopback protocol fixtures.
    image = root / 'image.elf'
    image.write_bytes(b'host discovery protocol fixture')
    calls = []
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            calls.append(('GET', self.path))
            if self.path == '/api/metrics':
                value = dict(build_elf_sha256=trial.image, boot_token='b' * 32,
                             served_device_us=100)
            else:
                value = rows.pop(0)
            self.reply(value)

        def do_POST(self):
            value = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            calls.append(('POST', self.path, value))
            self.reply(dict(accepted=True))

        def reply(self, value):
            data = json.dumps(value).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, format, *args):
            pass

    with HTTPServer(('127.0.0.1', 0), Handler) as server:
        trial = Trial(SimpleNamespace(out=root / 'evidence', image=image,
                                     host=f'127.0.0.1:{server.server_port}'))
        trial.before = dict(state='stopped', groups=[], config=config)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield trial, calls
        finally:
            server.shutdown()
            worker.join(timeout=2)


@contextmanager
def stop_protocol_fixture(root, transports, takeover=False):
    image = root / 'image.elf'
    image.write_bytes(b'host stop protocol fixture')
    calls = []
    owned = 'http://192.168.23.47:8000/stream.flac?sonos_intent=41'
    config = dict(player_id='SELECTED', autoplay=False)
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            calls.append(('GET', self.path))
            value = (dict(build_elf_sha256=trial.image, boot_token='b' * 32,
                          served_device_us=100) if self.path == '/api/metrics' else
                     dict(state='stopped', config=config))
            self.reply(json.dumps(value).encode())

        def do_POST(self):
            raw = self.rfile.read(int(self.headers['Content-Length']))
            if self.path == '/api/sonos':
                calls.append(('board', json.loads(raw)))
                self.reply(b'{"accepted":true}')
                return
            name = self.headers['SOAPAction'].strip('"').split('#')[-1]
            calls.append(('soap', name))
            if name == 'GetZoneGroupState':
                values = '<ZoneGroupState>&lt;ZoneGroups&gt;&lt;ZoneGroup Coordinator="SELECTED"&gt;&lt;ZoneGroupMember UUID="SELECTED" Location="http://127.0.0.1:1400/device.xml"/&gt;&lt;/ZoneGroup&gt;&lt;/ZoneGroups&gt;</ZoneGroupState>'
            elif name == 'GetMediaInfo':
                changed = takeover and any(call == ('soap', 'GetTransportInfo') for call in calls)
                values = '<CurrentURI>' + ('x-rincon:external' if changed else owned.replace('&', '&amp;')) + '</CurrentURI>'
            else:
                state = transports.pop(0) if len(transports) > 1 else transports[0]
                values = '<CurrentTransportState>' + state + '</CurrentTransportState>'
            self.reply(('<Envelope><Body>' + values + '</Body></Envelope>').encode())

        def reply(self, data):
            self.send_response(200)
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, format, *args):
            pass

    with HTTPServer(('127.0.0.1', 0), Handler) as server:
        host = f'127.0.0.1:{server.server_port}'
        trial = Trial(SimpleNamespace(out=root / 'evidence', image=image, host=host))
        trial.before = dict(config=config)
        trial.boot = 'b' * 32
        trial.owned_uri = owned
        trial.play_sent = True
        trial.base = 'http://127.0.0.1:1400/device.xml'
        trial.services = {f'urn:schemas-upnp-org:service:{name}:1': 'http://' + host + '/soap'
                          for name in ('AVTransport', 'ZoneGroupTopology')}
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield trial, calls
        finally:
            server.shutdown()
            worker.join(timeout=2)


class Guards(unittest.TestCase):
    def test_stream_identity_binds_format_buffer_generation_and_intent(self):
        uri = 'http://192.168.23.47:8000/stream.flac?buffer=med&network_generation=7&sonos_intent=41'
        self.assertEqual(stream_identity(uri, '192.168.23.47', 'flac', 'med'), 41)
        for value in (uri.replace('sonos_intent=41', 'sonos_intent=0'), uri + '&sonos_intent=42', uri.replace('23.47', '23.48')):
            with self.assertRaises(ValueError):
                stream_identity(value, '192.168.23.47', 'flac', 'med')

    def test_external_service_url_rejected(self):
        self.assertEqual(same_peer('http://192.168.23.205:1400/xml/device_description.xml', '/MediaRenderer/AVTransport/Control'), 'http://192.168.23.205:1400/MediaRenderer/AVTransport/Control')
        with self.assertRaises(ValueError):
            same_peer('http://192.168.23.205:1400/xml/device_description.xml', 'http://192.168.23.206:1400/control')

    def test_uart_requires_matching_sonos_owner_and_nonzero_delivery(self):
        raw = ('stream: 192.168.23.205:50000 joined /stream.flac (1 of 2) event_us=10 owner_id=3 sonos_intent=41\n'
               'stream-listener: peer=192.168.23.205:50000 frames_sent=2 bytes_sent=300 event_us=20\n'
               'stream: 192.168.23.205:50000 socket closed in 1 ms; releasing slot (1 held) event_us=25 owner_id=3 sonos_intent=41\n')
        self.assertEqual(payload_receipt(raw, '192.168.23.205', 'flac', 41)['bytes_sent'], 300)
        with self.assertRaises(ValueError):
            payload_receipt(raw.replace('bytes_sent=300', 'bytes_sent=0'), '192.168.23.205', 'flac', 41)
        with self.assertRaises(ValueError):
            payload_receipt(raw, '192.168.23.205', 'flac', 42)
        for altered in (raw.replace('event_us=20', 'event_us=9'),
                        raw.replace('event_us=20', 'event_us=26'),
                        raw.replace('event_us=25 owner_id=3', 'event_us=25 owner_id=4'),
                        raw + 'stream: 192.168.23.205:50000 joined /stream.flac (1 of 2) event_us=30 owner_id=4 sonos_intent=42\n'):
            with self.assertRaises(ValueError):
                payload_receipt(altered, '192.168.23.205', 'flac', 41)

    def test_failed_observation_never_establishes_cleanup_ownership(self):
        candidate = 'http://192.168.23.47:8000/stream.flac?buffer=med&network_generation=7&sonos_intent=41'
        for transport, track, source_live in [('STOPPED', candidate, True),
                                              ('PLAYING', 'x-rincon-queue:external', True),
                                              ('PLAYING', candidate, False)]:
            owned = None
            with self.assertRaises(ValueError):
                owned = confirmed_observation_uri(owned, candidate, transport, track, source_live)
            self.assertIsNone(owned)
            with self.assertRaises(ValueError):
                confirmed_cleanup_uri(owned, candidate)
        owned = confirmed_observation_uri(None, candidate, 'PLAYING', candidate, True)
        self.assertEqual(confirmed_cleanup_uri(owned, candidate), candidate)

    def test_uncertain_play_never_adopts_another_matching_board_uri(self):
        candidate = 'http://192.168.23.47:8000/stream.flac?buffer=med&network_generation=7&sonos_intent=42'
        with self.assertRaises(ValueError):
            confirmed_cleanup_uri(None, candidate)
        with self.assertRaises(ValueError):
            confirmed_cleanup_uri(candidate, candidate.replace('intent=42', 'intent=43'))
        self.assertEqual(confirmed_cleanup_uri(candidate, candidate), candidate)

    def test_cleanup_interrupt_propagates_and_expired_budget_runs_no_step(self):
        errors = []
        def interrupted():
            raise InterruptedError('cleanup alarm')
        with self.assertRaises(InterruptedError):
            cleanup_attempt(errors, 'interrupt', time.monotonic() + 10, interrupted)
        self.assertEqual(errors, [])
        called = []
        with self.assertRaises(TimeoutError):
            cleanup_attempt(errors, 'expired', time.monotonic() - 1, lambda: called.append(True))
        self.assertEqual(called, [])

    def test_real_cleanup_alarm_escapes_step(self):
        previous = signal.getsignal(signal.SIGALRM)
        def alarm(signum, frame):
            raise InterruptedError('real cleanup alarm')
        signal.signal(signal.SIGALRM, alarm)
        try:
            signal.setitimer(signal.ITIMER_REAL, .01)
            with self.assertRaises(InterruptedError):
                cleanup_attempt([], 'alarm', time.monotonic() + 1, signal.pause)
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)
            signal.signal(signal.SIGALRM, previous)

    def test_owned_source_stops_even_with_expired_control_budget(self):
        source = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        try:
            with self.assertRaises(TimeoutError):
                stop_owned_source(source, time.monotonic() - 1)
            source.wait(timeout=2)
            self.assertIsNotNone(source.returncode)
        finally:
            if source.poll() is None:
                source.kill()
                source.wait(timeout=2)

    def test_real_cleanup_stops_only_its_child_and_saves_receipt(self):
        previous = signal.getsignal(signal.SIGALRM)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / 'image.elf'
            image.write_bytes(b'host cleanup test image')
            trial = Trial(SimpleNamespace(out=root / 'evidence', image=image))
            trial.source = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
            try:
                trial.cleanup()
                self.assertIsNotNone(trial.source.poll())
                receipt = json.loads((root / 'evidence/result.json').read_text())
                self.assertTrue(receipt['cleanup_complete'])
                self.assertEqual(receipt['cleanup_errors'], [])
                self.assertLess(receipt['cleanup_elapsed_seconds'], 45)
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                signal.signal(signal.SIGALRM, previous)
                if trial.source.poll() is None:
                    trial.source.kill()
                    trial.source.wait(timeout=2)

    def test_discovery_waits_for_worker_completion_before_refreshing_selected_group(self):
        # A queued discovery initially still reports stopped, then runs to completion.
        config = dict(player_id='RINCON_SELECTED', autoplay=False)
        rows = [dict(state='stopped', groups=[], config=config),
                dict(state='discovering', groups=[], config=config),
                dict(state='discovering', groups=[], config=config),
                dict(state='stopped', groups=[dict(player_id='RINCON_SELECTED',
                     is_leader=True, host='192.168.23.205')], config=config)]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with discovery_protocol_fixture(root, rows, config) as (trial, calls):
                trial.discover()
                self.assertEqual(rows, [])
                self.assertEqual(calls, [
                    ('GET', '/api/metrics'), ('GET', '/api/sonos'),
                    ('POST', '/api/sonos', dict(action='discover')),
                    ('GET', '/api/sonos'), ('GET', '/api/sonos'),
                    ('GET', '/api/sonos'), ('GET', '/api/metrics')])
                self.assertEqual(trial.before['groups'][0]['player_id'], 'RINCON_SELECTED')

    def test_discovery_rejects_config_drift_before_replacing_original_snapshot(self):
        config = dict(player_id='RINCON_SELECTED', autoplay=False)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = dict(state='stopped', groups=[], config=config)
            rows = [original, dict(state='discovering', groups=[], config=config),
                    dict(state='stopped', groups=[], config=dict(config, player_id='OTHER'))]
            with discovery_protocol_fixture(root, rows, config) as (trial, calls):
                trial.before = original
                with self.assertRaisesRegex(ValueError, 'configuration changed during discovery'):
                    trial.discover()
                self.assertIs(trial.before, original)
                self.assertEqual(rows, [])
                self.assertEqual(calls[-1], ('GET', '/api/metrics'))

    def test_stop_waits_for_remote_completion_without_repeating_board_stop(self):
        with tempfile.TemporaryDirectory() as directory:
            with stop_protocol_fixture(Path(directory), ['PLAYING', 'PLAYING', 'STOPPED']) as (trial, calls):
                trial.stop()
                self.assertEqual([call for call in calls if call[0] == 'board'],
                                 [('board', dict(action='stop'))])
                self.assertEqual(sum(call == ('soap', 'GetTransportInfo') for call in calls), 3)
                self.assertIsNone(trial.owned_uri)
                self.assertFalse(trial.play_sent)

    def test_stop_rejects_takeover_while_waiting_without_more_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            with stop_protocol_fixture(Path(directory), ['PLAYING'], takeover=True) as (trial, calls):
                with self.assertRaisesRegex(ValueError, 'URI'):
                    trial.stop()
                self.assertEqual([call for call in calls if call[0] == 'board'],
                                 [('board', dict(action='stop'))])
                self.assertTrue(trial.play_sent)
                self.assertIsNotNone(trial.owned_uri)

    def test_stop_remote_wait_respects_existing_phase_deadline(self):
        with tempfile.TemporaryDirectory() as directory:
            with stop_protocol_fixture(Path(directory), ['PLAYING']) as (trial, calls):
                trial.deadline = time.monotonic() + .6
                with self.assertRaises(TimeoutError):
                    trial.stop()
                self.assertEqual([call for call in calls if call[0] == 'board'],
                                 [('board', dict(action='stop'))])
                self.assertTrue(trial.play_sent)

    def test_identify_rejects_missing_or_ambiguous_selected_coordinator(self):
        for groups in ([], [dict(player_id='OTHER', is_leader=True)],
                       [dict(player_id='SELECTED', is_leader=False)],
                       [dict(player_id='SELECTED', is_leader=True)] * 2):
            trial = Trial.__new__(Trial)
            trial.before = dict(config=dict(player_id='SELECTED'), groups=groups)
            with self.assertRaisesRegex(ValueError, 'selected coordinator missing/ambiguous'):
                trial.identify()

    def test_boot_or_image_change_rejected(self):
        record = dict(build_elf_sha256='a' * 64, boot_token='b' * 32)
        verify_attestation(record, 'a' * 64, 'b' * 32)
        for image, boot in [('c' * 64, 'b' * 32), ('a' * 64, 'c' * 32)]:
            with self.assertRaises(ValueError):
                verify_attestation(record, image, boot)


if __name__ == '__main__':
    unittest.main()
