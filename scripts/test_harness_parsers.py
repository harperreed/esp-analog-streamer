# ABOUTME: Checks production UART framing, pressure geometry and endpoint parsers.
# ABOUTME: Uses imported helpers and real temporary files without hardware.
import importlib
import contextlib
import io
import sys
from types import SimpleNamespace
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path('scripts').resolve()))

def reader_checks(module, label):
    with tempfile.TemporaryDirectory() as directory:
        uart = Path(directory) / 'uart.log'
        read = lambda: module.read_uart_lines(uart)
        assert read() == [], f'{label}: absent file'
        uart.write_bytes(b'complete\npartial')
        assert read() == ['complete'], f'{label}: partial line became visible'
        with uart.open('ab') as stream:
            stream.write(b'\r')
        assert read() == ['complete'], f'{label}: CR completed a line'
        with uart.open('ab') as stream:
            stream.write(b'\ninvalid\xff\nunfinished\xff')
        assert read() == ['complete', 'partial\r', 'invalid\ufffd'], f'{label}: LF/UTF-8 framing'


def geometry_checks(module):
    failure = module.Failure
    validate = module.validate_internal_geometry
    valid = dict(free=32768, largest=4095, experiment_floor=32768, sampled_min=32768,
                 stack_probe_failed=True, probe_before_free=32768, probe_before_largest=4095)
    validate(valid)
    invalid = [valid | patch for patch in
               [dict(stack_probe_failed=False), dict(stack_probe_failed=1),
                dict(probe_before_free=32767), dict(probe_before_largest=4096),
                dict(probe_before_largest=-1), dict(free=32767), dict(largest=4096),
                dict(sampled_min=32767), dict(sampled_min=32769), dict(experiment_floor=0)]]
    invalid += [{key: value for key, value in valid.items() if key != missing} for missing in valid]
    for index, fields in enumerate(invalid):
        try:
            validate(fields)
        except failure:
            continue
        raise AssertionError(f'geometry case {index} incorrectly accepted')


def endpoint_checks(module, name):
    original = sys.argv
    prefix = [name] + (['--mode', 'capture'] if name == 'fault-check' else [])
    try:
        for flags, expected in [(['--stream-port', '18000', '--state-port', '18080'], (18000, 18080)), ([], (8000, 80)),
                                (['--stream-port', '1', '--state-port', '65535'], (1, 65535))]:
            sys.argv = prefix + flags
            args = module.parse_args(sys.argv[1:])
            assert (args.stream_port, args.state_port) == expected
        if name == 'stream-soak':
            for flags, expected in [(['--listeners', '1'], 1), (['--listeners', '2'], 2), ([], 2)]:
                sys.argv = prefix + flags
                args = module.parse_args(sys.argv[1:])
                assert args.listeners == expected
                assert list(module.listener_indices(args)) == list(range(1, expected + 1))
            for invalid in ['0', '3', '1.5', 'abc']:
                sys.argv = prefix + ['--listeners', invalid]
                with contextlib.redirect_stderr(io.StringIO()):
                    try:
                        module.parse_args(sys.argv[1:])
                    except SystemExit as error:
                        assert error.code == 2
                    else:
                        raise AssertionError('stream-soak accepted invalid listener count')
        for option in ['--stream-port', '--state-port']:
            for invalid in ['0', '65536', '-1', 'abc', '1.5', '１２', '9' * 5000]:
                sys.argv = prefix + [option, invalid]
                try:
                    with contextlib.redirect_stderr(io.StringIO()):
                        module.parse_args(sys.argv[1:])
                except SystemExit as error:
                    assert error.code == 2
                else:
                    raise AssertionError(f'{name}: accepted invalid {option}')
    finally:
        sys.argv = original
    selected = SimpleNamespace(host='127.0.0.1', stream_port=18000, state_port=18080, path='/stream.flac', seconds=1)
    assert module.state_url('127.0.0.1', selected) == 'http://127.0.0.1:18080/api/state'
    assert module.stream_address('127.0.0.1', selected) == ('127.0.0.1', 18000)
    with tempfile.TemporaryDirectory() as directory:
        argv = module.decoder_command(selected, '127.0.0.1', Path(directory)/'progress', Path(directory)/'hashes')
        assert argv[argv.index('-i')+1] == 'http://127.0.0.1:18000/stream.flac'


def main():
    for name in ['fault-check', 'encoder-fault-check']:
        reader_checks(importlib.import_module(name.replace('-', '_')), name)
    geometry_checks(importlib.import_module('fault_check'))
    for name in ['fault-check', 'stream-soak', 'encoder-fault-check']:
        endpoint_checks(importlib.import_module(name.replace('-', '_')), name)
    print('PASS harness UART framing, internal geometry and actual endpoint parser checks (no hardware)')


if __name__ == '__main__':
    main()
