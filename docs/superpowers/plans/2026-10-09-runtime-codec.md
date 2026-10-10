# Runtime codec implementation plan

> Use subagent-driven-development with a clean-context Sol implementer. Parent
> owns integration, independent review, hardware, evidence and commits. Continue
> through the authorized work; do not ask again merely to start a later task.

**Goal:** One firmware offers persisted MP3/FLAC selection and existing buffers.
**Architecture:** One worker, a runtime codec profile and immutable active epochs;
selected settings apply when idle or on next Play, with transactional allocation.
**Tech stack:** Rust, pinned Shine/ESP-IDF, existing HTML/JS, FFmpeg and harnesses.
**Spec:** ../specs/2026-10-09-runtime-codec-design.md

## Now

Base a45d881 on wip/kata-capture-protection. Compaction count 2 for this phase.
User asked for both codecs configurable with the buffer retained. Optional timing
question offered next Stop→Play versus board reboot; Doctor Biz explicitly chose
next Stop→Play. Doctor Biz also requested a Settings page, now in the spec. This is authorized feature
work; additional skill permission loops would repeat the same authorization.
Preserve .kata.toml and docs-ignore. Unified hardware validation passed; leave
MP3 selected, stopped/autoplay off after the checks.

Doctor Biz reiterated that the office is empty for the weekend and authorized
continued testing. Finish runtime settings and board/browser verification, then
compare matched MP3/FLAC delivery runs. Keep the iteration firmware deployed.

## Global constraints

- One executing encoder, unchanged capture, bounded history and socket deadlines.
- Persist format and buffer separately; immutable current Sonos queue metadata.
- Startup buffer Low1s/Medium4s/High8s; WAV remains a diagnostic route.
- Prepare then commit a switch; allocation failure must leave prior state usable.
- MP3 refused when recognition enabled; no implicit feature disabling.
- No automatic firmware rollback, volume changes, push or merge.

## Review focus

Concurrent admission versus commit must not mix codecs. A stopped/canceled Play
must not publish/start a session later. Allocation failure/old blocked listeners
must not leak ring storage or replace active state partially. Idle switch/admission must arbitrate atomically; timed-out requests lose commit
authority. Invalid settings must not persist. Old selected-format omissions must use the single schema default.

### Task 1: Implement runtime selection and settings

**Files:** core/src/{stream_http,stream_buffer,sonos,sonos_web,sonos_queue}.rs;
core/tests/{buffer_presets,stream_codec,stream_codec_profile,stream_limits}.rs;
firmware/src/{lib,stream,stream_codec,stream_limits,sonos,web}.rs;
firmware/src/index.html; new settings HTML/shared UI script as needed;
test/sonos-web.test.cjs; Cargo manifests/locks;
scripts/{check,flash} and directly affected tests/docs.

- [x] Write and run failing tests for schema/action, profiles/routes, immutable
  queue codec, runtime adapter decode/reset and switch rollback/retired epochs.
- [x] Implement the spec using existing flow and smallest bounded ownership change.
  Expose one format/profile source of truth. Remove compile-time-only selection
  and its redundant test matrix; replace it with tests for both runtime profiles.
  Keep all prior meaningful coverage. No compatibility alias for --mp3 needed:
  one normal build contains both, docs explain selection via settings.
- [x] Run targeted tests and Clippy; report red/green commands, changed interfaces,
  uncertain behavior and allocation/memory tradeoffs. Parent owns commits.

### Task 2: Integrate and independently review

- [x] Review implementation against spec and command cancellation behavior.
- [x] Fix findings and run canonical scripts/check plus unified target build.
- [x] Keep unit DOM coverage labeled honestly; verify actual shipped UI/API on board.

Final canonical check passed all 20 stages; independent review closed three
correctness findings plus the resulting browser deadline mismatch. First hardware
run `target/runtime-codec/hardware/20261010T033710Z` failed High MP3 delivery.
Same-image run `target/runtime-codec/hardware/20261010T034411Z` passed all feature
gates, including real Chromium and occupied-slot retry. Both receipts are retained.

### Task 3: Hardware validation and completion

- [x] Run real source/decode switches both directions, repeated cycles, reboot
  persistence and buffer/queue checks; record capture and memory deltas.
- [x] Leave unified firmware with MP3 selected, stopped/autoplay off/Medium.
- [ ] Save sanitized evidence and known P1 delivery limits, update Kata, fresh-eyes
  review and local commit. Do not call runtime selection a throughput fix.

## Next reliability experiment

Matched MP3/FLAC/MP3 staged trials are running on the unchanged unified image:
`target/runtime-codec/matched/20261010T035157Z`. Each measures 30 seconds solo,
then a second receiver through 90 seconds. Preserve actual overlap and catch-up
separately; parent owns UART and front-jack noise source. No Sonos Play or reflash.
Same-image MP3 encode time varied across feature runs; allocation/cache placement
is a hypothesis, not an established cause. Record delivery results before choosing
the next measured change. Kata e8hd stays open for these failed reliability gates.
