# Six-hour two-listener endurance soak (bf85 endurance half, e8hd gate; 2026-10-19)

Six hours of two live MP3 listeners on the real board, image `613e7621…`
(built from `3fb6667`, before the close reaper), decoded exactly:
1,036,800,000 audio frames per listener, which is 21,600 s at 48 kHz,
zero discontinuities, zero skips, no departure until the decoders' own
end, loss counters flat from the post-startup baseline, PSRAM largest
free block unchanged, internal largest free block one 2 KiB step below
its boot value from the moment the listeners joined and flat for the
remaining six hours, no reboot. Every bound set in the plan before the
run held. The first attempt the night before was clean for 74 minutes
and then stopped by the harness, not the board: one `/api/state` poll
passed its 3 s timeout; that rule is fixed and the fix is on record. This
is the endurance half of bf85 only: no injection, quiet LINE IN, MP3
only, no Wi-Fi reconnects (the office network is out of bounds), no
Sonos. The stall half runs on the reaper image. Rows and hashes are in
`endurance-soak.json`.

## Run

- `scripts/stream-soak 192.168.23.217 --seconds 21600 --listeners 2
  --path /stream.mp3 --require-clean-startup` from harness commit
  `8b8926b`; raw artifacts under `target/kata-hardening/oct19-soak-1/
  20261019T125750.979125Z` (gitignored; the peer is masked here).
- Board reset once by the harness at start; boot token `05cb06db…`
  (read from `/api/metrics` after the run, no reboot in between); baseline
  taken at 22.7 s after a clean startup.
- Elapsed 21,629.5 s wall; both decoders reached their 21,600 s of audio.

## Bounds set before the run, and what was measured

| Bound | Measured |
| --- | --- |
| zero departures | 0 during the run; the two `left after 21600.1 s` lines are the decoders exiting at their own `-t` |
| exact frames | 1,036,800,000 per listener, 0 skipped, 0 duplicate, 0 discontinuities |
| loss counters flat from baseline | queue_overflows, software_dropped_frames, read_errors, restarts, discontinuities all +0 (baseline queue_overflows 8 from startup settling) |
| internal largest free block within 2 KiB of its start | 47,104 B at start, 45,056 B from 33 s (listeners joined) to the end: exactly one 2 KiB step, no further movement |
| PSRAM largest unchanged | 2,359,296 B throughout |
| no reboot | 0 boots after the harness's own reset; no panic or watchdog line |

Other readings: internal free 48,043 B at start, 46,415 B lowest (at
3,072 s), 47,451 B at the end; PSRAM free 2,729,884 B at start,
2,698,876 B lowest, 2,729,840 B at the end; encoder stack low-water
4,920 B, encode worst case 107.6 ms per window, 0 encoder resets;
listener stack low-water 1,044 B; board skip counters 0/0 on both
departure lines. `/api/state` answered 4,268 of 4,269 polls (the one miss
is the board's own reset at start); answer time 40 ms median, 0.7 s
worst.

## Limits

- Task runtime share (`runtime_valid`) is measurable only for the first
  71.6 minutes of uptime: the firmware marks a window invalid once the
  32-bit microsecond runtime counter can have wrapped. 428 windows were
  valid, 1,731 unjudged. This affects CPU-share telemetry only.
- Quiet LINE IN; the encoder's work is representative of silence, not
  music.
- One image, one board, one workstation receiver on the office network.
