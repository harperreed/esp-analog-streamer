# ABOUTME: Tests private bounded UART dump framing and decoder process ownership.
# ABOUTME: Uses real files, processes and a pseudo terminal without hardware.
import contextlib
import json
import io
import os
import pty
import threading
import time
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest

try:
    import core_dump as dump
except ImportError:
    dump = None


class CoreDumpTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(dump, 'bounded core dump module must exist')

    def frame(self, body=b'YWJj\n'):
        return dump.START + b'\r\n' + body + dump.END + b'\r\n'

    def test_extracts_complete_body_without_markers(self):
        result = dump.extract(io.BytesIO(b'boot\n' + self.frame()))
        self.assertEqual(result, b'YWJj\n')

    def test_rejects_truncation_nested_markers_and_bad_base64(self):
        for raw in (dump.START + b'\nYWJj\n', self.frame(dump.START + b'\n'),
                    self.frame(b'password!\n'), self.frame() + self.frame()):
            with self.subTest(raw=raw), self.assertRaises(dump.Failure):
                dump.extract(io.BytesIO(raw))

    def test_rejects_each_input_budget(self):
        for limits in (dict(max_uart=3), dict(max_body=3), dict(max_line=3)):
            with self.subTest(limits=limits), self.assertRaises(dump.Failure):
                dump.extract(io.BytesIO(self.frame()), **limits)

    def test_private_directory_refuses_existing_and_files_are_private(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'run'
            dump.private_directory(path)
            dump.write_private(path / 'receipt', b'ok')
            self.assertEqual(path.stat().st_mode & 0o777, 0o700)
            self.assertEqual((path / 'receipt').stat().st_mode & 0o777, 0o600)
            with self.assertRaises(dump.Failure):
                dump.private_directory(path)

    def test_child_output_timeout_and_nonzero_fail_without_console_output(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)
            for index, (source, opts) in enumerate((('print("secret"*10000)', dict(max_output=100)),
                                 ('import time; time.sleep(5)', dict(timeout=.1)),
                                 ('raise SystemExit(3)', {}))):
                with self.subTest(source=source), self.assertRaises(dump.Failure):
                    case = path / str(index)
                    case.mkdir()
                    dump.run_decoder([sys.executable, '-c', source], case, **opts)

    def test_uart_capture_from_real_terminal(self):
        master, slave = pty.openpty()
        def sender():
            time.sleep(.1)
            for byte in self.frame():
                os.write(master, bytes([byte]))
        worker = threading.Thread(target=sender)
        worker.start()
        try:
            output = io.BytesIO()
            self.assertEqual(dump.capture(os.ttyname(slave), output, timeout=2), b'YWJj\n')
            self.assertEqual(output.getvalue(), self.frame())
        finally:
            worker.join()
            os.close(master)
            os.close(slave)

    def test_reset_defaults_off(self):
        import inspect
        self.assertFalse(inspect.signature(dump.capture).parameters['reset'].default)

    def test_uart_capture_timeout_closes_terminal(self):
        master, slave = pty.openpty()
        try:
            with self.assertRaisesRegex(dump.Failure, 'capture_timeout'):
                dump.capture(os.ttyname(slave), io.BytesIO(), timeout=.1)
        finally:
            os.close(master)
            os.close(slave)

    def test_decoder_timeout_terminates_descendant(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)
            source = ('import subprocess,sys,time; '
                      'p=subprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"]); '
                      'print(p.pid,flush=True); time.sleep(30)')
            with self.assertRaisesRegex(dump.Failure, 'decode_timeout'):
                dump.run_decoder([sys.executable, '-c', source], path, timeout=.3)
            child = int((path / 'decode.stdout').read_text().strip())
            for _ in range(30):
                status = Path(f'/proc/{child}/stat')
                if not status.exists() or status.read_text().split()[2] == 'Z':
                    break
                time.sleep(.01)
            else:
                self.fail('decoder descendant is still running')

    def test_tool_version_receipt_is_bounded_and_private(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)
            dump.run_decoder([sys.executable, '--version'], path, prefix='version')
            self.assertIn(b'Python', (path / 'version.stdout').read_bytes())

    def test_saved_uart_replay_rejects_invalid_dump_with_official_decoder(self):
        repo = Path(__file__).resolve().parents[1]
        sdk = repo / 'firmware/.embuild/espressif/esp-idf/v5.5.5'
        decoder = repo / 'firmware/.embuild/espressif/python_env/idf5.5_py3.12_env/bin/esp-coredump'
        gdb = Path.home() / '.espressif/tools/xtensa-esp-elf-gdb/17.1_20260402/xtensa-esp-elf-gdb/bin/xtensa-esp32-elf-gdb'
        elf = repo / 'target/kata-hardening/network-generations/images/production.elf'
        if not all(path.exists() for path in (sdk, decoder, gdb, elf)):
            self.skipTest('installed pinned SDK decoder/GDB and saved real ELF required')
        parent = repo / 'backups/diagnostics'
        parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as root:
            root = Path(root)
            capture = root / 'input.uart'
            capture.write_bytes(self.frame())
            output = root / 'replay'
            args = ['decode', '--capture', str(capture), '--output', str(output)]
            for name, path in dict(elf=elf, decoder=decoder, gdb=gdb,
                                  rom=repo / 'firmware/.embuild/espressif/tools/esp-rom-elfs/20241011/esp32_rev300_rom.elf',
                                  config=repo / 'firmware/sdkconfig.defaults',
                                  partitions=repo / 'firmware/partitions.csv', sdk=sdk).items():
                args.extend(['--' + name, str(path)])
            console = io.StringIO()
            with contextlib.redirect_stdout(console):
                self.assertEqual(dump.main(args), 1)
            receipt = json.loads((output / 'manifest.json').read_text())
            self.assertEqual(receipt['reason'], 'decode_exit')
            self.assertEqual(receipt['decoder_version'], 'espcoredump.py v1.17.2')
            self.assertNotIn('Traceback', console.getvalue())
            self.assertEqual((output / 'uart.bin').read_bytes(), capture.read_bytes())
            self.assertTrue((output / 'decode.stderr').stat().st_size)
            self.assertTrue(all(path.stat().st_mode & 0o777 == 0o600 for path in output.iterdir()))

    def test_sigterm_capture_writes_failure_receipt(self):
        repo = Path(__file__).resolve().parents[1]
        decoder = repo / 'firmware/.embuild/espressif/python_env/idf5.5_py3.12_env/bin/esp-coredump'
        elf = repo / 'target/kata-hardening/network-generations/images/production.elf'
        if not decoder.exists() or not elf.exists():
            self.skipTest('installed SDK and saved real ELF required')
        parent = repo / 'backups/diagnostics'
        parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as root:
            output = Path(root) / 'capture'
            master, slave = pty.openpty()
            command = [sys.executable, str(repo / 'scripts/core_dump.py'), 'capture',
                       '--port', os.ttyname(slave), '--output', str(output)]
            for name, path in dict(elf=elf, decoder=decoder,
                    gdb=Path.home() / '.espressif/tools/xtensa-esp-elf-gdb/17.1_20260402/xtensa-esp-elf-gdb/bin/xtensa-esp32-elf-gdb',
                    rom=repo / 'firmware/.embuild/espressif/tools/esp-rom-elfs/20241011/esp32_rev300_rom.elf',
                    config=repo / 'firmware/sdkconfig.defaults', partitions=repo / 'firmware/partitions.csv',
                    sdk=repo / 'firmware/.embuild/espressif/esp-idf/v5.5.5').items():
                command.extend(['--' + name, str(path)])
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 10
                while not (output / 'uart.bin').exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(.02)
                self.assertTrue((output / 'uart.bin').exists(), 'capture must reach UART ownership')
                process.terminate()
                stdout, stderr = process.communicate(timeout=3)
                self.assertEqual(process.returncode, 1)
                receipt = json.loads((output / 'manifest.json').read_text())
                self.assertEqual(receipt['reason'], 'terminated')
                self.assertEqual(stderr, b'')
                self.assertNotIn(b'Traceback', stdout)
            finally:
                if process.poll() is None:
                    process.kill()
                process.communicate()
                os.close(master)
                os.close(slave)

    def test_child_success_preserves_report(self):
        with tempfile.TemporaryDirectory() as root:
            dump.run_decoder([sys.executable, '-c', 'print("private-report")'], Path(root))
            self.assertIn(b'private-report', (Path(root) / 'decode.stdout').read_bytes())


if __name__ == '__main__':
    unittest.main()
