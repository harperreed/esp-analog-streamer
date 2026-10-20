# ABOUTME: Checks the stream-soak's periodic receiver-side stall option on the host: flag validation,
# ABOUTME: the injection schedule, the continuity verdict and the teardown; never shapes traffic or contacts a board.
import contextlib
import io
import json
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace

import stream_soak as soak

SCRIPTS = Path(__file__).resolve().parent
STALL_FLAGS = ['--stall-every', '300', '--stall-seconds', '4', '--interface', 'enp5s0']
# The decoder argv a plain MP3 soak built before stalls existed; it must not change without the flags.
PLAIN_MP3_ARGV = ['ffmpeg', '-nostdin', '-nostats', '-v', 'level+info', '-xerror', '-stats_period', '1', '-progress', 'p',
                  '-rw_timeout', '10000000', '-err_detect', 'crccheck', '-f', 'mp3', '-i', 'http://192.0.2.1:8000/stream.mp3',
                  '-t', '4.0', '-c:a', 'pcm_s32le', '-f', 'framehash', '-flush_packets', '1', 'h']
# Captured from `tc qdisc show dev enp5s0` on the workstation, 2026-10-13 (iproute2-6.19.0).
ENP5S0_PLAIN = 'qdisc mq 0: root\nqdisc fq_codel 0: parent :4 limit 10240p flows 1024 quantum 1514 target 5ms interval 100ms memory_limit 32Mb ecn drop_batch 64\n'


def parse(argv):
    with contextlib.redirect_stderr(io.StringIO()):
        return soak.parse_args(argv)


class StallOptionTests(unittest.TestCase):
    def test_stall_flags_need_each_other_an_interface_and_mp3(self):
        args = parse(['--path', '/stream.mp3', *STALL_FLAGS])
        self.assertEqual((args.stall_every, args.stall_seconds, args.interface), (300.0, 4.0, 'enp5s0'))
        for argv in (['--path', '/stream.mp3', '--stall-every', '300'],
                     ['--path', '/stream.mp3', '--stall-seconds', '4'],
                     ['--path', '/stream.mp3', '--stall-every', '300', '--stall-seconds', '4'],
                     ['--path', '/stream.mp3', '--interface', 'enp5s0'],
                     ['--path', '/stream.mp3', '--stall-every', '4', '--stall-seconds', '4', '--interface', 'enp5s0'],
                     ['--path', '/stream.mp3', '--stall-every', '300', '--stall-seconds', '4', '--interface', 'bad name'],
                     STALL_FLAGS):
            with self.subTest(argv=argv), self.assertRaises(SystemExit) as caught:
                parse(argv)
            self.assertEqual(caught.exception.code, 2)
        off = parse([])
        self.assertEqual((off.stall_every, off.stall_seconds, off.interface), (None, None, None))

    def test_schedule_keeps_every_stall_inside_the_run(self):
        schedule = soak.stall_schedule(21600, 300, 4)
        self.assertEqual(schedule, [300.0 * k for k in range(1, 72)])
        self.assertEqual(schedule[-1], 21300.0)
        self.assertTrue(all(start + 4 <= 21600 for start in schedule))
        self.assertEqual(soak.stall_schedule(100, 50, 4), [50.0])
        self.assertEqual(soak.stall_schedule(10, 300, 4), [])

    def test_decoder_arguments_stretch_timeouts_only_with_stalls(self):
        plain = soak.decoder_arguments(parse(['--path', '/stream.mp3', '--seconds', '4']))
        self.assertEqual(soak.decoder_command(plain, '192.0.2.1', Path('p'), Path('h')), PLAIN_MP3_ARGV)
        self.assertFalse(hasattr(plain, 'rw_timeout_s'))
        stalled = soak.decoder_arguments(parse(['--path', '/stream.mp3', '--seconds', '4', *STALL_FLAGS]))
        # The board keeps a live listener for its 24 s write budget after a blackhole (TCP retransmit backoff can
        # delay the first retransmit by 12 s); the soak's decoders allow that budget plus the stall, so the harness
        # never ends a run the board did not (oct20-soak-1 ended at a 14 s allowance while the board held on).
        self.assertEqual((stalled.rw_timeout_s, stalled.progress_timeout_s), (28.0, 38.0))
        self.assertEqual(stalled.rw_timeout_s, soak.fault_tools().LIVE_WRITE_DEADLINE_S + 4)
        command = soak.decoder_command(stalled, '192.0.2.1', Path('p'), Path('h'))
        self.assertEqual(command[command.index('-rw_timeout') + 1], '28000000')
        self.assertEqual([item for item in command if item != '28000000'], [item for item in PLAIN_MP3_ARGV if item != '10000000'])

    def test_continuity_tolerates_discontinuities_and_short_frames_up_to_the_board_skips(self):
        expected = 4 * 48000
        decoder = dict(decoded_audio_frames=expected, decoded_discontinuities=0)
        self.assertIsNone(soak.continuity_problem(decoder, expected, skips=0, frames_skipped=0))
        decoder = dict(decoded_audio_frames=expected - 3 * 1152, decoded_discontinuities=2)
        self.assertIsNone(soak.continuity_problem(decoder, expected, skips=2, frames_skipped=2))
        self.assertIn('exceed', soak.continuity_problem(decoder, expected, skips=1, frames_skipped=2))
        self.assertIn('outside', soak.continuity_problem(decoder, expected, skips=2, frames_skipped=1))
        decoder = dict(decoded_audio_frames=expected + 1, decoded_discontinuities=0)
        self.assertIn('outside', soak.continuity_problem(decoder, expected, skips=0, frames_skipped=0))

    def test_restore_tree_attempts_every_step_and_verifies_the_listing(self):
        ran = []
        verified, errors = soak.restore_tree(ran.append, lambda: ENP5S0_PLAIN, 'enp5s0', '192.168.23.217', ENP5S0_PLAIN)
        self.assertEqual(ran, [['tc', 'qdisc', 'del', 'dev', 'ifb0', 'root'], ['tc', 'qdisc', 'del', 'dev', 'enp5s0', 'ingress'],
                               ['tc', 'qdisc', 'del', 'dev', 'enp5s0', 'root'], ['ip', 'link', 'del', 'ifb0']])
        self.assertEqual((verified, errors), (True, []))

        def failing(argv):
            ran.append(argv)
            if argv[-1] == 'ingress':
                raise RuntimeError('no ingress')
        ran.clear()
        verified, errors = soak.restore_tree(failing, lambda: ENP5S0_PLAIN.replace('mq', 'prio'), 'enp5s0', '192.168.23.217', ENP5S0_PLAIN)
        self.assertEqual(len(ran), 4, 'a failed step must not skip the remaining teardown')
        self.assertFalse(verified)
        self.assertEqual(errors, ['tc qdisc del dev enp5s0 ingress: no ingress'])

    def test_decoder_exit_check_allows_only_the_recorded_short_frames(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000',
                            '-ac', '2', '-t', '0.2', '-c:a', 'libmp3lame', '-b:a', '320k', str(work / 'clip.mp3')],
                           check=True, timeout=30, capture_output=True)
            errors = work / 'log'
            with errors.open('wb') as log:
                process = subprocess.Popen(['ffmpeg', '-nostdin', '-nostats', '-v', 'level+info', '-xerror', '-progress', str(work / 'progress'),
                                            '-f', 'mp3', '-i', str(work / 'clip.mp3'), '-c:a', 'pcm_s32le', '-f', 'framehash',
                                            str(work / 'hashes')], stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
                self.assertEqual(process.wait(timeout=30), 0)
            accounting = soak.FrameHash(work / 'hashes')
            accounting.update(final=True)
            decoded = accounting.summary()['decoded_audio_frames']
            accounting.close()
            self.assertGreater(decoded, 0)
            requested = (decoded + 3 * 1152) / 48000

            def job():
                return dict(label='short', process=process, tail=soak.Tail(work / 'progress'), errors=errors, log_tail=soak.Tail(errors),
                            accounting=soak.FrameHash(work / 'hashes'), decoded_audio_frames=0, decoded_seconds=0, timeline_seconds=0,
                            advanced_at=time.monotonic())
            base = dict(path='/stream.mp3', stream_port=8000, seconds=requested)
            for allowed, passes in ((0, False), (3 * 1152 - 1, False), (3 * 1152, True)):
                with self.subTest(allowed=allowed):
                    current = job()
                    try:
                        if passes:
                            soak.update_decoder(current, SimpleNamespace(**base, frames_short_allowed=allowed))
                        else:
                            with self.assertRaisesRegex(soak.Failure, f'decoded={decoded} actual frames'):
                                soak.update_decoder(current, SimpleNamespace(**base, frames_short_allowed=allowed))
                    finally:
                        current['tail'].close()
                        current['log_tail'].close()
                        current['accounting'].close()


class StallLifecycleTests(unittest.TestCase):
    def test_help_names_the_stall_flags_and_creates_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([str(SCRIPTS / 'stream-soak'), '--help'], cwd=directory, capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)
            for flag in ('--stall-every', '--stall-seconds', '--interface'):
                self.assertIn(flag, result.stdout)
            self.assertEqual(list(Path(directory).iterdir()), [])
        wrapper = (SCRIPTS / 'stream-soak').read_text()
        self.assertIn('--stall-every', wrapper)

    def test_loopback_refusal_keeps_event_shape_and_runs_no_sudo(self):
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        import threading

        class StateHandler(BaseHTTPRequestHandler):
            def do_GET(self):
                body = b'{"shazam_enabled": true}'
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args):
                pass
        server = ThreadingHTTPServer(('127.0.0.1', 0), StateHandler)
        worker = threading.Thread(target=server.serve_forever)
        worker.start()
        try:
            with tempfile.TemporaryDirectory() as directory:
                shapes = {}
                for name, extra in (('plain', []), ('stalls', ['--path', '/stream.mp3', *STALL_FLAGS])):
                    result = subprocess.run([str(SCRIPTS / 'stream-soak'), '127.0.0.1', '--seconds', '1', '--state-port', str(server.server_port),
                                             '--out', name, *extra], cwd=directory, capture_output=True, text=True, timeout=30)
                    self.assertEqual(result.returncode, 1, result.stderr)
                    summaries = list((Path(directory) / name).glob('*/summary.json'))
                    self.assertEqual(len(summaries), 1, result.stdout)
                    summary = json.loads(summaries[0].read_text())
                    self.assertEqual(summary['error'], '/api/state must report shazam_enabled=false')
                    events = [json.loads(line) for line in (summaries[0].parent / 'events.jsonl').read_text().splitlines()]
                    shapes[name] = [(event['kind'], sorted(event)) for event in events]
                    self.assertNotIn('command', [event['kind'] for event in events], 'the sudo probe must follow the state refusal')
                    if name == 'plain':
                        self.assertEqual(shapes[name], [('failure', ['elapsed_seconds', 'error', 'kind'])])
                        self.assertEqual((summary['stalls'], summary['restore_verified'], summary['skips']), (None, None, None))
                    else:
                        self.assertEqual(summary['stalls']['count'], 0)
                        self.assertEqual(summary['stalls']['scheduled'], 0)
                        self.assertEqual(summary['stalls']['interface'], 'enp5s0')
                        self.assertIsNone(summary['restore_verified'])
                self.assertEqual(shapes['plain'], shapes['stalls'])
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=2)


if __name__ == '__main__':
    unittest.main()
