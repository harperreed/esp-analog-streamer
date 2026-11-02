# Over-the-air updates proven on the home board (2026-10-21)

The second A1S, at Doctor Biz's house on reedazawa-internal, took its
last USB flash onto the two-slot layout and then two updates over the
tailnet: the same image onto the other slot (12 s upload of 1,687,440 B,
a 202 with the image's descriptor, a restart, back on `ota_1` with a new
boot token and the persisted +24 dB input gain, marked valid 76 s after
boot), and a deliberately broken image that panics after codec setup
(430 KB, 202, restart into `ota_0`, panic, and the bootloader back on
`ota_1` within 10 s of the push, new boot token, slot still `valid`). The
first OTA-layout image, `6af9d246…`, crashed every boot 15.6 s in: fat
LTO had left the `audio-metrics` task 56 bytes of stack and the first
ten-second telemetry export overflowed it; that image needed one more USB
flash because the board never lived long enough for an upload. Fixed by
raising that task to 6 KiB and the stream output workers to 5 KiB
(`034ea8cf…`, up 115 s and counting at first check, then through both
updates). Rows in `ota.json`.

## Timeline (UTC)

| Time | Event |
| --- | --- |
| 20:5x | USB flash of `6af9d246…` (OTA layout, input gain, LTO); `/api/ota` and `/api/input` answer; +24 dB set from the office raised a quiet input from -75 to -53 dBFS and `settings.pga_gain` read 0x88 |
| 21:0x | reboot loop found: `***ERROR*** A stack overflow in task audio-metrics` after the first `stream-encode` line, every boot, from the monitor on Doctor Biz's laptop; `metrics_stack_free_bytes=56` on the lines before it (568 before LTO) |
| 02:12 | OTA push of the fixed image refused by a reboot mid-upload: the board never lived 16 s |
| 02:2x | USB flash of `034ea8cf…`: up 44 to 115 s at first check, gain restored, `validated` true after 60 s of Wi-Fi |
| 02:23:48 | `scripts/ota-push` of the same image: 202 at 02:24:00, running from `ota_1` at 02:24:09, `pending_verify`, boot token changed |
| 02:25:16 | `ota_1` marked `valid` by the health thread |
| 02:27:18 | push of the broken probe image (`f310a406…`): 202, restart, panic, back on `ota_1` from boot `71e7963c` within 10 s |

## What this proves and does not

- Proven: upload, restart into the other slot, health mark, and rollback
  on a crash before the mark, all on the real board over the real
  network. Pushing the image a board already runs is detected only by
  the boot token, which the script now requires.
- Not proven: rollback on a hang rather than a panic (the task watchdog
  would have to fire), an upload interrupted by Wi-Fi loss (the slot is
  abandoned and the running image untouched by construction, not by
  test), and the office board, which was unplugged throughout.
- Headroom after the stack changes is not yet measured on the UART; the
  home board's internal free RAM sat at 19 KB before them.
