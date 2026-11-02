# ABOUTME: Measures the pinned encoder quantizer against decoded synthetic PCM.
# ABOUTME: Retains raw boundaries and one floating quantizer control for diagnosis only.
from pathlib import Path
import hashlib
import json
import subprocess
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
SHINE = ROOT / 'target/mp3-benchmark/source/shine/src/lib'
HOST = ROOT / 'target/mp3-benchmark/host'
NAMES = ['bitstream', 'huffman', 'l3bitstream', 'l3loop', 'l3mdct', 'l3subband', 'layer3', 'reservoir', 'tables']

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(command, label):
    with (OUT / (label + '.log')).open('wb') as log:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=log, check=True, timeout=45)
    if (OUT / (label + '.log')).read_bytes():
        raise RuntimeError('Unexpected output in ' + label + '.log')
    return result.stdout.decode()

def main():
    rows = []
    source = (SHINE / 'l3loop.c').read_text()
    begin = source.index('int quantize(int ix[GRANULE_SIZE], int stepsize, shine_global_config *config) {')
    end = source.index('\n/*\n * ix_max:', begin)
    replacement = '''int quantize(int ix[GRANULE_SIZE], int stepsize, shine_global_config *config) {
  int max = 0;
  double scale = exp2(-(double)stepsize / 4.0);
  for (int i = 0; i < GRANULE_SIZE; ++i) {
    double value = (double)config->l3loop.xrabs[i] / 2147483648.0 * scale;
    ix[i] = (int)floor(pow(value, 0.75) - 0.0946 + 0.5);
    if (ix[i] > max) max = ix[i];
  }
  return max;
}
'''
    variant = OUT / 'l3loop-direct-quantizer.c'
    variant.write_text(source[:begin] + replacement + source[end:])
    commands = []
    for name in ['baseline', 'direct-quantizer']:
        binary = OUT / ('cause-' + name)
        sources = [str(variant if (name == 'direct-quantizer' and item == 'l3loop') else SHINE / (item + '.c')) for item in NAMES]
        build = ['cc', '-O3', '-fsigned-char', '-I', str(SHINE), str(OUT / 'cause-probe.c'), *sources, '-lm', '-o', str(binary)]
        run(build, name + '-build')
        commands.append(build)
        for fixture in ['tones', 'noise']:
            for rate in [320, 192]:
                label = f'{name}-{fixture}-{rate}'
                mp3 = OUT / (label + '.mp3')
                boundary = OUT / (label + '.boundary.i32le')
                steps = OUT / (label + '.steps.i32le')
                command = [str(binary), str(rate), str(HOST / (fixture + '.s16le')), str(mp3), str(boundary), str(steps)]
                timing = json.loads(run(command, label + '-encode'))
                decoded = OUT / (label + '.f32le')
                decode = ['ffmpeg', '-nostdin', '-v', 'error', '-y', '-i', str(mp3), '-f', 'f32le', '-c:a', 'pcm_f32le', str(decoded)]
                run(decode, label + '-decode')
                commands.extend([command, decode])
                x = np.fromfile(HOST / (fixture + '.s16le'), dtype='<i2').reshape(-1, 2).astype(float) / 32768
                y = np.fromfile(decoded, dtype='<f4').reshape(-1, 2).astype(float)
                start = 4608
                stop = min(len(x), len(y) - 1057) - 4608
                a, b = x[start:stop], y[start+1057:stop+1057]
                gain = np.sum(a*b, axis=0) / np.sum(a*a, axis=0)
                data = np.fromfile(boundary, dtype='<i4').reshape(-1, 576, 2)[8:-8]
                scale = np.fromfile(steps, dtype='<i4').reshape(-1, 2)[8:-8]
                assert np.all(scale[:, 1] == scale[:, 0] + 210)
                xr = data[:, :, 0].astype(float) / (2**31)
                q = data[:, :, 1].astype(float)
                reconstructed = np.sign(q) * np.abs(q)**(4/3) * np.exp2(scale[:, 0]/4)[:, None]
                spectral_gain = float(np.sum(xr*reconstructed) / np.sum(xr*xr))
                spectral_snr = float(10*np.log10(np.sum(xr*xr) / np.sum((xr-reconstructed)**2)))
                rows.append(dict(variant=name, fixture=fixture, bitrate=rate, **timing,
                    pcm_gain_per_channel=gain.tolist(), quantizer_gain=spectral_gain,
                    quantizer_snr_db=spectral_snr, step_min=int(scale[:,0].min()), step_max=int(scale[:,0].max()),
                    mp3_matches_retained=mp3.read_bytes() == (HOST / f'{fixture}-{rate}.mp3').read_bytes(),
                    artifacts={p.name:digest(p) for p in [mp3,boundary,steps,decoded]}))
    result = dict(rows=rows, commands=commands, source_hashes={p.name:digest(p) for p in [SHINE/(n+'.c') for n in NAMES]},
        probe_sha256=digest(OUT/'cause-probe.c'), direct_quantizer_sha256=digest(variant),
        rules={'pcm_window':'start4608, stop=min(actualsource,actualdecode-1057)-4608; lag1057',
               'boundary':'native little-endian int32 pairs mdct_freq,signedl3_enc for each frame/gr/ch/bin; stepspairs quantizerStepSize,global_gain',
               'inverse_quantizer':'sign(q)*abs(q)^(4/3)*2^((global_gain-210)/4); allscalefactorszero',
               'spectral_window':'discardfirstandlast8granule/channelrows',
               'direct_variant':'Only quantize() replaced with real-valued3/4power and same -.0946+.5 rounding as originallookup; no productionchanges'})
    (OUT/'cause-experiments.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(rows,indent=2))

if __name__ == '__main__':
    main()
