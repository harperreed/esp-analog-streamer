# ABOUTME: Strictly reconstructs actual UART-exported MP3 and independently decodes it.
# ABOUTME: Validates missing/corrupt chunks and compares against continuous device PRNG input.
import pathlib,re,json,subprocess,hashlib
import numpy as np
p=pathlib.Path(__file__).resolve().parent
log=p.parent/'hardware/probe-uart.log'
def extract(text):
 outputs={};active=None
 for line in text.splitlines():
  if 'event=export_' not in line:continue
  m=re.search(r'event=export_(begin|chunk|end) (.*)',line);assert m,'invalid export line'
  kind,body=m.groups();fields=dict(x.split('=',1) for x in body.split());rate=int(fields['bitrate'])
  if kind=='begin':
   assert active is None and rate not in outputs and rate in (320,192)
   assert fields['fixture']=='continuous_xorshift_i16' and fields['seed']=='12345678'
   assert int(fields['input_frames'])==36864
   active=(rate,int(fields['bytes']),bytearray())
  else:
   assert active is not None and active[0]==rate;_,expected,data=active
   if kind=='chunk':
    assert int(fields['offset'])==len(data),'missing, duplicate or reordered chunk'
    h=fields['hex'];assert re.fullmatch('[0-9a-f]+',h) and len(h)%2==0,'invalid hex'
    b=bytes.fromhex(h);assert len(b)==min(128,expected-len(data)),'wrong chunk length';data.extend(b)
   else:
    assert int(fields['bytes'])==expected==len(data),'incomplete export';outputs[rate]=bytes(data);active=None
 assert active is None and set(outputs)=={320,192},'missing export'
 return outputs
text=log.read_bytes().decode('latin1');outputs=extract(text)
lines=text.splitlines();idx=next(i for i,x in enumerate(lines) if 'event=export_chunk' in x)
for label,mutated in [('missing_chunk','\n'.join(lines[:idx]+lines[idx+1:])),('bad_hex',text.replace('hex=','hex=z',1)),('missing_end','\n'.join(x for x in lines if 'event=export_end bitrate=192' not in x))]:
 try:extract(mutated)
 except (AssertionError,ValueError):pass
 else:raise AssertionError('negative control accepted: '+label)
rng=0x12345678;values=[]
for _ in range(36864*2):
 rng^=(rng<<13)&0xffffffff;rng^=rng>>17;rng^=(rng<<5)&0xffffffff;values.append((rng&65535)-65536 if rng&32768 else rng&65535)
ref=np.array(values,dtype='<i2').reshape(-1,2);raw=p/'device-decode-input.s16le';ref.tofile(raw);a=ref.astype(float)/32768;rows=[]
for rate,blob in outputs.items():
 mp3=p/f'device-decode-{rate}.mp3';mp3.write_bytes(blob);out=p/f'device-decode-{rate}.f32le'
 subprocess.run(['ffmpeg','-v','error','-y','-i',str(mp3),'-f','f32le','-acodec','pcm_f32le',str(out)],check=True)
 b=np.fromfile(out,dtype='<f4').reshape(-1,2);assert b.shape==(36864,2)
 corr=[np.sum(a[:min(len(a),len(b)-lag)]*b[lag:lag+min(len(a),len(b)-lag)]) for lag in range(1600)]
 lag=int(np.argmax(corr));x=a[:len(b)-lag];y=b[lag:];gain=float(np.sum(x*y)/np.sum(x*x))
 host=p/f'device-decode-host-{rate}.mp3';subprocess.run([str(p/'probe'),str(rate),str(raw),str(host)],check=True,capture_output=True)
 stride=rate*3;assert len(blob)==32*stride;assert all(blob[i]==255 and blob[i+1]&224==224 for i in range(0,len(blob),stride))
 rows.append(dict(bitrate=rate,bytes=len(blob),sha256=hashlib.sha256(blob).hexdigest(),input_samples_per_channel=36864,decoded_samples_per_channel=len(b),delay_samples=lag,compared_samples=len(x),gain=gain,snr_db=float(10*np.log10(np.sum(x*x)/np.sum((x-y)**2))),gain_adjusted_snr_db=float(10*np.log10(np.sum((gain*x)**2)/np.sum((gain*x-y)**2))),max_main_data_begin=max((blob[i+4]<<1)|(blob[i+5]>>7) for i in range(0,len(blob),stride)),host_bit_identical=host.read_bytes()==blob))
result=dict(uart_log_sha256=hashlib.sha256(log.read_bytes()).hexdigest(),negative_controls_rejected=['missing_chunk','bad_hex','missing_end'],results=rows)
(p/'device-decode-results.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
