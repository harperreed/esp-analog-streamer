# Finish the Kata hardening backlog

Doctor Biz's active goal on October 10: finish all Kata work, then assess.
Base c134069 on wip/kata-capture-protection. Compactions in this phase: 1.
Use GPT-6.1 Sol subagents with clean contexts. Parent owns review, integration,
canonical checks, commits and every hardware action. Continue through batches;
a phase checkpoint is not a reason to abandon the active goal.

## Completion rules

A Kata closes only when its actual acceptance criteria have evidence. Keep
hardware acceptance and implementation separate. Do not close an epic just
because code compiles, substitute a short run for a long soak, or claim a
physical calibration without measured input. Record concrete external blockers
while continuing all independent work. Read gotchas before hardware actions.

Preserve .kata.toml and docs-ignore. No push/merge, volume changes or automatic
FLAC firmware rollback. The final test state is stopped, MP3/Medium, autoplay off.
Capture input uses the confirmed front headphone jack; rear Line Out muted.
Hardware testing is authorized for the empty office weekend. Never run multiple
owners of UART/source/flash at once. Keep original failed trials.

## First batch

1. d2cb: bounded sender-call/writable-wait counters using the existing writer.
   Specification: 2026-10-09-sender-timing.md. One worker owns stream_http.rs
   and firmware stream.rs. Preserve behavior; test real sockets, then measure
   the board with the same matched workload and decide the next fix from data.
2. 5yf3: independent reference-model property tests in a new integration-test
   file, covering variable raw/encoded capacities and listener lifecycles.
   Existing dependencies are pinned; no production/manifests edited by this lane.
3. Reconcile all open issues against current code/evidence, then order remaining
   host hardening, capture/recovery and HIL work by dependencies. Read-only lane.

Ruling: these lanes run in parallel because only one edits existing production
code; the other adds independent tests and the audit is read-only. This follows
the conflict-avoidance intent of the implementation skill, plus Doctor Biz's
standing request to parallelize independent work. Parent commits reviewed changes.
Ruling: task plans and Kata acceptance are the working specification where no
separate spec exists. Routine hardening proceeds under the existing authorization;
new consequential architectural choices get a concrete design before code.

## Backlog at start

| Kata | Priority | Work | State |
| --- | --- | --- | --- |
| e8hd | P1 | Validate the experimental buffered MP3 pipeline on ESP32 | Reconciling |
| d2cb | P0 | Separate send-call delay from socket backpressure on the ESP32 | Committed 0d9d014; device comparison running |
| pd6c | P0 | Instrument and locate intermittent TCP delivery stalls | Reconciling |
| z5fs | P1 | Automate dedicated hardware-in-the-loop streaming validation | Reconciling |
| dp08 | P1 | Isolate capture publication from delayed raw-ring readers | Reconciling |
| dctv | P1 | Define bounded live-stream continuity and resynchronization policy | Reconciling |
| cn4m | P2 | Add channel diagnostics and calibrate jack-to-PCM fidelity | Reconciling |
| ed4k | P1 | Measure and narrow capture restart settling after short gaps | Reconciling |
| 93hm | P1 | Diagnose startup and TLS-related DMA capture service gaps | Reconciling |
| wmsy | P0 | Improve streaming recovery and validate independent failure handling | Reconciling |
| hv4x | P1 | Apply and verify a reproducible Wi-Fi protocol baseline | Reconciling |
| cxbw | P1 | Add pinned ESP-IDF heap, UART core-dump and selective timing diagnostics | Reconciling |
| 7hr4 | P1 | Test importable Python harness implementations instead of AST fragments | Implementing |
| ee3q | P1 | Validate 24-hour two-listener continuity and controlled recovery | Reconciling |
| 97bk | P1 | Make host, firmware and HIL evidence explicit streaming release gates | Reconciling |
| 89g8 | P0 | Build deterministic host replay of streaming failures | Reconciling |
| amqe | P0 | Add a separate ESP32 streaming dashboard and bounded metrics export | Reconciling |
| n4jb | P0 | Add reproducible network fault injection on the actual board traffic path | Reconciling |
| 1wny | P1 | Use cargo-mutants to check whether reliability tests detect wrong behavior | Reconciling |
| m31k | P1 | Measure host recovery-path coverage with cargo-llvm-cov | Reconciling |
| t4ae | P1 | Add bounded host fuzzing for FLAC, HTTP and stream transitions | Reconciling |
| jq3a | P0 | Model listener admission and shutdown interleavings with Loom | Implementing; heavy tests held during device measurement |
| 5yf3 | P0 | Add proptest state-machine tests for rings and listener lifecycles | Closed, 0d9d014 |
| 21tp | P2 | Move encoder and listener logs off progress and cleanup paths | Reconciling |
| qm2a | P2 | Measure and define FLAC16 quantization policy | Reconciling |
| 45k9 | P2 | Benchmark reversible stereo decorrelation for FLAC | Reconciling |
| bf85 | P1 | Run a six-hour reconnect and playback recovery soak | Reconciling |
| cv5a | P1 | Validate interruption recovery, ownership and sustained overload | Reconciling |
| 6vct | P2 | Track pipeline progress and enforce an explicit media-age budget | Reconciling |
| pqqq | P0 | Design ownership-safe Sonos playback recovery | Reconciling |
| 7vhq | P2 | Define bounded Sonos listener reservation or priority | Reconciling |
| 6hjs | P2 | Validate poll error and hangup handling in deadline writes | Reconciling |
| 8s9m | P2 | Prevent slow request headers from serializing stream admission | Reconciling |
| ewvq | P2 | Track Wi-Fi and IP generations across stream and control lifecycles | Reconciling |
| 37k3 | P2 | Defer Shazam requests until capture has fresh usable samples | Closed, 0d9d014 |
| wbz6 | P1 | Resolve diagnostic-image MP3 encoder throughput regression | Added from cold controls; investigating |

## Verification

First batch: sender diagnostics, property tests and recognition admission (37k3)
passed independent task and combined review. All 20 canonical stages passed on
the second run; the first exposed an ESP-IDF libc POLLNVAL omission, corrected
against the pinned platform header and generated bindings. Targeted red/green
recognition tests cover reset, configured timing and rate limits. Property tests
passed 512 cases per property and a separate 4,096-case run with seed 20261010.
Sender hardware overhead and matched measurements remain pending. Logs are at
target/kata-hardening/check-first-batch{,-fixed}.log. The backlog audit is retained at
target/kata-hardening/backlog-audit.md; no issue closes from static review alone.
Read-only planning for importable harnesses (7hr4) and listener Loom (jq3a)
continues while the parent validates this batch.

Second batch: importable harness modules, actual listener Loom models and real
poll departure tests passed task and combined review. The harness timing fixture
finding was corrected and re-reviewed. All 21 canonical stages passed, including
the target firmware build; log target/kata-hardening/check-second-batch.log.
The build locked optional host Loom dependencies in firmware/Cargo.lock; a target
cargo tree confirms they do not enter the default ESP-IDF dependency graph.

Device evidence: diagnostic three-trial run 174257, archived-image cold MP3
control 175027 and exact diagnostic cold repeat 175503 all completed cleanup.
Both images fail MP3 delivery, while diagnostic zero-listener encoding slows
to about 0.95x and loses raw history. Full receipts are in October 10 sender
analysis/control evidence. wbz6 blocks d2cb overhead acceptance; neither closes.
The diagnostic image is deployed, stopped MP3/Medium with autoplay off.

Each code task supplies targeted red/green evidence and independent review.
Run scripts/check on a frozen integration state. Hardware comparisons keep
source, radio policy, hashes, baseline counters and actual overlap intervals.
Keep task reports in the plan SDD workspace and public sanitized evidence in docs.
