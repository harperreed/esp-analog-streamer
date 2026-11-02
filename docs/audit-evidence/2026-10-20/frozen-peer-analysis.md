# Frozen-peer analysis (b814): what the 2026-10-19 runs show

Ticket b814 says a frozen MP3 reader degrades the surviving listener and
the board's `/api/metrics`. The retained data says otherwise. The same
stalls and poll failures appear with no frozen socket on the board
(null-1 at 19:02 UTC: 15 of 68 polls failed, one healthy reader blocked
1.13 s), they start before every SIGSTOP (run 1: the reader was 3.9 s
behind 1.8 s before it was frozen), and a run with a frozen socket
present for 75 s was spotless (pre-reaper a at 19:36 UTC: 0 of 126 polls
failed, survivor write max 0.109 s). Pooled over nine runs, polls failed
30.8% of the time with no frozen socket and 22 to 25% with one; survivors
stalled 4.7% of seconds without one and 1.4 to 3.3% with one. The
variable is the minute of the day. The three error types the ticket
lists are one thing: the harness's own 1 s deadline, which shuts its
socket down and so reads as EOF. The most likely cause is a bursty loss
window on the board's Wi-Fi hop, stretched into multi-second stalls by
lwIP's retransmit timer; whether the board's transmit path or the air
drops the frames is what the experiment at the end decides.

Clocks: device times are `event_us`; host times are the harness's
`elapsed_seconds`; each run's offset (host minus device, 1.45 to 1.76 s)
is the smallest `at_s - served_device_us` over its before/after samples.
UART lines reach the host about 0.33 s late. Peers are masked `P:<port>`;
`L` numbers are `uart.log` lines under `target/kata-hardening/<run>/`.

## Run 1 in full (`oct19-reaper-freeze`, reaper image, 19:09 UTC)

| host s | device s | line | event |
| --- | --- | --- | --- |
| 23.3 | 21.749 | L175 | P:33762 joined, freeze-1's only reader |
| 24.4 to 42.4 | 22.8 to 40.8 | | 11 of 19 polls fail before any freeze |
| 33 to 35, 38 to 40 | | | ffmpeg received nothing (progress file) |
| 39.6, 41.1 | 38.07, 39.50 | | media_age 3.14 s, 3.94 s: the reader is already behind |
| 42.894 | 41.338 | | SIGSTOP |
| 43.9 | 41.997 | L300 | skip, 281 frames (6.7 s), `last_skip_us` |
| 96.1 | 94.534 | L295, L299 | write timed out after 24.05 s, 53.2 s after SIGSTOP; poll_calls=7 ready=6, poll wall 33.7 s |
| 97.2 | 95.614 | L305 | fresh P:54718 joined, 1.08 s after release |
| 98.5 | 96.982 | L307 | P:33762 closed in 2446 ms |
| 113.9 | 112.37 | L349, L350 | P:33812 and P:33806 joined (freeze-2) |
| 118.4 to 133.4 | | | 10 of 19 polls fail before the freeze |
| 133.476 | 131.920 | | SIGSTOP on P:33812 |
| 181 to 183 | | | survivor's ffmpeg received nothing |
| 185.8 | 184.218 | L471 | frozen write timed out, 52.3 s after SIGSTOP |
| 185.9 | 184.365 | L506 | survivor skip, 180 frames: its write blocked 2.62 s, from about 181.7 |
| 187.1 | 185.503 | L481 | fresh P:38914 joined |
| 188.1 | 186.501 | L483 | P:33812 closed in 2281 ms |
| 193.8 | 192.267 | L502 to L506 | survivor left: write max 2.62 s, poll max 2.55 s, would_block 15, max_age 4.39 s |
| 197.8 | 196.270 | L511 to L515 | fresh left: write max 0.69 s, would_block 3 |

The survivor's block began 2.5 s before the frozen worker's poll timed
out and ended 0.15 s after it, 2.1 s before the reaper's close began.

## The other six freeze trials, key instants (device s)

| run, trial | joins (line) | polls failed before SIGSTOP | behind before SIGSTOP | SIGSTOP | skips of frozen (f) and survivor (s) | frozen timeout (line, after SIGSTOP) | fresh join | close (ms) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2, freeze-2 | 21.92, 21.96 (L176, L177) | 10 of 19 | media_age 1.05 to 3.95 s from 25.9; s stalled 33 to 34, 39 to 40 host | 40.552 on P:46096 | f 44.843, 323 frames (L313), a write that spanned the SIGSTOP; s none | 98.980 (L308, 58.4 s) | none (harness mis-paired) | 101.094 (2113) |
| 3, freeze-2 | 22.11, 22.15 (L176, L177) | 9 of 19 | media_age 1.31, 2.23 s; s stalled 37 to 39 host | 41.237 on P:44914 | s 41.144, 175 frames (L332), 0.09 s before the SIGSTOP, block began 36.9 | 94.659 (L297, 53.4 s) | 95.718 (L312) | 97.055 (2393) |
| 4, freeze-2 | 23.16 (L178, L179) | 17 of 19 | no stall, no media_age over 1 s | 42.414 on P:55504 | f 82.3, 217 frames, its own window closing; s none | past the trial (L327, 23.74 s) | none | none |
| 5, freeze-1 | 21.992 (L176) | 5 of 20 | f stalled 28 to 32 host; skip 33.87, 179 frames, 7.9 s before SIGSTOP | 41.736 | f 45.055, 177 more (L322) | 107.213 (L317, 65.5 s) | 108.272 (L325) | 109.592 (2377) |
| 5, freeze-2 | 125.56 (L371, L372) | 9 of 19 | s stalled 128 to 130 host, 2 s after joining; f skip 137.284, 244 frames (L497), 7.2 s before its SIGSTOP | 144.533 on P:43384 | s 152.2 (174), 190.9 (188), 215.4 (498, two), 274.2 (193); fresh P:49036 two skips, 476 | 196.731 (L492, 52.2 s) | 198.657 (L505) | 199.116 (2382) |
| a, freeze-2 | 21.05 (L176, L177) | 0 of 20 (0 of 126 in the run) | nothing | 40.803 on P:33142 | none | 113.655 (L330, 72.9 s; its only poll) | 503, no reaper | 115.729 (2073) |
| b, freeze-2 | 43.17 (L209, L210) | 11 of 20, first 0.3 s after the decoders started | s stalled 92 to 93 host | 61.931 on P:58682 | none; s stalled again 135 to 136, 156 to 157 host | 115.310 (L330, 53.4 s) | 503, no reaper | 117.421 (2110) |

Runs 2 to 4 ran 19:14 to 19:20 UTC, run 5 at 19:23, pre-reaper a at
19:36 and b at 19:39, 2.5 minutes after a ended. The ticket reads run
3's skip as "at the freeze instant"; it precedes the SIGSTOP, and the
4.2 s block behind it began 4.3 s earlier. A SIGSTOP cannot reach the
board for about 30 s in any case: the receiver's kernel kept taking the
stream for 29 to 42 s in every run (SIGSTOP to timeout 53.1 to 66.0 s,
less the 24 s budget). Null-1 (`oct19-reaper-capture`, 19:02 UTC, one
reader, no freeze) failed 15 of 68 polls; null-2 (`oct19-reaper-null2`,
19:32 UTC, two readers) failed none. Both rows are in the table below.

## Pooled by whether a frozen socket was on the board

Phases: none (no frozen socket), absorb (SIGSTOP to +30 s), zero-window
(+30 s to the departure line), close (departure to close plus 1 s).
Survivor seconds are ffmpeg progress blocks; stalled ones got nothing.

| phase | polls | failed | survivor seconds | stalled |
| --- | --- | --- | --- | --- |
| none | 484 | 149 (30.8%) | 488 | 23 (4.7%) |
| absorb | 272 | 68 (25.0%) | 210 | 3 (1.4%) |
| zero-window | 249 | 55 (22.1%) | 186 | 6 (3.2%) |
| close | 21 | 7 (33.3%) | 30 | 1 (3.3%) |

Whole runs, in start order: null-1 22.1% of polls failed, run 1 30.9%,
run 2 32.1%, run 3 36.6%, run 4 38.4%, run 5 30.5%, null-2 0%,
pre-reaper a 0%, pre-reaper b 34.4%. The six-hour soak before 19:00 UTC
had no skips; the stall-soak checks at 19:53 and 20:03 UTC skipped
before their first injected stall on both images; `oct19-soak-3` from
20:13 UTC had one unanswered poll and two skips, at 23:08 UTC;
`oct20-soak-1` has 1 of 356 polls failed so far.

## Sender telemetry of every listener

| run | peer | role | would_block | polls (ready) | poll max s | write max s | max_age s | skips (frames) | lines |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| null-1 | P:60058 | only | 4 | 4 (4) | 1.13 | 1.13 | 1.22 | 0 | L281 to L284 |
| 1 | P:33762 | frozen | 7 | 7 (6) | 24.05 | 24.05 | 6.81 | 1 (281) | L296 to L299 |
| 1 | P:54718 | fresh | 0 | 0 | 0 | 0.06 | 0.14 | 0 | L328 to L331 |
| 1 | P:33812 | frozen | 4 | 4 (3) | 24.06 | 24.06 | 0.98 | 0 | L472 to L475 |
| 1 | P:33806 | survivor | 15 | 15 (15) | 2.55 | 2.62 | 4.39 | 1 (180) | L502 to L505 |
| 1 | P:38914 | fresh | 3 | 3 (3) | 0.69 | 0.69 | 0.74 | 0 | L511 to L514 |
| 2 | P:46094 | survivor | 15 | 15 (15) | 2.01 | 2.02 | 2.07 | 0 | L301 to L304 |
| 2 | P:46096 | frozen | 24 | 24 (23) | 24.04 | 24.04 | 7.80 | 1 (323) | L309 to L312 |
| 3 | P:44914 | frozen | 15 | 15 (14) | 24.04 | 24.05 | 1.73 | 0 | L298 to L301 |
| 3 | P:44916 | survivor | 15 | 15 (14) | 4.20 | 4.20 | 4.28 | 1 (175) | L328 to L331 |
| 3 | P:49092 | fresh | 1 | 1 (1) | 0.27 | 0.27 | 0.36 | 0 | L337 to L340 |
| 4 | P:55502 | survivor | 19 | 19 (19) | 1.38 | 1.44 | 2.16 | 0 | L301 to L304 |
| 5 | P:58298 | frozen | 11 | 11 (10) | 24.02 | 24.02 | 4.38 | 2 (356) | L318 to L321 |
| 5 | P:35366 | fresh | 0 | 0 | 0 | 0.06 | 0.14 | 0 | L348 to L351 |
| 5 | P:43384 | frozen | 6 | 6 (5) | 24.02 | 24.03 | 5.92 | 1 (244) | L493 to L496 |
| 5 | P:49036 | fresh | 8 | 8 (8) | 5.84 | 5.84 | 5.91 | 2 (476) | L542 to L545 |
| 5 | P:43382 | survivor | 29 | 29 (29) | 9.73 | 9.74 | 12.29 | 5 (1279) | L648 to L651 |
| a | P:33142 | frozen | 1 | 1 (0) | 24.00 | 24.01 | 0.24 | 0 | L331 to L334 |
| a | P:33150 | survivor | 0 | 0 | 0 | 0.11 | 0.22 | 0 | L380 to L383 |
| b | P:58682 | frozen | 8 | 8 (7) | 24.04 | 24.05 | 1.03 | 0 | L331 to L334 |
| b | P:58684 | survivor | 24 | 24 (24) | 3.05 | 3.05 | 3.14 | 0 | L425 to L428 |
| null-2 | P:36992, P:36996 | both | 0 | 0 | 0 | 0.11 | 0.26 | 0 | L278 to L286 |

A frozen socket's one timeout poll is expected. Every other `would_block`
is a send that found lwIP's 46,080-byte send buffer full because the
peer's acknowledgements had stopped advancing; `ready_then_would_block`
is 0 everywhere.

## Mechanisms against the data

(a) Shared lwIP memory. Refuted. `internal_largest` never moved within a
run (40,960 B in runs 1, 3, 4, 5; 36,864 in run 2; 38,912 in run 1's
second trial; 45,056 to 47,104 on the pre-reaper image) and
`psram_largest` stayed 2,359,296 B through every phase; `audio:` lines
show internal free near 42 KB (run 1 L283). Nothing logged an allocation
failure or `ERR_MEM`. A frozen socket holds at most one send buffer of
unsent data (`tcp_write` refuses more than `snd_buf`); its persist probes
are one byte each. Pre-reaper a
carried a frozen socket through zero window and a 2.07 s lingering close
with 0 of 126 polls failed and a 0.109 s survivor write maximum. No phase
with a frozen socket is worse than the phase without one.

(b) Wi-Fi transmit queue or driver buffers (16 static, 32 cached). Not
decided. These images count no driver errors (no `ERR_MEM` or
`linkoutput` line in any run). The frozen socket adds no transmit load,
and the stalls precede the freezes (run 1 from device 35 s, SIGSTOP at
41.3; run 3 from 36.9, SIGSTOP at 41.2; run 5 from about 26, SIGSTOP at
41.7) and occur in null-1 with one reader, so the freeze did not cause
them. Run 4 points at this mechanism: 17 of 19 polls failed in 19 s while
both streams flowed without a stalled second. A fresh four-segment reply
cannot fast-retransmit; one dropped segment costs lwIP's initial
retransmit timer (`CONFIG_LWIP_TCP_RTO_TIME=1500`), longer than the
poller's 1 s. A stream with 46 KB in flight rides through single drops
and stalls only on a tail or repeated loss, with backoff. `wlanif.c:109`
turns the driver's `ESP_ERR_NO_MEM` into `ERR_MEM`, which lwIP treats as
a drop, so a board short of transmit buffers under a busy or faded link
would show this ordering of symptoms. Whether the drops happen in the
driver or in the air is what the experiment below measures.

(c) Output worker or stream lock held during the 24 s poll or the
lingering close. Refuted. The frozen worker sleeps in `poll()` (run 1
L299: 7 polls, 33.7 s wall; L296 task runtime 2.09 s of 72.8 s) and holds
no lock across socket calls: the encoded-ring mutex is taken with
`try_lock` in the read phase and released before the write
(`stream_pipeline.rs`). Encoder windows during the frozen phases report
`encoded_lock_wait_wall_total_us` of 25 to 26 ms per 10 s (run 1 L243,
L262), the same as idle. Timing rules it out too: in run 1 the survivor's
block began 2.5 s before the frozen worker woke and ended 2.1 s before
the reaper's close began (L471, L506, L483); in run 3 the survivor's skip
precedes the SIGSTOP; in run 5 the survivor stalled 2 s after joining,
with nothing frozen yet.

(d) `esp_http_server` starving, explaining `IncompleteRead` and
`RemoteDisconnected`. Refuted as a mechanism; the labels are the
harness's own deadline. `scripts/device_metrics.py:206` connects, then
arms a 1 s `threading.Timer` that calls `sock.shutdown(SHUT_RDWR)` and
`close()`. When it fires before the status line, `readline()` returns
EOF and `http.client` raises `RemoteDisconnected`; inside the chunked
body it raises `IncompleteRead(0 bytes read)`; `TimeoutError` is the
socket's own 1 s timeout on connect or read. The board logged no httpd
send error, no 503, no "session creation failed" or "error accepting new
connection" in any run; the only httpd lines are `error in recv : 104`,
one or two per run (run 1 L354, run 2 L200 and L248), the reset a
shut-down client sends when the late reply arrives. A reply over 1 s is
a reply with a lost segment waiting on the 1.5 s timer. Polls failed at
the same rate before and during the freezes, and not at all in null-2
and pre-reaper a under the same two-stream load.

(e) The receiver (workstation) rather than the board. Partly. The harness
explains the error labels, not the stalls. The board's sends blocked 1.1
to 9.7 s, which happens only when the peer's window or acknowledgements
stop advancing; the host kernel acknowledges on its own, and ffmpeg never
fell behind by the 25 s or more a frozen socket needed to close its
window, so an application pause cannot explain 2 to 10 s blocks. The
ffmpeg progress files show the same stalls from the host side, at the
seconds `media_age` climbs on the board. The pattern over the day (clean
before 19:00 UTC, degraded 19:02 to 19:28 and 19:39 to 20:06, clean 19:32
to 19:39, mostly clean after 20:13) fits a shared medium that changes by
the minute. The data cannot say whether frames are lost in the board's
transmit path (buffer exhaustion, rate control, retries exhausted) or on
the air and at the AP.

## Most likely cause

A time-varying loss window on the board's Wi-Fi hop, which lwIP's
retransmit timer (1.5 s initial, backed off) turns into multi-second
send blocks on the stream sockets and into replies slower than the
poller's 1 s deadline. The frozen peer is a bystander: it stops sending
once its window closes, holds one send buffer, and its worker sleeps.
The ticket's premise rests on null-2 being clean; null-1 was not, and
pre-reaper a was clean with a frozen socket present. "Survivor skips 0
across five runs" will pass or fail with the link, not with the close path.

## Next board experiment

Goal: decide between frames dropped inside the board (driver returns
`ESP_ERR_NO_MEM`, lwIP counts a link memory error) and frames lost on
the air (lwIP retransmits with no link error), freeze beside null.

Change: set `CONFIG_LWIP_STATS=y` in `firmware/sdkconfig.defaults` and
export `lwip_stats.link.memerr`, `lwip_stats.link.drop`,
`lwip_stats.tcp.rexmit` and `lwip_stats.tcp.memerr` plus
`esp_wifi_sta_get_rssi()` in `/api/metrics` and on the 10 s `audio:`
line (one struct read, no allocation). Verify at implementation that
esp-idf-sys exposes `lwip_stats` under that option; if not, read it
through a small C shim. Add a `deadline` label with the phase (connect,
status, body) and the elapsed time to `device_metrics.py`, so a slow
reply is no longer recorded as a disconnect; keep the 1 s budget, it is
a sensitive loss probe.

Run: on one image, alternate `null-2` (60 s) and `freeze-2` (120 s) six
times each inside one 25-minute window, polling at 1 Hz, with
`scripts/capture-stream-packets` on the workstation for the whole
window. Read per-second deltas of the four counters against the poll
failures, the survivor's `media_age` and the ffmpeg stall seconds.

Expected either way. If `link.memerr` steps at the stalled seconds and
`tcp.rexmit` follows, the board's transmit path drops under load: raise
`CONFIG_ESP_WIFI_CACHE_TX_BUFFER_NUM` or revisit dynamic TX buffers
(with the 2026-10-05 caveats), and consider a lower
`CONFIG_LWIP_TCP_RTO_TIME`. If `tcp.rexmit` steps while `link.memerr`
stays 0, the loss is on the air or at the AP; the board can only shorten
its recovery, and b814 is outside the board. If neither moves while a
send blocks, look at the tcpip task and the driver's queue timing. In
every branch the freeze trials should match their neighbouring null
trials; if they do not, b814's premise returns and the frozen socket's
zero-window state needs a packet-level look.

Limits: one board, one office AP, no AP-side evidence by rule; offsets
good to 0.1 s; stalls under 1 s are invisible; absorb fixed at 30 s.
