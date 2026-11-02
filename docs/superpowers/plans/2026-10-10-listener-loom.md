# jq3a implementable Loom plan

Implemented and independently reviewed. Ten models and all 21 canonical stages
passed, including firmware compilation. See October 10 listener-loom evidence.

Read-only preparation, 2026-10-10. No source, tests or manifest changed. Doctor Biz's missing machine reference: `~/.codex/docs/local.md` does not exist here.

## Boundary and smallest sharing change

`core/src/stream_wait.rs::Slot` is the actual firmware admission guard: AcqRel/Acquire fetch_update caps admission at two and AcqRel fetch_sub releases ownership. Its current std atomics are invisible to Loom. `firmware/src/stream_runtime.rs` owns the other real protocol: admission and switch commit share `active`; commit locks token, outcome, active in that order; a Release retired flag and lease count protect old epochs. `core/tests/runtime_switch.rs` and stream_properties already compile that firmware file through #[path]. There is no core stream_runtime module or stream_runtime.rs integration test.

Add host-only `listener-loom = ["dep:loom"]`, reusing pinned =0.7.2. Do not overload capture-loom: capture models currently replace ring atomics and use std Arc, and the two model families need isolated invocations. Add a compile_error for listener-loom on espidf, and preferably reject enabling both model features together. Default firmware feature set and public argument types stay std.

For Slot only, select loom Arc/AtomicUsize/Ordering under listener-loom and std otherwise. Keep stream_wait's unrelated Mutex imports std, since its timed/socket helpers are outside this model. Gate ordinary stream_wait unit tests off under listener-loom. No duplicated admission algorithm, replacement atomic orders, or test-only implementation of Slot.

In firmware stream_runtime.rs, select loom Arc/Mutex/AtomicBool/AtomicUsize/Ordering under listener-loom and std otherwise. Keep mpsc, Instant, codec/ring allocation and data processing unchanged. Add a private cfg(test, listener-loom) models module in that same file, able to construct Request and invoke actual Runtime::admit and Worker::commit. New core/tests/listener_runtime_loom.rs contains the same production #[path] imports as runtime_switch and a listener-loom crate gate. Gate normal runtime-switch/property targets off for this feature where their std synchronization arguments otherwise conflict. Inventory other #[path] users before editing. Ordinary cargo test --workspace still exercises all ordinary targets.

Models construct actual Prepared/Runtime and bounded Requests; they do not call switch's wall-clock/channel wait loop. Make deadlines far in the future for concurrency cases, and already expired for expiry cases. Avoid sleeps and wall-clock assertions. Never call ordinary timed read helpers inside Loom. mpsc notify remains a single nonblocking notification whose outcome is ignored by commit; do not claim channel delivery or timeout scheduling was modeled.

This is routine test instrumentation within existing contracts. No firmware architecture change, allocator change, replacement runtime, cancellation API, or compatibility shim is needed. If implementation instead proposes extracting a generic runtime/connection framework, stop: instrumentation of the already host-compilable firmware file is smaller.

## Models and assertions

All instrumented synchronization and all participating threads use Loom. Define an explicit type witness/compile-time assignment in model tests for Slot's counter and runtime token/active mutex types to catch accidental std imports.

1. Slot saturation: parent plus three contenders, each tries actual Slot::take(limit 2). Each successful contender retains its guard until it has observed count <=2, then releases it. Assert zero after joins and exactly two fresh admissions plus a third refusal afterward. Separate case begins with two guards, races release against admission, and allows either valid admission outcome; final count is zero. Avoid assertions equating separately loaded count with a separate observer count during concurrent transitions.
2. Cancellation/drop: actual guards are captured by worker closures; normal return and early return release them. Race an admission against another guard dropping. This covers Rust ownership cancellation before worker handoff, not Sonos Stop or socket cleanup.
3. Spawn failure ownership: construct a closure holding a real Slot and explicitly drop the unlaunched closure while another thread attempts admission. This models Rust's closure-destruction branch after failed launch, not pthread failure itself. Retain/extend core/tests/thread_spawn.rs with a real std Builder rejected-stack launch capturing Slot; require observed count zero afterward. That ordinary test establishes the modeled branch corresponds to real std behavior.
4. Actual runtime cancellation/commit: prepare a request, race token=false under the instrumented mutex against Worker::commit. Assert either commit linearizes before revocation or cancellation wins and active identity/retired state stays unchanged. After revocation joins, a second request using the old token cannot commit. Instrumented active Arc identity, outcome and retired observations must match actual commit result. State explicitly that token revocation is the boundary modeled; CommandSender acceptance/final TLS sends remain ordinary tests.
5. Admission versus idle commit: race Runtime::admit against an actual force=false commit. If admission owns the old epoch before commit's idle check, commit must refuse; if commit wins, admission leases the new epoch. Hold the returned lease until joins so the oracle cannot mistake an already released lease for an admission violation. A force=true variant may retire the leased old epoch; existing lease still observes its immutable original epoch.
6. Reclamation: use Loom Arc strong_count after threads join, drop all leases/requests/worker owners and verify no unintended owner remains. Verify runtime compressed-lease count returns to zero separately from socket Slot count: these are distinct counters by design. If stronger destructor evidence is needed, attach a test-local tracker outside production payload and be precise about what it proves. Do not claim Arc count proves ESP socket PCB or PSRAM reclamation.
7. Expiry/outcome: directly expired request refuses commit; separately pre-populated outcome refuses commit. A bounded concurrency case can race outcome revocation under its actual mutex against commit, without pretending a real clock timeout is explored.

## Bounds, evidence and verification

Start each model with Builder max_threads=4 (parent included), max_branches=1000, preemption_bound=Some(2), max_permutations=Some(10000), matching current capture models. Keep each operation sequence one-shot, split scenarios rather than adding loops. Record effective bounds and whether the permutation cap truncates exploration; a bounded passing result is not an exhaustive proof. Reduce construction/thread count only with a documented reason, never by replacing production operations.

Dedicated command, after mise environment activation:
`cargo test -p esp-shazam-core --features listener-loom --lib --test listener_runtime_loom models::`
Check this command discovers every intended model, not zero tests. Add a separate listener-loom stage to scripts/check; leave its capture-loom stage intact. Preserve existing pinned Rust 1.93.0 host and +esp firmware builds. Canonical scripts/check must pass: formatting, lint, ordinary tests, both isolated model invocations, existing scripts and actual firmware build. No tool install or hardware operation belongs to this prep. HIL remains required evidence before claiming deployed listener recovery, and must be tracked separately if no hardware run is authorized/available.

Failure artifacts: retain full model invocation, bounds, model name, assertion and backtrace under audit evidence; turn a found race into an ordinary targeted regression where feasible. Loom's checkpoint_file requires its optional checkpoint dependency feature (confirmed in installed 0.7.2 source); do not set it without enabling that feature. Only add checkpoint support if a real failure needs schedule persistence; otherwise avoid another dependency-feature change. A checkpoint is a replay aid, not a standalone proof.

Limits: no FreeRTOS task creation, ESP-IDF close/FFI, ISR timing, network behavior, deadlines, allocator failures, audio throughput or hardware ordering completeness. Slot release through socket close remains covered by real socket/production close tests and HIL; do not replace ListenerConnection with a pretend Loom socket and claim the close path was tested. Runtime models cover the actual shared-state commit boundary, not channels or the complete Play/Stop transaction.

## Ownership and sequence

Parent keeps canonical checks on the frozen tree. This document is the only preparation artifact. Implementation owner first adds failing bounded ownership assertions/type witnesses against actual code, then instruments imports and enables the isolated model targets; confirms ordinary std tests and firmware still compile. A second read-only reviewer checks cfg isolation, model discovery, actual-code reuse, ownership oracle validity and limits before completion. No commits, hardware or network access performed during preparation.
