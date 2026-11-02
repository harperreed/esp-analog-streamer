# Correct the MP3 transform gain

The approximately +1.02 dB tone gain came from the pinned Shine transform's
normalization. Its analysis and MDCT stages together divide by eight, while
the verified inverse overlap multiplies by nine. The encoder now multiplies
the MDCT coefficient table by 8/9 during initialization. This corrects the
transform before quantization and adds no multiplication to the frame loop.

The factor follows from the transform identity and an independent decoder
reference. It is not a scalar selected by fitting the previous PCM result.
The earlier [quantizer experiment](mp3-gain.md) remains a separate historical
result; its replacement quantizer was not adopted.

## Derivation

Let `x` be the actual signed-16-bit PCM divided by 32768, `A` the unscaled
polyphase analysis, and `M` the 36-to-18 sine-window MDCT. Shine's input and
tables use Q31 units, but each generic multiply shifts right by 32. Windowing
and the analysis cosine matrix therefore produce `Ax/4`; the MDCT adds
another half and stores `MAx/8`. The alias rotations use a 31-bit shift and
do not contribute another half.

The independent model computes trigonometric matrices directly. Its exact
integer replay also matches every actual compiled-stage coefficient on three
synthetic inputs: 82,944 subband and 82,944 MDCT values. This confirms the
interpretation of the source arithmetic, including per-product truncation.

For the long sine window, `w[n]^2 + w[n+18]^2 = 1`. Overlapping successive
unnormalized inverse MDCT blocks reconstructs nine times the input with an
18-subband-sample delay. All 18 interior unit basis positions verify the
identity after division by nine; maximum error is `3.735e-15`.

Decoder units were checked independently against original FFmpeg n8.0
calculation bodies. Its dequantization scale is `2^28/1.759`; the inverse
window includes `1.759/32`, and synthesis uses the integer window table
divided by `2^(16+23)`. The powers and auxiliary scalar cancel, leaving the
direct unnormalized inverse and the synthesis table divided by 65536.

The extracted inverse passes all 18 spectral basis columns within `1.155e-6`.
The original DCT32 passes all 32 columns within `9.445e-7`; original synthesis
passes all 32 band impulse responses, each 1,024 samples, within `1.064e-6`.
These tests check absolute scale rather than assuming a textbook convention.
Exact reference URLs and SHA256 values are in the
[manifest](mp3-normalization-probe/reference-manifest.json).

Unscaled analysis/synthesis reconstructs the impulse and two tones near unity
(0.999991, 0.999951, 0.999952). Passing actual stored MDCT coefficients through
the checked inverse gives 1.124990, 1.124944, 1.124946. Dividing those results
by their respective polyphase controls gives 9/8 within `4e-8`. Rounded filter
tables explain the small residual. Normalizing the coefficient table changes
`MAx/8` to the required `MAx/9`.

## Regression and corrected output

The new actual-wrapper/FFmpeg regression first failed at RMS gain 1.124832,
with sample count and source correlation already passing. After correction,
all three decoder integration tests pass. The new test checks exactly 110,592
decoded stereo frames, channel correlation above 0.999 and RMS gain within
0.5% of unity. Unequal 997/1703 Hz channels also catch swapped channels.
Startup and undrained tail are excluded from the steady gain measurement.

A separate upstream build applies only coefficient normalization to the exact
pin `ab5e3526b64af1a2eaa43aa6f441a7312e013519`. The current production bridge
matches it byte-for-byte for the xorshift golden, retained tones and retained
noise. The golden test now uses that documented normalized reference; its
original hash remains in `UPSTREAM.md` as historical provenance.

| Corrected 320 kbps fixture | Actual decoded frames | Left/right fitted gain | Left/right SNR versus source |
| --- | ---: | --- | --- |
| Half-scale 997/1703 Hz tones | 483,840 | 0.999858 / 0.999851 | 75.32 / 74.93 dB |
| Independent stereo noise | 483,840 | 1.025621 / 1.026119 | 12.59 / 12.59 dB |

These gains report measurements; no fitted correction is applied. The
comparison uses the previously established 1,057-sample delay and trims
4,608 source samples from each side of the aligned overlap. Decoding all
frame durations does not prove that an undrained finite stream retains its
last source samples.

## Acceptance limits

The transform defect is fixed. MP3 fidelity acceptance remains open: the noise
result still has substantial error and gain bias. Shine has no psychoacoustic
model; tone accuracy does not establish transparency on music, full-scale
headroom or analog quality. Resolve the codec quality and finite-stream tail
requirements before calling MP3 production-ready. Recovery, long-duration
delivery and distribution/relinking acceptance also remain separate e8hd gates.

The [archived probes](mp3-normalization-probe/README.md) and
[machine-readable receipt](mp3-normalization.json) record reproduction and
hashes. Private raw artifacts live under
`target/kata-hardening/oct13-mp3-normalization/`. Two Sol 6.1 workers supplied
the independent model and regression/fix; another reviewed both. The parent
reran the full model and independent corrected-build comparison.

All 27 canonical stages pass in `run.uO3pGenC`, including host tests, Clippy,
firmware build and IRAM checks. The ordinary corrected ELF is
`bb44c0f19a1ca09cf38eca4fc790f81ab314b1e72712fd28db7c6e2ee664ef6b`;
the flashed image and board API match it. It remains deployed.

A 60-second two-listener MP3 canary passes on that image: each actual decoder
produces 2,880,000 sample frames, with no reported decoded discontinuities.
Nine capture windows pass clean-startup and steady-state assertions, with no
counter increase. The eight pre-baseline queue overflows and 24,000 startup
settling frames remain explicit. Quiet uncontrolled ADC input cannot prove
source continuity or physical gain; this is neither the 7vhq contention gate
nor an endurance test.

The UART retains known MQTT TLS/select connection failures and boot warnings.
Both sockets report client-close resets after the requested decode duration;
these do not make every logged error a new encoder defect. A listener's maximum
poll wait reached 2.495 seconds, so this short success does not resolve the
transport diagnosis. No workstation audio, mixer, speaker volume or Sonos Play
operation occurred.

The first final-state probe failed because it assumed the freshly rebooted
board had a populated Sonos group list. Its failed receipt remains recorded.
The corrected read-only probe validates the preflight peer's current UDN before
reading transport state. Final verification confirms stopped MP3/Medium,
unchanged settings, recognition/autoplay off, idle listeners and remote
STOPPED/OK. All owned flash, decoder and serial processes exited. Fresh-eyes
review found no blocking issue in the corrected encoder and evidence.
