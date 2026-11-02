# ABOUTME: Independently verifies retained music fidelity counts, hashes and raw error.
# ABOUTME: Reads private local fixtures without playing audio or publishing their content.
import hashlib
import json
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
MUSIC = OUT.with_name('oct13-mp3-fidelity-music') / 'run-02'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
report = json.loads((MUSIC / 'report.json').read_text())
assert report['source_hashes_before'] == report['source_hashes_after']
for name, expected in report['source_hashes_before'].items():
    assert sha(ROOT / name) == expected, name
rows = []
for row in report['fixtures']:
    name, codec = row['fixture'], row['codec']
    raw = MUSIC / (name + '.s16le')
    encoded = MUSIC / (name + '-' + codec + '.mp3')
    decoded = MUSIC / (name + '-' + codec + '.f32le')
    assert sha(raw) == row['padded_pcm_sha256']
    assert sha(encoded) == row['mp3_sha256']
    assert sha(decoded) == row['decoded_float_sha256']
    n, lag = row['source_frames'], row['fixed_delay_frames']
    pcm = np.fromfile(raw, dtype='<i2').reshape(-1, 2)
    assert hashlib.sha256(pcm[:n].tobytes()).hexdigest() == row['source_pcm_sha256']
    assert len(pcm) == n + row['padding_frames'] and not pcm[n:].any()
    x = pcm[:n].astype(float) / 32768
    y = np.fromfile(decoded, dtype='<f4').reshape(-1, 2).astype(float)
    assert len(y) == row['decoded_frames'] == encoded.stat().st_size // 960 * 1152
    assert np.isfinite(y).all()
    assert len(y) - lag - n == row['decoded_frames_after_source']
    actual = []
    for ch in range(2):
        a, b = x[4608:-4608, ch], y[lag+4608:lag+n-4608, ch]
        snr = float(10 * np.log10(np.sum(a*a) / np.sum((b-a)**2)))
        gain = float(np.sum(a*b) / np.sum(a*a))
        np.testing.assert_allclose(snr, row['steady_metrics'][ch]['snr_db'], atol=1e-9, rtol=0)
        np.testing.assert_allclose(gain, row['steady_metrics'][ch]['gain'], atol=1e-12, rtol=0)
        assert int(np.count_nonzero(np.abs(y[:, ch]) > 1)) == row['all_decoded_amplitude'][ch]['outside_unit_interval']
        actual.append({'channel': ch, 'raw_snr_db': snr, 'gain': gain})
    rows.append({'fixture': name, 'codec': codec, 'original_frames': n,
                 'decoded_frames': len(y), 'metrics': actual})
receipt = {'source_receipt_sha256': sha(MUSIC/'report.json'),
           'script_sha256': sha(Path(__file__)), 'verified_rows': rows,
           'scope': 'Fresh parent check of actual source/output hashes, counts and uncompensated steady metrics.'}
(OUT/'verification.json').write_text(json.dumps(receipt, indent=2)+'\n')
print('Verified', len(rows), 'actual codec outputs and current source hashes')
