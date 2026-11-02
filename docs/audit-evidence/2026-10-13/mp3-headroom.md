# MP3 transform headroom

Near-full-scale PCM exposed a second Shine defect after the normalization fix:
the signed 32-bit MDCT accumulator can overflow. Two stereo input blocks suffice
to reproduce it. This also occurs in the original upstream encoder; it is
separate from the corrected transform gain.

The fix halves the normalized integer MDCT table and adds four to the emitted
quarter-power `global_gain` exponent. This reserves one internal bit while
preserving decoded scale. It adds no per-sample multiplication. A separate
UBSan failure in conversion of negative PCM to Q31 is fixed by multiplying by
65,536 instead of left-shifting a negative signed value.

## Evidence and scope

The exact `db8a086` baseline's 0.98-scale independent tones produce 85 invalid
final MDCT values among 221,184, reaching about ±2.344 billion. Its decoded
fitted gains fall to 0.593/0.403. The actual-wrapper regression first failed at
correlation 0.613455; the same thresholds pass after the correction.

An exact integer proof combines the actual filter rows before bounding their
absolute sums and includes every fixed-point rounding error. For arbitrary
signed-16-bit input and valid history, the corrected maximum MDCT prefix bound
is 1,750,915,446; the post-alias bound is 1,764,964,939. Both fit int32. The
alias numerator fits int64. A constructive full-scale witness produces an
unsafe baseline prefix of 3,501,775,918 and a corrected prefix of 1,750,887,950.
Independent C and Python calculations agree. All 648 corrected table entries
equal the baseline integers divided by two toward zero; other tables match.

The [proof](mp3-headroom-probe/bounds/report.md) describes assumptions and
exclusions. It does not prove arbitrary-input quantizer termination or table
indexing, target-generated table equivalence, or perceptual quality. Valid
quantizer steps yield compensated global gains in [87,214], within eight bits.
The existing quantizer loop's lack of an explicit upper guard remains outside
this transform change.

Sixteen actual-production fixtures cover independent, identical, inverted and
hard-panned tones, transients, independent noise, and low-level controls. Each
decodes to 110,592 stereo frames with finite float samples. The half- and
near-full-scale tone cases have gain within 0.5%, SNR above 70 dB and no samples
outside ±1. Four full-UBSan
codec runs, including the two-block reproducer, pass with empty stderr. The
constructive transform witness also passes full UBSan after the PCM fix.

| Corrected fixture | Left/right fitted gain | Left/right SNR |
| --- | --- | --- |
| 0.98-scale 997/1703 Hz tones | 0.999846 / 0.999855 | 74.92 / 75.13 dB |
| 0.98-scale independent noise | 1.025148 / 1.024417 | 12.61 / 12.63 dB |
| −60 dBFS tones | 0.999550 / 0.999716 | 50.31 / 49.76 dB |
| −90 dBFS tones | 0.998139 / 0.997897 | 30.96 / 30.25 dB |

The one-bit change costs about 0.04–0.07 dB of SNR on the −60 dBFS controls
and 0.41–0.47 dB at −90 dBFS. These are measurements, not audibility claims.
Near-full-scale noise still decodes to peaks of 1.541/1.507; the transient
fixture reaches 1.166. Floating output reveals this without hiding it behind
signed-16-bit clipping. Fixing integer overflow does not accept those fidelity
results. I recommend resolving codec quality before shipping MP3 as a finished
output profile; this remains a high-priority acceptance gap.

An independently patched exact upstream build matches production byte-for-byte
for the retained tones, noise and golden input. The golden SHA256 is
`d0209ba118ab74e5bab381c0e8be7142da2e48a1cbb6ed44762248def867d554`.
The [tail study](mp3-tail.md) separately measures the live encoder's delay,
finite cutoff and reset isolation. Manual Stop leaves the shared encoder running.

## Acquisition and review

A worker rerun overlapped source edits and moved a shared artifact directory.
That transitional result is excluded from baseline comparisons. A fresh exact
`db8a086` snapshot supplies the baseline; a frozen candidate snapshot supplies
the corrected results. The move interrupted one parent comparison with a
FileNotFoundError; its later retry passed. The first read-only board preflight
also failed on a misspelled `autoplay` field before any write; it was corrected.
These acquisition errors do not count as passing tests.

Two Sol 6.1 workers supplied the regression, characterization and independent
bound; another independently reviewed the production diff and proof with no
actionable finding. One characterization worker stopped on a tool-side content
flag; the parent completed candidate validation directly. No completed-worker
claim is inferred from its partial output.

All 27 canonical stages pass in `run.tqwOpLDl`. The exact hardware receipt and
remaining limits are recorded in [mp3-headroom.json](mp3-headroom.json).
Ordinary ELF `0ff2a7cc` is deployed and hash-verified. Two 60-second listeners
each decoded exactly 2,880,000 stereo sample frames. All nine capture windows,
including startup, pass; the eight initial queue overflows and 24,000 settling
discards remain explicitly recorded and did not increase. Existing MQTT
connection timeouts remain in UART; this does not claim MQTT works. The maximum
observed sender poll was 0.928 seconds. Quiet ADC cannot prove source continuity,
analog fidelity or the physical-source contention gate. Final boot
`42c314bb800c5d1cf6b9d8991cda7539` is stopped MP3/Medium, recognition/autoplay
off, listeners idle, with revalidated remote Sonos STOPPED/OK. No workstation
audio or access settings changed.

The [archived probes](mp3-headroom-probe/README.md) retain source hashes,
actual decoder metrics, bounds and red/green evidence. Raw local outputs remain
under `target/kata-hardening/oct13-mp3-headroom{,-fixed,-transitional}/`.
