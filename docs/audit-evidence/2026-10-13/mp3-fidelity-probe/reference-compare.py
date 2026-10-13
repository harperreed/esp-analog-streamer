# ABOUTME: Compares retained corrected Shine fixtures with local libmp3lame offline.
# ABOUTME: Saves exact sources, raw float metrics, delay controls, spectra and commands.
"""Throwaway analysis. Run from repository: uv run --quiet --locked python3 PATH."""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import sys

import numpy as np

OUT = Path(__file__).resolve().parent
ROOT = OUT.parents[2]
RETAINED = ROOT / 'target/kata-hardening/oct13-mp3-headroom-fixed'
CANDIDATE = ROOT / 'docs/audit-evidence/2026-10-13/mp3-headroom-probe/candidate.json'
N, RATE, TRIM = 110592, 48000, 4608
CASES = [f'{kind}_{level}' for kind in ['noise_independent', 'tones_independent', 'impulse_and_gated_tones'] for level in ['half', 'nearfull']]
COMMANDS = []


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(argv, name):
    log = OUT / f'{name}.log'
    with log.open('wb') as stream:
        result = subprocess.run([str(a) for a in argv], cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT, timeout=60)
    COMMANDS.append({'argv': [str(a) for a in argv], 'exit': result.returncode, 'log': log.name, 'log_sha256': sha(log), 'log_bytes': log.stat().st_size})
    (OUT / 'commands.json').write_text(json.dumps(COMMANDS, indent=2) + '\n')
    if result.returncode:
        raise RuntimeError(f'{name}: exit {result.returncode}; see {log}')
    return log.read_text()


def encode(source, destination, name):
    run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 's16le', '-ar', RATE, '-ac', 2, '-i', source,
         '-c:a', 'libmp3lame', '-b:a', '320k', '-write_xing', 0, '-id3v2_version', 0, '-y', destination], name)
    assert (OUT / f'{name}.log').stat().st_size == 0


def decode(source, destination, name):
    run(['ffmpeg', '-nostdin', '-v', 'error', '-c:a', 'mp3float', '-i', source,
         '-f', 'f32le', '-c:a', 'pcm_f32le', '-y', destination], name)
    assert (OUT / f'{name}.log').stat().st_size == 0
    values = np.fromfile(destination, dtype='<f4').reshape(-1, 2).astype(np.float64)
    assert np.isfinite(values).all()
    return values


def metrics(x, y):
    assert x.shape == y.shape and len(x)
    rows = []
    for ch in range(2):
        a, b = x[:, ch], y[:, ch]
        signal, error = np.mean(a*a), np.mean((b-a)**2)
        rows.append({'channel': ch, 'sample_frames': len(a), 'gain': float(np.dot(a, b)/np.dot(a, a)) if signal else None,
                     'raw_snr_db': float(10*np.log10(signal/error)) if signal and error else None,
                     'input_rms': float(np.sqrt(signal)), 'output_rms': float(np.sqrt(np.mean(b*b))),
                     'error_rms': float(np.sqrt(error)), 'input_peak': float(np.max(np.abs(a))),
                     'output_peak': float(np.max(np.abs(b))), 'output_min': float(np.min(b)), 'output_max': float(np.max(b)),
                     'outside_unit': int(np.count_nonzero(np.abs(b) > 1)), 'nonfinite': int(np.count_nonzero(~np.isfinite(b)))})
    return rows


def spectrum(x, y):
    count = len(x)
    frequencies = np.fft.rfftfreq(count, 1/RATE)
    weight = np.full(len(frequencies), 2.0)
    weight[0] = 1
    if count % 2 == 0:
        weight[-1] = 1
    rows = []
    for ch in range(2):
        power = [np.abs(np.fft.rfft(z[:, ch]))**2 * weight / count**2 for z in [x, y, y-x]]
        for low, high in [(0, 16000), (16000, 20000), (20000, 24000)]:
            selected = (frequencies >= low) & ((frequencies < high) if high < 24000 else (frequencies <= high))
            signal, output, error = [float(p[selected].sum()) for p in power]
            rows.append({'channel': ch, 'hz': [low, high], 'bins': int(selected.sum()), 'input_mean_square': signal,
                         'output_mean_square': output, 'error_mean_square': error,
                         'raw_snr_db': float(10*np.log10(signal/error)) if signal and error else None})
        assert np.isclose(sum(p.sum() for p in power[:1]), np.mean(x[:, ch]**2))
    return rows


def headers(path):
    data, offset, frames = path.read_bytes(), 0, []
    while offset < len(data):
        header = int.from_bytes(data[offset:offset+4], 'big')
        assert header >> 21 == 0x7ff and (header >> 19) & 3 == 3 and (header >> 17) & 3 == 1
        bitrate = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320][(header >> 12) & 15]
        rate = [44100, 48000, 32000][(header >> 10) & 3]
        mode = ['stereo', 'joint_stereo', 'dual_channel', 'mono'][(header >> 6) & 3]
        crc_bytes = 0 if (header >> 16) & 1 else 2
        side = offset+4+crc_bytes
        main_data_begin = (data[side] << 1) | (data[side+1] >> 7)
        size = 144000*bitrate//rate + ((header >> 9) & 1)
        assert offset+size <= len(data)
        frames.append({'bitrate_kbps': bitrate, 'sample_rate': rate, 'mode': mode, 'main_data_begin': main_data_begin})
        offset += size
    assert offset == len(data)
    return {'bytes': len(data), 'mpeg_frames': len(frames), 'decoded_nominal_frames': 1152*len(frames),
            'bitrate_kbps': sorted({v['bitrate_kbps'] for v in frames}), 'sample_rate': sorted({v['sample_rate'] for v in frames}),
            'mode_counts': {mode: sum(v['mode'] == mode for v in frames) for mode in sorted({v['mode'] for v in frames})},
            'nonzero_main_data_begin_frames': sum(v['main_data_begin'] != 0 for v in frames),
            'max_main_data_begin': max(v['main_data_begin'] for v in frames), 'tag_scope': 'Every byte parsed as MPEG1 Layer3 frame; no ID3/Xing frame added.'}


def estimate_delay(x, y):
    # A zero-padded linear FFT correlation over nonperiodic PCM avoids tone aliases.
    size = 1 << ((len(x)+len(y)-1).bit_length())
    score = np.zeros(2049)
    for ch in range(2):
        corr = np.fft.irfft(np.fft.rfft(y[:, ch], size)*np.conj(np.fft.rfft(x[:, ch], size)), size)
        score += corr[:2049]
    best = int(np.argmax(score))
    ranked = np.argsort(score)[-5:][::-1]
    return {'delay_samples': best, 'search_samples': [0, 2048], 'top_correlations': [{'lag': int(i), 'dot_product': float(score[i])} for i in ranked]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verify', action='store_true', help='Check frozen input/source/output hashes and measurement arithmetic without encoding.')
    args = parser.parse_args()
    if args.verify:
        report = json.loads((OUT/'comparison.json').read_text())
        for relative, digest in report['frozen_hashes'].items():
            assert sha(OUT/relative) == digest, relative
        for row in report['cases']:
            x = np.fromfile(OUT/f"{row['name']}.s16le", dtype='<i2').reshape(-1, 2).astype(np.float64)/32768
            for codec, record in row['codecs'].items():
                y = np.fromfile(OUT/f"{row['name']}-{codec}.f32le", dtype='<f4').reshape(-1, 2).astype(np.float64)
                lag = record['delay_samples']
                lo, hi = row['steady_source_range']
                assert metrics(x[lo:hi], y[lo+lag:hi+lag]) == record['steady']
        print(f"Verified {len(report['frozen_hashes'])} artifact hashes and six independently recomputed common-region codec metrics.")
        return
    assert not (OUT/'comparison.json').exists(), 'Result exists; do not overwrite a retained acquisition.'
    candidate = json.loads(CANDIDATE.read_text())
    for relative, digest in candidate['sources_sha256'].items():
        source = ROOT/relative
        assert sha(source) == digest, relative
        frozen = OUT/'source'/(source.name if 'current-bridge.c' in relative else Path(relative).relative_to('target/kata-hardening/oct13-mp3-headroom-fixed/candidate-source'))
        frozen.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, frozen)
    shutil.copy2(CANDIDATE, OUT/'retained-candidate.json')
    shutil.copy2(RETAINED/'probe.py', OUT/'retained-probe.py')
    versions = {key: run(cmd, key) for key, cmd in [
        ('ffmpeg-version', ['ffmpeg', '-version']), ('ffmpeg-lame-help', ['ffmpeg', '-hide_banner', '-h', 'encoder=libmp3lame']),
        ('ffmpeg-mp3-help', ['ffmpeg', '-hide_banner', '-h', 'muxer=mp3']),
        ('ffmpeg-full-help', ['ffmpeg', '-hide_banner', '-h', 'full']), ('uv-version', ['uv', '--version']),
        ('libmp3lame-package', ['dpkg-query', '-W', 'libmp3lame0'])]}
    versions.update(python=sys.version, numpy=np.__version__)
    source_rows = {row['name']: row for row in candidate['fixtures']}
    for name in CASES:
        for suffix in ['.s16le', '.mp3']:
            source = RETAINED/(name+suffix)
            assert sha(source) == source_rows[name]['artifacts'][source.name]['sha256']
            shutil.copy2(source, OUT/(name+('.s16le' if suffix == '.s16le' else '-shine.mp3')))
    # Separate alignment controls; no periodic fixture determines the fixed reference delay.
    rng = np.random.Generator(np.random.PCG64(0xEF22A6))
    calibration = np.zeros((N, 2), dtype='<i2')
    calibration[8192:90000] = rng.integers(-8192, 8193, size=(90000-8192, 2), dtype=np.int16)
    calibration.tofile(OUT/'delay-nonperiodic.s16le')
    impulse = np.zeros((N, 2), dtype='<i2')
    impulse[12000] = [16000, -16000]
    impulse[50000] = [-12000, 12000]
    impulse.tofile(OUT/'delay-impulses.s16le')
    delay = {}
    for kind, pcm in [('nonperiodic', calibration), ('impulses', impulse)]:
        name = f'delay-{kind}'
        encode(OUT/f'{name}.s16le', OUT/f'{name}.mp3', name+'-encode')
        y = decode(OUT/f'{name}.mp3', OUT/f'{name}.f32le', name+'-decode')
        delay[kind] = estimate_delay(pcm.astype(np.float64)/32768, y)
        delay[kind].update(input_frames=len(pcm), decoded_frames=len(y), headers=headers(OUT/f'{name}.mp3'))
        if kind == 'impulses':
            delay[kind]['peak_offsets'] = [[int(np.argmax(np.abs(y[p:p+2049, ch]))) for ch in range(2)] for p in [12000, 50000]]
    lame_lag = delay['nonperiodic']['delay_samples']
    assert delay['impulses']['delay_samples'] == lame_lag
    assert delay['impulses']['peak_offsets'] == [[lame_lag, lame_lag], [lame_lag, lame_lag]]
    rows = []
    for name in CASES:
        pcm = np.fromfile(OUT/f'{name}.s16le', dtype='<i2').reshape(-1, 2)
        x = pcm.astype(np.float64)/32768
        assert len(x) == N
        encode(OUT/f'{name}.s16le', OUT/f'{name}-lame.mp3', name+'-lame-encode')
        decoded = {codec: decode(OUT/f'{name}-{codec}.mp3', OUT/f'{name}-{codec}.f32le', name+'-'+codec+'-decode') for codec in ['shine', 'lame']}
        lags = {'shine': 1057, 'lame': lame_lag}
        stop = min(N, *(len(y)-lags[codec] for codec, y in decoded.items()))
        row = {'name': name, 'source_sample_frames': N, 'common_source_range': [0, stop], 'steady_source_range': [TRIM, stop-TRIM], 'codecs': {}}
        for codec, y in decoded.items():
            lag = lags[codec]
            rec = {'delay_samples': lag, 'decoded_sample_frames': len(y), 'aligned_available_source_frames': min(N, len(y)-lag),
                   'omitted_source_tail_frames': max(0, N-(len(y)-lag)), 'decoded_padding_after_source_frames': max(0, len(y)-lag-N),
                   'headers': headers(OUT/f'{name}-{codec}.mp3'), 'aligned_common': metrics(x[:stop], y[lag:lag+stop]),
                   'steady': metrics(x[TRIM:stop-TRIM], y[lag+TRIM:lag+stop-TRIM]),
                   'steady_spectrum_bands': spectrum(x[TRIM:stop-TRIM], y[lag+TRIM:lag+stop-TRIM]),
                   'all_decoded': metrics(np.zeros_like(y), y)}
            if name.startswith('impulse'):
                rec['event_regions'] = {label: {'source_range': [lo, hi], 'metrics': metrics(x[lo:hi], y[lo+lag:hi+lag])} for label, lo, hi in [
                    ('isolated_impulse', 7000, 17000), ('first_gate', 25000, 39000), ('second_gate', 55000, 68000),
                    ('first_gate_preceding_silence', 28017, 30017), ('second_gate_preceding_silence', 58029, 60029)]}
                rec['isolated_impulse_peak_offset'] = [int(np.argmax(np.abs(y[12000:14049, ch]))) for ch in range(2)]
                assert rec['isolated_impulse_peak_offset'] == [lag, lag]
            row['codecs'][codec] = rec
        rows.append(row)
        print(name, 'steady SNR', {c: [round(v['raw_snr_db'], 3) for v in r['steady']] for c, r in row['codecs'].items()}, flush=True)
    for relative, digest in candidate['sources_sha256'].items():
        assert sha(ROOT/relative) == digest, 'Retained source changed during acquisition'
    frozen_hashes = {str(p.relative_to(OUT)): sha(p) for p in sorted(OUT.rglob('*')) if p.is_file() and p.name not in ['comparison.json', 'commands.json', 'run.log']}
    report = {'scope': 'Throwaway offline reference comparison; no production change, playback, subjective acceptance or replacement recommendation.',
              'source_precision': 'Exact retained interleaved signed16 PCM divided by32768 into float64; FFmpeg decodes raw MP3 with mp3float to f32le, analyzed as float64. No fitted gain correction.',
              'source_provenance': {'candidate_sha256': sha(CANDIDATE), 'recorded_candidate_git_head': candidate['versions']['git_head'], 'task_repo_head_supplied_by_parent': '369af00'},
              'versions': versions, 'sample_rate': RATE, 'channels': 2, 'bitrate_kbps': 320,
              'reference_defaults': {'reservoir': True, 'joint_stereo': True, 'compression_level': -1, 'cutoff': 0,
                  'scope': 'FFmpeg help defaults; library-selected quality and lowpass frequency are not exposed by these logs and are not assumed. Stream mode/reservoir are parsed from every actual frame.'},
              'alignment': delay, 'cases': rows,
              'spectrum_definition': 'Rectangular window over exactly common steady source range; rfft squared magnitude/N^2; double positive-frequency bins except DC and even-N Nyquist. Sum equals mean-square energy (Parseval). Bands lower inclusive/upper exclusive except final Nyquist inclusive. Leakage is retained; no gain correction or weighting.',
              'limits': ['LAME keeps default psychoacoustic, joint-stereo and reservoir behavior; Shine does not have the same algorithm. This is a numerical reference, not a drop-in ESP codec.',
                         'Raw MP3 no Xing/ID3; LAME flush adds frames while retained live Shine has no finite drain. Fidelity uses equal common source prefix, not padded LAME tail.',
                         'White-noise SNR and out-of-range float samples alone do not prove audible quality or a structural encoder defect.',
                         'No audio playback, music transparency, analog path, target throughput, network or indefinite input bound is established.'],
              'frozen_hashes': frozen_hashes, 'commands': COMMANDS}
    (OUT/'comparison.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
