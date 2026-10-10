# Host coverage pilot — 2026-10-10

The first Linux pilot passed 380 Rust tests with zero failures and two explicitly
ignored tests. It generated detailed JSON and inspectable HTML. This verifies
cargo-llvm-cov 0.9.1 with Rust/Cargo 1.93.0 and that Rust's bundled LLVM 21.1.8
on x86_64-unknown-linux-gnu. This is host execution evidence, not device coverage.

Command:

```bash
CARGO_BUILD_TARGET=xtensa-esp32-espidf scripts/coverage-host
```

Retained command logs show explicit `--target x86_64-unknown-linux-gnu` on the
test invocation and both report invocations; the inherited Xtensa target did
not control this run. Versions, revision, working-tree status/diff, commands and
full logs are under `target/coverage-host/run.bDQwZHEr`. JSON is at
`reports/coverage.json`; the HTML index is `reports/html/html/index.html`.
The short parent log is `target/kata-hardening/coverage-pilot.log`. No test
warnings or errors appeared. cargo-llvm-cov emitted its normal cfg(coverage)
notice, and reports completed successfully.

This first run occurred before the new wrapper/docs were committed. The retained
tracked diff/status does not capture contents of untracked files. Treat it as a
diagnostic; a clean committed-tree repeat is required for durable reproduction.
Fresh unique run directories prevent profile reuse; this first run alone does
not establish the required repeated-run check.

## Measured scope

The report has 35 source files: 22 core, one tools, one MP3 Rust wrapper and 11
host-compiled firmware source files. Firmware paths are capture_fixture/events,
capture_timing, net_policy, recognition_admission, recognition_feed, sonos,
stream_codec, stream_limits, stream_runtime, threads and web. Host integration
tests include those files by path; the firmware Cargo workspace remains excluded.

Observed line coverage is 7,320/7,719 (94.83%) and region coverage
12,753/13,493 (94.52%). These totals include compiled unit-test source and are
not a pure production-code score or acceptance threshold. Stable branch/MC/DC
coverage was not enabled. Selected file line counts:

| File | Covered / counted lines |
| --- | --- |
| core/src/capture_health.rs | 79 / 79 |
| core/src/stream_http.rs | 689 / 735 |
| core/src/stream_wait.rs | 366 / 378 |
| firmware/src/stream_runtime.rs (host compiled) | 229 / 234 |
| firmware/src/threads.rs (host compiled) | 10 / 10 |
| tools/src/main.rs | 28 / 101 |

Ignored tests were `decimate::tests::print_taps` (coefficient-table printer) and
`song_clip_matches_upstream_byte_for_byte` (requires the local song clip fixture;
scripts/e2e-host creates it). Default host features only: separate Loom model
runs and capture-fixture feature execution are excluded. C encoder FFI, ISR,
DMA/I2S, FreeRTOS, ESP-IDF ABI, target hardware/Sonos/TLS/NVS recovery and physical
audio continuity are outside scope. Python/JavaScript, doctests, benches and
examples do not contribute to this report.

## Uncovered behavior review

Capture restart/reboot policy and thread launch-versus-cleanup distinctions
executed in host tests. Real-loopback deadline, half-close, broken-pipe and
listener release tests also passed. Coverage does not establish assertion
strength or every interleaving.

Real gaps remain in stream_http's interrupted send/poll handling, poll failure
and descriptor flag query/change/restore errors. These need real supported fault
conditions; fake socket results would not establish target semantics. Some
readiness and latency counter paths depend on host scheduling. stream_wait's
buffered history request larger than ring capacity is not exercised through
buffered_cursor_before, even though direct ring capacity rejection has tests.
Tools CLI/WAV validation and external HTTP failure handling largely remain
unexecuted in this default test run. These are named future targets, not proof
of a defect. No wrong behavior was discovered, so no percentage-padding test
was added. The supported-host rejection scenario is a useful small future
regression candidate; an actual failing invariant should drive a code fix.

`scripts/test-coverage-host` runs in the canonical check. Root reported all 22
canonical stages passing; its earlier mise alias warning was retained and then
resolved by the canonical llvm-tools component spelling. Two subsequent mise
activations had empty stderr with unchanged Rust/LLVM versions. A separate CI
coverage job remains unimplemented pending design review; Kata m31k stays open.

## Clean committed-tree repeat

Ran `scripts/coverage-host` with the default environment at commit
`9b7f363bbc4855ce16d670b2609357db154156ea`. Tracked diff was empty before and after
execution; only local `.kata.toml` and `docs-ignore/` were untracked. The wrapper
retained an empty diff.log and the exact commit. Test/report process completed
successfully and exited; source remained frozen through execution.

The repeat used new directory `target/coverage-host/run.3fGHOtuq`, with its own
build/profile directory and 59 fresh profraw files. The first run has a separate
59-profile directory; its JSON SHA-256 remained unchanged. No prior run directory
was reused. Test and both report commands again explicitly selected the detected
host. Detailed JSON and HTML indexes exist in the new run, using the same relative
report paths as above. Short log: `target/kata-hardening/coverage-repeat.log`.

Again 380 passed, zero failed, two explicitly ignored, with no test warnings or
errors. The same 35 source file paths appear in both reports. Repeat line coverage
was 7,324/7,719 (94.88%) and regions 12,754/13,493 (94.52%). The four additional
covered lines are in core/src/stream_http.rs (693/735 versus 689/735); other file
line counts agree. Host socket execution can vary between runs. This small
variation is neither a new behavioral regression nor a reliability improvement.
The named uncovered failure semantics and all scope exclusions still apply.

This completes the local pinned-tool compatibility and isolated repeated-run
pilot. The standalone CI job remains a separate acceptance gate; m31k stays open.
