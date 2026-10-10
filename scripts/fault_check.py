# ABOUTME: Implements the public fault-check hardware harness.
# ABOUTME: Exposes host-testable helpers while keeping runtime work behind main.
import argparse
import json
import os
import re
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

from decoder_integrity import FrameHash, validate_decoder_log
from stream_format import input_format_args


class Failure(Exception):
    pass


def validate_internal_geometry(fields):
    required = ['free', 'largest', 'experiment_floor', 'sampled_min', 'probe_before_free', 'probe_before_largest']
    if fields.get('stack_probe_failed') is not True:
        raise Failure('strict internal stack probe failure missing')
    if any(key not in fields for key in required):
        raise Failure('internal fragmentation fields missing')
    if fields['experiment_floor'] != 32768:
        raise Failure('internal fixture floor differs from pinned configuration')
    if fields['free'] < fields['experiment_floor'] or not 0 <= fields['largest'] < 4096:
        raise Failure('internal fixture did not establish stack-sized fragmentation')
    if fields['probe_before_free'] < fields['experiment_floor'] or not 0 <= fields['probe_before_largest'] < 4096:
        raise Failure('internal pre-probe geometry invalid')
    if fields['sampled_min'] > min(fields['free'], fields['probe_before_free']):
        raise Failure('internal sampled minimum omits a logged free sample')
    if fields['sampled_min'] < fields['experiment_floor']:
        raise Failure('internal fixture crossed its sampled experimental floor')


def port(value):
    if not 1 <= len(value) <= 5 or not value.isascii() or not value.isdecimal() or not 1 <= int(value) <= 65535:
        raise argparse.ArgumentTypeError('port must be an integer from 1 to 65535')
    return int(value)


def read_uart_lines(uart):
    # UART writes can end mid-record; only LF completes a telemetry line.
    return [line.decode('utf-8', errors='replace') for line in uart.read_bytes().split(b'\n')[:-1]] if uart.exists() else []


def wait_for(check, seconds, reason, read_lines):
    deadline = time.monotonic() + seconds
    while True:
        current = read_lines()
        if any('marker=FAIL_' in line or 'PRESSURE_FAILED' in line or 'INTERNAL_FAILED' in line or 'wav-freshness: marker=FAIL ' in line for line in current):
            raise Failure('fixture reported failure; see private uart.log')
        result = check()
        if result is not None:
            return result
        if time.monotonic() >= deadline:
            raise Failure(reason)
        time.sleep(0.05)


def duration(job):
    if sum(path.stat().st_size for path in [job['progress'], job['errors'], job['accounting'].path]
           if path.exists()) > 128 * 1024 * 1024:
        raise Failure(f"{job['label']}: decoder output exceeded 128 MiB; logs retained")
    job['accounting'].update(final=job['process'].poll() is not None)
    validate_decoder_log(job['errors'])
    job.update(job['accounting'].summary())
    return job['decoded_seconds']


def complete_decoder(job, wait, record):
    wait(lambda: True if job['process'].poll() is not None else None,
         job['seconds'] + 8, f"{job['label']}: decoder exceeded bounded completion time")
    got = duration(job)
    if job['process'].returncode != 0:
        raise Failure(f"{job['label']}: decode failed; see {job['errors']}")
    if job['decoded_audio_frames'] != round(job['seconds'] * 48000):
        raise Failure(f"{job['label']}: decoded {got:.3f}s, required {job['seconds']}s")
    timeline = 0
    with job['progress'].open() as progress:
        for line in progress:
            if line.startswith('out_time_us=') and line.split('=', 1)[1].strip().isdigit():
                timeline = int(line.split('=', 1)[1]) / 1000000
    record('decoded', label=job['label'], timeline_seconds=timeline, **job['accounting'].summary())


def stop_process(process):
    if process.poll() is None:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=2)


def decoder_command(args, ip, progress, hashes, seconds=None):
    command = ['ffmpeg', '-nostdin', '-nostats', '-v', 'level+warning', '-xerror', '-stats_period', '0.2',
               '-progress', str(progress), '-rw_timeout', '10000000', '-err_detect', 'crccheck',
               *input_format_args(args.path), '-i', f'http://{ip}:{args.stream_port}{args.path}']
    if seconds is not None:
        command += ['-t', str(round(seconds * 48000) / 48000)]
    command += ['-c:a', 'pcm_s32le', '-f', 'framehash', '-flush_packets', '1', str(hashes)]
    return command


def state_url(ip, args):
    return f'http://{ip}:{args.state_port}/api/state'


def stream_address(ip, args):
    return ip, args.stream_port


def resolve_output(args):
    return Path(args.out or Path(__file__).resolve().parent.parent / 'target/fault-check').resolve()


def match_uart(pattern, read_lines, wait, after=0, seconds=20):
    def find():
        for index, line in enumerate(read_lines()[after:], after):
            found = re.search(pattern, line)
            if found:
                return index, found
        return None
    return wait(find, seconds, f'missing UART evidence: {pattern}')


def parse_args(argv=None):
    parser = argparse.ArgumentParser(prog="-")
    parser.add_argument('--mode', required=True, choices=['capture', 'pressure', 'internal-pressure', 'network', 'wav-local', 'health'])
    parser.add_argument('host', nargs='?', default='esp-shazam.local')
    parser.add_argument('--stream-port', type=port, default=8000, help='stream TCP port (default: 8000)')
    parser.add_argument('--state-port', type=port, default=80, help='state HTTP port (default: 80)')
    parser.add_argument('--out')
    parser.add_argument('--path', default='/stream.flac', choices=['/stream.flac', '/stream.wav'])
    parser.add_argument('--listeners', type=int, default=2, choices=[1, 2])
    args = parser.parse_args(argv)
    if args.mode == 'health' and any(option in (sys.argv[1:] if argv is None else argv) for option in ['--path', '--listeners']):
        parser.error('health does not accept stream path/listener options')
    if args.mode != 'capture' and args.path != '/stream.flac':
        parser.error('pressure/internal-pressure/network/wav-local require --path /stream.flac')
    if args.mode != 'capture' and args.listeners != 2:
        parser.error('pressure/internal-pressure/network/wav-local require --listeners 2')

    return args


def main(argv=None):
    args = parse_args(argv)
    repo = Path(__file__).resolve().parent.parent
    os.umask(0o077)
    out = resolve_output(args) / datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    out.mkdir(parents=True)
    uart = out / 'uart.log'
    event_file = (out / 'events.jsonl').open('w')
    started = time.monotonic()
    processes, handles, readers, decoders = [], [], [], []
    evidence = {}
    failure = None
    admission_cap_checked = False

    def record(kind, **fields):
        value = dict(kind=kind, elapsed_seconds=round(time.monotonic() - started, 3), **fields)
        event_file.write(json.dumps(value) + '\n')
        event_file.flush()
        print(json.dumps(value), flush=True)

    def lines():
        return read_uart_lines(uart)

    def wait(check, seconds, reason):
        return wait_for(check, seconds, reason, lines)

    def uart_match(pattern, after=0, seconds=20):
        return match_uart(pattern, lines, wait, after, seconds)

    def marker(name, after=0, seconds=20):
        index, found = uart_match(r'capture-faults: marker=' + name + r'\b.*?ms=(\d+)', after, seconds)
        record('marker', marker=name, board_ms=int(found.group(1)))
        return index

    def state_ready(seconds=12):
        def fetch():
            try:
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                with opener.open(state_url(ip, args), timeout=1) as response:
                    value = json.load(response)
            except Exception:
                return None
            if value.get('shazam_enabled') is not False:
                raise Failure('/api/state does not report shazam_enabled=false')
            record('state', shazam_enabled=False, level_dbfs=value.get('level_dbfs'))
            return value
        return wait(fetch, seconds, '/api/state unavailable before the required fixture stage')

    def status(expected):
        with socket.create_connection(stream_address(ip, args), timeout=2) as peer:
            peer.settimeout(2)
            peer.sendall(f'GET {args.path} HTTP/1.1\r\nHost: esp-shazam\r\n\r\n'.encode('ascii'))
            head = bytearray()
            deadline = time.monotonic() + 3
            while b'\r\n\r\n' not in head:
                peer.settimeout(max(0.001, deadline - time.monotonic()))
                chunk = peer.recv(1024)
                if not chunk or len(head) > 4096 or time.monotonic() >= deadline:
                    raise Failure('HTTP response head incomplete or oversized')
                head.extend(chunk)
            actual = int(head.split(b'\r\n', 1)[0].split()[1])
            record('http_status', expected=expected, actual=actual)
            if actual != expected:
                raise Failure(f'expected HTTP {expected}, received {actual}')

    def refused_eof():
        deadline = time.monotonic() + 3
        received = 0
        with socket.create_connection(stream_address(ip, args), timeout=2) as peer:
            peer.settimeout(max(0.001, deadline - time.monotonic()))
            peer.sendall(f'GET {args.path} HTTP/1.1\r\nHost: esp-shazam\r\n\r\n'.encode('ascii'))
            while True:
                if time.monotonic() >= deadline:
                    raise Failure('internal admission refusal exceeded HTTP deadline')
                peer.settimeout(max(0.001, deadline - time.monotonic()))
                chunk = peer.recv(1024)
                if not chunk:
                    break
                received += len(chunk)
                # Current production spawn failure closes before sending any HTTP response.
                if received:
                    raise Failure('internal admission sent response bytes instead of clean EOF')
        record('internal_refusal_eof', response_bytes=received)

    def decoder(label, seconds=None):
        progress = out / f'{label}.progress'
        hashes = out / f'{label}.framehash'
        errors = (out / f'{label}.ffmpeg.log').open('wb')
        handles.append(errors)
        # Allow the production FLAC write budget (~8.02 s) before the host gives up;
        # stream-check uses the same 10 s timeout. Decode completion remains bounded.
        command = decoder_command(args, ip, progress, hashes, seconds)
        process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=errors, start_new_session=True)
        processes.append(process)
        job = dict(label=label, process=process, progress=progress, errors=out / f'{label}.ffmpeg.log',
                   seconds=seconds, accounting=FrameHash(hashes))
        decoders.append(job)
        return job


    def advancing(job, minimum=1, seconds=5):
        def check():
            if job['process'].poll() is not None:
                raise Failure(f"{job['label']}: decoder exited before required progress; see its ffmpeg log")
            return duration(job) if duration(job) >= minimum else None
        return wait(check, seconds, f"{job['label']}: no decoded audio progress")

    def completed(job):
        return complete_decoder(job, wait, record)

    def pair(seconds, label):
        nonlocal admission_cap_checked
        jobs = [decoder(f'{label}-{index}', seconds) for index in (1, 2)]
        for job in jobs:
            advancing(job)
        status(503)
        admission_cap_checked = True
        for job in jobs:
            completed(job)

    def telemetry(after=0, before=None):
        result = []
        for index, line in enumerate(lines()):
            if index < after or (before is not None and index >= before) or 'audio: frames=' not in line:
                continue
            fields = {key: int(value) for key, value in re.findall(r'\b([a-z_]+)=(-?\d+)', line)}
            result.append(fields)
        return result

    def require_capture_fields(sample, label):
        for key in ['frames', 'queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts', 'discontinuities']:
            if key not in sample:
                raise Failure(f'{label}: missing capture telemetry field {key}')

    def unchanged(first, last, label):
        require_capture_fields(first, label)
        require_capture_fields(last, label)
        for key in ['queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts', 'discontinuities']:
            if first[key] != last[key]:
                raise Failure(f'{label}: new {key}; capture continuity is not established')
        if last['frames'] <= first['frames']:
            raise Failure(f'{label}: captured frames did not advance')

    def drain_observer():
        peer = socket.create_connection(stream_address(ip, args), timeout=2)
        result = dict(peer=peer, stopping=threading.Event(), eof=None, error=None)
        readers.append(result)
        peer.settimeout(2)
        peer.sendall(f'GET {args.path} HTTP/1.1\r\nHost: esp-shazam\r\n\r\n'.encode('ascii'))
        head = bytearray()
        while b'\r\n\r\n' not in head:
            chunk = peer.recv(1024)
            if not chunk or len(head) > 4096:
                raise Failure('EOF observer received an incomplete HTTP response')
            head.extend(chunk)
        if head.split(b'\r\n', 1)[0].split()[1] != b'200':
            raise Failure('EOF observer was not admitted with HTTP 200')
        peer.settimeout(0.25)
        def drain():
            try:
                while not result['stopping'].is_set():
                    try:
                        if not peer.recv(16384):
                            result['eof'] = time.monotonic()
                            return
                    except socket.timeout:
                        continue
            except OSError as error:
                if not result['stopping'].is_set():
                    result['error'] = str(error)
        worker = threading.Thread(target=drain)
        result['worker'] = worker
        worker.start()
        return result


    def stop_reader(reader):
        reader['stopping'].set()
        try:
            reader['peer'].shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        reader['peer'].close()
        if reader.get('worker') is not None and reader['worker'].ident is not None:
            reader['worker'].join(timeout=1)

    def supervisor_reset(final, final_observed):
        restart, _ = uart_match(r'audio: no captured-frame progress .*requesting restart', after=final, seconds=5)
        # Recovery cannot wait for a logger lock. Require the actual software-reset
        # boot line within the tested six-second policy plus two seconds of slack.
        reboot, _ = uart_match(r'rst:0xc \(SW_CPU_RESET\)', after=restart + 1,
                               seconds=max(0.001, 8 - (time.monotonic() - final_observed)))
        reset_elapsed = time.monotonic() - final_observed
        if reset_elapsed > 8:
            raise Failure('software reset exceeded the observed eight-second recovery bound')
        if any(text in line for line in lines()[final:reboot]
               for text in ['Guru Meditation', 'panicked at', 'Task watchdog got triggered', 'socket close failed']):
            raise Failure('panic/watchdog/socket-close failure confounded capture-supervisor software reset')
        record('supervisor_reset', seconds_after_final_marker=round(reset_elapsed, 3), maximum_seconds=8)
        boot = marker('BOOT', after=reboot + 1, seconds=22)
        if not re.search(r'reset_reason=3\b', lines()[boot]):
            raise Failure('post-fault BOOT reset reason is not ESP_RST_SW (3)')
        marker('POST_REBOOT', after=boot + 1, seconds=3)
        evidence['ordered_reboot_lines'] = [final, restart, reboot, boot]
        return reboot

    try:
        ip = socket.getaddrinfo(args.host, args.stream_port, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
        raw = uart.open('wb')
        serial_errors = (out / 'serial.log').open('wb')
        handles += [raw, serial_errors]
        serial = subprocess.Popen([str(repo / 'scripts/serial-log'), '100'], stdout=raw, stderr=serial_errors,
                                  start_new_session=True)
        processes.append(serial)
        uart_match(r'rst:0x', seconds=20)
        record('reset_seen', mode=args.mode, host=args.host, stream_port=args.stream_port, state_port=args.state_port, ip=ip)
        state_ready()
        if args.mode == 'health':
            health_start = len(lines())
            short = marker('SHORT_SUSPEND', seconds=20)
            marker('SHORT_RESUME', after=short)
            recovery = marker('RECOVERY_SUSPEND', after=short, seconds=15)
            before_short = telemetry(before=short)
            after_short = telemetry(after=short, before=recovery)
            if not before_short or not after_short:
                raise Failure('health short-pause capture observations missing')
            unchanged(before_short[-1], after_short[-1], 'health short pause')
            if any('requesting restart' in line or 'rebooting' in line for line in lines()[short:recovery]):
                raise Failure('short pause caused a health supervisor trip')
            resumed = marker('RECOVERY_RESUME', after=recovery, seconds=6)
            request, _ = uart_match(r'audio: no captured-frame progress .*requesting restart', after=recovery, seconds=1)
            if request >= resumed:
                raise Failure('health recovery restart request did not precede actual resume')
            final = marker('FINAL_SUSPEND', after=resumed, seconds=15)
            final_observed = time.monotonic()
            prior = telemetry(before=recovery)
            restored = telemetry(after=resumed, before=final)
            if not prior or not restored:
                raise Failure('health recovery capture observations missing')
            require_capture_fields(prior[-1], 'before health recovery')
            require_capture_fields(restored[0], 'after health recovery')
            for key in ['frames', 'restarts', 'discontinuities']:
                if restored[0][key] <= prior[-1][key]:
                    raise Failure(f'health recovery: {key} did not increase')
            evidence['recovery'] = dict(request_line=request, before=prior[-1], after=restored[0])
            reboot = supervisor_reset(final, final_observed)
            state_ready(5)
            wait(lambda: True if len(telemetry(after=reboot + 1)) >= 2 else None,
                 22, 'two post-reboot health capture samples missing')
            post_reboot = telemetry(after=reboot + 1)
            unchanged(post_reboot[0], post_reboot[-1], 'post-reboot health')
            if any(text in line for line in lines()[health_start:]
                   for text in ['Guru Meditation', 'panicked at', 'Task watchdog got triggered', 'socket close failed']):
                raise Failure('panic/watchdog/socket-close failure confounded health validation')
            evidence['post_reboot_capture_heap'] = post_reboot
        elif args.mode == 'wav-local':
            def wav_marker(name, after=0, seconds=20):
                index, found = uart_match(r'wav-freshness: marker=' + name + r'\b(.*)', after=after, seconds=seconds)
                fields = {key: int(value) for key, value in re.findall(r'\b([a-z_]+)=(\d+)', found.group(1))}
                return index, fields
            suspended, suspension = wav_marker('SUSPEND', seconds=25)
            baseline = telemetry(before=suspended)
            if not baseline or 'ms' not in suspension:
                raise Failure('missing pre-suspension capture baseline or fixture timestamp')
            before_data = lines()[:suspended]
            for peer in (0, 1):
                if not any(re.search(rf'wav-freshness: marker=DATA id={peer} bytes=\d+ ms=', line) for line in before_data):
                    raise Failure('both loopback WAV peers must establish byte progress before suspension')
            resumed, _ = wav_marker('RESUME', after=suspended + 1, seconds=6)
            for peer in (0, 1):
                matches = [line for line in lines()[suspended + 1:resumed]
                           if re.search(rf'wav-freshness: marker=EOF id={peer}\b', line)]
                if len(matches) != 1:
                    raise Failure(f'loopback WAV peer {peer} needs exactly one actual TCP EOF')
                at = re.search(r'\bms=(\d+)', matches[0])
                if at is None or not 1_800 <= int(at.group(1)) - suspension['ms'] <= 3_400:
                    raise Failure(f'loopback WAV peer {peer} EOF outside 1.8–3.4 s freshness bound')
            recovery, _ = wav_marker('RECOVERY_CONNECT', after=resumed + 1, seconds=4)
            assertions = lines()[resumed + 1:recovery]
            for peer in (0, 1):
                matching = [line for line in assertions if re.search(rf'EOF_ASSERT id={peer} elapsed_ms=\d+', line)]
                if len(matching) != 1 or not 1_800 <= int(re.search(r'elapsed_ms=(\d+)', matching[0]).group(1)) <= 3_400:
                    raise Failure('fixture EOF timing assertions missing or outside budget')
            completed_at, _ = wav_marker('TRANSPORT_ASSERTIONS_PASSED', after=recovery + 1, seconds=8)
            for peer in (0, 1):
                if not any(re.search(rf'wav-freshness: marker=DATA id={peer} bytes=\d+ ms=', line)
                           for line in lines()[recovery:completed_at]):
                    raise Failure('recovered loopback WAV header/data assertions missing')
            freshness = lines()[suspended:recovery]
            if sum('stream:' in line and 'no fresh audio for 2 s' in line for line in freshness) != 2:
                raise Failure('both WAV peers require production no-fresh closures')
            request, _ = uart_match(r'audio: no captured-frame progress .*requesting restart', after=suspended + 1, seconds=1)
            if request >= resumed:
                raise Failure('production restart request did not precede capture resume')
            wait(lambda: True if sum('socket closed' in line and 'releasing slot' in line
                                    for line in lines()[recovery:]) >= 2 else None,
                 6, 'recovered loopback listener server sockets did not close')
            first_after, _ = uart_match(r'audio: frames=', after=completed_at + 1, seconds=12)
            restored = telemetry(after=resumed, before=first_after + 1)[-1]
            require_capture_fields(baseline[-1], 'before local WAV suspension')
            require_capture_fields(restored, 'after local WAV resume')
            for key in ['frames', 'restarts', 'discontinuities']:
                if restored[key] <= baseline[-1][key]:
                    raise Failure(f'local WAV recovery: {key} did not increase')
            state_ready(5)
            pair(10, 'wav-local-host-flac-recovered')
            final, _ = uart_match(r'audio: frames=', after=len(lines()), seconds=12)
            unchanged(restored, telemetry(after=final, before=final + 1)[0], 'post-local-WAV FLAC playback')
            window = lines()[suspended:final + 1]
            if any('frame write timed out' in line or 'rst:0x' in line or 'rebooting' in line for line in window):
                raise Failure('local WAV freshness confounded by frame-write timeout or reboot')
            evidence['wav_local'] = dict(suspend_line=suspended, resume_line=resumed,
                                         fixture_pass_line=completed_at, restart_request_line=request,
                                         baseline=baseline[-1], restored=restored,
                                         note='Fixture checks exact WAV headers and byte progress, not decoded waveforms; host recovery decodes FLAC.')
        elif args.mode == 'network':
            original = [decoder(f'network-baseline-{index}') for index in (1, 2)]
            for job in original:
                advancing(job, 2, 6)
                if job['errors'].stat().st_size:
                    raise Failure('network baseline decoder reported errors before injection')
            if any('network-faults: marker=DISCONNECT_REQUEST' in line for line in lines()):
                raise Failure('two baseline decoders did not establish progress before network injection')
            def disconnect_request():
                for index, line in enumerate(lines()):
                    if 'network-faults: marker=DISCONNECT_REQUEST' in line:
                        return index
                for job in original:
                    if job['process'].poll() is not None or job['errors'].stat().st_size:
                        raise Failure('network baseline decoder failed before disconnect injection')
                return None
            request = wait(disconnect_request, 30, 'network disconnect request missing')
            baseline = telemetry(before=request)
            if not baseline:
                raise Failure('missing capture baseline before network disconnect')
            result, found = uart_match(r'network-faults: marker=DISCONNECT_RESULT\b.*result=(-?\d+)', after=request + 1, seconds=3)
            if int(found.group(1)) != 0:
                raise Failure('esp_wifi_disconnect did not return ESP_OK')
            # Wi-Fi can report the posted event before esp_wifi_disconnect returns.
            disconnected, _ = uart_match(r'wifi: disconnected\b', after=request + 1, seconds=5)
            reconnected, _ = uart_match(r'wifi: reconnected\b', after=disconnected + 1, seconds=20)
            # Disconnect may terminate either decoder. Only pre-injection progress and
            # newly admitted streams after production reconnect are required.
            for job in original:
                stop_process(job['process'])
            wait(lambda: True if sum('socket closed' in line and 'releasing slot' in line
                                    for line in lines()[request:]) >= 2 else None,
                 12, 'original network listener sockets did not close before fresh admissions')
            state_ready(5)
            pair(10, 'network-recovered')
            final, _ = uart_match(r'audio: frames=', after=len(lines()), seconds=12)
            during = telemetry(after=request, before=final + 1)
            if len(during) < 2:
                raise Failure('network disconnect/recovery capture observations missing')
            for sample in during:
                unchanged(baseline[-1], sample, 'network disconnect and recovered playback')
            window = lines()[request:final + 1]
            if any('rst:0x' in line or 'requesting restart' in line or 'rebooting' in line for line in window):
                raise Failure('network disconnect caused a reboot or capture supervisor trip')
            evidence['network'] = dict(request_line=request, result_line=result,
                                       disconnected_line=disconnected, reconnected_line=reconnected,
                                       baseline=baseline[-1], capture_heap=during)
        elif args.mode == 'internal-pressure':
            pressure_start, _ = uart_match(r'INTERNAL_START\b', seconds=25)
            baseline = telemetry(before=pressure_start)
            if not baseline:
                raise Failure('missing capture baseline before internal allocation')
            ready, _ = uart_match(r'INTERNAL_READY\b', after=pressure_start, seconds=25)
            fields = {key: int(value) for key, value in re.findall(r'\b([a-z_]+)=(\d+)', lines()[ready])}
            fields['stack_probe_failed'] = bool(re.search(r'\bstack_probe_failed=true\b', lines()[ready]))
            validate_internal_geometry(fields)
            if fields.get('holes', 0) < 1 or fields.get('allocation_count', 0) < 1:
                raise Failure('internal fixture allocation/isolated-hole evidence missing')
            evidence['internal_pressure'] = dict(ready=fields, baseline=baseline[-1],
                                                  reserve_scope='sampled configured experimental floor, not a universal reserve guarantee')
            record('internal_ready', **fields)
            for attempt in (1, 2):
                request_start = len(lines())
                refused_eof()
                pthread_failed, _ = uart_match(r'pthread: Failed to create task!', after=request_start, seconds=3)
                no_thread, _ = uart_match(r'stream: no thread for a listener: .*?(?:os error 12|ENOMEM)',
                                          after=request_start, seconds=3)
                closed, _ = uart_match(r'stream: .*socket closed in \d+ ms; releasing slot \(1 held\)',
                                       after=request_start, seconds=3)
                evidence[f'internal_refusal_{attempt}'] = dict(pthread_failure_line=pthread_failed, no_thread_line=no_thread, checked_close_line=closed)
            state_ready(3)
            ended, _ = uart_match(r'INTERNAL_END\b', after=ready, seconds=25)
            end_fields = {key: int(value) for key, value in re.findall(r'\b([a-z_]+)=(\d+)', lines()[ended])}
            end_fields['stack_probe_failed'] = bool(re.search(r'\bstack_probe_failed=true\b', lines()[ended]))
            validate_internal_geometry(end_fields)
            evidence['internal_pressure']['end'] = end_fields
            released, _ = uart_match(r'INTERNAL_RELEASED\b', after=ended, seconds=3)
            release_fields = {key: int(value) for key, value in re.findall(r'\b([a-z_]+)=(\d+)', lines()[released])}
            evidence['internal_pressure']['release'] = release_fields
            if release_fields.get('free', 0) < fields['experiment_floor'] or release_fields.get('largest', 0) < 4096:
                raise Failure('released internal owners did not restore stack allocation geometry')
            if release_fields.get('sampled_min', -1) < fields['experiment_floor']:
                raise Failure('internal request window crossed sampled floor or lacks evidence')
            during = telemetry(after=pressure_start, before=released)
            if len(during) < 2:
                raise Failure('missing capture observations across internal allocation/request window')
            for sample in during:
                unchanged(baseline[-1], sample, 'internal fragmentation')
            state_ready(5)
            pair(10, 'internal-recovered')
            final, _ = uart_match(r'audio: frames=', after=len(lines()), seconds=12)
            for sample in telemetry(after=released, before=final + 1):
                unchanged(baseline[-1], sample, 'internal fragmentation and recovered playback')
            window = lines()[pressure_start:final + 1]
            if any('rst:0x' in line or 'requesting restart' in line or 'rebooting' in line or 'INTERNAL_FAILED' in line or 'Guru Meditation' in line or 'panic' in line.lower() or 'watchdog' in line.lower() for line in window):
                raise Failure('internal pressure fixture failed or capture supervisor/reset intervened')
            evidence['internal_pressure']['capture_heap'] = telemetry(after=pressure_start, before=final + 1)
        elif args.mode == 'pressure':
            pressure_start, _ = uart_match(r'PRESSURE_START', seconds=25)
            baseline = telemetry(before=pressure_start)
            if not baseline:
                raise Failure('missing post-boot capture baseline before pressure allocation')
            ready, found = uart_match(r'PRESSURE_READY .*requested=(\d+) psram_free=(\d+) psram_largest=(\d+)', after=pressure_start, seconds=25)
            requested, available, largest = map(int, found.groups())
            if not largest < requested < available:
                raise Failure('PRESSURE_READY does not establish real fragmentation')
            record('pressure_ready', requested=requested, psram_free=available, psram_largest=largest)
            status(503)
            status(503)
            wait(lambda: True if sum('listener buffer unavailable' in line for line in lines()[ready:]) >= 2 else None,
                 5, 'both HTTP refusals need UART allocation-failure evidence')
            state_ready(3)
            released, _ = uart_match(r'PRESSURE_RELEASED', after=ready, seconds=25)
            during = telemetry(after=pressure_start, before=released)
            if len(during) < 2:
                raise Failure('missing capture/heap observations throughout pressure')
            for sample in during:
                unchanged(baseline[-1], sample, 'pressure')
            if any('requesting restart' in line or 'rebooting' in line for line in lines()[pressure_start:released]):
                raise Failure('capture supervisor tripped during pressure')
            for key in ['internal_min_sum', 'psram_min_sum']:
                if any(key not in sample for sample in during):
                    raise Failure('rolling heap minima evidence missing')
            evidence['pressure_capture_heap'] = during
            state_ready(5)
            pair(10, 'pressure-recovered')
            uart_match(r'audio: frames=', after=len(lines()), seconds=12)
            final_pressure = telemetry()[-1]
            unchanged(baseline[-1], final_pressure, 'pressure and recovered playback')
            evidence['post_pressure_capture_heap'] = final_pressure
        else:
            baseline = decoder('capture-baseline')
            observer = drain_observer() if args.listeners == 2 else None
            advancing(baseline, 2, 6)
            if any('marker=SHORT_SUSPEND' in line for line in lines()):
                raise Failure('baseline decode did not establish progress before the fixed SHORT stage')
            short = marker('SHORT_SUSPEND', seconds=15)
            marker('SHORT_RESUME', after=short)
            before_progress = duration(baseline)
            advancing(baseline, before_progress + 1, 3)
            if args.listeners == 1:
                close_after = len(lines())
                stop_process(baseline['process'])
                uart_match(r'stream: .*socket closed .*releasing slot \(1 held\)', after=close_after, seconds=4)
                observer = drain_observer()
                if any('marker=RECOVERY_SUSPEND' in line for line in lines()):
                    raise Failure('single EOF observer was not established before recovery suspension')
            recovery = marker('RECOVERY_SUSPEND', after=short, seconds=15)
            suspended_at = time.monotonic()
            ended_by = time.monotonic() + 4.5
            wait(lambda: observer['eof'], max(0.001, ended_by - time.monotonic()),
                 'existing client did not receive EOF within 2 s freshness + 2.5 s slack')
            if observer['error']:
                raise Failure('existing client failed instead of receiving EOF')
            if observer['eof'] < suspended_at:
                raise Failure('existing client ended before recovery suspension')
            record('freshness_eof', seconds_after_marker=round(observer['eof'] - suspended_at, 3), maximum_seconds=4.5)
            uart_match(r'stream: .*no fresh audio for 2 s', after=recovery, seconds=2)
            resumed = marker('RECOVERY_RESUME', after=recovery, seconds=3)
            recovery_restart, _ = uart_match(r'audio: no captured-frame progress .*requesting restart', after=recovery, seconds=1)
            if recovery_restart >= resumed:
                raise Failure('recovery suspension did not trigger a production restart request before resume')
            stop_reader(observer)
            stop_process(baseline['process'])
            window = lines()[short:recovery]
            if any('requesting restart' in line or 'rebooting' in line for line in window):
                raise Failure('short pause caused a capture supervisor trip')
            before_short = telemetry(before=short)
            after_short = telemetry(after=short, before=recovery)
            if not before_short or not after_short:
                raise Failure('short-pause capture counter evidence missing')
            unchanged(before_short[-1], after_short[-1], 'short pause')
            recovered = decoder('capture-recovered', 6)
            completed(recovered)
            final = marker('FINAL_SUSPEND', after=resumed, seconds=15)
            final_observed = time.monotonic()
            before_recovery = telemetry(before=recovery)
            after_recovery = telemetry(after=resumed, before=final)
            if not before_recovery or not after_recovery:
                raise Failure('recovery capture counter evidence missing before final suspension')
            prior, restored = before_recovery[-1], after_recovery[0]
            require_capture_fields(prior, 'before recovery')
            require_capture_fields(restored, 'after recovery')
            for key in ['restarts', 'discontinuities', 'frames']:
                if restored[key] <= prior[key]:
                    raise Failure(f'recovery: {key} did not increase after resume')
            evidence['recovery'] = dict(request_line=recovery_restart, before=prior, after=restored)
            reboot = supervisor_reset(final, final_observed)
            state_ready(5)
            if args.listeners == 2:
                pair(8, 'capture-post-reboot')
            else:
                completed(decoder('capture-post-reboot', 8))
            uart_match(r'audio: frames=', after=len(lines()), seconds=12)
            post_reboot = telemetry(after=reboot + 1)
            if len(post_reboot) < 2:
                raise Failure('post-reboot capture counter evidence missing')
            unchanged(post_reboot[0], post_reboot[-1], 'post-reboot playback')
            evidence['post_reboot_capture_heap'] = post_reboot
        record('assertions_passed', mode=args.mode)
    except Exception as error:
        failure = str(error)
        record('failure', error=failure)
    except KeyboardInterrupt:
        failure = 'interrupted'
        record('failure', error=failure)
    finally:
        for reader in readers:
            try:
                stop_reader(reader)
            except Exception:
                failure = failure or 'reader cleanup failed'
        for process in reversed(processes):
            try:
                stop_process(process)
            except Exception:
                failure = failure or 'subprocess cleanup failed'
        for handle in handles:
            handle.close()
        for job in decoders:
            job['accounting'].close()
        summary = dict(passed=failure is None, error=failure, mode=args.mode, host=args.host, stream_port=args.stream_port, state_port=args.state_port, path=None if args.mode == 'health' else args.path,
                       listeners=0 if args.mode == 'health' else args.listeners, admission_cap_checked=admission_cap_checked,
                       scope='capture supervisor only; no freshness or decoded playback' if args.mode == 'health' else 'stream and capture assertions',
                       elapsed_seconds=round(time.monotonic() - started, 3), evidence=evidence,
                       decoder_accounting=[dict(label=job['label'], **job['accounting'].summary()) for job in decoders],
                       log_directory=str(out), uart_log=str(uart),
                       note='UART is private; health mode provides no stream freshness or decoded-playback evidence.' if args.mode == 'health' else
                       'UART is private; decoded duration establishes playback, not independent DMA continuity.')
        (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
        event_file.close()
        print(f"{'PASS' if failure is None else 'FAIL'} {out / 'summary.json'}", flush=True)
    return 0 if failure is None else 1


if __name__ == '__main__':
    sys.exit(main())

