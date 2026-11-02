# ABOUTME: Validates bounded completed framehash files without a live-reader backlog limit.
# ABOUTME: Uses the production record validator and rejects omitted or duplicated samples.
from pathlib import Path
import sys,json
root=Path.cwd();sys.path.insert(0,str(root/'scripts'))
from decoder_integrity import FrameHash,validate_decoder_log

def validate(path):
 path=Path(path);assert 0<path.stat().st_size<=1024*1024,'completed framehash outside 1MiB bound'
 f=FrameHash(path)
 with path.open() as src:
  while line:=src.readline(16385):
   if len(line)>16384 or not line.endswith('\n'):raise ValueError('invalid or incomplete record')
   f._record(line.rstrip('\n'))
 if not f.frames:raise ValueError('no decoded samples')
 return f.summary()
if __name__=='__main__':
 if len(sys.argv)>1 and sys.argv[1]=='--self-test':
  p=root/'target/flac-buffer/hardware/music/20261008T234644.884950Z/listener-1.framehash'
  assert p.stat().st_size>131072
  live=FrameHash(p)
  try:live.update(final=True)
  except ValueError as e:assert 'cannot keep up' in str(e)
  else:raise AssertionError('expected old final-only parser defect')
  finally:live.close()
  assert validate(p)['decoded_audio_frames']==5760000
  lines=p.read_text().splitlines(keepends=True);at=next(i for i,s in enumerate(lines) if not s.startswith('#'))+2
  for name,mutated in [('drop',lines[:at]+lines[at+1:]),('duplicate',lines[:at]+[lines[at]]+lines[at:])]:
   bad=root/f'target/mp3-benchmark/{name}.framehash';bad.write_text(''.join(mutated))
   try:validate(bad)
   except ValueError as e:assert 'discontinuity' in str(e)
   else:raise AssertionError('negative control passed')
  print('old backlog failure reproduced; complete120s passed; omission and duplicate rejected')
 else:
  out=root/'target/mp3-benchmark/hardware/load';run=json.loads((out/'result.json').read_text());rows=[]
  for decoder in run['decoders']:
   row={'label':decoder['label'],'exit_code':decoder['exit_code'],'requested_stereo_frames':decoder['requested_stereo_frames']}
   try:
    row.update(validate(out/(row['label']+'.framehash')));validate_decoder_log(out/(row['label']+'.log'))
    row['passed']=row['decoded_audio_frames']==row['requested_stereo_frames'] and row['exit_code']==0
   except Exception as e:row['passed']=False;row['error']=str(e)
   rows.append(row)
  (out/'decoder-verification.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))
