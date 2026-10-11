# FLAC stereo decorrelation (45k9)

Status: implementation verified; Sonos compatibility blocked. Base d0b15aa; 20 open Katas. Original `kata show 45k9`
is the acceptance contract. This work follows Doctor Biz's approved audit:
independent stereo versus left/side only, preserving quantized 16-bit samples.

## Design and constraints

RFC 9639 sections 4.2 and 9.1.3 define side=L-R, reconstructed R=L-side,
assignment 8 and one extra side-channel bit. The encoder will compare bounded
subframe plans and prefer independent coding on ties. Independent verbatim
remains the upper bound: MAX_FRAME_BYTES stays 16,399, including renumbering.
No output-profile change, dither, per-block allocation or larger stream ring.

Reuse the existing L/R sample storage. Accumulate a third predictor-sum column
for the side channel, choose all plans before writing, then replace R samples
with L-R only when side wins. Keep 16/17-bit writer widths explicit. Measure
the extra predictor work and storage rather than assuming they are free.

Parent owns canonical verification, target builds, device/UART and commits.
Sol 6.1 agents own disjoint codec, decoder-test and benchmark files. Preserve
user-owned untracked .kata.toml, docs-ignore and the device-metrics plan.
No mixer/volume changes. End hardware state MP3/Medium/stopped with autoplay
and recognition off. No push or merge.

## Work and gates

1. Extend the existing corrected flac-cost benchmark with shared deterministic
   stereo fixtures, maximum per-block timing and bounded memory observations.
   Save the independent-codec baseline ELF before editing production encoder.
2. TDD codec plans, side-bit handling, selection, header validation and bounds.
   Add independent Claxon/FFmpeg exact reconstruction for identical channels,
   opposite polarity, hard pans, extremes and independent noise. Preserve
   frame-number/CRC validation and output allocation invariants.
3. Parent runs scripts/check and builds/runs matched ESP32 benchmark images.
   Retain bytes, runtime/wall maxima, heap/stack and pointer/capacity evidence.
   Decide adoption from measured costs and savings. Real Sonos compatibility
   and workload scheduling gates remain explicit; host decoding is insufficient.
4. Independent review, fix findings, record evidence, commit. Close 45k9 only
   against its original acceptance; no invented passing Sonos result or claim
   that compression resolves transport stalls.

## Coordination

| Work pair | Shared boundary | Resolution |
| --- | --- | --- |
| Codec / decoder tests | Existing Encoder and renumber API | Preserve signatures; new assignment is 8 |
| Codec / benchmark | Frame bound and constant frame | Keep 16,399 bound and 14-byte constant frame |
| Benchmark / baseline | Target compilation reads core source | Freeze codec until baseline ELF is saved |

Each task's tests use real encoded output. The benchmark runs without capture
or network and cannot by itself certify loaded-system scheduling or Sonos.

## Phase checkpoint

Codec, exact decoder tests, capture validator and matched device benchmark are
implemented and independently reviewed. All 27 canonical stages passed in
run.FQL4yLHq. The loaded encoder kept up without added capture faults, while
listener delivery failed. The bounded Sonos test failed control setup before
Playing, so original acceptance is incomplete and 45k9 stays open. No further
transport or control investigation belongs in this implementation checkpoint.

Evidence: docs/audit-evidence/2026-10-11/flac-stereo.md and companion JSON.
Board remains on the candidate unified image, MP3/Medium/stopped, autoplay and
recognition disabled. No volume changes; source and UART helpers cleaned up.
Next: resolve the Sonos control precondition under the existing recovery work,
then repeat only this remaining compatibility gate. Count stays 20 open.
Compaction count: 1 in this implementation phase; retire at this checkpoint.
