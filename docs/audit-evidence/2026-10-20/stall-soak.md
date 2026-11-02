# Six-hour stall soak, two listeners (bf85 run 2; 2026-10-19 to 2026-10-20)

Six hours and six minutes of two live MP3 listeners on the close-reaper
image `2ed701de…` with a 4 s receiver-side blackhole every 300 s: all 71
scheduled stalls applied, zero departures, both decoders exact with zero
discontinuities for 21,365 s and 21,324 s of audio, loss counters flat
from the post-startup baseline, internal largest free block back at its
start value, PSRAM largest unchanged, no reboot, every tc tree restored.
That is `oct20-soak-3`, the fourth launch; the three before it were
clean for the board and ended on harness defects, each fixed with the
real failure as its test. Together the four launches cover 15 h 48 min
and 186 stalls without a board-side departure. The contract holds: a
stalled consumer gets a skip, never a hang-up. What the run also
measures is the cost of a stall under this contract: about 8 s of
skipped audio per listener per 4 s stall (the board's TCP retransmit
backoff after the path returns, ticket 2rjh), and skips between stalls
in the hours when the Wi-Fi hop is lossy (ticket b814). Rows and hashes
are in `stall-soak.json`.

## Bounds set before the run, and what `oct20-soak-3` measured

| Bound | Measured |
| --- | --- |
| zero departures | 0 in 21,969 s; no `left after` line on the UART |
| exact frames modulo the board's skips | 1,025,523,072 and 1,023,568,128 frames, 0 discontinuities, 0 skipped source frames per decoder |
| loss counters flat | queue_overflows, software_dropped_frames, read_errors, restarts, discontinuities all +0 |
| internal largest free block within 2 KiB of its start | 40,960 B at start and end; one transient dip to 34,816 B |
| PSRAM largest unchanged | 2,359,296 B throughout |
| no reboot | 0 boots after the harness's own reset |

Also measured: 205 skips and 49,914 skipped frames across both slots
(about 1.4 skips and 8.4 s of audio per listener per stall); longest
sampled media age 9.0 s, 31 samples over 4 s, none over 10 s; 12
dropped `/api/metrics` answers and 4 dropped `/api/state` answers,
all tolerated; no corrupt UART line.

## The four launches

| Launch | Wall | Stalls | Departures | Skips (frames) | Max media age | Ended on |
| --- | --- | --- | --- | --- | --- | --- |
| `oct19-soak-3` | 14,619 s | 48 | 0 | 4 (1,094) | 3.4 s | corrupt capture line, a dropped UART byte (`dcad09d`) |
| `oct20-soak-1` | 9,337 s | 31 | 0 | 83 (20,218) | 9.7 s sampled; the decoder saw more than 14 s | decoders' 14 s read allowance at a stall the board rode out (`fbba366`, 2rjh) |
| `oct20-soak-2` | 10,953 s | 36 | 0 | 80 (21,863) | 17.6 s | corrupt encoder-telemetry line (`93a71d5`) |
| `oct20-soak-3` | 21,969 s | 71 | 0 | 205 (49,914) | 9.0 s | harness wall deadline without the skipped audio; fixed in this commit |

The overnight launch skipped on only 2 of 48 stalls; the daytime
launches skipped on nearly every stall and between stalls. The image is
the same; the hour is not. The pre-reaper image showed the same daytime
pattern in the 120 s checks (`close-reaper.md`).

## What this does and does not prove

- Proven: six hours of two real decoders through 71 controlled
  interruptions with the board holding every socket and every bound.
- Not a Wi-Fi reconnect: receiver-side stalls stand in for them by rule
  (the office network is never touched). No Sonos, MP3 only, quiet LINE
  IN, one board.
- The plan's "one skip per stall per listener" expectation was wrong for
  both images; the measured cost is recorded instead.
- The decoders stopped 235 s and 276 s short of 21,600 s of audio
  because the harness clock did not count the skipped audio; the board
  was still streaming cleanly when the harness closed the sockets.
