# ABOUTME: Verifies retained fidelity artifacts and measures fade-level errors offline.
# ABOUTME: Adds explicit FFT conventions, music limits and red-acquisition provenance.
from pathlib import Path
import argparse
import importlib.util
import json

import numpy as np


def main():
    parser = argparse.ArgumentParser(description='Verify and summarize existing offline MP3 probe artifacts without reencoding.')
    parser.add_argument('--run', type=Path, required=True, help='Completed probe output directory.')
    parser.add_argument('--repo', type=Path, required=True, help='Repository whose unchanged sources are verified.')
    args = parser.parse_args()
    run, root = args.run.resolve(), args.repo.resolve()
    spec = importlib.util.spec_from_file_location('fidelity_probe', Path(__file__).with_name('probe.py'))
    probe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(probe)
    report_path = run/'report.json'
    report = json.loads(report_path.read_text())
    assert report['source_hashes_before'] == report['source_hashes_after']
    for path, digest in report['source_hashes_before'].items():
        assert probe.sha(root/path) == probe.sha(run/'source'/path) == digest
    assert probe.sha(Path(__file__).with_name('probe.py')) == report['script_sha256']
    assert probe.sha(run/'actual-bridge') == report['encoder_sha256']
    commands = json.loads((run/'commands-local.json').read_text())
    for command in commands:
        log = run/command['log']
        assert command['exit_code'] == 0 and probe.sha(log) == command['log_sha256']
        assert log.stat().st_size == command['log_bytes']
        if command['log'].endswith(('-encode.log', '-decode.log')) or command['log'] == 'compile.log':
            assert log.stat().st_size == 0
    originals, aligned = {}, {}
    for row in report['fixtures']:
        name, codec = row['fixture'], row['codec']
        raw = run/(name+'.s16le')
        pcm = np.fromfile(raw, dtype='<i2').reshape(-1, 2)
        original = pcm[:row['source_frames']]
        assert probe.sha(raw) == row['padded_pcm_sha256']
        assert probe.hashlib.sha256(original.tobytes()).hexdigest() == row['source_pcm_sha256']
        assert np.all(pcm[row['source_frames']:] == 0)
        for suffix, key in [('.mp3', 'mp3_sha256'), ('.f32le', 'decoded_float_sha256')]:
            assert probe.sha(run/(name+'-'+codec+suffix)) == row[key]
        y = np.fromfile(run/(name+'-'+codec+'.f32le'), dtype='<f4').reshape(-1, 2).astype(np.float64)
        lag = row['fixed_delay_frames']
        x, y = original.astype(np.float64)/32768, y[lag:lag+len(original)]
        assert probe.metrics(x, y) == row['aligned_metrics']
        assert probe.amplitude(y) == row['aligned_amplitude']
        originals[name], aligned[(name, codec)] = x, y
    music = originals.get('recorded_music')
    music_limits = None
    if music is not None:
        active = np.flatnonzero(np.any(music != 0, axis=1))
        music_limits = {'channel_samples_exactly_identical': bool(np.array_equal(music[:, 0], music[:, 1])),
                        'nonzero_source_start': int(active[0]), 'nonzero_source_stop_exclusive': int(active[-1]+1),
                        'zero_stereo_frames': int(np.count_nonzero(np.all(music == 0, axis=1))),
                        'source_peak': float(np.max(np.abs(music))), 'source_frames': len(music)}
    x = originals['fade']
    n, mid = len(x), len(x)//2
    envelope = np.zeros(n)
    envelope[4608:mid] = np.geomspace(10**(-90/20), .5, mid-4608)
    envelope[mid:n-4608] = np.geomspace(.5, 10**(-90/20), n-4608-mid)
    levels = np.full(n, -np.inf)
    positive = envelope > 0
    levels[positive] = 20*np.log10(envelope[positive])
    fade_rows = []
    for direction, direction_mask in [('rising', np.arange(n) < mid), ('falling', np.arange(n) >= mid)]:
        for low, high in [(-90, -75), (-75, -60), (-60, -45), (-45, -30), (-30, -15), (-15, -6)]:
            select = direction_mask & (levels >= low) & (levels < high)
            bounds = np.flatnonzero(select)
            for codec in ['production', 'lame']:
                y = aligned[('fade', codec)]
                fade_rows.append({'direction': direction, 'envelope_dbfs_range': [low, high],
                                  'codec': codec, 'source_start': int(bounds[0]), 'source_stop_exclusive': int(bounds[-1]+1),
                                  'metrics': probe.metrics(x[select], y[select])})
    silence = []
    for codec in ['production', 'lame']:
        silence.append({'codec': codec, 'source_frames': int(np.count_nonzero(~positive)),
                        'metrics': probe.metrics(x[~positive], aligned[('fade', codec)][~positive]),
                        'amplitude': probe.amplitude(aligned[('fade', codec)][~positive])})
    notes = {'report_sha256': probe.sha(report_path), 'summarizer_sha256': probe.sha(__file__),
             'verified_sources_and_artifact_hashes': True, 'verified_all_codec_logs_empty': True,
             'verified_aligned_metrics_exactly_recomputed': True, 'music_scope': music_limits,
             'spectral_convention': 'Hann-windowed one-sided rFFT magnitude-squared sums, without doubling interior bins. source_power_fraction denotes that bin-sum fraction, not a strictly weighted physical band-energy fraction. Error/reference SNR uses the same convention; no perceptual weighting.',
             'fade_by_nominal_envelope': fade_rows, 'fade_zero_envelope': silence,
             'failed_acquisition': 'run-01 encoded/decoded but report serialization failed on NaN from a zero-energy music timing window. Retained unchanged; run-02 reports null/unavailable_zero_energy for the same window.',
             'measurement_controls': ['Identity gives zero error and unity gain.', 'Doubled decoded gain gives measured gain2 and rawSNR0dB.',
                                      'Independent synthetic137sample delay is recovered.', 'Out-of-range floats are counted without integer clipping.',
                                      'Silent correlation windows return null; no warnings or false lag claim.']}
    (run/'measurement-notes.json').write_text(json.dumps(notes, indent=2, allow_nan=False)+'\n')
    print(json.dumps({'verified': True, 'music_scope': music_limits, 'fade_rows': len(fade_rows),
                      'report_sha256': notes['report_sha256']}, indent=2))


if __name__ == '__main__':
    main()
