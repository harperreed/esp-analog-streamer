# Close reaper and the six-hour soak (wmsy: new reaper ticket, bf85)

## Now
- Step: bf85 run 2, second launch: `target/kata-hardening/oct20-soak-1`,
  launched 2026-10-20 04:1x UTC on the reaper image `2ed701de…`, ends
  about 10:15 UTC. The first launch (`oct19-soak-3`, 23:5x UTC) ran
  14,617 s clean: 48 stalls, zero departures, exact frames, counters
  flat, `/api/metrics` never unavailable, PSRAM largest unchanged,
  internal largest 40,960 B at start and end (one transient dip to
  34,816 B); only 2 of 48 stalls produced a skip (one per listener,
  ~275 frames each, the 4 s stall plus TCP backoff), the rest were
  absorbed. It ended on a corrupt UART line (`discontnuities=0`, a
  dropped byte) that the parser treated as fatal; fixed
  (`CorruptCaptureLine`, recorded and skipped), tests green, check and
  commit pending while the soak runs.
- Next: evaluate soak 3 against: zero departures, exact frames, counters
  flat, internal largest within 2 KiB of its start, PSRAM largest
  unchanged, no reboot; report the skip pattern (per stall and between
  stalls) rather than gate on one-skip-per-stall, which both images
  missed in the 120 s checks this evening; write
  `docs/audit-evidence/2026-10-20/stall-soak.{md,json}`; bf85
  close-or-record. Relaunch if needed with `ESPFLASH_PORT=/dev/ttyUSB0
  scripts/stream-soak 192.168.23.217 --seconds 21600 --listeners 2 --path
  /stream.mp3 --require-clean-startup --stall-every 300 --stall-seconds 4
  --interface enp5s0 --out target/kata-hardening/<new dir>`. Never
  `pgrep`/`pkill -f` a name that is in your own command line (it matches
  itself; use `stream_soa[k]`). Later: b814 (evening skips on both
  images), ek5b (needs tcpdump), n4jb, ee3q.
- Open: whether `tcpdump` joins the sudoers rule (only ek5b needs it; not
  this plan)
- Approved: "ok. let's do it. let's do #1, and then #2 (starting now? )"
  (2026-10-18); earlier "Yes, push and open the draft PR" (2026-10-15)
  covers pushes of this branch
- Compactions: 1

## #1 Close reaper (early slot release)

Today a listener's slot stays held until its socket's checked close
completes: `SO_LINGER` 2 s plus lwIP's 500 ms close poll
(`firmware/src/socket_close.rs:12-15`), and the close runs on one of the
two permanent output workers (`firmware/src/stream.rs:249-256`), which is
why releasing the slot earlier is not a flag flip — a worker blocked in
`close()` cannot serve the next job, so a freed slot would 503 on
`try_submit` (`stream.rs:517-527`). A client that reconnects within that
window gets 503 (cv5a attempt 3 `stall-4-2`; batch 2 `stall-30-*` only
succeeded because the harness waits for the close line).

Design: a close reaper task owns draining sockets. When a listener ends,
the worker hands the socket (and its `RetainedClose` bookkeeping) to the
reaper and returns immediately; the slot is released at hand-off; the
reaper performs the checked close with the existing linger semantics and
stamps the Closed event; the two-slot bound now counts active streams,
with at most two more draining sockets in the reaper (bounded queue; a
third hand-off waits, keeping the slot, so memory stays bounded at four
sockets). A close failure (`RetainedClose`) keeps today's behavior:
recorded, and the board restarts, because a socket in an unknown state
must not be reused silently. UART receipts keep the Closed line and add
`released_us` so the harness can measure hand-off to re-admission.

Acceptance: core/host tests for hand-off, bounded queue, failure path and
event ordering (the `close_timestamp_precedes_capacity_reuse` test
inverts: release precedes close); the loom listener model extended with
the reaper's queue; `scripts/check`; on hardware, `network-fault-batch --
--policy live --trials null-1,null-2,stall-30-2` must show the
replacement admitted before the departed socket's close line, and the
client zoo unchanged. 7fe1 closes as superseded when this lands.

## #2 bf85, the six-hour soak

bf85 asks for six hours of real playback with repeated controlled Wi-Fi
reconnects. Wi-Fi manipulation is out of bounds (office network), so the
controlled interruption is the receiver-side stall the batches already
use. Two runs:

1. **Tonight, now**: `scripts/stream-soak` two MP3 listeners for 21,600 s
   on today's image — pure endurance under live semantics: exact decoded
   accounting, capture counters, heap minima, skip counters sampled, no
   injection. Pass/fail bounds set before the run: zero departures,
   exact frames, loss counters flat from the post-startup baseline,
   internal largest free block within 2 KiB of its start, PSRAM largest
   unchanged, no reboot.
2. **After the reaper**: the same six hours with a receiver stall of 4 s
   every 5 minutes (a `--stall-every/--stall-seconds` option on the
   soak, reusing `network_fault`'s qdisc plan), expecting zero
   departures and one skip per stall per listener. That run is bf85's
   closure evidence; tonight's is its endurance half and e8hd's
   endurance gate.

Both runs restore the ordinary image and leave settings unchanged; the
board stays the dev board; no Sonos involvement.

## Limits

Receiver-side stalls stand in for Wi-Fi reconnects by rule. Six hours on
one image, quiet LINE IN, MP3 only.
