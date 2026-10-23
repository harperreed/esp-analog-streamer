# ABOUTME: Runs bounded mixed-load MP3 measurements with real FLAC clients and local TLS.
# ABOUTME: Saves actual decoder counts and always stops the source and restores production.
from pathlib import Path
import subprocess,time,serial,json,re,sys,argparse
parser=argparse.ArgumentParser(description='Run temporary MP3 load probe with automatic production restore')
parser.add_argument('--exclusive',action='store_true',help='Capture/network/TLS with no FLAC encoder in the diagnostic')
parser.add_argument('--yield-repeat',action='store_true',help='Store the explicit RTOS-yield pacing repeat separately')
args=parser.parse_args()
root=Path.cwd();out=root/'target/mp3-benchmark/hardware'/(('capture' if args.exclusive else 'load')+('-yield' if args.yield_repeat else ''));out.mkdir(parents=True,exist_ok=True);out.chmod(0o700)
fw=root/'firmware';release=fw/'target/xtensa-esp32-espidf/release';players=[];clients=[];peer=None;source=None;events=[];readers={};decoder_errors={}
sys.path.insert(0,str(root/'scripts'))
from decoder_integrity import FrameHash,validate_decoder_log

def flash(elf,label):
 with (out/(label+'.log')).open('w') as log:
  subprocess.run(['espflash','flash','--port','/dev/ttyUSB0','--chip','esp32','--flash-size','4mb','--flash-freq','80mhz','--partition-table',str(fw/'partitions.csv'),'--bootloader',str(release/'bootloader.bin'),str(elf)],stdout=log,stderr=subprocess.STDOUT,check=True)
def client(label,seconds,boot_us):
 log=(out/(label+'.log')).open('w');dest=out/(label+'.framehash')
 p=subprocess.Popen(['ffmpeg','-hide_banner','-nostdin','-loglevel','level+warning','-rw_timeout','15000000','-f','flac','-i','http://192.168.23.217:8000/stream.flac','-t',str(seconds),'-c:a','pcm_s32le','-f','framehash','-hash','sha256','-y',str(dest)],stdout=subprocess.DEVNULL,stderr=log)
 readers[label]=FrameHash(dest)
 clients.append((label,p,log,dest,seconds));events.append({'event':'client_start','label':label,'requested_seconds':seconds,'phase_marker_boot_us':boot_us,'host_monotonic':time.monotonic()});print('client',label,'started',flush=True)
def stop(p):
 if p is not None and p.poll() is None:
  p.terminate()
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:p.kill();p.wait()
result={}
try:
 flash(release/('examples/mp3-capture' if args.exclusive else 'examples/mp3-load'),'flash-probe');print('load probe flashed',flush=True)
 peerlog=(out/'tls-peer.jsonl').open('w');peererr=(out/'tls-peer.stderr').open('w')
 peer=subprocess.Popen([str(root/'scripts/tls-capture-peer'),'--bind','192.168.23.153','--cert',str(root/'target/tls-capture/peer-cert.pem'),'--key',str(root/'target/tls-capture/peer-key.pem'),'--run-timeout','240'],stdout=peerlog,stderr=peererr)
 source=subprocess.Popen(['aplay','-q','-D','plughw:CARD=PCH,DEV=0',str(root/'target/flac-buffer/hardware/stereo-noise.wav')])
 with serial.Serial('/dev/ttyUSB0',115200,timeout=.2) as port,(out/'uart.log').open('wb') as uart:
  port.dtr=False;port.rts=True;time.sleep(.1);port.rts=False
  end=time.monotonic()+220;pending=b'';complete=False;seen=set()
  while time.monotonic()<end:
   chunk=port.read(4096)
   for label,reader in readers.items():
    if label not in decoder_errors:
     try:reader.update()
     except Exception as error:decoder_errors[label]=str(error)
   if not chunk:continue
   uart.write(chunk);uart.flush();pending+=chunk
   lines=pending.split(b'\n');pending=lines.pop()
   for line in lines:
    text=line.decode('latin1')
    if any(x in text for x in ['Guru Meditation','assert failed:','Task watchdog got triggered']):raise RuntimeError('device failure; see UART')
    match=re.search(r'mp3-load: event=phase_begin phase=(\w+) us=(\d+)',text)
    if match:
     phase=match[1];boot=int(match[2]);events.append({'event':'phase','phase':phase,'boot_us':boot,'host_monotonic':time.monotonic()});print('phase',phase,flush=True)
     assert phase not in seen;seen.add(phase)
     if phase=='one_flac':client('one',115,boot)
     elif phase=='two_flac':client('two',85,boot)
     elif phase=='recovery':
      departed=sum(p.poll() is not None for _,p,_,_,_ in clients)
      for i in range(departed):client('recovery-'+str(i+1),10,boot)
    if 'mp3-load: event=failed' in text:raise RuntimeError(text)
    if 'mp3-load: event=complete ' in text:complete=True;end=min(end,time.monotonic()+12);print('load measurement complete',flush=True)
  assert complete,'no completion marker'
 result['measurement_complete']=True
finally:
 try:
  stop(source)
  for label,p,log,path,seconds in clients:
   stop(p);log.close();record={'label':label,'exit_code':p.returncode,'requested_stereo_frames':seconds*48000}
   f=readers[label]
   try:
    f.update(final=True)
    if label in decoder_errors:raise ValueError(decoder_errors[label])
    validate_decoder_log(out/(label+'.log'));record.update(f.summary());record['passed']=f.frames==seconds*48000 and p.returncode==0
   except Exception as error:record.update(f.summary());record['passed']=False;record['error']=str(error)
   finally:f.close()
   result.setdefault('decoders',[]).append(record)
  stop(peer)
  if peer is not None:result['tls_peer_exit_code']=peer.returncode;peerlog.close();peererr.close()
  result['events']=events
  (out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
 finally:
  flash(root/'target/mp3-benchmark/hardware/production-before.elf','flash-restore');result['production_restored']=True;print('saved production flashed',flush=True)
  with serial.Serial('/dev/ttyUSB0',115200,timeout=.2) as port,(out/'restore-uart.log').open('wb') as log:
   port.dtr=False;port.rts=True;time.sleep(.1);port.rts=False;end=time.monotonic()+25
   while time.monotonic()<end:
    data=port.read(4096)
    if data:log.write(data);log.flush()
  (out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
