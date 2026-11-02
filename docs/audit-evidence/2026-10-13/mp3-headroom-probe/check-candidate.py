# ABOUTME: Checks the frozen headroom fix with full UBSan and low-level controls.
# ABOUTME: Compares actual float decodes with retained baseline outputs and hashes.
from pathlib import Path
import argparse
import json

import probe

parser=argparse.ArgumentParser(description='Verify corrected headroom candidate after probe.py; optional constructive witness uses real signed16 input.')
parser.add_argument('--witness',type=Path,help='Constructive full-scale signed16 stereo fixture from the bound derivation.')
args=parser.parse_args()
OUT,ROOT=probe.OUT,probe.ROOT
BASE=OUT.with_name('oct13-mp3-headroom')
r=json.loads((OUT/'headroom.json').read_text())
original=json.loads((BASE/'headroom.json').read_text())
tree=Path(r['source_tree'])
vendor=tree/'mp3/vendor/shine'
sources=[ROOT/'docs/audit-evidence/2026-10-13/mp3-gain-probe/current-bridge.c',tree/'mp3/src/bridge.c',*[vendor/(u+'.c') for u in probe.UNITS]]
for name,digest in r['sources_sha256'].items():
    assert probe.sha(ROOT/name)==digest
probe.run(['cc','-O1','-g','-fsigned-char','-Wall','-Wextra','-Werror','-fsanitize=undefined',
    '-fno-sanitize-recover=undefined','-I'+str(vendor),*sources,'-lm','-o',OUT/'ubsan-bridge'],'ubsan-compile')
assert not (OUT/'ubsan-compile.log').read_bytes()
sanitizers=[]
for label in ['tones_independent_nearfull','noise_independent_nearfull','impulse_and_gated_tones_nearfull']:
    log=probe.run([OUT/'ubsan-bridge',OUT/(label+'.s16le'),OUT/(label+'-ubsan.mp3')],label+'-ubsan')
    assert not log
    sanitizers.append({'fixture':label,'exit_code':0,'log_bytes':0})
minimal=BASE/'minimal-tones-nearfull-2-blocks.s16le'
assert minimal.stat().st_size==9216
assert not probe.run([OUT/'ubsan-bridge',minimal,OUT/'minimal-tones-nearfull-2-blocks-ubsan.mp3'],'minimal-nearfull-ubsan')
sanitizers.append({'fixture':str(minimal),'exit_code':0,'log_bytes':0,'input_sha256':probe.sha(minimal)})
if args.witness:
    witness=args.witness.resolve()
    assert witness.stat().st_size%(1152*2*2)==0
    assert not probe.run([OUT/'ubsan-bridge',witness,OUT/'constructive-witness-ubsan.mp3'],'constructive-witness-ubsan')
    assert not probe.run(['ffmpeg','-nostdin','-v','error','-c:a','mp3float','-i',OUT/'constructive-witness-ubsan.mp3','-f','f32le','-c:a','pcm_f32le','-y',OUT/'constructive-witness-ubsan.f32le'],'constructive-witness-decode')
    data=probe.np.fromfile(OUT/'constructive-witness-ubsan.f32le',dtype='<f4').reshape(-1,2)
    assert len(data)==witness.stat().st_size//4
    assert probe.np.isfinite(data).all()
    sanitizers.append({'fixture':str(witness),'exit_code':0,'log_bytes':0,'input_sha256':probe.sha(witness),
        'actual_decoded_frames':len(data),'decoded_peak_per_channel':probe.np.max(probe.np.abs(data),axis=0).tolist(),
        'outside_unit_per_channel':probe.np.sum((data < -1)|(data > 1),axis=0).tolist()})
low=[]
for name in ['tones_minus60db_minus60db','tones_minus90db_minus90db']:
    inp=OUT/(name+'.s16le');mp3=OUT/(name+'-baseline.mp3');dest=OUT/(name+'-baseline.f32le')
    assert not probe.run([BASE/'actual-bridge',inp,mp3],name+'-baseline-encode')
    assert not probe.run(['ffmpeg','-nostdin','-v','error','-c:a','mp3float','-i',mp3,'-f','f32le','-c:a','pcm_f32le','-y',dest],name+'-baseline-decode')
    x=probe.np.fromfile(inp,dtype='<i2').reshape(-1,2).astype(float)/32768
    y=probe.np.fromfile(dest,dtype='<f4').reshape(-1,2).astype(float)
    assert len(x)==len(y)==110592
    fixed=next(row for row in r['fixtures'] if row['name']==name)
    low.append({'fixture':name,'baseline_steady':probe.metrics(x[4608:len(y)-1057-4608],y[4608+1057:len(y)-4608]),'candidate_steady':fixed['steady'],
        'baseline_mp3_sha256':probe.sha(mp3),'baseline_float_sha256':probe.sha(dest)})
comparisons=[]
assert len(r['fixtures'])==16
for fixed in r['fixtures']:
    for filename,expected in fixed['artifacts'].items():
        assert probe.sha(OUT/filename)==expected['sha256']
    assert fixed['decoded_sample_frames']==110592
    assert all(row['nonfinite']==0 for row in fixed['all_decoded'])
    if 'minus' in fixed['name']:
        continue
    old=next(row for row in original['fixtures'] if row['name']==fixed['name'])
    comparisons.append({'fixture':fixed['name'],'baseline_steady':old['steady'],'candidate_steady':fixed['steady'],
        'baseline_all_decoded':old['all_decoded'],'candidate_all_decoded':fixed['all_decoded']})
    if 'tone' in fixed['name'] and not fixed['name'].startswith('impulse'):
        assert all(row['outside_unit_interval']==0 for row in fixed['all_decoded'])
        assert all(row['snr_db'] is None or row['snr_db']>70 for row in fixed['steady'])
        assert all(row['least_squares_gain'] is None or abs(row['least_squares_gain']-1)<0.005 for row in fixed['steady'])
for name,digest in r['sources_sha256'].items():
    assert probe.sha(ROOT/name)==digest
r['candidate_checks']={'full_ubsan':sanitizers,'low_level_comparison':low,'baseline_comparison':comparisons,'diagnostic_commands':probe.COMMANDS,
    'limitations':['Expected lossy float overshoot is not removed or accepted by this integer representation fix.',
    'Finite synthetic tests do not prove music transparency, physical calibration, delivery, finite tail or arbitrary input bounds.',
    'All-input transform bounds are supplied by the separate bound derivation, not inferred from this fixture set.']}
(OUT/'headroom.json').write_text(json.dumps(r,indent=2)+'\n')
(OUT/'candidate-checks.json').write_text(json.dumps(r['candidate_checks'],indent=2)+'\n')
print('Candidate16case hash/count checks, tonal unity/SNR/headroom checks and requested fullUBSan cases pass.')
for row in low:
    print(row['fixture'],'baselineSNR',[m['snr_db'] for m in row['baseline_steady']],'candidateSNR',[m['snr_db'] for m in row['candidate_steady']])
