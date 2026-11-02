# ABOUTME: Compares corrected production MP3 with an independently patched upstream build.
# ABOUTME: Retains synthetic gain measurements and the normalized noise golden provenance.
from pathlib import Path
import hashlib
import json
import subprocess
import numpy as np

ROOT = Path.cwd()
OUT = ROOT / 'target/kata-hardening/oct13-mp3-normalization/correction'
OUT.mkdir(exist_ok=True)
UP = ROOT / 'target/mp3-benchmark/source/shine/src/lib'
CURRENT = ROOT / 'mp3/vendor/shine'
UNITS = ['bitstream', 'huffman', 'l3bitstream', 'l3loop', 'l3mdct', 'l3subband', 'layer3', 'reservoir', 'tables']

def run(args, name):
    result = subprocess.run([str(a) for a in args], capture_output=True)
    (OUT / (name+'.stdout')).write_bytes(result.stdout)
    (OUT / (name+'.stderr')).write_bytes(result.stderr)
    assert result.returncode == 0, (name, result.stderr.decode())
    assert not result.stderr, (name, result.stderr.decode())

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

source = (UP/'l3mdct.c').read_text()
old = 'cos((PI / 72) * (2 * k + 19) * (2 * m + 1)) * 0x7fffffff)'
assert source.count(old) == 1
patched = OUT/'l3mdct.c'
patched.write_text(source.replace(old, 'cos((PI / 72) * (2 * k + 19) * (2 * m + 1)) * (8.0 / 9.0) * 0x7fffffff)'))
run(['cc','-O3','-fsigned-char','-I',UP,
     ROOT/'docs/audit-evidence/2026-10-09/mp3-probe/host/probe.c',
     *[patched if n == 'l3mdct' else UP/(n+'.c') for n in UNITS],
     '-lm','-o',OUT/'upstream'], 'build-upstream')
run(['cc','-O3','-fsigned-char','-Wall','-Wextra','-Werror','-I',CURRENT,
     ROOT/'docs/audit-evidence/2026-10-13/mp3-gain-probe/current-bridge.c',
     ROOT/'mp3/src/bridge.c',*[CURRENT/(n+'.c') for n in UNITS],
     '-lm','-o',OUT/'current'], 'build-current')
state = 0x12345678
raw = bytearray()
for _ in range(16*2304):
    state ^= (state << 13) & 0xffffffff
    state ^= state >> 17
    state ^= (state << 5) & 0xffffffff
    raw.extend((state >> 16).to_bytes(2,'little'))
golden_input = OUT/'golden.s16le'
golden_input.write_bytes(raw)
assert sha(golden_input) == 'c6f048ed066614b109d9b2494488394854bd448c215407c697af98519a9956f0'
results = {}
for name in ['golden','tones','noise']:
    pcm = golden_input if name == 'golden' else ROOT/('target/mp3-benchmark/host/'+name+'.s16le')
    upstream, current = [OUT/(name+'-'+v+'.mp3') for v in ['upstream','current']]
    run([OUT/'upstream','320',pcm,upstream], name+'-upstream')
    run([OUT/'current',pcm,current], name+'-current')
    assert upstream.read_bytes() == current.read_bytes(), name
    entry = {'input_sha256':sha(pcm), 'mp3_sha256':sha(current), 'bytes':current.stat().st_size,
             'normalized_upstream_byte_equal':True}
    if name == 'golden':
        fnv = 0xcbf29ce484222325
        for byte in current.read_bytes():
            fnv = ((fnv ^ byte)*0x100000001b3) & 0xffffffffffffffff
        entry['fnv1a64'] = f'{fnv:016x}'
    else:
        decoded = OUT/(name+'.f32le')
        run(['ffmpeg','-hide_banner','-loglevel','error','-y','-c:a','mp3float','-i',current,
             '-f','f32le','-c:a','pcm_f32le',decoded],name+'-decode')
        x = np.fromfile(pcm,dtype='<i2').reshape(-1,2)/32768
        y = np.fromfile(decoded,dtype='<f4').reshape(-1,2).astype(float)
        assert len(x) == len(y)
        entry['decoded_sample_frames'] = len(y)
        entry['decoded_sha256'] = sha(decoded)
        left, right = 4608, len(x)-1057-4608
        x, y = x[left:right], y[left+1057:right+1057]
        gain = np.sum(x*y,axis=0)/np.sum(x*x,axis=0)
        entry['gain'] = gain.tolist()
        entry['snr_db'] = (10*np.log10(np.sum(x*x,axis=0)/np.sum((y-x)**2,axis=0))).tolist()
        entry['peak'] = np.max(np.abs(y),axis=0).tolist()
        if name == 'tones':
            assert np.all(np.abs(gain-1)<.005)
    results[name] = entry
results['production_mdct_sha256'] = sha(CURRENT/'l3mdct.c')
results['upstream_patch_sha256'] = sha(patched)
(OUT/'results.json').write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
