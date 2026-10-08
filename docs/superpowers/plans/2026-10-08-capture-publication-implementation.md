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

- [x] First reproduce the old failure with a real reader-held mutex and >45 ms publication; retain red output for the claim that all produced samples remain available without waiting for that reader.
- [x] Add failing real tests for complete-block visibility, partial input/discard, two readers, stereo identity, partial output, capacity edge, reset, wrap/carry/era exhaustion and unchanged cursors on rejected reads.
- [x] Implement unique publisher, atomic payload, seven-word header, slot stamps and bounded validation per approved design. No hot allocation or non-atomic aliasing. A copy becomes usable only after slot and header checks.
- [x] Add a concise ordering argument adjacent to the protocol. Prove overwrite-before-head, reset rejection and era handling; do not treat SeqCst alone as proof.
- [x] Verify available proptest/Loom versions from primary sources, pin host test dependencies and exercise actual protocol code with tiny rings. Bound model exploration explicitly and report limits.
- [x] Add real threaded delayed-copy tests plus deterministic WAV packing/FLAC decode tests at the 4,096-frame boundary. Test real loopback socket pressure without application mock modes.
- [x] Run host fmt, clippy and workspace tests; independent review before integration. Report exact APIs and red/green evidence. Parent commits named files after review.

### Task 3: Integrate firmware consumers and publisher

Files: firmware/src/audio_in.rs and firmware/src/stream.rs, narrowly needed real host tests for shared wait policy.

- [x] Write failing shared-policy tests for Retry, caught-up timeout, overrun and cursor initialization; retain encoded-ring lock behavior where still needed.
- [x] Use the unique publisher on capture; remove contention staging and count only partial-block loss on real invalidation. Preserve recognition and health behavior.
- [x] Switch WAV and FLAC raw readers to bounded atomic reads and cursor initialization. Never feed scratch from rejected reads into encoder/socket. Preserve freshness/socket deadlines and listener cleanup.
- [x] Keep existing telemetry contracts accurate: staged frames are partial assembly; obsolete contention counters must not pretend to measure work. Update consumers/tests/docs together if a field changes.
- [x] Verify actual payload PSRAM, metadata/refcount internal allocation and fixed sizes. Fail startup explicitly if required placement is unavailable; do not add an unreviewed allocator bridge.
- [x] Parent builds both recognition profiles, with recognition-on never flashed. Inspect actual linked publisher/reader instructions and allocation placement. Independent integration review.

### Task 4: Hardware acceptance and final review

Files: diagnostic-only delayed-reader fixture if needed; docs/audit-evidence/2026-10-08/capture-publication.md/.json; plan/Kata updates.

- [x] Add a bounded real delayed-reader diagnostic that cannot pause the producer or depend on log output for cleanup. Parent runs it with one/two listeners at roughly 110–200 ms and beyond history capacity.
- [x] Require no new capture publication drops, DMA faults or restart/gap increments during isolated reader delay; allow explicit stale-reader overrun. Record actual samples and memory/task cost.
- [x] Repeat three-cycle real TLS, NVS writes and Wi-Fi reconnect using existing fixtures; retain failures and distinguish DMA faults from publication loss.
- [x] Run canonical scripts/check, independent whole-change review and parent fresh-eyes. Host checks passed; hardware failures remain explicit below.
- [ ] Restore production candidate only after checks, verify saved configuration/stopped autoplay-off state, and run exact two-listener decode acceptance.
- [ ] Commit evidence and update dp08 with satisfied and remaining gates. Close only if acceptance passes; keep unrelated DMA/settling work open.

## Progress

- Design approved; no new architecture approval needed for these tasks. Task 1 running as a throwaway feasibility probe. Task 2 can prepare failing host tests in parallel; firmware adoption waits for feasibility and reviewed core API.
- Compaction count for this implementation: 2. Resume from worker reports and commits; do not repeat completed diagnostics.

- Task 1 passed the compiler probe at optimization 3 and s: direct 32-bit SeqCst load/store with MEMW, no calls or unresolved symbols. Actual linked firmware/placement/cost gates remain open.
- Task 2 retains an old-path behavioral red showing 8,640 lost frames under contention. Workers corrected early drafts that preceded individual contract tests; those drafts were removed and behavioral TDD restarted. No hardware changes yet.

- Production source checkpoint `1934c94`: core/integration independent review clean, host workspace/property/codec tests and bounded Loom models pass. Actual default and recognition-on ESP builds pass; enabled profile never flashed, configuration restored exactly. Canonical 20 stages pass.
- Diagnostic checkpoint `940a8ce`: real-copy pause fixture with fixed numeric event storage; reporting follows buffer/ring release and recovery wait. Three actual event-store tests wired into canonical host suite. Final canonical 20 stages pass again.
- Candidate production `34bf5394` passed 45 seconds/two listeners, exactly 2,160,000 frames each and no added tracked capture faults. Startup faults persist. Four steady windows measured 9.306% core versus earlier control 8.167%; internal minimum sum 69,115 vs72,831 bytes and PSRAM768,820 vs754,292.
- Hardware underway: flashing delayed-reader image `3a354eaa`; run one/two listener pause tests next, then rebuilt TLS/NVS/network/capture-health fixtures. Saved baseline and candidate ELF files are under target/capture-publication. Keep original UART on failure and restore candidate production/settings at end.

- Delayed-reader diagnostic passed 120 seconds with one listener and two listeners on repeat: each completed 5,760,000 frames. Both short-copy coherence and beyond-history rejection/fresh-read cases passed without added capture faults. First two-listener failure remains failed: a listener left before injection, with the other completing. TLS/NVS/network/current-health acceptance continues.

- Three-cycle TLS with two120s listeners passed exact5,760,000 frames each and unchanged capture faults. NVS120×1,024B verified writes with two90s listeners passed exact4,320,000 each. Forced Wi-Fi disconnect/reconnect passed with both slots released, two fresh480,000-frame streams and unchanged capture counters. Current capture health/WAV and final production restore remain.


## Current checkpoint: hardware acceptance failed

Source commits 1934c94 and 940a8ce passed host/model tests and linked target review.
Harness fix 4aa931f passed all twenty canonical stages and independent review;
real paced FLAC exposes and fixes container-probing delay without weaker gates.
The corrected board health rerun still failed. Capture-only supervisor checks
passed, but full FLAC/WAV recovery decoder acceptance did not.

Connected-input standalone settling completed ten valid trials (480,000 frames).
I2S-only restarts were stable from block zero; codec-soft-reinit transients extended
to 490 ms under the documented descriptive limits. Retain the 500 ms policy and
keep ed4k open for physical cold-start and production recovery work.

Production is restored to image 36c559ca; saved firmware and Sonos configurations
match exactly, controller stopped, autoplay/recognition off, host playback ended.
Final 45-second two-listener acceptance failed at 1,990,656 / 532,480 frames against
2,160,000 each. Original UART retained for 120 seconds after failure; capture
counters remained unchanged. Do not mark the combined restore/acceptance checkbox
complete or close dp08. Detailed receipts live in the October 8 audit evidence.

Next: collect a bounded packet trace alongside the current-format stream test
for pd6c. sudo authentication blocks capture from this session; the prepared local
terminal command is scripts/capture-stream-packets 192.168.23.217 enp5s0 600.
Do not blindly repeat until green or change Wi-Fi/buffering without evidence.
