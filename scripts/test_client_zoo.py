# ABOUTME: Checks the client-zoo adapters, progress readers and verdicts on the host with the real players.
# ABOUTME: Serves a paced loopback stream from an ffmpeg-made fixture; never shapes traffic or contacts a board.
import contextlib
import io
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path

import client_zoo as zoo
import network_fault as nf

SCRIPTS = Path(__file__).resolve().parent
BOARD = '192.168.23.217'
PLAYERS = ('ffmpeg', 'mpv', 'mpg123', 'sox', 'curl')

# Real stderr of `sox -S -t mp3 - -n` fed by curl from the loopback stream, 2026-10-15 (SoX v14.4.2); lines end in \r.
SOX_PROGRESS = ('Input File     : \'-\' (mp3)\nChannels       : 2\nSample Rate    : 48000\n\n'
                'In:0.00% 00:00:04.44 [00:00:00.00] Out:209k  [     =|=     ]        Clip:0    \r'
                'In:0.00% 00:00:04.69 [00:00:00.00] Out:221k  [     =|=     ]        Clip:0    \r'
                'In:0.00% 00:00:05.02 [00:00:00.00] Out:241k  [     =|=     ]        Clip:0    \r')
SOX_FINITE = ('In:0.00% 00:00:00.00 [00:00:40.06] Out:0     [      |      ]        Clip:0    \r'
              'In:5.11% 00:00:02.05 [00:00:38.01] Out:96.0k [     =|=     ]        Clip:0    \r'
              'In:100%  00:00:40.00 [00:00:00.00] Out:1.92M [     =|=     ]        Clip:0    \rDone.\n')
# Real --log-file lines of mpv v0.41.0 opening the loopback stream.
MPV_LOG = ('[   0.003][v][stream_callback] Opening http://127.0.0.1:18000/stream.mp3\n'
           '[   0.003][v][ffmpeg] Opening http://127.0.0.1:18000/stream.mp3\n'
           '[   0.003][d][ffmpeg] tcp: Starting connection attempt to 127.0.0.1 port 18000\n'
           '[   0.003][d][ffmpeg] tcp: Successfully connected to 127.0.0.1 port 18000\n'
           '[   0.517][v][cplayer] Opening done: http://127.0.0.1:18000/stream.mp3\n'
           '[   0.519][i][cplayer] AO: [null] 48000Hz stereo 2ch floatp\n')
MPV_EOF = '[   1.606][v][cplayer] audio EOF reached\n[   1.606][i][cplayer] Exiting... (End of file)\n'
# Real ffmpeg 8.0.1 `-v level+verbose` lines for one HTTP open.
FFMPEG_LOG = ('[tcp @ 0x56572fd76540] [verbose] Starting connection attempt to 127.0.0.1 port 18000\n'
              '[tcp @ 0x56572fd76540] [verbose] Successfully connected to 127.0.0.1 port 18000\n'
              "[info] Input #0, mp3, from 'http://127.0.0.1:18000/stream.mp3':\n")
# Real curl 8.18.0 `-v` trace head.
CURL_LOG = ('*   Trying 127.0.0.1:18000...\n* Established connection to 127.0.0.1 (127.0.0.1 port 18000) from 127.0.0.1 port 37776 \n'
            '* using HTTP/1.x\n> GET /stream.mp3 HTTP/1.1\n> Host: 127.0.0.1:18000\n> \n< HTTP/1.1 200 OK\n< Content-Type: audio/mpeg\n')
# Real mpv IPC replies.
MPV_REPLY = '{"data":3.456982,"request_id":0,"error":"success"}'
MPV_STREAM_POS = '{"data":155564,"request_id":0,"error":"success"}'


class LoopbackStream(threading.Thread):
    """Serves fixture MP3/FLAC as an unbounded Icecast-shaped body at real-time pace; pause() stalls every sender."""
    FIXTURES = {'/stream.mp3': ('audio/mpeg', 'fixture.mp3'), '/stream.flac': ('audio/flac', 'fixture.flac')}

    def __init__(self, root):
        super().__init__(daemon=True)
        self.root = root
        self.server = socket.socket()
        self.server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.server.bind(('127.0.0.1', 0))
        self.server.listen(8)
        self.port = self.server.getsockname()[1]
        self.flowing = threading.Event()
        self.flowing.set()
        self.connections = []
        self.closing = False

    def url(self, fmt):
        return f'http://127.0.0.1:{self.port}/stream.{fmt}'

    def pause(self):
        self.flowing.clear()

    def resume(self):
        self.flowing.set()

    def run(self):
        while not self.closing:
            try:
                conn, _ = self.server.accept()
            except OSError:
                return
            self.connections.append(time.monotonic())
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    def serve(self, conn):
        try:
            conn.settimeout(5)
            request = b''
            while b'\r\n\r\n' not in request:
                chunk = conn.recv(1024)
                if not chunk:
                    return
                request += chunk
            path = request.split(b' ')[1].decode().split('?')[0]
            if path not in self.FIXTURES:
                conn.sendall(b'HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n')
                return
            content_type, name = self.FIXTURES[path]
            conn.sendall((f'HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nCache-Control: no-cache, no-store\r\n'
                          'Connection: close\r\nAccept-Ranges: none\r\nicy-name: loopback\r\n\r\n').encode())
            data = (self.root / name).read_bytes()
            rate = len(data) / FIXTURE_SECONDS  # the fixture's own byte rate; the body wraps to the file head after FIXTURE_SECONDS
            started, sent = time.monotonic(), 0
            while not self.closing:
                self.flowing.wait()
                offset = sent % len(data)
                chunk = data[offset:offset + 4096]
                conn.sendall(chunk)
                sent += len(chunk)
                time.sleep(max(0.0, sent / rate - (time.monotonic() - started)))
        except OSError:
            pass
        finally:
            conn.close()

    def close(self):
        self.closing = True
        self.server.close()


FIXTURE_SECONDS = 20


def make_fixtures(root):
    for fmt, codec in (('mp3', ['-c:a', 'libmp3lame', '-b:a', '320k']), ('flac', ['-c:a', 'flac'])):
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i',
                        f'sine=frequency=440:sample_rate=48000:duration={FIXTURE_SECONDS}', '-ac', '2', *codec, '-f', fmt,
                        str(root / f'fixture.{fmt}')], check=True, timeout=60)


class AdapterTests(unittest.TestCase):
    def test_the_five_players_are_registered_and_firefox_is_not(self):
        self.assertEqual(tuple(zoo.PLAYERS), PLAYERS)
        self.assertNotIn('firefox', zoo.PLAYERS)
        with self.assertRaises(NotImplementedError):
            zoo.FirefoxAdapter().pipeline('http://127.0.0.1:1/stream.mp3', 'mp3', Path('out'), 'firefox-mp3', 10, 4, Path('scratch'))
        for name, adapter in zoo.PLAYERS.items():
            self.assertEqual(adapter.name, name)
            self.assertIn('mp3', adapter.formats)
        self.assertEqual(zoo.PLAYERS['mpg123'].formats, ('mp3',))

    def test_pipelines_name_the_player_flags_and_output_files(self):
        out, scratch = Path('/out'), Path('/scratch')
        url = f'http://{BOARD}:8000/stream.mp3'
        ffmpeg = zoo.PLAYERS['ffmpeg'].pipeline(url, 'mp3', out, 'ffmpeg-mp3', 30, 4, scratch)
        self.assertEqual(len(ffmpeg), 1)
        argv = ffmpeg[0]
        self.assertEqual(argv[0], 'ffmpeg')
        self.assertEqual(argv[argv.index('-i') - 2:argv.index('-i') + 2], ['-f', 'mp3', '-i', url])
        self.assertEqual(argv[argv.index('-rw_timeout') + 1], str((nf.DECODER_RW_TIMEOUT_S + 4) * 1_000_000))
        self.assertEqual(argv[-5:], ['-f', 'framehash', '-flush_packets', '1', '/out/ffmpeg-mp3.framehash'])
        self.assertIn('level+verbose', argv)
        self.assertEqual(argv[argv.index('-t') + 1], '39')
        flac = zoo.PLAYERS['ffmpeg'].pipeline(url.replace('mp3', 'flac'), 'flac', out, 'ffmpeg-flac', 30, 4, scratch)[0]
        self.assertEqual(flac[flac.index('-i') - 2:flac.index('-i')], ['-f', 'flac'])
        mpv = zoo.PLAYERS['mpv'].pipeline(url, 'mp3', out, 'mpv-mp3', 30, 4, scratch)[0]
        self.assertEqual(mpv[:4], ['mpv', '--no-video', '--ao=null', '--really-quiet'])
        self.assertIn('--log-file=/out/mpv-mp3.mpv.log', mpv)
        self.assertIn('--input-ipc-server=/scratch/mpv-mp3.sock', mpv)
        self.assertEqual(mpv[-1], url)
        mpg123 = zoo.PLAYERS['mpg123'].pipeline(url, 'mp3', out, 'mpg123-mp3', 30, 4, scratch)[0]
        self.assertEqual(mpg123, ['mpg123', '-q', '--no-control', '-w', '/out/mpg123-mp3.wav', url])
        sox = zoo.PLAYERS['sox'].pipeline(url, 'mp3', out, 'sox-mp3', 30, 4, scratch)
        self.assertEqual(len(sox), 2)
        self.assertEqual(sox[0][:4], ['curl', '-sS', '-v', '-N'])
        self.assertEqual(sox[0][-1], url)
        self.assertEqual(sox[1], ['sox', '-S', '-t', 'mp3', '-', '-n'])
        self.assertEqual(zoo.PLAYERS['sox'].pipeline(url, 'flac', out, 'sox-flac', 30, 4, scratch)[1][2:4], ['-t', 'flac'])
        curl = zoo.PLAYERS['curl'].pipeline(url, 'mp3', out, 'curl-mp3', 30, 4, scratch)[0]
        self.assertEqual(curl[:4], ['curl', '-sS', '-v', '-N'])
        self.assertEqual(curl[curl.index('-o') + 1], '/out/curl-mp3.mp3')
        self.assertEqual(curl[curl.index('--max-time') + 1], '39')
        self.assertEqual(curl[-1], url)
        for name in PLAYERS:
            for argv in zoo.PLAYERS[name].pipeline(url, 'mp3', out, f'{name}-mp3', 30, 4, scratch):
                self.assertTrue(all(isinstance(item, str) for item in argv), name)


class ProgressParsingTests(unittest.TestCase):
    def test_sox_progress_reads_the_last_out_count_through_carriage_returns(self):
        self.assertEqual(zoo.sox_samples(SOX_PROGRESS), 241_000)
        self.assertEqual(zoo.sox_samples(SOX_FINITE), 1_920_000)
        self.assertEqual(zoo.sox_samples(SOX_FINITE.split('\r')[1]), 96_000)
        self.assertEqual(zoo.sox_samples('Input File : -\n'), 0)

    def test_mpv_reply_parses_data_and_rejects_errors(self):
        self.assertAlmostEqual(zoo.mpv_reply(MPV_REPLY), 3.456982)
        self.assertEqual(zoo.mpv_reply(MPV_STREAM_POS), 155564)
        self.assertIsNone(zoo.mpv_reply('{"request_id":0,"error":"property unavailable"}'))
        self.assertIsNone(zoo.mpv_reply('{"event":"file-loaded"}'))

    def test_connection_counts_come_from_each_players_own_log(self):
        self.assertEqual(zoo.PLAYERS['mpv'].connections(MPV_LOG), 1)
        self.assertEqual(zoo.PLAYERS['mpv'].connections(MPV_LOG + MPV_EOF + MPV_LOG), 2)
        self.assertEqual(zoo.PLAYERS['ffmpeg'].connections(FFMPEG_LOG), 1)
        self.assertEqual(zoo.PLAYERS['ffmpeg'].connections(FFMPEG_LOG * 3), 3)
        self.assertEqual(zoo.PLAYERS['curl'].connections(CURL_LOG), 1)
        self.assertEqual(zoo.PLAYERS['sox'].connections(CURL_LOG + CURL_LOG), 2)
        self.assertIsNone(zoo.PLAYERS['mpg123'].connections('anything'))
        self.assertEqual(zoo.PLAYERS['mpv'].connections(''), 0)

    def test_wav_samples_count_data_after_the_header(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'x.wav'
            self.assertEqual(zoo.wav_samples(path), 0)
            path.write_bytes(b'\0' * 44)
            self.assertEqual(zoo.wav_samples(path), 0)
            path.write_bytes(b'\0' * (44 + 4 * 480))
            self.assertEqual(zoo.wav_samples(path), 480)


class VerdictTests(unittest.TestCase):
    def samples(self, *values):
        return [dict(at_s=float(index), progress=value) for index, value in enumerate(values)]

    def test_receiving_at_end_needs_progress_inside_the_final_window(self):
        self.assertTrue(zoo.receiving_at_end(self.samples(0, 1, 2, 3, 4, 5, 6, 7, 8, 9), 5.0))
        self.assertFalse(zoo.receiving_at_end(self.samples(0, 1, 2, 3, 4, 5, 5, 5, 5, 5, 5, 5), 5.0))
        self.assertTrue(zoo.receiving_at_end(self.samples(0, 1, 2, 3, 4, 5, 5, 5, 5, 5, 5, 6), 5.0))
        self.assertFalse(zoo.receiving_at_end(self.samples(0), 5.0))
        self.assertFalse(zoo.receiving_at_end([], 5.0))
        self.assertFalse(zoo.receiving_at_end(self.samples(None, None, 3, None), 5.0))

    def record(self, **overrides):
        base = dict(player='mpv', format='mp3', samples=self.samples(*range(31)), board_joins=1, player_connections=1,
                    exit_codes=[None], ended_early=False, skips_before=0, skips_during=1, departure_skips=1)
        return dict(base, **overrides)

    def test_pass_needs_no_reconnect_and_progress_at_the_end(self):
        verdict = zoo.pair_verdict(self.record())
        self.assertTrue(verdict['passed'], verdict)
        self.assertEqual((verdict['reconnects'], verdict['receiving_at_end']), (0, True))
        self.assertIn('skips', verdict)
        self.assertFalse(zoo.pair_verdict(self.record(board_joins=2))['passed'])
        self.assertEqual(zoo.pair_verdict(self.record(board_joins=2))['reconnects'], 1)
        self.assertFalse(zoo.pair_verdict(self.record(player_connections=2))['passed'])
        self.assertFalse(zoo.pair_verdict(self.record(samples=self.samples(*range(20), *[19] * 11)))['passed'])
        self.assertFalse(zoo.pair_verdict(self.record(ended_early=True, exit_codes=[1]))['passed'])
        # No join line at all is unknown on the board side; the player's own count still decides.
        unknown = zoo.pair_verdict(self.record(board_joins=0))
        self.assertTrue(unknown['passed'], unknown)
        self.assertIsNone(unknown['board_reconnects'])
        quiet = zoo.pair_verdict(self.record(player_connections=None))
        self.assertTrue(quiet['passed'], quiet)
        self.assertFalse(zoo.pair_verdict(self.record(player_connections=None, board_joins=3))['passed'])

    def test_budget_covers_every_pair_with_its_stall(self):
        self.assertEqual(zoo.budget_seconds(10, 30, 4), 180 + 10 * (30 + 4 + zoo.PAIR_MARGIN_S))

    def test_pairs_follow_player_order_and_skip_formats_a_player_lacks(self):
        self.assertEqual(zoo.pairs(['mpg123', 'ffmpeg'], ['mp3', 'flac']), [('ffmpeg', 'mp3'), ('ffmpeg', 'flac'), ('mpg123', 'mp3')])
        self.assertEqual(zoo.player_names('all'), list(PLAYERS))
        self.assertEqual(zoo.player_names('curl,ffmpeg'), ['ffmpeg', 'curl'])
        for bad in ('firefox', 'ffmpeg,ffmpeg', ''):
            with self.assertRaises(ValueError):
                zoo.player_names(bad)
        self.assertEqual(zoo.format_names('flac'), ['flac'])
        with self.assertRaises(ValueError):
            zoo.format_names('wav')


class LoopbackTests(unittest.TestCase):
    """Every registered player against the paced loopback stream, through a 1 s sender stall, with its real progress reader."""
    @classmethod
    def setUpClass(cls):
        cls.root = Path(tempfile.mkdtemp(prefix='zoo-fixtures-'))
        make_fixtures(cls.root)
        cls.stream = LoopbackStream(cls.root)
        cls.stream.start()

    @classmethod
    def tearDownClass(cls):
        cls.stream.close()
        shutil.rmtree(cls.root, ignore_errors=True)

    def run_pair(self, name, fmt):
        adapter = zoo.PLAYERS[name]
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory(prefix='zoo-', dir='/tmp') as scratch:
            out = Path(directory)
            started = time.monotonic()
            opened = len(self.stream.connections)
            record = zoo.run_pair(adapter, fmt, self.stream.url(fmt), out, f'{name}-{fmt}', seconds=4.5, steady_s=1.5, stall_s=1.0,
                                  inject=self.stream.pause, remove=self.stream.resume, scratch=Path(scratch),
                                  elapsed=lambda: time.monotonic() - started)
            record['server_connections'] = len(self.stream.connections) - opened
            record['files'] = sorted(path.name for path in out.iterdir())
            return record

    def check(self, name, fmt):
        record = self.run_pair(name, fmt)
        self.assertEqual(record['server_connections'], 1, record)
        self.assertEqual(record['player_connections'], 1 if name != 'mpg123' else None, record)
        progress = [sample['progress'] for sample in record['samples'] if sample['progress'] is not None]
        self.assertTrue(progress and progress[-1] > 0, record)
        self.assertEqual(progress, sorted(progress), record)
        self.assertFalse(record['ended_early'], record)
        self.assertTrue(zoo.receiving_at_end(record['samples'], 2.0), record)
        self.assertEqual(record['stall'], dict(installed_s=record['stall']['installed_s'], removed_s=record['stall']['removed_s']))
        self.assertGreaterEqual(record['stall']['removed_s'] - record['stall']['installed_s'], 1.0)
        self.assertEqual(record['unit'], zoo.PLAYERS[name].unit)
        self.assertTrue(all(code is not None for code in record['exit_codes']), record)
        return record

    def test_ffmpeg_decodes_both_formats(self):
        for fmt in ('mp3', 'flac'):
            record = self.check('ffmpeg', fmt)
            self.assertGreater(record['samples'][-1]['progress'], 48000, record)

    def test_mpv_reports_playback_time_over_ipc(self):
        for fmt in ('mp3', 'flac'):
            record = self.check('mpv', fmt)
            self.assertGreater(record['samples'][-1]['progress'], 1.0, record)
            self.assertIn('stream_pos', record['samples'][-1]['detail'])

    def test_mpg123_writes_samples(self):
        self.assertGreater(self.check('mpg123', 'mp3')['samples'][-1]['progress'], 48000)

    def test_sox_counts_output_samples(self):
        for fmt in ('mp3', 'flac'):
            self.assertGreater(self.check('sox', fmt)['samples'][-1]['progress'], 48000)

    def test_curl_counts_bytes(self):
        self.assertGreater(self.check('curl', 'mp3')['samples'][-1]['progress'], 40_000)

    def test_a_player_that_dies_is_recorded_as_ended_early(self):
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory(prefix='zoo-', dir='/tmp') as scratch:
            started = time.monotonic()
            record = zoo.run_pair(zoo.PLAYERS['curl'], 'mp3', f'http://127.0.0.1:{self.stream.port}/missing', Path(directory), 'curl-mp3',
                                  seconds=3, steady_s=1, stall_s=0.5, inject=self.stream.pause, remove=self.stream.resume,
                                  scratch=Path(scratch), elapsed=lambda: time.monotonic() - started)
            self.assertTrue(record['ended_early'], record)
            self.assertTrue(self.stream.flowing.is_set())
            self.assertFalse(zoo.pair_verdict(dict(record, board_joins=1, skips_before=0, skips_during=0, departure_skips=None))['passed'])


class CommandLineTests(unittest.TestCase):
    def test_dry_run_prints_every_argv_and_creates_nothing(self):
        wrapper = SCRIPTS / 'client-zoo'
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'zoo-out'
            result = subprocess.run([str(wrapper), '--dry-run', BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', str(out),
                                     '--players', 'mpv,curl', '--formats', 'mp3', '--stall', '4'],
                                    cwd=directory, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(list(Path(directory).iterdir()), [])
            lines = result.stdout.splitlines()
            self.assertIn('sudo -n tc qdisc replace dev lo root handle 1: prio bands 4', lines)
            self.assertIn('sudo -n tc qdisc change dev ifb0 root netem loss 100%', lines)
            self.assertIn('sudo -n tc qdisc change dev ifb0 root netem loss 0%', lines)
            self.assertEqual(lines[-1], 'sudo -n ip link del ifb0')
            self.assertTrue(any(line.startswith('# player mpv mp3: mpv --no-video --ao=null') for line in lines), lines)
            self.assertTrue(any(line.startswith('# player curl mp3: curl -sS -v -N') for line in lines), lines)
            self.assertFalse(any('ffmpeg' in line for line in lines))

    def test_help_bad_arguments_and_existing_out_create_nothing(self):
        wrapper = SCRIPTS / 'client-zoo'
        with tempfile.TemporaryDirectory() as directory:
            existing = Path(directory) / 'taken'
            existing.mkdir()
            cases = [(['--help'], 0), (['--unrecognized-option'], 2), ([], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', str(existing), '--dry-run'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', 'x', '--players', 'firefox', '--dry-run'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', 'x', '--formats', 'wav', '--dry-run'], 2),
                     ([BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', 'x', '--stall', '0', '--dry-run'], 2),
                     (['esp-shazam.local', '--interface', 'lo', '--serial', '/dev/null', '--out', 'x', '--dry-run'], 2)]
            for args, expected in cases:
                result = subprocess.run([str(wrapper), *args], cwd=directory, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, expected, (args, result.stdout, result.stderr))
                self.assertEqual(list(Path(directory).iterdir()), [existing], args)

    def test_root_refusal_and_import_inertness(self):
        stderr = io.StringIO()
        with self.assertRaises(SystemExit) as raised, contextlib.redirect_stderr(stderr):
            zoo.main(['--dry-run', BOARD, '--interface', 'lo', '--serial', '/dev/null', '--out', 'never-made'], euid=0)
        self.assertEqual(raised.exception.code, 2)
        self.assertFalse(Path('never-made').exists())
        code = """
import sys
sys.path.insert(0, sys.argv[1])
def audit(event, args):
    if event.startswith('socket.') or event.startswith('subprocess.') or event in ('os.mkdir', 'os.system'):
        raise RuntimeError('import side effect: ' + event)
sys.addaudithook(audit)
import client_zoo
"""
        result = subprocess.run([sys.executable, '-B', '-c', code, str(SCRIPTS)], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
