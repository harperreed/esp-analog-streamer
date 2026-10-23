# Live-stream robustness for any consumer

## Now
- Step: hardware acceptance DONE (2026-10-18). Batch 2 on image
  `613e7621…` (24 s live budget): 19/19 under the final verdict
  (`19d7608`), every stall through 15 s survived with exact frames, 30 s
  departed cleanly and re-admitted, both deficits kept their sockets.
  Client zoo 5/5 (ffmpeg, mpv, mpg123, sox, curl, zero reconnects). A
  narrow sudoers rule for `tc`/`ip link` now lets the agent run the
  batches itself. Evidence pair under `docs/audit-evidence/2026-10-18/`.
- Next: commit the evidence, close dctv on it, record v6vw/pqqq/cv5a
  outcomes; then the one remaining item from the contract, early slot
  release during close linger (a close reaper), as its own bounded task.
- Known consequence to state in the evidence: a Live listener whose
  supply (encoder) is persistently more than 4 s stale drops frame after
  frame with one skip event each and never terminates; that is the
  contract, and it will read as a skip storm in metrics rather than a
  departure. Exact readers still end with `MediaAge` there.
- Open: none
- Approved: "i want to stop thinking about sonos directly, and focus on
  making the stream MUCH more robust. for ANY consumer" (2026-10-15);
  "Yes, that's the contract (Recommended)" to live-radio semantics with
  exact-audio readers kept strict (2026-10-15)
- Compactions: 0

## What breaks today, measured

Every listener loss measured this week has one mechanism. The consumer
stops reading for a while (receiver stall of 2.5–4 s in cv5a attempts 3
and 5; a quiet-network sender stall of 3.98 s in attempt 5 and 3 of 60
churn cycles), the frame being written ages past the media-age bound
(4 s from `oldest_accepted_us`, `core/src/media_age.rs:16-35`; plus the
preset seconds on the buffered route), and the board ends the listener
(`End::MediaAge`, `core/src/stream_pipeline.rs:358-391`) through the
checked-close path. There is no in-stream resynchronization: every end is
terminate, linger about 2 s, slot released, and the consumer must notice
and reconnect (`stream_pipeline.rs:727-740`). Sonos reads that close as
end of track and never retries (passive-reconnect, resume probe cycle 1);
ffmpeg reports "stream ends prematurely"; a browser or mpv will do the
same. The board is treating a slow reader as a fault. The reader was
fine; it was just late.

Second mechanism, smaller: a reconnecting consumer during the ~2 s close
linger hits a held slot and gets 503 (cv5a attempt 3 `stall-4-2`).

## The contract: live radio semantics

1. **A lagging listener is never terminated for lag.** When its cursor
   would exceed the age bound, the listener skips forward to the newest
   complete frame and continues on the same socket. MP3: frame boundary,
   decoders resync on the sync word. FLAC: frame boundary with the
   existing per-listener renumbering (`core/src/flac.rs`
   `renumber_frame`), so numbering stays monotonic and CRCs valid. WAV:
   drop samples. The consumer hears a gap the length of its own stall.
   Each skip is counted per listener and exported in metrics and the UART
   receipt (frames skipped, seconds skipped, count).
2. **Termination is for dead sockets only**: write error, TCP timeout,
   network lease change, capture reset where no valid frame exists to
   skip to, or an explicit Stop. The socket-death detector for a live
   listener is the encoded ring depth (about 12 s for MP3 and FLAC, 2 s
   for FLAC with recognition on), decided 2026-10-15: a socket that has
   accepted nothing for longer than the board holds audio has lost data
   that cannot be supplied, so it ends with `StoppedReading`; any shorter
   stall never ends the listener. Exact readers keep the 8 s limit. It no
   longer competes with media age.
3. **Debug and archive readers keep exact-audio termination** (the WAV
   diagnostic path and any `?exact=1` request): a skip would corrupt
   sample accounting there. Default for an unknown consumer on
   `/stream.mp3` and `/stream.flac` is live semantics.
4. **Response head for any consumer**: `HTTP/1.0 200 OK`-style unbounded
   body (no chunked encoding, no Content-Length), `Content-Type:
   audio/mpeg` or `audio/flac`, `Cache-Control: no-cache, no-store`,
   `Connection: close`, `Accept-Ranges: none`, `icy-name` and
   `icy-description` headers (ICY metadata off), the shape Icecast serves
   and every player and browser already accepts. Verify the exact current
   head at `firmware/src/stream.rs` and change only what differs.
5. **Slot release on close initiation**, not after linger: a reconnecting
   consumer is admitted while the old socket drains. The two-slot bound
   stays; the held-during-linger window goes.
6. **Startup burst** stays as the buffer presets (1/4/8 s of history);
   default for an untagged consumer becomes Medium (4 s), which fills a
   typical player's buffer at connect.

What this does not promise: it does not hide capture resets or encoder
restarts (those remain visible as generation changes and are terminal
when no valid frame exists), it does not add listeners beyond two, and it
does not make a dead TCP connection live.

## Why this subsumes the open stream tickets

dctv asked for exactly this policy choice; v6vw's broadcast-client reset
is most plausibly the response head (item 4) and is tested by the same
client zoo; pqqq's "resume" becomes unnecessary for the common case
because the stream never ends on lag — a consumer that keeps reading
keeps playing. cv5a's fourth scenario stops mattering for any consumer
that isn't Sonos.

## Acceptance, measurable

- **Stall matrix, existing harness**: `scripts/network-fault-batch
  -- --policy live`, stalls of 0.5/1/2.5/4/8 s with one and two
  listeners: zero listener departures, decoders exit clean, decoder
  discontinuities at most the board-reported skips (ffmpeg derives MP3
  and FLAC timestamps by frame counting, so a skip shows as late audio,
  not a PTS gap), decoded frames within one frame of expected minus
  frames skipped, media age back under the null bound. Stall 15 s is
  beyond the ring depth: exactly one checked `stopped reading` departure
  per listener and a fresh admission within 10 s of it. Deficit trials:
  sustained 0.8× and 0.5× for 60 s end with the listener still connected,
  skip counter monotonic with no gap longer than twice the predicted
  period, never departed.
- **Client zoo, on the workstation, wired**: ffmpeg, mpv, VLC, `curl |
  mpg123`, a browser `<audio>` element, each playing `/stream.mp3` and
  `/stream.flac` through a 4 s receiver stall without a reconnect, with
  the gap audible and the connection intact. Receipts per client: bytes,
  reconnects (must be 0), board-side skip counters.
- **Exact readers unchanged**: the WAV diagnostic and `?exact=1` paths
  still terminate on lag and still decode sample-exact.
- **Memory and deadlines bounded**: no new allocation on the listener
  path; skip is a cursor move; heap minima unchanged across the matrix.
- **Churn**: 30 two-listener cycles with no change in internal largest
  free block (gcq8's harness).

## Skip rule (Task 1 result, 2026-10-15)

Paths corrected from the first draft: the listener state machine is
`firmware/src/stream_pipeline.rs` (`CompressedListener`, phases
`Admission → Header → Frame → Read → Write → End`, :403-727); the encoded
ring for both MP3 and FLAC is `core/src/flac_ring.rs`; the raw PCM ring is
`core/src/stream_ring.rs`.

Facts the rule rests on:
- A skip target exists and is free: `cursor_retained(1)` is the newest
  complete frame, `cursor_now()` the next one; both are 24-byte copies
  under the ring mutex the read already holds (`flac_ring.rs:119-141`).
  Every slot below `written` is a whole frame (`push`, :170-187).
- FLAC numbering is per listener: `renumber_frame` ignores the source
  number and writes `block_number` (`core/src/flac.rs:63-118`;
  `stream_pipeline.rs:423,712`). A skip is "keep counting"; STREAMINFO is
  constant; frames carry no inter-frame state.
- MP3 frames are independent: Shine's reservoir is off (`ResvMax = 0`,
  `mp3/vendor/shine/layer3.c:117`), the bridge rejects any frame with
  `main_data_begin ≠ 0` (`mp3/src/bridge.c:38-41`), and a mid-stream join
  is already tested (`mp3/tests/decode.rs:69`).
- Skips happen only at frame boundaries: reads copy whole frames
  (`offset` returns to 0), and a partially written frame must never be
  followed by a different frame (media-progress rule, 2026-10-11).
- `MediaAge` has two sites: before the write (`:636-643`) and after a
  complete late write (`:713-722`); `Overwritten` is a third lag site
  (`:595`), and with recognition on the FLAC ring is 2.05 s, shorter than
  the 4 s age bound, so lag reaches `Overwritten` first there.

| Phase / event | Exact reader (WAV, `?exact=1`) | Live reader (default on `/stream.mp3`, `/stream.flac`, buffered route) |
|---|---|---|
| Read: `GenerationChanged` (`:595`) | `EncodedReset`, terminal | `EncodedReset`, terminal (first cut; resuming across a capture reset is a separate decision) |
| Read: `Overwritten` (`:595`) | `FellBehind`, terminal | **skip**: `cursor = cursor_retained(1)` (or `cursor_now()` if the ring is empty) under the same lock; count one skip, `frames_skipped += written-1-old_sequence`; continue in Read |
| Read: `InvalidOffset` | `InvalidCursor` | `InvalidCursor` (cursor corruption is not lag) |
| Read: no fresh frame for 2 s (`:606`) | `CaptureStalled` | `CaptureStalled` (supply, not lag) |
| Read: frame metadata invalid (`:629-634`) | `MediaAge` | `MediaAge` (data invalid, not lag) |
| Read: pre-write `Stale` (`age.deadline` err, `:636-643`) | `MediaAge`, terminal | **skip** as above, then re-read; the stale frame is never written |
| Write: `TimedOut`/`WouldBlock` with any bytes accepted (`:687-696`, `:385`) | `StoppedReading`, terminal | `StoppedReading`, terminal (socket-death detector; 8 s budget; partial frame on the wire) |
| Write: other error | `Failed(e)` | `Failed(e)` |
| Write: complete but late (`age.check` fails after success, `:713-722`) | `MediaAge`, terminal | bytes counted, `block_number` advanced, then **skip** before the next Read |
| Any step: slot revoked / lease changed / encoder unhealthy (`stream.rs:1094-1100`, `:508`) | as today, precedence unchanged | as today, precedence unchanged |
| Header phase | as today (no age check) | as today |

Counters, fixed-size, per listener: `skips`, `frames_skipped`,
`last_skip_us`, exported in `last_listener_departure.stats`, the
listener slot in `/api/metrics`, and the UART departure/receipt lines;
`queue_frames_max` records pre-skip lag. No allocation on the skip path.

Flag: `?exact=1` parsed in `route_for` beside the strict `buffer` parser
(`core/src/stream_http.rs:603-636`), carried on `ListenerJob` into
`CompressedListener` next to `age`/`write_limit`; WAV is exact by
construction and gets no flag. Tagged (Sonos) requests are live unless
they say `exact=1`.

Tests that change (from the read): `core/tests/replay_pipeline.rs:529`
(`FellBehind` → skip in live mode; stays in exact), `:685` (late complete
→ skip in live); unchanged and re-asserted: `:256` (partial write stays
`StoppedReading` in both modes), `:132`, `:645`, `:877`. New: a
frame-boundary skip keeping FLAC numbering continuous; a pre-write stale
skip; precedence of Retired/NetworkChanged/EncoderUnhealthy over a skip;
a real-socket slow reader that recovers on the same socket
(`core/tests/listener_telemetry.rs:294` as the model); `route_for` table
for `exact=1` × `buffer=` × `sonos_intent`; a new loom model of an
encoder pushing across wrap while a listener resyncs (the existing
listener-loom covers slots only).

Client zoo, pinnable through mise without an apt step: ffmpeg (present),
`conda:mpv` 0.41.0, `conda:mpg123` 1.33.7, `conda:sox` 14.4.2; Firefox is
installed for the browser case. VLC is not pinnable and is dropped.

Response head and slot release (points 4 and 5), from the read: the head
is already `HTTP/1.1 200 OK`, `Content-Type`, `Cache-Control: no-store`,
`Connection: close`, raw close-delimited body (`core/src/stream_http.rs:386-411`).
Change to `no-cache` plus `icy-name`/`icy-description`, add
`Accept-Ranges: none`, and answer `HEAD` with the same head and no body
(today 405). Early slot release is gated by the two permanent output
workers blocking in `close()` (`firmware/src/stream.rs:249-256,628-633`);
it needs a close reaper and is its own task after the skip lands.

## Tasks

1. Done above.
2. Core: skip-to-live in the pipeline behind the listener role, counters,
   loom model extended for the cursor move; TDD with the existing
   real-socket and loom tests.
3. Firmware: response head, slot release at close initiation, metrics
   and UART receipt fields; `scripts/check` all stages.
4. Harness: `network_fault` verdicts learn `skips`; a `scripts/client-zoo`
   driver over the five players with receipts.
5. Hardware: the stall matrix and the client zoo on the board; evidence
   pair; close dctv, record v6vw/pqqq outcomes.

## Hardware batch 1 (2026-10-16, `oct16-interruption-1`, image `217cfbfa…`)

15 of 17 under `--policy live`, 1037 s, qdisc restored. Every stall from
0.5 s to 8 s with one listener kept its socket with exact decoded frames
and zero departures (board skips 0–2 per listener, matching the harness);
both 15 s stalls ended with one checked `stopped reading` per listener
and a fresh admission, as the contract said; 0.8× deficit kept its socket
with 4 board skips at a steady rate; the delayed WAV reader ended on its
own deadline with the MP3 listener exact. Two failures, neither the
board's:

- `stall-8-2`: one of two listeners departed `stopped reading` with a
  single sender poll of 12,048,858 µs — the 12 s live deadline, hit
  exactly. Its write began ~2 s into the 8 s blackhole and the first ACK
  came back more than 12 s later: lwIP's retransmit backoff adds several
  seconds on top of the blackhole. The ring depth was never a real bound
  (the frame is a private copy), so the live deadline becomes 2× ring
  depth (24 s MP3/FLAC, 4.1 s FLAC with recognition); 15 s stalls are
  expected to survive and 30 s becomes the beyond-deadline case.
- `deficit-0.5-1`: the board skipped (`skips=7 frames_skipped=1261`, 30 s
  of audio) and the decoder got exactly 60 s of audio over 90 s of wall
  time, but the verdict read zero skips because the ifb0 rate limit
  throttles all board→workstation traffic including `/api/metrics` (0
  of 96 samples) and the departure fell outside the window. Harness
  verdict fix: attributed decoder skips first; metrics starvation
  recorded, not failed.

## Limits

Skips are audible gaps; this is continuity, not lossless delivery, and
the evidence must say so. The receiver-side harness proves recovery from
what the receiver sees; the board's own Wi-Fi stalls (ek5b) still cause
skips rather than drops, which is the point, but their cause stays open.
