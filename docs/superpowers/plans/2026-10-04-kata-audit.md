# Kata audit continuation

## Current state

Compaction count: 4. Fresh validation controller `/root/overnight_verify` owns the
board, UART, flashes and evidence; root owns ticket integration. No board process
is running. Board is parked in its bootloader after the first pressure flash;
restore production after the remaining fixtures. Shazam remains off under R26. Physical
input is quiet (roughly -84 to -90 dBFS), so no worst-case input claim is valid.

- Isolated capture health policy passed after the complete-line UART reader fix.
- One FLAC and one WAV trickle admission round passed; TCP states reclaimed.
- The requested six-hour production soak failed after 33.156 seconds. A separate
  raw diagnosis confirmed terminal-frame truncation with valid complete frames;
  no six-hour acceptance or transport root cause is established.
- PCB send-state/direct-drain diagnosis found nonzero windows and ACK stalls.
  The hook-free internal-pressure fixture/harness is under review; a safe
  linkoutput observer may follow. Flash only reviewed builds and restore
  production after fixtures. No merges or pushes are authorized.
- User-owned `.gitignore` and `.kata.toml` stay untouched. All failed evidence
  remains in `docs/audit-evidence/2026-10-04/`, with raw files private in `target/`.

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

`7cb5ae6` fixes staging order and adds narrowly scoped raw-read/staging timings.
Canonical checks pass 188 tests (two pre-existing ignored), build and warnings.
Two instrumented retention attempts stopped around 30 s on web-status timeouts.
A separate 110 s UART observation kept capture counters flat through a churn
attempt, but churn failed on round-two replacement admission timeout. Measured
read closure maximum was 14,046 us; this excludes acquisition/unlock tails.
These runs do not establish the prior loss mechanism or retention acceptance.
Their structured results are versioned; raw UART remains private.

`31e28cf` moves heap/UART telemetry to priority3 metrics and leaves priority11
health polling free of logs and heap work. It adds a 4 KiB stack plus metadata.
Restart-request counts are atomic and logged asynchronously. The capture fault
harness now requires actual SW_CPU_RESET within the tested eight-second bound,
then ESP_RST_SW3 and recovered playback, rather than a blocking reboot log.
Independent review and canonical checks pass; board revalidation is next.
Source audit found Wi-Fi shutdown and untimed timekeeping locks in esp_restart;
no unconditional reset bound is claimed. Metrics can still inherit a logger
waiter's priority; its base priority is not a universal scheduling guarantee.

HTTP source audit found no direct raw-ring dependency in /api/state: store
serialization unlocks before socket writes. The HTTP server is single-threaded
with five-second socket timeouts. Timeout location remains unproven; phase-timed
HTTP probes are being added rather than blaming the route or weakening tests.

`kjfx` closed after an independent acceptance audit. Its real partial-send host
regression and board WAV/FLAC deadline plus checked-close evidence satisfy the
original ticket. WAV writes took 2,010/2,011 ms and closes 2,393/697 ms; FLAC writes
took 8,029/8,027 ms and closes 2,153/2,013 ms. Failed overall runs remain failed.
This does not close TCP control-state reclamation or routed reliability (`zqnh`).

Current transport evidence: ten idle HTTP probes passed a two-second absolute
deadline (maximum 485 ms). During a passing one-round stopped-reader churn, two
of thirty probes failed the same deadline in the response-header phase, after
zero and 92 bytes. The stream probe retains its original three-second admission
deadline; firmware's FLAC write budget is 8.021 s. No timeout thresholds changed.
Packet capture is unavailable: BPF access is denied and noninteractive sudo
requires a password. No administrator access was requested while Doctor Biz slept.

Updated capture-fault verification has not passed: two-client input lost one
listener to ring backlog before the injected fault despite flat capture counters;
one-client input did not reach two decoded seconds within its six-second check
(termination flushed 2.304 s). Do not call the second failure zero audio.
`47114f6` provides a reviewed, target-built real encoder-stall fixture; board test
still pending. `cbbd69e` adds port80 state and public Wi-Fi signal/channel samples
to the separate TCP observer. Production remains unchanged by these diagnostics.

## Fresh validation handoff

`cbbd69e` TCP observer ran 270 s. The child churn stopped on an API poll timeout
after one of three rounds; UART continued. Capture counters remained flat.
FLAC frame timeouts were 8,029/8,025 ms; slow socket closes 2,207/2,123 ms. Both
ports returned to LISTEN-only by log time 193.131 s and stayed clear. Baseline to
final: PSRAM free 919,492 to 919,516 B, largest 917,504 to 851,968 B; internal
free 82,211 to 80,871 B, largest 77,824 to 73,728 B. AP samples were -56 to
-52 dBm on channel1. These samples do not identify the transport root cause.
Structured evidence is `partial-churn-reclamation.json`; no complete churn
acceptance or memory reserve is claimed.

`159319c` adds an isolated UART-only health mode to fault-check and defers
transient state-poll failures in stream-churn until its final verdict. Any poll
error still makes the run FAIL; bad/malformed Shazam configuration stops promptly.
Admissions retain the same deadlines. Independent review and syntax checks pass.

After this long session checkpoint, fresh Sol6.1 controller `/root/overnight_verify`
owns board/serial/flash and validation. Encoder-faults has been flashed and its
real harness is running. Next: capture-faults with `--mode health`; remaining
slow-client/reclamation evidence if feasible; restore production; canonical
checks; sustained two-listener soak. Root retains issue integration/final reporting.
No other agent may flash or open UART. Shazam stays off; current source is quiet.
The six-hour production soak has NOT begun. User-owned .gitignore/.kata.toml
remain untouched. Nine tickets remain open; do not claim them done without their
original acceptance evidence. Physical broadband and Shazam-on checks remain
unavailable under current constraints.

## Controller hardware checkpoint

The encoder-fault run `20261005T051208.429099Z` failed its recent pre-suspension
body-progress assertion. Independently, one draining peer reached an 8,025 ms
frame timeout against its 8,021 ms budget; the other reported freshness closure.
Capture counters remained 13 overflows, 480 software-dropped frames, zero read
errors, two restarts and two gaps through the sampled interval. This does not
establish encoder-stall acceptance. Preserve `failed-encoder-routed-progress.json`.

The isolated health run `20261005T051548.085310Z` recorded recovery restart,
resumed capture and actual SW_CPU_RESET 6.177 s after the final marker, then
ESP_RST_SW (3). Overall FAIL remains: the harness counted a partially written
UART line as a second post-reset sample and failed on its absent read_errors
field. `failed-health-partial-uart.json` preserves this failure. A complete-line
reader regression and fix are under independent review before fresh rerun.
The board currently runs capture-faults; production restoration remains required.

`9eeac3d` fixes both fault harnesses to observe only LF-terminated UART byte lines;
real-file red/green regressions and independent review passed. Fresh isolated
health run `20261005T052015.218828Z` PASS: actual software reset occurred 6.367 s
after final suspension marker, ESP_RST_SW (3), and complete post-reset capture
samples advanced with flat faults. `capture-health.json` preserves the evidence.
This verifies capture-only health policy; Shazam-on false-reset acceptance remains
excluded by R26 and no stream decoding/freshness is claimed from this mode.

One FLAC and one WAV trickle round passed strict existing churn assertions with
30 s holds and one byte drained every 0.5 s. A 310.43 s UART observer included
208.08 s after final child cleanup. Capture faults stayed flat across 30 complete
samples; 264 TCP reports had no invalid observations. Both ports stayed LISTEN
only from log time 214.921 s. Maximum close was 2,474 ms. Original FLAC peers hit
8,029/8,030 ms deadlines; original WAV peers instead fell a generation behind,
so no original-WAV deadline or decoded-playback claim follows from child PASS.
PSRAM free returned 919,500 to 919,520 B and largest block to 917,504 B; internal
free fell 82,319 to 80,503 B. This short collection does not prove a safe internal
reserve or long-term stability. Evidence: `trickle-reclamation.json`.
Production restoration is now running; the six-hour soak has not started yet.

Production restore completed at 05:31:10 UTC. Fresh canonical `scripts/check`
passed all stages, 188 tests with two pre-existing ignored. Six-hour two-listener
soak attempted at `20261005T053150.884877Z` but FAILED after 33.156 s. Listener 2
reported HTTP premature EOF/demux I/O followed by invalid residual after decoding
1.024 s; listener 1 decoded 2.816 s before harness cleanup and its stderr is empty. UART reported an 8,035 ms frame timeout against
8,021 ms; sampled capture deltas stayed zero. Raw received FLAC bytes were not
retained by the null-output soak, so malformed encoding versus truncated delivery
remains unresolved. Preserve `failed-production-soak-decode.json`; do not retry
blindly or claim long endurance. Board runs production with Shazam disabled.

A bounded raw-byte tee diagnosis reproduced invalid residual with a truncated
terminal frame. Peer 0 received complete frames 191–193 (all CRC16-valid) plus
1,980 bytes of frame 194; its complete prefix decodes cleanly. Peer 1 received
complete frames 192–198, all valid and cleanly decoded. Shared complete frames
192/193 match byte-for-byte; the partial frame 194 matches the other peer's full
frame prefix. Headers passed CRC8 and frame numbers were consecutive. HTTP
framing was valid 200/Connection-close. UART saw an 8,029 ms write timeout and a
separate ring-overrun closure, with flat capture faults; collector-to-socket
mapping was not recorded. This sample finds transport truncation, not a malformed
complete encoder frame; it does not make the failed soak pass or prove its exact
mechanism. Evidence: `partial-flac-terminal-frame.json`; raw private bodies under
`target/audit-flac-tee/20261005T053523.231346Z/`. Board remains production.

`b70afc4` PCB send-state observer with direct raw-file draining recorded 68 send
rows and 64 valid reports. All sampled windows were 65,535 bytes. One connection
showed an ACK plateau at log times 26.521/27.701/28.901 s, retransmission count 1,
congestion window 1,440 B and queued unsent/unacked data; ACKs later advanced and
it fell behind at 18.8 s since admission. The second ran to its 60 s host bound.
This excludes observed zero-window backpressure and live decoder-pipe blocking
in this run, without locating the network failure. Eight capture samples stayed
flat. Naturally ended peer: 109 complete frames, clean decode. Host-bounded peer:
703 valid complete frames, clean prefix decode, host-cut terminal tail. All 109
shared frames match. `direct-drain-send-state.json` preserves structured evidence.

Internal-pressure `783be6c` was flashed but immediately parked in the bootloader
before running its harness, after root identified late failed-callback publication
as unsafe to assume. No pressure acceptance is claimed. Its replacement removes
the hook and uses real strict allocation probes. Board is currently parked;
callback-free fixture review/build precedes the next flash, then restore production.

Hook-free internal-pressure `21a3355` target built/reviewed, but its hardware run
`20261005T055912.280009Z` FAILED before READY: it could not form stack-sized
fragmentation above the unchanged 32,768 B experimental floor. START geometry
was free 80,283 B / largest 77,824 B; final failed geometry was not logged.
No admission failure or decoded recovery was tested. Preserve
`failed-internal-pressure-geometry.json`; no threshold relaxation or blind retry.
State reported -120 dBFS, so source is now effectively zero. Output-observer
fixture flash is underway; restore production and final canonical check follow.
