# ABOUTME: Adds one constrained LAME music comparison and matching delay calibration.
# ABOUTME: Disables stereo sharing and bit reservoir while retaining all other defaults.
from pathlib import Path
import argparse
import importlib.util
import json

import numpy as np


def main():
    parser = argparse.ArgumentParser(description='Bounded supplemental stereo/no-reservoir LAME reference; no playback.')
    parser.add_argument('--run', type=Path, required=True, help='Retained primary acquisition.')
    parser.add_argument('--out', type=Path, required=True, help='Fresh output directory.')
    parser.add_argument('--repo', type=Path, required=True, help='Repository for source hash verification.')
    args = parser.parse_args()
    run, out, root = args.run.resolve(), args.out.resolve(), args.repo.resolve()
    out.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('fidelity_probe', Path(__file__).with_name('probe.py'))
    probe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(probe)
    primary = json.loads((run/'report.json').read_text())
    hashes = primary['source_hashes_before']
    assert {p: probe.sha(root/p) for p in hashes} == hashes
    rows, lag = [], None
    for name in ['alignment', 'recorded_music']:
        original_row = next(r for r in primary['fixtures'] if r['fixture'] == name and r['codec'] == 'lame')
        raw = run/(name+'.s16le')
        assert probe.sha(raw) == original_row['padded_pcm_sha256']
        pcm = np.fromfile(raw, dtype='<i2').reshape(-1, 2)
        x = pcm[:original_row['source_frames']].astype(np.float64)/32768
        mp3, floats = out/(name+'.mp3'), out/(name+'.f32le')
        probe.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 's16le', '-ar', '48000', '-ac', '2', '-i', raw,
                   '-c:a', 'libmp3lame', '-b:a', '320k', '-joint_stereo', '0', '-reservoir', '0',
                   '-write_xing', '0', '-id3v2_version', '0', mp3], name+'-encode', out, root)
        probe.run(['ffmpeg', '-nostdin', '-v', 'error', '-c:a', 'mp3float', '-i', mp3,
                   '-c:a', 'pcm_f32le', '-f', 'f32le', floats], name+'-decode', out, root)
        assert not (out/(name+'-encode.log')).stat().st_size
        assert not (out/(name+'-decode.log')).stat().st_size
        blob = mp3.read_bytes()
        assert len(blob) % 960 == 0
        assert all(blob[i:i+3] == bytes([255, 251, 228]) and blob[i+3] >> 6 == 0 and
                   blob[i+4] == 0 and blob[i+5] & 128 == 0 for i in range(0, len(blob), 960))
        y = np.fromfile(floats, dtype='<f4').reshape(-1, 2).astype(np.float64)
        assert np.all(np.isfinite(y)) and len(y) == len(blob)//960*1152
        if name == 'alignment':
            scores = probe.lag_scores(x, y, 4608, 100000, range(1800))
            assert all(v is not None for v in scores)
            lag = int(np.argmax(scores))
            calibration = {'samples': lag, 'best_correlation': max(scores),
                           'next_best_correlation': sorted(scores)[-2], 'source_window': [4608, 100000]}
        assert lag is not None and len(y)-lag >= len(x)
        aligned = y[lag:lag+len(x)]
        rows.append({'fixture': name, 'source_pcm_sha256': original_row['source_pcm_sha256'],
                     'padded_pcm_sha256': probe.sha(raw), 'mp3_sha256': probe.sha(mp3),
                     'decoded_float_sha256': probe.sha(floats), 'source_frames': len(x), 'padding_frames': 2304,
                     'decoded_frames': len(y), 'encoded_mp3_frames': len(blob)//960, 'fixed_delay_frames': lag,
                     'decoded_frames_after_source': len(y)-lag-len(x), 'all_decoded_amplitude': probe.amplitude(y),
                     'aligned_metrics': probe.metrics(x, aligned), 'steady_metrics': probe.metrics(x[4608:-4608], aligned[4608:-4608]),
                     'spectral_error': probe.spectral(x, aligned),
                     'verified_stereo_headers_and_zero_main_data_begin': True})
    assert {p: probe.sha(root/p) for p in hashes} == hashes
    result = {'primary_report_sha256': probe.sha(run/'report.json'), 'script_sha256': probe.sha(__file__),
              'source_hashes_before_and_after': hashes, 'rate_hz': 48000, 'channels': 2, 'bitrate_kbps': 320,
              'configuration': 'libmp3lame with joint_stereo=false,reservoir=false; all other options/defaults identical to primary LAME reference, including automatic cutoff and unspecified quality. Raw MP3 with no Xing/ID3; encoder flush retained.',
              'fixed_delay_calibration': calibration, 'fixtures': rows,
              'limits': 'One supplemental configuration, not a tuning sweep. Same low-level dualmono excerpt; no listening acceptance. Spectral bin sums use primary report convention; padding is probe-only. No production changes.'}
    (out/'report.json').write_text(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps({'delay': calibration, 'music': rows[-1]['steady_metrics'],
                      'music_amplitude': rows[-1]['all_decoded_amplitude']}, indent=2))


if __name__ == '__main__':
    main()
