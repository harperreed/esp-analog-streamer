# ABOUTME: Reproducible throwaway quantization experiment with real FLAC and FFmpeg.
# ABOUTME: Measures synthetic digital error and size without changing production policy.
import hashlib
import json
import pathlib
import subprocess
import time
import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent
N = 4096 * 12
FS = 48000
SEED = 20261012
rng = np.random.default_rng(SEED)
t = np.arange(N) / FS
# Bin-coherent tone permits inspecting error lines without leakage/window correction.
freq = 997 * FS / N
sources = {
    'silence': np.zeros((N, 2), dtype=np.int64),
    'tone_minus90_dbfs': np.column_stack([np.sin(2*np.pi*freq*t), np.cos(2*np.pi*freq*t)]) * (2**23 * 10**(-90/20)),
    'fade_minus60_to_minus120_dbfs': np.column_stack([np.sin(2*np.pi*freq*t), np.cos(2*np.pi*freq*t)]) * (2**23 * 10**(np.linspace(-60, -120, N)[:, None]/20)),
    'dc_quarter_lsb': np.tile([64, -64], (N, 1)),
    'noise_minus100_dbfs_rms': rng.normal(0, 2**23 * 10**(-100/20), (N, 2)),
    'fullscale_extremes': np.resize(np.array([-(2**31), 2**31-1, -(2**23), 2**23-1, -257, -256, -255, -129, -128, -127, -1, 0, 1, 127, 128, 129, 255, 256, 257]), (N, 2)),
}
def quantize(x, mode):
    if mode == 'truncation':
        return np.clip(x >> 8, -32768, 32767)
    if mode == 'rounded_saturated':
        # Explicit tie convention: half-way rounds towards positive infinity.
        return np.clip((x + 128) >> 8, -32768, 32767)
    # Two independent uniform(-.5,.5) draws sum to TPDF +/- 1 output LSB.
    r = np.random.default_rng(SEED)
    dither = r.random(x.shape) + r.random(x.shape) - 1
    return np.clip(np.floor(x/256 + dither + .5), -32768, 32767).astype(np.int64)

# Independent exact vector oracles catch rounding, sign, and saturation mistakes.
x = np.array([-2**31, -257, -256, -255, -129, -128, -127, -1, 0, 127, 128, 255, 256, 2**31-1], dtype=np.int64)
assert quantize(x, 'truncation').tolist() == [-32768,-2,-1,-1,-1,-1,-1,-1,0,0,0,0,1,32767]
assert quantize(x, 'rounded_saturated').tolist() == [-32768,-1,-1,-1,-1,0,0,0,0,0,1,1,1,32767]
y = quantize(np.zeros((N, 2), dtype=np.int64), 'tpdf_rounded_saturated')
assert set(np.unique(y)) == {-1,0,1}
assert abs(y.mean()) < .01 and .23 < y.var() < .27
assert np.array_equal(y, quantize(np.zeros_like(y), 'tpdf_rounded_saturated'))
results = []
for name, source in sources.items():
    source = np.rint(source).astype(np.int64)
    source.astype('<i4').tofile(ROOT / f'{name}.source.i32le')
    for mode in ['truncation', 'rounded_saturated', 'tpdf_rounded_saturated']:
        start = time.perf_counter_ns()
        q = quantize(source, mode)
        quant_ns = time.perf_counter_ns() - start
        # Feed candidate 16-bit output left-shifted into unchanged production encoder.
        # Its arithmetic shift then preserves candidate samples exactly.
        capture = source if mode == 'truncation' else q * 256
        stem = ROOT / f'{name}.{mode}'
        capture.astype('<i4').tofile(str(stem) + '.i32le')
        encoded = subprocess.run([str(ROOT/'encode'), str(stem)+'.i32le', str(stem)+'.flac'], capture_output=True, check=True)
        assert not encoded.stderr, encoded.stderr
        decoded = subprocess.run(['ffmpeg','-nostdin','-v','error','-i',str(stem)+'.flac','-f','s16le','-'], capture_output=True, check=True)
        assert not decoded.stderr, decoded.stderr
        assert np.array_equal(np.frombuffer(decoded.stdout, '<i2').reshape(q.shape), q), (name,mode)
        error = q.astype(float) - source/256
        # Out-of-range inputs test saturation; don't mislabel clipping as quantizer noise.
        valid = (source >= -2**23) & (source <= 2**23-1)
        ev = error[valid]
        spectrum = np.abs(np.fft.rfft(error[:,0])) / N
        spectrum[1:-1] *= 2
        db = lambda v: float(20*np.log10(float(v)/32768)) if v > 0 else None
        result = dict(case=name, mode=mode, source_sha256=hashlib.sha256(source.astype('<i4').tobytes()).hexdigest(), flac_bytes=pathlib.Path(str(stem)+'.flac').stat().st_size, decoder_exact=True, digital_bias_lsb=float(ev.mean()), digital_rms_error_lsb=float(np.sqrt(np.mean(ev**2))), digital_rms_error_dbfs=db(np.sqrt(np.mean(ev**2))), valid_sample_count=int(valid.sum()), maximum_error_all_samples_lsb=float(np.abs(error).max()), largest_non_dc_error_line_dbfs_peak=db(spectrum[1:].max()), quantization_python_host_ns=quant_ns, production_encode_host_ns=int(encoded.stdout))
        results.append(result)
        np.save(str(stem)+'.error-spectrum.npy', spectrum)
(ROOT/'results.json').write_text(json.dumps(dict(seed=SEED, frames=N, sample_rate_hz=FS, seconds=N/FS, tone_hz=freq, results=results), indent=2)+'\n')
print('18 production FLAC streams independently decoded exactly; vector and TPDF controls passed')
