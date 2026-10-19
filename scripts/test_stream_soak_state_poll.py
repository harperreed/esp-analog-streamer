# ABOUTME: Checks the stream-soak's /api/state poll on the host against a loopback server: a slow or refused
# ABOUTME: answer is recorded and tolerated until the board has gone silent for the limit; never contacts a board.
import json
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import stream_soak as soak


class Clock:
    """A settable monotonic clock so the silence limit is judged without waiting it out."""
    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now


class StatePollTests(unittest.TestCase):
    def serve(self, behaviour):
        tests = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                if behaviour['mode'] == 'slow':
                    time.sleep(behaviour['sleep'])
                body = json.dumps(dict(shazam_enabled=behaviour.get('shazam_enabled', False), level_dbfs=-68.1)).encode()
                try:
                    self.send_response(200)
                    self.send_header('Content-Type', 'application/json')
                    self.send_header('Content-Length', str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        tests.addCleanup(server.server_close)
        tests.addCleanup(server.shutdown)
        return server

    def poller(self, server, clock, timeout=0.2):
        events = []
        poll = soak.StatePoll(f'http://127.0.0.1:{server.server_port}/api/state',
                              lambda kind, **fields: events.append(dict(kind=kind, **fields)), timeout=timeout, clock=clock)
        return poll, events

    def test_an_answer_counts_and_records_its_duration(self):
        clock = Clock()
        poll, events = self.poller(self.serve(dict(mode='answer')), clock)
        self.assertTrue(poll())
        self.assertEqual(poll.polls, 1)
        self.assertEqual(events[-1]['kind'], 'state')
        self.assertEqual(events[-1]['level_dbfs'], -68.1)
        self.assertIn('seconds', events[-1])

    def test_a_slow_answer_is_recorded_and_tolerated_inside_the_limit(self):
        clock = Clock()
        poll, events = self.poller(self.serve(dict(mode='slow', sleep=0.6)), clock)
        clock.now += soak.STATE_UNANSWERED_LIMIT_S  # silent for exactly the limit: still tolerated
        self.assertFalse(poll())
        self.assertEqual(poll.polls, 0)
        self.assertEqual(events[-1]['kind'], 'state_unanswered')
        self.assertIn('timed out', events[-1]['error'])
        self.assertEqual(events[-1]['unanswered_seconds'], soak.STATE_UNANSWERED_LIMIT_S)
        self.assertEqual(poll.unanswered, 1)

    def test_silence_past_the_limit_fails_the_run(self):
        clock = Clock()
        poll, events = self.poller(self.serve(dict(mode='slow', sleep=0.6)), clock)
        clock.now += soak.STATE_UNANSWERED_LIMIT_S + 0.5
        with self.assertRaises(soak.Failure) as caught:
            poll()
        self.assertIn('unanswered', str(caught.exception))
        self.assertEqual(events[-1]['kind'], 'state_unanswered')

    def test_an_answer_resets_the_silence(self):
        clock = Clock()
        behaviour = dict(mode='slow', sleep=0.6)
        poll, events = self.poller(self.serve(behaviour), clock)
        clock.now += 20
        poll()  # unanswered at 20 s
        behaviour['mode'] = 'answer'
        clock.now += 1
        poll()  # answered at 21 s
        behaviour['mode'] = 'slow'
        clock.now += 20
        poll()  # 20 s since the answer, not 41 s since the start
        self.assertEqual([event['kind'] for event in events], ['state_unanswered', 'state', 'state_unanswered'])
        self.assertEqual(poll.polls, 1)
        self.assertEqual(poll.unanswered, 2)

    def test_a_refused_connection_is_a_transient_too(self):
        clock = Clock()
        server = self.serve(dict(mode='answer'))
        poll, events = self.poller(server, clock)
        server.shutdown()
        server.server_close()
        poll()
        self.assertEqual(events[-1]['kind'], 'state_unanswered')
        self.assertEqual(poll.unanswered, 1)

    def test_the_required_gate_fails_on_any_unanswered_poll(self):
        clock = Clock()
        poll, events = self.poller(self.serve(dict(mode='slow', sleep=0.6)), clock)
        with self.assertRaises(soak.Failure) as caught:
            poll(required=True)
        self.assertIn('timed out', str(caught.exception))
        self.assertEqual(events[-1]['kind'], 'state_unanswered')

    def test_startup_polls_never_fail_on_silence_and_report_the_answer(self):
        clock = Clock()
        behaviour = dict(mode='slow', sleep=0.6)
        poll, events = self.poller(self.serve(behaviour), clock)
        clock.now += soak.STATE_UNANSWERED_LIMIT_S + 30
        self.assertFalse(poll(starting=True))
        self.assertEqual(events[-1]['kind'], 'state_unanswered')
        behaviour['mode'] = 'answer'
        self.assertTrue(poll(starting=True))
        self.assertEqual(events[-1]['kind'], 'state')

    def test_recognition_enabled_fails_even_when_answered(self):
        clock = Clock()
        poll, _ = self.poller(self.serve(dict(mode='answer', shazam_enabled=True)), clock)
        with self.assertRaises(soak.Failure):
            poll()


if __name__ == '__main__':
    unittest.main()
