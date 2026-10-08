# Kata reliability first batch

## Now

Doctor Biz approved three parallel Sol implementation lanes, then the Sonos worker-start retry. Base: `20cca99`. Branch: `wip/kata-reliability-first-batch`. No merge or push requested. Compaction count: 1.

Success: real omitted/duplicated/corrupt audio fails validation; the intended radio configuration is applied and read back; the three small defects and retryable worker launch are fixed; independent review and canonical checks pass. Hardware-only acceptance remains explicitly open unless actually observed.

## Global constraints

- Use Sol subagents. Fresh contexts were used initially; the runtime refused a third worker, so the two Sol threads handle subsequent tasks and review each other's changes. Parent owns canonical commits, integration, all hardware access and final verification.
- Work in the current checkout with disjoint file ownership. No worker stages, commits, flashes, opens UART, contacts Shazam/Sonos, changes network/audio settings, or launches subagents.
- Preserve user-owned `.kata.toml`, `docs-ignore/`, private media and credentials. Add only the requested `/media-ignore/` ignore rule.
- Follow TDD for behavior changes. Use real codec/file/socket behavior; no mocked end-to-end APIs or application mock modes. Keep narrow regression evidence and all failures.
- Preserve stable host Rust, pinned ESP-IDF/+esp, mise and uv conventions. New comment-capable source has two ABOUTME lines.
- Current stream formats remain 48 kHz stereo FLAC16 and WAV24. Do not change capture, gain, codec, session ownership or source/volume settings.
- `scripts/check` is the canonical integration check. Parent alone runs shared firmware builds and final checks; workers run focused host tests and supply commands/logs.
- No six-hour or 24-hour run in this batch. No automatic closure of a Kata whose hardware acceptance remains unverified.

## Task 1: Trustworthy decoder validation — 4dss, r0ve

Own scripts/stream-soak, scripts/fault-check, scripts/stream-check, scripts/stream_format.py, scripts/check-harnesses, and narrowly needed decoder/test helper files. Request parent integration of any added canonical check stage. Do not edit firmware or core implementation.

Read the full Kata bodies/comments. Reproduce the missing-block timestamp false pass and the corrupt-frame false pass using real generated 48 kHz/stereo/16-bit FLAC. Add failing tests first. Count actual decoded samples with bounded streaming accounting, preserving WAV24 precision, and keep timeline progress separate. Reject missing/duplicate frame numbers and CRC/decoder errors, retaining diagnostics. Test actual helpers/harness paths, not copied substitute logic. Include complete, omitted, duplicated and corrupted fixtures; bounded process cleanup/output. Keep the fix narrower than the deferred full Python harness refactor. Report red/green commands, self-review and limitations.

## Task 2: Reproducible Wi-Fi baseline — hv4x

Own firmware/src/net.rs and narrowly needed network-policy helper/host test files. Request coordination before editing firmware/src/lib.rs or a shared manifest. No device/network actions.

Verify exact pinned ESP-IDF APIs/lifecycle. Explicitly set intended b/g/n protocol and disabled power saving, read both back, and log effective policy plus association channel/RSSI/disconnect reason where exposed without credentials. Preserve reconnection behavior. Write meaningful host-testable policy/regression checks where possible without pretending mocks validate IDF. Parent builds firmware and owns device validation. Report precise remaining hardware gates; do not claim the transport root cause fixed.

## Task 3: Small fixes — n97a, 6sw1, ntja

Own firmware/examples/flac-cost.rs, .gitignore, scripts/e2e-host and narrowly needed fixture-transaction tests/helpers. Do not edit Task 1's shared harness files; request parent wiring for new test commands.

- n97a: correct stale constant-frame benchmark expectation from 16 to current FLAC16's 14, verified against the actual encoder and a focused regression. Parent builds the example.
- 6sw1: add /media-ignore/ exclusion, check actual Git ignore behavior and confirm private media is untracked without reading/copying it.
- ntja: retain all pre-existing fixture bytes on failed host E2E validation, including later failures; promote complete validated results on success. Use real file/subprocess regression behavior, no live Shazam calls or mocked API claims. Keep source and entry-point contracts unchanged.

## Task 4: Retry Sonos worker startup — j4ae

Starts after a lane is available. Own firmware/src/sonos.rs and its directly corresponding host tests. Retain launch resources if fallible thread setup/allocation fails, without duplicate workers or loss of NVS/receiver ownership. Write a failing retry regression against the actual lifecycle abstraction; preserve commands, session ownership and lazy-start behavior. A narrow threads.rs change is authorized if needed to distinguish spawn failure from configuration-reset failure after a live thread was created. Parent handles any hardware checks.

## Verification and review

Each implementer writes a report in this plan's SDD workspace with changed files, red/green commands, actual results and remaining limits. The two Sol workers cross-review each other's lanes; parent reviews the combined change. Parent runs scripts/check and explicitly builds flac-cost, reviews warnings, commits only named task files, and updates each Kata with evidence. Hardware acceptance must use actual device observations with settings preserved; unavailable access is reported, not fabricated.

## Progress

- Tasks 1–3 implemented. The first canonical run passed all 15 stages, including 273 Rust tests (two existing tests ignored), ten decoder-integrity tests and six fixture transaction tests. The FLAC cost example builds.
- Independent Sol reviews cleared tasks 2 and 3. Task 1 review found a decoder-exit race requiring final output draining and final validation before success; correction is in progress.
- Task 1 reproduced missing-frame, CRC, duplicate and valid-prefix truncation false passes. Its scope also covers `scripts/encoder-fault-check`.
- Task 4 follows the decoder-exit correction. A successful thread launch followed by configuration-reset failure must not cause a duplicate launch.
- Board HTTP is reachable, Sonos stopped and autoplay off. No firmware flash, playback command, or network change performed yet. Hardware acceptance remains open.
