# SIFT epic yzgs: board evidence per batch

Office board 192.168.23.217, pushes over the air with `scripts/ota-push`, UART on /dev/ttyUSB0.
Report: `docs/sift-audit-2026-10-22.md`.

## Batch A: OTA (dvqk, 690e, dhhp), office board, 2026-10-22 15:09 UTC

Image `ef836673…` (c3384c9, office cfg) pushed over the air from the previous image on
`ota_1` into `ota_0`; raw capture `target/kata-hardening/sift-a/`.

- `/api/ota` polled every 5 s after the restart: `pending_verify` with `validated:false`
  until the health mark, then `valid` with `validated:true` 64 s after the first poll.
  No poll ever showed `validated:true` beside `pending_verify` (the contradiction dvqk
  removed). UART: `ota: running image marked valid` at 70.4 s of uptime.
- `GET /api/ota` once a second during the push: 6 × 200 (before and after), 19 × no
  answer within 3 s (httpd serves one request at a time and was reading the upload),
  0 × 500 `OTA handle: ESP_ERR_INVALID_STATE` (the answer 690e removed). The new 503
  "upload in progress" was not exercised: a GET never reached the handler while the
  guard was held.
- Restart 7.9 s after the 202 by the pusher's clock; the `ota-restart` thread now runs
  through `ThreadSpec` at priority 2 on core 0 (dhhp) and the 1 s sleep then restart
  behaved as before.
- Stack watermarks after the image change: `metrics_stack_free_bytes` 1,824 (1,832 the
  day before), `supervisor_stack_min_bytes` 2,504 (2,544), encoder `stack_free_bytes`
  5,096 (5,096). No listener in this run, so no listener/reaper lines.
- Upload took 70 s on the UART clock (7.7 s to 77.7 s) against about 12 s for the
  14:00 UTC push the same day. The serial collector reset the board as it opened the
  port, so the push started 8 s into a boot, and the 1 Hz GET poll left 19 half-open
  connections on lwIP during the upload. Suspect the poll, not the change; batch B's
  push without a poll is the control.
- Not yet checked: the health mark overlapping a push (dvqk's retry path and 690e's
  blocking lock in the health thread). Planned with batch B's image: restart, push at
  second 55, expect `marked valid` after `image complete` and before the restart.

## Batch B: web (t8wq, z818) and the OTA overlap test (690e), office board, 2026-10-22 15:29 to 15:36 UTC

Image `e06efb5b…` (1e690cf, office cfg); raw capture `target/kata-hardening/sift-b/`.

- Three pushes without any polling: upload 61 s and 57 s on the UART clock (writing to
  image complete), restart 7.4 s and 7.6 s after the 202, `marked valid` at 70.3 s and
  70.3 s of the next boot. The 14:00 UTC push the same day also took about 50 s end to
  end, so the office board's Wi-Fi hop, not batch A, sets the upload time; the home
  board's 12 s on 2026-10-21 was a different hop.
- The overlap case cannot occur. A push at second 57 of a pending boot was refused by
  ESP-IDF before a byte was written: `ota: upload failed: begin update:
  ESP_ERR_OTA_ROLLBACK_INVALID_STATE` (`esp_ota_ops.c:169` refuses `esp_ota_begin`
  while the running slot is pending). The health mark can never collide with an
  upload. The handler answers that refusal with a closed socket (the pusher prints
  "unreachable"); follow-up `tn86` gives it a status.
- Curl matrix on `/api/input` and `/api/sonos`: 200 for 12 dB (GET then reports 12,
  0 restored afterwards); 400 with the `GainError` text for 7 dB, 27 dB and a wrong
  key; 415 for `text/plain`; 411 without `Content-Length`; 400 `Malformed body` for
  chunked; 413 `Body exceeds 64 bytes`; 400 `Invalid Sonos action`; GETs 200. No status
  code changed. A 4 s slow drip answered 400 `Malformed body`, not the predicted 408:
  the ESP socket read reports the stalled body as an error rather than a timeout, and
  that mapping predates batch B (same `TimedOut | WouldBlock` arms at c3384c9).
- z818's busy path: two concurrent 9 dB / 15 dB POSTs both answered 200; the busy
  window is one I2C write and read-back, as the ticket predicted.

## Batches C and D (s2e1, jmwa, hyvz, hsqv, 3k3g, 6bce, 64yy, r9r6), office board, 2026-10-22 16:26 to 18:01 UTC

Image `f6da5baf…` (6359e6f, office cfg); raw captures `target/kata-hardening/sift-cd/`.

- Port 8000 matrix with MP3 active and generation 2 (hyvz): HEAD `/stream.mp3` 200; GET
  `/stream.mp3` 200; `/stream.flac` 404; `?buffer=bad` 400 on both paths (the one
  deliberate precedence change: a malformed query is 400 whatever codec is active; before,
  the inactive path said 404 while `answer` said 400); `?exact=2` 400;
  `?network_generation=0` 503; `?sonos_intent=1` with the live generation and no
  reservation 503; POST 405; WAV 200 with and without `exact=1`; HEAD `?buffer=med` 200.
- Two-listener MP3 stall soak, 900 s, a 4 s receiver stall every 300 s: the first run
  ended at 648 s on a harness gap (one `503 {"error":"metrics_unavailable"}` from
  `/api/metrics` while a publisher held the cache's try-lock; the harness treated any
  non-200 as fatal; fixed in 43468d9 with the real answer as the fixture). The rerun
  passed: 900.0 s decoded exactly on both listeners, stalls at 323 s and 623 s (4.05 s
  and 4.09 s), 4 skips, zero departures, counters flat, `internal_min_sum` 41.5 KB,
  zero corrupt capture or telemetry lines, one unanswered state poll inside the budget.
- UART comparisons against the pre-change captures: the `capture:` line is identical in
  field names and order (36 fields) and the staging maxima reset per window (6bce);
  `encoder-telemetry` `queue_bytes=440 record_bytes=200` identical, `listener-telemetry`
  `queue_bytes=2984 record_bytes=368` identical to the 2026-10-20 soak (hsqv); the
  departure `End` vocabulary unchanged (`socket error kind` for harness-closed sockets,
  `fell a ring behind` for the exact WAV reader) (r9r6).
- Stack watermarks: metrics 1,816 B (1,824 the day before), supervisor 2,480 (2,504),
  listener 1,800 (1,880), reaper 3,548 (3,548); all inside the stacks grown after LTO.
- Not run: a Shazam match on this board (recognition is off by cfg; 3k3g is covered by
  the host tests and leaves the `/api/state` shape untouched), the `flac-cost` example
  (needs a flash; 64yy's bytes are proven identical by the decode tests).

## Batch E (62a5, rmve, 4m9h) and the harness fix, office board, 2026-10-22 18:03 to 18:08 UTC

Image `3fc37e22…` (6e2bc56, office cfg: the WebSocket layer deleted); raw captures
`target/kata-hardening/sift-e/`.

- Push restarted the board 7.6 s after the 202 and the slot validated at 70 s; the image
  without the retired transport boots, serves and answers every API as before (4m9h).
- Admission probe with Sonos control off (62a5): play, discover, select, autoplay,
  format and buffer each answered 409 "Sonos control is off; turn it on in Settings";
  stop and enable answered 202; config and state unchanged afterwards. The gate's
  shape (everything but Enable and Stop refused while off) is the same as at 3b99cb3.
- Side finding: the admitted stop/enable(false) spawn the 16 KiB Sonos worker through
  `ensure_started()`, dropping `internal_free` from 47.6 KB to 29.6 KB (largest block
  23.5 KB) for the rest of the boot. Pre-existing; filed as `k4qv`.
- `scripts/sonos-http-check` (rmve's hardware proof of the `Session` wiring) could not
  run: the office coordinator "Altec + 3" (192.168.23.205) reported `PLAYING` and the
  harness requires it STOPPED; stopping someone's playback is not this session's call.
  Host coverage: the extracted `Session` transitions and the admission function are
  unit-tested; `Worker::poll`, `stop`, `fail` and `play_current` compile only on the
  target and remain to be proven by the harness when the speaker is free. Sonos control
  on the office board was switched back off afterwards.
- Harness: the 503 `metrics_unavailable` gap found by the first C+D soak is fixed in
  43468d9 (`device_metrics.Unavailable`, tolerated by `MetricsPoll`'s silence budget).

## Final image and the Sonos run (rmve, tn86, k4qv, q128), office board, 2026-10-23 20:27 to 20:50 UTC

Image `a3164f8e…` (b440b5d, office cfg: every epic commit); raw captures
`target/kata-hardening/sift-f/`.

- k4qv on a fresh boot with control off: `internal_free` 48,199 B before and 47,827 B
  after `stop`, `enable=false`, `enable=true`, `enable=false`; zero worker lines in 90 s
  of UART; `enabled=true` persisted across the next OTA restart. (The first probe had
  run against the previous image, which is why it showed the spawn.)
- tn86 at 55 s of a pending boot: a direct small POST got
  `409 {"error":"running image not yet validated; retry after the health mark"}`; the
  pusher, which would otherwise see a connection reset while still sending the image,
  now reads `/api/ota` first and refused with the same words (cdff2f9); the slot read
  `valid` after the mark.
- rmve, `scripts/sonos-http-check` with the office coordinator "Altec + 3" STOPPED (it had
  been stopped by then; Doctor Biz had approved stopping it) and the workstation mixer at
  0 dB for the run: phase 1 FLAC/Medium passed end to end (discover, Play, `connecting`
  → `buffering` → `playing`, speaker PLAYING 1 s later, positions 13 to 27 over 15 s,
  checked Stop, speaker STOPPED within 0.7 s, the listener leaving with `format epoch
  retired`). Phase 2 MP3/High reached `playing` and speaker PLAYING, then 3 s later the
  speaker reset the TCP connection itself (`left after 13.9 s: socket error
  kind=ConnectionReset`, 874 KB sent, no skips, no would-block pressure), played out its
  8 s buffer and went PAUSED_PLAYBACK; the worker observed the loss and reported
  `stopped`. That is the open v6vw problem (the Sonos broadcast client and this MP3
  stream), not the worker: every transition the `Session` enum owns ran in order, and the
  harness's cleanup "did not settle" because the board, holding no URI evidence after
  the loss, sends no remote Stop by policy. The speaker was returned to STOPPED and the
  mixer to its previous values afterwards; Sonos control on the board is off again.
- Not exercised: a forced control-read timeout (Degraded → Recovering → Playing); it
  needs the speaker's replies blocked on the speaker's side, which nothing here can do.
