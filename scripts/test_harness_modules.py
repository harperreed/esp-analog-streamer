# ABOUTME: Exercises importable harness interfaces without physical hardware.
# ABOUTME: Keeps parsing and UART framing tied to the production implementation.
import importlib
import json
import os
import signal
import subprocess
import sys
import time
import tempfile
import unittest
from pathlib import Path


class ModuleTests(unittest.TestCase):
    def test_imports_and_parsers(self):
        for name in ('fault_check', 'encoder_fault_check', 'stream_soak', 'stream_check'):
            module = importlib.import_module(name)
            args = module.parse_args(['--mode', 'capture'] if name == 'fault_check' else [])
            self.assertEqual(args.host, 'esp-shazam.local')

    def test_uart_readers(self):
        for name in ('fault_check', 'encoder_fault_check'):
            module = importlib.import_module(name)
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'uart.log'
                self.assertEqual(module.read_uart_lines(path), [])
                path.write_bytes(b'complete\npartial\r')
                self.assertEqual(module.read_uart_lines(path), ['complete'])
                with path.open('ab') as stream:
                    stream.write(b'\ninvalid\xff\nunfinished')
                self.assertEqual(module.read_uart_lines(path), ['complete', 'partial\r', 'invalid\ufffd'])

    def test_uart_wait_and_match_use_real_complete_records(self):
        import functools
        import fault_check as fault
        import encoder_fault_check as encoder
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'uart.log'
            for module, failure_line, failure in [(fault, 'marker=FAIL_fixture', fault.Failure),
                                                  (encoder, 'encoder-faults: marker=FAIL', RuntimeError)]:
                read = functools.partial(module.read_uart_lines, path)
                kwargs = dict(read_lines=read)
                if module is encoder:
                    kwargs['check_output_budget'] = functools.partial(encoder.check_budget, Path(directory))
                wait = functools.partial(module.wait_for, **kwargs)
                path.write_bytes(b'ignored\nmarker=READY ms=12\nmarker=READY ms=99')
                index, found = module.match_uart(r'marker=READY ms=(\d+)', read, wait, after=1, seconds=0)
                self.assertEqual((index, found.group(1)), (1, '12'))
                with self.assertRaisesRegex(failure, 'missing UART evidence'):
                    module.match_uart('not-present', read, wait, seconds=0)
                path.write_text(failure_line+'\n')
                with self.assertRaisesRegex(failure, 'fixture'):
                    wait(lambda: True, 0, 'never')

    def test_imports_have_no_runtime_side_effects(self):
        scripts = Path(__file__).resolve().parent
        code = """
import os, signal, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
sys.argv = ['unrelated', '--nonsense']
for name in list(os.environ):
    if name.endswith('_REPO') or name == 'STREAM_CHECK_DEFAULT_OUT':
        del os.environ[name]
before = {number: signal.getsignal(number) for number in (signal.SIGTERM, signal.SIGHUP)}
def audit(event, args):
    if event.startswith('socket.') or event.startswith('subprocess.') or event in ('os.mkdir', 'os.system'):
        raise RuntimeError('import side effect: ' + event)
sys.addaudithook(audit)
import fault_check, encoder_fault_check, stream_soak, stream_check
assert before == {number: signal.getsignal(number) for number in before}
assert not list(Path.cwd().iterdir())
"""
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run([sys.executable, '-B', '-c', code, str(scripts)], cwd=directory,
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, '')

    def test_wrapper_help_and_errors_are_inert(self):
        scripts = Path(__file__).resolve().parent
        with tempfile.TemporaryDirectory() as directory:
            for name in ('fault-check', 'encoder-fault-check', 'stream-soak', 'stream-check'):
                for args, expected in [(['--help'], 0), (['--unrecognized-option'], 2)]:
                    result = subprocess.run([str(scripts/name), *args], cwd=directory,
                                            capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, expected, (name, result.stderr))
                    self.assertEqual(list(Path(directory).iterdir()), [])

    def test_output_paths_preserve_caller_directory(self):
        scripts = Path(__file__).resolve().parent
        with tempfile.TemporaryDirectory() as directory:
            previous = Path.cwd()
            try:
                os.chdir(directory)
                for name in ('fault_check', 'encoder_fault_check', 'stream_soak', 'stream_check'):
                    module = importlib.import_module(name)
                    args = module.parse_args(['--mode', 'capture'] if name == 'fault_check' else [])
                    default = (Path(directory)/'target/encoder-fault-check' if name == 'encoder_fault_check'
                               else scripts.parent/'target'/name.replace('_', '-'))
                    self.assertEqual(module.resolve_output(args), default)
                    if name in ('stream_check', 'encoder_fault_check'):
                        args.out = ''
                        self.assertEqual(module.resolve_output(args), Path(directory))
                    args.out = 'relative-output'
                    self.assertEqual(module.resolve_output(args), Path(directory)/'relative-output')
            finally:
                os.chdir(previous)

    def test_tail_preserves_partial_and_rejects_excess_output(self):
        import stream_soak as soak
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'uart.log'
            reader = soak.Tail(path)
            try:
                self.assertEqual(list(reader.lines()), [])
                path.write_bytes(b'one\npartial')
                self.assertEqual(list(reader.lines()), ['one'])
                with path.open('ab') as output:
                    output.write(b'\ninvalid\xff\n')
                self.assertEqual(list(reader.lines()), ['partial', 'invalid\ufffd'])
            finally:
                reader.close()
            for content, message in [(b'x'*17000, 'oversized'), (b'x\n'*70000, 'cannot keep up')]:
                path.write_bytes(content)
                reader = soak.Tail(path)
                try:
                    with self.assertRaisesRegex(soak.Failure, message):
                        list(reader.lines())
                finally:
                    reader.close()

    def test_sparse_output_budgets_retain_evidence(self):
        import encoder_fault_check as encoder
        import stream_soak as soak
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            path = out/'evidence.log'
            with path.open('wb') as output:
                output.truncate(128*1024*1024)
            for check, error in [(encoder.check_budget, RuntimeError), (soak.check_budget, soak.Failure)]:
                check(out)
                with path.open('r+b') as output:
                    output.truncate(128*1024*1024+1)
                with self.assertRaisesRegex(error, 'logs retained'):
                    check(out)
                self.assertEqual(path.stat().st_size, 128*1024*1024+1)
                with path.open('r+b') as output:
                    output.truncate(128*1024*1024)

    def test_stop_helpers_reap_real_process_groups(self):
        import fault_check as fault
        import stream_soak as soak
        import encoder_fault_check as encoder
        for stop, timeout in [(fault.stop_process, 2), (soak.stop, 3), (encoder.stop_process, 2)]:
            for ignore_term in (False, True):
                code = ('import signal,time; ' +
                        ('signal.signal(signal.SIGTERM, signal.SIG_IGN); ' if ignore_term else '') +
                        "print('ready', flush=True); time.sleep(30)")
                process = subprocess.Popen([sys.executable, '-c', code], stdout=subprocess.PIPE,
                                           text=True, start_new_session=True)
                try:
                    self.assertEqual(process.stdout.readline().strip(), 'ready')
                    started = time.monotonic()
                    stop(process)
                    elapsed = time.monotonic()-started
                    self.assertIsNotNone(process.poll())
                    if ignore_term:
                        self.assertGreaterEqual(elapsed, timeout)
                        self.assertEqual(process.returncode, -signal.SIGKILL)
                    self.assertLess(elapsed, timeout+2)
                finally:
                    if process.poll() is None:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=3)
                    process.stdout.close()

    def test_fault_wrapper_retains_shell_option_guards(self):
        script = Path(__file__).resolve().with_name('fault-check')
        cases = [ ['--mode', 'capture', '--mode', 'capture'],
                  ['--mode', 'capture', '--path', '/stream.flac', '--path', '/stream.flac'],
                  ['--mode', 'capture', '--listeners', '2', '--listeners', '2'],
                  ['--mode', 'health', '--path', '/stream.flac'],
                  ['--mode', 'health', '--listeners', '2'],
                  ['--mode', 'pressure', '--path', '/stream.wav'],
                  ['--mode', 'network', '--listeners', '1'] ]
        with tempfile.TemporaryDirectory() as directory:
            for args in cases:
                result = subprocess.run([str(script), *args], cwd=directory,
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 2, (args, result.stderr))
                self.assertEqual(list(Path(directory).iterdir()), [])

    def test_soak_loopback_failure_summary_and_signal_cleanup(self):
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        import threading
        scripts = Path(__file__).resolve().parent
        entered = threading.Event()
        release = threading.Event()
        paths = []
        class StateHandler(BaseHTTPRequestHandler):
            def do_GET(self):
                paths.append(self.path)
                entered.set()
                release.wait(timeout=5)
                body = b'{"shazam_enabled": true}'
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                try:
                    self.wfile.write(body)
                except BrokenPipeError:
                    pass
            def log_message(self, *_args):
                pass
        server = ThreadingHTTPServer(('127.0.0.1', 0), StateHandler)
        worker = threading.Thread(target=server.serve_forever)
        worker.start()
        try:
            with tempfile.TemporaryDirectory() as directory:
                for interrupt in (False, True):
                    entered.clear()
                    release.clear()
                    if not interrupt:
                        release.set()
                    output = 'interrupted' if interrupt else 'refused'
                    process = subprocess.Popen([str(scripts/'stream-soak'), '127.0.0.1', '--seconds', '1',
                                                '--state-port', str(server.server_port), '--out', output],
                                               cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                    try:
                        self.assertTrue(entered.wait(timeout=5), 'CLI did not reach local state fixture')
                        if interrupt:
                            process.send_signal(signal.SIGTERM)
                        stdout, stderr = process.communicate(timeout=5)
                        self.assertEqual(process.returncode, 1, stderr)
                        summaries = list((Path(directory)/output).glob('*/summary.json'))
                        self.assertEqual(len(summaries), 1, stdout)
                        summary = json.loads(summaries[0].read_text())
                        self.assertFalse(summary['passed'])
                        self.assertEqual(summary['error'], 'interrupted' if interrupt else
                                         '/api/state must report shazam_enabled=false')
                        self.assertIn('FAIL '+str(summaries[0]), stdout)
                        self.assertEqual(summary['output_budget_bytes'], 128*1024*1024)
                        self.assertFalse(list((Path(directory)/output).glob('*/uart.log')),
                                         'state rejection must precede UART collector startup')
                    finally:
                        release.set()
                        if process.poll() is None:
                            process.kill()
                            process.wait(timeout=3)
                        process.stdout.close()
                        process.stderr.close()
        finally:
            release.set()
            server.shutdown()
            server.server_close()
            worker.join(timeout=2)
        self.assertEqual(paths, ['/api/state', '/api/state'])


if __name__ == '__main__':
    unittest.main()
