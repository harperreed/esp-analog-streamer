# Kata audit continuation

Doctor Biz resumed the audit with `gpt-6.1-sol` subagents on 2026-10-04.
The approved architecture and acceptance criteria remain in the 2026-10-03 plan.

## Initial continuation state (historical)

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

## Initial next steps (historical)

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

Compaction count: 3. Clean image one-listener results: `target/audit-shared-one/summary.txt`.
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

## Overnight continuation

Doctor Biz authorized continued work while sleeping and asked for independent
judgment. He approved reboot on an unrecoverable TCP close failure. Shazam remains
disabled under R26. Physical source correction: the player is streaming FIP.

- `r759` is closed: `021d411`'s real ESP32 benchmark passed all assertions. Zero/DC
  frames are 16 B, ~6.44 ms encoding elapsed and ~6.47 ms task CPU each over 128
  blocks. Mixed frames are 12,301 B, noise 24,586 B. This measures encoder cost;
  it does not send synthetic noise through the actual capture/network path.
- Shared encoder stable-window medians on `57a59e7`: 18.00% of one core idle,
  18.76% with one FLAC listener, 19.17% with two. Output stays ~11.72 blocks/s,
  resets zero. One/two-listener FIP checks all passed. Worst-case source and
  Shazam-on evidence remain outstanding.
- `01cf98d` requires PSRAM and enables static TX16/cache32 while preserving
  RX10/dynamicRX32/BA6/OOSEQ4. Generated config, source review, and canonical
  checks pass. It adds rolling heap minimum sums; board candidate tests remain.
- `05bde2a` enables two-second positive linger and keeps the listener slot until
  checked close, including allocation/spawn failures. Configuration failure gets
  503; unrecoverable close failure reboots as approved. Full checks and reviews
  pass. FIN/TIME_WAIT control PCBs remain distinct from queued payload retention.
- `aa94814` passed real PSRAM exhaustion/fragmentation assertions. Exhaustion
  left 4 B; fragmentation left 2,081,280 B total with a 16,384 B largest block.
  Both rejected a 24,591 B frame without consuming internal RAM (270,195 B
  before/after). Releasing owners restored exactly 4,192,024 B. Evidence:
  `target/audit-logs/psram-check-serial.log`. This alone does not prove HTTP refusal.
- `3025277` fixes a held-mutex hole in fresh-audio deadlines: cursor acquisition
  and reads now include contention in their deadline. Three real-mutex regressions
  failed first, then passed; independent review passed. WAV and FLAC frame timeout
  logs now report actual elapsed/limit milliseconds without per-frame allocations.
- `d2dd0d3` provides real capture-suspension and live HTTP/PSRAM-pressure examples
  through the production startup library. No production injection mode or route.
  `9022278` adds WAV and trickling-reader coverage to the real reconnect harness.
  Canonical checks pass 187 tests (two pre-existing ignored); all examples build.

Current board: listener-pressure fixture, successfully flashed at 115200 baud.
Next: reviewed host assertions for real HTTP refusal/recovery, then capture faults,
then restore production and run WAV/FLAC trickle/churn and sustained playback.
Restore production after fixtures; do not leave a fault image running. Board
examples run no Shazam calls. User-owned `.gitignore` and `.kata.toml` remain
unmodified by this audit. Physical noise-input and Shazam-on acceptance remain
unavailable under the current source and R26; do not claim them from FIP playback.

## Archived diagnostic disposition

Retire `hb9p`, `sznz`, `b69f`, `3db1`, and `vya4` as wontfix for obsolete temporary
diagnostics. Independent source review confirmed no active module or route uses
that code. This changes the earlier keep-open policy; it does not repair the
archives or validate their old measurements. No production feature is removed.
Any future reuse needs fresh repair and verification. Production telemetry and
separate hardware examples cover current checks, but linkoutput error counts,
raw throughput blast, and controlled encoder working-buffer placement comparisons
remain absent. `23mz` stays open for the unknown transmit-resource failure cause.

## Live fault evidence

`8efd61f` pressure run passed: two real allocation refusals returned 503 with
matching UART reasons. During fragmentation, free PSRAM was 458,868 B but the
largest block was 16,384 B, below the 24,591 B request. Capture fault counters
stayed flat through allocation, hold, release, and two recovered 10 s decodes.
Internal window minimum reached 76,195 B; final internal free was 90,847 B and
PSRAM free 918,836 B. Evidence:
`target/audit-pressure/20261005T034646.847875Z/summary.json`.
This does not verify failure under internal-RAM exhaustion: Rust allocates thread
metadata before the fallible pthread call, and Wi-Fi shares that heap. No safe
reserve threshold was inferred from these measurements.

`5032a49` capture-FLAC run passed: the short pause added no reported loss;
caught-up client EOF arrived 1.769 s after suspension. The real supervisor
requested restart; capture restarted and six seconds decoded. A permanent
suspension produced the ordered restart request, reboot, ESP_RST_SW boot, and
two eight-second decoded streams with unchanged post-boot fault counters.
Intentional long suspension increased queue overflows 12 to 704 and restart/gap
counters 2 to 3, demonstrating reported loss rather than silent stitching.
Evidence: `target/audit-capture-fault/20261005T035333.169410Z/summary.json`.

Keep the failed runs: `target/audit-capture-fault/20261005T035018.801715Z/`
used a host read timeout of four seconds, below the firmware's 8.02 s FLAC
allowance; both post-reboot decoders timed out at 7.424 s. The corrected timeout
is ten seconds, matching stream-check; the routed stall's cause remains unknown.
The two-WAV run `target/audit-capture-fault-wav/20261005T035510.089672Z/`
hit a 2,010 ms frame deadline and a generation overrun, so it did not establish
the fresh-audio timeout. A single-WAV run
`target/audit-capture-fault-wav-one/20261005T035925.930645Z/` also hit a 2,011 ms
frame deadline before the fault. Both stopped cleanly, but routed WAV playback
is not reliable under these conditions. Test WAV freshness with real board
loopback sockets to separate that contract from transport capacity. Current
input has become quiet (~-90 dBFS); do not label it sustained music or noise.

Current work: real Wi-Fi disconnect/reconnect fixture and WAV loopback freshness
fixture, then restore production for churn and soak. `c7b709a` provides bounded
two-decoder soak checks with private logs and a checked 128 MiB output budget.
No long-soak result exists yet.

`ce5db1c` real Wi-Fi disconnect/reconnect passed: two original FLAC streams
advanced before injection, production reconnected, and two new 10 s decodes plus
third-client 503 passed. Capture fault counters did not change. The test injects
one disconnect; it does not establish behavior under every loss/latency pattern.

`df60f15` real board-loopback WAV test passed. Both EOFs arrived about 2.03 s
after suspension with production fresh-audio timeout reasons. Restart/gap counters
increased 1 to 2, both WAV slots admitted new peers with valid headers/data, then
two host FLAC decodes passed. WAV byte progress is not waveform decoding. `5es1`
is closed with this evidence and the real-lock/socket host tests.

Sanitized structured results, including all three failed runs, are versioned in
`docs/audit-evidence/2026-10-04/`; raw UART remains local under the paths recorded
there. The export commands are throwaway; these files are the lasting evidence.
`f3de476` safely samples actual TCP lists through the lwIP task, using generated
pinned private layouts. Canonical checks and target build pass. Idle board smoke
reported valid samples, no invalid observations, and port 8000 LISTEN only.
Churn/reclamation measurements and the production soak remain next.

## Capture-loss investigation checkpoint

`627d1ed` retention run failed after 42 s: software drops rose 240 to 2,400
and gaps 1 to 2; queue overflows stayed 11. The encoder reset and disconnected
both listeners. Keep the failed result in `failed-retention-capture-loss.json`.
The 2,160-frame staging capacity does not prove a single 45 ms lock hold.

Confirmed core bug: full staging was discarded before trying an available ring
mutex. A real-mutex regression failed first; the ordering fix preserves staged
then incoming frames without growing capacity. Independent review is pending.
Pinned logger source also shows that synchronous supervisor logs can block on
Rust filter, FILE, and UART locks; UART writes busy-wait FIFO space. Measure raw
read and staging delays before changing task scheduling. Health decisions must
not depend on synchronous logging. `esp_restart` shutdown handlers require a
separate bound audit. Do not increase buffers or weaken capture-loss assertions.

Board remains `tcp-retention`; restore production after fixtures. No six-hour
soak has started. Shazam remains off; physical input is currently quiet.
