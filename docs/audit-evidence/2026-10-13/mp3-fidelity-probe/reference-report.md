# Corrected Shine versus libmp3lame: synthetic reference

This is a throwaway offline comparison at 48 kHz, stereo, 320 kbps. It uses
six exact retained signed-16-bit fixtures from the corrected Shine headroom
study: independent noise, independent 997/1703 Hz tones, and an isolated impulse
with two abrupt tone gates, each at 0.5 and 0.98 scale. It changes no production
source and makes no playback, replacement, or subjective acceptance claim.

`comparison.json` contains full per-channel raw metrics, commands, tool versions,
stream headers, source hashes, frequency-band energies, and event windows.
`source/` freezes the corrected encoder source identified by the archived
`candidate.json`; `retained-candidate.json` preserves that original receipt.
The candidate's recorded Git HEAD is baseline metadata for a patched snapshot,
not a claim that the corrected source equals that baseline commit.

`publication.json` is a derived compact receipt, not a byte-identical copy of
`comparison.json`. It preserves all measurements, source/alignment/hash fields,
commands and tool versions, while replacing full FFmpeg help bodies with exact
relevant lines and hashed local log receipts. It records the original JSON's
SHA256 and byte count. `prepare-publication.py` reproduces that transformation;
no raw PCM, MP3 or decoded float content is embedded in either JSON receipt.

## Method and counts

FFmpeg 8.0.1-3ubuntu2 uses installed libmp3lame 3.101~svn6525+dfsg-2. The reference
keeps its documented defaults: reservoir and joint stereo enabled,
`compression_level=-1`, `cutoff=0`. These logs do not expose the library's chosen
internal quality or exact lowpass frequency; no numeric value is assumed.
The output uses `-write_xing 0 -id3v2_version 0`, with no gapless trimming.
Each actual MPEG header confirms 48 kHz and 320 kbps. Shine has 96 stereo frames
and no bit reservoir; LAME has 97 joint-stereo frames and uses the reservoir.
Those algorithm differences remain part of this reference, not a controlled
comparison of only one encoder stage.

Each source has exactly 110,592 stereo sample frames. Shine decodes to 110,592;
LAME decodes to 111,744. LAME's independently measured delay is 1,105 samples:
a separate deterministic nonperiodic source's linear FFT correlation and two
separated impulses agree, both channels. Shine's 1,057-sample delay comes from
the October 9 independent study and is freshly confirmed by the retained
isolated impulse at both amplitudes and on both channels. No tone fit determines
delay. `alignment` records the new LAME controls; each transient case records
the confirming Shine peak offsets.

All comparisons use the same source range `[0,109535)`, 109,535 sample frames.
Steady measurements use `[4608,104927)`, exactly 100,319 sample frames. Thus
steady results exclude the first 4,608 and last 5,665 original source frames.
Shine lacks the last 1,057 source frames in this finite retained stream; LAME
flushes one extra MPEG frame and retains the complete source plus 47 decoded
frames after it. Extra LAME tail output is excluded from the common fidelity
comparison and included in whole-decoded peak/count measurements. This does
not evaluate a finite-drain implementation for the live encoder.

Source samples are signed16 divided by 32,768. Both streams decode with
`mp3float` into raw f32, then calculations use float64. Gain is the reported
least-squares scalar; it is never applied. SNR always uses actual `output-input`.
Each frequency band sums squared rFFT magnitude divided by N², doubling
positive-frequency bins except DC and an even-N Nyquist bin. A rectangular
window retains spectral leakage. Band sums equal mean-square energy by Parseval.
Bands are `[0,16k)`, `[16k,20k)`, and `[20k,24k]` Hz.

## Results

Values are left/right. SNR uses the common steady region. Peaks and samples
outside ±1 use every decoded float sample, including startup and flush output.

| Fixture | Shine raw SNR dB | LAME raw SNR dB | Shine peak | LAME peak | Shine outside ±1 | LAME outside ±1 |
|---|---:|---:|---:|---:|---:|---:|
| Half noise | 12.56 / 12.58 | 7.56 / 7.57 | .776 / .740 | .882 / .803 | 0 / 0 | 0 / 0 |
| 0.98 noise | 12.61 / 12.63 | 7.56 / 7.55 | 1.541 / 1.507 | 1.716 / 1.563 | 5,942 / 5,799 | 3,450 / 3,468 |
| Half tones | 75.37 / 74.83 | 78.79 / 77.50 | .500 / .502 | .500 / .501 | 0 / 0 | 0 / 0 |
| 0.98 tones | 74.92 / 75.13 | 80.18 / 79.74 | .982 / .983 | .981 / .983 | 0 / 0 | 0 / 0 |
| Half impulse/gates | 37.84 / 36.93 | 40.61 / 40.24 | .621 / .621 | .505 / .518 | 0 / 0 | 0 / 0 |
| 0.98 impulse/gates | 38.17 / 37.50 | 40.31 / 40.17 | 1.166 / 1.166 | .996 / 1.020 | 1 / 1 | 0 / 2 |

Noise gain is approximately 1.025–1.028 for Shine and 0.840 for LAME. Tone
gain remains near unity: 0.99985–0.99986 for Shine and 0.99994–0.99997 for LAME.
The noise scalar mixes bandwidth and error; it does not establish a uniform
volume gain difference.

Full-band white-noise SNR favors Shine here because LAME removes much of the
top band. On 0.98 noise, LAME retains about 6.1–6.4% of source energy from
20–24 kHz; Shine retains about 110%. Below 16 kHz, Shine measures
12.64/12.67 dB against LAME's 16.68/16.72 dB. In 16–20 kHz, Shine measures
12.55/12.55 dB against LAME's 14.56/14.58 dB. Thus the lower full-band LAME
score cannot establish worse listening quality. No exact cutoff is inferred
from these coarse bands.

Transient regions are separate from the aggregate score. On the 0.98 input,
the isolated impulse measures 10.72/10.72 dB for Shine and 7.94/7.94 dB for
LAME. Its whole-region peaks are 1.166/1.166 and .823/.823, respectively.
The first gate measures 40.82/40.77 dB versus 48.10/48.16 dB; the second gate
36.97/35.63 versus 41.02/40.53 dB. Source-zero windows immediately before
each gate have no defined SNR; their output RMS and peaks are recorded rather
than relabeled as signal fidelity. LAME has lower RMS in those windows, although
some preceding-silence peak comparisons favor Shine.

Both codecs can generate float samples outside ±1 on bounded near-full-scale
noise. This proves a downstream integer conversion could clip those samples;
it does not prove internal integer overflow or a structural Shine defect.
Shine has higher below-20-kHz noise error and gate error in this matrix, while
its simple tone gain is stable. These findings characterize numerical quality;
they neither certify nor reject audible quality. Audible-quality acceptance
still requires the separately scoped listening decision.

## Verification and reproduction

`uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-fidelity-reference/compare.py --verify`
checks 93 retained artifact hashes and recomputes both codecs' steady metrics
for all six cases. `verification.json` separately checks arithmetic controls,
Parseval normalization, and byte-identical Shine float re-decodes against the
archived candidate receipts. All acquisition encoder/decoder error logs are
empty. `failed-first-run.log` retains the initial calibration assertion failure:
the assertion expected one receipt for the deliberately two-impulse control.
It was corrected before any comparison case ran. It is not a codec failure.

To reacquire, use a fresh output directory and retain this directory first;
the script rejects an existing result. Production checks, firmware behavior,
analog fidelity, transport, and endurance are outside this offline study.
