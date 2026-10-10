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


LISTENER_RECEIPT = ('listener-telemetry: dropped=0 loss_occurred=false observed_event_range_valid=true '
                    'observed_event_min_us=1000 observed_event_max_us=2000 record_bytes=248 '
                    'queue_bytes=2016 metrics_stack_free_bytes=512 export_us=3000')


class ModuleTests(unittest.TestCase):
    def test_shared_observation_state_defaults_to_mp3_and_accepts_explicit_flac(self):
        from encoder_probe_check import require_state
        for output_format in ('mp3', 'flac'):
            snapshot = json.loads(json.dumps(dict(state='stopped', active_format=output_format,
                config=dict(output_format=output_format, buffer_preset='med', autoplay=False))))
            app = json.loads('{"shazam_enabled":false}')
            self.assertIsNone(require_state(snapshot, app, expected_format=output_format))
            if output_format == 'mp3':
                self.assertIsNone(require_state(snapshot, app))
            else:
                with self.assertRaises(ValueError):
                    require_state(snapshot, app)
            variants = [dict(snapshot, state='playing'),
                        dict(snapshot, active_format='flac' if output_format == 'mp3' else 'mp3')]
            for key, value in (('output_format', 'flac' if output_format == 'mp3' else 'mp3'),
                               ('buffer_preset', 'low'), ('autoplay', True)):
                variants.append(dict(snapshot, config=dict(snapshot['config'], **{key: value})))
            for index, changed in enumerate(variants):
                with self.subTest(output_format=output_format, gate=index), self.assertRaises(ValueError):
                    require_state(changed, app, expected_format=output_format)
                if output_format == 'mp3':
                    with self.assertRaises(ValueError):
                        require_state(changed, app)
            for invalid_app in ({'shazam_enabled': True}, {}):
                with self.assertRaises(ValueError):
                    require_state(snapshot, invalid_app, expected_format=output_format)
                if output_format == 'mp3':
                    with self.assertRaises(ValueError):
                        require_state(snapshot, invalid_app)
        for unsupported in ('wav', 'FLAC', '', None):
            with self.subTest(unsupported=unsupported), self.assertRaises(ValueError):
                require_state(snapshot, app, expected_format=unsupported)

    def test_listener_reason_receipts_from_complete_real_files(self):
        import encoder_fault_check as encoder
        reasons = ' dropped_full=1 dropped_contended=2 dropped_poisoned=4294967295'
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'uart.log'
            path.write_text(LISTENER_RECEIPT + reasons)
            self.assertEqual(encoder.read_uart_lines(path), [])
            with path.open('a') as stream:
                stream.write('\n')
            line = encoder.read_uart_lines(path)[0]
            self.assertEqual(encoder.listener_export_reasons(line), dict(dropped_full=1, dropped_contended=2, dropped_poisoned=4294967295))
            self.assertEqual(len(encoder.listener_export_receipt(line)), 9)
            encoder.validate_listener_export([LISTENER_RECEIPT])
            for key in ('dropped_full', 'dropped_contended', 'dropped_poisoned'):
                for bad in ('', key+'=0 '+key+'=1', key+'=4294967296', key+'=-1', key+'=1x', key+'='):
                    tokens = [token for token in reasons.split() if not token.startswith(key+'=')]
                    path.write_text(LISTENER_RECEIPT+' '+' '.join(tokens)+' '+bad+'\n')
                    with self.subTest(key=key, bad=bad), self.assertRaises(RuntimeError):
                        encoder.listener_export_reasons(encoder.read_uart_lines(path)[0])
            for total, sticky in ((1, 'false'), (0, 'true')):
                path.write_text(LISTENER_RECEIPT.replace('dropped=0', 'dropped='+str(total)).replace('loss_occurred=false', 'loss_occurred='+sticky)+reasons+'\n')
                with self.assertRaisesRegex(RuntimeError, 'export loss'):
                    encoder.validate_listener_export(encoder.read_uart_lines(path))

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

    def test_listener_loss_rejects_success_before_wait_returns(self):
        import fault_check as fault
        import encoder_fault_check as encoder
        for module, failure in [(fault, fault.Failure), (encoder, RuntimeError)]:
            kwargs = dict(read_lines=lambda: ['listener-telemetry: dropped=0 loss_occurred=true observed_event_range_valid=false'])
            if module is encoder:
                kwargs['check_output_budget'] = lambda: None
            with self.assertRaisesRegex(failure, 'listener telemetry'):
                module.wait_for(lambda: True, 0, 'never', **kwargs)

    def test_listener_export_rejects_malformed_and_duplicate_loss_fields(self):
        import encoder_fault_check as encoder
        for line in ['listener-telemetry: dropped=0 dropped=1',
                     'listener-telemetry: dropped=0 loss_occurred=unknown',
                     'listener-telemetry: dropped=-1',
                     'listener-telemetry: dropped=0 loss_occurred=false loss_occurred=true']:
            with self.subTest(line=line), self.assertRaises(RuntimeError):
                encoder.validate_listener_export([line])

    def test_sender_event_time_uses_producer_stamp_and_requires_departure_before_close(self):
        import encoder_fault_check as encoder
        self.assertEqual(encoder.listener_event_ms('I (9999) stream: peer left after 1.0 s: stopped reading event_us=1250000', 9000), 1250)
        lines = ['I (9999) stream: 127.0.0.1:9 socket closed in 2 ms; releasing slot (1 held) event_us=1500000']
        self.assertEqual(encoder.completed_listener_closes(lines), [])
        lines.insert(0, 'I (9998) stream: 127.0.0.1:9 left after 1.0 s: stopped reading event_us=1250000')
        self.assertEqual(encoder.completed_listener_closes(lines), [])
        lines.append(LISTENER_RECEIPT.replace('observed_event_min_us=1000', 'observed_event_min_us=1250000').replace('observed_event_max_us=2000', 'observed_event_max_us=1500000').replace('export_us=3000', 'export_us=1600000'))
        self.assertEqual(len(encoder.completed_listener_closes(lines)), 1)
        self.assertEqual(encoder.completed_listener_closes(lines, after_event_ms=1600), [])
        with self.assertRaisesRegex(RuntimeError, 'event_us'):
            encoder.listener_event_ms('stream: peer left after 1.0 s event_us=broken')

    def test_listener_peer_from_retained_logger_lines(self):
        import encoder_fault_check as encoder
        # Retained WAV failure: UART arrival is later than the producer event.
        left = ('I (34077) esp_shazam_firmware::stream: stream: 192.168.23.153:60800 '
                'left after 2.1 s: fell a ring behind listener_stack_free_bytes=1672 event_us=32363015')
        close = ('I (37827) esp_shazam_firmware::stream: stream: 192.168.23.153:60800 '
                 'socket closed in 242 ms; releasing slot (1 held) event_us=32605374')
        receipt = ('I (38067) esp_shazam_firmware::stream: listener-telemetry: dropped=0 '
                   'loss_occurred=false observed_event_range_valid=true '
                   'observed_event_min_us=32605374 observed_event_max_us=32605374 '
                   'record_bytes=248 queue_bytes=2024 metrics_stack_free_bytes=1132 '
                   'export_us=36839499 dropped_full=0 dropped_contended=0 dropped_poisoned=0')
        rows = [left, close, receipt]
        index, found = encoder.checked_listener_closes(rows)[0]
        self.assertEqual(index, 1)
        self.assertEqual(found.group(1), '192.168.23.153:60800')
        self.assertEqual(encoder.listener_event_ms(rows[index]), 32605.374)
        self.assertEqual(encoder.checked_listener_closes(rows, after_event_ms=32364), [])
        self.assertEqual(encoder.checked_listener_closes(rows, departure_pattern='stopped reading'), [])
        self.assertEqual(encoder.checked_listener_closes(rows[:-1]), [])
        with self.assertRaisesRegex(RuntimeError, 'listener telemetry export loss'):
            encoder.checked_listener_closes([left, close, receipt.replace('dropped=0', 'dropped=1')])
        with self.assertRaisesRegex(RuntimeError, 'close precedes departure'):
            encoder.checked_listener_closes([left, close.replace('32605374', '32363014'), receipt])
        for peer in ('[2001:db8::1]:60800', 'unknown peer'):
            with self.subTest(peer=peer):
                peer_rows = [row.replace('192.168.23.153:60800', peer) for row in rows]
                self.assertEqual(encoder.checked_listener_closes(peer_rows)[0][1].group(1), peer)
        # Prefix variation must not change peer identity; another port must.
        self.assertEqual(len(encoder.checked_listener_closes([left.replace('esp_shazam_firmware::stream: ', ''), close, receipt])), 1)
        self.assertEqual(encoder.checked_listener_closes([left.replace(':60800 left', ':60801 left'), close, receipt]), [])

    def test_wait_revalidates_real_file_changed_by_success_callback(self):
        import fault_check as fault
        import encoder_fault_check as encoder
        for module, failure in [(fault, fault.Failure), (encoder, RuntimeError)]:
            with self.subTest(module=module.__name__), tempfile.TemporaryDirectory() as directory:
                path=Path(directory)/'uart.log'
                path.write_text('before callback\n')
                def check():
                    with path.open('a') as stream:
                        stream.write(LISTENER_RECEIPT.replace('dropped=0', 'dropped=1').replace('loss_occurred=false', 'loss_occurred=true')+'\nmarker=READY\n')
                    return True if 'marker=READY' in ''.join(module.read_uart_lines(path)) else None
                kwargs=dict(read_lines=lambda: module.read_uart_lines(path))
                if module is encoder:
                    kwargs['check_output_budget']=lambda: None
                with self.assertRaisesRegex(failure, 'listener telemetry'):
                    module.wait_for(check, 0, 'not ready', **kwargs)

    def test_receipts_require_unique_bounded_fields_and_ordered_event_range(self):
        import encoder_fault_check as encoder
        invalid = [LISTENER_RECEIPT.replace('loss_occurred=false ', ''),
                   LISTENER_RECEIPT.replace('observed_event_range_valid=true', 'observed_event_range_valid=trueBAD'),
                   LISTENER_RECEIPT.replace('observed_event_min_us=1000', 'observed_event_min_us=9999 observed_event_min_us=1000'),
                   LISTENER_RECEIPT.replace('observed_event_min_us=1000', 'observed_event_min_us=2001'),
                   LISTENER_RECEIPT.replace('observed_event_max_us=2000', 'observed_event_max_us=18446744073709551616'),
                   LISTENER_RECEIPT.replace('dropped=0', 'dropped=4294967296'),
                   LISTENER_RECEIPT.replace('observed_event_range_valid=true', 'observed_event_range_valid=false'),
                   LISTENER_RECEIPT.replace('export_us=3000', 'export_us=1999')]
        for key in ('dropped', 'loss_occurred', 'observed_event_range_valid', 'observed_event_min_us', 'observed_event_max_us', 'record_bytes', 'queue_bytes', 'metrics_stack_free_bytes', 'export_us'):
            token=next(token for token in LISTENER_RECEIPT.split() if token.startswith(key+'='))
            invalid.extend([LISTENER_RECEIPT.replace(token,''), LISTENER_RECEIPT+' '+token])
        left='stream: p left after 1.0 s: stopped reading event_us=1000'
        close='stream: p socket closed in 2 ms; releasing slot (1 held) event_us=2000'
        for receipt in invalid:
            with self.subTest(receipt=receipt), self.assertRaisesRegex(RuntimeError, 'listener telemetry'):
                encoder.completed_listener_closes([left,close,receipt])
        self.assertEqual(len(encoder.completed_listener_closes([left,close,LISTENER_RECEIPT])),1)

    def test_fault_production_close_waits_require_batch_and_correct_departure(self):
        import fault_check as fault
        close='stream: p socket closed in 2 ms; releasing slot (1 held) event_us=2000'
        left='stream: p left after 1.0 s: no fresh audio for 2 s event_us=1000'
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'uart.log'
            read=lambda: fault.read_uart_lines(path)
            # Run the same three domain operations used by main, with the real
            # wait/reader and a zero observation budget for these static files.
            wait=lambda check, seconds, reason: fault.wait_for(check,0,reason,read)
            for operation, departure_required in [(fault.wait_for_internal_refusal_close,False),
                                                  (fault.wait_for_capture_baseline_close,True),
                                                  (fault.wait_for_capture_freshness_close,True)]:
                for rows in [[left,close],[left,LISTENER_RECEIPT],[close,LISTENER_RECEIPT]]:
                    if not departure_required and rows == [close,LISTENER_RECEIPT]:
                        continue
                    path.write_text('\n'.join(rows)+'\n')
                    with self.subTest(operation=operation.__name__,rows=rows), self.assertRaises(fault.Failure):
                        operation(read,wait,0)
                path.write_text('\n'.join(([left] if departure_required else [])+[close,LISTENER_RECEIPT])+'\n')
                index,found=operation(read,wait,0)
                self.assertIn('socket closed',found.group(0))
                self.assertEqual(index,1 if departure_required else 0)
            for rows in [[left.replace('p left','other left'),close,LISTENER_RECEIPT],
                         [left.replace('no fresh audio for 2 s','stopped reading'),close,LISTENER_RECEIPT]]:
                path.write_text('\n'.join(rows)+'\n')
                with self.assertRaises(fault.Failure):
                    fault.wait_for_capture_freshness_close(read,wait,0)

    def test_close_without_producer_time_cannot_certify_a_batch(self):
        import encoder_fault_check as encoder
        rows=['stream: p left after 1.0 s: stopped reading',
              'stream: p socket closed in 2 ms; releasing slot (1 held)',LISTENER_RECEIPT]
        for departure_required in (False,True):
            self.assertEqual(encoder.checked_listener_closes(rows,require_departure=departure_required),[])

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
