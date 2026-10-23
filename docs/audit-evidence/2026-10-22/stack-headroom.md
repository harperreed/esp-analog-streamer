# Stack headroom on the fat-LTO image, office board (2026-10-21)

Image `2130ed3b…` (HEAD `d688899` built with the office config: MP3,
medium buffer, recognition off, no Sonos session, OTA layout), 130 s of
UART right after the USB flash, with one MP3 listener for 20 s, three
`/api/metrics` polls and one WAV reader. Each row is a task's lowest
reported free stack in that window against its allocated size.

| Task | Stack | Lowest free | Source line |
| --- | --- | --- | --- |
| stream-encode | 8,192 B | 5,096 B | `stream-encode: … stack_free_bytes=` |
| output workers (listener pipeline) | 5,120 B | 1,880 B | `listener_stack_free_bytes=` on departure lines |
| audio-metrics | 6,144 B | 1,832 B | `metrics_stack_free_bytes=` on telemetry exports |
| stream-reap | 4,096 B | 3,548 B | `reaper_stack_free_bytes=` on Closed lines |
| audio-supervise | 4,096 B | 2,544 B | `supervisor_stack_min_bytes=` |

For comparison, the first LTO image (`6af9d246…`, audio-metrics at
4,096 B) reported 56 B free on that task and overflowed at the first
ten-second export. The listener's 1,880 B is on a 5,120 B stack; on the
pre-LTO 4,096 B stack the same task reported 700 to 1,052 B.

Not in this window: the Sonos device task (no session on the office
board; the home board reported `stack_free_min=8,916` of 16,384 B while
playing on 2026-10-21), the recognizer (compiled out), the OTA health
thread (not spawned on a USB-flashed slot), httpd (`http_stack_free_bytes`
is in `/api/metrics`, not on the UART).

The WAV reader was cut after 2.9 s with `fell a ring behind`: an exact
reader at 288 KB/s over the board's Wi-Fi hop, ended by the exact rule as
designed; the MP3 listener ran its 20 s. Internal free RAM sat at 47.6 KB
throughout. The office board then took its first over-the-air update
(7.5 s, into `ota_1`) on 2026-10-22.
