# ABOUTME: Collects bounded same-ELF OFF/ON/OFF trials without media listeners or settings writes.
# ABOUTME: Preserves raw evidence and separates sequence validity from performance acceptance.
import argparse
import hashlib
import ipaddress
import json
import os
import sys
from pathlib import Path
import re
import signal
import subprocess
import threading
import time
import wave

from encoder_fault_check import check_budget, read_uart_lines, validate_listener_export, listener_event_ms

FAULTS = ('queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts',
          'supervisor_restarts', 'overflow_restarts', 'discontinuities', 'health_restart_requests')
PHASES = ('OFF1', 'ON', 'OFF2')
METRICS = ('elapsed_us', 'task_runtime_us', 'runtime_valid', 'encoded_frames', 'resets',
           'listeners_start', 'listeners_end', 'block_samples', 'raw_read_overruns',
           'raw_wait_total_us', 'encode_wall_total_us', 'encoded_lock_wait_wall_total_us',
           'encoded_push_held_wall_total_us')
PROBE = ('sequence', 'phase', 'interval', 'retained', 'completed', 'valid',
         'success_attempt_us', 'rejected_attempt_us', 'pcm_push_us', 'delay_us',
         'delay_calls', 'raw_remainder_us', 'residual_us')


def fields(line):
    values = {}
    for key, value in re.findall(r'\b([a-z_]+)=(Some\(-?\d+\)|None|true|false|-?\d+|[A-Z][A-Z0-9]*)(?=\s|$)', line):
        if key in values:
            raise ValueError('duplicate field: ' + key)
        if value in ('true', 'false'):
            values[key] = value == 'true'
        elif value == 'None':
            values[key] = None
        elif value.startswith('Some('):
            values[key] = int(value[5:-1])
        elif re.fullmatch(r'-?\d+', value):
            values[key] = int(value)
        else:
            values[key] = value
    return values


def require_state(sonos, app):
    config = sonos['config']
    if not (sonos['state'] == 'stopped' and sonos['active_format'] == 'mp3'
            and config['output_format'] == 'mp3' and config['buffer_preset'] == 'med'
            and config['autoplay'] is False and app.get('shazam_enabled') is False):
        raise ValueError('Requires stopped MP3/med, autoplay false and shazam_enabled false')


def storage_receipt(line):
    """Read frozen Rust Debug receipt layout without inferring a memory class."""
    result = {}
    for name, count in [('raw_payload', 2), ('raw_stamps', 2), ('chunk', 3)]:
        match = re.search(r'\b' + name + r'=\(([^)]+)\)', line)
        if not match:
            raise ValueError('missing storage ' + name)
        numbers = [int(value.strip(), 0) for value in match[1].split(',')]
        if len(numbers) != count or any(value <= 0 for value in numbers):
            raise ValueError('invalid storage ' + name)
        result[name] = numbers
    numeric = fields(line)
    for key in ('output_address', 'output_len', 'output_capacity', 'output_required'):
        result[key] = numeric[key]
    if (result['chunk'][1] != 1920 or result['chunk'][2] < result['chunk'][1]
            or result['output_address'] <= 0 or result['output_len'] < 0
            or result['output_required'] != 960 or result['output_capacity'] < 960
            or result['output_len'] > result['output_capacity']):
        raise ValueError('invalid chunk/output reservation')
    match = re.search(r'mp3=Some\(\((\d+),\s*(\d+),\s*EncoderReceipt\s*\{\s*config:\s*AllocationReceipt\s*\{\s*address:\s*(\d+),\s*bytes:\s*(\d+)\s*\},\s*bitstream:\s*AllocationReceipt\s*\{\s*address:\s*(\d+),\s*bytes:\s*(\d+)\s*\}\s*\}\)\)', line)
    if not match:
        raise ValueError('missing MP3 live allocation receipt')
    pcm, pcm_bytes, config, config_bytes, bitstream, bitstream_bytes = map(int, match.groups())
    if min(pcm, config, config_bytes, bitstream) <= 0 or pcm_bytes != 4608 or bitstream_bytes != 4096:
        raise ValueError('invalid PCM/Shine receipt')
    result.update(pcm_address=pcm, pcm_bytes=pcm_bytes, config_address=config,
                  config_bytes=config_bytes, bitstream_address=bitstream, bitstream_bytes=bitstream_bytes)
    if 'shine_policy=MALLOC_CAP_SPIRAM|MALLOC_CAP_8BIT' not in line:
        raise ValueError('unexpected allocator policy receipt')
    return result


def phase_summary(windows):
    totals = {key: sum(row['metrics'][key] for row in windows) for key in METRICS
              if key.endswith('_us')}
    totals['frame_delta'] = sum(row['frame_delta'] for row in windows)
    totals['reset_delta'] = sum(row['reset_delta'] for row in windows)
    totals['generated_seconds'] = sum(row['frame_delta'] * row['metrics']['block_samples'] / 48000
                                     for row in windows)
    totals['generation_ratio'] = totals['generated_seconds'] / (totals['elapsed_us'] / 1e6)
    totals['codec_wall_us_per_frame'] = totals['encode_wall_total_us'] / totals['frame_delta']
    for key in ('success_attempt_us', 'rejected_attempt_us', 'pcm_push_us', 'delay_us', 'delay_calls',
                'raw_remainder_us', 'residual_us'):
        totals[key] = sum(row['probe'][key] for row in windows)
    return totals


def analyze(lines):
    """Analyze original encounter order; unknown first deltas stay unknown."""
    sequences, captures, errors = [], [], []
    current = pending = previous = None
    for index, (stamp, raw) in enumerate(lines):
        line = re.sub(r'\x1b\[[0-9;]*m', '', raw)
        row = dict(line=index + 1, host_monotonic=stamp, raw=raw)
        try:
            if 'listener-telemetry:' in line:
                try:
                    validate_listener_export([line])
                except RuntimeError as error:
                    errors.append(f'line {index+1}: {error}')
            if ('event_us=' in line and ('stream-listener:' in line or 'stream-sender:' in line
                    or re.search(r'stream: .* (joined |left after |socket closed )', line))):
                row['event_ms'] = listener_event_ms(line)
                # No common clock receipt ties deferred listener events to probe windows.
                # Reject the acquisition rather than place the event at its UART arrival.
                errors.append(f'delayed listener activity has ambiguous probe window at line {index+1}')
            if 'audio: frames=' in line:
                row['counters'] = fields(line)
                captures.append(row)
            if 'encoder-probe-storage:' in line:
                value = fields(line)
                if pending is not None:
                    current['errors'].append('unpaired encoder report before storage')
                sequence = value['sequence']
                current = dict(sequence=sequence, storage=row, windows=[], errors=[], phases={}, valid=False, storage_valid=False, storage_errors=[])
                try:
                    current['storage_receipt'] = storage_receipt(line)
                    current['storage_valid'] = True
                except (ValueError, KeyError) as error:
                    current['storage_errors'].append(str(error))
                if any(item['sequence'] == sequence for item in sequences):
                    current['errors'].append('duplicate sequence storage')
                if value.get('baseline_pending') is not True:
                    current['errors'].append('missing fresh baseline receipt')
                sequences.append(current)
                pending = previous = None
            elif 'encoder-probe: invalidated' in line:
                if current:
                    current['errors'].append(f'invalidated at line {index+1}')
                pending = previous = None
            elif 'rst:' in line or 'stream: active format ' in line or 'stream: encoder failed' in line:
                expected_announcement = (current is not None and not current['windows']
                                         and pending is None and current['storage_valid']
                                         and not current.get('format_announcement')
                                         and re.search(r'stream: active format Mp3\s*$', line))
                if expected_announcement:
                    current['format_announcement'] = row
                elif current:
                    current['errors'].append(f'epoch interrupted at line {index+1}')
                pending = previous = None
            elif ('stream-listener:' in line or 'stream-sender:' in line
                  or re.search(r'stream: .* (joined /stream\.|left after )', line)) and current:
                current['errors'].append(f'listener activity at line {index+1}')
            elif 'stream-encode:' in line and current:
                if pending is not None:
                    current['errors'].append('unpaired/duplicate encoder report')
                row['metrics'] = fields(line)
                pending = row
            elif 'encoder-probe: sequence=' in line:
                if current is None:
                    errors.append(f'probe report without storage at line {index+1}')
                    continue
                probe = fields(line)
                if pending is None:
                    current['errors'].append(f'unpaired probe report at line {index+1}')
                    continue
                metric = pending['metrics']
                row.update(probe=probe, metrics=metric, metric_line=pending['line'],
                           frame_delta=None, reset_delta=None)
                pending = None
                position = len(current['windows'])
                post_complete = probe.get('phase') == 'COMPLETE' and position == 24
                if post_complete:
                    current.setdefault('post_completion', []).append(row)
                else:
                    current['windows'].append(row)
                problem = []
                if any(key not in metric for key in METRICS) or any(key not in probe for key in PROBE):
                    raise ValueError('missing report field')
                if any(type(value) is int and not 0 <= value <= 2**64-1
                       for source in (metric, probe) for value in source.values()):
                    problem.append('negative or overflowing metric')
                expected_phase = 'COMPLETE' if post_complete else PHASES[min(position // 8, 2)]
                if (position >= 24 and not post_complete) or (probe['sequence'], probe['phase'], probe['interval']) != (
                        current['sequence'], expected_phase, position % 8 + 1):
                    problem.append('nonconsecutive sequence/phase/interval')
                if probe['retained'] is not (position < 24 and position % 8 >= 2) or probe['completed'] is not (position == 23):
                    problem.append('incorrect retained/completed label')
                if probe['valid'] is not True or metric['runtime_valid'] is not True:
                    problem.append('invalid probe/runtime')
                if metric['elapsed_us'] <= 0 or metric['block_samples'] != 1152:
                    problem.append('invalid MP3 elapsed/block samples')
                if metric['listeners_start'] != 0 or metric['listeners_end'] != 0 or metric['raw_read_overruns'] != 0:
                    problem.append('listener/raw-loss contamination')
                if previous:
                    row['frame_delta'] = metric['encoded_frames'] - previous['metrics']['encoded_frames']
                    row['reset_delta'] = metric['resets'] - previous['metrics']['resets']
                    if row['frame_delta'] <= 0 or row['reset_delta'] != 0:
                        problem.append('nonpositive frame delta or reset')
                if probe['retained'] and previous is None:
                    problem.append('missing consecutive predecessor')
                raw_remainder = metric['raw_wait_total_us'] - probe['success_attempt_us'] - probe['rejected_attempt_us']
                accounted = sum(metric[key] for key in ('raw_wait_total_us', 'encode_wall_total_us',
                                  'encoded_lock_wait_wall_total_us', 'encoded_push_held_wall_total_us'))
                accounted += probe['pcm_push_us'] + probe['delay_us']
                residual = metric['elapsed_us'] - accounted
                if raw_remainder < 0 or residual < 0 or probe['raw_remainder_us'] != raw_remainder or probe['residual_us'] != residual:
                    problem.append('invalid checked remainder/residual')
                if probe['phase'] != 'ON' and any(probe[key] != 0 for key in (
                        'success_attempt_us', 'rejected_attempt_us', 'pcm_push_us', 'delay_us', 'delay_calls')):
                    problem.append('ON-only timing present in OFF')
                row['errors'] = problem
                current['errors'].extend(f'line {index+1}: {item}' for item in problem)
                previous = row
        except (ValueError, KeyError, TypeError, IndexError, RuntimeError) as error:
            (current['errors'] if current else errors).append(f'line {index+1}: {error}')
            pending = previous = None
    if pending is not None:
        current['errors'].append('unfinished encoder/probe pair')
    for sequence in sequences:
        windows = sequence['windows']
        if len(windows) != 24:
            sequence['errors'].append(f'expected 24 windows, found {len(windows)}')
        sequence['phase_valid'] = not sequence['errors']
        capture_errors_start = len(sequence['errors'])
        retained = [row for row in windows if row['probe'].get('retained') is True]
        if retained:
            # The preceding completed report bounds the first retained device interval.
            first = next((row for row in windows if row['probe'].get('phase') == 'OFF1' and row['probe'].get('interval') == 2), None)
            start = first['line'] if first else retained[0]['line']
            end = retained[-1]['line']
            before = [row for row in captures if row['line'] < start]
            after = [row for row in captures if row['line'] > end]
            if not before or not after:
                sequence['errors'].append('capture reports do not bracket retained span')
            else:
                left, right = before[-1], after[0]
                covered = [row for row in captures if left['line'] <= row['line'] <= right['line']]
                sequence['capture_coverage'] = dict(start=left, end=right, fault_deltas={})
                frames = [row['counters'].get('frames') for row in covered]
                if (any(type(value) is not int or value < 0 for value in frames)
                        or frames[-1] <= frames[0]
                        or any(b < a for a, b in zip(frames, frames[1:]))):
                    sequence['errors'].append('missing/nonpositive capture progress')
                for key in FAULTS:
                    values = [row['counters'].get(key) for row in covered]
                    if any(type(value) is not int or value < 0 for value in values):
                        sequence['errors'].append('missing/invalid capture counter ' + key)
                    elif any(value != values[0] for value in values):
                        sequence['errors'].append('capture fault/discontinuity ' + key)
                    else:
                        sequence['capture_coverage']['fault_deltas'][key] = 0
        sequence['capture_errors'] = sequence['errors'][capture_errors_start:]
        sequence['capture_valid'] = bool(retained) and not sequence['capture_errors']
        sequence['errors'].extend(sequence['storage_errors'])
        if not sequence['errors']:
            try:
                for phase in PHASES:
                    selected = [row for row in retained if row['probe']['phase'] == phase]
                    if len(selected) != 6:
                        raise ValueError('phase missing retained windows')
                    sequence['phases'][phase] = phase_summary(selected)
                sequence['valid'] = True
            except (ValueError, KeyError, TypeError, ZeroDivisionError) as error:
                sequence['errors'].append(str(error))
    return dict(valid_sequence=not errors and any(row['valid'] for row in sequences),
                sequences=sequences, errors=errors, capture_reports=captures,
                performance_acceptance='not assessed')


def load_uart(out):
    """Use existing LF framing and match each line end to its original read arrival."""
    raw = (out/'uart.log').read_bytes()
    arrivals = [json.loads(line) for line in (out/'uart-arrival.jsonl').read_text().splitlines()]
    cursor = 0
    for arrival in arrivals:
        if arrival['offset'] != cursor or arrival['bytes'] <= 0:
            raise ValueError('nonconsecutive UART arrivals')
        cursor += arrival['bytes']
    if cursor != len(raw):
        raise ValueError('UART arrival coverage differs from raw bytes')
    lines, offset, chunk = [], 0, 0
    for line in read_uart_lines(out/'uart.log'):
        offset = raw.index(b'\n', offset) + 1
        while arrivals[chunk]['offset'] + arrivals[chunk]['bytes'] < offset:
            chunk += 1
        lines.append((arrivals[chunk]['host_monotonic'], line))
    return lines


def require_source(player):
    if player.poll() is not None:
        raise RuntimeError(f'Analog source exited unexpectedly: {player.returncode}')


def join_started(thread, seconds):
    if thread is not None and thread.ident is not None:
        thread.join(seconds)


def stop_source(player, event, log):
    size = lambda: log.stat().st_size if log.exists() else 0
    first_error = None
    try:
        event('source_cleanup_begin', returncode=player.poll(), aplay_log_bytes=size())
    except BaseException as error:
        first_error = error
    try:
        if player.poll() is None:
            player.terminate()
            try:
                player.wait(5)
            except subprocess.TimeoutExpired:
                try:
                    event('source_cleanup_kill')
                except BaseException as error:
                    if first_error is None:
                        first_error = error
                finally:
                    player.kill()
                    player.wait(5)
    except BaseException as error:
        if first_error is None:
            first_error = error
    finally:
        try:
            event('source_cleanup_end', returncode=player.poll(), aplay_log_bytes=size())
        except BaseException as error:
            if first_error is None:
                first_error = error
    if first_error is not None:
        raise first_error


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description='Bounded same-ELF probe. Owns UART/reset and analog source. GET only; no media listener, flash, mixer writes or Sonos Play. Parent must release UART/source first.')
    parser.add_argument('host', type=ipaddress.IPv4Address)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--budget-seconds', type=float, default=360)
    args = parser.parse_args(argv)
    if not 240 <= args.budget_seconds <= 600:
        parser.error('--budget-seconds must be 240..600')
    try:
        args.image = args.elf.read_bytes()
        args.fixture = args.fixture.resolve()
        with wave.open(str(args.fixture)) as wav:
            args.fixture_info = dict(seconds=wav.getnframes()/wav.getframerate(), rate=wav.getframerate(),
                                     channels=wav.getnchannels(), sample_bytes=wav.getsampwidth(), frames=wav.getnframes())
        required = 12 + args.budget_seconds + 16
        if args.fixture_info['seconds'] < required:
            raise ValueError(f'fixture must cover at least {required:g}s without looping')
        if args.fixture_info['rate'] != 48000 or args.fixture_info['channels'] != 2 or args.fixture_info['sample_bytes'] != 2:
            raise ValueError('requires approved 48 kHz stereo 16-bit PCM WAV')
        args.fixture_hash = hashlib.sha256(args.fixture.read_bytes()).hexdigest()
        if args.out.exists():
            raise ValueError('output directory must be fresh')
    except (OSError, ValueError, wave.Error) as error:
        parser.error(str(error))
    return args


def remaining_timeout(deadline, now=None, maximum=8):
    remaining = deadline - (time.monotonic() if now is None else now)
    if remaining <= 0:
        raise TimeoutError('fixed global deadline exhausted')
    return min(maximum, remaining)


# A child bounds DNS/header/body reads by an absolute parent timeout, including
# responses that keep supplying bytes without ever hitting a socket timeout.
STATUS_REQUEST = r"""
import json, sys, urllib.request, urllib.error
row = {}
try:
    with urllib.request.urlopen(sys.argv[1], timeout=float(sys.argv[2])) as response:
        row['http_status'] = response.status
        raw = response.read(131073)
        row['raw'] = raw.decode(errors='replace')
        if len(raw) > 131072:
            raise ValueError('status exceeds 128 KiB')
        row['value'] = json.loads(raw)
except BaseException as error:
    row['error'] = repr(error)
    if isinstance(error, urllib.error.HTTPError):
        row['http_status'] = error.code
        row['raw'] = error.read(131073).decode(errors='replace')
print(json.dumps(row))
"""


def fetch_status(url, timeout):
    try:
        command = subprocess.run([sys.executable, '-c', STATUS_REQUEST, url, str(timeout)],
                                 capture_output=True, text=True, timeout=timeout)
        if command.returncode:
            return dict(error=f'status child exited {command.returncode}', stdout=command.stdout, stderr=command.stderr)
        return json.loads(command.stdout)
    except subprocess.TimeoutExpired as error:
        return dict(error=f'absolute status timeout ({timeout}s)',
                    partial_stdout=(error.stdout or b'').decode(errors='replace'),
                    partial_stderr=(error.stderr or b'').decode(errors='replace'))


def main(argv=None):
    args = parse_args(argv)
    os.umask(0o077)
    args.out.mkdir(parents=True, exist_ok=False)
    (args.out/'unified.elf').write_bytes(args.image)
    result = dict(completed=False, acquisition_complete=False, errors=[], events=[], status_receipts=[],
                  image_provenance=dict(path=str(args.elf.resolve()), sha256=hashlib.sha256(args.image).hexdigest(), device_attestation=False),
                  fixture_sha256=args.fixture_hash, fixture_info=args.fixture_info, budget_seconds=args.budget_seconds,
                  limitations=['RTS reset is not a physical power cycle.', 'Source liveness/routing do not prove samples at ADC.',
                               'Arrival stamps are host read times, not device execution.', 'Zero listener endpoints cannot exclude brief churn.',
                               'No overhead tolerance or release performance acceptance is inferred.'])
    port = reader = player = None
    stopping = threading.Event()
    reader_errors = []
    uart_lock = threading.Lock()
    source_stopping = threading.Event()
    source_monitor = None
    source_errors = []
    deadline = None
    final_deadline = None
    reset_asserted = False
    previous_sigterm = signal.getsignal(signal.SIGTERM)

    def interrupted(number, frame):
        raise SystemExit(number)

    signal.signal(signal.SIGTERM, interrupted)

    def event(kind, **values):
        row = dict(kind=kind, host_monotonic=time.monotonic(), utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), **values)
        result['events'].append(row)
        with (args.out/'events.jsonl').open('a') as output:
            output.write(json.dumps(row)+'\n')
        return row['host_monotonic']

    def failure(error):
        row = dict(error=repr(error), host_monotonic=time.monotonic())
        result['errors'].append(row)
        # Append-only receipts survive any later failure or finalization.
        with (args.out/'errors.jsonl').open('a') as output:
            output.write(json.dumps(row)+'\n')

    def get(path, final=False):
        timeout = (remaining_timeout(final_deadline, maximum=6) if final else
                   8 if deadline is None else remaining_timeout(deadline))
        row = dict(path=path, started_monotonic=time.monotonic())
        row.update(fetch_status('http://' + str(args.host) + path, timeout))
        row['ended_monotonic'] = time.monotonic()
        result['status_receipts'].append(row)
        with (args.out/'status.jsonl').open('a') as output:
            output.write(json.dumps(row)+'\n')
        if 'error' in row:
            raise RuntimeError(f'{path}: {row["error"]}')
        return row['value']

    def snapshot(label, final=False):
        values = []
        for path in ('/api/sonos', '/api/state'):
            try:
                values.append(get(path, final))
            except BaseException as error:
                if not final:
                    raise
                failure(error)
                values.append(None)
        sonos, app = values
        if sonos is None or app is None:
            raise RuntimeError('incomplete final state snapshot; request errors retained')
        event(label, sonos=sonos, app_state=app, source_returncode=None if player is None else player.poll())
        require_state(sonos, app)
        if 'before' in result and sonos['config'] != result['before']['config']:
            raise ValueError('saved config changed')
        return sonos

    def routing(label, final=False):
        reads = {}
        for control, expected in [('Headphone', '[on]'), ('Line Out', '[off]'), ('Master', None), ('PCM', None)]:
            timeout = (remaining_timeout(final_deadline, maximum=1) if final else
                       5 if deadline is None else remaining_timeout(deadline, maximum=5))
            if timeout <= 0:
                raise TimeoutError('routing deadline reached')
            command = subprocess.run(['amixer', '-c', 'PCH', 'sget', control], capture_output=True, text=True, timeout=timeout)
            reads[control] = dict(stdout=command.stdout, stderr=command.stderr, returncode=command.returncode)
            event('routing_readback', label=label, control=control, **reads[control])
            if command.returncode or expected and command.stdout.count(expected) < 2:
                raise ValueError('unexpected frontend routing: ' + control)
        return reads

    def collect():
        offset = 0
        try:
            with (args.out/'uart.log').open('wb') as log, (args.out/'uart-arrival.jsonl').open('w') as arrivals:
                while not stopping.is_set():
                    data = port.read(4096)
                    if data:
                        stamp = time.monotonic()
                        with uart_lock:
                            log.write(data); log.flush()
                            arrivals.write(json.dumps(dict(host_monotonic=stamp, offset=offset, bytes=len(data)))+'\n'); arrivals.flush()
                            offset += len(data)
        except BaseException as error:
            reader_errors.append(repr(error))

    def monitor_source():
        with (args.out/'source-liveness.jsonl').open('w') as output:
            while not source_stopping.is_set():
                row = dict(host_monotonic=time.monotonic(), returncode=player.poll())
                output.write(json.dumps(row)+'\n'); output.flush()
                if row['returncode'] is not None:
                    source_errors.append(row)
                    break
                source_stopping.wait(.2)

    def watch():
        require_source(player)
        if source_errors:
            raise RuntimeError(source_errors)
        if reader_errors:
            raise RuntimeError(reader_errors)
        check_budget(args.out)
        if deadline is not None and time.monotonic() >= deadline:
            raise TimeoutError('fixed global collection budget exhausted; invalid sequences retained')

    def pause(seconds):
        end = time.monotonic()+seconds
        while time.monotonic() < end:
            watch()
            time.sleep(min(.2, end-time.monotonic()))

    reset_stamp = None
    try:
        result['before'] = snapshot('before_reset')
        result['routing_before'] = routing('before_source')
        import serial
        port = serial.Serial('/dev/ttyUSB0', 115200, timeout=.2)
        reader = threading.Thread(target=collect, daemon=True)
        reader.start()
        with (args.out/'aplay.log').open('wb') as log:
            argv = ['aplay', '-q', '-D', 'plughw:CARD=PCH,DEV=0', str(args.fixture)]
            launch = event('source_launch', argv=argv)
            deadline = launch + 12 + args.budget_seconds
            player = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT)
            event('source_started', pid=player.pid, returncode=player.poll())
            source_monitor = threading.Thread(target=monitor_source, daemon=True)
            source_monitor.start()
            pause(12)
            port.dtr = False
            reset_asserted = True
            port.rts = True
            event('reset_asserted')
            pause(.1)
            port.rts = False
            reset_asserted = False
            reset_stamp = event('reset_released')
            pause(25)
            snapshot('after_boot')
            while True:
                watch()
                # Reader owns writes; analyze only bytes covered by complete arrival records.
                try:
                    with uart_lock:
                        rows = load_uart(args.out)
                except (ValueError, json.JSONDecodeError):
                    pause(.2)
                    continue
                analysis = analyze([row for row in rows if row[0] >= reset_stamp])
                if analysis['valid_sequence']:
                    result['analysis'] = analysis
                    result['acquisition_complete'] = True
                    break
                pause(.2)
            result['completed'] = True
    except BaseException as error:
        failure(error)
    finally:
        if port is not None and reset_asserted:
            try:
                port.rts = False
                event('reset_released_during_cleanup')
            except BaseException as error:
                failure(error)
        if player is not None:
            final_deadline = min(deadline + 16, time.monotonic() + 16)
            # Each attempt preserves its failure and does not suppress the other.
            try:
                result['routing_after'] = routing('final_before_source_cleanup', final=True)
            except BaseException as error:
                failure(error)
            try:
                snapshot('final_before_source_cleanup', final=True)
            except BaseException as error:
                failure(error)
            try:
                require_source(player)
            except BaseException as error:
                failure(error)
        source_stopping.set()
        try:
            join_started(source_monitor, 1)
        except BaseException as error:
            failure(error)
        for error in source_errors:
            failure(RuntimeError(error))
        if player is not None:
            try:
                stop_source(player, event, args.out/'aplay.log')
            except BaseException as error:
                failure(error)
        stopping.set()
        try:
            join_started(reader, 3)
        except BaseException as error:
            failure(error)
        if port:
            try:
                port.close()
            except BaseException as error:
                failure(error)
        if reader and reader.is_alive():
            failure(RuntimeError('UART reader did not stop within three seconds'))
        for error in reader_errors:
            failure(RuntimeError(error))
        if (args.out/'uart.log').exists():
            try:
                rows = load_uart(args.out)
                result['analysis'] = analyze([row for row in rows if reset_stamp is not None and row[0] >= reset_stamp])
            except BaseException as error:
                failure(error)
        result['completed'] = result['completed'] and not result['errors'] and result.get('analysis', {}).get('valid_sequence', False)
        (args.out/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        signal.signal(signal.SIGTERM, previous_sigterm)
    print(json.dumps(dict(out=str(args.out), completed=result['completed'], errors=result['errors'])))
    return 0 if result['completed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
