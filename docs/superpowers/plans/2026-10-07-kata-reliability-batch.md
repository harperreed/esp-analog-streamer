# Kata reliability first batch

## Now

Implementation and review are complete for the approved first batch. Base: `20cca99`. Branch: `wip/kata-reliability-first-batch`. Commits: `ed5e26a` and `350e497`. No merge or push requested. Compaction count: 1. The broader Wi-Fi hardware acceptance remains open in hv4x.

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

- All four tasks implemented and independently reviewed. The final canonical check passed all 15 stages: 279 Rust tests, two existing ignored tests, 12 decoder-integrity tests and six fixture transaction tests. The final FLAC cost example builds without project warnings.
- Review found and fixed decoder-exit races and fractional-duration rounding. Exact sample counts remain mandatory. Negative controls cover omitted, duplicated, damaged, CRC-invalid and truncated-tail audio, with WAV24 precision preserved.
- Actual ESP32 benchmark completed zero/DC/mixed/noise assertions. Production was restored. A 30-second two-listener run decoded exactly 1,440,000 stereo frames per listener with no new capture faults after baseline.
- Startup already recorded 15 queue overflows, 480 dropped frames, two restarts and two discontinuities. Keep startup continuity separate from the measured steady interval; capture protection remains the next priority.
- The final Sonos image accepted and completed two real discovery requests; saved configuration stayed unchanged, playback stopped and autoplay off. Failed launch/retry uses real host thread tests; IDF memory-pressure failure was not injected.
- Closed with evidence: 4dss, r0ve, n97a, 6sw1, ntja, j4ae. hv4x remains open for diagnostic-mode persistence, forced reconnect and correlated sender/AP measurements. Parent wmsy remains open for later batches.
- Durable results: [reliability-first-batch.md](../../audit-evidence/2026-10-07/reliability-first-batch.md) and its machine-readable JSON. Private raw logs and binary hashes are under target/kata-hardware. Nothing pushed or merged.
