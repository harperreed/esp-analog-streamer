# Kata audit continuation

Doctor Biz resumed the audit with `gpt-6.1-sol` subagents on 2026-10-04.
The approved architecture and acceptance criteria remain in the 2026-10-03 plan.

## State

- `r759`: constant subframes implemented in `fed9e42`. Claxon and ffmpeg decode tests, independent spec and quality reviews, and `scripts/check` passed. Exact constant stereo frames shrink from 1,037 to 16 bytes. Board timing remains unverified; the issue stays open with `needs-review`.
- `f8rn`: one core-1 encoder now feeds the shared ring; listeners copy whole frames under one write deadline. Raw overrun invalidates old cursors and resets the encoder without allocation. Off: 2 s raw + 94 encoded slots (~8 s); on: 0.5 s raw + 24 encoded slots (~2 s). Claxon and ffmpeg decode mid-stream joins bit-exact. Spec and quality reviews and the target build passed; device measurements remain pending.
- `6yb1`: optional recognition buffering now skips 416,162 bytes of sample storage when Shazam is off. Three real-lock tests cover gating, snapshot contention, and bounded waiting. Spec and quality reviews and `scripts/check` passed. Board allocation measurements remain pending; Shazam-on device tests remain excluded by R26.
- `3pz5`: core restart-then-reboot policy has eight passing tests and both reviews. Firmware supervision remains pending. Doctor Biz approved a small local ESP-IDF I2S wrapper for capture counters; its source awaits target wiring and validation.
- `kjfx`: the prior socket-timeout approach can exceed its deadline inside one macOS send. A real burst-reader regression reproduces it. The replacement uses nonblocking sends plus a deadline-bound readiness wait; committed in `96b66f6` after spec/quality approval and all 30 focused socket tests. The canonical target build passed. Device slow-reader acceptance remains pending.
- `hb9p`, `sznz`, `b69f`, `3db1`, `vya4`: current production contains no diagnostic module or routes. The affected code remains in archived `target/task15b/` and `target/task16/` assets. Issues have source-audit comments and remain open; fix them before reusing those diagnostics.

## Ownership

Another session changed tracked firmware during this run and committed `ee68892`
and `cf42331`, then left a handoff in `8c09ab4`. Doctor Biz confirmed that session
was stopped. This audit continuation now owns the checkout and board.

The board answered `/api/state` at `192.168.200.224` with Shazam disabled.
`192.168.200.8` is the MQTT broker. This run flashed clean shared-encoder firmware `96b66f6` at 115200 baud.
The replaced image was diagnostic image 3, with unsafe temporary diagnostic code. Run D recorded one persistent 240-frame capture
deficit, so zero read errors or successful decoding cannot establish continuity.

## Next

1. Finish two-listener playback and record clean-image evidence.
2. Wire capture counters, bounded capture work, independent supervision, and listener timeouts.
3. Keep board-dependent issues open until their actual acceptance evidence exists.

Final verification for this core increment: `scripts/check` passed formatting,
clippy, 170 tests, firmware formatting, firmware build, and firmware warning checks.
Two existing tests remain ignored: coefficient-table printing and the separately
invoked real-song fingerprint fixture. A 60-second real FLAC stream passed length, level, channel, and zero-run checks; all 61 state polls answered. Two-listener checks are running. This does not establish capture continuity.

For `53gv`, HAL 0.47.0 exposes neither the RX handle nor a distinct queue-overflow
callback. The approved local IDF wrapper preserves pin, clock, and DMA settings;
do not reinterpret private driver layout. For `zqnh`,
finite positive socket linger requires enabling `CONFIG_LWIP_SO_LINGER`, which
the checked generated configuration disables. Both need device validation.

## Session checkpoint

Compaction count: 1. Clean image one-listener results: `target/audit-shared-one/summary.txt`.
The capture and freshness agents own their current edits; PSRAM buffer source is
not yet wired or verified. User-owned `.gitignore` and `.kata.toml` remain untouched.

## Capture and memory checkpoint

`f67aacf` adds the approved local RX wrapper, separate overflow counters, bounded
45 ms raw staging, nonblocking store publication, independent 3 s / 3 s recovery,
2 s fresh-audio waits, and strict PSRAM listener-frame admission. Canonical checks
passed 184 tests plus target build and warning checks. Independent reviews passed.
ELF inspection found both callbacks/literals in IRAM and counters in internal DRAM;
atomic increments are inline, without helper calls. Whole-IDF ISR cache safety is
not established (`CONFIG_I2S_ISR_IRAM_SAFE` remains off).

Actual board startup recorded 12 queue overflows and one recovery restart. Those
counts stayed flat throughout a 60 s passing FLAC check and three passing stalled
reconnect rounds. The restart flushed uncertain queued data and reported a gap.
No read errors occurred. These tests do not replace forced blocked-capture and
reboot tests, or worst-case input testing. Shazam stayed disabled.

Telemetry in `target/audit-logs/capture-serial.log`: idle PSRAM free 973,956 B,
largest 966,656; six retained stalled peers reduced sampled free PSRAM to 647,032 B
and largest to 638,976. Sampled internal free ranged 64,363–123,423 B; largest
61,440–77,824. These are samples, not allocator minimum-water marks. Maximum
observed raw publication work was 2,600 us and store work 1,336 us. Frame/timer
deficit is labeled an estimate and includes boot loss and scheduling uncertainty.

`57a59e7` adds task-self runtime statistics and `scripts/stream-churn`; canonical
checks and independent review passed. CPU-instrumented firmware is being flashed.
A separate close helper is untracked and unwired: Doctor Biz's decision on a
terminal close-error recovery policy is pending. Positive linger only bounds queued
payload; control PCB retention remains a separate measurement requirement.

Board evidence: `target/audit-capture-one/summary.txt` and
`target/audit-capture-churn/20261005T030101.350958Z/summary.json`.
