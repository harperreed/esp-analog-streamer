# ABOUTME: Implements the public encoder-fault-check hardware harness.
# ABOUTME: Exposes host-testable helpers while keeping runtime work behind main.
import argparse, json, os, re, signal, socket, subprocess, sys, threading, time, urllib.request
from datetime import datetime, timezone
from pathlib import Path
from collections import deque

def port(value):
    if not 1 <= len(value) <= 5 or not value.isascii() or not value.isdecimal() or not 1 <= int(value) <= 65535:
        raise argparse.ArgumentTypeError('port must be an integer from 1 to 65535')
    return int(value)

from stream_format import validate_flac_header
from decoder_integrity import FrameHash, validate_decoder_log


def read_uart_lines(uart):
    # UART writes can end mid-record; only LF completes a telemetry line.
    return [line.decode('utf-8', errors='replace') for line in uart.read_bytes().split(b'\n')[:-1]] if uart.exists() else []


def listener_export_receipt(line):
    """Parse the complete fixed exporter receipt; ambiguous evidence cannot certify close."""
    line = re.sub(r'\x1b\[[0-9;]*m', '', line)
    bounds = dict(dropped=2**32-1, observed_event_min_us=2**64-1,
                  observed_event_max_us=2**64-1, record_bytes=2**32-1,
                  queue_bytes=2**32-1, metrics_stack_free_bytes=2**32-1,
                  export_us=2**64-1)
    values = {}
    for key in (*bounds, 'loss_occurred', 'observed_event_range_valid'):
        tokens = re.findall(r'\b' + key + r'=([^\s]+)', line)
        if len(tokens) != 1 or len(re.findall(r'\b' + key + '=', line)) != 1:
            raise RuntimeError('listener telemetry missing or duplicate field: ' + key)
        token = tokens[0]
        if key in bounds:
            maximum = bounds[key]
            if (not re.fullmatch(r'[0-9]+', token) or len(token) > len(str(maximum))
                    or int(token) > maximum):
                raise RuntimeError('listener telemetry invalid bounded number: ' + key)
            values[key] = int(token)
        else:
            if token not in ('true', 'false'):
                raise RuntimeError('listener telemetry invalid boolean: ' + key)
            values[key] = token == 'true'
    low, high = values['observed_event_min_us'], values['observed_event_max_us']
    if ((values['observed_event_range_valid'] and not low <= high <= values['export_us'])
            or (not values['observed_event_range_valid'] and (low != 0 or high != 0))
            or values['record_bytes'] == 0 or values['queue_bytes'] == 0):
        raise RuntimeError('listener telemetry invalid event range or storage size')
    return values


def listener_export_reasons(line):
    """Require all reason counters when attributing loss in a complete receipt.

    Atomic counters are sampled separately, so their sum need not equal dropped.
    """
    listener_export_receipt(line)
    line = re.sub(r'\x1b\[[0-9;]*m', '', line)
    values = {}
    for key in ('dropped_full', 'dropped_contended', 'dropped_poisoned'):
        tokens = re.findall(r'\b' + key + r'=([^\s]+)', line)
        if len(tokens) != 1 or len(re.findall(r'\b' + key + '=', line)) != 1:
            raise RuntimeError('listener telemetry missing or duplicate field: ' + key)
        token = tokens[0]
        if (not re.fullmatch(r'[0-9]+', token) or len(token) > 10
                or int(token) > 2**32-1):
            raise RuntimeError('listener telemetry invalid bounded number: ' + key)
        values[key] = int(token)
    return values


def validate_listener_export(lines):
    """Loss makes listener-based acceptance unknown, even when the last count wraps."""
    for line in lines:
        if 'listener-telemetry:' in line:
            values = listener_export_receipt(line)
            if values['dropped'] != 0 or values['loss_occurred']:
                raise RuntimeError('listener telemetry export loss')


def encoder_export_receipt(line):
    """Encoder batches share the bounded numeric receipt contract, with every reason required."""
    renamed = line.replace('encoder-telemetry:', 'listener-telemetry:', 1)
    try:
        values = listener_export_receipt(renamed)
        values.update(listener_export_reasons(renamed))
    except RuntimeError as error:
        raise RuntimeError(str(error).replace('listener telemetry', 'encoder telemetry')) from error
    return values


def validate_encoder_export(lines):
    """Sticky loss and any nonzero reason reject encoder evidence, including total wrap."""
    for line in lines:
        if 'encoder-telemetry:' in line:
            values = encoder_export_receipt(line)
            if values['loss_occurred'] or any(values[key] != 0 for key in (
                    'dropped', 'dropped_full', 'dropped_contended', 'dropped_poisoned')):
                raise RuntimeError('encoder telemetry export loss')


def listener_event_us(line):
    line = re.sub(r'\x1b\[[0-9;]*m', '', line)
    found = re.findall(r'\bevent_us=([0-9]+)(?=\s|$)', line)
    if (len(found) != 1 or len(re.findall(r'\bevent_us=', line)) != 1
            or len(found[0]) > 20 or int(found[0]) > 2**64-1):
        raise RuntimeError('missing or malformed listener event_us')
    return int(found[0])


def listener_event_ms(line, rtos_minus_esp_timer_ms=0):
    """Use production event time; the UART prefix timestamps delayed output."""
    if 'event_us=' in line:
        return listener_event_us(line) / 1000
    stamp = re.search(r'\(\s*(\d+)\)', line)
    if stamp is None:
        raise RuntimeError('missing listener event timestamp')
    return int(stamp.group(1)) - rtos_minus_esp_timer_ms


def checked_listener_closes(lines, require_departure=True, after_event_ms=None,
                            departure_pattern=None):
    """Return indexed checked-close evidence with its required departure and batch receipt."""
    validate_listener_export(lines)
    validate_encoder_export(lines)
    receipts = [(index, listener_export_receipt(line)) for index, line in enumerate(lines)
                if 'listener-telemetry:' in line]
    departures, closed = {}, []
    for index, line in enumerate(lines):
        left = re.search(r'stream: (unknown peer|\S+) left after ', line)
        if left:
            departures[left.group(1)] = line
        close = re.search(r'stream: (unknown peer|\S+) socket closed in \d+ ms; releasing slot \(\d+ held\)', line)
        if close is None:
            continue
        departure = departures.get(close.group(1))
        if require_departure and departure is None:
            continue
        if departure_pattern is not None and (departure is None or not re.search(departure_pattern, departure)):
            continue
        # Every new checked-close record carries producer time. Missing provenance
        # cannot be certified by an unrelated batch or by its UART arrival prefix.
        if 'event_us=' not in line or (require_departure and 'event_us=' not in departure):
            continue
        at = listener_event_us(line)
        left_at = listener_event_us(departure) if departure is not None else at
        if at < left_at:
            raise RuntimeError('listener close precedes departure')
        if after_event_ms is not None and left_at / 1000 < after_event_ms:
            continue
        if not any(receipt_index > index and receipt['observed_event_range_valid'] and
                   receipt['observed_event_min_us'] <= at <= receipt['observed_event_max_us']
                   for receipt_index, receipt in receipts):
            continue
        closed.append((index, close))
        departures.pop(close.group(1), None)
    return closed


def completed_listener_closes(lines, after_event_ms=None):
    return [lines[index] for index, _ in checked_listener_closes(lines, after_event_ms=after_event_ms)]


def check_budget(out, budget=128 * 1024 * 1024):
    total = sum(path.stat().st_size for path in out.iterdir() if path.is_file())
    if total > budget:
        raise RuntimeError(f'private output budget exceeded: {total} > {budget}; logs retained')


def wait_for(check, seconds, reason, read_lines, check_output_budget):
    deadline = time.monotonic() + seconds
    while True:
        check_output_budget()
        current = read_lines()
        validate_listener_export(current)
        validate_encoder_export(current)
        if any('encoder-faults: marker=FAIL' in line for line in current):
            raise RuntimeError('fixture failure; see private UART log')
        value = check()
        if value is not None:
            final = read_lines()
            validate_listener_export(final)
            validate_encoder_export(final)
            if any('encoder-faults: marker=FAIL' in line for line in final):
                raise RuntimeError('fixture failure; see private UART log')
            return value
        if time.monotonic() >= deadline:
            raise RuntimeError(reason)
        time.sleep(0.05)


def match_uart(pattern, read_lines, wait, after=0, seconds=20):
    def find():
        for index, line in enumerate(read_lines()[after:], after):
            found = re.search(pattern, line)
            if found:
                return index, found
    return wait(find, seconds, f'missing UART evidence: {pattern}')


def decoder_duration(progress, accounting):
    accounting[progress].update()
    return accounting[progress].frames/48000


def decoder_command(args, ip, progress, hashes):
    return ['ffmpeg','-nostdin','-nostats','-v','level+warning','-xerror','-stats_period','0.2','-progress',str(progress),'-rw_timeout','10000000','-err_detect','crccheck','-i',f'http://{ip}:{args.stream_port}/stream.flac','-t','10','-c:a','pcm_s32le','-f','framehash','-flush_packets','1',str(hashes)]


def state_url(ip, args):
    return f'http://{ip}:{args.state_port}/api/state'


def stream_address(ip, args):
    return ip, args.stream_port


def resolve_output(args):
    return Path(args.out).resolve()


def stop_process(process):
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=2)


def parse_args(argv=None):
    p = argparse.ArgumentParser(prog="-")
    p.add_argument('host', nargs='?', default='esp-shazam.local')
    p.add_argument('--stream-port', type=port, default=8000, help='stream TCP port (default: 8000)')
    p.add_argument('--state-port', type=port, default=80, help='state HTTP port (default: 80)')
    p.add_argument('--out', default='target/encoder-fault-check')
    return p.parse_args(argv)


SUSPEND_MARKER_WAIT_SECONDS = 30


def eof_within_allowance(value, suspended):
    return value is not None and suspended - 0.1 <= value <= suspended + 16.532


def main(argv=None):
    a = parse_args(argv)
    os.umask(0o077)
    out = resolve_output(a) / datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    out.mkdir(parents=True)
    repo = Path(__file__).resolve().parent.parent
    processes, handles, peers, workers = [], [], [], []
    accounting = {}
    stop = threading.Event()
    eof, errors, counts = [None, None], [None, None], [0, 0]
    progress_times = [deque(maxlen=512), deque(maxlen=512)]
    started = time.monotonic()
    evidence = {}
    failure = None
    uart = out / 'uart.log'
    arm_clock_offset = 0
    OUTPUT_BUDGET = 128 * 1024 * 1024

    def interrupted(signum, frame):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)

    def check_output_budget():
        return check_budget(out, OUTPUT_BUDGET)

    def lines():
        return read_uart_lines(uart)

    def wait(check, seconds, reason):
        return wait_for(check, seconds, reason, lines, check_output_budget)

    def match(pattern, after=0, seconds=20):
        return match_uart(pattern, lines, wait, after, seconds)

    def state():
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with opener.open(state_url(ip, a), timeout=2) as response:
            value = json.load(response)
        if value.get('shazam_enabled') is not False:
            raise RuntimeError('Shazam must remain disabled')
        return value

    def samples(after=0, before=None):
        result = []
        for line in lines()[after:before]:
            if 'audio: frames=' in line:
                value = {key:int(number) for key, number in re.findall(r'\b([a-z_]+)=(-?\d+)', line)}
                for key in ['frames', 'queue_overflows', 'software_dropped_frames', 'read_errors', 'restarts', 'discontinuities']:
                    if key not in value:
                        raise RuntimeError(f'missing capture field {key}')
                stamp=re.search(r'\(\s*(\d+)\)',line)
                if stamp is None:raise RuntimeError('missing board telemetry timestamp')
                value['board_ms']=int(stamp.group(1))-arm_clock_offset
                result.append(value)
        return result

    def drain(index, peer):
        try:
            while not stop.is_set():
                try:
                    chunk = peer.recv(16384)
                    if not chunk:
                        eof[index] = time.monotonic()
                        return
                    counts[index] += len(chunk)
                    stamp = time.monotonic()
                    if progress_times[index] and int(progress_times[index][-1]*10)==int(stamp*10):
                        progress_times[index][-1]=stamp
                    else:
                        progress_times[index].append(stamp)
                except socket.timeout:
                    continue
        except OSError as error:
            if not stop.is_set():
                errors[index] = str(error)

    try:
        ip = socket.getaddrinfo(a.host, a.stream_port, socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
        raw, serial_error = uart.open('wb'), (out/'serial.log').open('wb')
        handles += [raw, serial_error]
        serial = subprocess.Popen([str(repo/'scripts/serial-log'), '100'], stdout=raw, stderr=serial_error, start_new_session=True)
        processes.append(serial)
        match(r'stream: listening on port 8000', seconds=25)
        state()
        for index in range(2):
            peer = socket.create_connection(stream_address(ip, a), timeout=2)
            peers.append(peer)
            peer.settimeout(2)
            peer.sendall(b'GET /stream.flac HTTP/1.1\r\nHost: esp-shazam\r\n\r\n')
            head = bytearray()
            deadline = time.monotonic()+3
            while b'\r\n\r\n' not in head:
                chunk=peer.recv(1)
                if not chunk or len(head)>4096 or time.monotonic()>deadline:
                    raise RuntimeError('invalid FLAC response head')
                head.extend(chunk)
            if not head.startswith(b'HTTP/1.1 200 OK\r\n'):
                raise RuntimeError('baseline peer not admitted')
            metadata = bytearray()
            while len(metadata) < 42:
                chunk = peer.recv(42-len(metadata))
                if not chunk or time.monotonic()>deadline:
                    raise RuntimeError('incomplete FLAC stream metadata')
                metadata.extend(chunk)
            validate_flac_header(metadata)
            peer.settimeout(0.2)
            worker=threading.Thread(target=drain,args=(index,peer))
            workers.append(worker)
            worker.start()
        initial=counts.copy()
        time.sleep(2)
        if any(count-initial[index]<32 for index,count in enumerate(counts)) or any(eof) or any(errors):
            raise RuntimeError('both baseline FLAC peers must advance without ending')
        if any('encoder-faults: marker=SUSPEND' in line for line in lines()):
            raise RuntimeError('baseline established after suspension')
        armed, arm_found=match(r'encoder-faults: marker=ARM ms=(\d+) rtos_ms=(\d+)', seconds=35)
        arm_received=time.monotonic()
        arm_ms=int(arm_found.group(1))
        arm_clock_offset=int(arm_found.group(2))-arm_ms
        baseline=samples(before=armed)
        if not baseline:
            raise RuntimeError('missing pre-fault capture sample')
        # Deferred delivery follows up to 10 s finding the task and a 17 s hold; allow 3 s UART slack.
        suspended, found=match(r'encoder-faults: marker=SUSPEND ms=(\d+)', after=armed+1, seconds=SUSPEND_MARKER_WAIT_SECONDS)
        suspend_ms=int(found.group(1))
        resumed,resume_found=match(r'encoder-faults: marker=RESUME ms=(\d+)',after=suspended+1,seconds=5)
        resume_ms=int(resume_found.group(1))
        estimated_suspend=arm_received+(suspend_ms-arm_ms)/1000
        # ARM delivery adds UART/network uncertainty; this conservative receipt-based estimate
        # is not an exact host/board clock synchronization. Production reasons are independent.
        for index in range(2):
            if not any(estimated_suspend-1.1<=stamp<=estimated_suspend for stamp in list(progress_times[index])):
                raise RuntimeError('peer lacks recent pre-suspension body progress')
        if any(errors) or any(not eof_within_allowance(value, estimated_suspend) for value in eof):
            raise RuntimeError('peer failed, ended early or exceeded backlog/freshness/close allowance')
        evidence['eof_seconds_after_estimated_suspend']=[value-estimated_suspend for value in eof]
        evidence['timing_note']='ARM receipt plus saved esp_timer delta; UART delivery uncertainty remains, 100ms early tolerance. RTOS log prefixes converted via paired ARM clock samples with tick quantization.'
        evidence['rtos_minus_esp_timer_ms']=arm_clock_offset
        wait(lambda: True if len(completed_listener_closes(lines()[armed:])) >= 2 else None,
             4, 'original slots lack departure/checked-close evidence')
        # Export may arrive after RESUME. Select original event times from all retained lines.
        interval = [line for line in lines()[armed:] if
                    ('left after ' in line or 'frame write timed out' in line) and
                    suspend_ms <= listener_event_ms(line, arm_clock_offset) <= resume_ms]
        if sum('no fresh audio for 2 s' in line for line in interval)!=2 or any('frame write timed out' in line for line in interval):
            raise RuntimeError('both original closures must report freshness rather than write timeout')
        during_suspend=[sample for sample in samples(after=armed) if suspend_ms<=sample['board_ms']<resume_ms]
        if not during_suspend:raise RuntimeError('capture telemetry missing during actual suspension')
        state()
        jobs=[]
        for index in range(2):
            progress=out/f'recovered-{index}.progress'
            hashes=out/f'recovered-{index}.framehash'
            error=(out/f'recovered-{index}.log').open('wb');handles.append(error)
            job=subprocess.Popen(decoder_command(a, ip, progress, hashes),stdout=subprocess.DEVNULL,stderr=error,start_new_session=True)
            accounting[progress]=FrameHash(hashes)
            processes.append(job);jobs.append((job,progress,out/f'recovered-{index}.log'))
        def duration(progress):
            return decoder_duration(progress, accounting)
        wait(lambda: True if all(job.poll() is None and duration(progress)>=1 for job,progress,error in jobs) else None,6,'both recovered decoders did not advance')
        with socket.create_connection(stream_address(ip, a),timeout=2) as third:
            third.settimeout(2);third.sendall(b'GET /stream.flac HTTP/1.1\r\nHost: esp-shazam\r\n\r\n')
            head=bytearray()
            deadline=time.monotonic()+3
            while b'\r\n' not in head:
                check_output_budget()
                third.settimeout(max(0.001, deadline-time.monotonic()))
                if time.monotonic()>=deadline:raise RuntimeError('third-client status deadline exceeded')
                chunk=third.recv(1)
                if not chunk or len(head)>512:raise RuntimeError('incomplete third-client status')
                head.extend(chunk)
            if not head.startswith(b'HTTP/1.1 503 '):raise RuntimeError('third client was not refused')
        for job,progress,error in jobs:
            wait(lambda: True if job.poll() is not None else None,18,'recovered decoder exceeded bounded completion')
            accounting[progress].update(final=True)
            validate_decoder_log(error)
            if job.returncode or accounting[progress].frames!=480000:
                raise RuntimeError('recovered FLAC decode failed; see decoder log')
        evidence['decoder_accounting']=[counter.summary() for counter in accounting.values()]
        final,_=match(r'audio: frames=',after=len(lines()),seconds=12)
        capture=samples(after=armed,before=final+1)
        if len(capture)<2:raise RuntimeError('missing capture observations through fault and recovery')
        previous=baseline[-1]
        for sample in capture:
            for key in ['queue_overflows','software_dropped_frames','read_errors','restarts','discontinuities']:
                if sample[key]!=baseline[-1][key]:raise RuntimeError(f'capture changed: {key}')
            delta=(sample['frames']-previous['frames']) & 0xffffffff
            if not 0<delta<0x80000000:raise RuntimeError('successive capture samples did not progress')
            previous=sample
        if any('rst:0x' in line or 'requesting restart' in line or 'rebooting' in line for line in lines()[armed:final+1]):
            raise RuntimeError('encoder suspension reset capture or board')
        state()
        evidence.update(baseline=baseline[-1],capture=capture,during_suspend=during_suspend,admission_cap_checked=True)
    except Exception as error:
        failure=str(error)
    except KeyboardInterrupt:
        failure='interrupted'
    finally:
        stop.set()
        for peer in peers:
            try:peer.shutdown(socket.SHUT_RDWR)
            except OSError:pass
            peer.close()
        for worker in workers:
            if worker.ident is not None:worker.join(timeout=1)
        for process in reversed(processes):
            try:
                stop_process(process)
            except Exception as error:failure=failure or f'cleanup failed: {error}'
        for handle in handles:handle.close()
        for counter in accounting.values():counter.close()
        try:check_output_budget()
        except Exception as error:failure=failure or str(error)
        summary=dict(passed=failure is None,error=failure,host=a.host,stream_port=a.stream_port,state_port=a.state_port,elapsed_seconds=time.monotonic()-started,evidence=evidence,decoder_accounting=[counter.summary() for counter in accounting.values()],uart_log=str(uart),output_budget_bytes=OUTPUT_BUDGET,note='Pre-fault peers validate byte progress; recovered peers count actual decoded FLAC samples. UART is private.')
        (out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
        print(('PASS' if failure is None else 'FAIL')+' '+str(out/'summary.json'))
    raise SystemExit(0 if failure is None else 1)


if __name__ == "__main__":
    main()

