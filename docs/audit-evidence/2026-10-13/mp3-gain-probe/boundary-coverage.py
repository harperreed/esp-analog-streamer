# ABOUTME: Checks stored quantizer indices against actual MP3 side information.
# ABOUTME: Verifies source formatter coverage without claiming independent Huffman decoding.
from pathlib import Path
import hashlib
import json
import numpy as np

OUT = Path(__file__).resolve().parent
BANDS = [0,4,8,12,16,20,24,30,36,42,50,60,72,88,106,128,156,190,230,276,330,384,576]

def header(blob):
    bits = ''.join(f'{x:08b}' for x in blob)
    position = 0
    def take(n):
        nonlocal position
        value = int(bits[position:position+n], 2)
        position += n
        return value
    assert take(9) == 0
    take(3)
    take(8)
    result = []
    for gr in range(2):
        for ch in range(2):
            length = take(12)
            big = take(9)
            gain = take(8)
            compress = take(4)
            assert take(1) == 0
            tables = [take(5) for _ in range(3)]
            region0 = take(4)
            region1 = take(3)
            preflag = take(1)
            scale = take(1)
            count1table = take(1)
            assert compress == preflag == scale == 0
            result.append(dict(big=big, gain=gain, tables=tables, region0=region0, region1=region1,
                               part2_3_length=length, count1table=count1table))
    assert position == 256
    return result

rows = []
for item in json.loads((OUT/'cause-experiments.json').read_text())['rows']:
    label = f'{item["variant"]}-{item["fixture"]}-{item["bitrate"]}'
    mp3 = OUT/(label+'.mp3')
    blob = mp3.read_bytes()
    framebytes = item['bitrate']*3
    spectral = np.fromfile(OUT/(label+'.boundary.i32le'),dtype='<i4').reshape(-1,576,2)
    steps = np.fromfile(OUT/(label+'.steps.i32le'),dtype='<i4').reshape(-1,2)
    side = [s for i in range(0,len(blob),framebytes) for s in header(blob[i+4:i+36])]
    assert len(side) == len(spectral) == len(steps)
    omitted = 0
    for values, step, s in zip(spectral,steps,side):
        assert s['gain'] == step[1] == step[0]+210
        xr, q = values[:,0], values[:,1]
        assert np.all((q==0) | (np.sign(q)==np.sign(xr)))
        absolute = np.abs(q)
        end = 576
        while end > 1 and np.all(absolute[end-2:end]==0):
            end -= 2
        count1end = end
        while end > 3 and np.all(absolute[end-4:end]<=1):
            end -= 4
        assert end == s['big']*2
        bounds = [0, BANDS[s['region0']+1], BANDS[s['region0']+s['region1']+2], end]
        for i,t in enumerate(s['tables']):
            begin = min(bounds[i],end)
            stop = min(bounds[i+1],end)
            if t == 0:
                omitted += stop-begin
                assert np.all(q[begin:stop]==0)
        omitted += 576-count1end
        assert np.all(q[count1end:]==0)
    rows.append(dict(label=label, granule_channel_rows=len(side), omitted_zero_positions=omitted,
                     nonzero_untransmitted_positions=0, source_formatter_coverage_verified=True,
                     mp3_sha256=hashlib.sha256(blob).hexdigest()))
result = dict(rows=rows, limitation='Uses actual side-info plus the pinned formatter and run-length rules; does not independently decode Huffman main data. Boundary metrics refer to stored quantizer coefficients.',
              source_pointers=['src/lib/l3loop.c:429 calc_runlen', 'src/lib/l3bitstream.c:32 signs',
                               'src/lib/l3bitstream.c:145 Huffmancodebits', 'src/lib/l3bitstream.c:122 transmitted global_gain'])
(OUT/'boundary-coverage.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
