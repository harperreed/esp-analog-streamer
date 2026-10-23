# Throwaway FLAC16 quantization host experiment

No production policy or device settings changed. All 18 streams used the unchanged production FLAC encoder; FFmpeg decoded every sample exactly to the expected 16-bit value.

This is partial qm2a evidence. The parent independently repeated the experiment: all 18 FLAC files were byte-identical, and every non-timing result field matched. See `quantization-repeat.json`.

Run from repository root; copy the retained experiment into scratch storage first:

```sh
mkdir -p target/kata-hardening/oct12-quantization
cp docs/audit-evidence/2026-10-12/quantization-encode.rs target/kata-hardening/oct12-quantization/encode.rs
cp docs/audit-evidence/2026-10-12/quantization-measure.py target/kata-hardening/oct12-quantization/measure.py
rustc --edition=2021 -O -A dead_code target/kata-hardening/oct12-quantization/encode.rs -o target/kata-hardening/oct12-quantization/encode
uv run --locked python target/kata-hardening/oct12-quantization/measure.py
```

Each case contains 49,152 stereo frames (1.024 s) at 48 kHz. Seed 20261012 controls synthetic noise and independently drawn per-channel TPDF values. The coherent tone is 973.6328125 Hz. Input floats round to integer capture codes before conversion. Rounded conversion uses floor((x+128)/256), with ties towards positive infinity and saturation. TPDF adds the sum of two independent uniform(-0.5,0.5) draws before rounding; support is ±1 output LSB. Candidate output is shifted into the capture domain before passing through unchanged production quantization. This measures actual production compression of each candidate PCM, not candidate firmware quantizer cost.

| Source | Policy | Bias (LSB) | RMS error (LSB) | Error dBFS RMS | FLAC bytes |
|---|---|---:|---:|---:|---:|
| silence | truncation | 0.000000 | 0.000000 | −∞ | 210 |
| silence | rounded_saturated | 0.000000 | 0.000000 | −∞ | 210 |
| silence | tpdf_rounded_saturated | 0.000437 | 0.500884 | -96.31 | 17114 |
| tone_minus90_dbfs | truncation | -0.494853 | 0.603260 | -94.70 | 14774 |
| tone_minus90_dbfs | rounded_saturated | 0.001343 | 0.246408 | -102.48 | 14022 |
| tone_minus90_dbfs | tpdf_rounded_saturated | 0.000610 | 0.501593 | -96.30 | 21288 |
| fade_minus60_to_minus120_dbfs | truncation | -0.496290 | 0.600030 | -94.75 | 16342 |
| fade_minus60_to_minus120_dbfs | rounded_saturated | 0.001726 | 0.247516 | -102.44 | 11988 |
| fade_minus60_to_minus120_dbfs | tpdf_rounded_saturated | 0.000851 | 0.500299 | -96.32 | 22284 |
| dc_quarter_lsb | truncation | -0.500000 | 0.559017 | -95.36 | 210 |
| dc_quarter_lsb | rounded_saturated | 0.000000 | 0.250000 | -102.35 | 210 |
| dc_quarter_lsb | tpdf_rounded_saturated | 0.000376 | 0.500351 | -96.32 | 18242 |
| noise_minus100_dbfs_rms | truncation | -0.497785 | 0.586004 | -94.95 | 18666 |
| noise_minus100_dbfs_rms | rounded_saturated | 0.000211 | 0.266829 | -101.78 | 14787 |
| noise_minus100_dbfs_rms | tpdf_rounded_saturated | -0.000857 | 0.500160 | -96.33 | 18977 |
| fullscale_extremes | truncation | -0.411544 | 0.567193 | -95.23 | 190989 |
| fullscale_extremes | rounded_saturated | 0.000230 | 0.381698 | -98.67 | 190989 |
| fullscale_extremes | tpdf_rounded_saturated | -0.051876 | 0.536981 | -95.71 | 191055 |

Errors are relative to integer 24-bit input divided by 256. dBFS uses 32768 as amplitude reference. Extreme-case RMS/bias excludes source values outside signed 24-bit range; independent decode still checks all samples, including i32 minimum/maximum. Saturation error at the positive rail remains included. Per-case error spectra are saved as .npy amplitude arrays with DC and Nyquist single-sided, other bins doubled. JSON uses null for zero-error dBFS (−∞), and records largest non-DC line, input hashes, and raw host timings. Spectrum maximum is a peak-bin measure and is not comparable to integrated RMS as though both represented the same noise quantity.

Rounding removes the roughly -0.5 LSB arithmetic-shift bias in the tone/fade/noise cases and reduces their integrated digital error. TPDF spreads low-level quantization structure into noise, and increases silence/DC compressed size sharply. These are digital synthetic results; they do not prove improved analog fidelity or audibility.

Memory and CPU limits: production encoder storage stays the same because its code is unchanged. This offline Python candidate allocates arrays and RNG outputs; these allocations and Python quantization times cannot budget an ESP32 implementation. JSON production encode timings are single host observations, not target CPU acceptance or robust performance statistics. A target candidate needs a chosen PRNG, overflow-safe wider arithmetic, memory accounting and repeated matched board measurements.

Unmet qm2a closure criteria: actual analog source/ADC noise capture, controlled audibility assessment, target CPU/memory budget, intended policy review, matched Sonos compatibility/board comparisons, and scripts/check. No analog or device acceptance ran. The parent subsequently passed all 27 canonical stages for the unchanged quantization policy and separate timeout fix in run.83iaZywQ; that does not validate a candidate quantizer. No RME claim was used; no exact primary source was supplied or established. Keep qm2a open pending these gates. Recommend retaining the current policy until measured analog evidence and policy review justify a change.

Verification: explicit sign/tie/saturation vectors and seeded silence TPDF support/mean/variance controls passed; 18 real production encodes passed FFmpeg sample equality with empty error output. This is a throwaway measurement, not a production feature or test-suite replacement. No dependencies were added.

Versions and provenance:
```
rustc 1.93.0 (254b59607 2026-01-19)
uv 0.12.19 (x86_64-unknown-linux-gnu)
ffmpeg version 8.0.1-3ubuntu2 Copyright (c) 2000-2025 the FFmpeg developers
7a0d320813e340e509aee60342c3bce30a253167
core/src/flac.rs sha256 01f0efabe7b759d4e09ef3ab66c5d5b75a12737713bf1664b29242134d472d46
```
