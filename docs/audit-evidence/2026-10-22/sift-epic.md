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
