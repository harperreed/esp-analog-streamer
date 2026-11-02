#!/usr/bin/env python3
# ABOUTME: Measures one direct MP3 receiver, then adds a second with independent TCP_INFO sampling.
# ABOUTME: Retains bounded private evidence and separates decoded prefix samples from inferred phase rates.
import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import threading
import time

spec = importlib.util.spec_from_file_location('tcp_probe', Path(__file__).parent.parent / 'mp3-stream-probe' / 'tcp-stall-probe.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)


def main():
    parser = argparse.ArgumentParser(description='Throwaway direct MP3 diagnostic: drain listener 1 immediately, add listener 2 at 30 s, finish at 90 s. Does not control firmware, UART or audio.')
    parser.add_argument('host')
    parser.add_argument('--port', type=int, default=8000)
    parser.add_argument('--seconds', type=float, default=90, help='total after first admission, 1..90 (default: 90)')
    parser.add_argument('--join-after', type=float, default=30, help='second connect start after first admission (default: 30)')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if not 0 < args.join_after < args.seconds <= 90:
        parser.error('require 0 < join-after < seconds <= 90')
    os.umask(0o077)
    args.out.mkdir(parents=True, exist_ok=True)
    result = dict(passed=False, requested_seconds=args.seconds, join_after_seconds=args.join_after,
                  verdict_scope='no EOF/error and valid whole prefixes; phase rates describe delivery, not exact source continuity',
                  limitations=['TCP_INFO describes this receiver; no remote retransmission claim',
                               'phase sample equivalents derive from complete 960-byte frames, not separately decoded phase PCM',
                               'no sender/receiver clock synchronization inferred; monotonic admission and local ports permit UART correlation',
                               'HTTP header admission is observed after server cursor selection; connect start and admission bound that transition'])
    jobs = []
    events = []
    workers = []
    registry = threading.Lock()
    stopping = threading.Event()
    observer = None
    origin = None
    deadline = None
    telemetry_bytes = 0
    def event(name, **fields):
        stamp = time.monotonic()
        with registry:
            events.append(dict(kind=name, monotonic=stamp, seconds=None if origin is None else stamp-origin, **fields))
    def snapshot(job):
        with job['lock']:
            return {key: value for key, value in job.items() if key not in ('lock', 'peer', 'data')}
    def mark_phase(name):
        stamp = time.monotonic()
        with registry:
            current = list(jobs)
        values = [snapshot(job) for job in current]
        result[name] = dict(monotonic=stamp, seconds=stamp-origin,
                            listeners=[dict(label=j['label'], app_bytes=j['bytes'], whole_frames=j['bytes']//960) for j in values])
    def drain(job):
        try:
            while not stopping.is_set() and time.monotonic() < deadline:
                try:
                    chunk = job['peer'].recv(65536)
                except socket.timeout:
                    continue
                stamp = time.monotonic()
                with job['lock']:
                    if not chunk:
                        job.update(eof=True, eof_monotonic=stamp, eof_seconds=stamp-origin)
                        event('eof', label=job['label'])
                        break
                    if len(job['data']) + len(chunk) > base.RAW_LIMIT:
                        raise ValueError('raw capture exceeded 8 MiB')
                    gap = stamp-job['last_recv_monotonic']
                    if gap >= .5:
                        event('receive_gap', label=job['label'], gap_seconds=gap, app_bytes_before=job['bytes'])
                    job['max_recv_gap'] = max(job['max_recv_gap'], gap)
                    job['last_recv_monotonic'] = stamp
                    if job['first_recv_monotonic'] is None:
                        job['first_recv_monotonic'] = stamp
                        event('first_receive', label=job['label'], receive_monotonic=stamp)
                    job['recv_calls'] += 1
                    job['data'] += chunk
                    job['bytes'] += len(chunk)
        except Exception as error:
            with job['lock']:
                job['error'] = f'{type(error).__name__}: {error}'
            event('receive_error', label=job['label'], error=job['error'])
        finally:
            event('drain_stopped', label=job['label'])
    def add_listener(label):
        nonlocal origin, deadline
        start = time.monotonic()
        event('connect_start', label=label)
        peer, admitted = base.helper.connect(args.host, args.port, '/stream.mp3')
        peer.settimeout(.25)
        if origin is None:
            origin = admitted
            deadline = origin + args.seconds
            result['origin_monotonic'] = origin
        job = dict(label=label, peer=peer, lock=threading.Lock(), data=bytearray(), bytes=0,
                   connect_start_monotonic=start, admitted_monotonic=admitted,
                   connect_start_seconds=start-origin, admitted_seconds=admitted-origin,
                   local_port=peer.getsockname()[1], receiver_buffer_bytes=peer.getsockopt(socket.SOL_SOCKET,socket.SO_RCVBUF),
                   first_recv_monotonic=None, last_recv_monotonic=admitted, max_recv_gap=0,
                   recv_calls=0, eof=False, error=None)
        with registry:
            jobs.append(job)
        thread = threading.Thread(target=drain, args=(job,), daemon=True)
        workers.append(thread)
        thread.start()
        event('admitted', label=label, local_port=job['local_port'], admitted_monotonic=admitted)
    def sample(fields):
        nonlocal telemetry_bytes
        next_sample = origin
        try:
            with (args.out/'tcp-info.jsonl').open('w') as output:
                while not stopping.is_set() and time.monotonic() <= deadline:
                    stamp = time.monotonic()
                    with registry:
                        current = list(jobs)
                    values = []
                    for job in current:
                        raw = job['peer'].getsockopt(socket.IPPROTO_TCP, socket.TCP_INFO, 512)
                        state = snapshot(job)
                        counters = {name: int.from_bytes(raw[offset:offset+size], os.sys.byteorder)
                                    if offset+size <= len(raw) else None for name,(offset,size) in fields.items()}
                        values.append(dict(label=state['label'], local_port=state['local_port'], tcp_info_length=len(raw),
                                           counters=counters, app_bytes=state['bytes'], recv_calls=state['recv_calls'],
                                           seconds_since_recv=max(0,time.monotonic()-state['last_recv_monotonic']),
                                           max_recv_gap=state['max_recv_gap'], eof=state['eof'], error=state['error']))
                    line = json.dumps(dict(monotonic=stamp, seconds=stamp-origin, listeners=values), separators=(',',':'))+'\n'
                    telemetry_bytes += len(line.encode())
                    if telemetry_bytes > base.TELEMETRY_LIMIT:
                        raise ValueError('telemetry exceeded 4 MiB')
                    output.write(line)
                    output.flush()
                    next_sample += .1
                    stopping.wait(max(0,next_sample-time.monotonic()))
        except Exception as error:
            result['telemetry_error'] = f'{type(error).__name__}: {error}'
            stopping.set()
    try:
        fields = base.layout()
        result['tcp_info_layout'] = {name: dict(offset=offset,size=size) for name,(offset,size) in fields.items()}
        add_listener('listener-1')
        observer = threading.Thread(target=sample,args=(fields,),daemon=True)
        observer.start()
        stopping.wait(max(0,origin+args.join_after-time.monotonic()))
        if not stopping.is_set():
            mark_phase('before_second_connect')
            add_listener('listener-2')  # Its blocking handshake cannot stop the first drain or observer.
            mark_phase('after_second_admission')
            stopping.wait(max(0,deadline-time.monotonic()))
    except BaseException as error:
        result['error'] = repr(error)
    finally:
        stopping.set()
        for worker in workers + ([observer] if observer else []):
            worker.join(2)
            if worker.is_alive():
                result['worker_error'] = 'worker did not stop within two seconds'
        ended = time.monotonic()
        if origin is not None:
            mark_phase('capture_end')
        result['end_monotonic'] = ended
        result['telemetry_bytes'] = telemetry_bytes
        result['listeners'] = []
        for job in jobs:
            job['peer'].close()
            state = snapshot(job)
            data = job['data']
            whole = len(data)//960*960
            (args.out/f"{state['label']}-raw.mp3").write_bytes(data)
            state['partial_tail_bytes'] = len(data)-whole
            state['final_no_receive_seconds'] = ended-state['last_recv_monotonic']
            try:
                state['prefix_decode'] = base.helper.check(memoryview(data)[:whole],args.out,f"{state['label']}-whole-prefix")
            except Exception as error:
                state['decode_error'] = repr(error)
            result['listeners'].append(state)
        phases=[]
        boundaries=[('single', None, 'before_second_connect'), ('overlap', 'after_second_admission', 'capture_end')]
        for name,start_key,end_key in boundaries:
            if end_key not in result or (start_key and start_key not in result):
                continue
            start=result[start_key] if start_key else dict(monotonic=origin,listeners=[dict(label='listener-1',app_bytes=0,whole_frames=0)])
            end=result[end_key]
            seconds=end['monotonic']-start['monotonic']
            for final in end['listeners']:
                first=next((value for value in start['listeners'] if value['label']==final['label']),None)
                if first and seconds>0:
                    frames=final['whole_frames']-first['whole_frames']
                    phases.append(dict(phase=name,label=final['label'],wall_seconds=seconds,
                                       coded_bytes=final['app_bytes']-first['app_bytes'],completed_frame_delta=frames,
                                       coded_sample_frame_equivalent=frames*1152,
                                       audio_seconds_per_wall_second=frames*.024/seconds))
        result['phase_rates']=phases
        result['events']=events
        result['passed']=len(jobs)==2 and not any(result.get(key) for key in ['error','worker_error','telemetry_error']) and all(not row['eof'] and not row['error'] and not row.get('decode_error') for row in result['listeners'])
        (args.out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
    return 0 if result['passed'] else 1


if __name__=='__main__':
    raise SystemExit(main())
