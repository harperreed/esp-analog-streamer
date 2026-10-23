# ABOUTME: Throwaway host experiment: error spectra of truncation, saturated rounding and TPDF
# ABOUTME: rounding on synthetic 24-bit inputs, encoded by the unchanged production FLAC encoder.
#
# Not a harness and not application code. Usage, from a scratch directory holding the
# compiled docs/audit-evidence/2026-10-12/quantization-encode.rs as ./encode:
#   uv run --project <repo> --locked python <repo>/docs/audit-evidence/2026-10-20/quantization-spectra.py <scratch>
# Writes <scratch>/spectra.json and prints the memo's tables.
import hashlib
import json
import pathlib
import subprocess
import sys

import numpy as np

OUT = pathlib.Path(sys.argv[1]).resolve()
ENCODE = OUT / "encode"
N = 4096 * 12  # 1.024 s of 48 kHz, twelve production blocks
FS = 48000
SEED = 20261020
FULL = 2**23  # 24-bit capture codes; the output LSB is 256 of them
K0 = 997  # bin-coherent tone: 997 * 48000 / 49152 = 973.6328125 Hz
F0 = K0 * FS / N
rng = np.random.default_rng(SEED)
t = np.arange(N) / FS


def tone(dbfs):
    return np.column_stack([np.sin(2 * np.pi * F0 * t), np.cos(2 * np.pi * F0 * t)]) * (FULL * 10 ** (dbfs / 20))


def gauss(dbfs_rms):
    return rng.normal(0, FULL * 10 ** (dbfs_rms / 20), (N, 2))


# Board levels: idle LINE IN read about -79 dBFS and LIN1/RIN1 (the ADC alone) -86 dBFS on
# 2026-10-03 (gotchas.md); both are meter readings, not spectra, so Gaussian stands in here.
sources = {
    "tone_minus90": tone(-90),
    "tone_minus60": tone(-60),
    "tone_minus20": tone(-20),
    "tone_minus90_plus_idle_noise_minus79": tone(-90) + gauss(-79),
    "tone_minus60_plus_idle_noise_minus79": tone(-60) + gauss(-79),
    "noise_adc_minus86": gauss(-86),
    "noise_idle_minus79": gauss(-79),
    "fade_minus60_to_minus120_plus_idle_noise_minus79": tone(0) * 10 ** (np.linspace(-60, -120, N)[:, None] / 20) + gauss(-79),
    "dc_quarter_lsb_plus_idle_noise_minus79": np.tile([64.0, -64.0], (N, 1)) + gauss(-79),
    "fullscale_sine_0dbfs": tone(0) * (FULL - 1) / FULL,
    "rail_extremes_24bit": np.resize(
        np.array([-FULL, FULL - 1, FULL - 128, FULL - 129, FULL - 256, -FULL + 127, -FULL + 128, -FULL + 255, 0]), (N, 2)
    ),
}


def quantize(x, mode):
    if mode == "truncation":
        return np.clip(x >> 8, -32768, 32767)
    if mode == "rounded_saturated":
        return np.clip((x + 128) >> 8, -32768, 32767)
    r = np.random.default_rng(SEED)  # independent per-sample draws, support +/- 1 output LSB
    dither = r.random(x.shape) + r.random(x.shape) - 1
    return np.clip(np.floor(x / 256 + dither + 0.5), -32768, 32767).astype(np.int64)


x = np.array([-(2**31), -257, -256, -255, -129, -128, -127, -1, 0, 127, 128, 255, 256, 2**31 - 1], dtype=np.int64)
assert quantize(x, "truncation").tolist() == [-32768, -2, -1, -1, -1, -1, -1, -1, 0, 0, 0, 0, 1, 32767]
assert quantize(x, "rounded_saturated").tolist() == [-32768, -1, -1, -1, -1, 0, 0, 0, 0, 0, 1, 1, 1, 32767]
# The top 128 capture codes round to 32768 and saturate; 0x7FFF7F is 32767 exactly.
assert quantize(np.array([FULL - 1, FULL - 128, FULL - 129, FULL - 385]), "rounded_saturated").tolist() == [32767, 32767, 32767, 32766]


def db(v):
    return float(20 * np.log10(float(v) / 32768)) if v > 0 else None


def band_rms(power, lo_hz, hi_hz):
    bins = np.arange(len(power)) * FS / N
    return float(np.sqrt(power[(bins >= lo_hz) & (bins < hi_hz)].sum()))


results = []
for name, source in sources.items():
    source = np.rint(source).astype(np.int64)
    assert source.min() >= -FULL and source.max() <= FULL - 1, name
    for mode in ["truncation", "rounded_saturated", "tpdf_rounded_saturated"]:
        q = quantize(source, mode)
        capture = source if mode == "truncation" else q * 256
        stem = OUT / f"{name}.{mode}"
        capture.astype("<i4").tofile(str(stem) + ".i32le")
        encoded = subprocess.run([str(ENCODE), str(stem) + ".i32le", str(stem) + ".flac"], capture_output=True, check=True)
        assert not encoded.stderr, encoded.stderr
        decoded = subprocess.run(
            ["ffmpeg", "-nostdin", "-v", "error", "-i", str(stem) + ".flac", "-f", "s16le", "-"], capture_output=True, check=True
        )
        assert not decoded.stderr, decoded.stderr
        assert np.array_equal(np.frombuffer(decoded.stdout, "<i2").reshape(q.shape), q), (name, mode)
        error = q.astype(float) - source / 256
        # Single-sided error power per bin of the left channel; its sum is the mean square.
        spectrum = np.abs(np.fft.rfft(error[:, 0])) / N
        spectrum[1:-1] *= np.sqrt(2)
        power = spectrum**2
        assert abs(power.sum() - np.mean(error[:, 0] ** 2)) < 1e-6 * max(1.0, power.sum())
        amplitude = spectrum.copy()
        amplitude[1:-1] *= np.sqrt(2)  # peak amplitude of a coherent line
        harmonics = {f"h{k}": db(amplitude[K0 * k]) for k in range(1, 6)}
        results.append(
            dict(
                case=name,
                mode=mode,
                source_sha256=hashlib.sha256(source.astype("<i4").tobytes()).hexdigest(),
                flac_bytes=(OUT / f"{name}.{mode}.flac").stat().st_size,
                decoder_exact=True,
                bias_lsb=float(error.mean()),
                rms_error_lsb=float(np.sqrt(np.mean(error**2))),
                rms_error_dbfs=db(np.sqrt(np.mean(error**2))),
                max_abs_error_lsb=float(np.abs(error).max()),
                output_rms_dbfs=db(np.sqrt(np.mean(q.astype(float) ** 2))),
                input_rms_dbfs=db(np.sqrt(np.mean((source / 256) ** 2))),
                samples_at_rail=int(((q == 32767) | (q == -32768)).sum()),
                error_rms_dbfs_20_1k=db(band_rms(power, 20, 1000)),
                error_rms_dbfs_1k_5k=db(band_rms(power, 1000, 5000)),
                error_rms_dbfs_5k_20k=db(band_rms(power, 5000, 20000)),
                largest_non_dc_line_dbfs_peak=db(amplitude[1:].max()),
                error_line_dbfs_peak=harmonics,
                production_encode_host_ns=int(encoded.stdout),
            )
        )
(OUT / "spectra.json").write_text(
    json.dumps(dict(seed=SEED, frames=N, sample_rate_hz=FS, seconds=N / FS, tone_hz=F0, results=results), indent=2) + "\n"
)


def fmt(v, digits=2):
    return "none" if v is None else f"{v:.{digits}f}"


print(f"{len(results)} production FLAC streams decoded exactly by FFmpeg; vector controls passed\n")
print("| Source | Policy | Bias (LSB) | RMS err (LSB) | RMS err dBFS | 20 Hz-1 kHz | 1-5 kHz | 5-20 kHz | Largest line | FLAC bytes |")
print("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
for r in results:
    print(
        f"| {r['case']} | {r['mode']} | {r['bias_lsb']:.6f} | {r['rms_error_lsb']:.6f} | {fmt(r['rms_error_dbfs'])} | "
        f"{fmt(r['error_rms_dbfs_20_1k'])} | {fmt(r['error_rms_dbfs_1k_5k'])} | {fmt(r['error_rms_dbfs_5k_20k'])} | "
        f"{fmt(r['largest_non_dc_line_dbfs_peak'])} | {r['flac_bytes']} |"
    )
print("\n| Source | Policy | h1 | h2 | h3 | h4 | h5 |")
print("|---|---|---:|---:|---:|---:|---:|")
for r in results:
    if r["case"].startswith("tone"):
        h = r["error_line_dbfs_peak"]
        print(f"| {r['case']} | {r['mode']} | " + " | ".join(fmt(h[f'h{k}']) for k in range(1, 6)) + " |")
print("\n| Source | Policy | Input RMS dBFS | Output RMS dBFS | Samples at rail | Max abs err (LSB) |")
print("|---|---|---:|---:|---:|---:|")
for r in results:
    print(f"| {r['case']} | {r['mode']} | {fmt(r['input_rms_dbfs'])} | {fmt(r['output_rms_dbfs'])} | {r['samples_at_rail']} | {r['max_abs_error_lsb']:.4f} |")
