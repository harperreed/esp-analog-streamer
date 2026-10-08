# Capture Publication Implementation Plan

> **For agentic workers:** Use subagent-driven-development task by task, with TDD and independent review. Parent owns Git, firmware builds, flashing and hardware; Sol workers own disjoint source changes.

**Goal:** Prevent a delayed raw-audio reader from blocking I2S publication while preserving WAV and FLAC contracts.

**Architecture:** One preallocated atomic sample history, one unique publisher, independent read cursors. Complete 240-frame blocks; SeqCst metadata and payload, bounded snapshot attempts, checked era/position exhaustion.

**Tech Stack:** Pinned Rust host/ESP toolchains and ESP-IDF 5.5.5; real Rust tests, property tests and Loom on the production protocol, FFmpeg sample accounting, real-board fixtures.

**Spec:** [Approved design](2026-10-08-capture-publication.md). Doctor Biz approved with “continue”; execution remains Sol subagents. Implementation details below resolve the design's remaining bounded choices.

## Global constraints

- Preserve 48 kHz stereo/24-bit raw WAV, shared 16-bit FLAC, gain/routing, two-second off-profile and half-second recognition-on history.
- No reader-owned locks, hot-path allocation or publisher retries waiting for consumers. No 64-bit atomics or non-atomic shared payload.
- No speaker commands, credentials changes, Wi-Fi changes or live recognition calls. Parent serializes hardware/build work.
- Preserve 500 ms settling and existing capture health recovery. DMA starvation remains a separate open failure.
- Work on the current WIP branch, preserve .kata.toml and docs-ignore; no push/merge. Canonical verification is scripts/check.
- One header snapshot attempt per read phase. Contention returns Retry; firmware waits one existing tick within the existing two-second freshness deadline. No spin loop.
- Exhaustion checks happen before mutation. A terminal publication error stops publication and uses existing fatal firmware restart handling; it must never continue under recycled identities. Test seeded boundaries.
- Property/model tests must exercise the same ring protocol. Any pause injection belongs in test/diagnostic code and must compile out of production.

## Review focus

- A paused reader must not stop the writer; task 2 includes a real barrier-paused copy and independent reader.
- Reset or overwrite during a copy must never leak mixed audio; task 2 models these interleavings and tests unchanged cursor after rejection.
- A FLAC frame spans partial raw blocks; tasks 2/3 test 4,096-frame assembly and exact stereo reconstruction.
- Target atomics may use hidden locks; tasks 1/4 inspect actual instruction lowering and linked final paths, plus memory placement.
- Startup faults and stale summary counters can mask new faults; task 4 retains original UART and separate pre/post setup observations.

## File ownership and interfaces

Core worker owns core/src/stream_ring.rs, core/src/capture_buffer.rs, affected core/src/stream_wait.rs tests, core tests, Cargo manifests/locks and narrow canonical model-test wiring. Firmware worker starts after core interface freeze and owns firmware/src/audio_in.rs, firmware/src/stream.rs and a diagnostic example if required. Parent owns plan/evidence/Kata and all device actions.

Use a unique non-Clone `CaptureBuffer` publisher token to preserve the existing domain name. Construction returns the publisher and `Arc<StreamRing>`; it accepts capacity and block size, allowing small real models while firmware pins 240 frames. `publish(&mut self, &[i32])` and `invalidate(&mut self)` return explicit exhaustion errors; invalidation returns the actual partial-frame discard count. `staged_frames` now means partial assembly only. Reader API keeps capacity, cursor_now and read: read returns frame count (zero means caught up), with Retry distinct from Overrun. Cursor initialization can return Retry. Freeze concrete signatures in the worker report before firmware starts; no compatibility wrapper retaining the old mutex path.

### Task 1: Verify atomic feasibility

Files: throwaway probe and assembly under target/capture-publication; report under .superpowers/sdd/2026-10-08-capture-publication.

- [x] Compile minimal actual AtomicI32/U32 SeqCst load/store operations with installed rustc +esp and the exact Xtensa target.
- [x] Inspect assembly and callees; record compiler identity, flags, hashes and instructions. Require no PSRAM CAS, 64-bit helper or reader-held/global lock in the copy/publication path.
- [x] Confirm ordering/placement limitations against pinned source. Failure is a design feasibility result; do not weaken atomics to force a pass.

### Task 2: Implement and verify shared core history

Files: core/src/stream_ring.rs, core/src/capture_buffer.rs, affected stream_wait tests, core/tests/capture_publication.rs, narrow Loom/property test support.

- [ ] First reproduce the old failure with a real reader-held mutex and >45 ms publication; retain red output for the claim that all produced samples remain available without waiting for that reader.
- [ ] Add failing real tests for complete-block visibility, partial input/discard, two readers, stereo identity, partial output, capacity edge, reset, wrap/carry/era exhaustion and unchanged cursors on rejected reads.
- [ ] Implement unique publisher, atomic payload, seven-word header, slot stamps and bounded validation per approved design. No hot allocation or non-atomic aliasing. A copy becomes usable only after slot and header checks.
- [ ] Add a concise ordering argument adjacent to the protocol. Prove overwrite-before-head, reset rejection and era handling; do not treat SeqCst alone as proof.
- [ ] Verify available proptest/Loom versions from primary sources, pin host test dependencies and exercise actual protocol code with tiny rings. Bound model exploration explicitly and report limits.
- [ ] Add real threaded delayed-copy tests plus deterministic WAV packing/FLAC decode tests at the 4,096-frame boundary. Test real loopback socket pressure without application mock modes.
- [ ] Run host fmt, clippy and workspace tests; independent review before integration. Report exact APIs and red/green evidence. Parent commits named files after review.

### Task 3: Integrate firmware consumers and publisher

Files: firmware/src/audio_in.rs and firmware/src/stream.rs, narrowly needed real host tests for shared wait policy.

- [ ] Write failing shared-policy tests for Retry, caught-up timeout, overrun and cursor initialization; retain encoded-ring lock behavior where still needed.
- [ ] Use the unique publisher on capture; remove contention staging and count only partial-block loss on real invalidation. Preserve recognition and health behavior.
- [ ] Switch WAV and FLAC raw readers to bounded atomic reads and cursor initialization. Never feed scratch from rejected reads into encoder/socket. Preserve freshness/socket deadlines and listener cleanup.
- [ ] Keep existing telemetry contracts accurate: staged frames are partial assembly; obsolete contention counters must not pretend to measure work. Update consumers/tests/docs together if a field changes.
- [ ] Verify actual payload PSRAM, metadata/refcount internal allocation and fixed sizes. Fail startup explicitly if required placement is unavailable; do not add an unreviewed allocator bridge.
- [ ] Parent builds both recognition profiles, with recognition-on never flashed. Inspect actual linked publisher/reader instructions and allocation placement. Independent integration review.

### Task 4: Hardware acceptance and final review

Files: diagnostic-only delayed-reader fixture if needed; docs/audit-evidence/2026-10-08/capture-publication.md/.json; plan/Kata updates.

- [ ] Add a bounded real delayed-reader diagnostic that cannot pause the producer or depend on log output for cleanup. Parent runs it with one/two listeners at roughly 110–200 ms and beyond history capacity.
- [ ] Require no new capture publication drops, DMA faults or restart/gap increments during isolated reader delay; allow explicit stale-reader overrun. Record actual samples and memory/task cost.
- [ ] Repeat three-cycle real TLS, NVS writes and Wi-Fi reconnect using existing fixtures; retain failures and distinguish DMA faults from publication loss.
- [ ] Run canonical scripts/check, independent whole-change review and parent fresh-eyes. Fix new failures; retain known limitations honestly.
- [ ] Restore production candidate only after checks, verify saved configuration/stopped autoplay-off state, and run exact two-listener decode acceptance.
- [ ] Commit evidence and update dp08 with satisfied and remaining gates. Close only if acceptance passes; keep unrelated DMA/settling work open.

## Progress

- Design approved; no new architecture approval needed for these tasks. Task 1 running as a throwaway feasibility probe. Task 2 can prepare failing host tests in parallel; firmware adoption waits for feasibility and reviewed core API.
- Compaction count for this implementation: 0. Resume from worker reports and commits; do not repeat completed diagnostics.

- Task 1 passed the compiler probe at optimization 3 and s: direct 32-bit SeqCst load/store with MEMW, no calls or unresolved symbols. Actual linked firmware/placement/cost gates remain open.
- Task 2 retains an old-path behavioral red showing 8,640 lost frames under contention. Workers corrected early drafts that preceded individual contract tests; those drafts were removed and behavioral TDD restarted. No hardware changes yet.
