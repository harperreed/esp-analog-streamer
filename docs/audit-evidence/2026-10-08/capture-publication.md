# Capture publication isolation

October 11 scope reconciliation: dp08's original reader-isolation acceptance is
complete. The delayed-reader controls, three-cycle TLS setup, protocol tests,
canonical checks and cost measurements below provide the evidence. Independent
review confirmed this closure against the original ticket. The historical
decision below incorrectly made full transport health an additional closure gate.
All failed run verdicts remain unchanged. Separate startup/TLS DMA servicing
failures remain open in 93hm, including the later initial Sonos TLS recurrence
in [buffer-presets.md](buffer-presets.md). TCP delivery remains pd6c; this closure
does not establish release readiness.

Historical October 8 conclusion:

The shared-ring implementation and host checks passed, but full hardware
acceptance failed. Kata dp08 remains open. Production firmware and saved
configuration are restored. The final two-listener failure below prevents a
release-ready claim. Numeric results and source hashes are in
[capture-publication.json](capture-publication.json).

## Target feasibility

The pinned ESP Rust distribution 1.98.1.0 reports rustc 1.98.1-nightly,
commit `183f762d61935f539d2d68d52d80bca4bf8b7c76`, LLVM 21.1.3. A throwaway
AtomicI32/AtomicU32 SeqCst probe was compiled for `xtensa-esp32-espidf` at both
optimization levels used here (3 for core, s for firmware).

Both generated direct 32-bit operations: loads use `l32i.n; memw`; stores use
`memw; s32i.n; memw`. The 480-sample loops retain those barriers. Object
inspection found no calls, CAS/S32C1I, 64-bit helpers or unresolved symbols.
LLVM IR retains seq_cst operations with four-byte alignment. This passes the
compiler feasibility check; final linked inspection below checks production paths.

The standalone target lacks installed core libraries. The probe used cached
core/compiler_builtins libraries built by the same compiler, with explicit
`--extern` arguments. No firmware build or hardware change was needed.
Commands and full reports remain under the local SDD workspace; probe source,
IR, assembly and objects are retained under `target/capture-publication/`.

| Artifact | SHA-256 |
| --- | --- |
| atomic_probe.rs | e77a01b2aed20c79f12b048131bbcbf195439e5f6ffa0bc5ff8aa59155c4fb8e |
| atomic_probe.o (opt 3) | 78daeb59a3807a35b1c8770d4d962dcbf4b09ca8f26dddcf0103f40efe3f0bd3 |
| atomic_probe.o (opt s) | 593307069e48198d7b684ec4a2b656b2330f967a0f9a76269bd7c877b01466c2 |

Pinned IDF `components/xtensa/include/xtensa/cacheasm.h` documents MEMW's
ordering role. IDF's external-RAM atomic helper locks do not appear in these
objects. This checks the emitted code, not the compiler backend's complete
memory-model correctness. Runtime cost and cache-disabled PSRAM limitations
remain hardware concerns.

## Allocation review

Pinned Rust `alloc/src/sync.rs` defines ArcInner with repr(C), strong and weak
atomic counts followed by the data in one allocation. Pinned IDF
`heap_caps_base.c` chooses one registered heap for an allocation, and
`multi_heap_malloc_impl` allocates from that heap's TLSF pool. An Arc allocation
does not bridge internal RAM and PSRAM pools. Checking that the live ring object
is internal therefore also establishes the region for that allocation's counts,
without subtracting a guessed private header offset.

Firmware checks actual object/stamp storage endpoints as internal and
payload endpoints as PSRAM before capture starts. Normal allocation preference
alone is not the acceptance check. No custom allocator bridge is currently
justified. Measured memory and runtime costs appear below; no production task or
stack was added.

## Initial linked candidate

The default firmware build passed without warnings after correcting an unavailable
Rust binding: internal byte RAM is checked with the exposed byte-accessible and
external-RAM predicates, verified against pinned ESP32 source. The initial
candidate SHA-256 is `9f98237b6b8ec5424d2fb456c574109802e9ee8e93849c43a00d9cab3b67472f`.

Its linked StreamRing::read and CaptureBuffer::publish/invalidate use direct
32-bit atomic accesses with MEMW barriers and no atomic helper or S32C1I
instructions. Read and publish call ROM unsigned 64-bit division/remainder
for logical positions; these are arithmetic, not 64-bit atomic operations.
Their other calls are cold Rust bounds/divide-by-zero failure paths.
Invalidate has no callees. This does not claim that panic or IDF reboot paths
are free of all locks.

GNU objdump initially rendered two Rust functions as data because of Xtensa
property metadata. Inspection used a separate scratch ELF with only .xt.prop
and .xt.lit removed. Its .flash.text bytes were verified identical to the
original (SHA-256 `319e404ab7442d7a5a7f7aacfd5347d474ba29730818c7950f0ad7b16c6777b3`).
The inspection copy is never a flash artifact. Assembly and call lists are
retained under target/capture-publication/linked-*.

Independent review found no unresolved production defect. It corrected a host
test that treated valid Retry as a terminal read error, and recorded the limits
of bounded Loom exploration. All 20 final canonical stages pass; the hardware
acceptance results below retain failures as well as passes.

## First production board comparison

Implementation is committed as `1934c94`. The final pre-fixture candidate image
is `34bf5394197540c97a2323fc92d8a3fa2082c0930bcda0fe2a64cc10e1dcec79`; linked
inspection again confirms the same direct atomic operations and call classes.
Both 45-second listeners decoded exactly 2,160,000 frames, with zero skipped,
duplicate or discontinuous decoder frames and no added tracked capture faults.
Startup still had capture faults; the result does not clear startup or TLS.

Against the earlier quiet-input production control with the same listener workload,
four telemetry windows at host seconds 32–65 measured capture runtime of 8.167%
versus 9.306% of a core. Sampled internal heap minimum sum fell from 72,831 to
69,115 bytes, while PSRAM minimum sum rose from 754,292 to 768,820 bytes. These
are separate real runs, not an isolated CPU benchmark; radio/scheduling and heap
minimum sampling remain limits. The roughly 1.14 percentage-point CPU increase
and 3.7 KB lower internal minimum are costs to retain in TLS acceptance.

Recognition-on firmware also compiled without warnings and was never flashed.
The private configuration was restored byte-for-byte. All 20 canonical checks
passed again after the final diagnostic reporting change; its three actual
event-store tests run through the host suite. The final diagnostic build passed.
TLS/NVS/reconnect results appear below.

## First delayed-reader run: capture passes, listener gate fails

Diagnostic image `3a354eaab69efd33584494da7a4be90a494e245a869846a665a074725e83c7fa`
recorded coherent copies after 202,354 and 204,328 µs pauses inside actual payload
reads. A 2,508,363 µs pause returned Overrun with unchanged cursor and a fresh
240-frame read. All three cases and recovery passed; the 16-slot event array
used 392 bytes and did not overflow. Two reference buffers occupied 3,840 internal
bytes; the diagnostic task used an 8 KiB stack. Reporting followed buffer/ring
release, with retrospective timestamps. No watchdog occurred.

The overall two-listener run failed. One listener fell behind at device time
38.979 seconds, before the first injected pause at 40.211 seconds. Only one
listener remained during the pauses. It completed 5,760,000 frames; the failed
listener ended at 385,024 frames. The failure summary's 675,840-frame snapshot
for the surviving listener is stale; final retained framehashes supply the totals.
Original UART stayed open through the 120-second failure-observation interval.
Capture reached 7,710,720 frames at 162.109 seconds with queue-overflow/software
drop/restart/discontinuity totals unchanged at 14/480/2/2.

This supports reader-independent capture with one surviving listener. It does
not establish two-listener continuity or locate the transport failure. A separate
one-listener run and a two-listener repeat retain the failed run as a failure.

## Completed delayed-reader controls

A separate one-listener run passed 120 seconds: exactly 5,760,000 decoded frames,
1,407 packets, no timestamp discontinuities and no added tracked capture faults.
Its two short copies remained coherent after 203,332 and 211,380 µs pauses.
The 2,507,424 µs pause rejected the stale cursor and recovered with 240 fresh frames.

The two-listener repeat also passed 120 seconds with both listeners present during
all pauses. Each decoded exactly 5,760,000 frames across 1,407 packets without
missing/duplicate frames or timestamp discontinuities. Actual pauses were 232,789,
202,934 and 2,500,872 µs; the first exceeded its requested 200 ms, so the actual
value is retained. Short copies were coherent, the long copy was rejected, and
the stale cursor stayed unchanged before a fresh 240-frame read. Capture counters
stayed at that boot's baseline of 13 queue overflows, 480 software-discarded frames,
two restarts and two discontinuities through 152.149 seconds. Neither completed
control produced a watchdog or overflowing event store.

Diagnostic reporting is retrospective. It releases the two buffers and ring
reference, waits 20 seconds, then prints stored numeric events with bounded yields.
The 392-byte event store and 8 KiB task stack remain until reporting ends. Diagnostic
memory is extra test overhead, not production usage. The first two-listener failure
remains a failure; a clean repeat does not resolve the intermittent TCP fault.

## Host protocol verification

The old reader-held lock reproduced 8,640 lost frames before replacement. Real
publisher/reader tests now cover block visibility, partial assembly, two cursors,
reset, lag, physical wrap, split-position carry and checked identity exhaustion.
A real paused reader stays suspended while publication advances beyond two seconds;
a separate reader remains coherent. Channel-coded WAV and FFmpeg-decoded FLAC
checks validate exact stereo across the 4,096-frame boundary. Socket pressure uses
real loopback sockets and the production absolute write deadline.

Property tests use the actual ring and an independent sequential history, seed
0xCA970810, 128 cases of up to 149 operations. Loom substitutes its atomics into
the production protocol: two models, at most two preemptions, 10,000 permutations,
1,000 branches and four threads. Overwrite/reset uses two readers; era rollover
uses one. Carry/exhaustion tests are sequential, and standard Arc bookkeeping is
outside the model. These bounds support the ordering argument; they do not prove
arbitrary schedules or ESP32 progress. All 20 canonical stages passed after the
final diagnostic fix. Independent core, integration and diagnostic reviews passed.

## TLS setup with two listeners

The rebuilt TLS diagnostic completed all three verified local TLS/WebSocket cycles
in 2,529, 2,499 and 2,502 ms. Both listeners decoded exactly 5,760,000 frames over
120 seconds with no skipped/duplicate frames or timestamp discontinuities. Capture
fault totals stayed at that boot's baseline of 12 queue overflows, 240 software
discards, one restart and one discontinuity through device time 152.196 seconds.
The fixture reported recovery and no watchdog occurred. This run passes setup
acceptance; earlier DMA failures remain unresolved evidence, not erased by it.

Sampled minimum sums were 20,855 internal bytes and 758,436 PSRAM bytes. The TLS
fixture adds its own controller, observer and buffers, so this is diagnostic
headroom rather than isolated production TLS usage. The observer retained 3,994
samples without overflow, but its largest gap was 59,417 µs and two gaps exceeded
two ticks. Those sampling limits prevent a complete scheduling account. Fixture,
logger-prefix and peer timestamps have distinct origins. The peer accepted three
connections; it sent no Sonos playback or settings commands.

## Encoder and stack cost

Using encoder records whose endpoints fall in the same approximate host 32–65 s
interval as the capture comparison, three steady two-listener windows measured
13.575% of a core before and 14.095% after (+0.520 percentage points). Their
first windows begin before that interval. Restricting to the two wholly contained
windows gives 13.514% versus 14.193% (+0.679 points). Numeric capture observations
map device/logger time to host time approximately; these are not exact encoder
UART receipt timestamps. Both comparisons include charged interrupts/logging and
exclude unfinished scheduling slices. Neither isolates atomic-copy execution.

Production stack reservations remain unchanged: capture 8 KiB, shared encoder
4 KiB, acceptor 4 KiB and each listener 4 KiB. No publication task was added.
Linked function entry frames changed as follows:

| Function | Before | After |
| --- | ---: | ---: |
| Capture task function | 496 B | 464 B |
| Encoder task function | 320 B | 320 B |
| Raw ring read | 48 B | 64 B |
| Capture publish | 48 B | 96 B |

These are individual function frames, not the total nested peak. Assembly samples
remain heap-owned. Runtime stack high-water use was not measured; configured
stacks and linked frame sizes establish the compared stack cost only. Keep this
limit when assessing future TLS/recognition or worst-case noise workloads.

## Configuration writes with two listeners

The rebuilt NVS diagnostic completed 120 sequential 1,024-byte writes with every
readback verified, then erased its isolated test key and reported recovery.
The longest write took 58,225 µs. Both listeners decoded exactly 4,320,000 frames
over 90 seconds, 1,055 packets each, without missing/duplicate frames or timestamp
discontinuities. Capture totals stayed at that boot's baseline of 15 queue overflows,
480 software discards, two restarts and two discontinuities through 121.947 seconds.
No watchdog occurred. Sampled minimum sums were 59,271 internal bytes and 859,624
PSRAM bytes; the fixture adds an 8 KiB task stack and NVS work. This confirms the
publication path under the existing IRAM-safe driver/configuration-write test.

## Wi-Fi reconnect

The existing fault harness verified a real disconnect, production reconnect, old
socket cleanup/slot release and two newly admitted streams. Each fresh stream
decoded exactly 480,000 frames across 118 packets without timestamp gaps. Capture
fault totals stayed at the pre-injection baseline of 14 queue overflows, 480
software discards, two restarts and two discontinuities through 61.756 seconds.
No watchdog or unexpected reboot occurred. Original streams were interrupted as
intended; their final decoded prefixes are retained separately from earlier
summary snapshots. This is stream/capture reconnect coverage, not automatic
Sonos playback recovery or a test of foreign ownership.

## Capture health: first FLAC attempt failed before recovery

The first two-listener capture-health run failed because its EOF observer had
already disconnected before the long capture suspension. UART reports a listener
falling behind at logger time 20.057 seconds and closing at 20.707 seconds; the
long suspension marker arrived at 26.297 seconds. Two capture observations kept
queue/software-drop/restart/gap totals fixed at 16/480/2/2. This does not identify
the transport cause. The final baseline decoder file contains 663,552 frames;
the failure summary's earlier 450,560-frame snapshot is stale.

The fault harness closed its UART at failure, so later restart/reboot stages are
unobserved in this attempt. It remains failed. The unchanged-image repeat below
checks those gates separately; no timeout or buffering policy changed.


An unchanged two-listener FLAC repeat failed before injection because its decoder
showed no progress within the startup budget. The final framehash was empty and
stderr contained no decoder error. One telemetry row cannot establish capture
counter deltas. Host investigation separately reproduced a default-FFmpeg probing
false negative with constant FLAC delivered steadily over localhost. The retained
board run has no body-byte/entropy trace, so that finding does not establish its
specific cause. Both failed FLAC attempts remain failures.

The capture-only health test passed: the short pause kept fault counters fixed,
the temporary long stall requested a real capture restart, and the final permanent
stall caused software reset in 6.42 seconds, followed by reset reason 3 and
POST_REBOOT. This test has no listeners and proves no decoder/freshness behavior.

A one-listener WAV test observed EOF 1.852 seconds after the long-stall marker,
then decoded exactly 288,000 fresh frames after the real capture restart. The
final stall caused the expected software reset and boot markers. Its post-reboot
stream failed with premature HTTP EOF after 45,056 frames, below the required
384,000. The overall WAV result remains failed; a sole post-reboot capture row
cannot clear the later interval. Known-good recovery stages are kept distinct
from the failed post-reboot decoder gate.


## Known-format decoder regression

Commit `4aa931f` fixes a separate confirmed harness bug (Kata b8rs). A real paced
localhost stream from the production FLAC encoder delivered 69 complete blocks,
282,624 PCM frames and 1,008 body bytes by six seconds. FFmpeg auto detection
produced no decoded output then; its first output arrived at 12.284 seconds.
Selecting the known FLAC input demuxer alone reduced first output to 1.773 seconds
and delivered two decoded seconds by 3.720 seconds. The helper also selects WAV
and rejects unknown endpoints. Existing timeouts, CRC, discontinuity and exact
sample requirements remain unchanged.

Four real-input tests, eight stream-format tests and twelve decoder-integrity
tests pass. The canonical `scripts/check` passes all twenty stages after this
fix. Behavioral RED/GREEN receipts remain under the local probe-delay directory.
The corrected board FLAC health rerun still failed startup progress, with a final
118,784-frame prefix and no observed fixture markers. This preserves the separate
board failure; the localhost reproduction does not establish its cause.

## Connected input and settling

Doctor Biz confirmed LINE IN was connected to LINE OUT. The exact connection
time is unknown. A driven 1 kHz left / 2 kHz right source reached the ADC, verified
in a 4,096-frame WAV prefix. That requested four-second capture failed premature
EOF, so it proves tone presence only. Peaks near 2,004 Hz on ADC left and 996 Hz
on ADC right show reversed channels relative to the source file; the swap's
location remains unknown. Workstation Master and PCM measured -23.25 dB and
-0.60 dB; neither mixer was changed for these measurements.

A standalone diagnostic then acquired five codec-soft-reinit / I2S-only restart
pairs with driven input. All ten trials contained 200 complete 240-frame blocks:
480,000 frames total, with no read errors, queue overflows, late reads or watchdog.
The diagnostic has no Wi-Fi workload and is not a production recovery test.

I2S-only trials contained driven energy from their first block. Every block stayed
within descriptive limits of 1 dB AC RMS, 1,000 raw 24-bit LSB DC deviation and
1.25 times peak relative to the final 800–1,000 ms reference. Codec soft reinit
showed zeros and transients, with the last descriptive anomaly ending at 490 ms.
These comparisons support retaining the current 500 ms policy; they do not prove
physical cold-start settling or justify changing recovery delays. Absolute gain,
input voltage and channel routing remain uncalibrated. The 2,000 numeric block
rows are retained in [the settling CSV](capture-settling-connected.csv); trial
summaries, limits and hashes are in the companion JSON. Kata ed4k stays open.

## Final production acceptance failed

The restored production image `36c559ca` ran the corrected decoder harness with
two listeners requesting 45 seconds, or 2,160,000 frames each. Final decoder files
contain 1,990,656 and 532,480 frames: both failed. The original UART remained open
for another 120 seconds after failure. Listener ring-overrun logs appeared at
42.186 and 71.836 device seconds. Capture queue-overflow/software-discard/restart/
discontinuity totals stayed at 17/480/2/2 from 11.426 through 162.016 seconds, while
capture advanced from 477,360 to 7,705,920 frames. No watchdog appeared. Those
unchanged counters do not locate the transport fault or clear startup losses.

Radio readback was protocol 0x07, power-save 0, channel 6, RSSI -45 dBm and numeric
PHY value 4. The cable was connected, but the standalone tone had ended before
this soak. The final image's flash text is byte-identical to the initial candidate;
its source difference is a metric documentation correction. Configuration files
and board Sonos settings match the saved originals. The board controller is
stopped, autoplay and recognition are off, and host playback has finished. Actual
speaker playback and volume were not queried.

P0 transport diagnosis (pd6c) needs correlated packet evidence. The bounded trace
script is ready, but this session's sudo requires interactive authentication and
has no graphical display; no capture or privilege change occurred. P1 DMA service
work (93hm), settling (ed4k) and long-run acceptance (ee3q) remain separate. Keep
dp08 open until the full capture-health decoder and final production gates pass.
