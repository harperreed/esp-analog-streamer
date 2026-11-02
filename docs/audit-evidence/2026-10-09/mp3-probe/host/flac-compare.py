# ABOUTME: Compares actual production FLAC payload with Shine on exact common prefixes.
# ABOUTME: Independently verifies FLAC PCM equality and records content-dependent traffic savings.
import pathlib,subprocess,json
import numpy as np
p=pathlib.Path(__file__).resolve().parent;rows=[]
probe=p/'flac-target/release/mp3-feasibility-flac-probe'
for fixture in ('recorded_music','noise','tones','fade'):
 src=(p/(fixture+'.s16le')).read_bytes();prefix=src[:117*4096*4];raw=p/f'flac-prefix-{fixture}.s16le';raw.write_bytes(prefix)
 flac=p/f'production-{fixture}.flac';stats=json.loads(subprocess.check_output([str(probe),str(raw),str(flac)]))
 assert stats['samples_per_channel']==479232;duration=479232/48000
 decoded=p/f'production-{fixture}.decoded.s16le'
 subprocess.run(['ffmpeg','-v','error','-y','-i',str(flac),'-f','s16le','-acodec','pcm_s16le',str(decoded)],check=True)
 assert decoded.read_bytes()==prefix,'FLAC mismatch'
 item=dict(fixture=fixture,duration_seconds=duration,decoded_samples_per_channel=len(decoded.read_bytes())//4,lossless_pcm_equal=True,payload_bytes_per_second=stats['frame_bytes']/duration,total_with_header_bytes_per_second=flac.stat().st_size/duration,**stats)
 for rate in (320,192):
  mp3=p/f'flac-prefix-{fixture}-{rate}.mp3';st=json.loads(subprocess.check_output([str(p/'probe'),str(rate),str(raw),str(mp3)]));assert st['samples_per_channel']==479232
  throughput=st['bytes']/duration;item[f'mp3_{rate}_bytes_per_second']=throughput;item[f'mp3_{rate}_traffic_reduction_percent']=100*(1-throughput/item['payload_bytes_per_second'])
 rows.append(item)
(p/'flac-compare-results.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))
