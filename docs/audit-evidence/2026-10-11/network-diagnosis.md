# TCP delivery investigation: acceptance evidence

Reviewed pd6c receipt, October 11. This adds a read-only pinned-source
inventory. It neither changes firmware nor claims reliable delivery or a located
root cause. pd6c explicitly permits precisely documenting unavailable evidence.

## Pinned API and counter inventory

Paths below are relative to
`firmware/.embuild/espressif/esp-idf/v5.5.5/components/`.
Within lwIP, `core/`, `api/` and `include/` abbreviate `lwip/lwip/src/`.
The inventory concerns the checked socket/public driver interfaces, not every
symbol in the closed Wi-Fi driver binaries.

| Requested observation | Verified source and meaning | Availability / limitation |
| --- | --- | --- |
| TCP retransmission count | `lwip/lwip/src/include/lwip/tcp.h:305` has PCB `nrtx`; `core/tcp_out.c:1695,1766` saturates it at 255; `core/tcp_in.c:921,1264` resets it. `tcp_out.c:1774` increments `mib2.tcpretranssegs`. | `nrtx` is not a lifetime retransmission total. PCB/raw-stack access needs stack-context synchronization; it is not a socket getter. No `TCP_INFO` definition or implementation was found in this pinned lwIP tree. MIB2 is aggregate instrumentation, not a per-listener socket receipt. Neither is collected by this firmware. |
| Send-buffer / pbuf pressure | `include/lwip/tcp.h:326,328,434,436`: `snd_buf`, `snd_queuelen`, `tcp_sndbuf`, `tcp_sndqueuelen`. `core/tcp_out.c:336,662` checks queued pbuf limits. `api/api_msg.c:1768–1857` handles byte space and nonblocking partial writes; readiness checks at `:389,425` depend on byte and pbuf thresholds. | Raw PCB fields exist, without a supported socket pressure getter identified here. Existing WouldBlock/poll aggregates observe acceptance pressure but cannot distinguish byte exhaustion, pbuf allocation and scheduling. Kconfig mentions `TCP_SNDBUF` at `lwip/Kconfig:661`, but a whole pinned lwIP search finds that name only in this help text; this receipt does not promise that runtime option. |
| Aggregate allocation errors | `include/lwip/stats.h` gates `lwip_stats` and `TCP_STATS_INC`; `core/tcp_out.c` increments `tcp.memerr` on several failure paths. | Aggregate errors would not identify a listener or a Wi-Fi allocation failure. Both retained generated sdkconfigs have `CONFIG_LWIP_STATS` disabled; no runtime sample is claimed. |
| Wi-Fi TX allocation failures / station retries | Public `esp_wifi/include/esp_wifi.h:1413–1421` declares `esp_wifi_statis_dump(uint32_t modules)`. `esp_wifi/Kconfig:105–116` describes cache queue use after static TX allocation fails. | The public dump declaration documents neither module masks nor a structured counter schema, units or reset semantics. No typed public getter for the requested allocation-failure/retry totals was identified in the inspected headers. A usable dump with verified meanings remains unavailable, rather than assumed zero. |
| Wi-Fi TX completion | `esp_wifi/include/esp_private/wifi.h:592–611`: `wifi_tx_done_cb_t` and `esp_wifi_set_tx_done_cb`, with boolean `txStatus`. | Private interface; completion status is not a retry count or TCP ACK. Not installed or measured. Driver internals and callback safety/cost were not established. |
| Link changes / effective radio | Public association APIs and events are used in `firmware/src/net.rs`: station disconnect count/reason/RSSI, network generations, IP observations, channel, negotiated PHY and protocol/power readback. | These are implemented link observations, not packet retries or proof of an unbroken radio path. Historical receipts retain their own image/boot/config identities. |

Both generated sdkconfigs under
`firmware/target/xtensa-esp32-espidf/release/build/esp-idf-sys-{f1224fba55564b99,cdba9cc7a85cd33a}/out/sdkconfig`
agree: MSS 1440, TCP send buffer 46080 bytes, static TX buffers 16,
cache TX buffers 32, static RX buffers 10 and dynamic RX buffers 32.
These are local generated-build observations, not live-device readback.

## Buffer tradeoffs from pinned source

`lwip/Kconfig:652–667` says a smaller TCP send buffer saves RAM and can reduce
performance. `include/lwip/opt.h:1396–1418` ties the default pbuf queue limit to
send-buffer size/MSS and defines separate writable low-water thresholds.
Increasing byte capacity therefore does not remove every allocation/readiness
constraint and does not establish a cure for delayed ACK/output/service.

`esp_wifi/Kconfig:70–116` distinguishes dynamic per-frame TX allocation from
static buffers, recommends static TX with PSRAM, assigns approximately 1.6 KB
per static TX buffer, and describes a dedicated cache queue when static TX
allocation fails. Extra slots consume memory and permit more queued data;
they do not identify why service is slow. No buffer enlargement was performed
for this receipt. No numerical performance benefit or latency bound is inferred.

## Original acceptance mapping

The five requested per-listener fields map as follows. These compressed-listener
departure aggregates are distinct from live MP3/FLAC/WAV write observations.

| Requested field | Actual field / calculation and limit |
| --- | --- |
| Cursor lag in milliseconds | `firmware/src/stream_pipeline.rs:586` retains `queue_frames_max = max(ring.progress().1 - receipt.sequence)`. `core/src/flac_ring.rs:144,232` makes producer position the committed frame count and receipt sequence the selected frame's one-based sequence: this counts newer queued blocks, **excluding** the selected frame. Convert to maximum observed queue duration as `queue_frames_max * block_samples * 1000 / sample_rate` (48 kHz for these trials). This is a departure maximum sampled at reads, not live current cursor lag or a bound during a blocked write. Media age measures elapsed time since software capture acceptance and is a different quantity. No dedicated current cursor-lag-ms field is exported; that literal field remains absent. |
| Bytes sent per second | `stream-sender.accepted_bytes * 1000000 / payload_elapsed_us` when elapsed is nonzero; `firmware/src/stream.rs:1045–1047,1255`. This is the payload-interval mean of stack-accepted bytes including partial final writes, not ACKed/received throughput or a rolling one-second sample. |
| Total socket-blocked time | `stream-sender-poll.poll_wall_total_us` records writable-wait wall; `stream-sender.send_wall_total_us` separately records send-call wall (`stream.rs:1255–1262`). Both include preemption/rescheduling. Pure TCP-buffer-blocked time is unavailable; do not rename either aggregate as an exact measurement of it. |
| Maximum frame/write duration | `stream-listener.frame_write_wall_max_us`, updated around each non-header payload write (`stream.rs:995–997`); separate `send_wall_max_us`/`poll_wall_max_us` retain syscall boundaries. Wall includes scheduling and wrapper work. |
| Disconnect reason | Pipeline `End` becomes the deferred departure's `EndCode`; exporter logs `left ...: {end}` (`stream.rs:1265`). Wi-Fi disconnect reason/RSSI is a separate link event, not the listener's causal explanation. |

Independent review accepts these as sampled maximum lag, mean accepted rate and
elapsed socket-operation measurements. The original issue does not explicitly
require a live lag field or scheduler attribution. Their limits above remain;
neither a pure task-blocked-time claim nor a current-lag claim is supported.

| Original criterion | Existing evidence / precise limit |
| --- | --- |
| Reproducible full/half-rate audio-free controls and one/two-listener production comparisons | [Historical radio controls](../2026-10-07/wired-radio-comparison.json) retain deterministic two-peer full/half/full controls at 135168/67584 bytes per second per peer, with full-rate failures and half-rate completion. [Sender overhead](../2026-10-10/sender-overhead.md) retains current 16-bit source MP3/FLAC/MP3 production trials: 30 seconds solo then approximately 60 seconds joint. Historical absolute transport rates are not declared equivalent to today's encoded workload. These independent controls do not locate a cause. |
| Simultaneous sender/AP/receiver evidence where access allows, drops and actual route | [Receiver correlation](../2026-10-08/stream-packet-correlation.md) and its JSON retain simultaneous UART/receiver acquisition, wired receiver interface `enp5s0`, 7945 packets and zero kernel drops. Parent read-only verification on October 11 reports `ip route get 192.168.23.217`: direct via `enp5s0`, source `192.168.23.153`; default gateway `192.168.23.1`, interface UP. This establishes the receiver route, not AP radio topology. There is no retained AP-side or sender packet capture. No AP capture/control method has been supplied or AP-management tool identified; Doctor Biz's access answer is pending. Parent's `sudo -n true` requires interactive authentication; the October 8 receiver capture was user-started. AP reachability/access impossibility is not claimed. |
| Correlate holes, ACK progress, send pressure, lag and capture health; quantify overhead | Receiver correlation retains sequence-hole/ACK/window analysis and exact failed-stream body reconciliation. [Sender analysis](../2026-10-10/sender-analysis.md) separates send-call wall from writable-poll wall and capture/encoder health; [sender overhead](../2026-10-10/sender-overhead.md) quantifies the matched ON/OFF bundle. [Live metrics](live-listener-metrics.md) supplies current listener media-age/write and network observations. These are separate acquisitions: no exact cross-clock sender/AP/receiver pressure-to-hole correlation exists. UART arrival times are not an exact device clock mapping. |
| Implicated hop or precisely missing evidence; no receiver-only root cause | Receiver delayed in-order delivery is established; its captured windows never reached zero. Long MP3 send calls and FLAC writable waits identify observed boundaries on other runs. Missing evidence is same-run TCP stack output/ACK and byte/pbuf state, Wi-Fi TX allocation/retry state, and AP ingress/egress/queue/radio observations with clock mapping and drop counts. Sender scheduling/mailbox/copy work, Wi-Fi/AP service and downstream loss remain unseparated. |
| Pinned buffer tradeoffs; no blind enlargement or endless soak | The inventory and tradeoffs above record source-backed meanings and unavailable counters. This receipt performs no soak, tuning, hardware action or firmware build. |

The bounded sender diagnostics are complete under closed child d2cb; their
accepted-byte counters mean stack acceptance, including partial final writes,
not receiver delivery or acknowledgment. Reporting and storage remain bounded;
the matched overhead receipt retains its code-layout and sequential-run limits.

Failed production FLAC, WAV and MP3 trials remain failed. An investigation
acceptance decision must not erase those failures or imply a streaming release.
Independent acceptance review leaves pd6c open. The AP-access answer is pending:
if access is available, collect the synchronized evidence; otherwise document
the specific unavailable acquisition. A pending access answer is not evidence
that collection was impossible. A proven root cause, successful streaming or
another general soak is not required to finish this investigation ticket.
