# FLAC16 quantization policy (qm2a; 2026-10-20)

Doctor Biz, this memo maps qm2a's acceptance list to the retained evidence,
adds the host-only error spectra that were still missing, and ends with a
recommendation. Short form: keep truncation; do not adopt TPDF dither; if
you want a change at all, saturated rounding is the only candidate the
numbers support, and its benefit is a half-LSB DC offset (-102 dBFS) that
nothing downstream can hear. No production source changed. The audio
policy decision is yours; this memo is the review the ticket asks for.

## What the board does today

`core/src/flac.rs` `quantize` converts each 24-bit capture code to 16 bits
with `(sample >> 8).clamp(SAMPLE_MIN, SAMPLE_MAX)`: an arithmetic shift,
so floor toward negative infinity, no rounding, no dither. The clamp never
acts on 24-bit input; it guards the `i32` range. The file hashes
`01f0efab…` today, the same as in the 2026-10-12 memo, so every number
below describes the shipping quantizer. The 16-bit output is the approved
Sonos format; capture and WAV stay 24-bit.

## Acceptance list against the retained evidence

| qm2a acceptance | Status | Proof |
| --- | --- | --- |
| Compare low-level tones, fades, DC bias, noise floor, full-scale saturation and extreme samples | met on the host; unmet on the board | `2026-10-12/quantization-host.md` (six inputs, 18 decodes); this memo (eleven inputs, 33 decodes, band spectra, harmonic lines, rail counts) |
| Measure actual analog noise and audibility alongside objective error spectra; retain controls and reproducible inputs | partly met | spectra, seeds, oracles and sources retained here and in `2026-10-12/`; analog noise exists only as meter readings (-79 dBFS idle, -86 dBFS ADC alone, `gotchas.md`, 2026-10-03); no spectrum of it, no listening test |
| Quantify CPU/memory and compressed size of any dither; no application mock mode | met | `2026-10-13/quant-target.md`: 180 ESP32 phases, heap endpoints, stack low-water; the probe was a temporary example, removed, with the ordinary ELF rebuilt bit-exact |
| Review the intended audio policy before modifying output; update the exact WAV-to-FLAC assertions | partly met | this memo is the review; output and `assert_quantized_samples` (`flac.rs`, `>> 8`) are unchanged because no policy change was made |
| Preserve Sonos compatibility; a 24-bit mode needs its own profile and budget, not automatic fallback | met by inaction | the 16-bit stream format is unchanged; no 24-bit FLAC path exists or is proposed here |
| Independent decoder checks, `scripts/check`, board comparisons | decoder checks met; `scripts/check` met for the unchanged policy; board comparison unmet | 18 + 30 + 33 FFmpeg sample-exact decodes; canonical runs `run.83iaZywQ` and `run.v8h8Edp2` (prior memos); no A/B of candidate streams on Sonos. `scripts/check` was not run for this memo: a six-hour soak holds the board and the main tree |
| Check the cited RME measurement source | not applicable | no URL was ever supplied; neither memo relies on it |

## Host measurements added today

Method: `quantization-spectra.py` (throwaway, retained beside this memo)
builds eleven deterministic stereo inputs of 49,152 frames (1.024 s at
48 kHz, seed 20261020), converts each with the three candidate policies,
then feeds the result through the unchanged production encoder (the
2026-10-12 `quantization-encode.rs` shim) and checks every sample with an
FFmpeg decode. Error is `output - input/256` in 16-bit LSB; dBFS is re
32768. Band figures are the error's RMS inside the band (Parseval over
the left channel's FFT). Harmonic lines are peak amplitudes at
`k × 973.63 Hz`, a bin-coherent tone, so no window leakage. Rounding is
`floor((x+128)/256)` with saturation; TPDF adds two uniform(-0.5, 0.5)
draws before rounding. The two noise levels are Gaussian stand-ins for the
board's meter readings, not the board's noise. The `tone_minus90`
truncation row reproduces the 2026-10-12 row to the last digit
(bias -0.494853, RMS 0.603260, 14,774 bytes), which ties the two runs.

### Error, bands and compressed size

| Source | Policy | Bias (LSB) | RMS err (LSB) | RMS err dBFS | 20 Hz-1 kHz | 1-5 kHz | 5-20 kHz | Largest line | FLAC bytes |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| tone_minus90 | truncation | -0.494853 | 0.603260 | -94.70 | -116.24 | -101.52 | -104.52 | -98.85 | 14774 |
| tone_minus90 | rounded_saturated | 0.001343 | 0.246408 | -102.48 | -114.87 | -106.77 | -105.30 | -104.01 | 14022 |
| tone_minus90 | tpdf_rounded_saturated | -0.000224 | 0.499689 | -96.34 | -110.14 | -104.16 | -98.36 | -126.99 | 21241 |
| tone_minus60 | truncation | -0.498271 | 0.572285 | -95.16 | -116.66 | -107.88 | -103.36 | -112.27 | 21960 |
| tone_minus60 | rounded_saturated | 0.001953 | 0.293238 | -100.96 | -115.36 | -108.20 | -102.90 | -113.28 | 22477 |
| tone_minus60 | tpdf_rounded_saturated | 0.000509 | 0.498696 | -96.35 | -110.04 | -104.12 | -98.40 | -127.16 | 32304 |
| tone_minus20 | truncation | -0.498637 | 0.575835 | -95.10 | -114.97 | -108.83 | -103.20 | -125.02 | 44355 |
| tone_minus20 | rounded_saturated | 0.002157 | 0.288888 | -101.09 | -114.62 | -108.98 | -103.13 | -125.16 | 44386 |
| tone_minus20 | tpdf_rounded_saturated | 0.001211 | 0.501045 | -96.31 | -110.03 | -104.09 | -98.40 | -126.68 | 52460 |
| tone_minus90_plus_idle_noise_minus79 | truncation | -0.497665 | 0.575030 | -95.12 | -115.10 | -108.95 | -103.15 | -132.14 | 49686 |
| tone_minus90_plus_idle_noise_minus79 | rounded_saturated | 0.002040 | 0.289290 | -101.08 | -114.86 | -108.83 | -103.14 | -131.56 | 49842 |
| tone_minus90_plus_idle_noise_minus79 | tpdf_rounded_saturated | 0.000596 | 0.500000 | -96.33 | -109.98 | -104.06 | -98.39 | -126.33 | 49945 |
| tone_minus60_plus_idle_noise_minus79 | truncation | -0.497623 | 0.575234 | -95.11 | -115.10 | -108.79 | -103.16 | -132.11 | 58563 |
| tone_minus60_plus_idle_noise_minus79 | rounded_saturated | 0.002723 | 0.288794 | -101.10 | -115.06 | -108.90 | -103.14 | -131.75 | 58563 |
| tone_minus60_plus_idle_noise_minus79 | tpdf_rounded_saturated | 0.000637 | 0.499918 | -96.33 | -110.00 | -104.15 | -98.40 | -127.60 | 58621 |
| noise_adc_minus86 | truncation | -0.498817 | 0.576232 | -95.10 | -114.92 | -108.82 | -103.15 | -131.42 | 35237 |
| noise_adc_minus86 | rounded_saturated | 0.001956 | 0.289059 | -101.09 | -115.01 | -108.83 | -103.11 | -131.99 | 35990 |
| noise_adc_minus86 | tpdf_rounded_saturated | 0.000064 | 0.499827 | -96.33 | -109.85 | -104.14 | -98.39 | -126.13 | 36436 |
| noise_idle_minus79 | truncation | -0.497969 | 0.575347 | -95.11 | -115.00 | -108.89 | -103.12 | -131.95 | 49419 |
| noise_idle_minus79 | rounded_saturated | 0.000292 | 0.289122 | -101.09 | -115.01 | -108.94 | -103.13 | -131.84 | 49587 |
| noise_idle_minus79 | tpdf_rounded_saturated | 0.000505 | 0.500559 | -96.32 | -109.96 | -104.08 | -98.38 | -126.61 | 49699 |
| fade_minus60_to_minus120_plus_idle_noise_minus79 | truncation | -0.498761 | 0.576096 | -95.10 | -114.87 | -108.97 | -103.12 | -130.84 | 52175 |
| fade_minus60_to_minus120_plus_idle_noise_minus79 | rounded_saturated | 0.002795 | 0.289058 | -101.09 | -115.08 | -109.00 | -103.11 | -131.71 | 52256 |
| fade_minus60_to_minus120_plus_idle_noise_minus79 | tpdf_rounded_saturated | 0.000455 | 0.499839 | -96.33 | -109.95 | -104.04 | -98.39 | -126.87 | 52346 |
| dc_quarter_lsb_plus_idle_noise_minus79 | truncation | -0.496725 | 0.574624 | -95.12 | -114.99 | -108.86 | -103.10 | -131.09 | 49454 |
| dc_quarter_lsb_plus_idle_noise_minus79 | rounded_saturated | -0.000093 | 0.288749 | -101.10 | -114.95 | -108.96 | -103.16 | -132.53 | 49584 |
| dc_quarter_lsb_plus_idle_noise_minus79 | tpdf_rounded_saturated | 0.001240 | 0.501192 | -96.31 | -109.86 | -104.04 | -98.37 | -127.04 | 49697 |
| fullscale_sine_0dbfs | truncation | -0.498047 | 0.575841 | -95.10 | -114.93 | -108.84 | -103.18 | -127.39 | 61202 |
| fullscale_sine_0dbfs | rounded_saturated | 0.000631 | 0.289840 | -101.07 | -115.15 | -108.54 | -103.20 | -127.38 | 61198 |
| fullscale_sine_0dbfs | tpdf_rounded_saturated | -0.001495 | 0.501014 | -96.31 | -110.05 | -104.08 | -98.35 | -126.97 | 62737 |
| rail_extremes_24bit | truncation | -0.442707 | 0.575094 | -95.11 | -152.68 | -140.31 | -102.68 | -100.10 | 196770 |
| rail_extremes_24bit | rounded_saturated | -0.220498 | 0.469571 | -96.87 | -166.29 | -141.67 | -98.20 | -99.59 | 196770 |
| rail_extremes_24bit | tpdf_rounded_saturated | -0.219827 | 0.551499 | -95.48 | -113.07 | -106.66 | -97.59 | -100.60 | 196770 |

### Harmonic error lines of the tones (dBFS peak at k × 973.63 Hz)

| Source | Policy | h1 | h2 | h3 | h4 | h5 |
|---|---|---:|---:|---:|---:|---:|
| tone_minus90 | truncation | -113.91 | -133.81 | -111.15 | -135.29 | -98.85 |
| tone_minus90 | rounded_saturated | -112.35 | -147.17 | -122.17 | -149.08 | -104.01 |
| tone_minus90 | tpdf_rounded_saturated | -133.14 | -137.34 | -135.26 | -134.10 | -137.16 |
| tone_minus60 | truncation | -125.14 | -160.23 | -124.99 | -159.22 | -124.55 |
| tone_minus60 | rounded_saturated | -121.08 | -175.42 | -120.97 | -193.20 | -120.78 |
| tone_minus60 | tpdf_rounded_saturated | -136.76 | -144.05 | -137.94 | -148.02 | -139.54 |
| tone_minus20 | truncation | -142.76 | -161.06 | -135.14 | -163.69 | -167.87 |
| tone_minus20 | rounded_saturated | -141.47 | -158.98 | -133.71 | -162.47 | -138.26 |
| tone_minus20 | tpdf_rounded_saturated | -133.32 | -154.75 | -133.82 | -141.54 | -138.38 |
| tone_minus90_plus_idle_noise_minus79 | truncation | -136.55 | -142.53 | -135.61 | -159.71 | -142.37 |
| tone_minus90_plus_idle_noise_minus79 | rounded_saturated | -137.70 | -150.84 | -134.04 | -152.58 | -136.26 |
| tone_minus90_plus_idle_noise_minus79 | tpdf_rounded_saturated | -137.22 | -139.29 | -143.47 | -142.01 | -144.01 |
| tone_minus60_plus_idle_noise_minus79 | truncation | -152.94 | -148.04 | -150.05 | -145.90 | -159.11 |
| tone_minus60_plus_idle_noise_minus79 | rounded_saturated | -140.59 | -142.59 | -147.86 | -138.72 | -147.86 |
| tone_minus60_plus_idle_noise_minus79 | tpdf_rounded_saturated | -138.08 | -142.04 | -145.47 | -145.86 | -139.30 |

### Level, rail hits and worst sample

| Source | Policy | Input RMS dBFS | Output RMS dBFS | Samples at rail | Max abs err (LSB) |
|---|---|---:|---:|---:|---:|
| noise_adc_minus86 | truncation | -86.00 | -85.51 | 0 | 0.9961 |
| noise_adc_minus86 | rounded_saturated | -86.00 | -85.86 | 0 | 0.5000 |
| noise_adc_minus86 | tpdf_rounded_saturated | -86.00 | -85.62 | 0 | 1.4766 |
| noise_idle_minus79 | truncation | -78.98 | -78.88 | 0 | 0.9961 |
| noise_idle_minus79 | rounded_saturated | -78.98 | -78.95 | 0 | 0.5000 |
| noise_idle_minus79 | tpdf_rounded_saturated | -78.98 | -78.90 | 0 | 1.4727 |
| fullscale_sine_0dbfs | truncation | -3.01 | -3.01 | 488 | 0.9961 |
| fullscale_sine_0dbfs | rounded_saturated | -3.01 | -3.01 | 468 | 0.9961 |
| fullscale_sine_0dbfs | tpdf_rounded_saturated | -3.01 | -3.01 | 444 | 1.4766 |
| rail_extremes_24bit | truncation | -0.51 | -0.51 | 87382 | 0.9961 |
| rail_extremes_24bit | rounded_saturated | -0.51 | -0.51 | 65538 | 0.9961 |
| rail_extremes_24bit | tpdf_rounded_saturated | -0.51 | -0.51 | 64216 | 1.0039 |

The other cases hit the rail 0 times under every policy; their rows are
in `quantization-spectra.json`.

### What the numbers say

- Truncation's error is a -0.5 LSB DC offset plus 0.289 LSB RMS of AC
  error; rounding keeps the AC part and drops the offset, which is why
  its RMS is 6 dB lower in every row. The offset is a -102 dBFS DC term.
- On a clean tone with no noise, truncation and rounding both leave
  signal-correlated lines. At -90 dBFS truncation's worst line is
  -98.85 dBFS (5th harmonic), rounding's -104.0; TPDF flattens them to
  about -135. At -60 dBFS the lines are already at -112 and below, and at
  -20 dBFS they sit at -125 under every policy.
- Add Gaussian noise at the board's idle meter level (-79 dBFS, 3.68 LSB
  RMS) or the ADC's own level (-86 dBFS, 1.64 LSB RMS) and the lines
  vanish under truncation too: every harmonic is -134 dBFS or lower,
  the band RMS figures agree across policies to 0.2 dB, and the three
  outputs differ in level by at most 0.07 dB (0.35 dB at the ADC-only
  level: -85.51 / -85.86 / -85.62). Noise above 0.5 LSB RMS does what
  TPDF does; both board readings exceed that, if the real noise is
  broadband (unmeasured; see below).
- Compressed size: with idle-level noise present, rounding costs
  +0.3 % and TPDF +0.6 % (14 and 23 bytes per 4,096-frame block). On
  clean inputs TPDF costs +44 % (-90 dBFS tone), +47 % (-60 dBFS) and
  +18 % (-20 dBFS); the 2026-10-12 silence case went from 210 to 17,114
  bytes per 1.024 s, and the ESP32 probe from 14 to 1,421 bytes per block.
  That penalty needs digital silence or a noiseless input, which the
  analog path does not produce. Rounding changes size by -5 % to +2.4 %
  on clean inputs and leaves the incompressible rail pattern at exactly
  196,770 bytes under every policy.
- Saturation: with rounding, the top 128 capture codes (0x7FFF80 to
  0x7FFFFF) would become 32768 and clamp to 32767; truncation already maps
  those codes to 32767. The worst positive-rail error is 0.9961 LSB under
  both, and a 0 dBFS sine hits the rail 488 times under truncation and 468
  under rounding (truncation also pins 256 negative codes to -32768,
  rounding 128). Saturated rounding adds no clipping and no new worst
  case. `rail_extremes_24bit` stays within the 24-bit range on purpose;
  the 2026-10-12 `fullscale_extremes` case also fed `i32` extremes, which
  the clamp catches identically under all three policies.
- TPDF's worst sample error is 1.48 LSB (support ±1 LSB plus rounding),
  against 0.9961 for truncation and 0.5 for rounding.

## What the host cannot measure

- The board's real noise: its level is a meter reading from 2026-10-03,
  and its spectrum is unknown. Hum or 1/f noise concentrated below 200 Hz
  would not linearize the quantizer the way the Gaussian stand-in does.
  No 24-bit capture from the board is retained in the repository (the
  only `.i32le` files under `docs/audit-evidence/` are MP3 bound tables),
  and the board is held by a soak, so this memo could not take one.
  The gotchas also record idle frames of "roughly 11 KB" per block at
  about -82 dBFS, against 4.1 KB for the Gaussian stand-in at -79 dBFS,
  so the real noise is not white Gaussian at that level.
- Audibility. Nothing here is a listening test; the differences are at
  -95 to -135 dBFS, under a -79 dBFS idle floor.
- Sonos behaviour with a rounded or dithered stream. The container,
  sample rate and bit depth do not change, so the expected answer is
  "identical", but it has not been shown.
- Loaded CPU headroom. The ESP32 costs below are standalone.

## Costs on the ESP32 (from `2026-10-13/quant-target.md`)

| Policy | Added preprocessing per 4,096-frame block | Share of the block's 85.33 ms | Bytes per block, silence / quarter-LSB DC / triangle fade |
| --- | ---: | ---: | --- |
| truncation (today) | 0 (baseline 2.71-3.15 ms in the unfused probe) | 0 | 14 / 14 / 1,425 |
| saturated rounding | +0.348 to +0.394 ms | 0.41-0.46 % | 14 / 14 / 1,425 (unchanged) |
| TPDF (xorshift32, discrete) | +2.462 to +2.850 ms | 2.9-3.3 % | 1,421 / 1,518 / 1,857 (+30 % on the fade) |

Heap endpoints were identical across phases; the TPDF state is eight
bytes per channel; minimum free task stack 1,348 bytes in the probe task.
These numbers repeat the baseline shift inside the unchanged encoder, so
a fused implementation would cost less than the rounding figure, and the
TPDF figure is for one specific generator.

## Recommendation

Keep truncation. The host evidence shows the only defect of the current
policy that survives a realistic noise floor is a -0.5 LSB DC offset
(-102 dBFS), and the ticket's own premise, that the analog path may
already dither, holds for any broadband noise above 0.5 LSB RMS, which
both board readings exceed by 3 to 7 times. TPDF is not justified: it costs
about 3 % of core 0 per block in the unfused probe, raises the worst
sample error to 1.5 LSB, balloons the size of any quiet or synthetic
input, and in every noisy row of this memo it changes nothing the Sonos
could receive differently beyond a 0.05 dB higher floor. Rounding is the
only candidate with a measurable gain at near-zero cost (0.4 % of the
block, +0.3 % bytes, no new clipping), but the gain is a DC offset below
the noise, and adopting it would re-pin every byte-exact FLAC assertion
and the retained 2026-10-12/13 receipts for a change no one can hear or
measure on the board. If you value an unbiased quantizer as a matter of
spec cleanliness, take saturated rounding as a single fixed policy; do
not add a runtime mode, and do not add dither.

A board comparison, if you want one before closing qm2a, would have to
show: (1) a 24-bit idle capture's spectrum, to confirm the noise is
broadband above 0.5 LSB16 RMS rather than hum; (2) the same fused
quantizer's per-block cost with capture and two listeners running, not
standalone; (3) compressed frame size on real idle and music frames for
the candidate against truncation; (4) one Sonos play of each stream.
Without (1), the rows above are the strongest evidence available, and
they favour leaving the code alone.

## Reproduction and provenance

```sh
mkdir -p target/kata-hardening/oct20-quantization && cd target/kata-hardening/oct20-quantization
cp ../../../docs/audit-evidence/2026-10-12/quantization-encode.rs encode.rs   # fix its #[path] to core/src/flac.rs from here
rustc --edition=2021 -O -A dead_code encode.rs -o encode
uv run --project ../../.. --locked python ../../../docs/audit-evidence/2026-10-20/quantization-spectra.py "$PWD"
```

- 33 production encodes, every sample equal to the Python oracle after an
  FFmpeg `s16le` decode with empty stderr; the sign, tie, saturation and
  top-128-code vectors passed.
- Host tests: `cd core && cargo test` green at the memo's commit; no
  application code changed. `scripts/check` was not run (board soak).
- rustc 1.93.0 (254b59607 2026-01-19); ffmpeg 8.0.1; numpy 2.4.4;
  `core/src/flac.rs` sha256
  `01f0efabe7b759d4e09ef3ab66c5d5b75a12737713bf1664b29242134d472d46`.
- Raw rows: `quantization-spectra.json` (seed, per-case hashes, every
  metric including the harmonic lines of the noise-only cases).
