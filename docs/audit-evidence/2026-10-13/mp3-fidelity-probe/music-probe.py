# ABOUTME: Compares frozen production MP3 with libmp3lame on music and fade controls.
# ABOUTME: Writes offline float measurements and hashes; never plays or publishes audio.
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import subprocess
import sys

import numpy as np

RATE, BLOCK, PAD = 48000, 1152, 2304
UNITS = 'bitstream huffman l3bitstream l3loop l3mdct l3subband layer3 reservoir tables'.split()
COMMANDS = []


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(argv, name, out, root):
    log = out / (name + '.log')
    entry = {'argv': list(map(str, argv)), 'log': log.name}
    COMMANDS.append(entry)
    with log.open('wb') as stream:
        result = subprocess.run(argv, cwd=root, stdout=stream, stderr=subprocess.STDOUT, timeout=90)
    entry.update(exit_code=result.returncode, log_bytes=log.stat().st_size, log_sha256=sha(log))
    (out / 'commands-local.json').write_text(json.dumps(COMMANDS, indent=2) + '\n')
    if result.returncode:
        raise RuntimeError(f'{name}: exit {result.returncode}; see {log}')
    return log.read_text()


def db(numerator, denominator):
    return float(10 * np.log10(numerator / denominator)) if numerator > 0 and denominator > 0 else None


def metrics(x, y):
    assert x.shape == y.shape and len(x)
    rows = []
    for ch in range(2):
        a, b = x[:, ch], y[:, ch]
        p, q = np.mean(a*a), np.mean(b*b)
        e = np.mean((b-a)**2)
        rows.append({'channel': ch, 'frames': len(a), 'input_rms': float(np.sqrt(p)),
                     'decoded_rms': float(np.sqrt(q)), 'error_rms': float(np.sqrt(e)),
                     'snr_db': db(p, e), 'error_dbfs': db(e, 1),
                     'gain': float(np.dot(a, b)/np.dot(a, a)) if p else None})
    return rows


def amplitude(y):
    return [{'channel': ch, 'frames': len(y), 'peak': float(np.max(np.abs(y[:, ch]))),
             'outside_unit_interval': int(np.count_nonzero(np.abs(y[:, ch]) > 1)),
             'at_or_outside_unit_interval': int(np.count_nonzero(np.abs(y[:, ch]) >= 1)),
             'nonfinite': int(np.count_nonzero(~np.isfinite(y[:, ch])))} for ch in range(2)]


def lag_scores(x, y, start, stop, lags):
    # The same source window for every candidate avoids energy/length bias.
    a = x[start:stop]
    energy = np.sum(a*a)
    scores = []
    for lag in lags:
        b = y[start+lag:stop+lag]
        assert b.shape == a.shape
        denominator = np.sqrt(energy*np.sum(b*b))
        scores.append(float(np.sum(a*b) / denominator) if denominator else None)
    return scores


def spectral(x, y):
    # Average Hann-windowed error/reference power; no gain fitting or weighting.
    size, step = 4096, 2048
    window = np.hanning(size)[:, None]
    xp, ep = np.zeros((size//2+1, 2)), np.zeros((size//2+1, 2))
    count = 0
    for start in range(0, len(x)-size+1, step):
        a, e = x[start:start+size]*window, (y[start:start+size]-x[start:start+size])*window
        xp += np.abs(np.fft.rfft(a, axis=0))**2
        ep += np.abs(np.fft.rfft(e, axis=0))**2
        count += 1
    frequency = np.fft.rfftfreq(size, 1/RATE)
    bands = []
    for low, high in [(0, 200), (200, 2000), (2000, 8000), (8000, 16000), (16000, 24001)]:
        select = (frequency >= low) & (frequency < high)
        bands.append({'low_hz': low, 'high_hz_exclusive': high,
                      'snr_db': [db(np.sum(xp[select, ch]), np.sum(ep[select, ch])) for ch in range(2)],
                      'source_power_fraction': [float(np.sum(xp[select, ch])/np.sum(xp[:, ch])) for ch in range(2)]})
    return {'window': 'Hann', 'fft_frames': size, 'hop_frames': step, 'windows': count, 'bands': bands}


def quantize(x):
    pcm = np.rint(x*32768)
    assert np.max(pcm) <= 32767 and np.min(pcm) >= -32768
    return pcm.astype('<i2')


def main():
    parser = argparse.ArgumentParser(description='Throwaway offline MP3 fidelity probe; no audio playback.')
    parser.add_argument('--repo', type=Path, required=True, help='Repository containing production mp3 sources.')
    parser.add_argument('--music-pcm', type=Path, help='Optional authorized whole-block48kHz stereo signed16LE fixture; path stays in local command receipts.')
    parser.add_argument('--out', type=Path, required=True, help='Fresh child directory for generated artifacts.')
    args = parser.parse_args()
    root, out = args.repo.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    live = [root/'mp3/src/bridge.c', *sorted((root/'mp3/vendor/shine').glob('*.c')), *sorted((root/'mp3/vendor/shine').glob('*.h'))]
    before = {str(p.relative_to(root)): sha(p) for p in live}
    snapshot = out/'source'
    for p in live:
        target = snapshot/p.relative_to(root)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(p, target)
    runner = root/'docs/audit-evidence/2026-10-13/mp3-gain-probe/current-bridge.c'
    shutil.copyfile(runner, snapshot/'current-bridge.c')
    versions = {'python': sys.version, 'numpy': np.__version__,
                'git_head': run(['git', 'rev-parse', 'HEAD'], 'git-head', out, root).strip(),
                'cc': run(['cc', '--version'], 'cc-version', out, root),
                'ffmpeg': run(['ffmpeg', '-version'], 'ffmpeg-version', out, root)}
    vendor = snapshot/'mp3/vendor/shine'
    encoder = out/'actual-bridge'
    run(['cc', '-O3', '-fsigned-char', '-Wall', '-Wextra', '-Werror', '-I'+str(vendor),
         snapshot/'current-bridge.c', snapshot/'mp3/src/bridge.c', *[vendor/(u+'.c') for u in UNITS], '-lm', '-o', encoder], 'compile', out, root)
    assert not (out/'compile.log').stat().st_size
    n = BLOCK*420
    t = np.arange(n)/RATE
    tones = np.column_stack((.5*np.sin(2*np.pi*997*t), .5*np.sin(2*np.pi*1703*t)))
    # Independent seeded nonperiodic broadband control, used only to establish delay.
    calibration = quantize(np.random.Generator(np.random.PCG64(20261013)).uniform(-.25, .25, (BLOCK*96, 2)))
    envelope = np.zeros(n)
    # Fade up from -90 to -6dBFS and back, with genuine silent endpoints.
    mid = n//2
    envelope[4608:mid] = np.geomspace(10**(-90/20), .5, mid-4608)
    envelope[mid:n-4608] = np.geomspace(.5, 10**(-90/20), n-4608-mid)
    fade = np.column_stack((np.sin(2*np.pi*997*t), np.sin(2*np.pi*1703*t))) * envelope[:, None]
    inputs = {'alignment': calibration, 'tones': quantize(tones), 'fade': quantize(fade)}
    music_hash = None
    if args.music_pcm:
        music_hash = sha(args.music_pcm)
        pcm = np.fromfile(args.music_pcm, dtype='<i2').reshape(-1, 2)
        assert len(pcm) == n, 'Bounded10.08s music fixture required'
        inputs['recorded_music'] = pcm
    rows, delays, decoded = [], {}, {}
    for name, pcm in inputs.items():
        assert len(pcm) % BLOCK == 0
        x = pcm.astype(np.float64)/32768
        padded = np.concatenate((pcm, np.zeros((PAD, 2), dtype='<i2')))
        raw = out/(name+'.s16le')
        padded.tofile(raw)
        for codec in ('production', 'lame'):
            mp3, floats = out/(name+'-'+codec+'.mp3'), out/(name+'-'+codec+'.f32le')
            if codec == 'production':
                run([encoder, raw, mp3], name+'-'+codec+'-encode', out, root)
            else:
                run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 's16le', '-ar', str(RATE), '-ac', '2', '-i', raw,
                     '-c:a', 'libmp3lame', '-b:a', '320k', '-write_xing', '0', '-id3v2_version', '0', mp3], name+'-'+codec+'-encode', out, root)
            run(['ffmpeg', '-nostdin', '-v', 'error', '-c:a', 'mp3float', '-i', mp3, '-c:a', 'pcm_f32le', '-f', 'f32le', floats], name+'-'+codec+'-decode', out, root)
            assert not (out/(name+'-'+codec+'-encode.log')).stat().st_size
            assert not (out/(name+'-'+codec+'-decode.log')).stat().st_size
            y = np.fromfile(floats, dtype='<f4').reshape(-1, 2).astype(np.float64)
            assert np.all(np.isfinite(y))
            blob = mp3.read_bytes()
            assert len(blob) % 960 == 0 and len(y) == len(blob)//960*BLOCK
            assert all(blob[i:i+3] == bytes([255, 251, 228]) for i in range(0, len(blob), 960))
            if name == 'alignment':
                candidates = list(range(0, 1800))
                scores = lag_scores(x, y, 4608, 100000, candidates)
                assert all(score is not None for score in scores)
                lag = candidates[int(np.argmax(scores))]
                delays[codec] = {'samples': lag, 'best_correlation': max(scores),
                                 'next_best_correlation': sorted(scores)[-2], 'window': [4608, 100000],
                                 'source_hash': hashlib.sha256(pcm.tobytes()).hexdigest()}
            lag = delays[codec]['samples']
            assert len(y)-lag >= len(x)
            aligned = y[lag:lag+len(x)]
            row = {'fixture': name, 'codec': codec, 'source_frames': len(x), 'padding_frames': PAD,
                   'encoder_input_frames': len(padded), 'encoded_mp3_frames': len(blob)//960,
                   'decoded_frames': len(y), 'fixed_delay_frames': lag, 'aligned_compared_frames': len(x),
                   'decoded_frames_after_source': len(y)-lag-len(x), 'unavailable_tail_without_padding_frames': lag if codec == 'production' else None,
                   'source_pcm_sha256': hashlib.sha256(pcm.tobytes()).hexdigest(),
                   'padded_pcm_sha256': sha(raw), 'mp3_sha256': sha(mp3), 'decoded_float_sha256': sha(floats),
                   'input_amplitude': amplitude(x), 'all_decoded_amplitude': amplitude(y),
                   'aligned_amplitude': amplitude(aligned), 'aligned_metrics': metrics(x, aligned),
                   'steady_metrics': metrics(x[4608:-4608], aligned[4608:-4608]),
                   'spectral_error': spectral(x, aligned),
                   'segments': [{'source_start': start, 'source_stop': min(start+24000, len(x)),
                                 'metrics': metrics(x[start:start+24000], aligned[start:start+24000])} for start in range(0, len(x), 24000)]}
            if name == 'recorded_music':
                row['lag_drift_checks'] = []
                for start in (4608, len(x)//2-12000, len(x)-28608):
                    candidates = list(range(lag-8, lag+9))
                    scores = lag_scores(x, y, start, start+24000, candidates)
                    valid = all(score is not None for score in scores)
                    row['lag_drift_checks'].append({'source_start': start, 'frames': 24000,
                                                   'status': 'measured' if valid else 'unavailable_zero_energy',
                                                   'best_offset_from_fixed_delay': candidates[int(np.argmax(scores))]-lag if valid else None,
                                                   'scores_for_offsets_minus8_through_plus8': scores})
            rows.append(row)
            decoded[(name, codec)] = aligned
    after = {str(p.relative_to(root)): sha(p) for p in live}
    assert before == after, 'Production sources changed; acquisition rejected'
    assert (not args.music_pcm) or music_hash == sha(args.music_pcm), 'Music fixture changed'
    report = {'scope': 'Throwaway offline bounded characterization; no playback or perceptual acceptance.',
              'rate_hz': RATE, 'channels': 2, 'bitrate_kbps': 320, 'source_hashes_before': before,
              'source_hashes_after': after, 'wrapper_sha256': sha(runner), 'encoder_sha256': sha(encoder),
              'script_sha256': sha(__file__), 'versions': versions, 'delays': delays, 'fixtures': rows,
              'settings': {'production': 'Frozen actual bridge: stereo, reservoir disabled, fixed320kbps; corrected Shine tables.',
                           'lame': 'FFmpeg libmp3lame320kbps; default joint_stereo=true, reservoir=true, abr=false; cutoff0 (automatic). No explicit quality option. Raw MP3: write_xing0,id3v2_version0; library flush included.'},
              'limits': ['Actual quantized signed16 source divided by32768; gain measured but never compensated.',
                         'One nonperiodic calibration establishes each fixed delay. Local music searches diagnose drift and do not realign segments.',
                         'Two zero blocks are probe-only input. Original source, added zeros, decoder delay, library flush and trailing decoded frames reported separately.',
                         'The production bridge closes without draining. Padded fidelity does not prove production finite-stream tail retention.',
                         'Float peaks above1 indicate downstream clipping risk, not automatically integer overflow.',
                         'FFT-band and segment SNR are unweighted objective diagnostics; they cannot establish audibility or transparency.',
                         'One10.08s private music excerpt cannot represent every musical style; no subjective listening performed.',
                         'LAME defaults (stereo tools, reservoir and automatic lowpass/quality) differ; comparison is a reference configuration, not isolation of one codec mechanism.']}
    (out/'report.json').write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    print(json.dumps({'delays': delays, 'rows': [{'fixture': r['fixture'], 'codec': r['codec'],
          'snr_db': [c['snr_db'] for c in r['steady_metrics']], 'gain': [c['gain'] for c in r['steady_metrics']],
          'peak': [c['peak'] for c in r['all_decoded_amplitude']]} for r in rows]}, indent=2))


if __name__ == '__main__':
    main()
