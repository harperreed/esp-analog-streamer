# Same-image observer crossover

Doctor Biz, use one diagnostic-only image with a boot-timed OFF / ON / OFF gate around the added metrics-state observer. Do not reuse or relax encoder-probe acceptance: its loss-sensitive advancement answers a different question. This experiment must finish its scheduled observation even when raw loss occurs, and must still report that loss as a failure.

Implementation contract reviewed independently after all four acquisition blockers were resolved. Parent owns builds, hardware and integration; workers own only assigned source or scratch files. The machine-local file `~/.codex/docs/local.md` is absent in this environment. Existing active HIL images and original evidence remain untouched.

## Why this experiment

The saved default has five complete MP3 windows near 0.785x with raw loss in each; the matched archived control keeps up near 1.0x with no raw loss in five windows. Both delivery trials fail. All 28 examined C hot-path bodies match apart from resolved literal-load relocations, but placement changed and some Rust literal targets remain unresolved. Neither this association nor the observed 8-us query maximum identifies execution cost. Diagnostic dump wall spans of roughly 1–1.4 seconds include preemption and logging.

A same-image runtime crossover holds linked code and data addresses fixed. It can test whether executing the added observer bundle changes progress in that image. It cannot prove layout caused the archived-image difference.

## Smallest intervention

Add compile-time diagnostic feature `metrics-state-crossover` in `firmware/Cargo.toml`, absent from normal builds. In `firmware/src/main.rs`, add `compile_error!` when both `metrics-state-crossover` and existing `encoder-probe` are enabled. Record enabled features in the build receipt; the analyzer rejects encoder-probe records rather than interpreting them as ordinary telemetry. In `firmware/src/audio_in.rs`, keep a single atomic phase index and a metrics-owned fixed elapsed schedule. No HTTP, NVS, settings, mixer or codec path. Do not change task creation, queues, stack sizes, priorities, encoder telemetry, capture reporting, sleeps or health handling.

Use boot-monotonic deadlines: settling until 20 seconds, OFF1 from 20–100 seconds, ON from 100–180 seconds, OFF2 from 180–260 seconds, then remain OFF. Metrics checks the schedule at the start of each existing loop, after the previous loop and any diagnostic render returned. These are experiment constants, not production settings. The saved default already shows an encoder export delayed by 11 seconds; allow a conservative 20-second transition delay, 10-second washout and up to 10 seconds of window alignment. An 80-second phase then has approximately 40 seconds available for complete windows, enough for three approximately 10-second windows with modest margin. This is a pragmatic budget, not a measured 20-second upper bound. Actual eligibility still decides sufficiency.

If startup MP3 is not established by the first retained window, keep the run invalid rather than silently moving the schedule. Loss counters, encoder windows, listener survival and dump success never decide advancement. A delayed metrics loop records the actual transition late; it must not synthesize past phases or emit an apparent success if it skips one. Fixed elapsed scheduling is not a hard real-time completion guarantee. Supervisor continues its unchanged health handling before optional sampling.
Gate all added observer work as a bundle:

- Every `METRICS_PHASE.store`, including Init and Sleep stores, through one small inlined helper whose only OFF work is the enabled load/branch.
- Supervisor `observe_metrics`, its state-query clock reads, count updates and `METRICS_OBSERVATIONS.publish`.
- New periodic `export_metrics_observations`, including totals, eight phase rows, completion and supervisor stack scan. Advance its existing periodic due gate even while OFF, so ON does not accumulate missed dumps.

Leave original capture/restart/listener/encoder exports running. Keep handle publication/setup identical in all phases. Do not reset observer counters between phases: receipts remain cumulative and independent. OFF receipts need not be emitted; phase transition evidence identifies suppression. Keep the existing 1-second sampling due gate advancing while OFF as well. No extra capture/encoder hot-path instrumentation is needed.

The gate itself has a small shared cost in all phases; OFF is therefore not byte-for-byte or cycle-for-cycle the archived control. Avoid a generalized state framework or a new coherent snapshot mechanism.

## Transition evidence and exclusion rules

Metrics is the sole schedule writer. At the start of a loop it compares board-monotonic time with the fixed deadlines, stores the resulting atomic phase index (settling/OFF1/ON/OFF2/finished), then emits a short transition record from metrics only. The record contains sequence, phase, planned deadline and actual gate-change producer timestamp. Immediately after that log returns, emit one short ready record with the same sequence and a new producer timestamp. Ready means the previous loop/render and the transition log returned; it is explicit evidence, not an inferred acknowledgement. The ready record's own formatting remains experiment overhead covered by washout. All records belong to the owned ROM-framed boot. Never log from supervisor or add any UART formatting to its high-priority path.

Supervisor loads the phase index immediately before optional query/publication; enabled means ON. On the current same-core arrangement supervisor has higher priority than metrics. Thus a supervisor query and publication already running complete before metrics can make a transition; metrics cannot change the gate underneath that block. Verify this core/priority arrangement in the actual code/build before relying on it. A change to that arrangement invalidates this simple design and requires a fresh decision, not an unmentioned synchronization assumption. Metrics uses its local phase for all stores and renders in that loop. No handoff, queue, lock or second gate is needed.

Emit explicit OFF1 transition/ready at its first eligible loop, even though settling was also disabled. At the first loop at/after 260 seconds emit finished transition/ready and remain disabled. This terminal pair proves schedule completion, not a final drain or audio pass. Transition/ready records are rare overhead present across phase boundaries. Preserve a render's actual completion if it delayed the next transition. Never substitute log-prefix or host UART arrival times for producer timestamps.

For each phase, a usable encoder window begins after its ready timestamp plus a fixed 10-second washout. Its producer end must precede both the next planned deadline and actual next transition. A late ready narrows or eliminates usable evidence. Require at least three complete windows per phase; otherwise report insufficient crossover evidence. Preserve every excluded window with a reason. Missing/malformed transition or ready, skipped phase, reset, active codec switch, incomplete UART or missing terminal pair invalidates inference, not historical loss reports. Record transition lateness numerically; lateness alone need not invalidate a phase whose remaining eligible evidence is sufficient.

## Frame rates and verdicts

Use existing ordinary encoder producer window/boundary timestamps and cumulative frame counts. Keep every prior complete report as a possible count anchor even if its window is excluded: subtract the count proven at this window's exact start, never the last eligible window's count. `Owner::report` advances its baseline even if telemetry publication fails; missing anchors reject the affected window and sticky loss rejects evidence sufficiency. MP3 generation ratio is `frame_delta * 1152 / 48000 / elapsed_seconds`. Do not divide by scheduled phase duration, UART span, logger time or dump time. Never split a window across a phase transition by prorating frames.

Startup FLAC-to-MP3 replacement resets the cumulative MP3 count; use the actual replacement baseline and exclude prior-format frames. A retained Overrun boundary that keeps the window baseline/count must not start a new denominator. A true generation reset or format replacement invalidates a crossing interval and establishes its recorded new baseline. Preserve each complete overrun, reset and truncated tail separately. Capture counter snapshots bracket usable intervals where available; do not turn stable capture counters into absence of encoder raw loss.

Report three separate outcomes: acquisition/cleanup validity, crossover evidence sufficiency, and existing audio/delivery acceptance. Loss does not halt the schedule. Raw loss, capture faults, telemetry sticky loss, listener failure, cancellation and cleanup failure remain explicit failures under their original rules. Cancellation produces a partial failed experiment; do not let a later timer completion clear it. Receiver accepted bytes are not decoded samples. A zero-listener fixed workload is the smallest causal first run; it cannot pass delivery acceptance, which was not exercised. Preserve existing failed delivery trials verbatim. Do not allow an existing listener-failure harness to kill this diagnostic prematurely: reuse `target/kata-hardening/listener-device-trial.py` ownership and cleanup for a new observation-only timed scenario, described below.

Interpretation: slow ON with recovery in both OFF phases supports runtime cost of this bundled observer in this image. All phases slow leaves layout, shared diagnostic overhead, workload and other state effects unresolved. All phases fast means this image did not reproduce the slowdown. An initial fast OFF and slow final OFF is not reversible evidence. Even reversible results do not assign cost to queries versus publication versus rendering; separate those only if this first experiment warrants it. Repeating the same image/sequence addresses run variability; a single sequence remains vulnerable to elapsed-time confounding.

## Concrete scope and numeric TDD

Target under 150 production lines: feature declaration; local phase-index/deadline helper and metrics-only transition/ready records; replacements of direct phase stores; two optional-work guards in `audio_in.rs`. Add focused host tests using the repository's existing core/firmware test inclusion pattern, plus the separate scratch crossover analyzer and fixtures listed below. The diagnostic analyzer must not change `encoder_probe_check.py` or its acceptance grammar. Counts are estimates in lines, not a promise before reading the final diff.

Before implementation, write failing real-number tests for the schedule and analyzer:

- At 99,999,999 us OFF1; at 100,000,000 ON; raw loss 5 still permits transition at 180,000,000 to OFF2. A jump over a phase is invalid, and cancellation remains failed after 260 seconds.
- 329 frames over 10.059091 seconds gives 0.784962x; 417 over 10 seconds gives 1.0008x. A 99–109 second window is excluded, not split. A ready record at 121 seconds makes 131 seconds the earliest allowed ON start; windows 140–150, 150–160 and 160–170 qualify before the 180-second deadline. Assert the actual accepted window list and ratios.
- Startup replacement frames2 followed by MP3 baseline0 must not subtract2. An overrun retaining frames329 and the original baseline must not restart its window. A real generation reset requires the recorded fresh baseline; missing baseline rejects rate inference.
- Missing OFF2 ready, duplicate/skipped sequence, reset, truncated terminal line, sticky loss and listener failure cannot yield a passed acquisition/audio result. Healthy elapsed progression with five raw overruns is valid diagnostic scheduling and failed audio acceptance.

Use actual records/numeric fixtures, not tests of mocked query timing. Test gate selection separately from hardware cost; host tests cannot prove query suppression in linked target execution.

## Bounded acquisition reuse: binding file scope

Modify only `target/kata-hardening/listener-device-trial.py` for harness integration. Add explicit `observe` selection; preserve `both` and `slow` branches and return-code checks unchanged. Put the current unconditional slow-UART work under its own selected branch. Observe uses the existing source launch/monitor, hash/copy provenance, API/routing snapshots, subreaper supervisor and bounded final cleanup. It runs no listeners, slow acceptance or encoder-probe checker. Retain the 420-second fixture minimum and existing 360-second acquisition deadline, which starts before source launch. Twelve seconds of source lead-in plus 300 seconds of collection leaves about 48 seconds for acquisition overhead. Final cleanup retains its separate bounds.

Add scratch helper `target/kata-hardening/metrics-state/crossover-uart.py`, owned as a child by the existing harness supervisor. This is the only new collector, approximately 90–130 lines including error handling/help. Its concrete interface is `--out DIRECTORY --seconds 300`, invoked with the harness's Python interpreter and supervised child launch. It does not own source playback or invoke any API. Reuse the existing `scripts/encoder_probe_check.py` byte reader mechanics and serial/reset sequence directly: `/dev/ttyUSB0`, 115200 baud, 0.2-second read timeout, reader started before reset, DTR false, RTS asserted for 0.1 seconds then released. Do not use `scripts/serial-log`, whose UTF-8 replacement output loses raw bytes. No generic serial abstraction or change to the probe checker is needed.

The helper writes `uart.log` as bytes; `uart-arrival.jsonl` has the existing `host_monotonic`, `offset`, `bytes` records, flushed with each chunk. `events.jsonl` records reset assertion/release with host-monotonic time. Only the reader owns UART/arrival writes. Reset release starts the fixed monotonic 300-second collection deadline; reaching it is the sole natural completion condition. Reader exceptions are sticky errors; interrupted/forced collection is never natural completion. At the deadline stop the reader, join with a finite bound compatible with its 0.2-second timeout, close the serial port and flush/close artifacts. A join/close/write failure makes completion failed. A cleanup path releases asserted reset if startup failed, recording that separately from the owned reset release.

`collector-result.json` is the typed receipt: kind `timed_uart_observation`, requested seconds, reset-release monotonic timestamp, natural deadline, completion monotonic timestamp, `natural_completion`, final raw-byte count, final arrival-covered byte count, reader/cleanup errors. It must be written on both success and failure where possible; a missing receipt is failure. Helper returns zero only for natural elapsed completion with no errors; this is its genuine process exit, not a fabricated probe result. The outer harness checks source and collector liveness throughout its wait, retaining its existing source/output-budget checks. Early collector death is failure. At expected natural exit it requires the typed receipt, genuine zero exit and the existing positive supervisor cleanup/root-status receipt. Bound waiting by the existing overall monotonic deadline. Do not kill the collector at 300 seconds and label that natural success.

Observe records its own typed scenario row: kind `timed_observation`, collector receipt path, natural completion and cleanup validity. `scenarios_passed` explicitly validates that row for `observe`; it continues checking real `returncode == 0` for the existing soak/slow scenario rows. No fake scenario child return code. Acquisition `passed` means source/UART/provenance/cleanup success only. The separate analyzer reports crossover sufficiency, raw/capture/telemetry faults and delivery `not_exercised`; it never converts a canceled encoder probe or zero-listener acquisition into an audio pass. Missing terminal transition/ready or insufficient windows remains failed/insufficient analysis after a naturally completed collection.

Add scratch `target/kata-hardening/metrics-state/crossover-analyze.py` and `test_crossover.py`. Offline import `load_uart` from `scripts/encoder_probe_check.py` with the same filenames to validate consecutive complete arrival coverage. The current module has no `select_owned_boot` function: copy only its small first-post-release reset-selection logic from `analyze_acquisition`, retaining byte offset, original line number and containing arrival metadata, and apply crossover grammar instead of calling its probe-specific `analyze`. Require a valid owned reset release; reject an unexpected first post-release reset candidate and any later reset rather than searching for a favorable boot. Preserve unterminated tails. Hash the collector, harness, analyzer, imported parser, feature/build receipt and ELF in acquisition/analysis manifests. Raw arrival timestamps still identify chunks, not individual producer times.

Listener absence is a bounded evidence limit. Existing `require_state` verifies stopped MP3/Medium, autoplay false and recognition false; it does not check active listeners, and this plan identifies no usable API listener-count field. Do not invent one or claim the snapshot proves absence. Reject observed contamination: any positive producer `listeners_start/end`, any listener join/close/departure evidence in this boot, or telemetry loss that could hide such events. Keep all loss counters and incomplete-tail failures. Even with zero retained observations, absence of transient listeners is not proven; describe the workload as no listeners intentionally opened, with no observed contamination. If the initial real state conflicts with the existing stopped/config requirements, reject without writing settings.

## Exact tests and budget

`core/tests/metrics_state_crossover.rs` can include a small pure schedule helper from `firmware/src/metrics_state_crossover.rs`, following the existing firmware-helper/core-test pattern. Keep the gate/transition integration and incompatible-feature rejection in `audio_in.rs`, so both the production binary and library-backed examples reject the combination. This extra tiny helper file makes exact deadlines/skipped-phase behavior host-testable without a firmware build; it is not a framework. Production scope remains approximately 90–140 Rust/TOML lines including the helper and gated stores, subject to final diff. Harness changes approximately 30–50 Python lines; collector approximately 90–130; offline analyzer/tests are additional scratch diagnostic code. Do not force an abstraction to meet these estimates.

Before hardware, `test_crossover.py` must cover raw bytes split through a UTF-8 sequence (byte-identical retention), consecutive arrival offsets, missing arrival coverage, missing release, unexpected first/later reset, natural typed completion, early UART/source death, interruption and supervisor cleanup failure. Test the collector's deadline/receipt decision as a pure function with numeric monotonic inputs; test byte framing offline with actual-format fixture bytes, not mocked serial timing. Existing subreaper child integration tests remain the ownership proof; real UART acquisition is the eventual integration check. Harness tests prove observe selection never falls through to soak/slow and rejects a fabricated return-code-only row while both/slow keep existing semantics. Analyzer fixtures cover excluded report as the next exact count anchor, Switched prior-format frames/reset, ordinary retained Overrun, missing anchors/ready/terminal, contamination and sticky loss. Compile checks prove the incompatible feature combination fails. Do not alter baseline/probe acceptance or claim host timing tests prove target behavior.

## Next verification and limits

Read the canonical project check instructions and current plan before editing. Run host tests and canonical checks, then inspect linked call sites to confirm OFF skips the intended queries/publications/render/stack scan and every phase store is gated. Compare all OFF/ON symbols within the single ELF: this is one image, not two feature builds. Inspect stack frames; no runtime margin claim follows from static arithmetic.

After source, harness, linked and canonical checks pass: save the exact diagnostic ELF/image hashes, capture an owned ROM-framed boot, retain raw UART bytes and decoder/source/cleanup receipts, and perform the bounded fixed-workload sequence. Verify actual timestamps, sufficient complete windows, all fault counters and minimum stack observations before causal interpretation. Do not add deployment or cleanup actions to this read-only design task.

The main residual limitations are moderate causal uncertainty from time/order and fixed gate overhead, and high uncertainty about a specific underlying cost or cache mechanism. This is diagnostic evidence only; known delivery and raw-loss failures still block shipping.


## Frozen firmware/analyzer record contract

Feature name: `metrics-state-crossover`; compiling it with `encoder-probe` fails.
Only this feature emits these records. Literal tokens and field order:

```
metrics-crossover: transition schema=1 seq=0 phase=settling planned_us=0 changed_us=123 skipped=false
metrics-crossover: ready schema=1 seq=0 phase=settling ready_us=456
```

Sequence/phase/planned_us are 0/settling/0, 1/OFF1/20000000,
2/ON/100000000, 3/OFF2/180000000, 4/finished/260000000.
Changed and ready timestamps use the existing board microsecond clock; example
123/456 above are illustrative. First observed phase must be settling, otherwise
skipped is true. A sequence jump also sets skipped true. Never synthesize missed
records. Ready follows the actual transition logger return. The analyzer rejects
skips, duplicates, missing pairs or mismatched phase/deadline values. Producer
intervals use ordinary encoder telemetry only. A partial unrelated UART tail is
retained and limits tail claims; an incomplete required transition or encoder
record rejects the affected evidence, never supplies a fabricated endpoint.
