# ABOUTME: Measures corrected production MP3 headroom on bounded synthetic inputs.
# ABOUTME: Retains commands, raw floats, source hashes and errors without playing audio.
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
RATE, BLOCK, BLOCKS, LAG, TRIM = 48000, 1152, 96, 1057, 4608
UNITS = 'bitstream huffman l3bitstream l3loop l3mdct l3subband layer3 reservoir tables'.split()
COMMANDS = []


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(args, name):
    log = OUT / (name + '.log')
    receipt = {'argv': [str(x) for x in args], 'cwd': str(ROOT), 'log': str(log)}
    COMMANDS.append(receipt)
    with log.open('wb') as stream:
        result = subprocess.run(args, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT, timeout=90)
    receipt.update(exit_code=result.returncode, log_sha256=sha(log), log_bytes=log.stat().st_size)
    (OUT / 'commands.json').write_text(json.dumps(COMMANDS, indent=2) + '\n')
    if result.returncode:
        raise RuntimeError(f'{name}: exit {result.returncode}; see {log}')
    return log.read_text()


def scalar(value):
    return float(value) if np.isfinite(value) else None


def metrics(x, y):
    rows = []
    for ch in range(2):
        a, b = x[:, ch], y[:, ch]
        a2, b2 = np.mean(a*a), np.mean(b*b)
        error = b-a
        e2 = np.mean(error*error)
        rows.append({
            'channel': ch, 'frames': len(a), 'input_rms': float(np.sqrt(a2)),
            'decoded_rms': float(np.sqrt(b2)), 'rms_ratio': scalar(np.sqrt(b2/a2)) if a2 else None,
            'least_squares_gain': scalar(np.dot(a,b)/np.dot(a,a)) if a2 else None,
            'error_rms': float(np.sqrt(e2)),
            'relative_error_rms': scalar(np.sqrt(e2/a2)) if a2 else None,
            'snr_db': scalar(10*np.log10(a2/e2)) if a2 and e2 else None,
            'input_peak': float(np.max(np.abs(a))), 'decoded_peak': float(np.max(np.abs(b))),
            'decoded_min': float(np.min(b)), 'decoded_max': float(np.max(b)),
            'outside_unit_interval': int(np.count_nonzero((b < -1) | (b > 1))),
            'nonfinite': int(np.count_nonzero(~np.isfinite(b))),
        })
    return rows


def patterns():
    n = BLOCK*BLOCKS
    t = np.arange(n)/RATE
    a, b = np.sin(2*np.pi*997*t), np.sin(2*np.pi*1703*t)
    zeros = np.zeros(n)
    rng = np.random.Generator(np.random.PCG64(0xD0C70B12))
    noise = rng.uniform(-1, 1, (n,2))
    transient = np.zeros((n,2))
    # Isolated impulses have >4608 samples of silence on each side.
    transient[12000] = [1, -1]
    # Abrupt gate edges exercise long-block ringing and pre-echo independently.
    transient[30017:33791,0] = a[30017:33791]
    transient[30017:33791,1] = b[30017:33791]
    transient[60029:61991,0] = a[60029:61991]
    transient[60029:61991,1] = -b[60029:61991]
    return {
        'tones_independent': np.column_stack((a,b)),
        'tones_same': np.column_stack((a,a)),
        'tones_opposite': np.column_stack((a,-a)),
        'tone_left_only': np.column_stack((a,zeros)),
        'tone_right_only': np.column_stack((zeros,b)),
        'impulse_and_gated_tones': transient,
        'noise_independent': noise,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__ or 'Offline MP3 headroom characterization; writes only beside this script.')
    parser.add_argument('--replace', action='store_true', help='Replace this diagnostic run after retaining any previous results elsewhere.')
    parser.add_argument('--source-tree', type=Path, default=ROOT, help='Use a retained source tree containing mp3/vendor/shine and mp3/src/bridge.c; defaults to live repository.')
    parser.add_argument('--include-lowlevel', action='store_true', help='Add unequal -60 and -90 dBFS tones to assess integer precision.')
    args = parser.parse_args()
    if (OUT/'headroom.json').exists() and not args.replace:
        parser.error('headroom.json exists; archive the directory or explicitly use --replace')
    source_tree = args.source_tree.resolve()
    vendor = source_tree/'mp3/vendor/shine'
    sources = [ROOT/'docs/audit-evidence/2026-10-13/mp3-gain-probe/current-bridge.c', source_tree/'mp3/src/bridge.c'] + [vendor/(u+'.c') for u in UNITS]
    hash_paths = sources + list(vendor.glob('*.h'))
    source_hashes = {str(p.relative_to(ROOT)):sha(p) for p in hash_paths}
    encoder = OUT/'actual-bridge'
    ffmpeg = Path(shutil.which('ffmpeg')).resolve()
    versions = {
        'ffmpeg': run([ffmpeg, '-version'], 'ffmpeg-version'),
        'cc': run(['cc', '--version'], 'cc-version'),
        'git_head': run(['git', 'rev-parse', 'HEAD'], 'git-head').strip(),
        'numpy': np.__version__, 'python': sys.version,
    }
    run(['cc', '-O3', '-fsigned-char', '-Wall', '-Wextra', '-Werror', '-I'+str(vendor), *sources, '-lm', '-o', encoder], 'compile')
    assert not (OUT/'compile.log').read_bytes(), 'compiler diagnostics need review'
    fixtures = []
    signals = patterns()
    if args.include_lowlevel:
        signals['tones_minus60db'] = signals['tones_independent']
        signals['tones_minus90db'] = signals['tones_independent']
    for pattern, raw in signals.items():
        levels = [('minus60db',0.001)] if pattern.endswith('minus60db') else [('minus90db',10**(-90/20))] if pattern.endswith('minus90db') else [('half',0.5),('nearfull',0.98)]
        for label, amplitude in levels:
            name = pattern+'_'+label
            pcm = np.rint(raw*amplitude*32768).astype('<i2')
            assert np.max(np.abs(pcm.astype(np.int32))) <= round(amplitude*32768)
            input_path, mp3_path, float_path = [OUT/(name+ext) for ext in ['.s16le','.mp3','.f32le']]
            pcm.tofile(input_path)
            run([encoder,input_path,mp3_path], name+'-encode')
            assert not (OUT/(name+'-encode.log')).read_bytes(), 'encoder diagnostics need review'
            run([ffmpeg,'-nostdin','-v','error','-c:a','mp3float','-i',mp3_path,'-f','f32le','-c:a','pcm_f32le','-y',float_path], name+'-decode')
            assert not (OUT/(name+'-decode.log')).read_bytes(), 'decoder diagnostics need review'
            x = pcm.astype(np.float64)/32768
            y = np.fromfile(float_path,dtype='<f4').reshape(-1,2).astype(np.float64)
            assert len(y) == len(x) == BLOCK*BLOCKS
            assert mp3_path.stat().st_size == 960*BLOCKS
            stop = min(len(x),len(y)-LAG)
            row = {'name':name,'pattern':pattern,'nominal_amplitude':amplitude,
                'input_frames':len(x),'encoded_frames':mp3_path.stat().st_size//960,'decoded_sample_frames':len(y),
                'alignment_samples':LAG,'aligned_source_start':0,'aligned_source_stop':stop,
                'steady_source_start':TRIM,'steady_source_stop':stop-TRIM,
                'aligned':metrics(x[:stop],y[LAG:stop+LAG]),
                'steady':metrics(x[TRIM:stop-TRIM],y[TRIM+LAG:stop-TRIM+LAG]),
                'all_decoded':[{key:value for key,value in item.items() if key in [
                    'channel','frames','decoded_rms','decoded_peak','decoded_min','decoded_max',
                    'outside_unit_interval','nonfinite']} for item in metrics(np.zeros_like(y),y)],
                'artifacts':{p.name:{'sha256':sha(p),'bytes':p.stat().st_size} for p in [input_path,mp3_path,float_path]},
                'errors':{'encode_exit':0,'decode_exit':0,'encoder_log_bytes':0,'decoder_log_bytes':0},
            }
            if pattern == 'impulse_and_gated_tones':
                row['segments'] = {label:metrics(x[start:end],y[start+LAG:end+LAG]) for label,start,end in [
                    ('isolated_impulse',7000,17000),('gated_tones',25000,68000)]}
            fixtures.append(row)
            print(name, 'peaks', [v['decoded_peak'] for v in row['all_decoded']], 'outside', [v['outside_unit_interval'] for v in row['all_decoded']], flush=True)
    assert source_hashes == {str(p.relative_to(ROOT)):sha(p) for p in hash_paths}, 'Source changed during acquisition; retain failed run and repeat from stable tree'
    report = {'scope':'Throwaway offline actual production bridge headroom characterization; no production edits or playback.',
        'reproduce':['uv','run','--quiet','--locked','python3',str(OUT/'probe.py')],
        'versions':versions,'ffmpeg_binary':str(ffmpeg),'ffmpeg_binary_sha256':sha(ffmpeg),
        'sources_sha256':source_hashes,'source_tree':str(source_tree),
        'encoder_sha256':sha(encoder),'script_sha256':sha(__file__),
        'sample_rate':RATE,'channels':2,'bitrate_kbps':320,'frames_per_case':BLOCK*BLOCKS,
        'design':'Seven patterns at half and 0.98 scale: unequal tones cover both channel frequencies; same/opposite and each hard-pan cover stereo extremes; silence-separated impulse and abrupt tone gates cover transients; PCG64(seed0xD0C70B12) uniform independent noise covers broadband quantization. This is a bounded set, not a Cartesian sweep.',
        'fixtures':fixtures,
        'limits':['Known1057sample lag is fixed from previous nonperiodic proof; no gain compensation applied.',
            'All decoded float samples are checked before signed16 conversion; outside-unit counts indicate potential downstream clipping, not automatically integer overflow.',
            'Steady metrics exclude4608source samples at both edges; isolated impulse and gate metrics retain transients.',
            'This probe does not drain finite-stream tails or verify tail preservation.',
            'No listening, music transparency, calibrated analog path, firmware timing, transport or endurance claim.',
            'Finite fixtures cannot establish a worst-case all-input integer bound.'],
        'commands':COMMANDS}
    (OUT/'headroom.json').write_text(json.dumps(report,indent=2)+'\n')


if __name__ == '__main__':
    main()
