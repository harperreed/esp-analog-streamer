# Listener End Reasons Implementation Plan

> **For agentic workers:** Use the available subagent-driven-development or executing-plans skill to implement task by task. Parent owns review, integration, canonical checks, commits and hardware.

**Goal:** Fix Kata ce69 (parent pd6c, related wbz6): report encoded reset, retained-capacity loss, invalid cursor and epoch retirement accurately while keeping existing termination behavior.

**Architecture:** Replace the encoded ring's unit error with one reason enum produced by the existing checks. Carry that reason through the actual listener wait and deferred telemetry; keep generation ownership in the ring. This is an internal monorepo API migration, with no compatibility shim, recovery, resynchronization or new policy.

**Tech Stack:** Existing Rust core/firmware and Python harness modules.

**Spec:** This contract and `2026-10-10-kata-hardening.md`; explicitly authorized reporting bug. The earlier deterministic-replay plan's instruction to retain retirement-as-FellBehind is superseded only for diagnostic identity here.

## Global Constraints

- Same checks, precedence, lock order, attempts, clock reads, sleeps, deadlines, byte delivery, socket writes, close calls, Slot and Lease lifetimes.
- No raw-ring error migration: WAV and encoder raw Overrun behavior remains as implemented.
- Actual capacity loss retains End::FellBehind and the exact text `fell a ring behind`.
- Generation mismatch means an encoded generation changed; do not claim every reset was a capture restart or prove its underlying cause from this error alone.
- No added state, allocation, logs on producer paths, numeric codes, configuration, dependencies or firmware startup changes.
- Preserve stable format/capture/encoder enum values and telemetry counters. End and EndCode currently have no repr or numeric export; append their diagnostic variants and do not reorder existing variants or touch encoder_telemetry::Reason.
- Preserve original failed hardware records. Plan writing runs no tests/build/hardware and makes no commit.

## Review Focus

- Reset and overwrite simultaneously: generation mismatch still wins.
- Error completing at/after freshness expiry still beats timeout, with unchanged read_wait accounting.
- Retirement observed inside an acquired read lock returns Retired through the same wait-completion path.
- Invalid offset remains terminal, leaves cursor/output unchanged and never poisons the ring.
- Departure identity survives deferred export and parser classification without changing close/release pairing.

## Inspected scope

`core/src/flac_ring.rs::read` currently returns Overrun for generation mismatch, `behind > capacity`, and offset beyond the retained slot. Its tests already exercise all three. Actual encoded callers are the compressed pipeline, generic stream_wait tests/wrappers, buffer/codec/limits/property/FFmpeg tests and replay support. Generic FreshWait and read_locked_fresh already carry arbitrary error types; they need no policy change.

`firmware/src/stream_pipeline.rs` turns retirement inside Phase::Read into the same ring error and maps all read failures to FellBehind. `stream.rs` separately maps WAV/raw Overrun to FellBehind; leave that path alone. EndCode conversion is in listener_telemetry.rs and its Display implementation is in stream.rs.

`core/tests/replay_pipeline.rs` explicitly expects the retirement race to be FellBehind. `stream_replay.rs` and `fixtures/stream-replay/reset.json` expect reset to be FellBehind. `overrun.json` is genuine capacity lag and keeps its expectation. `support/replay.rs` stores Debug terminal names and checks explicit invariants; update reset identity, preserve the capacity-lag invariant.

Parser search found one reason classifier: `scripts/retention-check` currently sends unfamiliar reasons to socket_error. `encoder_fault_check.py` parses departure endpoints/timestamps without a reason whitelist; encoder_probe_check.py likewise recognizes generic departures. `fault_check.py` and encoder_fault_check.py deliberately assert the existing no-fresh-audio reason for their suspension tests; preserve those strict assertions. replay_stream.py accepts any nonempty terminal reason but limits incomplete-write reasons; preserve that write-tail rule. Add parser coverage in the actual imported harness tests, not a copied parser or synthetic HTTP server.

## Task 1: Typed encoded-ring failures

**Files:** `core/src/flac_ring.rs`, `core/src/stream_wait.rs` tests, `core/tests/stream_properties.rs`, `core/tests/stream_limits.rs`; any remaining compiled references to flac_ring::Overrun found by a fresh repository search.

**Interface:** `flac_ring::ReadError { GenerationChanged, Overwritten, InvalidOffset }`; `FlacRing::read(&self, cursor: &mut Cursor, out: &mut [u8]) -> Result<usize, ReadError>`. Delete unit Overrun rather than retain a second API.

- [ ] Change/add tests first: partial overwritten frame yields Overwritten, reset yields GenerationChanged before and after new publication, foreign cursor with excessive offset yields InvalidOffset. Assert cursor and sentinel output unchanged for each error, including zero output length. Add a reset-plus-capacity-loss case proving generation precedence; preserve success, empty and exact-capacity behavior.
- [ ] Run `cargo test -p esp-shazam-core --lib flac_ring` and observe the missing typed API failures before implementation.
- [ ] Replace each existing error return without moving any checks or changing success behavior. Update only encoded error imports/assertions. Split the property model's combined mismatch/lag condition into generation-first assertions so it independently checks the reason.
- [ ] Run focused core ring/wait tests and `cargo test -p esp-shazam-core --test stream_properties --test stream_limits`. Parent reviews this API migration before integrating it with the next task; do not leave a partially migrated build as a completion claim.

## Task 2: Preserve error identity through the actual listener

**Files:** `firmware/src/stream_pipeline.rs`, `firmware/src/listener_telemetry.rs`, `firmware/src/stream.rs`, `core/tests/replay_pipeline.rs`, `core/tests/stream_replay.rs`, `core/tests/listener_telemetry.rs`, `core/tests/support/replay.rs`, `core/tests/fixtures/stream-replay/reset.json`.

**Interfaces:** Append `End::EncodedReset`, `End::InvalidCursor` and matching EndCode variants. Add `End::from_encoded_read_error(flac_ring::ReadError) -> End`: GenerationChanged → EncodedReset; Overwritten → FellBehind; InvalidOffset → InvalidCursor. Display text: `encoded ring generation changed` and `encoded ring cursor invalid`. Existing Retired text stays `format epoch retired`.

- [ ] Write failing tests against real rings/listeners: reset while waiting ends EncodedReset; actual overwrite ends FellBehind; invalid-offset mapping ends InvalidCursor; real runtime switch while fresh-waiting ends Retired; empty expiry remains CaptureStalled. Exercise error completion before/at/after expiry with controlled clocks and assert identical read_wait accounting. Use actual foreign cursor ring tests for InvalidOffset; do not add a test-only cursor injection API.
- [ ] Update the existing raw invalidation replay to expect EncodedReset from the old listener, preserving rejected-copy counts, fresh listener behavior and source ledger assertions. Change only reset fixture terminal identity; keep events, actor ranges, timing, cleanup and overrun fixture unchanged. Add an explicit encoded_reset replay invariant only if required by the existing fixture's expectations.
- [ ] Observe focused tests fail, then keep Phase::Read's result as `Result<usize, End>`: retirement produces Err(End::Retired), actual read uses map_err(End::from_encoded_read_error), and WaitStep::Failed(end) performs the existing read_wait update before ending with end. Do not early-return from inside the ring lock or add retirement/clock reads.
- [ ] Extend fixed EndCode conversion and Display. Test each new conversion using actual End values and queue drain; verify the existing production cleanup seam still closes once before release. Keep frame-invalid and socket-invalid diagnostics distinct.
- [ ] Run `cargo test -p esp-shazam-core --test replay_pipeline --test stream_replay --test listener_telemetry`. Reuse the existing actual replay/writer integration; no mock server or second orchestration implementation.

## Task 3: Report consumers and final review

**Files:** `scripts/retention-check`, actual harness module/test files if needed for a narrow importable reason classifier, `scripts/test_harness_modules.py`. Avoid extracting unrelated retention orchestration.

- [ ] Add failing tests for production reason classification: old FellBehind → ring_overrun, new generation text → encoded_reset, invalid cursor → invalid_cursor, retirement → format_retired, existing timeout/freshness/socket reasons retain their categories. Cover actual logger prefix and trailing event/stack fields. If retention-check has no import seam, place only its reason classifier in the existing importable harness module and call it from the original script; tests must exercise that production function.
- [ ] Implement those reason classifications; preserve admission/departure/close timestamps, counts, fault suspension checks, strict telemetry-loss rejection and replay incomplete-tail validation. Run the actual corresponding Python module tests via the repository's uv workflow.
- [ ] Re-search every `flac_ring::Overrun`, old imported unit error, End match, reason string and reset/FellBehind replay expectation. Confirm no duplicate generation state, compatibility API or stable enum edits. Independent parent review covers full diff and exact behavior preservation.
- [ ] Parent runs `scripts/check`, firmware target builds and real device trial under sole hardware ownership. Existing full checks cover lint/type/unit/integration and real replay decoder checks; do not substitute syntax checks. Hardware must verify emitted generation-change identity after a real reset and compare unchanged termination/close/release behavior; failure to trigger a reset leaves device acceptance open.
- [ ] Parent records evidence and limitations, updates ce69, then commits with `fix: distinguish encoded listener end reasons`. This change improves attribution; it does not fix capture loss or slow delivery.

## Self-review

The plan covers every current encoded-ring error and the known retirement race, with actual capacity lag and WAV unaffected. All reason names agree across ring, listener, telemetry and replay. No check is moved across the wait boundary. Invalid cursor is proven at the actual ring plus the production mapping, without exposing private cursor state. Device acceptance remains the parent's responsibility. The missing machine-local `~/.codex/docs/local.md` was reported; no machine-specific build commands were invented.
