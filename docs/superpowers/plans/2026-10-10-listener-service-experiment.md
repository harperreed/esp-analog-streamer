# Listener service experiment

Doctor Biz, compare the frozen ordinary priority-7 listener image against a
throwaway image whose sole source patch sets LISTENER_THREAD priority to 8.
This is an authorized, reversible device experiment, not a production bugfix.
Permanent scheduling policy remains unset. An improved B image is a candidate
intervention; distinct linked images leave priority versus layout confounded.

## Evidence and hypothesis

The complete sampler receipt contains 174 valid Ready observations, including
96 at frame-write ages at least 100 ms. Both owners remained live at the
10-second expiry: the UART receipt is complete but lifetime coverage incomplete.
Ready proportions are neither runtime fractions nor proof of starvation, and
frame-write brackets include mode/send/poll work. The ordinary control decoded
2.640/2.736 seconds with send maxima about 441/436 ms and no WouldBlock/polls.
Diagnostic encoding near .996x versus ordinary .86-.87x is a material image
confound, not measured observer benefit.

The hypothesis is that the priority-8 MP3 encoder limits priority-7 listener
service on core 1. Equal priority creates additional service opportunities.
Pinned IDF FreeRTOSConfig.h:86,94 enables preemption/time slicing; the actual
dual-core FreeRTOS-Kernel/tasks.c:3559-3640 selects the highest runnable,
affinity-compatible priority and implements best-effort round robin. This gives
no hard latency or equal CPU-share guarantee. ThreadSpec passes configuration
through public ThreadSpawnConfiguration (firmware/src/threads.rs:26-40).
MP3 encoder priority remains 8 (core/src/stream_buffer.rs:79-83 and
firmware/src/stream.rs:628-633).

Changing the existing eight-publication vTaskDelay(1) cadence adds encoder sleep
and risks worsening already deficient producer progress. Zero-tick delay/plain
yield cannot make a ready priority-7 task outrank a ready priority-8 task.
Lowering encoder priority or moving cores changes more task relationships.
Listener-only 7-to-8 is the smallest useful intervention. Audio gates decide
whether the added CPU competition is acceptable.

## Frozen artifacts and preflight

A is current ordinary image identified by the parent's `6f261bf4` hash prefix;
retain its full ELF hash and exact source/config/build archive. B is a frozen
ordinary build from that exact source with only explicit `priority: 8` in
LISTENER_THREAD (firmware/src/stream.rs:162-166). Keep accept priority 7,
encoder MP3 priority 8, core affinity, stacks, eight-publication delay, codec,
ring/history, socket behavior and deadlines unchanged. Omit listener-frame-state,
encoder-probe and crossover features from both. Add no feature, API or logs.

Parent preserves the minimal throwaway patch and B build privately. Preserve
normal source archive and existing evidence; do not commit a production priority
change. Run all canonical checks and target build on B before device access.
Inspect linked pthread spawn/configuration evidence for both images, including
listener configured priority/core and MP3 encoder override. A mismatch blocks
flashing. This proves configured values, not runtime readback; no new readback
instrumentation is authorized by this plan. Existing canonical checks and real
device trials verify this one-constant throwaway experiment rather than a
unit test that restates the constant. Production work later retains normal TDD.

## Controlled acquisition

Use the unchanged existing wrapper `--scenario both` for every trial. Its primary
soak requests `stream-soak --seconds 30 --listeners 2 --path /stream.mp3
--failure-observe-seconds 15`, with soak cap 160 seconds and total cap 360 seconds.
Thirty seconds means decoded audio per listener, not joint wall time. The
nested decoder permits requested duration plus 60 seconds wall time. Keep all
these budgets unchanged; no fabricated soak-only flag or helper change.

Order is A1/B1/A2/B2/A3, with early termination below. The existing ordinary
trial can count as A1 only after verifying identical helper/fixture hashes,
source/configuration, source launch/routing, settings and acquisition controls.
Otherwise acquire a new A1. Reuse frozen A/B ELFs throughout; no intervening
rebuilds or tuning. Keep the same verified source segment/start offset/level,
physical routing, network/AP/workstation route, MP3 48 kHz stereo 320 kbps,
Medium history, recognition setting, admission order and requested gap.
Retain actual admission skew, departure times and source/helper/ELF identities.
Use identical cold-start/setup and cleanup for each trial. Invalidate an affected
comparison for unplanned source/network/config changes; retain its evidence.

Compare only the nested soak scenario as the primary A/B outcome. The wrapper
may reach a separate slow scenario, including after an accepted soak failure;
its results are separate follow-up coverage. Neither its verdict nor duration
enters the primary comparison. Preserve original failures and ordinary failure
tails. Catch-up after one listener departs cannot clear joint throughput.

## Decision rules

After any B audio/integrity, capture/raw-loss or encoder-progress failure,
complete the retained ordinary failure tail and cleanup, then stop the series.
Do not repeat a rejected intervention. Acquisition failure is inconclusive,
not a scheduling outcome. If A1 does not reproduce the target failure, stop
for control assessment rather than claiming that B repaired it. Continue
A2/B2/A3 only after B1 clears every existing gate and A1 reproduces.

A positive candidate requires both B trials to pass existing delivery/decoder
integrity gates, no added capture/raw loss, current encoding and no growing
producer/delivery deficit; all three A trials must reproduce the target failure.
A 30-second decoded total reached by slow catch-up does not suffice. Report
actual joint wall intervals, progress/rates and survivor-only intervals using
existing records. If retained records cannot establish joint progress, mark
that acceptance unresolved rather than add an instrumentation project.

Compare pre-admission and joint producer rate/backlog/reset/raw-loss, capture
faults, stack watermarks, each ordered listener's decoded integrity and separate
send-call/frame-write maxima plus WouldBlock/poll totals. Predeclare a reduction
of at least 50% in each B listener's send maximum relative to the smallest
corresponding A maximum as a secondary diagnostic target. It is neither an audio
gate nor proof of scheduler cause. Better sends with failed audio reject B;
full delivery without that reduction may still identify a candidate image.
Mixed or drifting controls are inconclusive. Repeated B success still cannot
separate priority effects from image layout, especially if producer rate changes.

A B failure rejects this remedy for the tested workload, not every scheduling
hypothesis. A passing candidate requires the already required broader and longer
delivery/capture/TLS acceptance before permanent policy or production changes.
No audio gate is weakened. Keep current experimental MP3 after failure; no
automatic FLAC rollback. After each run verify stopped MP3/Medium, autoplay off,
and source/helper cleanup. Parent owns implementation, review and acquisition.

Scoped design re-review accepts this plan with no remaining blocker; receipt:
target/kata-hardening/listener-service-experiment-round1-review.md.
Implementation, compiled configuration checks and hardware acceptance remain
pending. Optional exact linked-byte/placement comparison can narrow the layout
confound only if the actual artifacts support that conclusion.
