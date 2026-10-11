# Concurrent Admission Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a complete stream request progress while one earlier peer holds an incomplete HTTP head, without changing admission or cleanup policy.

**Architecture:** Two persistent admission workers share one `Arc<Mutex<TcpListener>>`. The mutex covers only blocking `accept`; each worker releases it before parsing or handling its owned socket. A one-shot Pending/Run/Exit startup gate prevents partial startup from accepting requests.

**Tech Stack:** Rust std sockets/synchronization, existing stream_http parser/deadline writer, ESP-IDF 5.5.5 ThreadSpec and lwIP.

**Spec:** `target/kata-hardening/admission-resume.md`, with ownership details in `target/kata-hardening/admission-policy-decisions.md`. This tracked plan includes the binding decisions because those supporting audit artifacts are ignored.

**State:** Tasks 1–2 implemented and independently reviewed; all 27 canonical stages pass. Actual device and resource acceptance remain pending. Existing finish-all authorization covers implementation; no additional architecture permission gate. Compaction count: 0.

## Global Constraints

- Preserve port 8000, exact routes/status bodies, 4096-byte head cap, two-second absolute head allowance starting after peer formatting, and completed two-second absolute refusal writer.
- Keep two shared stream slots, Runtime::admit's compressed lease authority and lease-before-slot order, and allocation before HTTP 200.
- Pending heads hold no slots or codec leases. Do not move socket-close configuration earlier.
- Preserve current implicit drops before ListenerConnection ownership, including configured full-slot refusal. Do not claim checked close on these paths or add a pre-admission restart trigger.
- ListenerConnection owns the socket/slot/lease once admitted; allocation and listener-launch failure retain it through checked close. Successful close releases capacity before reporting. Failed close retains concrete failure and guards through restart; never reconstruct or retry an uncertain FD.
- ThreadSpec launch success remains success after pthread reset failure. Reuse finish_spawn and its tests; no retries, supervisor or target join claim.
- Both workers use core 1, priority 7, 4096-byte stacks. One additional stack is internal RAM, plus actual task/synchronization/socket overhead that requires target measurement.
- Failed admission startup does not roll back the already started encoder/runtime. No supported start retry is introduced. Target panic_abort gives no destructor guarantee.
- No new queue, per-head task, dispatcher, nonblocking test mode, application shutdown framework, parser rewrite, or service latency guarantee.
- Tests exercise real blocking production accept and shared head parsing; test teardown uses real loopback wake connections.

## Review Focus

- Second worker launch fails after first succeeds: the first exits before accepting and readiness stays absent (Task 2).
- Listener launch fails after slot/lease transfer: checked-close ownership remains intact (Task 2).
- Valid write-half-close or fragmented request: shared parsing keeps current successful request behavior (Task 1).
- Two workers are occupied: third request waits for a worker and eventually gets the correct response; no burst fairness claim (Task 1).
- Added task competes with audio/TLS for internal memory: measure heap, largest block, stacks and sockets before target acceptance (Task 3).

## File ownership

- Create `firmware/src/stream_admission.rs`: shared blocking accept-one operation, actual head-read operation, and one-shot startup gate; no ESP dependencies.
- Modify `firmware/src/lib.rs`: register the module.
- Modify `firmware/src/stream.rs`: use shared operations, launch two gated admission workers, preserve answer and listener ownership.
- Create `core/tests/stream_admission.rs`: import the production module by path; real socket and gate regression.
- Extend `core/tests/thread_spawn.rs` only for missing actual launch/ownership coverage; preserve existing finish_spawn coverage.
- Extend `core/tests/listener_telemetry.rs` only for missing real Runtime/Slot/ConnectionResources transitions. Do not duplicate covered transitions or modify close semantics for tests.
- Update this plan and project gotchas with verified results. Parent owns separate device receipts and Kata updates.

### Task 1: Shared blocking acceptance and slow-head regression

**Interfaces:** Create `accept_one(listener: &Mutex<TcpListener>) -> io::Result<(TcpStream, SocketAddr)>` and `read_head(socket: &TcpStream, deadline: Instant, max_bytes: u64) -> io::Result<String>` in stream_admission. `accept_one` releases its lock before returning either result. `read_head` uses the existing DeadlineReader, 256-byte BufReader and read_request_line; caller retains socket ownership and creates the deadline at the existing point after peer formatting. Mutex poisoning becomes an io error rather than an unwrap panic.

- [x] Write `complete_second_request_progresses_before_first_head_deadline` using actual blocking accept_one and read_head on real loopback sockets. Coordinate with channels after accept; worker one holds an incomplete head. Worker two reads a complete request and writes the actual Status response via existing write_response. Assert exact response bytes before the first two-second deadline. The fixture handler only observes the extracted operation; it does not claim to test firmware route/allocation handling.
- [x] Run `cargo test -p esp-shazam-core --test stream_admission complete_second_request_progresses_before_first_head_deadline -- --nocapture` against a one-worker fixture. Record actual RED from response timing, not a missing symbol. First extract current operations without concurrency if needed to obtain this behavioral RED. Use bounded client reads, close clients on failure, wake blocked accepts with loopback connections, and join every host thread even when assertions fail.
- [x] Implement the two-worker fixture using the same blocking production operations. Each worker performs a finite known number of accept/handle iterations; teardown supplies wake connections for remaining accepts and drops all clients so head reads finish. There is no application stop flag or host-only socket mode.
- [x] Wire production stream::accept to accept_one and answer's existing head-read site to read_head. Keep accept-error logging/sleep outside the mutex. Keep all peer lookup, parsing, routing, allocation, refusal and close outside the lock. Do not restructure the rest of answer.
- [x] Add real socket cases for silent and byte-trickled heads reaching the absolute deadline, fragmented valid head, valid write-half-close, malformed UTF-8, over-cap head and peer disconnect. Reuse existing exact-cap/parser unit coverage; add an integration exact-cap case only if the extracted path reveals a gap. Every thread has bounded reads and a teardown owner.
- [x] Add `two_slow_heads_delay_third_until_worker_available`: synchronize both accepted states, send a complete third request, then release one slow peer and verify complete correct response. Record connect-to-response separately from accept-to-response. Do not infer a six-second bound or arbitrary-burst fairness.
- [x] Run `cargo test -p esp-shazam-core --test stream_admission` and existing stream_http unit tests through `cargo test -p esp-shazam-core stream_http`. Inspect all output; no new warnings or errors.
- [x] Review the small shared module and diff, then commit the independently testable extraction/regression with explicit file staging and a conventional commit. Production still has one worker until Task 2; do not claim 8s9m fixed yet.

### Task 2: Two gated workers with unchanged admission ownership

**Interfaces:** Add `StartupGate::new() -> Self`, `wait(&self) -> bool`, `run(&self)` and `exit(&self)` to stream_admission. Internally use a mutex/Condvar and private Pending/Run/Exit state. `wait` blocks only while Pending and returns true for Run, false for Exit. State publishes once; no reusable lifecycle or task registry. Keep ThreadSpec::spawn and finish_spawn signatures unchanged.

- [x] Write gate unit tests for waiting while Pending, releasing two real waiting threads on Run, and releasing waiting threads on Exit. Assert no accepted socket before Run. Synchronize worker entry with channels rather than sleeps; use real loopback connections to observe acceptance after Run.
- [x] Write startup failure coverage: first actual Builder launch succeeds and waits; second actual launch fails using the existing rejected-stack pattern; parent publishes Exit. Join the host first worker and assert no handler ran and its listener Arc was released. Reuse finish_spawn to establish that successful launch plus reset error advances startup exactly once, without retry. Cover zero-worker failure dropping the captured listener closure too. These inspect real thread outcomes and ownership, not a fake spawner.
- [x] Launch exactly two named ThreadSpecs in stream::start using cloned listener/ring/runtime/listener-count ownership and shared gate. Use names `stream-in-1` and `stream-in-2`, each core 1/priority 7/4096-byte stack. On either Err publish Exit and return the error; on both Ok publish Run before existing mDNS/readiness. Workers return immediately on Exit. No target join is promised; last worker Arc release can be asynchronous.
- [x] Inspect all answer branches against the existing ownership table. Preserve Runtime::admit before slot, configure before slot, full-slot refusal lease lifetime, ListenerConnection construction before allocation and closure capture before LISTENER_THREAD.spawn. Do not add an admission socket owner or change checked-close behavior.
- [x] Extend meaningful ownership tests only where missing: actual listener launch failure drops a captured ConnectionResources owner through its close handler, successful close releases slot/lease before publication, failure retains them, and two occupied slots refuse without a new listener buffer. Use real Runtime/Slot/ConnectionResources plus real sockets; controlled close outcomes may inspect existing ownership seams but are not ESP close evidence. Existing allocation-before-200 coverage must remain; if no host seam covers the firmware allocation failure branch, record that gap for Task 3 device/fault acceptance rather than invent a parallel fake answer.
- [x] Run `cargo test -p esp-shazam-core --test stream_admission --test thread_spawn --test listener_telemetry`. Verify the existing reset-failure tests still pass and the two-worker regression exercises the same operations production uses. Review and commit only the task files.

### Task 3: Canonical and actual service acceptance

**Interfaces:** No new application behavior. Parent runs the existing canonical check and device harnesses when independent hardware work permits; implementation must not claim target acceptance before these receipts exist.

- [x] Run `scripts/check`; retain its run directory, stage exits and warnings inventory. This includes workspace tests, lint/format checks, concurrency models and pinned target build when configured. Confirm `firmware-build` actually ran; a skipped target build is not a target pass. If needed run the canonical target build from firmware: `cargo +esp build --release` with the established environment, without changing dependencies or board settings.
- [ ] Run real firmware end-to-end admission on the board: hold one incomplete request, submit a complete supported stream request and verify valid HTTP/header/audio bytes before the first head expires. Repeat fragmented and write-half-closed requests, full two-stream capacity 503 with exact bytes, both pending heads followed by a complete request, and disconnected peers. Use existing harness conventions; synchronize observed acceptance or explicitly report when device observability cannot establish order.
- [ ] Record internal free heap and largest block, both admission task stack high-water marks, task count and socket occupancy with two pending heads plus two streams and concurrent TLS/audio work. Compare against the unchanged baseline; account for the added 4096-byte task stack and measured overhead. Zero task/slot/lease growth across finite cycles is required. No invented heap threshold substitutes for actual resource/error evidence.
- [ ] Validate actual allocation/listener-launch failure behavior using an existing bounded device fault seam if available. Do not create broad fault modes in application code. If unavailable, explicitly report which branch has only host ownership evidence and code review; target acceptance remains limited accordingly.
- [ ] Keep MP3 delivery/encoder loss as separate existing failed gates. Report admission response observations without claiming audio reliability or hard end-to-end latency from head/refusal deadlines. Device startup panic/abort is failure evidence, never unwind-cleanup proof.
- [ ] Perform final fresh review of shared-listener lock lifetime, startup exit/run ordering and every socket/slot/lease terminal path. Update the plan, gotchas and Kata with actual receipts and remaining limits. Commit verified implementation; parent owns shipping/PR workflow under existing authorization.

## Recommendation and residual risk

Proceed with Tasks 1–2 after the separate audio acquisition completes. The accept-only mutex is the smallest supported path: pinned lwIP documentation requires locking shared control blocks even though full-duplex implementation contains protected socket references and mailbox waiters. Do not rely on two simultaneous accepts.

The material remaining risk is target memory/scheduling pressure from the added persistent worker; Task 3 measures it before acceptance. Both workers can still be occupied by slow heads, and current logging/implicit close paths prevent a hard response-latency promise. Encoder/runtime survival after startup failure and panic-abort behavior remain existing limitations, explicitly unchanged by this scope.

Task 1 receipt: behavioral one-worker RED, ten real-socket admission tests and
31 existing HTTP tests pass without warnings. Independent spec and quality review
passes with no findings. Production still has one worker; canonical combined
implementation and actual device acceptance remain Tasks 2–3. Detailed logs and
review live in .superpowers/sdd/2026-10-10-concurrent-admission/.

Task 2 receipt: actual startup/launch RED retained, 15 admission + 3 thread-spawn
+ 21 telemetry tests pass; scoped Clippy/format pass. Independent spec and quality
review has no findings. Parent scripts/check passes all 27 stages including the
actual pinned firmware build, IRAM audit and firmware warning check in
`target/check-logs/run.QSVJJTto`; complete/equal boundary snapshots verified.
No compiler warnings. Full firmware answer allocation/launch and full-slot
behavior still need device evidence; host close ownership is not ESP close proof.
