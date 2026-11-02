# ABOUTME: Generates deterministic stereo fixtures and measures independent FFmpeg decode.
# ABOUTME: Writes raw artifacts and delay-aligned fidelity metrics for a throwaway probe.
import json, subprocess, pathlib
import numpy as np
p=pathlib.Path(__file__).resolve().parent
n=1152*420;t=np.arange(n)/48000;rng=np.random.default_rng(20261009)
fixtures={"tones":np.column_stack([.5*np.sin(2*np.pi*997*t),.5*np.sin(2*np.pi*1703*t)]),"noise":rng.uniform(-.35,.35,(n,2))}
# Synthetic polyphonic fixture with transient envelopes; this is not recorded music.
music=np.zeros((n,2))
for ch,notes in enumerate(([130.81,164.81,196,261.63],[146.83,174.61,220,293.66])):
 for k,f in enumerate(notes):
  env=np.exp(-5*((t+k*.17)% .75))
  music[:,ch]+=.12*env*(np.sin(2*np.pi*f*t)+.3*np.sin(2*np.pi*2*f*t))
fixtures['synthetic_music']=music
fixtures['fade']=fixtures['tones']*np.linspace(1,0,n)[:,None]
private_source=p.parents[1]/'tls-capture/music-180s.wav'
subprocess.run(['ffmpeg','-v','error','-y','-i',str(private_source),'-t','10.08','-ar','48000','-ac','2','-f','s16le',str(p/'recorded_music.s16le')],check=True)
fixtures['recorded_music']=np.fromfile(p/'recorded_music.s16le',dtype='<i2').reshape(-1,2).astype(float)/32767
assert len(fixtures['recorded_music'])==n
rows=[]
for name,x in fixtures.items():
 src=np.rint(x*32767).astype('<i2');raw=p/(name+'.s16le');src.tofile(raw)
 for rate in (320,192):
  mp3=p/f'{name}-{rate}.mp3';decoded=p/f'{name}-{rate}.decoded.f32le'
  timing=json.loads(subprocess.check_output([str(p/'probe'),str(rate),str(raw),str(mp3)]))
  subprocess.run(['ffmpeg','-v','error','-y','-i',str(mp3),'-f','f32le','-acodec','pcm_f32le',str(decoded)],check=True)
  y=np.fromfile(decoded,dtype='<f4').reshape(-1,2);ref=src.astype(float)/32768
  # Cross-correlation over a nonperiodic multichannel sequence; tones ambiguity constrained to known pipeline window.
  corr=[]
  for lag in range(0,1600):
   length=min(len(ref),len(y)-lag,480000)
   corr.append(np.sum(ref[:length]*y[lag:lag+length]))
  lag=int(np.argmax(corr));length=min(len(ref),len(y)-lag);a=ref[:length];b=y[lag:lag+length];err=a-b
  gain=float(np.sum(a*b)/np.sum(a*a)); adjusted_snr=float(10*np.log10(np.sum((gain*a)**2)/np.sum((gain*a-b)**2)))
  framebytes=rate*3;blob=mp3.read_bytes();assert len(blob)%framebytes==0
  assert all(blob[i]==255 and blob[i+1]&224==224 for i in range(0,len(blob),framebytes))
  reservoir=[(blob[i+4]<<1)|(blob[i+5]>>7) for i in range(0,len(blob),framebytes)]
  rows.append(dict(fixture=name,rate_kbps=rate,decoded_samples_per_channel=len(y),estimated_delay_samples=lag,compared_samples=length,missing_tail_samples=len(ref)-length,snr_db=10*np.log10(np.sum(a*a)/np.sum(err*err)),rmse=float(np.sqrt(np.mean(err*err))),gain=gain,gain_adjusted_snr_db=adjusted_snr,max_main_data_begin=max(reservoir),**timing))
(p/'results.json').write_text(json.dumps(rows,indent=2)+'\n')
print(json.dumps(rows,indent=2))
