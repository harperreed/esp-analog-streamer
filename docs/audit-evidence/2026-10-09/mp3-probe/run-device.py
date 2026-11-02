# ABOUTME: Runs the bounded disposable encoder probe and always restores the saved production ELF.
# ABOUTME: Retains UART and flash receipts; requires pyserial and espflash from the project environment.
from pathlib import Path
import subprocess,time,serial,json
root=Path.cwd();out=root/'target/mp3-benchmark/hardware'
fw=root/'firmware'; release=fw/'target/xtensa-esp32-espidf/release'
def flash(elf,label):
 with (out/(label+'.log')).open('w') as log:
  subprocess.run(['espflash','flash','--port','/dev/ttyUSB0','--chip','esp32','--flash-size','4mb','--flash-freq','80mhz','--partition-table',str(fw/'partitions.csv'),'--bootloader',str(release/'bootloader.bin'),str(elf)],stdout=log,stderr=subprocess.STDOUT,check=True)
def capture(filename,seconds,until_complete=False):
 with serial.Serial('/dev/ttyUSB0',115200,timeout=.2) as port,(out/filename).open('wb') as log:
  port.dtr=False;port.rts=True;time.sleep(.1);port.rts=False
  deadline=time.monotonic()+seconds;tail=b''
  while time.monotonic()<deadline:
   chunk=port.read(4096)
   if not chunk:continue
   log.write(chunk);log.flush();tail=(tail+chunk)[-20000:]
   if until_complete:
    if b'event=complete' in tail:return
    if b'Guru Meditation' in tail or b'assert failed:' in tail or b'Task watchdog got triggered' in tail:raise RuntimeError('device failure, see UART')
  if until_complete:raise TimeoutError('probe completion absent, see UART')
result={}
try:
 flash(release/'examples/mp3-cost','flash-probe');print('probe flashed',flush=True)
 capture('probe-uart.log',600,True);result['probe_completed']=True;print('probe completed',flush=True)
finally:
 flash(out/'production-before.elf','flash-restore');result['production_restored']=True;print('saved production flashed',flush=True)
 capture('restore-uart.log',25)
 (out/'run-result.json').write_text(json.dumps(result,indent=2)+'\n')
