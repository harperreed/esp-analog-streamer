# ABOUTME: Acquires bounded private UART core dumps and replays them through Espressif tools.
# ABOUTME: Preserves matching ELF/config provenance and owns decoder child cleanup.
import argparse
import base64
import hashlib
import io
import json
import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import sys
import time

START = b'================= CORE DUMP START ================='
END = b'================= CORE DUMP END ================='
UART_LIMIT = 2 * 1024 * 1024
BODY_LIMIT = 1024 * 1024
OUTPUT_LIMIT = 2 * 1024 * 1024


class Failure(Exception):
    """Safe status code; raw diagnostic text belongs only in private artifacts."""


def private_directory(path):
    try:
        path.mkdir(mode=0o700)
    except OSError as error:
        raise Failure('output_directory') from error


def write_private(path, data):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as out:
        out.write(data)


class Framing:
    def __init__(self, max_body=BODY_LIMIT, max_line=4096):
        self.pending = bytearray()
        self.body = bytearray()
        self.state = 'waiting'
        self.max_body = max_body
        self.max_line = max_line

    def feed(self, chunk):
        for byte in chunk:
            self.pending.append(byte)
            if len(self.pending) > self.max_line:
                raise Failure('line_budget')
            if byte != 10:
                continue
            line = bytes(self.pending).rstrip(b'\r\n')
            self.pending.clear()
            if line == START:
                if self.state != 'waiting':
                    raise Failure('multiple_dumps')
                self.state = 'body'
            elif line == END:
                if self.state != 'body' or not self.body:
                    raise Failure('unexpected_end')
                self.state = 'complete'
            elif self.state == 'body':
                try:
                    if not line:
                        raise ValueError()
                    base64.b64decode(line, validate=True)
                except ValueError as error:
                    raise Failure('invalid_base64') from error
                if len(self.body) + len(line) + 1 > self.max_body:
                    raise Failure('body_budget')
                self.body.extend(line + b'\n')

    def finish(self):
        if self.state != 'complete':
            raise Failure('incomplete_dump')
        return bytes(self.body)


def extract(stream, max_uart=UART_LIMIT, max_body=BODY_LIMIT, max_line=4096):
    framing = Framing(max_body, max_line)
    total = 0
    while chunk := stream.read(min(4096, max_uart - total + 1)):
        total += len(chunk)
        if total > max_uart:
            raise Failure('uart_budget')
        framing.feed(chunk)
    return framing.finish()


def run_decoder(command, directory, timeout=60, max_output=OUTPUT_LIMIT, prefix='decode'):
    environment = dict(os.environ, TMPDIR=str(directory.resolve()))
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               stdin=subprocess.DEVNULL, start_new_session=True, env=environment)
    selector = selectors.DefaultSelector()
    files = []
    deadline = time.monotonic() + timeout
    total = 0
    try:
        for stream, name in ((process.stdout, prefix + '.stdout'), (process.stderr, prefix + '.stderr')):
            output = os.fdopen(os.open(directory / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb')
            files.append(output)
            selector.register(stream, selectors.EVENT_READ, output)
        while selector.get_map():
            if time.monotonic() >= deadline:
                raise Failure('decode_timeout')
            for key, _ in selector.select(min(.1, max(0, deadline - time.monotonic()))):
                chunk = os.read(key.fileobj.fileno(), 4096)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                available = max_output - total
                key.data.write(chunk[:available])
                total += len(chunk)
                if total > max_output:
                    raise Failure('decode_output_budget')
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise Failure('decode_timeout')
        try:
            code = process.wait(timeout=remaining)
        except subprocess.TimeoutExpired as error:
            raise Failure('decode_timeout') from error
        if code:
            raise Failure('decode_exit')
    finally:
        # The decoder can leave GDB running after exiting or failing.
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=.5)
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()
        selector.close()
        for output in files:
            output.close()
        process.stdout.close()
        process.stderr.close()
        for temporary in directory.glob('tmp*'):
            if temporary.is_file():
                temporary.unlink()


def identity(path):
    digest = hashlib.sha256()
    size = 0
    with path.open('rb') as source:
        while chunk := source.read(65536):
            digest.update(chunk)
            size += len(chunk)
    return dict(sha256=digest.hexdigest(), bytes=size)


def capture(port, output, timeout=120, max_uart=UART_LIMIT, reset=False):
    import serial
    framing = Framing()
    total = 0
    deadline = time.monotonic() + timeout
    # Opening without an implicit reset keeps trigger ownership with the caller.
    connection = serial.Serial(port=None, baudrate=115200, timeout=.1, exclusive=True)
    connection.dtr = False
    connection.rts = False
    connection.port = port
    try:
        connection.open()
        if reset:
            connection.rts = True
            time.sleep(.1)
            connection.rts = False
        while framing.state != 'complete':
            if time.monotonic() >= deadline:
                raise Failure('capture_timeout')
            chunk = connection.read(min(4096, max_uart - total + 1))
            if not chunk:
                continue
            available = max_uart - total
            output.write(chunk[:available])
            total += len(chunk)
            if total > max_uart:
                raise Failure('uart_budget')
            framing.feed(chunk)
        return framing.finish()
    finally:
        try:
            if reset and connection.is_open:
                connection.rts = False
        finally:
            connection.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description='Private bounded UART core-dump capture and official decode; reset is explicit.')
    parser.add_argument('operation', choices=('capture', 'decode'))
    parser.add_argument('--port', help='UART path for capture')
    parser.add_argument('--reset', action='store_true', help='Explicitly pulse owned RTS for 100ms after opening UART')
    parser.add_argument('--capture', type=Path, help='Saved UART file for replay')
    for name in ('elf', 'output', 'decoder', 'gdb', 'rom', 'config', 'partitions', 'sdk'):
        parser.add_argument('--' + name, required=True, type=Path)
    args = parser.parse_args(argv)
    if args.operation == 'decode' and args.reset:
        parser.error('--reset applies only to capture')
    if args.operation == 'capture' and not args.port:
        parser.error('capture requires --port')
    if args.operation == 'decode' and not args.capture:
        parser.error('decode requires --capture')
    root = Path(__file__).resolve().parents[1]
    allowed = (root / 'backups/diagnostics').resolve()
    destination = args.output.absolute()
    if not destination.parent.resolve().is_relative_to(allowed):
        parser.error('--output must be a new directory beneath backups/diagnostics; create its parent first')
    manifest = dict(status='incomplete', reset_requested=args.reset, limits=dict(uart_bytes=UART_LIMIT, body_bytes=BODY_LIMIT,
                    line_bytes=4096, capture_seconds=120, decode_seconds=60, decode_bytes=OUTPUT_LIMIT))
    created = False
    started = time.monotonic()
    old_umask = os.umask(0o077)
    def terminate(signum, frame):
        raise Failure('terminated')
    previous_term = signal.signal(signal.SIGTERM, terminate)
    try:
        private_directory(destination)
        created = True
        inputs = {}
        for name in ('elf', 'decoder', 'gdb', 'rom', 'config', 'partitions'):
            path = getattr(args, name).resolve(strict=True)
            inputs[name] = dict(path=str(path), **identity(path))
            setattr(args, name, path)
        sdk = args.sdk.resolve(strict=True)
        for name in ('tools/tools.json', 'components/espcoredump/espcoredump.py'):
            inputs[name] = identity(sdk / name)
        manifest['inputs'] = inputs
        sdk_commit = subprocess.run(['git', '-C', str(sdk), 'rev-parse', 'HEAD'], check=True,
                                    capture_output=True, timeout=5).stdout.decode().strip()
        manifest['sdk_commit'] = sdk_commit
        run_decoder([str(args.decoder), '--version'], destination, timeout=5, max_output=16384, prefix='decoder-version')
        run_decoder([str(args.gdb), '--version'], destination, timeout=5, max_output=16384, prefix='gdb-version')
        manifest['decoder_version'] = (destination / 'decoder-version.stdout').read_text().strip()
        manifest['gdb_version'] = (destination / 'gdb-version.stdout').read_text().splitlines()[0]
        if manifest['decoder_version'] != 'espcoredump.py v1.17.2':
            raise Failure('decoder_version')
        if manifest['gdb_version'] != 'GNU gdb (esp-gdb) 17.1_20260402':
            raise Failure('gdb_version')
        packages = list(args.decoder.parent.parent.glob('lib/python*/site-packages/esp_coredump'))
        if len(packages) != 1:
            raise Failure('decoder_package_provenance')
        manifest['decoder_distributions'] = {str(path.relative_to(packages[0].parent)): identity(path)
                                             for path in sorted(packages[0].parent.glob('*.dist-info/METADATA'))}
        manifest['decoder_sources'] = {str(path.relative_to(packages[0])): identity(path)
                                      for path in sorted(packages[0].rglob('*.py'))}
        with args.elf.open('rb') as source, os.fdopen(os.open(destination / 'application.elf',
                 os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as out:
            shutil.copyfileobj(source, out)
        for name in ('config', 'partitions'):
            write_private(destination / (name + '.txt'), getattr(args, name).read_bytes())
        if identity(destination / 'application.elf') != {key: inputs['elf'][key] for key in ('sha256', 'bytes')}:
            raise Failure('elf_changed')
        with os.fdopen(os.open(destination / 'uart.bin', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as out:
            if args.operation == 'capture':
                body = capture(args.port, out, reset=args.reset)
            else:
                with args.capture.open('rb') as source:
                    raw = source.read(UART_LIMIT + 1)
                out.write(raw[:UART_LIMIT])
                body = extract(io.BytesIO(raw))
        write_private(destination / 'dump.b64', body)
        command = [str(args.decoder), '--chip', 'esp32', '--gdb-timeout-sec', '10', 'info_corefile',
                   '--core', str(destination / 'dump.b64'), '--core-format', 'b64',
                   '--gdb', str(args.gdb), '--rom-elf', str(args.rom), str(destination / 'application.elf')]
        manifest['decoder_command'] = command
        run_decoder(command, destination)
        report = (destination / 'decode.stdout').read_bytes()
        if b'ESP32 CORE DUMP END' not in report:
            raise Failure('decode_incomplete_report')
        manifest['status'] = 'pass'
    except (Exception, KeyboardInterrupt) as error:
        manifest['status'] = 'fail'
        manifest['reason'] = str(error) if isinstance(error, Failure) else type(error).__name__
    finally:
        try:
            if created:
                manifest['elapsed_seconds'] = time.monotonic() - started
                manifest['artifacts'] = {path.name: identity(path) for path in destination.iterdir() if path.is_file()}
                write_private(destination / 'manifest.json', json.dumps(manifest, indent=2).encode() + b'\n')
        finally:
            os.umask(old_umask)
            signal.signal(signal.SIGTERM, previous_term)
    print(json.dumps(dict(status=manifest['status'], reason=manifest.get('reason'), output=str(destination))))
    return 0 if manifest['status'] == 'pass' else 1


if __name__ == '__main__':
    sys.exit(main())
