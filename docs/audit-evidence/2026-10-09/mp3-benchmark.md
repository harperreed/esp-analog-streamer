# Original ESP32 MP3 feasibility benchmark

Date: 2026-10-09. Kata: `e8hd`. Production source: `9550f65`.

**Recommendation: continue with a 320 kbps, codec-exclusive prototype; do not
adopt this encoder in production yet.** Shine reduced the supplied music's
payload rate by 52.59% relative to our FLAC encoder. It kept up on average with
capture and three local TLS cycles when FLAC was absent. Running both encoders
fell behind before any audio HTTP clients joined. The exclusive test still had
a 508 ms encoding call during the TLS phase and has little CPU headroom.

This was a disposable benchmark. Production dependencies, codec selection and
HTTP routes did not change. MP3 input was synthetic PCM, not a consumer of the
live capture ring. There was no MP3 network stream or Sonos playback test.

## Candidate and method

[Shine](https://github.com/toots/shine) is pinned to
`ab5e3526b64af1a2eaa43aa6f441a7312e013519` (`3.1.1-59-gab5e352`).
The target was the original ESP32 rev 3.1 at 240 MHz with 80 MHz PSRAM, ESP-IDF
5.5.5 and Xtensa GCC 14.2.0_20260121. The C build reused the actual IDF flags,
with `-O3 -fsigned-char`. A diagnostic example linked a scratch static archive.
No host transcoding was used for board cost measurements.

Both settings encode 48 kHz stereo, 1,152 samples per channel per frame: 24 ms
of input. C timers surround the encoder call alone; fixture generation, checksum,
logging and pacing have separate measurements. Wall time includes preemption.
Task runtime includes charged interrupts and is not pure encoder CPU time.
Selected fixtures and observed maxima do not establish worst-case bounds.

Upstream describes Shine as lacking a psychoacoustic model. Its generic target
code also uses floating-point math despite the fixed-point description.
The pinned COPYING file contains the GNU Library General Public License v2;
identifying it is not a distribution/relinking compliance review.
[Upstream limitations](https://github.com/toots/shine#limitations)

## Board results

The isolated worker ran on core 1, priority 7, with a 16 KiB stack. Each case had
one warmup and 128 measured blocks. Silence, DC, tones and independent noise
were tested at both bitrates and both allocation placements. All measured
isolated calls completed within 24 ms.

| Independent noise | Mean encode | Maximum encode | Encoder heap |
|---|---:|---:|---:|
| 320 kbps, internal | 13.610 ms | 13.886 ms | 102,400 B internal |
| 192 kbps, internal | 13.550 ms | 13.805 ms | 102,400 B internal |
| 320 kbps, PSRAM | 18.046 ms | 18.369 ms | 100,352 B PSRAM |
| 192 kbps, PSRAM | 17.947 ms | 18.211 ms | 100,352 B PSRAM |

These allocations exclude the task stack, 4,608-byte PCM buffer, capture history,
transport history and system services. The internal allocation exceeds the free
internal memory observed during normal startup/TLS operation; PSRAM was used
for load tests. Teardown returned both encoder allocations. Stack watermarks are
cumulative observations, not independent per-case stack requirements.

The corrected codec-exclusive run retained production capture, raw-ring storage,
Wi-Fi, web, MQTT and Sonos control setup, but omitted the FLAC worker/server in
the diagnostic entry point. It encoded continuous deterministic noise at 320 kbps.

| Corrected exclusive phase | Blocks | Mean encode | Maximum | Calls over 24 ms | Final backlog |
|---|---:|---:|---:|---:|---:|
| Baseline, about 30 s | 1,250 | 20.687 ms | 25.411 ms | 4 | 0 blocks |
| Three TLS cycles, about 40 s | 1,667 | 21.374 ms | 508.374 ms | 88 | 0 blocks |

Worker runtime was about 87.2% and 88.4% of those phase windows. It caught up
after late blocks; zero final backlog does **not** mean every deadline was met.
The TLS-phase maximum finish lateness was 490.939 ms. All three controlled TLS
connections completed. Capture counters stayed at the startup baseline: 14
queue overflows, 240 software-dropped frames, one restart/discontinuity and
28,800 settling-discarded frames. Thus the measured load added no reported
capture faults; boot was not fault-free. Background MQTT connection timeouts
also occurred and remain part of the recorded workload.

With production FLAC running concurrently, the MP3 mean was already 30.18 ms
before HTTP audio clients. It grew to about 36.02/42.81 ms with one/two FLAC
listeners, and 47.77 ms during the TLS phase. Over 150 seconds it encoded
3,693 of 6,250 due blocks: 61.368 seconds of accumulated backlog. Generated
samples were not skipped. The two FLAC clients independently decoded exactly
115/85 seconds (5,520,000/4,080,000 stereo sample frames), with no checked
decoder frame-number warnings or decoded timestamp discontinuities. These
checks cannot detect repeated or omitted capture content with continuous
timestamps. Capture counters did not increase from
that run's separate startup baseline. This rejects coexistence for the tested
implementation; it is not a universal proof that dual-codec operation is impossible.

## Audio and bandwidth

Independent FFmpeg decoding counted PCM bytes/samples, never output timestamps.
Fixtures included tones, fades, independent noise, synthetic music and 10.08
seconds of supplied recorded music. The actual board additionally exported 32
continuous noise frames at each bitrate. Both board outputs decoded to exactly
36,864 samples per channel and matched host encoding of the same input byte for
byte. Negative controls rejected missing export chunks, corrupt hex and missing
end markers.

Measured alignment delay was 1,057 samples, about 22.02 ms. An equal decoded
sample count includes this startup delay and does not preserve the last 1,057
source samples without explicit draining/padding. Shine's flush did not add
tail frames or gapless metadata in these tests.

| Supplied music | Payload rate | Reduction versus our FLAC | Raw SNR | Gain-adjusted SNR |
|---|---:|---:|---:|---:|
| Current 16-bit FLAC | 84,362.68 B/s | — | Bit-exact | Bit-exact |
| MP3 320 kbps | 40,000 B/s | 52.59% | 18.11 dB | 39.22 dB |
| MP3 192 kbps | 24,000 B/s | 71.55% | 17.60 dB | 26.91 dB |

Bandwidth uses an identical 9.984-second prefix with whole FLAC and MP3 frames,
the actual repository FLAC encoder, and excludes network overhead and FLAC's
once-per-stream 42-byte header. Fidelity uses the longer delay-aligned fixture.
FLAC decoding was bit-exact for all four comparison fixtures. On noise, FLAC
used 179,298.58 B/s; its rate depends strongly on content.

Tonal/music/fade MP3 output showed roughly +1 dB fitted gain; the cause is
unresolved. Raw and gain-adjusted scores are both retained, with no silent
normalization. Noise fidelity was substantially worse at 192 kbps while its
isolated CPU cost barely changed. These objective measurements do not substitute
for listening tests or justify a production gain correction.

All measured MP3 frames had `main_data_begin=0`. Sixteen fresh FFmpeg decoders
joining at selected frame boundaries produced the expected sample counts.
Initial filter warmup could differ through sample 1,042; output matched the
full-stream decode after two frames in every case. This verifies representative
software decoder joins, not arbitrary byte joins or Sonos receiver behavior.

## Measurement corrections retained

- The first mixed-run cleanup read the entire framehash file at once and hit the
  live reader's backlog limit. The original failed harness verdict is retained.
  A bounded completed-file check using the production record validator proved
  both full sample counts, reproduced the original failure, and rejected omitted
  and duplicated records. The archived runner now drains framehash incrementally.
- The original probes assumed short Rust sleeps yielded. In pinned IDF
  `components/newlib/src/time.c`, sub-tick `usleep` busy-waits. The exclusive
  repeat used actual `vTaskDelay(1)` calls and bounded catch-up bursts. Both
  source versions and results are retained. The original exclusive worker's
  roughly 99% runtime is not an encoder CPU estimate. The mixed run was mostly
  continuously overdue; its measured encoder times and backlog still establish
  failure for that tested configuration, but not idle headroom.

## Remaining adoption gates

High priority: connect the real capture consumer to one shared MP3 encoder,
bound output history, measure media age and stalls, and exercise one/two MP3
listeners, slow readers, actual Sonos joins, manual Stop/source ownership and
controlled network faults. The lower offered rate does not locate or prove a
fix for the existing radio/TCP stalls. Retain the existing capture release gates.

Before shipping: resolve or characterize gain/headroom and tail handling,
evaluate audible quality, review the pinned library's allocation-failure paths,
license and maintenance obligations, and measure whole-profile memory. Our
diagnostic allocation wrapper is not a production allocation policy. A 320 kbps
history costs 40,000 payload bytes per second, but none was built here.

The existing benchmark phase is complete. Kata `e8hd` remains open for these
unperformed integration/receiver acceptance criteria. The recommended next
experiment is a codec-exclusive 320 kbps prototype, not a default codec change.

## Receipts and restoration

- [Host measurements](mp3-host.json): fidelity, sample counts, device byte checks,
  joins and actual FLAC comparison.
- [Device measurements](mp3-device.json): per-case/per-phase observations,
  capture baselines, artifact hashes and original/corrected verdicts.
- [Reproduction sources](mp3-probe/README.md): disposable probes and source hashes.

Every flash experiment restored the saved production ELF. The final restore
boot reported `0a8ee33c9…`; the saved ELF SHA256 is
`0a8ee33c9a74587acd05ff23e44a0ace59c666fdc40c8704b3d7c9a3c974f4cf`.
The API reported stopped, autoplay false, Medium buffer and the saved destination.
The workstation PCM device was closed and rear Line Out muted after cleanup.
No Sonos volume writes or Play calls were part of this benchmark.

`scripts/check` passed all 20 stages after temporary examples were removed,
including host tests, Loom, decoder negative controls, firmware compilation and
IRAM verification. The suite retained two existing ignored tests (tap-table
printing and a separately provisioned music fixture); no
production code changed. The completed-file validator's separate self-test also
reproduced the old backlog failure and rejected omitted/duplicated records.
