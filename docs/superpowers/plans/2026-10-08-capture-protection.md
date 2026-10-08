# Capture protection batch

## Now

Doctor Biz approved the next Kata set after the reliability batch. Base `8ba0450`, branch `wip/kata-capture-protection`. Scope: a0mf optional recognition work, dege I2S IRAM protection, and evidence-led ed4k restart investigation. Compaction count: 1. No push or merge requested.

## Constraints and success

Preserve audio format, gain, routing, speaker settings and credentials. Use the pinned Rust/ESP-IDF toolchains and canonical scripts/check. No live Shazam requests. Parent alone stages/commits, builds firmware, flashes and controls the board. Workers use Sol, disjoint files and real host regressions, then independent cross-review. Preserve .kata.toml and docs-ignore. Keep raw UART and private config out of Git.

Optional recognition state must own its FIR and feed buffers, with no off-profile resampling or new hot-path allocation/blocking. Enable the exact pinned I2S IRAM-safe option only after auditing driver/callback code, data and allocation placement. Verify effective build and linked image, not defaults alone. Preserve startup/restart discontinuity accounting; do not shorten settling without physical evidence. A measured limit is an investigation result, not a fixed ticket.

## Lanes

1. Recognition implementation (a0mf): owns audio_in.rs, a narrow recognition-processing helper and corresponding host tests. TDD enabled/off behavior, reset after gaps, buffer reuse. Coordinate capture timing instrumentation with parent; do not change restart policy.
2. IRAM implementation (dege): owns sdkconfig.defaults, i2s_capture.rs and narrowly needed diagnostic example/checks. Inspect pinned IDF source and linked-path requirements. Dedicated NVS test namespace only; never alter Sonos/Wi-Fi settings. Request shared-file ownership before touching lib.rs/audio_in.rs/scripts/check.
3. Parent investigation (ed4k): preserve baseline firmware; collect boot and restart evidence, classify known gaps versus uncertain DMA state, measure actual discarded frames and progress. Hardware serialized. No unproved reduction of the 500 ms codec-settling budget.

## Verification

Use paired baseline/current-build capture measurements with unchanged workload. Host tests and builds cover recognition off/on; do not flash the API-calling recognition profile. Check effective IRAM flag, callback/driver code and data placement, memory delta, and continuous decoding during real NVS writes with one/two listeners. Setup/TLS/reconnect findings remain distinct from flash safety. Restore final production, verify stopped/autoplay-off state without modifying speaker settings, run canonical checks and independent review, commit named paths and update Kata with exact evidence/remaining gates.

## Progress

- Repository and prior evidence read; all three Katas claimed.
- Initial source had unconditional FIR/feed work and lacked the I2S ISR IRAM option. Restart still discards 500 ms after every I2S reset; physical settling is unmeasured.
- Both Sol implementations and independent cross-reviews are complete with no actionable findings. Parent canonical checks pass all 17 stages, including 287 host Rust tests (two existing ignored), effective IRAM configuration and linked-symbol checks.
- Identically instrumented IRAM-off baseline and optional-FIR images are preserved under `target/capture-protection/`, with separate SHA-256 identities in the evidence.
- Implementation commits: `493c6c6` optional recognition and capture measurements; `1d751fe` IRAM protection, placement checks and isolated NVS diagnostic. Independent reviews and parent fresh-eyes found no issues. Final canonical 17 stages pass.
- Paired two-listener runtime fell from 16.6307% to 8.4046% of one core with identical telemetry. Both runs decoded exactly 2,160,000 frames/listener. Startup faults remain.
- One/two-listener NVS tests each passed 120 verified writes and cleanup/recovery, with exactly 4,320,000 frames/listener and no additional capture faults. Both configuration profiles compile; recognition-on never flashed, configuration restored exactly.
- Existing capture-fault fixture passed short-pause continuity, recovery, terminal-suspension reboot and exact decoded recovery. One recovery discarded another 24,000 frames. The 500 ms policy stays unchanged because this fixture cannot expose its discarded samples.
- Final production restored; two-listener 45-second exact-count check passed. Sonos remained stopped/autoplay off; settings match the initial snapshot. Evidence: `docs/audit-evidence/2026-10-08/capture-protection.md/.json`.
- Next: separate TLS/setup/reconnect acceptance for a0mf/dege under dp08; controlled physical pre-discard measurement for ed4k. Keep all three Katas open with measured progress rather than claiming those remaining gates passed. Batch implementation phase is complete; retire this session after evidence commit. No push/merge.
