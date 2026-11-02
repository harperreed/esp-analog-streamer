# Metrics-state crossover device evidence, 2026-10-10

Doctor Biz: trial 2 completes acquisition and supplies numeric phase coverage. Both trials still have actual encoder raw loss; neither exercises audio delivery. OFF1, ON and OFF2 each generate about 0.72x audio. These observations do not identify a sole cause, establish observer-overhead equivalence or clear audio acceptance.

Both runs use the same crossover-only image, built successfully from dirty revision `a4cc129f764fec73c4ca61495004887ef0b34de8` with `metrics-state-crossover`. Copied ELF SHA256 is `8ef23d4fae8ecd16013e3eace538b373e461f11e266a64ac9756d457a142922c`. Each saved image and build receipt matches its original acquisition hash. Full device attestation remains false; the build receipt preserves exact source hashes and command.

| Result | Trial 1 | Trial 2 |
|---|---|---|
| Natural 300 s collector completion | Yes; root exit 0 | Yes; root exit 0 |
| Wrapper / acquisition valid | False | True |
| Final numeric evidence sufficient | False, gated by wrapper failure | True |
| Audio passed | False | False |
| Actual Overrun boundaries / final resets | 38 / 38 | 38 / 38 |
| Complete encoder windows | 29 | 29 |
| Qualifying windows per OFF1 / ON / OFF2 | 6 / 6 / 6 | 6 / 6 / 6 |
| Delivery | Unexercised | Unexercised |

Trial 1 retains its completion-event failure: `TypeError("main.<locals>.event() got multiple values for argument 'kind'")`. Its collector completed naturally, but that fact does not repair wrapper passed=false or final acquisition_valid=false/evidence_sufficient=false. Trial 2 has wrapper passed=true, empty acquisition/cleanup errors and genuine acquisition_valid=true/evidence_sufficient=true. Successful collection clears only its acquisition gate; audio_faults_present=true/audio_passed=false remain separate.

All five transition/ready pairs appear in order without skipped phases. Windows qualify only when they begin at least 10 s after ready and end strictly before the next phase deadline. OFF1 uses W4–W9, ON W12–W17 and OFF2 W20–W25 in each run.

| Trial / phase | Generation ratio range | Weighted ratio | Mean encoder task runtime / encode wall per window |
|---|---|---:|---|
| 1 / OFF1 | 0.717238–0.729265x | 0.724442x | 9.587037 s / 9.015671 s |
| 1 / ON | 0.716100–0.729841x | 0.724459x | 9.601319 s / 9.020720 s |
| 1 / OFF2 | 0.717408–0.729831x | 0.725195x | 9.596701 s / 9.026371 s |
| 2 / OFF1 | 0.715827–0.728169x | 0.723949x | 9.614392 s / 9.039124 s |
| 2 / ON | 0.715967–0.728169x | 0.723700x | 9.611125 s / 9.030880 s |
| 2 / OFF2 | 0.716032–0.728307x | 0.724335x | 9.605718 s / 9.028961 s |

Ratios use actual producer spans, 1152 samples per MP3 frame at 48000 Hz, and cumulative frame differences. Ordinary MP3 count resets to 0 on format switch: trial 1's prior-format frames=2 and trial 2's frames=3 are excluded. First MP3 baselines are 4.483814 s and 4.562719 s respectively; subsequent complete endpoints anchor the next window. Overrun boundaries retain the ordinary window baseline and never become replacement anchors. No later format switch or GenerationReset occurs. All 29 windows per run have consecutive endpoints and independently recomputed deltas matching saved parent analysis.

| Phase | Trial 1 changed / ready (s) | Trial 2 changed / ready (s) |
|---|---|---|
| Settling | 0.145063 / 0.147751 | 0.145204 / 0.147885 |
| OFF1 | 20.406329 / 20.409010 | 20.505614 / 20.508297 |
| ON | 100.158451 / 100.161222 | 100.067361 / 100.070218 |
| OFF2 | 180.076872 / 180.079899 | 180.065602 / 180.068551 |
| Finished | 260.086354 / 260.089650 | 260.175760 / 260.179049 |

The per-phase values are descriptive, without a predefined tolerance or equivalence test. Task runtime includes charged interrupts; encode wall includes preemption. Similar roughly 9 s encode-wall totals cannot locate codec, scheduler or memory cost. No cache or sole-cause conclusion follows from these runs or their comparison with other images. No listeners were intentionally opened or observed, but zero endpoints do not prove continuous absence of brief churn. There is no decoded media or reclamation test.

Both runs have 38 actual Overrun boundaries and cumulative resets 1→38, including multiple overruns within some reporting windows. Last observed overrun events are 296.718587 s and 296.048498 s. Numerical phase eligibility does not remove these failures. Each run has 29 complete, monotonic, nonoverlapping capture reports. All qualifying phase windows have complete capture brackets; W1 lacks preceding capture and W29 lacks following capture.

| Retained capture counter | Trial 1, constant | Trial 2, constant |
|---|---:|---:|
| queue_overflows | 17 | 16 |
| software_dropped_frames | 480 | 480 |
| read_errors | 0 | 0 |
| restarts / overflow_restarts | 2 / 2 | 2 / 2 |
| supervisor_restarts / health_restart_requests | 0 / 0 | 0 / 0 |
| settling_discarded_frames | 26880 | 26880 |
| discontinuities | 2 | 2 |

These startup faults remain part of the evidence. Their differing overflow baselines cannot be subtracted as treatment effects, and stable sampled capture counters do not clear explicit encoder loss or prove coherent state between samples.

Observed free stack minima are metrics 1388 / encoder 5192 / supervisor 3108 bytes for trial 1, and metrics 1388 / encoder 5184 / supervisor 3092 bytes for trial 2. Capture task watermark is absent; listener path is unexercised. Complete encoder export receipts number 62 and 61, all with dropped/full/contended/poisoned=0 and sticky loss=false; no listener receipts exist. These values do not certify future stack safety, terminal drain or loss-free audio.

Each archived analyzer replays using its frozen encoder_probe_check and encoder_fault_check; all six archived helpers match acquisition-recorded hashes. Their numeric, phase, fault and ownership outputs match the saved parent reports. Trial 1's structural numeric coverage is gated false by its wrapper failure; trial 2 preserves true. The transitive stream_format import was not archived, so its checked current hash is recorded explicitly as a provenance limit. Later mutable helper changes are not substituted for frozen analyzer evidence.

Owned ROM banners start at byte 0 and reset markers at byte 28 / line 3. Reset releases are host monotonic 1129802.539359507 and 1130332.628220331. First arrival chunks are 816 bytes at offset 0, arriving at 1129802.634970644 and 1130332.723760591. Ownership checks find no later reset. Trial 1 has 116444 raw bytes / 509 complete lines, selecting 507 owned lines; trial 2 has 115913 bytes / 504 complete lines, selecting 502. Both arrival maps cover every raw byte with no tail. Host arrivals establish framing and ownership, not device execution time.

Both supervision receipts report root return code 0, cleaned=true and errors=[]. Source cleanup ends with return code 0 and empty aplay log. Parent reports sessions reaped, trial 1 wrapper exit 1 and trial 2 exit 0, with UART/aplay absent; the independent reviews perform no live hardware or process action. Source survival/routing does not prove ADC fidelity. Companion JSON preserves original artifact/helper/image/build hashes, analysis report hashes, full transition timing, ordinary window baselines and phase calculations; original manifests verify unchanged.

Recommendation: retain trial 1's acquisition failure, trial 2's valid acquisition, both raw-loss failures and unexercised delivery. The repeated near-0.72x values support further bounded controls; they do not establish root cause, overhead clearance or audio acceptance.
