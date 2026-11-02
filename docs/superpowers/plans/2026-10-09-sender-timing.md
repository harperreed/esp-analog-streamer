# Next transport diagnostic

Read-only recommendation after feature commit fa7f610. No product changes, firmware build, hardware operations or commits performed in this task.

Measure the existing `DeadlineWriter` path before changing TCP policy, task priorities, encoder allocation or buffering. The matched observation supplied by the parent is MP3 solo 0.794, joint 0.475/0.433 with EOF, versus FLAC solo 0.996, joint 0.965/0.955 without EOF over 90 s. Encoder throughput was near real time for both formats. This makes the actual sender/delivery deficit the next target.

## What the pinned source establishes

All IDF paths below are relative to `firmware/.embuild/espressif/esp-idf/v5.5.5/`.

- `core/src/stream_http.rs:56`: the production writer sets nonblocking mode for the whole frame and restores original flags afterward. At line75 it calls `send`; at line 94 it waits for writability only after WouldBlock. `wait_writable` begins at line 103 and polls the same absolute deadline. Existing frame-level wall time combines all these operations.
- `firmware/src/stream.rs:738`: listener telemetry currently counts complete frames/bytes and total/max frame-write wall time. At line841 it starts the whole-frame write measurement. It cannot distinguish send calls from writable waits. Telemetry logs at departure, outside the media loop (`:357`).
- `components/lwip/lwip/src/api/sockets.c:1450`: `lwip_send` uses `NETCONN_COPY` and calls `netconn_write_partly`. A positive result means stack acceptance, not acknowledgment or delivery to the reader.
- `components/lwip/lwip/src/api/api_lib.c:1011`: nonblocking mode prevents waiting for TCP send-buffer space. It still invokes `netconn_apimsg` (`:1067`), which calls `tcpip_send_msg_wait_sem` (`:134`).
- `components/lwip/lwip/src/api/tcpip.c:460`: with core locking disabled, that operation posts to the TCP/IP mailbox and waits for its completion semaphore. Consequently even a successful nonblocking send can spend wall time waiting for TCP/IP processing or for the listener to run again.
- `components/lwip/lwip/src/api/api_msg.c:1768`: available TCP send bytes limit an attempt. Around`:1813–1857`, incomplete nonblocking writes mark the socket non-writable and `ERR_MEM` becomes WouldBlock if no bytes were accepted. WouldBlock can represent byte-buffer or allocation/pbuf pressure; it does not identify which.
- `components/lwip/lwip/src/api/api_msg.c:389` and`:425`: writable notifications require both send-byte and queued-pbuf low-water conditions. `components/lwip/lwip/src/api/sockets.c:2453` waits for that notification through a semaphore. Poll wall time also includes rescheduling after notification; it is not a pure measurement of how long TCP lacked space.
- Generated `firmware/target/xtensa-esp32-espidf/release/build/esp-idf-sys-f1224fba55564b99/out/sdkconfig:1537` disables TCP/IP core locking. Lines 1604/1608/1616 select MSS 1440, send buffer 46080 and oversize=MSS. Lines 1536/1639 select TCP/IP priority 18 with no core affinity. These are this build's values, not universal IDF defaults.
- `firmware/src/stream.rs:154` sets listener priority 7; the MP3 profile selects encoder priority 8, applied at`:539`. The encoder also explicitly delays one tick after every eight MP3 frames (`:608`). A completion semaphore becoming ready does not guarantee immediate listener execution.

## Smallest useful bounded probe

Add a fixed aggregate to the existing `DeadlineWriter`; use the actual same send/poll implementation in firmware and host tests. Do not add another socket implementation, retry policy or logger callback in the media path.

Collect these fields:

| Fields | Meaning |
| --- | --- |
| `send_calls`, `accepted_bytes`, `short_sends` | Count every actual send attempt and every positive result, including the accepted prefix of a frame that later fails. A short send compares the result with the bytes requested in that attempt. |
| `send_would_block`, `send_interrupted`, `send_errors` | Count actual errno outcomes. Capture errno immediately after a negative syscall result, before reading clocks or touching other APIs. |
| `send_wall_total_us`, `send_wall_max_us`, `send_over_10ms` | Separate time inside actual send calls; the last count distinguishes frequent delays from one outlier. 10 ms is one configured tick here, used as a diagnostic bin rather than a deadline change. |
| `poll_calls`, `poll_ready`, `poll_timeouts`, `poll_interrupted`, `poll_errors`, `poll_error_ready` | Count actual poll results. Positive results with ERR/HUP/NVAL count as error readiness rather than being presented as successful writable notifications. Preserve existing retry-send behavior. |
| `poll_wall_total_us`, `poll_wall_max_us`, `poll_over_10ms` | Separate wall time inside writable waits. |
| `ready_to_send_wall_max_us`, `ready_then_would_block` | Measure the interval from a successful POLLOUT return to the next send attempt, and count a WouldBlock on that retry. This can expose delays after observed readiness without claiming to measure readiness-to-running latency. |

That is six 64-bit aggregates and fourteen 32-bit counts, plus at most one pending readiness timestamp/flag. Use a compile-time size assertion of at most 160 bytes for the diagnostic aggregate and timestamp state. At most two listeners means at most 320 bytes of added per-writer state. If the chosen platform representation exceeds that bound, reduce fields before proceeding; this is a design budget, not a measured struct size.

Clear only these new socket aggregates after the HTTP/media header succeeds. Record one `payload_started_us` at that point, then emit `payload_elapsed_us` at departure. Preserve the existing lifetime telemetry. This avoids subtracting startup-buffer fill and header work from payload-only totals. Current `ListenerMetrics` starts before startup fill (`firmware/src/stream.rs:342`); its existing elapsed field is not payload-only.

Move/copy the final aggregate into the existing departure report once, including on partial-write failure. Log after `stream_compressed` exits, as today. Keep a separate short line if extending one format string would inflate stack use. No per-frame formatting, UART writes, heap allocation, shared lock, channel traffic or stack scan. Exclude these counters from API polling.

For each send attempt: two monotonic clock reads, one existing syscall, fixed integer additions/max comparisons and errno classification. For each poll attempt: the same two clock reads and fixed accounting around the existing syscall. A pending POLLOUT timestamp adds one comparison/subtraction at the next send. There is no payload copy and no loop added by the probe. Existing deadline checks remain authoritative; measure probe overhead on the device rather than inventing a duration estimate.

## How to interpret it

| Result | Next inference |
| --- | --- |
| Few WouldBlock results and polls, but send wall time dominates | Stack handoff, TCP/IP work/copy cost or listener rescheduling is the next boundary to inspect. This is not evidence that TCP send bytes are full. |
| Many WouldBlock results and poll wall time dominates | Stack acceptance is constrained. This is genuine socket backpressure/readiness waiting, although scheduler delay can extend the measured wall time. Distinguish byte-buffer, pbuf allocation, ACK/loss/window and writable-notification causes next. |
| Large POLLOUT-return-to-send gaps | The application did not promptly attempt its next send after observing readiness. This supports investigating listener scheduling or intervening work; it does not measure the earlier semaphore wakeup delay. |
| Large frame-write wall time unexplained by send/poll wall totals | Inspect mode changes and scheduling between measured operations. The residual includes instrumentation and wrapper work; do not call it pure scheduler time. |
| Accepted bytes near required rate while receiver delivery is low | Move downstream to ACK/output/packet evidence. Stack acceptance and receiver delivery remain separate measurements. |

Existing task-runtime totals provide context, but per-call `vTaskGetInfo` deltas would not reliably isolate CPU use: the existing runtime accounting excludes an unfinished scheduling slice. Do not add per-send task scans. Stage 1 cannot prove whether time inside a send was copy/cache work, mailbox delay or runnable-but-not-running delay. If those dominate, a second, separately scoped pinned-lwIP hook around mailbox dispatch/completion would be needed; do not add that complexity before this boundary is measured.

The PSRAM placement hypothesis remains secondary. A one-time buffer address/size at admission is already logged by `PsramBytes`. Retain it alongside format/peer, but do not allocate, sweep or move buffers to diagnose sender stalls. Long accepted sends with little polling would justify a later matched placement/copy experiment; writable pressure would direct investigation toward transport first.

## Nagle and 960-byte writes

No stream source sets TCP_NODELAY. The build's configured MSS upper bound is 1440, not 1460; the actual negotiated PCB MSS may be lower. `TCP_MAXSEG`, `TCP_INFO` and an outbound-queue byte ioctl are not implemented in this pinned sockets source; do not invent a getsockopt-based queue probe. `SO_SNDBUF` is explicitly unimplemented (`components/lwip/lwip/src/include/lwip/sockets.h:220`).

`components/lwip/lwip/src/include/lwip/priv/tcp_priv.h:90–105` defines the exact Nagle predicate: output is allowed when nothing remains unacknowledged, NODELAY/fast-retransmit applies, the first unsent segment reaches MSS, there is a second unsent segment, or send-buffer/queue capacity is exhausted. `components/lwip/lwip/src/core/tcp_out.c:1347` stops output when that predicate fails, subject to memory-error/FIN exceptions. Successful socket acceptance can therefore precede wire output while a lone short segment waits.

This is a plausible pacing/coalescing difference between MP3 and larger FLAC writes. It does not establish the sustained deficit's cause: continued MP3 acceptance can fill a segment or create another unsent segment, which releases the Nagle gate. Small writes also cause more application→TCP/IP handoffs per unit of audio, and sender scheduling can delay those handoffs independently of Nagle. Do not label every delayed MP3 frame a delayed-ACK stall.

Retain the default setting for the first telemetry run. Pair accepted-byte progress and actual receiver/packet timing. If acceptance stays prompt while short-segment output waits for ACKs, a separately scoped NODELAY comparison becomes useful. If acceptance itself stalls, investigate that boundary first. Do not turn NODELAY on as a claimed fix without matched evidence; extra small packets can change driver/queue pressure.

## Feasible validation for the next task

Use TDD against the real production writer and actual loopback sockets:

1. An ordinary reader receives exact bytes; accepted-byte accounting equals the payload, send calls are nonzero, no fields imply more accepted bytes than requested, and original descriptor flags are preserved. Do not assert a platform-specific exact number of sends.
2. Reuse the existing small-buffer stalled/bursty reader cases (`core/src/stream_http.rs:660`,`:739`). Assert real WouldBlock/poll activity, accepted partial bytes on failure, unchanged absolute deadline and restored flags. Do not require a short send on a platform that reports pressure through WouldBlock instead.
3. A receiver begins draining an already pressured real socket; verify a POLLOUT retry is counted and accepted bytes arrive exactly, using actual socket pressure and thread synchronization rather than a fake send/poll backend. Retain the existing timeout and later-write tests.
4. Header followed by payload verifies resetting the diagnostic counters excludes only header accounting, without losing accepted bytes from a failed final payload frame. A broken peer retains the original OS error even with timer reads added.

Host tests validate accounting and unchanged write semantics; they cannot reproduce ESP32 scheduler/cache timing or certify lwIP transport behavior. The parent then runs the same matched MP3→FLAC→MP3 solo/joint workload, retaining receiver sample progress, EOF/close reason, encoder rate, capture deltas and new departure aggregates. Compare the diagnostic image with the retained baseline before interpreting an apparent improvement. Run canonical scripts/check and scoped review before device use.

Recommendation: scope the next task to this fixed sender accounting and its real-socket tests. Do not combine it with an encoder optimization, batching, TCP_NODELAY, priority changes or Wi-Fi profile changes.
