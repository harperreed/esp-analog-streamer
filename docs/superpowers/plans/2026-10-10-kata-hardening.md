# Finish the Kata hardening backlog

Doctor Biz's active goal on October 10: finish all Kata work, then assess.
Base c134069 on wip/kata-capture-protection. Compactions in this phase: 14; latest firmware implementation checkpoint is 7c4ed7b (refusal deadlines; device admission acceptance remains open).
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
| 7hr4 | P1 | Test importable Python harness implementations instead of AST fragments | Closed, b68318b |
| ee3q | P1 | Validate 24-hour two-listener continuity and controlled recovery | Reconciling |
| 97bk | P1 | Make host, firmware and HIL evidence explicit streaming release gates | Reconciling |
| 89g8 | P0 | Build deterministic host replay of streaming failures | Full local replay gate reviewed and passed; first hosted PR run pending |
| amqe | P0 | Add a separate ESP32 streaming dashboard and bounded metrics export | Reconciling |
| n4jb | P0 | Add reproducible network fault injection on the actual board traffic path | Reconciling |
| 1wny | P1 | Use cargo-mutants to check whether reliability tests detect wrong behavior | Closed, runner 823c0cc and pilot evidence 7b05adf |
| m31k | P1 | Measure host recovery-path coverage with cargo-llvm-cov | Two local pilots passed; CI written/reviewed, hosted run pending |
| t4ae | P1 | Add bounded host fuzzing for FLAC, HTTP and stream transitions | Closed with f57aa3e/ba5b842; hosted execution remains in 97bk |
| jq3a | P0 | Model listener admission and shutdown interleavings with Loom | Closed, b68318b |
| 5yf3 | P0 | Add proptest state-machine tests for rings and listener lifecycles | Closed, 0d9d014 |
| 21tp | P2 | Move encoder and listener logs off progress and cleanup paths | Reconciling |
| qm2a | P2 | Measure and define FLAC16 quantization policy | Reconciling |
| 45k9 | P2 | Benchmark reversible stereo decorrelation for FLAC | Reconciling |
| bf85 | P1 | Run a six-hour reconnect and playback recovery soak | Reconciling |
| cv5a | P1 | Validate interruption recovery, ownership and sustained overload | Reconciling |
| 6vct | P2 | Track pipeline progress and enforce an explicit media-age budget | Reconciling |
| pqqq | P0 | Design ownership-safe Sonos playback recovery | Concrete design; LAN join and eviction proof pending |
| 7vhq | P2 | Define bounded Sonos listener reservation or priority | Reconciling |
| 6hjs | P2 | Validate poll error and hangup handling in deadline writes | Closed, b68318b |
| 8s9m | P2 | Prevent slow request headers from serializing stream admission | Reconciling |
| ewvq | P2 | Track Wi-Fi and IP generations across stream and control lifecycles | Reconciling |
| 37k3 | P2 | Defer Shazam requests until capture has fresh usable samples | Closed, 0d9d014 |
| wbz6 | P1 | Resolve diagnostic-image MP3 encoder throughput regression | Repeated without listeners; root cause unresolved |

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

Third batch: the shared write-progress operation preserves real socket behavior
and supports deterministic actual PCM/FLAC/ring replay. Seven replay tests and
31 writer tests passed, followed by all 22 canonical stages. Review corrected
one test's reuse after a terminal error. Full production orchestration and shared
FFmpeg validation still remain for 89g8; see 2026-10-10-deterministic-replay.md.
The isolated host coverage wrapper passed review and its lightweight tests;
review added explicit host targets against inherited cross-compilation settings.
Mise's llvm-tools-preview alias caused a new missing-tool warning despite an
installed component. The canonical llvm-tools name resolves it without changing
Rust 1.93.0 or LLVM 21.1.8; repeated activation is clean. The real coverage pilot
passed 380 tests with two documented existing ignores, producing JSON and HTML.
An inherited Xtensa target did not change its explicit host test/report target.
A clean committed repeat and the separate CI job remain acceptance work.

Paired no-listener controls old-1/diag-1 each contain 60 seconds of complete
encoder windows. Archived image: 1.00010x, no raw loss; diagnostic: 0.96376x,
one raw overrun/reset. Both have zero interval capture fault deltas, from nonzero
startup baselines. Sanitized receipts are encoder-cold.md/.json in October 10
evidence. This confirms the association, not a code/cache cause. Hardware is
idle on the exact earlier diagnostic ELF, stopped MP3/Medium, autoplay off.

Sonos recovery design is recorded in 2026-10-10-sonos-recovery.md. Before enabling
automatic media actions, prove the local join-only wire contract and rejection
of writes to evicted sessions. No resume API or ownership guarantee is invented.

Third batch committed 9b7f363. Clean committed coverage repeat passed with fresh
profiles and the same 380 passing tests; receipt committed as 5d43b1f. Fourth
batch adds the separate GitHub-hosted coverage workflow and bounded Linux
mutation runner. Both passed independent review; all 23 canonical stages passed
without the earlier activation warning. Workflow action pins/inputs and mise
bootstrap PATH were verified, and actionlint passed. No push or hosted job ran.
The mutation tool's real unmodified baseline passed eight tests and generated
five candidates. The full mutation run must follow a clean reviewed commit;
inventory alone is not evidence that assertions catch mutations.

The full replay orchestration brief passed two design reviews after correcting
four contracts: blocking locks remain; virtual replay assumes uncontended steps;
deadline/retirement observation boundaries remain exact; pending runtime results
are consumed once; delayed close retains socket, Slot and Lease until completion.
Implement that extraction only after the current mutation input is snapshotted.
A separate diagnostic brief will define encoder timing and allocation receipts
without changing priorities, codec quality or buffers during attribution.

Each code task supplies targeted red/green evidence and independent review.
Run scripts/check on a frozen integration state. Hardware comparisons keep
source, radio policy, hashes, baseline counters and actual overlap intervals.
Keep task reports in the plan SDD workspace and public sanitized evidence in docs.

## Fifth-batch checkpoint

Fourth batch is committed as 823c0cc. All 23 canonical checks and independent
reviews passed before that commit. The mutation pilot completed from an immutable
archive of that revision: five attempted, four caught, one unviable, no missed
mutations or timeouts. Independent receipt review confirmed intended assertions
and the separate compiler failure. This establishes only the capture-health
pilot scope; see host-mutations.md/.json in October 10 evidence.
The approved full replay contract now lives in 2026-10-10-deterministic-replay.md;
one Sol 6.1 worker owns the coupled production extraction and characterization.
A separate Sol 6.1 lane prepares a bounded fuzzing design without installing or
changing tools. Parent owns integration, reviews, canonical checks and hardware.

The third-compaction checkpoint is saved here rather than relying on conversation
state. This environment cannot start a replacement root session; fresh-context
workers continue from these durable plans under the active goal. No new hardware
run has started; the earlier diagnostic ELF remains stopped MP3/Medium.

Full replay extraction now uses the actual encoder/listener/runtime and wait
operations. Independent review approved clock boundaries, full-tick sleep,
close-before-release ownership, and the unchanged departure-log parser contract.
The artifact oracle passes real FFmpeg controls and preserves failed receipts.
The first 25-stage canonical check passed. Scheduler review then found two P1
false-pass risks: accepted-input accounting could omit capture ranges, and
listener expectations could follow whichever publications arrived. Its fix wave
adds exhaustive source categories, exact per-actor fixture expectations, and
named omission/duplication controls. Disconnect/stale-ID, same-graph reset and
partial-block runtime cases were also corrected. The final targeted run passes
nine Rust tests, 14 oracle positives and 12 intended negative rejections at
target/stream-replay.N21tfn. Scoped rereview approved all five fixes; the fresh
canonical run passed all 25 stages, including firmware compilation, with its own
replay artifacts at target/stream-replay.uHZRMM. See replay.md in October 10
audit evidence; no firmware was flashed.

The separate replay PR workflow passes actionlint and retains per-run evidence.
It has not run on GitHub; no push is authorized. Fuzz review likewise found a
timeout-cleanup defect and weak route/write assertions. A Sol 6.1 fix worker is
finishing those checks after two earlier worker turns hit unspecified automatic
content-risk filters. Saved partial work is not accepted evidence. The scoped
February nightly leaves stable and ESP toolchain pins unchanged; final fuzz
acceptance requires a reviewed committed-source archive.

Ruling: continue with clean-context workers after this fourth compaction because
the environment offers no replacement-root-session action. Preserve checkpoints
and reports rather than imply a fresh session started. The next encoder probe
brief has been adapted to the shared pump in target/kata-hardening/
encoder-probe-adapted.md; it remains design-only, with no source or hardware work.

Replay is committed as 32c2300 and its Kata comment points to the durable receipt.
The fuzz fix reviews approved independent HTTP/write assertions, nested timeout
cleanup and strict child-shell failure propagation. The latter caught another
false-success path: exported functions do not inherit shell strict options.
Real ordinary-process tests now reject that case and preserve evidence. Corpus
attributes preserve exact binary inputs; seed bytes remain unchanged. Scoped
Clippy, formatting and actionlint pass, as do all 27 canonical stages after adding
lightweight supervisor/guard and stable helper tests. Log:
target/kata-hardening/check-fuzz-batch.log. Full ASan acceptance must use the next
clean committed snapshot; the earlier mixed-input run remains development-only.
The adapted encoder diagnostic plan passed review after explicit per-phase
settling, reset/replacement invalidation and diagnostic-only report ordering.
Implementation waits until the fuzz input archive is immutable.

Fuzz implementation is committed as f57aa3e. The actual ASan pilot passed from
its immutable archive: 20,788 FLAC cases, one million HTTP cases and one million
stream-operation cases, status 0. Independent receipt review verified tool,
source and lock hashes, all 40 original seeds, all three instrumented builds,
and byte-exact preservation of all 489 archived files. No finding artifacts were
produced. Durable receipts: October 10 host-fuzz.md/.json. Hosted workflow and
upload execution remain explicitly pending in release-gate Kata 97bk.

The next worker implements the reviewed default-off encoder diagnostic in
2026-10-10-encoder-boundary-probe.md. Fuzz inputs are already frozen, so this does
not alter acceptance. The first phase requires 80 seconds; earlier MP3 raw loss
may prevent a full valid sequence. Retain invalid phase/window receipts and bound
the device experiment instead of restarting indefinitely or tuning away faults.
A separate evidence search read all 16 files in docs-ignore/victrola-hacks.tgz
without modification; it did not establish a LAN join-only command or stale-write
rejection. The observed failed suspend targeted a replacement empty session,
not the evicted board session. Automatic recovery writes remain gated by pqqq.

The default-off encoder probe passed independent source review, feature/default
host suites, all 27 canonical stages, target feature compilation and feature IRAM
verification. The query symbol is absent from the saved default ELF and present
in the saved diagnostic ELF. Build receipts are in October 10
encoder-probe-build.md. No flash has occurred. A separate worker is implementing
the bounded collector: settle input before reset, require enough source duration,
preserve all UART/state/failure receipts, and derive phase totals from consecutive
complete windows. A 300-second source is too short for its proposed 360-second
collection allowance plus setup/final receipts; do not silently loop it.

Probe implementation is committed as 5460cac. The fifth-compaction checkpoint
continues with clean-context Sol 6.1 workers; no replacement root-session action
exists. Collector review found missing final receipts on timeout, incomplete
storage validation, same-chunk UART ordering ambiguity and unrecognized WAV
listener churn. Correct those before hardware. The separate offline noise
generator passed four actual-WAV tests. Its explicit 420-second run produced
20,160,000 stereo frames; SHA256
9311d9ca5dfe1a5f9269631afe22647834207f2b3239571474acc8ac15f3228d.
This is a new PCG64 recipe, not the historical noise fixture. Hardware preflight
still shows stopped MP3/Medium, autoplay and recognition off, front Headphone
enabled and rear Line Out muted. No probe flash or source playback has occurred.

A read-only 8s9m design is saved at target/kata-hardening/admission-design.md.
It proposes two persistent header workers without increasing the two-listener
cap. Extra stack headroom and pre-admission close ownership need resolution;
there is no implemented admission change or measured latency result yet.

Collector review fixes are complete, including actual startup log order and
post-completion validation. Final cleanup also handles unstarted helper threads
and receipt-writing failure without skipping real source termination. Tests
reproduced those failures with ordinary child processes. Independent review
reports no unresolved source findings; all 22 collector, four noise and 11
existing harness module tests pass alongside the existing parser checks. The
first and final canonical runs both passed all 27 stages; the latter includes
the cleanup fixes (target/kata-hardening/check-probe-collector-final.log).
Before claiming device acceptance, retain actual probe UART/arrival/state data
and add a real replay regression. The synthetic host records do not prove that
integration. Parent alone will flash the saved probe ELF and run one bounded
trial, preserving failure if a full sequence cannot survive.

Collector/noise work is committed as 26d21c1. The actual 360-second probe trial
failed with raw overruns at device 166116 and 346626 ms; neither sequence reached
the retained OFF2 span. Its original timeout, source cleanup and unchanged
stopped MP3/Medium state are preserved. Six retained OFF1/ON windows generated
0.993720885/0.980450643x in sequence 1 and 0.989639746/0.985457331x in sequence 2.
Zero later capture fault deltas do not clear those encoder losses. Durable trial
receipts and selected real UART replay are independently reviewed. Review found
a provenance-length test gap; zip(strict=True) rejects the shortened mapping
that previously passed. The restored real fixture passes.

The saved default unified ELF then ran the identical source and collector for
one fixed budget. Its missing probe feature necessarily produces the original
timeout and completed=false; assess ordinary encoder/capture counters separately,
without converting that acquisition negative control into a probe pass. The
run has ended and source/UART are released. Independent full-window analysis
finds 32 capture-covered windows, 320.287614 seconds and 13,345 MP3 frames:
0.999976228x generation, no raw overruns or resets and zero capture fault deltas.
The final encoder window lacks a later capture bracket and stays excluded.
The feature OFF path cannot substitute for the normal-image baseline; causation
and listener delivery remain unresolved. All 27 canonical stages passed, including
the real rejected-trial replay and firmware build; log is
target/kata-hardening/check-probe-evidence.log. Independent control review verified
artifact hashes, retained/rolling totals, capture brackets and the expected
timeout with no findings. Hardware retains MP3/Medium, stopped,
autoplay and recognition off. Do not close wbz6 or d2cb from this control alone.

Next bounded unit: 21tp listener telemetry must not delay checked close or slot
release. Keep encoder/probe export out of this first slice while the OFF-path
audit proceeds. Capture fixed-size records without waiting; the existing metrics
task formats them after removing them from bounded storage. Explicit overflow
must invalidate diagnostic acceptance. No new task, playback policy, socket
ownership policy, per-frame allocation or firmware fallback is authorized by
this unit. Existing audit authorization covers this routine implementation;
the broader recovery and admission architecture decisions remain separate.

Evidence checkpoint is 0ef0e3e. Listener-only 21tp implementation is uncommitted;
its durable brief is 2026-10-10-listener-telemetry.md. The first full canonical
run passed all 27 stages (check-listener-telemetry.log). Independent task review
still requests three acceptance fixes: validating a different UART snapshot
from the success predicate, malformed/missing receipt fields, and remaining
raw-close waits without batch completion. Fix round 1 goes to the original
Sol 6.1 implementer. No new firmware has been flashed. Hardware remains on the
saved normal control image, stopped MP3/Medium with autoplay off.

Fix round 1 addressed all three findings with real-file and production-helper
regressions. Independent scoped review reports no new finding. The final
canonical run passed all 27 stages (check-listener-telemetry-final.log); final
harness coverage includes 25 probe, four noise and 18 module tests. Both target
builds pass, feature IRAM checks pass and the allocation-query symbol remains
feature-only. The target queue symbol occupies 2008 bytes; observed DRAM data
plus BSS grows by 2008 bytes against the saved normal control. This is not a
live heap or stack margin measurement. See listener-telemetry-build.md evidence.
Next: parent-owned bounded MP3 soak/slow-reader validation, retaining separate
scenario failures and measured cleanup/stack/heap results. No hardware run or
full 21tp completion is claimed at this implementation checkpoint.

Listener implementation and acceptance fixes are committed as 3aa5b5f. Parent
flashed the checked default unified ELF; flash completed and postflash GETs
verify stopped MP3/Medium with autoplay and recognition off. Before playback,
parent found the throwaway trial wrapper could leave UART/FFmpeg descendants
behind because the soak helper starts separate sessions. Correct and test owned
descendant cleanup before running either scenario. A clean-context Sol 6.1
worker is independently designing the remaining encoder telemetry slice.

Trial 1 stopped in preflight before audio/reset: duplicate event timestamp and
mono Master validation bugs in the throwaway wrapper. Both have real-receipt
regressions; original evidence remains intact. Trial 2 failed MP3 delivery: both
listeners fell behind, while two checked closes had valid later zero-loss batch
receipts. Its final UART line is incomplete and cannot extend capture coverage.
The separate slow-only trial 3 reached a write timeout and decoded ten frames
on a later connection, but the fast stream failed and export reported one lost
record. Neither run clears delivery or full 21tp. All helper/source cleanup
receipts passed; hardware remains stopped MP3/Medium, autoplay/recognition off.

Measured metrics stack minimum is 420 bytes out of 4096. Linked inspection finds
an 800-byte persistent metrics frame (capture formatting is inlined) beneath the
992-byte listener exporter frame. Ruling: test only an out-of-line boundary on
existing log_capture before adding encoder telemetry; preserve task sizes,
priorities, output and sampling order. Actual target call/frame comparison and a
matched hardware repeat are the meaningful checks, not a host test asserting an
attribute's presence. No overflow or transport cause is established by this
stack measurement. The encoder telemetry design remains gated on that result.

Failed-device evidence is independently reviewed and saved as
docs/audit-evidence/2026-10-10/listener-telemetry-device.md and paired JSON.
The reviewer verified all 30 original hashes, complete-line capture coverage,
actual decoded samples and strict rejection of trial 3's sticky export loss.
The one-line stack experiment builds in both configurations: metrics now reserves
208 bytes, separate capture reporting 528, and listener export 992. The nested
caller/export reservation drops from 1792 to 1200 bytes, while caller/capture
drops from 800 to 736. These are linked frame sizes, not measured runtime savings;
canonical verification, independent review and a matched device repeat remain.

Device evidence checkpoint is cf03fd8. The stack-only change passed all 27
canonical stages (check-metrics-stack.log), separate probe IRAM checks and
independent source/linked-call review. Default artifact SHA matches the canonical
output. Build receipt: docs/audit-evidence/2026-10-10/metrics-stack-isolation.md.
Next: commit the reviewed boundary, flash its exact default unified ELF, and
repeat bounded two-listener and slow-only measurements with the same source.
Do not close 21tp or add encoder output based on linked sizes alone.

Stack boundary committed as 3b3afdb and flashed. Both bounded repeats measured
1188 bytes minimum free metrics stack, versus 420/580 in prior soak/slow trials.
Both still failed delivery; the slow repeat failed before third admission, so
its zero-loss record set is smaller and cannot clear the original loss. Strict
complete-line analysis and all 27 evidence hashes passed independent review.
See metrics-stack-device.md and JSON in the October 10 evidence directory.
Source/UART cleanup passed; stopped MP3/Medium, autoplay/recognition off remains.

Next unit: distinguish full, contended and poisoned listener publication drops
without changing capacity or scheduling. Binding brief:
2026-10-10-listener-loss-reasons.md. The existing nine-field acceptance contract
and loss rejection remain; attribution adds a strict three-counter reader.
Implementation is in progress with a clean-context Sol 6.1 worker; parent still
owns independent review, target builds, canonical checks and hardware. Encoder
export and broader recovery/admission architecture remain separate.

Stack repeat evidence and the reason-counter brief are committed as 644c9cb.
The four-file loss-reason implementation passed independent review, all 27
canonical stages, the probe build/IRAM check and feature allocation-query gating.
Both target queue symbols grow 16 bytes to 2024; exporter frame grows 16 bytes to
1008. Metrics/capture frames remain 208/528. Build checkpoint:
docs/audit-evidence/2026-10-10/listener-loss-build.md. Next commit the reviewed
diagnostic, then parent runs a bounded slow-reader measurement on its exact
default unified image. Preserve the 3b3afdb images and original failed trials.

Loss reasons committed as bb56186 and its exact default image tested. The slow
trial reproduced one full-queue rejection (contended/poisoned zero), with failed
fast delivery and a successful ten-frame third-connection probe. Strict acceptance
correctly remains failed. Minimum metrics stack free is 1156 bytes; cleanup and
final stopped MP3/Medium state passed. Sanitized evidence is under independent
review before its checkpoint commit.

Ruling: keep listener capacity eight for the next unit. At least ten
publication attempts are evidenced, but final pending count is unknown and
enlarging storage does not explain slow consumption.
Proceed with already-scoped encoder logging isolation and measure again, keeping
the failed gate explicit. Binding unit: 2026-10-10-encoder-telemetry.md. The small
fixed encoder queue, producer identities and strict capture bounds preserve
acceptance under delayed export; no scheduling, codec, stack or buffer changes.
The prior audit authorization covers this extraction; no renewed permission gate.

Independent reason-trial review verified 13 original hashes and caught the exact
attempt-count overclaim; corrected to at least ten with pending count unknown.
Evidence saved as listener-loss-device.md/.json under October 10 audit evidence.
No delivery or complete 21tp pass is claimed. Encoder extraction is now underway
with a clean Sol 6.1 worker; capture-failure reporting design is a read-only lane.

Encoder source extraction and capture-failure handoff are under independent
review. Parent wired the capture handoff only after explicit audio_in/lib release.
Its five actual-module tests and scoped integration review pass; target stack and
atomic lowering remain unmeasured. Binding capture follow-up plan is
2026-10-10-capture-failure-telemetry.md.

Parent target-source inspection found that ESP Rust std Mutex::try_lock can
allocate on first access: the pthread backend initializes OnceBox/pthread storage,
and pinned IDF mutex initialization allocates a semaphore. Both telemetry queues
now initialize on main before spawn_capture starts metrics or capture threads.
Real initialization-preservation tests cover state/counters; independent source
review verifies backend and call ordering. Linux host tests cannot prove this
ESP allocation behavior or target runtime cost. No queue capacity changed.

Full-host lifecycle regression exposed an incorrect eight-turn test bound: the
fixture publishes 16-frame raw blocks, requiring 72 reads for 1152-frame MP3 output.
Production behavior is unchanged; preserve the failed host logs and derive a
bounded test allowance from fixture dimensions before final verification.

Encoder/capture telemetry integration is frozen and independently reviewed. All
27 canonical stages pass; the final parser refinement also passed a fresh
check-harnesses run. Host default443/probe465 tests pass, with two existing
ignored tests in each. Both target builds and IRAM checks pass. Target encoder
queue storage is 440/632 bytes, capture failure handoff16; listener stays2024.
Metrics frame224 and nested application paths1232/default,1360/probe remain
static measurements, not runtime headroom. Fresh-eyes review found no new code
defect and corrected the capture plan's no-retry claim to match target CAS loops.
Build receipt: docs/audit-evidence/2026-10-10/encoder-telemetry-build.md. Next:
commit, then bounded exact-image default and probe trials, preserving failures.
No queue, stack, codec, priority or buffering policy change accompanied this unit.

Encoder/capture extraction is committed as 05cf802. Its exact default image
flashed successfully. Bounded two-listener trial1 reached premature MP3 EOF;
retain the failure and complete cleanup before the separate slow-reader trial.
The probe preflight also omitted Master/PCM gain checks. A Sol6.1 follow-up now
validates each declared channel plus Master unmuted, with actual archived
readback fixtures, runtime negative controls and passing check-harnesses.
No mixer writes or firmware changes accompany that harness correction.

Independent local97bk unit: README now maps release gates to existing commands,
exact sample/progress limits, diagnostic completeness and distinct five/nine
capture-counter schemas. Missing/canceled/infrastructure failures cannot pass.
Coverage evidence now distinguishes historical unimplemented CI from the later
checked-in workflow; hosted execution remains unverified. Independent source/link
review and parent fresh-eyes pass. This is a review contract, not an implemented
CI aggregator or dedicated HIL runner; 97bk/m31k remain open for those gates.

Device acceptance for 05cf802 failed. Independent reviews verified all original
hashes and producer-time accounting. Default two-listener trial decoded 7.368 /
9.072 seconds, with one full encoder queue drop and missing window 4. Slow-only
trial reclaimed a slot and decoded ten probe frames, but fast delivery failed;
three encoder queue drops hide windows 5/6, and window 7 proves raw overrun/reset.
Observed metrics minima are 1164 / 1148 bytes, not safe worst-case allowances.
Receipts: encoder-telemetry-default-device.md/.json in October 10 audit evidence.

Probe image ran no media listeners. Five raw overruns repeatedly invalidated its
first OFF1 interval; baseline-backed output was roughly 0.87x real time. Storage
and Window paths ran, with metrics minimum free stack 428 bytes. Parent ended
the partial acquisition through its SIGTERM cleanup handler after sufficient
failure evidence; original canceled result and raw bytes remain. Independent
strict replay and provenance review agree. Receipt: encoder-telemetry-probe-device
.md/.json. No completed OFF/ON/OFF, performance pass or overflow proof is claimed.

Parent restored the exact default unified image, verified stopped MP3/Medium,
autoplay and recognition off, and checked routing with no mixer writes. Source
and UART cleanup passed before every reset. Kata 21tp/wbz6 comments retain all
failed gates. Next investigate measured exporter stack cost and delay using
pinned logger/task source; do not enlarge queues/stacks or tune priority blindly.
The new release contract and CI-status clarification are committed as 5eb5786.

The bounded encoder-export stack fix passed all 27 canonical stages, both host
configurations, both target builds/IRAM, independent source and linked reviews,
and parent fresh-eyes. Actual duplicate 200/296-byte Record copy is removed;
probe Storage application nesting drops 1360 to 880 bytes. Record lifetime ends
before receipt formatting and Boundary returns before Storage. All output and
queue/task policies remain unchanged. Build receipt: encoder-export-stack-build
.md. Parent now commits and repeats exact-image hardware; no runtime margin or
throughput acceptance from these static reductions.

The eb38c65 device repeat exercises probe Storage/Window and measures 908 bytes
minimum free metrics stack, versus 428 previously. Five actual raw overruns still
invalidate the canceled partial probe; no completed OFF/ON/OFF is claimed.
Default slow-reader delivery also fails, with one full encoder queue drop and
missing window 6. Its metrics minimum remains 1148 bytes. Independent evidence
reviews pass; see encoder-export-stack-device.md/.json. Default unified firmware
remains installed, stopped MP3/Medium, autoplay and recognition off. Cleanup and
read-only routing pass; original failed artifacts remain intact.

Kata sjpm fixes a separate acquisition bug: an interrupted prior-boot UART line
can complete after reset release. Live and final analysis now start at the first
exact owned ROM reset marker and retain byte-offset provenance. Any malformed
first reset candidate or later reset rejects. Independent review caught an
initial skip-forward false pass for prefixed reset text; runtime negatives now
cover it. The healthy crossed-reset fixture passes and actual saved device bytes
still reject all five raw-loss boundaries. No decoder or probe grammar was relaxed.
Committed as 7891c44 after independent review, parent fresh-eyes and all 27
canonical checks. Replay uses the original reset_released event and unchanged
trial bytes; it is integration evidence, not a fresh healthy hardware run.

Next: independently verify the minimum metrics-delay design against pinned
FreeRTOS critical sections and permanent-task lifetime before implementation.
Only low-rate sampled Ready/Blocked counts are proposed; priorities, stacks,
queues and stream policy remain unchanged. A missing final diagnostic receipt
must remain incomplete evidence. Kata 21tp/wbz6 remain open.

## Metrics-state hardware checkpoint

The bounded sampled-state observer is committed as 55d5416 after all 27
canonical stages, both target builds/IRAM, independent source and linked reviews,
and parent fresh-eyes. Three immutable-image acquisitions retain their failures.
New default encoding measured 0.780–0.789x across five complete windows, each
with raw loss; the exact preceding eb38c65 control measured 0.994–1.006x across
five windows with no raw loss. Both fast listeners fell behind before reclamation.
This comparison establishes an association, not an observer or cache root cause.

The new probe ran without listeners and retained four raw overruns. Parent ended
the partial run through its owned cleanup handler; thirteen complete windows
measured 0.928–0.941x. It did not complete its probe sequence. Sampled Ready
counts do not prove uninterrupted execution; supervisor preemption itself can
leave metrics Ready. Query maxima of 8/12 microseconds are observed values,
not bounds. Late or missing diagnostic dumps restrict which intervals they cover.

Reviewed receipts are in docs/audit-evidence/2026-10-10/metrics-state-device.md
and .json. All acquisitions completed cleanup; original failed artifacts remain
under target/kata-hardening/metrics-state. The exact preceding unified default image eb38c65 is installed
because the new default regressed encoding. Fresh API/routing checks confirm
stopped MP3/Medium, autoplay and recognition off, front Headphone on, rear Line
Out off, unchanged zero-dB gains, and no source/UART processes. This preserves
both output formats; it does not revert to FLAC playback policy.

Next: independently review the small same-image observer OFF/ON/OFF experiment.
Metrics owns its transitions and logging; supervisor never formats transition
logs. Preserve existing capture/encoder work and hold code placement fixed within
the run. All losses remain failures and missing phase evidence stays incomplete.
Kata 21tp/wbz6 remain open. Root-session replacement is still unavailable; this
durable checkpoint and fresh Sol 6.1 contexts preserve continuity without claiming
a fresh root session.


The same-image crossover now passes source/harness reviews, linked inspection,
both target builds/IRAM and all 27 canonical checks on the final source. Four
schedule tests and fifteen scratch harness tests pass, including negative controls
for boundary-only loss and replaced provenance. Linked review moved transition
formatting out of the persistent metrics frame: 432→304 bytes, compared with
256 in the exact rebuilt default. Supervisor remains 272; runtime margin remains
unverified. The frozen diagnostic SHA is 8ef23d4fae8ecd16013e3eace538b373e461f11e266a64ac9756d457a142922c.
See metrics-state-crossover-build.md and the matching crossover implementation plan.
Parent will run bounded raw-byte observation after this reviewed checkpoint.

The exact crossover image completed two 300-second acquisitions. Trial1 retained
all raw bytes and cleanup but its wrapper raised a duplicate event-key TypeError;
it remains a failed acquisition. A real-file red/green regression fixed that
scratch logging bug. Trial2 completed naturally with clean ownership receipts,
strict provenance and all phases. Six complete windows per phase measure about
0.724x in OFF1, ON and OFF2, with 38 raw overruns. The observer runtime bundle
alone does not explain this image’s deficit; layout and input/platform causes
remain unlocated. Delivery was not exercised. Independent evidence review is
underway; no Kata closes.

Next use the same owned acquisition with a bounded 90-second duration for the
archived eb38c65 default and current bacdecb default. Real default feature receipts
remain separate from the strict 300-second crossover oracle. These are diagnostic
comparisons, not new audio acceptance gates. No extra firmware instrumentation
until that comparison establishes whether the image separation repeats.

The independently reviewed 97bk canonical-stage evidence plan now has a clean
Sol 6.1 implementation worker. Its scripts/tests are independent of the frozen
firmware and scratch acquisition. Heavy checks wait until hardware measurement
is idle. Root replacement remains unavailable; durable checkpoints and fresh
worker contexts continue under the recorded exception.

The 97bk local evidence unit passes independent review and all 27 canonical
stages with complete per-stage receipts and stable boundary snapshots. Real
negative controls cover command failures, evidence failures and interruption.
Mac Bash 3.2 runtime remains unavailable; the reviewed code uses its supported
descriptor syntax. The hosted and hardware gates stay open.

The matched 90-second default observations both completed cleanly. Archived
eb38c65 generates 0.961316x with one raw overrun; current bacdecb generates
0.786533x with eight. Exact images, fixture, helper hashes and routing receipts
remain preserved. Independent paired-evidence review is underway. Current
unified default remains installed, stopped MP3/Medium, autoplay/recognition off;
source and UART ownership cleaned up. Next compare representative real WAV
input through a throwaway host Shine work-count experiment before changing
target instrumentation. This cannot reproduce target cursor/reset history.

Kata 2qez fixes actual listener peer extraction from the doubled firmware logger
prefix. The retained-line regression failed before the two-expression fix;
independent review and all 27 canonical stages pass. Actual stage receipts are
target/check-logs/run.CNK4yPju, with complete status and equal boundary snapshots.
The real WAV input trial still fails: current default delivered only 2,160 stereo
frames (45 ms) before its listener fell behind. Acquisition and cleanup completed;
this is not a valid eight-second host benchmark fixture. The corrected parser
does not change that verdict. Archived default is now flashed, with the matched
WAV trial next. Preserve the checker revision difference in that comparison.

The host Shine work-count experiment is reviewed and ready for valid WAV inputs;
no real capture comparison has run. Priority starvation remains a hypothesis:
the encoder's periodic delay does not guarantee listener service, and current
evidence lacks per-chunk wakeup timing. Do not change priorities on that basis.
Root replacement remains unavailable; this checkpoint and fresh Sol 6.1 workers
continue under the recorded session exception.

The 8s9m refusal-write prerequisite is verified: both paths now use one response
deadline with the unchanged two-second allowance. Real-socket legacy RED and
GREEN, independent review and all 27 canonical checks pass (run.jurAgEJG).
This does not implement concurrent admission or prove target scheduling bounds;
8s9m remains open. Parent will next extend the shared acquisition state validator
for explicit FLAC input collection, with temporary saved format restoration and
no Sonos Play. The earlier failed WAV inputs remain unusable as eight-second
fixtures; no downstream host work comparison has run.

Both MP3-selected WAV attempts now have independent evidence review and durable
reports in metrics-state-wav-input.md/.json. Archived also truncated (275 ms),
so neither is a valid eight-second fixture. Weighted encoder rates remain
0.785716x current versus 0.959784x archived, with eight versus one raw overruns.
Both owned acquisitions completed naturally and cleaned up; the board remains
archived unified default, stopped MP3/Medium. This closes no audio acceptance gate.

The explicit expected-format acquisition extension passes focused tests,
independent review, fresh-eyes and all 27 canonical stages (run.OAqOjceh).
Shared require_state remains the source of truth; FLAC is limited to the strict
90-second WAV acquisition. Next: parent selects saved and active FLAC while
stopped on the archived image, collects the input, then explicitly restores MP3
even after uncertain command delivery. If input fails, stop this collection route.

The archived FLAC-selected attempt completed its acquisition but its WAV input
failed after 1.465 seconds. Independent review verifies 0.999743x FLAC generation
and zero observed encoder overrun/reset, separately from the listener's ring loss.
Reports: metrics-state-flac-wav-input.md/.json. Parent reaped session 10016,
verified no source/UART owner, and restored saved and active MP3/Medium with the
complete original config. Sonos remains stopped. This ADC-fixture route is closed;
do not run a current-image FLAC counterpart or accept partial bodies for host work.

Next diagnosis: four monotonic readings around the three existing Shine phases,
under a diagnostic feature with unchanged normal allocation and audio contracts.
Keep existing queued metrics export/reporting cadence and interrupted tails as
incomplete evidence. Source provenance review must distinguish the frozen eb38c65
and bacdecb contexts from current HEAD; rate separation must survive instrumentation
before interpreting phase differences. No cause, delivery pass or broader Kata
completion follows from these measurements.

The reviewed phase plan is now 2026-10-10-mp3-phase-timing.md. Exact source
attestation is missing for both saved images despite matching partial hashes.
A Sol 6.1 worker is preparing copied git-archive reconstructions with secure local
config copies and isolated outputs; no worktrees or original-image mutations.
Parent must first build and measure these uninstrumented baselines. Timing code
remains unimplemented until their rate gap reproduces. Hardware is idle, stopped
MP3/Medium on the archived unified default, with no source/UART owner.

Both reconstructed baselines now built and passed their linked IRAM checks.
The completed 90-second observations measured 0.999482x archived and 0.999629x
current, with zero encoder overruns. This fails the historical rate-gap
reproduction gate: phase timing remains unimplemented. Independent review and
artifact checks support reconstructed-default-controls.md/.json; startup capture
faults, false device attestation and the unarchived parser dependency stay explicit.

The separate current reconstruction delivery trial failed; final decoder files
contain only 2.064 and 2.256 seconds. Its wrapper stopped before the slow-reader
phase and cleaned up. Reviewed evidence is in reconstructed-listener-failure.md
and .json; the earlier summary's 2.184-second snapshot remains unchanged.
Current reconstructed unified image
626456a3 remains installed, stopped MP3/Medium, with no source/UART owner.
Kata ce69 tracks a confirmed reporting bug: producer generation resets and actual
listener history loss both report FellBehind. A Sol 6.1 worker is implementing the
small diagnostic correction; stream termination and recovery policy stay intact.
Plan 418609e passed independent review and a fresh Sol 6.1 worker is implementing
its real ring/pipeline/telemetry/parser regressions before the production change.
