# Kata audit continuation

## Current state

Ethernet follow-up, 2026-10-07: cable moved to DHCP-configured `enp5s0` (the other
port is link-local only). Route to board verified through 192.168.23.153 before
and after test. Same two-listener/music test still failed at 49.424 s on a
three-second status request timeout; decoded 17.493/16.725 s. Cleanup stopped
the streams; do not label the UART peer reset a natural streaming failure.
Sampled post-baseline capture counters remained flat. Firmware unchanged,
playback stopped, status API responsive afterward. Workstation Wi-Fi is not
required for the failure; AP/ESP boundary observation remains the next useful
discriminator. See `docs/audit-evidence/2026-10-07/ethernet-comparison.json`.

2026-10-07 Linux continuation, compaction count 8: workstation setup and physical
analog validation are committed on `wip/linux-audio-validation`. Board IP is
192.168.23.217, UART `/dev/ttyUSB0`. Private credentials and test media stay local.
One listener decoded 120 seconds successfully; two listeners failed twice.
The latest repeat dropped listener 2 after 27.9 seconds for ring lag, with sampled
post-baseline capture fault counters unchanged. Packet capture is now available
at `target/packet-capture/run-gPRKBEPN/stream.pcap`; analyze delivery and ACK timing
before any more transport tuning. Capture, playback and soak processes stopped.

Packet analysis now confirms ACK-supported sequence holes plus fully ACKed
no-arrival intervals of 1.132/0.955 seconds. Valid receive windows remained open;
local ACK generation was fast, but ACK delivery to ESP is unknown. This does not
locate ESP/driver, RF, AP or client-side delay. Initial capture did not preserve
kernel-drop counters; the capture script now does. Curated results are in
`docs/audit-evidence/2026-10-07/linux-stream-packet-timing.json`. Next discriminator
is simultaneous AP boundary capture; a wired workstation comparison is a simpler
first isolation step if physical Ethernet is available. No more speculative
buffer or retry tweaks.

Selective +12 dB left ADC gain changed FLAC left by +12.74 dB and left right
unchanged, localizing reversed source tones before the ADC. Do not alter I2S
channel ordering. Workstation analog output, cable and board wiring need physical
localization; a manufacturer schematic has opposite input/output jack labels,
but its match to this physical revision is unverified. Evidence lives under
`docs/audit-evidence/2026-10-07/`. Normal firmware is restored and its ELF SHA256
matches saved control `82bb107be87811c66d02fc400315564476ddcf9ce5e151c2ff30df863bcf73fa`.
All canonical checks pass. Browser player, WAV removal, port consolidation,
MP3 and Sonos remain future work. Retire this diagnostic phase after recording
packet findings; no production fix has been established. Use plain chat for
operator questions because Doctor Biz cannot select TUI answers.

## Prior Mac continuation state (2026-10-05)

Compaction count: 7. Playback reliability remains unresolved. All speculative
production changes from this resumed investigation were reverted. Normal
production was restored at 2026-10-05 13:02:47 UTC with static TX16/cache32,
mandatory PSRAM, original DeadlineWriter and Shazam off. Root's UART collectors,
decoders, native TCP sampler and relay are stopped; no board owner is active.
No merges or pushes are authorized. User `.gitignore`/`.kata.toml` stay untouched.

The new isolated Wi-Fi fixture (`1c91bc4`) also fails without capture, encoding,
web or MQTT, despite ample heap. It establishes that audio workload is not
necessary for a paced transport stall. Dynamic TX and periodic send retries
both failed and were rejected. Native Mac counters show received out-of-order
data (202,787 / 243,360 bytes) and some duplicate data. Latest harness failure
at 78.931 s was specifically the relay's two-second upstream API connect
timeout; streams were then stopped by cleanup. Do not call that a natural EOF.

Doctor Biz is away from the computer. The prior capture-selection reply did not
establish that a capture exists: neither `target/fix-stream-packets/en0.pcap` nor
a tcpdump process is visible here. Do not keep asking for Terminal operation.
No privileged action, global power setting or route change has been made.
The bounded default/more/default ACK-policy comparison is complete; it establishes
no fix. Both first runs delivered about 120 s of CRC-valid decoded audio, but
the repeated default lost both peers after about 84 s to eight-second frame-write
deadlines. The Mac's sampled receive windows remained at least 64,095 bytes;
capture-counter deltas stayed zero after each initial telemetry snapshot.
All owned clients and UART processes are stopped; production is unchanged and
Shazam remains off. Final en0 state probe answered HTTP 200 in 73.5 ms.
Retire this long session after this recorded phase. Next useful discriminator
is packet timing across the network path; current evidence does not locate
driver, radio, router, or Mac loss and does not justify another firmware tweak.

Canonical checks passed: 188 Rust tests, two pre-existing ignored, five relay
tests, seven new real-socket collector tests, harness regressions, formatting,
clippy, production target build and warning check. The separate Wi-Fi fixture
also built cleanly and passed independent ownership/timing review. Evidence is
under `docs/audit-evidence/2026-10-05/`; raw UART and network logs remain private.
The brief -30 dBFS startup reading in the last run returned near -80 dBFS;
it does not establish sustained music or broadband input.

## Native ACK-policy diagnostic, 2026-10-05 18:51 UTC

Doctor Biz clarified that they are away from the computer. No BPF capture was
available, so root used a reviewed throwaway receiver with public macOS socket
counters. Two direct en0 peers ran with default ACK option 0, then
TCP_SENDMOREACKS=1, then fresh default sockets. Readbacks confirmed each policy;
the read-only global delayed_ack value was 3. Firmware and global settings did
not change. Each run reset identically and checked Shazam off before/after;
there was no periodic API polling or live-decoder backpressure.

- First default: 119.979 / 119.893 s of CRC-valid audio in 120 s. Largest observed
  gaps between body receives were 1.883 / 2.034 s.
- More ACKs: 119.979 s each in 120 s; largest gaps 1.408 / 1.816 s.
- Repeated default: natural EOF at 83.771 / 83.846 s, with only 73.813 / 73.387 s
  of complete audio. UART independently shows 8027 / 8033 ms frame-write
  timeouts against 8021 ms limits, followed by checked close in 243 / 305 ms.

All six complete prefixes decoded without error. Capture deltas stayed flat;
startup RSSI was -49 / -50 / -50 dBm. ACK-policy causality remains inconclusive:
the unchanged control itself both passed and failed. Short runs do not establish
endurance, sustained source music, or worst-case FLAC throughput.

Evidence: `docs/audit-evidence/2026-10-05/ack-policy-comparison.json`.
Private raw data: `target/ack-comparison-20261005T185104Z/`.
Tools: `target/ack-policy-check.py`, `target/run-ack-comparison.py`, and
`target/export-ack-comparison.py`. Real local TCP tests verified option 0/1,
112-byte native info, exact 100,000-byte capture, eight CRC-valid frames and
0.682667 s of decoded audio; premature HTTP-header EOF was correctly rejected.
Independent review corrected metadata bounds, exit status and cleanup races
before hardware use. No repository production code changed in this phase.

Public TCP_CONNECTION_INFO has no duplicate-byte field; offset 104 is transmitted
retransmission packets. Its TX packet count excludes pure ACKs, and RX packet
counts can reflect coalesced buffers. Out-of-order bytes do not locate loss or
measure lost-packet percentage. Public ESP-IDF station getters expose RSSI and
PHY/AP context, but no supported ordinary-TCP retry-count API was established;
do not treat the raw-80211 TX callback as a proven TCP completion observer.

## Resumed investigation sequence (historical)

Resumed after Doctor Biz asked to fix the playback blocker. Root owns the board
and build; clean-context sol-6.1 agents audit the target socket contract and the
historical successful run. Compaction count: 6. The current static profile is
being reproduced with one 120-second en0-relayed decoder before a controlled
Wi-Fi profile comparison. No production fix is established yet.

Historical runB decoded 600.064 seconds in 601 seconds with dynamic TX32 and
577/579 successful API polls. Its exact image source hash is unavailable; input
and routed source address also differ. IDF forbids dynamic TX with mandatory
PSRAM and Wi-Fi/lwIP PSRAM preference together, so the experiment must explicitly
account for coupled driver behavior. The temporary diagnostic enables
IGNORE_NOTFOUND and verifies mapped PSRAM on this board before client tests;
it preserves TRY_ALLOCATE_WIFI_LWIP, whose source still prefers PSRAM regardless
of IGNORE_NOTFOUND. The profile changes static/cache/by-reference TX into
dynamic/no-cache/copying TX. Restore the boot guarantee before any production
adoption. Preserve current encoder/deadline/close policy.
Source audit found no fcntl/send/poll contract bug;
nonblocking readiness low-water hysteresis remains an unproven alternative.

Fresh unchanged-production control, 2026-10-05 12:17 UTC: one en0-relayed
listener decoded 120.0 seconds; 31 API polls passed and capture fault deltas
stayed zero. Two listeners requested for 300 seconds then failed at 65.458
seconds: UART independently records ring lag after 41.9 seconds and checked
close in 449 ms. This reproduces the blocker despite a short one-listener pass.
Evidence is in `docs/audit-evidence/2026-10-05/static-*.json`. The temporary
dynamic profile passed all canonical stages; its generated config confirms
TX type 1/count 32, no cache/static count, PSRAM preference, unchanged RX and
TCP limits. ELF SHA256 `ba2aa653e447f430a264a27c5d0c479fcfb20625ec98b749f877bad77e48dc55`.
Root is flashing that diagnostic; no causal result yet.

Dynamic profile outcome: failed at 93.806 seconds, independent ring lag after
71.1 seconds, checked close 66 ms. Capture fault deltas stayed flat. Internal
minimum was lower than the static control. Boot verified mapped PSRAM and
dynamic TX32. `dynamic-two-failure.json` preserves the result; this profile is
rejected as a fix. Source defaults are restored and all canonical stages pass;
root is restoring the static production image. A bounded en0 packet capture
requires macOS administrator access; Doctor Biz has been asked to start it in
Terminal, restricted to this board's ports 80/8000 and 30,000 truncated packets.
No packet capture permission or changed system settings is assumed.

Static production restoration finished at 12:31:02 UTC. Next independent
diagnostic: target-only 20 ms poll slices followed by nonblocking send retries,
retaining the absolute frame deadline and original flags. Pinned lwIP clears
readiness after a partial write and reasserts it only above its send-buffer and
pbuf low-water marks; retry slicing tests that delay without switching to a
blocking send, whose timeout can wait a nominal extra TCP poll interval. This
is temporary until real decoded comparison and deadline tests establish value.

The retry diagnostic failed at 108.331 seconds: ring lag after 85.4 seconds,
checked close 179 ms, capture fault deltas zero. The eight-line experiment is
removed. Canonical verification passes for restored production, flashed again
at 12:42:41 UTC. No candidate fix remains in production source.

Bounded path probes: five en0-bound DF ICMP requests with 1440-byte payloads
all answered (27–203 ms); five at 1100 bytes also answered (20–80 ms). This does
not prove TCP delivery, but no large-packet black hole appeared in these probes.
The Mac's local gateway answered ten probes (4–26 ms). Mac system sleep is
prevented and low-power mode is off; no power or routing settings changed.

Next independent isolation: a separate Wi-Fi-only throughput example, using
the same pinned network configuration and DeadlineWriter, two bounded real
peers, deterministic checked bytes, no capture/encoder/web/MQTT/Shazam workload.
`working_comparison` owns new example/host harness; root retains all board/build
ownership. This tests whether audio workload is necessary for the stalls. It is
a diagnostic, not a production fallback or playback acceptance. Packet capture
request remains pending; no BPF access has been granted or assumed.

Wi-Fi-only fixture reviewed and built without warnings. Seven real host
regressions and the full canonical check pass. Two peers receive 15,814,656
deterministic bytes each, offered as 11,264-byte chunks twelve times per second
for 117 s (135,168 B/s each, approximately 1.3% above the prior quiet FLAC rate).
Each chunk expires 8.021 s after its scheduled time, with 120 s total; host
checks every byte and EOF with bounded reads. Heap snapshots label concurrent
completion and owned buffers. Host completion alone is not a receive-pace claim;
inspect per-second bins. ELF SHA256
`fc59f269e17ff56ff9ae3329262aa365db4c34b632d2e1bd97163f0b84fff0c2`.
Root is flashing this isolated fixture; it must be followed by production restore.

Wi-Fi-only outcome: both failed, with correct received prefixes of 891,072 and
1,380,512 bytes, approximately 54.6 and 74.0 kB/s including their stalls. UART
records scheduled write-deadline failures after 14.618 and 18.198 s; host EOFs
arrived after 16.332 and 18.660 s. Both peers had zero-byte delivery seconds.
Before admission internal free/largest was 181,675/77,824 B and PSRAM
4,126,772/4,063,232 B; concurrent completion snapshots retained at least
169,467 B internal free. This demonstrates that audio/application workload is
not necessary for a paced transport stall. It does not isolate driver, radio,
router, or Mac. Same BSSID/channel as production; startup RSSI was -61 dBm
versus prior -48/-49 dBm, so radio conditions are a remaining confound.
`wifi-only-failure.json` preserves exact host data. All test peers finished;
root stopped its UART and relay and is restoring production again.

Compaction count: 5. The validation controller released board ownership after
cleanup; no UART, flash, relay or client process remains active. Root owns ticket
integration. FINAL PRODUCTION RESTORED: normal image 7ee8a2a with ac93d16 close
fix flashed successfully 2026-10-05 07:21:26 UTC. Generated settings are cache 32,
static TX 16, MSS 1440 and send buffer 46080; boot cache 32 and absence of fixture
markers were verified.
Last bounded tests failed: two-peer 300 s at 32.159 s on API timeout; one-peer 600 s
at 96.052 s on API timeout with independent ring lag after 71.3 s. All owned host
clients, UART collectors and relay were cleaned; production remains running.
Final framed HTTP state probe at 07:27:59 UTC answered in 828 ms with Shazam off.
No long soak acceptance pass.
No merges or pushes are authorized. Shazam stays off under R26.

- Isolated capture health policy passed after complete-line UART reader repair.
- One FLAC and one WAV trickle admission round passed; TCP states reclaimed.
- Six-hour production soak failed after 33.156 s. Raw diagnostics confirmed
  terminal-frame truncation with valid complete frames; no endurance pass.
- Direct drainers showed nonzero windows and ACK/retransmission stalls. The
  output observer recorded 49 actual single-pbuf driver ESP_ERR_NO_MEM results
  among 2364 calls. Independent pinned binary/source review identifies the
  class 9 preallocated TX-cache free-list empty return, with PSRAM-first setup;
  why slots fail to retire and per-stream cause remain unresolved.
- Corrected internal-pressure geometry reached free 35,087 B/largest 3,712 B,
  both real 4096-byte probes returned NULL, and two actual pthread ENOMEM
  admissions closed and released slots. Sampled minimum was 34,439 B above the
  32,768 B floor; capture counters stayed flat. Strict decoded recovery failed,
  so nr6z remains open.
- Client TCP_NOOPT flags were set, but actual board MSS stayed 1440; no smaller
  segment or PMTU conclusion follows. en0 direct sockets each delivered 641
  matching complete CRC-valid frames (54.6987 s PCM), then host-cut tails after
  60 s. Output still recorded 128 driver ERR_MEM among 10,492 calls. API polls
  used default routing; production decoded endurance is still unverified.
- The default-route cache 32 → 64 → 32 experiment had zero driver errors in both
  repeats, but transport still failed. On en0, cache 64 also removed measured
  errors yet lost both streams earlier. Original cache 32 remains; no fix claim.
- Early -120 dBFS readings later became quiet ADC noise; decoded RMS about
  -82.42 dBFS. No FIP music or worst-case input acceptance follows.
- User-owned `.gitignore` and `.kata.toml` stay untouched. Failed evidence lives
  in `docs/audit-evidence/2026-10-04/`; raw files remain private under `target/`.

Full canonical verification passed for the final source: 188 Rust tests, two
existing ignored tests, five real relay socket tests, harness regressions,
formatting, clippy, firmware build and warning checks. Eight katas are closed
(three completed fixes and five archived-only issues closed as out of scope);
nine remain open. Playback/API reliability is the high-severity shipping blocker.
Do not increase buffers again without new evidence. The next useful network
evidence is packet-level delivery/ACK tracing; local BPF capture was unavailable.
Physical broadband-input and Shazam-on acceptance remain unperformed; R26 still
excludes enabling recognition.

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
- `01cf98d` requires PSRAM and enables static TX16/cache 32 while preserving
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
One early state reported -120 dBFS; this does not establish a physical source change. Output-observer
fixture flash is underway; restore production and final canonical check follow.

`66f2717` output observer with two direct raw-file drainers recorded hook installed,
2,364 single-pbuf output calls, 49 single-pbuf ERR_MEM results and no chained or
other errors. Pinned source maps the single-pbuf result to actual driver
ESP_ERR_NO_MEM; all STA ports are counted, and no proprietary pool is identified.
Both streams ended naturally after about 24–25 s since admission with partial
terminal frames. Complete prefixes contain 136/138 valid frames and decode
cleanly; all 136 shared complete frames match. Eight capture samples stayed flat.
API polling retained four timeouts. Early -120 dBFS meter readings later became
-86/-84.6; decoded overall RMS was about -82.42 dBFS and frames about 11 KB.
This was quiet nonconstant input, not constant silence, FIP music or worst-case
input. `driver-output-errors.json` preserves all counters and raw references.
No transport/endurance pass follows. Failure-only pressure geometry diagnostics
are being reviewed before one further observation; no allocation policy changes.

Failure-only geometry observer `7099af7` again FAILED before READY, now with
bounded evidence: free 37,143 B, largest 11,776 B, sampled minimum 37,135 B above
32,768 B floor, eight holes released and 30 owners (all rows reported). Four
initial holes remained contiguous/unreleased. The conservative filler admission
requires floor + 8,192 B, exceeding remaining free, while a stack-sized tail
remains. No stack/admission failure was exercised. Preserve
`failed-internal-pressure-tail.json`; no floor/policy relaxation occurred.

Controlled cache-capacity experiment changed only TX cache 32 to 64, then restored
32 and repeated the same observer/drain collection. Generated config and boot
counts were verified. B64: 3,951 single calls, zero errors, but both peers reached
8,029 ms frame deadlines after about 29.29 s; delivered ~93,122 B/s per peer.
A32 repeat: 5,358 calls, zero errors; one peer ended after 10.92 s, one reached
60.45 s host bound. Complete prefixes decode cleanly (B64 241/241 frames; A32
23/625), capture faults flat. Original A32's 49 errors did not recur, so no causal
capacity/fix claim follows. B64 minus A32 repeat pre-listener heap: PSRAM free
-54,400 B, internal free +32 B, internal largest unchanged; this supports observed
PSRAM placement, not an unconditional fallback guarantee. Source cache 32 is
fully restored, no config commit. Evidence: `cache-capacity-comparison.json` and
both detailed observation JSONs. Board remains A32 observer pending next phase;
mandatory production restore/final canonical check still required.

Corrected pressure geometry `5d4ea21` produced READY free 35,087 B / largest
3,712 B, ten holes, and real NULL 4,096 B probes before/after. Two real pthread
ENOMEM failures returned zero-byte EOF, checked close 1 ms and released slots.
Sampled end minimum 34,439 B exceeded unchanged 32,768 B floor; capture counters
stayed flat through release. Strict run still FAILED: decoder 1 did not meet
five-second progress check; final cleanup showed 0.597333/3.754667 s real decoded
audio, no full 10 s decodes or third-client cap check. No nr6z closure or global
OOM guarantee follows. Preserve `failed-internal-pressure-recovery.json`.


## Interface-scoped comparison — 2026-10-05 06:43–06:50 UTC

A real en0-bound `/api/state` request answered HTTP 200 with Shazam false in
67.438 ms. Two en0-bound raw drainers then held the 60-second host bound;
641 sequential complete frames per peer passed CRC16 and clean prefix decode.
All shared complete frames matched. Host-cut tails were incomplete. Actual
PCM delivery in successive 20-second windows was 17.7493, 22.1867 and 14.7627 s;
the late deficit prevents a realtime/endurance claim. Output counts were
10,492 single-pbuf calls and 128 driver ERR_MEM results. Capture fault counters
were flat. API meter polls still used default routing and one timed out.

The unchanged default-route repeat produced natural EOF after 17.7662 and
43.5155 s receiving. Its files contained 92 and 407 complete sequential frames,
no terminal partial frames, and decoded cleanly. UART reported both peers fell
a ring behind; checked closes took 165 and 185 ms. Output recorded 114 actual
driver ERR_MEM among 4,174 calls; capture fault counters stayed flat and one
API poll timed out. This path comparison identifies a useful direct-interface
control, without proving a transport root cause or fixing routed acceptance.

Structured evidence: `client-noopt-observation.json`, `en0-preflight.json`,
`en0-direct-drain-observation.json`, `default-route-repeat-observation.json`
under `docs/audit-evidence/2026-10-04/`. Private byte/timing/UART evidence remains
in `target/audit-flac-tee/20261005T064355.718928Z` and
`target/audit-flac-tee/20261005T064907.073755Z`.

Next: reviewed local en0 relay for strict pressure recovery, then mandatory
production restoration, refreshed canonical check after host harness changes,
and at least five minutes of actual two-decoder production validation before
considering a long soak. Cache32/MSS 1440 remain unchanged.


## Cache-capacity en0 control — 2026-10-05 06:56 UTC

The approved temporary cache 64 only observer built without compiler warnings;
generated header and boot verified 64, static TX 16/MSS 1440/send 46080 unchanged.
Two actual en0-bound sockets naturally ended after 22.0538/21.9275s receiving.
Each file contained 126 matching complete sequential CRC-valid frames, decoded
10.752 s PCM cleanly, with no terminal partial. First 20 s delivered10.4107 s PCM;
only0.3413 s followed before EOF. UART attributed both departures to ring lag
at 21.9/21.0 s, with checked closes 118/893 ms. Capture fault counters stayed flat.
Output hook 1 recorded 2083 single-pbuf calls and zero driver errors. This did not
improve the earlier cache 32 control; keep 32 and do not infer success from zero
errors. Pre-listener PSRAM free 865084 B versus 919492 B (-54408 B), internal 81999 B
versus 81975 B (+24 B), support sampled placement only. Full temporal output
increments and receive windows remain in `en0-cache64-observation.json`;
private raw path `target/audit-flac-tee/20261005T065638.371359Z`.

Sourcecache 32 restored; no permanent firmware configuration change. Reviewed
host relay changes and canonical check follow, then strict pressure via explicit
en0 relay and mandatory final production flash. No 6 h soak before bounded decoded
production acceptance.


## Strict pressure via en0 relay — 2026-10-05 07:04 UTC

Reviewed relay/harness endpoints forwarded real traffic through en0 index 11,
with 16 KiB relay buffers and no routing changes. Cache32/MSS 1440 were generated
and flashed. Ready free 35119 B/largest 3712 B with 10 holes; both 4096-byte probes
returned NULL. Two real pthread ENOMEM refusals produced zero-byte EOF and
checked closes. End free 34895 B/largest 3712 B; sampled minimum 34483 B remained
above 32768 B. Release free 80039 B/largest 77824 B.

Strict result FAIL: recovered 1 did not reach required decoded progress within
5 s. Both recovery sockets were actually admitted, relay forwarded 116771 bytes
per peer, and both decoders reported only 0.085333 s at cleanup with empty stderr.
The third-peer cap was unchecked. UART capture fault counters stayed flat
through pressure, release and sampled recovery (last 41.572 s board time).
No natural recovery close was recorded before harness cleanup. Keep nr6z open;
no threshold changes or blind rerun. Structured failure:
`docs/audit-evidence/2026-10-04/failed-internal-pressure-en0-recovery.json`;
private raw: `target/audit-internal-pressure-en0/20261005T070423.159120Z`.

Canonical host/production check at 84d0128 passed 188 Rust tests, two preexisting
ignored tests, five real relay tests, harness regressions, formatting/clippy,
firmware build and warnings. Refreshed dd79c11 canonical verification passed the same full stages. Production restoration remains mandatory after fixtures.


## Encoder fault via en0 relay — 2026-10-05 07:12 UTC

Strict result FAIL at 44.667 s: peer lacked recent pre-suspension body progress.
UART recorded both frame-write deadlines at 18.060/18.070 s board time,
8031/8037 ms against 8021 ms, then checked closes 2028/2011 ms. ARM arrived 31.190 s;
the actual scheduled 13 s suspension/resume occurred after both peers had already
failed. Capture fault counters remained flat, but no attributed fault EOF,
recovered decoding or admission cap acceptance follows. Structured evidence:
`docs/audit-evidence/2026-10-04/failed-encoder-en0-progress.json` with actual
pre-close-logger-fix ELF hash; private raw directory
`target/audit-encoder-en0/20261005T071200.303392Z`.

Final canonical dd79c11 passed every stage: 188 Rust tests and two preexisting
ignored, five relay tests, harness regressions, fmt/clippy, firmware build and
warnings. A newly reviewed narrow logger fix requires another canonical check
before mandatory final production restoration. Then two actual decoders run
for five minutes through en0 relay; if that fails, collect one-listener 10 min
without further buffer/MSS changes. No long soak starts from held socket counts.


## Final production restoration and bounded failures — 2026-10-05 07:21–07:24 UTC

Normal production 7ee8a2a/ac93d16 flashed without example at 07:21:26 UTC exit 0.
Generated config and UART boot verify cache 32/static TX 16/MSS 1440/send 46080;
fixture markers absent. Real state polls verified Shazam false. Source/config/ELF
hashes are embedded in both final evidence files. Exact-source canonical
check passed 188 Rust tests, two existing ignored, five real relay tests, harness
regressions, fmt/clippy, firmware build and warnings before flash.

The 300 s two-decoder en0-relayed check failed 32.159 s on actual state timeout
(seven earlier successful polls), with no decoded progress at the live summary.
Cleanup final progress was 0.597333/0.170667 s. Each decoder's stderr separately
starts with Immediate exit requested then residual/decode errors; raw bytes
were not retained, so do not claim independent encoder corruption. UART
admitted both, but collection stopped before natural close evidence.

The authorized 600 s single-decoder check failed 96.052 s on state timeout.
Independent UART peer departure was ring lag after 71.3 s, checked close 2191 ms;
ffmpeg decoded 59.050667 s and reported premature HTTP end at 7882060 bytes.
Capture frames advanced and all capture fault counter deltas stayed zero.
Actual sampled heap/state/decoder data remains in the full reports. Neither
bounded test passed, so no blind six-hour rerun follows.

Structured final failures:
`docs/audit-evidence/2026-10-04/failed-final-production-two-en0.json` and
`failed-final-production-one-en0.json`. Raw paths:
`target/audit-final-production-two-en0/20261005T072141.403548Z` and
`target/audit-final-production-one-en0/20261005T072243.114765Z`.
All owned decoders/UART/relay were cleaned; relay termination exited0 and a
process inventory confirmed no owned client remained. Board stays on normal
production with Shazam off. Routed playback, strict pressure/encoder recovery,
worst-case physical input, on-profile tests and long endurance remain open.


Final post-cleanup en0 state probe at 07:27:59 UTC passed the unchanged canonical
framing/JSON/Shazam false checks in 828.055 ms (240 wire bytes,110 decoded body bytes).
A first throwaway collector incorrectly waited for TCP EOF despite receiving
240 bytes and timed out; preserve that as a collector defect, not a board API
blackout. The corrected run reused actual scripts/http-probe with only en0
socket binding added; no UART reset or config changes. Evidence:
`docs/audit-evidence/2026-10-04/final-en0-state-after-cleanup.json`.

Production UART also recorded 118 encoded frames per ~10.07 s with resets 0 and
encoder task runtime ~1.50–1.66 s/window. That supports sampled real-time production
while delivery lagged; it does not identify the transport cause.
