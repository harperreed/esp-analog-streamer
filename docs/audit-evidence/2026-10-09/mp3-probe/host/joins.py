# ABOUTME: Checks independent FFmpeg decoding after deterministic frame-boundary joins.
# ABOUTME: Compares joined output with full-stream decoding to expose filter warmup.
import pathlib,subprocess,json
import numpy as np
p=pathlib.Path(__file__).resolve().parent
rows=[]
for fixture in ('noise','recorded_music'):
 for rate in (320,192):
  blob=(p/f'{fixture}-{rate}.mp3').read_bytes();full=np.fromfile(p/f'{fixture}-{rate}.decoded.f32le',dtype='<f4').reshape(-1,2)
  for frame in (1,17,101,319):
   path=p/f'{fixture}-{rate}-join-{frame}.mp3';out=path.with_suffix('.decoded.f32le');path.write_bytes(blob[frame*rate*3:])
   subprocess.run(['ffmpeg','-v','error','-y','-i',str(path),'-f','f32le','-acodec','pcm_f32le',str(out)],check=True)
   joined=np.fromfile(out,dtype='<f4').reshape(-1,2);ref=full[frame*1152:];assert joined.shape==ref.shape
   err=np.max(np.abs(joined-ref),axis=1);bad=np.flatnonzero(err>1e-6)
   rows.append(dict(fixture=fixture,rate_kbps=rate,join_frame=frame,decoded_samples_per_channel=len(joined),expected_samples_per_channel=(420-frame)*1152,last_difference_over_1e_6_sample=int(bad[-1]) if len(bad) else -1,first_frame_rmse=float(np.sqrt(np.mean((joined[:1152]-ref[:1152])**2))),after_two_frames_max_error=float(err[2304:].max())))
(p/'joins.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))
