# Capture protection batch

## Now

Doctor Biz approved the next Kata set after the reliability batch. Base `8ba0450`, branch `wip/kata-capture-protection`. Scope: a0mf optional recognition work, dege I2S IRAM protection, and evidence-led ed4k restart investigation. Compaction count: 3. No push or merge requested.

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
- Continuing under Doctor Biz's correction: separate TLS/setup/reconnect acceptance for a0mf/dege under dp08, then controlled pre-discard measurement for ed4k. Commits mark checkpoints; they do not stop authorized execution. Keep gates open until measured. No push/merge.

## Active continuation

Run production capture/FLAC concurrently with repeated real TLS/WebSocket connections to a local receive-only fixture using the existing Sonos transport implementation and an explicitly trusted test certificate. This tests allocation, handshake and teardown/reconnect capture effects without sending speaker commands or changing production trust. One Sol lane owns the diagnostic example; another owns the real host fixture and its tests. Parent owns integration, builds, board and evidence. Baseline before TLS, exact decoded samples through three connection cycles, capture faults/generations, heap, success markers and cleanup are required. If capture fails, retain the failing run and diagnose before changing production code.

- Continuation checkpoint `558d774`: real TLS peer/production fixture, bounded post-failure UART recording and pre-discard settling diagnostic. Canonical 19 stages pass; independent reviews complete.
- TLS one/two-listener passes coexist with retained failure: three handshakes added 37 overflows, three restarts and 72,000 settling frames. Physical ALSA playback ran but ADC stayed near noise; do not claim music delivery. TLS capture failure is under investigation.
- Forced Wi-Fi disconnect/reconnect passed unchanged capture counters and exact 480,000 frames on each of two recovered listeners.
- First standalone settling acquisition completed ten valid trials, but its UART dump starved IDLE0 and watchdog text corrupted records. Retain that run; post-acquisition yields fixed the reporting failure in two complete repeat runs. The workstation is not yet connected to LINE IN.
- Sol completed the bounded task-state sampler and independent source/hardware review. Parent owns all builds/hardware; production has been restored.

- Corrected settling report passed two complete real-board runs: 2,000 ordered blocks/ten valid trials each, no watchdogs. Soft codec reset has initial zeros and a retained transient through about 420 ms; I2S-only has immediate quiet samples. Doctor Biz confirmed the workstation is not yet connected to LINE IN; hold signal-dependent policy work until connected.
- Task sampling reproduced a DMA-overflow failure and a distinct staging failure. The mapped repeat kept DMA overflow/restart counters flat but lost 4,320 frames while the raw-ring read held its mutex for 110,567 us; publication pending for 112,352 us with a full 2,160-frame staging buffer. Actual capture priority 10 verified. Full evidence in capture-setup.md/.json; never infer ISR activity or a specific lock owner from sampled task identity alone.
- Canonical 19 stages pass after the final sampler changes; independent source and hardware reviews complete. Parent restored saved production image `3dc70b4a`; the final 45-second two-listener run passed exactly 2,160,000 frames per listener with no added tracked capture faults. Configuration matches the initial snapshot; Sonos remains stopped/autoplay off. Sol is preparing the minimal dp08 publication design, preserving WAV/FLAC; architecture discussion precedes implementation.
