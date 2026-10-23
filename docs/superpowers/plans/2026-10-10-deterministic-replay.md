# Kata 89g8: deterministic production replay

The first shared writer slice is committed in 9b7f363: seven deterministic
PCM/FLAC/ring replay tests, 31 real writer tests, independent review and all
22 canonical stages passed. Full orchestration remains open.

The following implementation contract supersedes the initial proposed core
orchestration/codec-adapter design. Two design reviews approved this bounded
firmware extraction after correcting lock, deadline, runtime finalization and
close-ownership contracts. Implementation began after the mutation pilot took
an immutable archive of 823c0cc. Approval is for implementation; it is not
acceptance evidence. Existing hardware and playback policy remain unchanged.

## Decision

Keep the orchestration in a new host-buildable firmware/src/stream_pipeline.rs.
Host tests already import stream_codec.rs and stream_runtime.rs by path
(core/tests/runtime_switch.rs, stream_properties.rs, listener_runtime_loom.rs).
Import the pipeline alongside them. Use the actual Worker, Runtime, Epoch,
codec::Encoder, CaptureBuffer, StreamRing, FlacRing, Slot and Lease. Do not add a
codec trait, another encoder, a second policy loop, or a production mock mode.
The firmware module must contain no esp_idf_svc, CONFIG, logging or RTOS calls;
stream.rs keeps those adapters and existing metrics presentation.

The pipeline runs one finite orchestration operation per step, with no wall-time
bound. A step may complete a real raw read, codec call or blocking mutex
acquisition, but has no explicit sleep, channel receive or socket I/O. Preserve
all existing blocking lock operations and lock order; do not convert them to
try_lock. Cooperative replay invokes steps only with no external mutex guards
held and schedules events between operations. It cannot suspend an actor inside
lock acquisition or codec execution; reject fixtures that would deadlock on a
scheduler-held mutex. The existing atomic copy hook remains the one explicit
mid-copy fault boundary. Keep real contention tests and Loom as separate evidence.
A step returns a wake/spin request or a complete-frame write request. Production executes a write
request with DeadlineWriter unchanged; replay executes that same request using
committed WriteProgress over virtual send/poll results. Both report completion
back to the same pipeline. This avoids another resumable socket adapter and
keeps descriptor flag/errno/diagnostic ownership in the existing writer.

## Exact ownership and API worklist

These are proposed internal names/signatures; review them as one contract before
implementation. Duration always means one run's monotonic epoch. Existing public
stream endpoint, codec-selection and DeadlineWriter contracts stay intact.

### core/src/stream_wait.rs: share the current wait decisions

Add RawWait (outer-started, retry-started, limit, interval) and FreshWait
(started, limit, interval), with separate attempt-start and attempt-finish
observations returning Ready, Failed, RetryNow or SleepUntil. The helpers own
the clock callback reads described below; callers must not replace finish time
with start time. Reset the 1 ms raw retry budget after each outer sleep turn,
not per Retry attempt. Add SnapshotWait::before_attempt(now) for strict
pre-attempt cursor/lock deadlines. Add BufferedWait (deadline, freshness,
last_progress, observed generation/written) with before_attempt(now),
observe_progress(snapshot_finished_at, progress), and retained-result
classification.

Clock boundary contract:

- FreshWait starts before the first read, performs each actual attempt, then
  samples completion time. Nonzero success and Overrun finishing before/at/after
  expiry win; only empty completion at/after expiry stalls.
- RawWait samples its outer freshness start once and its inner retry start on
  each outer turn. After a Retry, separately sample inner elapsed then outer
  elapsed in the existing short-circuit order. At equality to either exhausted
  budget, Retry becomes empty; sample outer completion time for that observation
  and either stall or sleep. The next inner retry start follows that sleep.
- SnapshotWait samples before attempting. A permitted cursor/lock attempt just
  before expiry may finish late and succeed. Do not add a finish-time veto.
- BufferedWait samples before checking absolute/freshness expiry, then observes
  peer connectivity, then takes its snapshot. A qualifying progress timestamp
  is sampled after the snapshot, as today. A retained cursor admitted by that
  attempt can succeed after expiry. No extra deadline check follows the snapshot.
- Use literal before/at/after outcomes in virtual tests. For real-time tests use
  closure-controlled completion or synchronization; a short sleep near a
  deadline cannot characterize an exact equality edge.

Migrate read_fresh/read_atomic_fresh_with/cursor_before/lock_before/
buffered_cursor_before to these helpers; existing function signatures remain
wrappers that run the shared decision, real ring read and real sleep. Keep
read_locked_fresh calling the common FreshWait policy through read_fresh.
No test reimplements these deadlines.

Critical characterization: read_fresh attempts a read before checking expiry
and accepts any successful nonzero result, even if it finishes late. Overrun
also beats a timeout on that attempt. Raw Retry exhaustion becomes an empty
outer observation; only that observation checks the freshness timeout.
Cursor/lock/buffered admission check expiry before an attempt instead. A generic
precheck on every read would change behavior. Buffered admission's first
snapshot does not establish progress; an increased write count in the same
generation, or a new generation with written > 0, refreshes freshness. Reset
never renews the absolute fill deadline. Socket connectivity stays outside locks.

### firmware/src/stream_pipeline.rs: move encoder decisions once

EncoderPump owns raw cursor acquisition state, raw wait, current block number,
and the existing heap chunk buffer (240 stereo frames). It refers to Worker
rather than owning a second encoder/ring. Proposed methods:

- EncoderPump::new(): starts needing a raw cursor, with no clock read.
- EncoderPump::step(raw, worker, runtime, now, observe): execute one state step;
  return Progress, Spin, SleepUntil(Duration), Switched or Stopped(error). The
  operation may block on the original encoded-ring or commit locks; this is not
  a wall-time bound.
- EncoderPump::block_boundary(): expose whether Worker::service may run; the
  step itself calls service at the existing frames_wanted == block_frames point.

now is a mutable callback returning Duration; observe is a callback receiving
bounded EncoderObservation values. Observations carry actual raw attempt
result/duration, whole read-wait completion/duration, codec duration, encoded
lock wait and push-held duration, reset and publication. These replace only
measurement plumbing; they do not choose transitions. stream.rs folds them
into the current RawReadMetrics/EncoderMetrics and RAW_READ_MAX_US atomic.
Keep metric counter definitions and reset/log intervals unchanged.

The step directly calls raw.cursor_now/read, worker.encoder.push/encode_block/
reset and worker.epoch.ring reset/push. It limits read size to min(240,
frames_wanted), retains partial codec input on absent capture, retries cursor
acquisition forever in bounded two-second turns, and on raw Overrun resets the
encoded generation, discards partial codec input, obtains a fresh raw cursor
and resets numbering to zero. A codec failure invalidates encoded output and
ends the worker exactly as today; a frame-fit failure remains the same invariant
failure. No healing or catch-up policy is added.

Production encode becomes the step driver plus existing metrics/logging and
FreeRTOS effects. Format-change priority changes and MP3 yield-every-eight
published-frame behavior remain in that wrapper and are triggered by shared
Switched/publication observations. Preserve the current yield counter reset
locations; do not silently change resets while moving them.

### Same module: move compressed listener decisions once

CompressedListener owns its cursor, buffered/live admission state, FreshWait,
listener-local frame number, and phase (admission/header/read/write/end). It
borrows the caller's existing complete-frame scratch buffer; it owns only the
small response+codec header. Proposed methods:

- CompressedListener::new(epoch, preset, recognition, now): derive budgets from
  the real OutputFormat/BufferPreset helpers, with no allocation of frame storage.
- step(epoch, scratch, now, peer_connected): peer_connected is a callback invoked
  only at the existing buffered-admission check; snapshot/read actual FlacRing under
  the existing try_lock policy, release locks, call actual
  codec::prepare_listener_frame, and return Progress, SleepUntil, Write or End.
- write_bytes(scratch): return the pending header or prepared-frame slice.
- write_finished(result): map errors through the existing End::from_write_error;
  advance number/frame/byte accounting only after a complete successful payload.

Write includes absolute deadline and whether it is the initial header; the
caller obtains the bytes from write_bytes. It cannot run another read or reuse
scratch until write_finished. The firmware converts that deadline using its
one monotonic origin and calls DeadlineWriter::write_all_before. Replay keeps
WriteProgress plus the borrowed actor buffer between scheduled send/poll turns.
It reports the final result through the same write_finished. A final send
finishing late remains success per the already characterized writer contract.

Retirement and write boundaries are phase-specific: check retirement at entry
to the outer frame loop, once before constructing its FreshWait. Do not repeat
that precheck on each resumable FreshWait turn or after its sleep. Retirement
inside the actual read closure remains Overrun/FellBehind. Once a Write request
exists, its absolute deadline is fixed and neither retirement nor a peer probe
can preempt its completion; observe disconnect through the existing writer.

Move End and its error mapping into this module. Keep Display formatting and
logging in stream.rs. Listener observations feed the existing metrics, including
header/payload boundary reset_diagnostics and payload elapsed timing. Preserve
retirement checks and classifications exactly: retirement at outer frame-loop
entry returns Retired, while retirement discovered in the encoded read closure follows
today's Overrun/FellBehind branch. Do not improve that distinction in a refactor.
Buffered disconnect still maps to CaptureStalled internally with refusal
suppressed; other buffered errors retain current refusal behavior.

### firmware/src/stream_runtime.rs: bounded command steps for real cancel races

Reuse actual Runtime::admit/Lease and Worker::service; do not emulate them in the
scheduler. Existing switch methods block on recv_timeout and use Instant, so
extract their prepare/enqueue/expiry steps, not an alternate switch engine:

- begin_switch(format, force, token, timeout) -> SwitchStart: validate and
  check active format first, then sample the deadline before actual Prepared::new.
  Preserve same-format immediate success even with a revoked token. A revoked
  different-format token is still prepared/enqueued and reaches the existing
  worker cancellation check; no early enqueue veto. Preparation failure and
  try_send busy/unavailable preserve their current errors.
- SwitchStart is either Finished(result) or Pending(PendingSwitch).
  PendingSwitch owns the real notification Receiver, outcome Arc, deadline and
  a clone of the same runtime clock callback.
- PendingSwitch::poll(self) -> SwitchPoll consumes the handle and returns
  Pending(the same handle) or Finished(result). try_recv notification or channel
  disconnection finalizes immediately; on Empty sample the owned clock and stay
  pending until expiry. This
  is not a general executor. A returned Finished has no handle to poll again.
- PendingSwitch::finalize(self) -> result consumes notification/outcome ownership
  once, locks the existing outcome, and takes any worker result. If absent, write
  the same caller-cancellation outcome and return the same caller timeout error,
  even on early notification-channel disconnection. A worker result already
  present wins even if finalization occurs late, as in the current adapter.
- The blocking switch/switch_prepared adapter uses the same begin request, waits
  with recv_timeout for its remaining budget, and calls that same finalize after
  notification, timeout or disconnection. No wait-until-expiry replacement for
  early channel disconnection and no duplicate outcome consumption.
- Worker::service retains try_recv and the same commit implementation. Commit
  acquires token, outcome and active locks in the original order, then samples
  its clock for expiry. Caller timeout revokes under the same outcome lock.
  Keep worker cancellation and caller timeout messages/error kinds distinct.

Use one injected clock callback Arc<dyn Fn() -> Duration + Send + Sync> stored
in Runtime. Runtime::new supplies a real Instant-origin closure and delegates
to a crate-visible constructor accepting that callback. Replay supplies a shared
atomic integer clock. Request deadlines become Duration. This one clock
injection is necessary across actual request/worker paths; it is not a test-only
policy or a generic Runtime type propagated through firmware. Keep existing
Loom synchronization selection and lock order intact. Ensure preparation time
consumes the same timeout and late worker service cannot revive expired authority.

Extract play_format's capacity/token/deadline decision only if the accepted
cancel scenarios require it; final Sonos create/load sends remain outside 89g8.
Do not claim these runtime tests prove ownership-safe Sonos recovery (pqqq).

### Connection ownership: retain the actual synchronous close

Replay models a draining interval by delaying the call to the shared synchronous
close transition while its whole actor retains the socket, actual Slot and
actual Lease. It does not model a suspended lwIP close. Production invokes actual
socket_close exactly once, with the current configure and ownership requirements.
No separate close engine, new retries or close timing policy is introduced.

If extraction is needed for reuse, use a small resource holder in the
host-buildable pipeline: ConnectionResources<S> owns socket, Slot and Lease;
close(self, close_once) consumes it and returns success or an owned retained
failure. close_once consumes the socket; on failure the actual CloseFailure owns
any still-owned socket or uncertain descriptor state. The retained result owns
that failure plus both Slot and Lease. The entire failed actor remains retained
on the host; production holds it through the existing nonreturning restart,
without logging first. Never drop Lease merely because streaming stopped.

The shared success transition has explicit order: actual close success, Slot
release, then Lease release. Construct ownership before frame allocation/spawn
so setup and spawn failures use the same drop/close path. Keep the production
ListenerConnection wrapper's Drop guarantee; the shared holder must not silently
release its guards when actual close reports an uncertain outcome. Review
its failure ownership with the concrete Rust implementation before integration.
If a generic holder makes Drop semantics harder, keep the existing wrapper and
extract only its consumed resource transition; do not add a hierarchy of guards.

Actual Slot::take remains admission authority; actual Runtime::admit supplies
the Lease. A remote cancellation is a disconnect observation/socket error, not
a new local cancel policy. Replay connection IDs address owned actors; late
disconnect events for an old ID must never target a newly admitted peer. During
payload writes production observes disconnect through send/poll; no extra cancel
or retirement precheck is inserted before write completion.

## Files and lanes

One implementation owner owns stream_pipeline.rs, stream.rs, stream_runtime.rs,
core stream_wait.rs and firmware module registration together: their contracts
are coupled. This lane also owns the small host-path characterization tests.
No competing edits to those files. Runtime characterization tests should exercise
real elapsed time before introducing the callback, then the identical request/
commit/outcome paths under virtual time; no test-only commit method. After the
seam is frozen, an independent
host lane may own core/tests/stream_replay.rs, test-only replay support and
versioned fixtures; another independent lane may own scripts/replay_stream.py,
its tests and canonical wiring. Parent owns integration/review/canonical gates
and every device action. Existing stream_codec.rs and committed stream_write.rs
need no behavior changes. No new runtime dependency should be needed.

Expected production footprint: 450–700 changed lines, predominantly moving
existing decisions and replacing loops with drivers. Runtime async extraction
and ownership plumbing may exceed this estimate; stop for review if the final
contract requires more modules or generalized abstractions. Do not force the
line estimate by hiding unshared decisions in test code.

## Characterization before extraction, then the full deterministic gate

First add wall-time/real-socket characterization around currently callable
wait/runtime/codec methods: late successful read vs empty-at-deadline; raw Retry
budget; buffered fill/freshness reset; canceled/expired switch and close-before-
slot-release. Retain the existing real-socket writer, runtime and Loom tests.
The equivalent virtual tests drive the extracted production functions and assert
literal outcomes/source ranges; they never reproduce policies as reference code.
For currently firmware-only encoder/listener branches, establish explicit
expected traces from source and make a missing-seam test fail before moving
code; do not pretend a post-extraction test characterized the old binary.

The full test scheduler orders events by (integer time, insertion ID), with
finite event/virtual-duration/output bounds. Its operations are only capture
publish/invalidate, actor scheduling, environmental send/poll/peer/close outcomes,
actual Runtime admission/command enqueue/token revoke and codec work. It dispatches
shared actions; it never selects overrun/reset/timeout/reclamation policy itself.
Use production capacity/profile defaults in one workload and small explicit ring
capacities in boundary cases. Record source generation and source sample range
for each accepted input, publication and completed listener frame.

Required fixed regression fixtures and assertions:

- Two independent listeners: normal exact delivery and one paused for three
  seconds. Derive the expected end from the actual production profile; FLAC's
  existing fresh timeout is two seconds, but a pause while retained data exists
  is not absent capture. Include a forced overrun with small capacity and verify
  the healthy peer remains current, irrespective of the failed peer's outcome.
- Absent capture before admission and during partial codec assembly: listeners
  terminate on their real budgets, partial codec input remains until a later
  valid continuation or generation reset, and no zero samples are fabricated.
- Generation reset after partial assembly and during an atomic copy (existing
  capture-fixture hook): discard rejected scratch, no mixed-generation block,
  old encoded cursors fail, and a fresh reconnect decodes exact new source PCM.
- Partial writes/backpressure: repeated short sends, EINTR, writable-then-
  WouldBlock, HUP/error readiness, and final partial-frame timeout. Verify the
  complete decoded prefix, exact accepted incomplete tail and terminal reason.
  Do not count a partly sent codec frame as delivered samples.
- Churn/disconnect at pre-admission, claimed slot, header and payload phases;
  delayed successful close retains slot/lease, rejection of excess listeners,
  and subsequent readmission. Failed close is reset-required with resources
  retained, not a reclamation pass. Real allocation/spawn failure outcomes stay
  explicitly limited to environmental injection plus shared ownership checks.
- Runtime request cancel before enqueue/service and after expiry, worker service
  at a complete block boundary, force retirement, idle switch while an admitted
  or draining lease exists, and reconnect to current epoch. Use real token,
  request and outcome locks; virtual ordering supplements Loom, never replaces it.
- Same fixture/seed yields identical event log and bytes twice; another schedule
  can change decisions but cannot corrupt valid delivered PCM. Zero active slots
  after successful teardown, no expired request commits, bounded frame storage,
  no cursor advancement on rejected read, and exact source-loss accounting.

## Shared oracle and durable failure reduction

The Rust driver streams per-listener transport artifacts and source-range
manifests to a bounded run directory. Claxon checks exact PCM quickly. A named
uv script invokes real FFmpeg with -f flac and explicit pcm_s32le/framehash,
reusing decoder_integrity.FrameHash and validate_decoder_log from 4dss. Validate
sequential FLAC numbering/CRC through the existing decoder integrity gate,
check exact frame counts, and compare normalized stereo PCM hashes using fixed
sample-count blocks (not FFmpeg packet boundaries). Expected source PCM is
quantized by the documented FLAC16 clamp/shift contract and normalized to the
same s32 representation. No digital/analog equality claim.

For a terminal incomplete frame, store full accepted bytes and separately the
verified complete-frame prefix length/ranges from the shared listener's completed
write events. Decode only the complete prefix, assert its exact samples, and
independently assert the remainder bytes/terminal reason; do not silently strip
an arbitrary decoder error. A capture discontinuity terminates the old stream
under today's policy; each reconnect has its own stream header and numbering.
Missing final complete blocks fail via exact expected sample counts even when
FFmpeg emits no error. Negative controls omit, duplicate, corrupt and truncate
real frames through the same validator entrypoint and must fail.

Version fixtures under core/tests/fixtures/stream-replay/ with schema version,
explicit ordered events, seed/generator identifier, capacities/limits and named
expected invariant/outcome. Stable explicit event lists outlive PRNG revisions.
A test-only reducer removes dependency-valid event chunks, then shrinks times,
write sizes and repetition counts. Each candidate reruns the actual pipeline and
preserves the same failure predicate (not merely any panic). Renumber insertion
IDs deterministically after removal; preserve admit-before-disconnect and actor
identity dependencies. Regression tests consume minimized checked-in fixtures;
artifacts record the original and minimized cases. Do not silently rewrite
fixtures on passing CI runs. Parent reviews new production-failure regressions
only when evidence supports the host fault boundary.

Add a finite replay stage to scripts/check: deterministic Rust orchestration
cases plus the actual FFmpeg validator negative controls. Retain real sockets,
Loom, firmware compilation and HIL gates. Full 89g8 completion needs all required
scenarios, oracle use, reducer/fixtures and canonical evidence, not another
standalone codec pass. Hardware scheduler/radio/DMA causation remains outside
what this deterministic run can establish. Future dctv/6vct recovery/media-age
policies must change the shared production decisions before expected traces;
this refactor preserves current fail-closed behavior throughout.

## Architecture-review fixes

| Review contract | Concrete correction | Verification required |
| --- | --- | --- |
| Blocking operations | Preserve original blocking locks and their order; steps have finite work, no wall-time guarantee; replay requires no externally held guards. | Real contention/Loom stay separate; reject scheduler-held-lock fixtures. |
| Deadline/retirement edges | Own attempt-start/finish clock reads; distinguish successful late reads from empty expiry, raw inner/outer clocks, snapshot admission, buffered progress time and outer-loop-only retirement. | Literal before/at/after cases and synchronized real characterization. |
| Runtime terminal ownership | Actual begin/enqueue, consuming PendingSwitch poll/finalize, immediate disconnected-channel finalization, single outcome take, commit clock after original locks. | Same-format revoked token, prepared/enqueued revoked token, early disconnect, late worker/caller races and single consumption. |
| Delayed close | Delay invoking actual synchronous close while full socket/Slot/Lease actor remains owned; retain failure plus both guards through reset-required terminal; success releases close/slot/lease in order. | Admission blocked while draining, setup/spawn failures and failed-close authority retained. |

The revised contract passed architecture re-review and is approved for
implementation. Production extraction and its acceptance checks remain in progress.
