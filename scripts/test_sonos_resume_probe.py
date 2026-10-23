# ABOUTME: Checks the resume probe's UART parsing, pure cycle verdicts and loopback board/SOAP driving.
# ABOUTME: Fixture timings come from the October 13 passive-reconnect receipts; no board or Sonos is contacted.
import io
import json
import os
import signal
import tempfile
import threading
import time
import unittest
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from types import SimpleNamespace

from sonos_resume_probe import (ResumeProbe, await_operator, cycle_verdict, listener_events,
                                parse_args, tagged_identity, tagged_match)

SPEAKER = '192.168.23.205'
OWNED = 'http://192.168.23.217:8000/stream.mp3?buffer=med&network_generation=7&sonos_intent=41'
BROADCAST = OWNED.replace('http://', 'x-rincon-mp3radio://')
# The loopback fixture stands in for the board, so the tagged URL it echoes names the loopback host.
LOOP = OWNED.replace('192.168.23.217', '127.0.0.1')
# Device timestamps lifted from docs/audit-evidence/2026-10-13/passive-reconnect.json (established trial).
JOIN_US, INJECTION_US, DEPARTURE_US, CLOSE_US, FIRST_STOPPED_US = 31062475, 41206014, 41206442, 41478215, 47659492
PHASE1 = (f'passive-media-close: phase=1 event_us={INJECTION_US} owner_id=3 sonos_intent=41 network_generation=7 '
          'operator_epoch=1 first_payload_us=31062475 frames_sent=584 bytes_sent=560640')
UART = [
    'probe-broadcast: play requested broadcast=true',
    f'probe-broadcast: enabled=true uri={BROADCAST}',
    f'stream: {SPEAKER}:50000 joined /stream.mp3 (1 of 2) event_us={JOIN_US} owner_id=3 sonos_intent=41 role=sonos',
    f'passive-media-close: phase=0 event_us={JOIN_US + 25} owner_id=3 sonos_intent=41 network_generation=7 operator_epoch=1 first_payload_us=0 frames_sent=0 bytes_sent=0',
    f'probe-broadcast: echoed uri={OWNED}',
    'stream-listener: peer=192.168.23.205:50000 frames_sent=584 bytes_sent=560640 event_us=41206400',
    PHASE1,
    f'\x1b[0;32mI (41206) stream: {SPEAKER}:50000 left after 10.1 s: diagnostic passive media close listener_stack_free_bytes=2048 event_us={DEPARTURE_US}\x1b[0m',
    f'stream: {SPEAKER}:50000 socket closed in 271 ms; releasing slot (0 held) event_us={CLOSE_US} owner_id=3 sonos_intent=41',
    'audio: frames=1234 unrelated line',
]
CLOSE = dict(device_us=INJECTION_US, phase=1)
CHECKED_STOP = dict(device_us=131_200_000)


def sample(device_us, state, status='OK', uri=OWNED, generation=7, error=None):
    row = dict(sample_started_monotonic=device_us / 1e6, device_us=device_us, phase='observe', board_state='playing',
               transport_state=state, transport_status=status, current_uri=uri, track_uri=uri, rel_time='0:00:06',
               listeners_active=1, network_generation=generation)
    if error is not None:
        row['error'] = error
    return row


def failure_stop_series():
    return ([sample(36_000_000, 'PLAYING')]
            + [sample(41_300_000 + i * 1_000_000, 'PLAYING') for i in range(5)]
            + [sample(FIRST_STOPPED_US, 'STOPPED', 'ERROR_OCCURRED')]
            + [sample(48_700_000 + i * 1_000_000, 'STOPPED', 'ERROR_OCCURRED') for i in range(3)]
            + [sample(132_200_000, 'STOPPED', 'OK')])


class Parsing(unittest.TestCase):
    def test_listener_events_parse_real_join_left_close_and_probe_receipts(self):
        events = listener_events(UART)
        self.assertEqual([row['kind'] for row in events],
                         ['play-requested', 'broadcast', 'join', 'passive-close', 'echoed', 'passive-close', 'left', 'closed'])
        requested, broadcast, join, phase0, echoed, phase1, left, closed = events
        self.assertEqual(requested, dict(kind='play-requested', broadcast=True))
        self.assertEqual(broadcast, dict(kind='broadcast', enabled=True, uri=BROADCAST))
        self.assertEqual(echoed, dict(kind='echoed', uri=OWNED))
        self.assertEqual(join, dict(kind='join', peer=f'{SPEAKER}:50000', host=SPEAKER, path='/stream.mp3', held=1,
                                    event_us=JOIN_US, owner_id=3, sonos_intent=41, role='sonos'))
        self.assertEqual(phase1, dict(kind='passive-close', phase=1, event_us=INJECTION_US, owner_id=3, sonos_intent=41,
                                      network_generation=7, operator_epoch=1, first_payload_us=31062475,
                                      frames_sent=584, bytes_sent=560640))
        self.assertEqual(phase0['phase'], 0)
        self.assertEqual(left, dict(kind='left', peer=f'{SPEAKER}:50000', host=SPEAKER, event_us=DEPARTURE_US,
                                    reason='diagnostic passive media close', category='socket_error'))
        self.assertEqual(closed, dict(kind='closed', peer=f'{SPEAKER}:50000', host=SPEAKER, event_us=CLOSE_US,
                                      elapsed_ms=271, held=0, owner_id=3, sonos_intent=41))
        self.assertEqual(listener_events([]), [])

    def test_listener_events_reject_malformed_event_time_or_receipt(self):
        with self.assertRaises(RuntimeError):
            listener_events([UART[2].replace('event_us=' + str(JOIN_US), 'event_us=abc')])
        with self.assertRaises(ValueError):
            listener_events([PHASE1.replace(' frames_sent=584', '')])
        with self.assertRaises(ValueError):
            listener_events(['probe-broadcast: enabled=maybe uri=x'])

    def test_tagged_identity_records_any_scheme_and_rejects_foreign_urls(self):
        self.assertEqual(tagged_identity(OWNED, '192.168.23.217'), ('http', 7, 41))
        self.assertEqual(tagged_identity(BROADCAST, '192.168.23.217'), ('x-rincon-mp3radio', 7, 41))
        for bad in (OWNED.replace('23.217', '23.218'), OWNED.replace(':8000', ':8001'), OWNED.replace('stream.mp3', 'stream.flac'),
                    OWNED.replace('sonos_intent=41', 'sonos_intent=0'), OWNED + '&sonos_intent=42',
                    'x-rincon-queue:RINCON_X#0', 'x-sonosapi-stream:s1234?sid=254'):
            with self.assertRaises(ValueError):
                tagged_identity(bad, '192.168.23.217')

    def test_tagged_match_ignores_scheme_only(self):
        self.assertTrue(tagged_match(OWNED, BROADCAST))
        self.assertTrue(tagged_match(OWNED, OWNED))
        self.assertFalse(tagged_match(OWNED, OWNED.replace('sonos_intent=41', 'sonos_intent=42')))
        self.assertFalse(tagged_match(OWNED, 'x-sonosapi-stream:s1234?sid=254'))
        self.assertFalse(tagged_match(OWNED, ''))


class Verdicts(unittest.TestCase):
    def test_failure_stop_with_non_ok_status_then_ok_after_checked_stop_passes_a_not_b(self):
        verdict = cycle_verdict(failure_stop_series(), listener_events(UART), OWNED, 41, 7, SPEAKER,
                                close=CLOSE, checked_stop=CHECKED_STOP)
        self.assertEqual(verdict['status_after_close'], dict(status='ERROR_OCCURRED', state='STOPPED', uri_ours=True,
                                                             uri=OWNED, delay_seconds=(FIRST_STOPPED_US - INJECTION_US) / 1e6))
        self.assertEqual(verdict['status_after_stop'], dict(status='OK', state='STOPPED', uri_ours=True, uri=OWNED, delay_seconds=1.0))
        self.assertIsNone(verdict['status_after_app_stop'])
        self.assertTrue(verdict['uri_retained_after_close'])
        self.assertEqual(verdict['uris_after_close'], [OWNED])
        self.assertEqual(verdict['readmissions_after_close'],
                         dict(count=0, first_delay_seconds=None, intent_match=None, generation_current=None, intents=[]))
        self.assertIsNone(verdict['playing_again'])
        self.assertEqual(verdict['owner_departure'], dict(event_us=CLOSE_US, departure_us=DEPARTURE_US, owner_id=3,
                                                          reason='diagnostic passive media close',
                                                          delay_seconds=(CLOSE_US - INJECTION_US) / 1e6))
        self.assertTrue(verdict['pass_a'])
        self.assertFalse(verdict['pass_b'])
        self.assertEqual(verdict['scheme_observed'], 'http')

    def test_readmission_and_playing_again_passes_b_while_ok_status_fails_a(self):
        rejoin = 50_000_000
        uart = UART + [f'stream: {SPEAKER}:50001 joined /stream.mp3 (1 of 2) event_us={rejoin} owner_id=4 sonos_intent=41 role=sonos']
        # Sonos echoes the plain scheme while the board sent the broadcast one: still our tagged URL.
        samples = ([sample(41_300_000 + i * 1_000_000, 'PLAYING', uri=OWNED) for i in range(5)]
                   + [sample(FIRST_STOPPED_US, 'STOPPED', uri=OWNED)]
                   + [sample(50_500_000, 'PLAYING', uri=OWNED)]
                   + [sample(132_200_000, 'STOPPED', uri=OWNED)])
        verdict = cycle_verdict(samples, listener_events(uart), BROADCAST, 41, 7, SPEAKER, close=CLOSE, checked_stop=CHECKED_STOP)
        self.assertEqual(verdict['readmissions_after_close'], dict(count=1, first_delay_seconds=(rejoin - INJECTION_US) / 1e6,
                                                                   intent_match=True, generation_current=True, intents=[41]))
        self.assertEqual(verdict['playing_again'], dict(delay_seconds=(50_500_000 - INJECTION_US) / 1e6, device_us=50_500_000,
                                                        after_readmission=True))
        self.assertIsNone(verdict['status_after_close'])
        self.assertTrue(verdict['uri_retained_after_close'])
        self.assertFalse(verdict['pass_a'])
        self.assertTrue(verdict['pass_b'])
        self.assertEqual(verdict['scheme_observed'], 'x-rincon-mp3radio')

    def test_playing_again_requires_an_intervening_non_playing_sample_or_a_readmission(self):
        # Sonos keeps reporting PLAYING from its buffer right after the close; that is not a resume.
        samples = [sample(41_300_000 + i * 1_000_000, 'PLAYING') for i in range(5)]
        verdict = cycle_verdict(samples, listener_events(UART), OWNED, 41, 7, SPEAKER, close=CLOSE)
        self.assertIsNone(verdict['playing_again'])
        self.assertIsNone(verdict['uri_retained_after_close'])
        self.assertFalse(verdict['pass_b'])

    def test_foreign_intent_or_stale_generation_readmission_does_not_pass_b(self):
        uart = UART + [f'stream: {SPEAKER}:50001 joined /stream.mp3 (1 of 2) event_us=50000000 owner_id=4 sonos_intent=42 role=sonos',
                       'stream: 192.168.23.153:40000 joined /stream.mp3 (2 of 2) event_us=50000001 owner_id=5 sonos_intent=0 role=diagnostic']
        verdict = cycle_verdict(failure_stop_series(), listener_events(uart), OWNED, 41, 7, SPEAKER, close=CLOSE)
        self.assertEqual(verdict['readmissions_after_close']['count'], 1)
        self.assertFalse(verdict['readmissions_after_close']['intent_match'])
        self.assertFalse(verdict['pass_b'])
        uart = UART + [f'stream: {SPEAKER}:50001 joined /stream.mp3 (1 of 2) event_us=50000000 owner_id=4 sonos_intent=41 role=sonos']
        samples = failure_stop_series()[:6] + [sample(50_500_000, 'PLAYING', generation=8)]
        verdict = cycle_verdict(samples, listener_events(uart), OWNED, 41, 7, SPEAKER, close=CLOSE)
        self.assertFalse(verdict['readmissions_after_close']['generation_current'])
        self.assertFalse(verdict['pass_b'])

    def test_app_stop_cycle_reports_both_stops_and_no_b_verdict(self):
        app_stop = dict(device_us=41_200_000)
        samples = ([sample(36_000_000, 'PLAYING'), sample(41_300_000, 'PLAYING'), sample(44_200_000, 'STOPPED', 'OK')]
                   + [sample(45_200_000 + i * 1_000_000, 'STOPPED', 'OK') for i in range(3)] + [sample(132_200_000, 'STOPPED', 'OK')])
        verdict = cycle_verdict(samples, listener_events(UART), OWNED, 41, 7, SPEAKER, app_stop=app_stop, checked_stop=CHECKED_STOP)
        self.assertEqual(verdict['status_after_app_stop'], dict(status='OK', state='STOPPED', uri_ours=True, uri=OWNED, delay_seconds=3.0))
        self.assertEqual(verdict['status_after_stop']['status'], 'OK')
        for key in ('status_after_close', 'uri_retained_after_close', 'uris_after_close', 'readmissions_after_close',
                    'playing_again', 'owner_departure', 'pass_b'):
            self.assertIsNone(verdict[key], key)
        self.assertTrue(verdict['pass_a'])

    def test_takeover_after_close_fails_a_and_marks_uri_not_retained(self):
        samples = failure_stop_series()
        samples[7] = sample(48_700_000, 'STOPPED', 'OK', uri='x-rincon-queue:RINCON_X#0')
        verdict = cycle_verdict(samples, listener_events(UART), OWNED, 41, 7, SPEAKER, close=CLOSE, checked_stop=CHECKED_STOP)
        self.assertFalse(verdict['uri_retained_after_close'])
        self.assertEqual(verdict['uris_after_close'], [OWNED, 'x-rincon-queue:RINCON_X#0'])
        self.assertFalse(verdict['pass_a'])

    def test_errored_samples_and_missing_stop_are_excluded_without_passing(self):
        samples = failure_stop_series()
        samples[6] = sample(FIRST_STOPPED_US, 'STOPPED', 'ERROR_OCCURRED', error='timeout')
        verdict = cycle_verdict(samples, listener_events(UART), OWNED, 41, 7, SPEAKER, close=CLOSE)
        self.assertEqual(verdict['status_after_close']['delay_seconds'], (48_700_000 - INJECTION_US) / 1e6)
        self.assertIsNone(verdict['status_after_stop'])
        self.assertFalse(verdict['pass_a'])
        self.assertEqual(verdict['sample_errors'], 1)

    def test_nothing_observed_yields_no_verdict(self):
        verdict = cycle_verdict([], [], None, None, None, None)
        self.assertIsNone(verdict['pass_a'])
        self.assertIsNone(verdict['pass_b'])
        self.assertIsNone(verdict['scheme_observed'])


@contextmanager
def probe_fixture(root, transports, scheme='plain', echo=LOOP, initial_state='playing'):
    """Loopback board API plus read-only SOAP, like test_sonos_http_check's stop_protocol_fixture."""
    root.mkdir(parents=True, exist_ok=True)
    image = root / 'image.elf'
    image.write_bytes(b'host resume probe fixture')
    calls = []
    config = dict(player_id='SELECTED', autoplay=False, output_format='mp3', buffer_preset='med')
    board = dict(state=initial_state)
    clock = [41_000_000]
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            calls.append(('GET', self.path))
            if self.path == '/api/metrics':
                clock[0] += 100_000
                value = dict(build_elf_sha256=probe.image, boot_token='b' * 32, served_device_us=clock[0],
                             settings=dict(recognition_enabled=False), network=dict(connected=True, generation=7),
                             listeners=[dict(state='streaming'), dict(state='idle')])
            elif self.path == '/api/state':
                value = dict(shazam_enabled=False)
            else:
                value = dict(state=board['state'], config=config, active_format='mp3', buffer_available=True)
            self.reply(json.dumps(value).encode())

        def do_POST(self):
            raw = self.rfile.read(int(self.headers['Content-Length']))
            if self.path.startswith('/api/sonos'):
                body = json.loads(raw)
                calls.append(('board', self.path, body))
                board['state'] = 'playing' if body['action'] == 'play' else 'stopped'
                self.reply(b'{"accepted":true}')
                return
            name = self.headers['SOAPAction'].strip('"').split('#')[-1]
            calls.append(('soap', name))
            if name == 'GetZoneGroupState':
                values = '<ZoneGroupState>&lt;ZoneGroups&gt;&lt;ZoneGroup Coordinator="SELECTED"&gt;&lt;ZoneGroupMember UUID="SELECTED" Location="http://127.0.0.1:1400/device.xml"/&gt;&lt;/ZoneGroup&gt;&lt;/ZoneGroups&gt;</ZoneGroupState>'
            elif name == 'GetMediaInfo':
                values = '<CurrentURI>' + echo.replace('&', '&amp;') + '</CurrentURI>'
            elif name == 'GetPositionInfo':
                values = '<TrackURI>' + echo.replace('&', '&amp;') + '</TrackURI><RelTime>0:00:06</RelTime>'
            else:
                state, status = transports.pop(0) if len(transports) > 1 else transports[0]
                values = f'<CurrentTransportState>{state}</CurrentTransportState><CurrentTransportStatus>{status}</CurrentTransportStatus>'
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
        probe = ResumeProbe(SimpleNamespace(out=root / 'evidence', image=image, host=host, cycle='close', scheme=scheme,
                                            serial='/dev/null', previous_boot=None))
        probe.before = dict(state=initial_state, config=config)
        probe.boot = 'b' * 32
        probe.speaker = SPEAKER
        probe.base = 'http://127.0.0.1:1400/device.xml'
        probe.services = {f'urn:schemas-upnp-org:service:{name}:1': 'http://' + host + '/soap'
                          for name in ('AVTransport', 'ZoneGroupTopology')}
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            yield probe, calls
        finally:
            server.shutdown()
            worker.join(timeout=2)


def owned(probe):
    probe.owned_uri, probe.intent, probe.generation, probe.play_sent = LOOP, 41, 7, True


class Driving(unittest.TestCase):
    def test_sample_records_state_status_uris_and_uart_events(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with probe_fixture(root, [('PLAYING', 'OK')]) as (probe, calls):
                owned(probe)
                uart = root / 'evidence' / 'uart.log'
                uart.write_text('\n'.join(UART[:3]) + '\n' + UART[3][:20])
                row = probe.sample('hold')
                self.assertEqual((row['transport_state'], row['transport_status'], row['current_uri'], row['track_uri']),
                                 ('PLAYING', 'OK', LOOP, LOOP))
                self.assertEqual((row['board_state'], row['listeners_active'], row['rel_time'], row['network_generation']),
                                 ('playing', 1, '0:00:06', 7))
                self.assertEqual([event['kind'] for event in probe.uart_events], ['play-requested', 'broadcast', 'join'])
                with uart.open('a') as handle:
                    handle.write(UART[3][20:] + '\n' + '\n'.join(UART[4:]) + '\n')
                probe.sample('hold')
                self.assertEqual([event['kind'] for event in probe.uart_events][-3:], ['passive-close', 'left', 'closed'])
                self.assertEqual(len(probe.samples), 2)
                kinds = [json.loads(line)['kind'] for line in (root / 'evidence' / 'events.jsonl').read_text().splitlines()]
                self.assertEqual(kinds.count('sample'), 2)
                self.assertEqual(kinds.count('uart-listener'), 8)

    def test_play_sends_broadcast_query_and_binds_ownership_to_this_boots_admission(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with probe_fixture(root, [('PLAYING', 'OK')], scheme='broadcast', initial_state='stopped') as (probe, calls):
                uart = root / 'evidence' / 'uart.log'
                # A join from an earlier boot with the same tag must not establish ownership.
                uart.write_text(UART[2].replace(f'event_us={JOIN_US}', 'event_us=1000') + '\n')
                probe.deadline = time.monotonic() + 3
                with self.assertRaisesRegex(ValueError, 'current-boot admission'):
                    probe.play()
                self.assertIsNone(probe.owned_uri)
                self.assertEqual([call for call in calls if call[0] == 'board'],
                                 [('board', '/api/sonos?broadcast=1', dict(action='play'))])
            with probe_fixture(root / 'second', [('PLAYING', 'OK')], scheme='broadcast', initial_state='stopped') as (probe, calls):
                uart = root / 'second' / 'evidence' / 'uart.log'
                uart.write_text('\n'.join(UART[:5]).replace(f'event_us={JOIN_US}', 'event_us=90000000') + '\n')
                probe.play()
                self.assertEqual((probe.owned_uri, probe.intent, probe.generation), (LOOP, 41, 7))
                self.assertEqual(probe.result['scheme_observed'], 'http')
                self.assertEqual(probe.result['broadcast_receipts'],
                                 [dict(kind='play-requested', broadcast=True), dict(kind='broadcast', enabled=True, uri=BROADCAST),
                                  dict(kind='echoed', uri=OWNED)])

    def test_plain_play_uses_the_ordinary_path_and_refuses_a_broadcast_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with probe_fixture(root, [('PLAYING', 'OK')], initial_state='stopped') as (probe, calls):
                uart = root / 'evidence' / 'uart.log'
                uart.write_text('\n'.join(UART[:5]).replace(f'event_us={JOIN_US}', 'event_us=90000000') + '\n')
                with self.assertRaisesRegex(ValueError, 'broadcast'):
                    probe.play()
                self.assertEqual([call for call in calls if call[0] == 'board'], [('board', '/api/sonos', dict(action='play'))])

    def test_await_close_brackets_the_phase_one_receipt_for_this_intent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with probe_fixture(root, [('PLAYING', 'OK')]) as (probe, calls):
                owned(probe)
                uart = root / 'evidence' / 'uart.log'
                uart.write_text('\n'.join(UART[:6]) + '\n')
                probe.deadline = time.monotonic() + 2.5
                with self.assertRaises(TimeoutError):
                    probe.await_close()
                uart.write_text('\n'.join(UART) + '\n')
                probe.deadline = time.monotonic() + 10
                marker = probe.await_close()
                self.assertEqual((marker['device_us'], marker['phase'], marker['receipt']['frames_sent']), (INJECTION_US, 1, 584))
                self.assertIs(probe.markers['close'], marker)

    def test_checked_stop_polls_same_speaker_until_remote_stopped_with_one_board_stop(self):
        with tempfile.TemporaryDirectory() as directory:
            with probe_fixture(Path(directory), [('PLAYING', 'OK'), ('PLAYING', 'OK'), ('STOPPED', 'OK')]) as (probe, calls):
                owned(probe)
                marker = probe.checked_stop()
                self.assertEqual([call for call in calls if call[0] == 'board'], [('board', '/api/sonos', dict(action='stop'))])
                self.assertEqual(marker['response'], dict(accepted=True))
                self.assertLess(marker['device_us'], marker['after_device_us'])
                self.assertEqual(probe.samples[-1]['transport_state'], 'STOPPED')
                self.assertGreaterEqual(sum(call == ('soap', 'GetTransportInfo') for call in calls), 3)
                self.assertFalse(probe.play_sent)

    def test_checked_stop_refuses_without_confirmed_ownership_or_on_a_foreign_uri(self):
        with tempfile.TemporaryDirectory() as directory:
            with probe_fixture(Path(directory), [('PLAYING', 'OK')]) as (probe, calls):
                with self.assertRaisesRegex(ValueError, 'refusing Stop'):
                    probe.checked_stop()
            with probe_fixture(Path(directory) / 'foreign', [('PLAYING', 'OK')], echo='x-rincon-queue:RINCON_X#0') as (probe, calls):
                owned(probe)
                with self.assertRaisesRegex(ValueError, 'refusing Stop'):
                    probe.checked_stop()
            self.assertEqual([call for call in calls if call[0] == 'board'], [])

    def test_checked_stop_respects_bounded_deadline(self):
        with tempfile.TemporaryDirectory() as directory:
            with probe_fixture(Path(directory), [('PLAYING', 'OK')]) as (probe, calls):
                owned(probe)
                probe.deadline = time.monotonic() + 1.5
                with self.assertRaises(TimeoutError):
                    probe.checked_stop()
                self.assertEqual([call for call in calls if call[0] == 'board'], [('board', '/api/sonos', dict(action='stop'))])


class Lifecycle(unittest.TestCase):
    def test_await_operator_is_bounded_and_returns_the_typed_line(self):
        read_fd, write_fd = os.pipe()
        with os.fdopen(read_fd) as reader, os.fdopen(write_fd, 'w') as writer:
            started = time.monotonic()
            self.assertIsNone(await_operator(reader, .2))
            self.assertLess(time.monotonic() - started, 1)
            writer.write('done\n')
            writer.flush()
            self.assertEqual(await_operator(reader, 2), 'done')

    def test_cleanup_without_board_writes_atomic_receipt_and_keeps_verdict(self):
        previous = signal.getsignal(signal.SIGALRM)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / 'image.elf'
            image.write_bytes(b'host cleanup image')
            probe = ResumeProbe(SimpleNamespace(out=root / 'evidence', image=image, host='127.0.0.1:9', cycle='close',
                                                scheme='broadcast', serial='/dev/null', previous_boot=None))
            probe.result['error'] = 'ValueError(\'fixture\')'
            try:
                probe.cleanup()
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                signal.signal(signal.SIGALRM, previous)
            receipt = json.loads((root / 'evidence' / 'result.json').read_text())
            self.assertFalse((root / 'evidence' / 'result.pending.json').exists())
            self.assertFalse(receipt['passed'])
            self.assertTrue(receipt['cleanup_complete'])
            self.assertEqual(receipt['cleanup_errors'], [])
            self.assertEqual((receipt['cycle'], receipt['scheme']), ('close', 'broadcast'))
            self.assertIsNone(receipt['verdict']['pass_a'])

    def test_out_must_not_exist(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'image.elf').write_bytes(b'x')
            with self.assertRaises(FileExistsError):
                ResumeProbe(SimpleNamespace(out=root, image=root / 'image.elf', host='127.0.0.1:9', cycle='close',
                                            scheme='plain', serial='/dev/null', previous_boot=None))

    def test_parse_args_validates_cycle_scheme_and_previous_boot(self):
        base = ['192.168.23.217', '--serial', '/dev/ttyUSB0', '--image', 'a.elf', '--out', 'evidence']
        args = parse_args(base + ['--cycle', 'close', '--scheme', 'plain'])
        self.assertEqual((args.cycle, args.scheme, args.previous_boot), ('close', 'plain', None))
        self.assertEqual(parse_args(base + ['--cycle', 'app-stop', '--scheme', 'broadcast', '--previous-boot', 'a' * 32]).previous_boot, 'a' * 32)
        with self.assertRaises(ValueError):
            parse_args(base + ['--cycle', 'close', '--scheme', 'plain', '--previous-boot', 'nope'])
        for bad in (['--cycle', 'other', '--scheme', 'plain'], ['--cycle', 'close', '--scheme', 'tls'], ['--help']):
            with self.assertRaises(SystemExit), redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                parse_args(base + bad)


if __name__ == '__main__':
    unittest.main()
