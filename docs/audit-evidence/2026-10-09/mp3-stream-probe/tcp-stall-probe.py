#!/usr/bin/env python3
# ABOUTME: Records bounded direct MP3 receive progress and own-socket Linux TCP_INFO.
# ABOUTME: Derives TCP_INFO field layout from installed headers and validates saved whole-frame prefixes.
import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time

spec = importlib.util.spec_from_file_location('acceptance', Path(__file__).with_name('acceptance.py'))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
FIELDS = ['tcpi_bytes_received', 'tcpi_data_segs_in', 'tcpi_rcv_ooopack', 'tcpi_rcv_space', 'tcpi_rcv_wnd']
RAW_LIMIT = 8 * 1024 * 1024
TELEMETRY_LIMIT = 4 * 1024 * 1024


def layout():
    declarations = '\n'.join('printf("%s %%zu %%zu\\n", offsetof(struct tcp_info, %s), sizeof(((struct tcp_info *)0)->%s));' % (name,name,name) for name in FIELDS)
    code = '#include <stdio.h>\n#include <stddef.h>\n#include <linux/tcp.h>\nint main(void) {\n'+declarations+'\nreturn 0;\n}\n'
    with tempfile.TemporaryDirectory() as directory:
        source = Path(directory)/'layout.c'
        binary = Path(directory)/'layout'
        source.write_text(code)
        subprocess.run(['cc','-Wall','-Wextra','-Werror',str(source),'-o',str(binary)],check=True,capture_output=True,timeout=15)
        output = subprocess.run([str(binary)],check=True,capture_output=True,text=True,timeout=5).stdout
    return {name: (int(offset),int(size)) for name,offset,size in (line.split() for line in output.splitlines())}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('host')
    parser.add_argument('--port',type=int,default=8000)
    parser.add_argument('--seconds',type=float,default=90)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    if not 1 <= args.seconds <= 90:
        parser.error('--seconds must be 1..90')
    os.umask(0o077)
    args.out.mkdir(parents=True,exist_ok=True)
    result=dict(passed=False,requested_seconds=args.seconds,listeners=[],
                verdict_scope='capture completed without EOF/error and whole prefixes decoded; delivery rate is reported separately')
    peers=[]
    jobs=[]
    stopping=threading.Event()
    threads=[]
    try:
        fields=layout()
        result['tcp_info_layout']={key:dict(offset=value[0],size=value[1]) for key,value in fields.items()}
        for index in range(2):
            peer,admitted=helper.connect(args.host,args.port,'/stream.mp3')
            peer.settimeout(.25)
            peers.append(peer)
            jobs.append(dict(label=f'listener-{index+1}',admitted=admitted,data=bytearray(),bytes=0,
                             last_recv=admitted,max_recv_gap=0,recv_calls=0,eof=False,error=None,
                             receiver_buffer_bytes=peer.getsockopt(socket.SOL_SOCKET,socket.SO_RCVBUF)))
        origin=time.monotonic()
        deadline=origin+args.seconds
        def drain(peer,job):
            try:
                while not stopping.is_set() and time.monotonic()<deadline:
                    try:
                        chunk=peer.recv(65536)
                    except socket.timeout:
                        continue
                    now=time.monotonic()
                    if not chunk:
                        job['eof']=True
                        job['eof_seconds']=now-origin
                        break
                    gap=now-job['last_recv']
                    job['max_recv_gap']=max(job['max_recv_gap'],gap)
                    job['last_recv']=now
                    job['recv_calls']+=1
                    if len(job['data'])+len(chunk)>RAW_LIMIT:
                        raise ValueError('raw capture exceeded 8 MiB')
                    job['data']+=chunk
                    job['bytes']+=len(chunk)
            except Exception as error:
                job['error']=f'{type(error).__name__}: {error}'
        telemetry=args.out/'tcp-info.jsonl'
        telemetry_bytes=0
        def sample():
            nonlocal telemetry_bytes
            try:
                with telemetry.open('w') as output:
                    next_sample=origin
                    while not stopping.is_set() and time.monotonic()<=deadline:
                        stamp=time.monotonic()
                        values=[]
                        for peer,job in zip(peers,jobs):
                            raw=peer.getsockopt(socket.IPPROTO_TCP,socket.TCP_INFO,512)
                            counters={}
                            for name,(offset,size) in fields.items():
                                counters[name]=int.from_bytes(raw[offset:offset+size],byteorder=os.sys.byteorder) if offset+size<=len(raw) else None
                            values.append(dict(label=job['label'],tcp_info_length=len(raw),counters=counters,
                                               app_bytes=job['bytes'],recv_calls=job['recv_calls'],
                                               seconds_since_recv=stamp-job['last_recv'],max_recv_gap=job['max_recv_gap'],
                                               eof=job['eof'],error=job['error']))
                        line=json.dumps(dict(seconds=stamp-origin,listeners=values),separators=(',',':'))+'\n'
                        telemetry_bytes+=len(line.encode())
                        if telemetry_bytes>TELEMETRY_LIMIT:
                            raise ValueError('telemetry exceeded 4 MiB')
                        output.write(line)
                        output.flush()
                        next_sample+=.1
                        stopping.wait(max(0,next_sample-time.monotonic()))
            except Exception as error:
                result['telemetry_error']=f'{type(error).__name__}: {error}'
                stopping.set()
        # Continuous recv drain and independent 100 ms sampling; no decoder competes with receive.
        for peer,job in zip(peers,jobs):
            thread=threading.Thread(target=drain,args=(peer,job),daemon=True)
            threads.append(thread)
            thread.start()
        observer=threading.Thread(target=sample,daemon=True)
        observer.start()
        while time.monotonic()<deadline and not stopping.is_set():
            if all(not thread.is_alive() for thread in threads):
                break
            stopping.wait(.1)
        stopping.set()
        for thread in threads+[observer]:
            thread.join(2)
            if thread.is_alive():
                raise TimeoutError('probe worker did not stop')
        for peer in peers:
            peer.close()
        peers.clear()
        observed_end=time.monotonic()
        for job in jobs:
            data=job.pop('data')
            whole=len(data)//960*960
            (args.out/f"{job['label']}-raw.mp3").write_bytes(data)
            job['partial_tail_bytes']=len(data)-whole
            job['received_audio_seconds']=whole//960*.024
            job['received_audio_per_wall_second']=job['received_audio_seconds']/(observed_end-origin)
            job['duration_seconds']=observed_end-origin
            job['final_no_receive_seconds']=observed_end-job['last_recv']
            for key in ['admitted','last_recv']:
                job.pop(key)
            try:
                job['prefix_decode']=helper.check(memoryview(data)[:whole],args.out,f"{job['label']}-whole-prefix")
            except Exception as error:
                job['decode_error']=f'{type(error).__name__}: {error}'
            result['listeners'].append(job)
        result['telemetry_bytes']=telemetry_bytes
        result['passed']=not result.get('telemetry_error') and all(not job['eof'] and not job['error'] and not job.get('decode_error') for job in jobs)
    except Exception as error:
        result['error']=f'{type(error).__name__}: {error}'
    finally:
        stopping.set()
        for peer in peers:
            peer.close()
        (args.out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
    return 0 if result['passed'] else 1


if __name__=='__main__':
    raise SystemExit(main())
