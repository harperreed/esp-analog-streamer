# ABOUTME: Compares real FFmpeg fixed and floating decoders on retained synthetic MP3.
# ABOUTME: Counts raw PCM and fits gain on an explicitly delay-aligned steady window.
from pathlib import Path
import hashlib,json,subprocess
import numpy as np
root=Path(__file__).resolve().parents[3]; src=root/'target/mp3-benchmark/host'; out=Path(__file__).resolve().parent
reference=np.fromfile(src/'tones.s16le',dtype='<i2').reshape(-1,2).astype(np.float64)/32768
rows=[]
for decoder in ['mp3float','mp3']:
 dest=out/(decoder+'.f32le'); log=out/(decoder+'.log')
 cmd=['ffmpeg','-nostdin','-v','error','-c:a',decoder,'-i',str(src/'tones-320.mp3'),'-f','f32le','-c:a','pcm_f32le',str(dest)]
 with log.open('wb') as f: subprocess.run(cmd,stdout=subprocess.DEVNULL,stderr=f,timeout=20,check=True)
 assert not log.read_bytes()
 data=np.fromfile(dest,dtype='<f4').reshape(-1,2).astype(np.float64)
 lag=1057; start=4608; stop=min(len(reference),len(data)-lag)-4608
 x=reference[start:stop]; y=data[start+lag:stop+lag]
 gains=np.sum(x*y,axis=0)/np.sum(x*x,axis=0)
 rows.append({'decoder':decoder,'decoded_sample_frames':len(data),'fit_start':start,'fit_stop':stop,'alignment_samples':lag,'gain_per_channel':gains.tolist(),'gain_db_per_channel':(20*np.log10(gains)).tolist(),'reference_peak':np.max(np.abs(x),axis=0).tolist(),'decoded_peak':np.max(np.abs(y),axis=0).tolist(),'pcm_sha256':hashlib.sha256(dest.read_bytes()).hexdigest(),'command':cmd})
r={'reference_sha256':hashlib.sha256((src/'tones.s16le').read_bytes()).hexdigest(),'mp3_sha256':hashlib.sha256((src/'tones-320.mp3').read_bytes()).hexdigest(),'ffmpeg_version':subprocess.check_output(['ffmpeg','-version'],text=True).splitlines()[0],'rows':rows,'limits':['Two implementations in FFmpeg, not two wholly independent libraries.','Fixed1057sample lag from prior nonperiodic fixture; steadywindow excludes transientedges.','Synthetic retained half-scale tones; no physical source or audiblequality claim.']}
(out/'decoder-controls.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(rows))
