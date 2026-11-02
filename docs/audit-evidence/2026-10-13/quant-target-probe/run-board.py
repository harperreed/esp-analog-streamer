# ABOUTME: Runs one standalone quantization benchmark and restores the saved ordinary image.
# ABOUTME: Retains complete flash/UART logs and attempts restoration after every diagnostic attempt.
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

OUT = Path(__file__).resolve().parent
REPO = OUT.parents[2]
PORT = '/dev/serial/by-id/usb-Silicon_Labs_CP2102_USB_to_UART_Bridge_Controller_0001-if00-port0'


def run_logged(argv, output, timeout, env=None, event=None):
    with output.open('wb') as log:
        process = subprocess.Popen(argv, cwd=REPO, env=env, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        if event is not None:
            event['pid'] = process.pid
        try:
            return process.wait(timeout=timeout)
        except BaseException:
            for sig in [signal.SIGTERM, signal.SIGKILL]:
                try:
                    os.killpg(process.pid, sig)
                except ProcessLookupError:
                    pass
                if sig == signal.SIGTERM:
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        pass
            process.wait(timeout=5)
            # A leader can exit before its UART-owning descendant. Zombies have
            # already released descriptors; require every live group member gone.
            deadline = time.monotonic() + 5
            while True:
                live = []
                for status in Path('/proc').glob('[0-9]*/stat'):
                    try:
                        fields = status.read_text().rpartition(')')[2].split()
                    except (FileNotFoundError, ProcessLookupError):
                        continue
                    if int(fields[2]) == process.pid and fields[0] != 'Z':
                        live.append(status.parent.name)
                if not live:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f'owned process group did not exit: {live}')
                time.sleep(0.05)
            raise


def install_interrupt_handler():
    def interrupted(signum, frame):
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        raise KeyboardInterrupt(f"received signal {signum}; restoring ordinary image")
    signal.signal(signal.SIGTERM, interrupted)


def main():
    install_interrupt_handler()
    ordinary = OUT / 'ordinary.elf'
    diagnostic = OUT / 'diagnostic.elf'
    assert diagnostic.is_file() and ordinary.is_file()
    assert hashlib.sha256(ordinary.read_bytes()).hexdigest() == '0ff2a7ccb677fb8045f6faa1d040b930f8adaaf38913946c1847e1f3eb14faf2'
    assert not (OUT / 'board-run.json').exists(), 'retain prior run; do not overwrite'
    executable = subprocess.check_output(
        ['bash', '-lc', 'eval "$(mise env -s bash)"; command -v espflash'],
        cwd=REPO, text=True).strip()
    assert Path(executable).is_file()
    receipt = {'diagnostic_sha256': hashlib.sha256(diagnostic.read_bytes()).hexdigest(),
               'ordinary_sha256': hashlib.sha256(ordinary.read_bytes()).hexdigest(),
               'events': [], 'status': 'incomplete'}
    errors = []

    def command(name, argv, timeout, env=None):
        event = {'name': name, 'argv': [str(x) for x in argv], 'started_monotonic': time.monotonic()}
        receipt['events'].append(event)
        try:
            code = run_logged(argv, OUT / (name + '.log'), timeout, env, event)
            event['exit_code'] = code
            if code:
                raise RuntimeError(f'{name} returned {code}; see its log')
        finally:
            event['ended_monotonic'] = time.monotonic()
            (OUT / 'board-run.json').write_text(json.dumps(receipt, indent=2) + '\n')

    def flash(name, image):
        command(name, [executable, 'flash', '--port', PORT, '--chip', 'esp32',
                      '--flash-size', '4mb', '--flash-freq', '80mhz', '--after', 'no-reset',
                      '--partition-table', OUT / 'partitions.csv', '--bootloader', OUT / 'bootloader.bin',
                      image], 240)

    serial_env = dict(os.environ, ESPFLASH_PORT=PORT)
    command('preflight', ['uv', 'run', '--quiet', '--locked', 'python3', OUT / 'board-state.py', 'before'], 40)
    try:
        flash('flash-diagnostic', diagnostic)
        command('diagnostic-uart', [REPO / 'scripts/serial-log', '90'], 110, serial_env)
    except BaseException as error:
        errors.append(type(error).__name__ + ': ' + str(error))
    finally:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        try:
            flash('restore-ordinary', ordinary)
            command('restore-uart', [REPO / 'scripts/serial-log', '25'], 45, serial_env)
            command('postflight', ['uv', 'run', '--quiet', '--locked', 'python3', OUT / 'board-state.py', 'after'], 40)
        except BaseException as error:
            errors.append('RESTORATION: ' + type(error).__name__ + ': ' + str(error))
    receipt['errors'] = errors
    receipt['status'] = 'failed' if errors else 'acquired_and_restored_pending_data_validation'
    (OUT / 'board-run.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'status': receipt['status'], 'errors': errors}))
    if errors:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
