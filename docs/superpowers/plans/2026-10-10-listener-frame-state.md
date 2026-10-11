# Bounded listener frame-write sampling: d2cb / pd6c

Doctor Biz, bracket the existing firmware payload call to
DeadlineWriter::write_all_before and sample its task state from the existing
supervisor. Do not add per-send hooks, kernel patches, another task, priority
changes or encoder phase markers. This bounded diagnostic implements the authorized transport investigation.
It does not fix audio delivery.

Design review: target/kata-hardening/listener-sampling-plan-review.md.
Pinned-source feasibility: target/kata-hardening/listener-scheduling-feasibility.md.
The reviewed ON-only design controls this implementation; earlier per-send
and crossover alternatives are not implementation requirements.

## Scope and success criterion

`firmware/src/stream.rs:758` already calls write_all_before once per
ListenerStep::Write. Instrument only `!header` writes. Set an active frame-write
sequence immediately before that call and clear it immediately after return,
before report_timeout or other telemetry. This covers all success/error returns
from the writer without changing deadlines or error propagation.

`core/src/stream_http.rs:117` first changes socket mode, runs its write loop,
and restores the original mode. The bracket therefore includes mode changes,
all send calls, writable polls, local work and preemption throughout. Existing
sender diagnostics separately report send wall totals, accepted bytes,
WouldBlock and poll counts. Their prior zero-poll/full-send result makes this
coarser bracket useful, but does not make any individual sampled state a send
state. Call records **frame-write-state samples** everywhere.

Per-send instrumentation would narrow correlation to actual libc::send but
requires core writer callbacks or a target-specific telemetry dependency in
shared code, tests of the new callback contract and more per-call work. It is
unnecessary for the first qualitative observation. If new receipts show polls
or mode-related costs dominate, this draft cannot attribute samples to sends;
return to scope selection with that evidence.

Success means a complete receipt with valid selected identities, actual sampled
states during matching active frame-write sequences, explicit query gaps/losses,
and unchanged failed/passed delivery verdicts. It does not mean obtaining enough
samples to support a causal conclusion. Zero useful samples is a valid,
inconclusive measurement result.

## Registration, lifetime and cleanup

1. Register from inside the selected compressed listener's own task, after
   buffer/epoch ownership exists and before stream_compressed payload work.
   Obtain only its current task handle. WAV, refused requests and failed thread
   spawns do not register. Select at most the first two compressed tasks in the
   boot; keep their slot identities permanently reserved for this acquisition.
2. Use an owner scope guard to clear active sequence and handle when the
   compressed streaming scope ends. Clear before metrics.departure and certainly
   before returning from listen. Do not wait for observer acknowledgment and do
   not delay or retain the connection for diagnostics. Keep archived identity
   metadata separate from the live handle so export never queries a dead task.
3. `listen` at stream.rs:327 has one outer completion path: WAV/compressed match,
   departure publication, then drop(connection). stream_compressed loops until
   End, then has the optional admitted refusal and one final return. Its breaks,
   write errors, capture invalidation and peer departure all leave through that
   scope. The guard must surround the actual compressed call, not just a success
   arm. Refusal writes and close work remain outside the payload bracket.
4. LISTENER_THREAD inherits core 1/priority 7 from ACCEPT_THREAD. Supervisor is
   core 1/priority 11. ThreadSpec sets actual pthread affinity/priority. Only
   supervisor synchronously loads and queries these handles; never hand them to
   metrics, queue them, retain them across yield/sleep, or query from another core.
   A lower-priority pinned owner cannot exit concurrently on the same core while
   supervisor is querying. The query's kernel critical section may spin or be
   delayed but does not voluntarily block on a semaphore. Other-core/ISR work
   can mutate Ready state, which is expected and is protected by the kernel query.
5. Pinned pthread.c:244 calls pthread_exit only after task_arg->func returns.
   pthread_exit:517 cleans TLS and updates pthread state before detached
   self-delete or joinable suspension. pthread_detach:487 only marks a RUN
   pthread detached; its cross-core deletion path is for an exited thread.
   Join's deletion likewise follows exit notification/state. ThreadSpec drops
   the JoinHandle rather than retaining/joining it. Clearing inside the listener
   scope precedes both runtime teardown and detached deletion. The firmware
   source currently has no direct vTaskDelete/pthread_cancel call. panic_abort
   ends the boot; no claim of unwinding cleanup is needed.

These are design invariants, not a generic pointer reclamation scheme. Before
code review accepts them, verify the pinned Rust std JoinHandle detach path and
that no new external task deletion, affinity or priority mutation was introduced.
If these invariants cannot hold, stop this design and use an explicit lifetime
handshake. A pointer recheck after dereferencing cannot repair use-after-free.

## Fixed storage and observation

Use two statically preallocated slots and one supervisor-owned aggregate block.
Owner-written slot fields: reserved identity (1 or 2), handle, endpoint metadata
captured at registration, monotonically increasing u32 write sequence, active
sequence (zero means inactive) and write start timestamp. Omit unused write-end
timestamps and completed counters.
A slot is never reused during the boot, so identity cannot alias a later task
whose TCB address gets recycled. Extra listener registrations increment an
unselected count; do not replace either selected listener.

Use native-width atomic publication fields where tasks share access. Do not add
64-bit atomics on Xtensa. A split 64-bit timestamp must have an explicit versioned
publication protocol or exploit the proved same-core single-writer/read ordering;
plain shared Rust mutable data across tasks is not permitted. Prefer storing
bounded acquisition-relative u32 microseconds with explicit expiration before
wrap. Write sequence overflow disables that slot and sets a sticky invalid flag.
No dynamic buffer, task-name lookup, stack scan, lock added by the diagnostic,
logging, or allocation occurs at frame entry/exit or per sample.

Supervisor runs health actions first. On a separately gated sample opportunity:
read live identity/handle/active sequence/start time, call eTaskGetState only for
a non-null selected active slot, then re-read token/identity. Count a matching
sample under the returned state; mismatches are unavailable. Same-core priority
makes owner mutation during this synchronous query impossible under the stated
invariants, but keep fail-closed identity/token checks. Kernel state can change
on the other core after the query, so timestamp each query's start/end and do
not pretend its return is the state at either exact endpoint.

Count attempted opportunities, inactive/unregistered, valid
Ready/Blocked/Running/Suspended/Deleted/other, invalidation and saturation.
Maintain a second state-count row for samples whose observed active-write age
is at least 100 ms. This filters early samples without pretending to observe
the full completed-write duration. Report both rows and their valid counts.
Deleted is an invariant failure, not a successful sample of task teardown.
Measure query wall maximum/total and actual opportunity gaps. Keep one longest
observed active-write-age exemplar per slot, with sequence,
query start/end, state and age. These are sampled ages, not exact
whole-write durations. Deterministic retention records replaced/not-retained
counts. All counters saturate with sticky saturation flags; complete receipt
reports capacity, selected count, valid count and all omissions. Fix exact byte
layout and confirm it in the real target build before acquisition.

## Cadence, gate and export

Retain the supervisor's existing 100 ms sleep and health loop. Add at most one
sample of each slot per loop while enabled; do not use the existing one-second
metrics observation gate for this diagnostic. No catch-up burst after a late
loop. Report actual spacing: the sleep and query overhead do not guarantee a
100 ms period. Existing one-second/eight-phase metrics counts are unsuitable
for the current 1.7-2.7 second listener lifetimes and should remain separately
identified. Even the proposed cadence can miss an entire short write or capture
only a few valid samples before raw reset.

A dedicated diagnostic build feature gates all listener sampling code and is
absent from ordinary builds. The diagnostic is ON for the first selected pair;
there is no runtime crossover, new HTTP endpoint or persisted setting. First
registration starts a bounded 10-second acquisition deadline. After both
selected registered owners retire, freeze early; otherwise freeze at the deadline
with explicit no/one/two-selected, live-owner and incomplete/expiry fields.
If no listener registers, freeze an empty incomplete receipt at 30 seconds after
supervisor launch. These are diagnostic cutoffs, not socket deadlines. Supervisor
checks them in its existing loop, so record actual cutoff lateness.

At expiry set a terminal sampling gate before publishing. Selected owners may
continue their ordinary stream/close lifecycle and clear their live slots later;
never force close, wait for them, or read their mutable slot fields from metrics.
Freeze supervisor-owned aggregates plus copies of needed identity metadata into
one immutable receipt. Later owner markers must check the terminal gate and
cannot mutate that receipt. Both-handle retirement must be read before expiry
publication; an unregistered slot is not a retired second owner. A receipt with
one selected listener can still contain useful samples, but is explicitly not
two-listener coverage. End the diagnostic once; do not restart or recycle slots.

Metrics renders only after atomic publication, in bounded batches using existing
yield practice. No critical-path logging. Acquisition retains UART until the
complete receipt or reports it missing. Exact-image normal-versus-diagnostic
reproduction checks whether the failure pattern survives, with observer and
code-layout confounding explicit. Query wall total/max and actual sampling gaps
report observed diagnostic cost, including lock wait/preemption; they are not
isolated CPU overhead. Do not infer matched runtime cost from this design.

## Verification before a hardware interpretation

Future implementation must follow TDD: host model tests for two-slot selection,
identity/sequence expiry, active/inactive correlation, 100 ms age boundary,
early retirement, no/one/two-listener cutoffs, skipped opportunities, immutable
handoff, counter saturation and longest-exemplar retention.
Tests model actual state input rather than asserting fake scheduler behavior.
Run canonical checks and target compile; inspect target field placement, native
atomic lowering, stack/callee frames and linked eTaskGetState availability.
Host tests cannot prove affinity, lifetime, scheduler timing or target overhead.

Parent owns comparable original versus diagnostic runs, exact image/source
identity, normal hardware fixture/routing, cleanup
and stopped MP3/Medium final state. Keep all delivery/decoder-integrity/raw-loss
failure gates; retain failures and runtime watermarks. Compare the same workload,
not a no-listener rate window substituted for this overlap. Correlate selected
peer identities, actual departure/raw-reset times, active-write exemplars and
existing sender totals. Preserve original records and completeness receipts.

Ready includes a listener the supervisor itself just preempted. Repeated Ready
samples neither prove continuous waiting nor measure runnable wall time.
Blocked establishes only observed kernel waiting; lwIP completion versus other
blocking inside the bracket remains unresolved. Pending-ready placement can
lag logical completion. State proportions are biased observations, never time
fractions. Sparse/incomplete samples or sampling-associated changes to failure
pattern are inconclusive. No scheduler priority/yield change or encoder phase
instrumentation follows automatically from this draft.

## Execution and global constraints

- Parent owns hardware, full canonical checks, integration, review and commits.
  Use a clean-context Sol 6.1 implementer, then independent Sol 6.1 review.
- One coherent implementation unit owns the firmware diagnostic module,
  feature wiring, supervisor/metrics calls and payload guard. Test the actual
  shared diagnostic logic through host imports, not a copied sampler model.
- TDD precedes implementation. Target-specific lifetime and atomic lowering
  remain target verification requirements beyond host tests.
- No core writer changes, runtime crossover, new endpoint, persisted setting,
  task, priority, buffer, codec or deadline changes. Ordinary builds omit this
  diagnostic. Preserve existing observers and all failed evidence.
- Preserve .kata.toml and docs-ignore. No push/merge, mixer changes or automatic
  FLAC rollback. Hardware cleanup leaves stopped MP3/Medium, autoplay off.

Ruling: ON-only sampling replaces crossover because unequal short listener
lifetimes cannot support matched phase-cost claims. Cost if wrong: a new
instrumented image may alter the observed failure; treat it as inconclusive,
not as proof of production scheduler behavior or a reason to tune priorities.

Status: design reviewed; implementation pending. No Kata closes from this plan.
