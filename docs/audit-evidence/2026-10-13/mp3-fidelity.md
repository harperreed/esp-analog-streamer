# MP3 fidelity after the gain and headroom fixes

The required bounded objective comparisons now cover music, fades, tones,
independent noise and transients on the corrected encoder at `369af00`.
They do not establish listening acceptance. The results do not justify
replacing the encoder solely because of a noise SNR or float peak.

## Music and fades

All encoders receive the same signed-16-bit, 48 kHz stereo PCM at 320 kbps.
The source is the previously supplied 10.08-second excerpt; no recording is
published. Decoded float samples are compared without gain correction.

| Music measurement | Current Shine | Default libmp3lame | Independent stereo, no-reservoir libmp3lame |
| --- | ---: | ---: | ---: |
| Raw steady SNR | 39.19 dB | 58.95 dB | 37.70 dB |
| Measured gain | 0.998959 | 1.000014 | 0.999865 |
| Maximum decoded magnitude | 0.295068 | 0.294185 | 0.294787 |

The channels are identical, the first 0.70025 seconds are silent, and the
source peak is 0.294220. Default LAME uses joint stereo and a bit reservoir;
Shine uses independent stereo and no reservoir references. Disabling both
LAME features reverses the numerical ranking on this excerpt. Actual frame
headers verify the configurations. This does not identify either setting's
individual contribution, establish audibility, or predict ESP32 encoding cost.
One modest-level excerpt cannot represent stereo music or a broad repertoire.

Independent nonperiodic controls establish fixed delays of 1,057 samples for
Shine and 1,105 for both LAME configurations. Music middle/end timing checks
agree without per-segment realignment. The silent beginning provides no timing
evidence and remains explicitly unavailable.

The music/fade probe appends 2,304 known zero frames, then scores only the
original source. Shine music decodes to 486,144 frames; LAME to 487,296. Neither
the added input zeros nor decoder trailing padding improve the score. This
does not add a drain to the live encoder or prove a universal tail length.

The fade rises from −90 to about −6 dBFS and falls back. Aggregate SNR is
74.81/74.71 dB for Shine and 77.35/76.55 dB for default LAME. Level matters:
the rising −90 to −75 dBFS interval measures 35.54/35.41 versus 26.36/26.12 dB.
The receipts retain both fade directions and silent-endpoint error; silence
has no source-relative SNR. No acquired music/fade/tone float exceeds ±1.

## Noise and transients

A separate six-case comparison uses the retained half- and near-full-scale
independent noise, unequal tones and impulse/gate fixtures. Both encoders use
the same source prefix after independent delay accounting. Whole-decoded peak
counts include startup and flush; steady scores exclude them.

Near-full-scale noise gives full-band SNR of 12.61/12.63 dB for Shine and
7.56/7.55 dB for default LAME. That ranking hides bandwidth differences:
below 16 kHz, LAME measures 16.68/16.72 dB versus Shine's 12.64/12.67 dB.
LAME retains only about 6.1–6.4% of source energy in the 20–24 kHz band.
The reference's exact internal cutoff is unknown; coarse bands cannot supply it.

Both encoders overshoot on this noise: Shine peaks at 1.541/1.507 and LAME at
1.716/1.563. Such reconstructed float peaks could clip on integer conversion;
they do not by themselves prove internal overflow or audible clipping. LAME
has less error on the tone gates, while isolated-impulse and full-band noise
scores favor Shine. The [synthetic report](mp3-fidelity-probe/reference-report.md)
retains these distinctions instead of reducing them to one quality ranking.

## Verification and acceptance

Two Sol 6.1 workers acquired the bounded comparisons; a third independently
checked hashes, actual decoded counts, source scaling, delays, frame headers
and representative numerical results. The parent reran the six-case verifier,
independently recomputed all eight primary music/control outputs, and checked
the two constrained-reference outputs. No blocking measurement defect remains.

Two failed diagnostic attempts remain recorded. An initial synthetic calibration
assertion expected one result from two impulses. The first music acquisition
failed JSON serialization because a silent timing window produced NaN; the
corrected run keeps that same window and reports unavailable evidence. Neither
failure establishes a codec fault. Final encoder/decoder diagnostics are empty.

Production source, firmware, hardware, playback, access settings and credentials
were unchanged. No canonical product checks were repeated for these diagnostic
artifacts; the preceding code change's 27-stage pass remains separate evidence.
The [archive](mp3-fidelity-probe/README.md) contains scripts, source/output hashes,
complete numeric receipts and review results, without media content.

This completes bounded objective characterization of the current encoder;
it does not close e8hd. My recommendation is to avoid another speculative codec
rewrite from these scores. Before release, resolve the remaining listening,
physical-source, recovery, delivery/endurance and distribution acceptance.
Quiet-input device checks and this host comparison cannot replace those gates.
