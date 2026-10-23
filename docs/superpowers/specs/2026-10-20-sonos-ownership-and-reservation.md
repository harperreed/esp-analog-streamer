# Sonos ownership-safe recovery (pqqq) and listener reservation (7vhq): options

Status: proposal, 2026-10-20. No code changed, nothing here was tested.
Sonos work is deprioritized ("stop thinking about sonos directly, and focus
on making the stream MUCH more robust for ANY consumer", 2026-10-15). This
document lays out the remaining options and their costs so the two tickets
can be decided later. Sources: the two tickets, v6vw, cv5a's closing audit,
`docs/superpowers/plans/2026-10-14-ownership-safe-resume.md`,
`docs/superpowers/plans/2026-10-15-live-stream-robustness.md`,
`docs/audit-evidence/2026-10-14/sonos-resume-probe.md`, and the code named
below at commit 980fb19.

## 1. pqqq: ownership-safe Sonos playback recovery

### The problem in plain words

The board owns the speaker only for a Play it issued itself. Two things must
never be undone by the board's recovery: a Stop pressed by a person (in the
Sonos app or on the speaker), and a switch to another source (the radio,
Spotify, another controller). Today the firmware honors that by never writing
after a loss. A control-read failure goes Playing → Degraded → Recovering →
Playing read-only (`firmware/src/sonos.rs`, `ControlRecovery`: 3 attempts,
1/2/4 s ±20%, 30 s budget). A media loss, that is our listener for the active
intent departing, goes to `fail()` (`sonos.rs:1805-1830`): the queue, owned
URI and group are forgotten, state is Error, and a person presses Play again.
The ticket asks for an automatic resume that is provably ownership-safe.

Since 2026-10-18 the stream itself no longer ends on lag. A consumer that
keeps reading keeps playing through stalls up to 15 s and sustained deficits
(live-radio contract, 24 s live deadline). The board now ends a Sonos listener
only on a stall past 24 s (`StoppedReading`), a capture reset
(`EncodedReset`), a network lease change, or a Stop. Resume matters only for a
consumer that stops requesting, and Sonos is that consumer: after a clean
close it reports end of track and never re-requests a plain URL (103 s on
2026-10-13, 90 s on 2026-10-14).

### What the probe proved and disproved (2026-10-14, Altec, software 97.1-80312)

| Claim | Result |
| --- | --- |
| After a board-side close, `GetTransportInfo` shows `STOPPED` with a status other than `OK` (design A's discriminator) | Disproved. 145 samples of `STOPPED/OK` on our unchanged URI, identical to the `STOPPED/OK` seen 1.2 s after our checked Stop |
| The player re-requests a plain `http://` URL after a close | Disproved (0 re-admissions in 90 s) |
| The player accepts our tagged URL as `x-rincon-mp3radio://` with an `audioBroadcast` DIDL item | Proved: echoed back exactly |
| Under the broadcast scheme the player fetches before it acknowledges Play | Proved: the fetch arrived at 5.0 s and the board's own 5 s Play deadline refused it with 503 (cycle 2) |
| With a 20 s Play deadline the player joins our stream | Proved: `joined ... role=sonos` on the right intent and generation (cycle 3) |
| The player plays our MP3 response under the broadcast scheme | Disproved so far: it reset the connection after 0.1 s and did not retry |
| Behavior after a Stop pressed in the app, under packet loss, or with the board unreachable | Not measured |

Design A was withdrawn because every board-side termination is a clean TCP
close, and this player treats a clean close as end of track. No readable
transport status separates "the board cut the stream" from "a person pressed
Stop". A non-`OK` status is plausible only when the player cannot reach the
board at all, and then the board cannot act either. The contract limit
recorded on 2026-10-13 stands: URI equality cannot infer restart authority.

### Options

**P0. Do nothing automatic; document the manual recovery.** The shipped
behavior: Error, the page shows `last_error`, a person presses Play. A
variant, P0b, adds a Resume button on the board page that runs the ordinary
Play path (`sonos.rs:1960-2080`: fresh reservation, `verify`, `set_uri`,
`media_uri` check, `play`), so the person's click is the ownership proof.

**P1. Design B gated on v6vw: the player retries its own live-broadcast
source.** Hand Sonos the tagged URL as `x-rincon-mp3radio://` with an
`audioBroadcast` DIDL item. A media loss then becomes the player's own
reconnect to its own current source; the firmware writes nothing after a
loss. `Registry::claim` (`core/src/stream_wait.rs:365-392`) already
re-admits the same intent on the same generation and revokes its stale owner,
and `Slot::take` keeps a slot free for it, so the board side is observation
and reporting only.

**P2. Transport-state watcher with one guarded Play.** Retain the candidate
through `fail()` and resume only when all of: the intent is still active and
its command token current; the network lease unchanged; our listener departed
with a transport reason (`StoppedReading`, `Failed(io)`, `EncodedReset`);
same UDN, same coordinator, same host; `GetMediaInfo.CurrentURI` equals our
tagged URI exactly; `GetTransportInfo` is `STOPPED` with a status other than
`OK`. Then one `Play` (never `SetAVTransportURI`), under `run_if_current`,
followed by `PLAYING` on our URI and a re-admission on our intent, else Error.

| | P0 / P0b | P1 (design B) | P2 (watcher) |
| --- | --- | --- | --- |
| Needs from v6vw | nothing | everything: what the broadcast client needs in the response, whether it retries after a close, how often and for how long | nothing from v6vw, but its own probe (below) |
| Evidence that would prove it safe | none needed; the board never writes | one close cycle under broadcast with `pass_b` (re-request, re-admission, `PLAYING`); one app-Stop cycle showing no re-request; one source switch during the retry window showing the player requests the new source, not ours | a player-side failure-stop showing a non-`OK` status on this player; then an app Stop after that failure showing whether status returns to `OK`; then a resume, a no-resume after Stop, and a no-reclaim after a source switch |
| Code touch points | `www` page and `/api/sonos` copy; P0b: one command through `Commands`, reusing the Play path | `sonos.rs` `queue_for_play`/`set_uri` with DIDL metadata; `core/src/sonos_upnp.rs` `set_uri` escaping and the loopback tests; the Play deadline (5 s refuses the player's own fetch; 20 s worked); `owned_uri` and `observe()` must carry the `x-rincon-mp3radio` form so the checked Stop still matches; state reporting Buffering → Playing on re-admission | `sonos.rs` `fail()` and `ControlObservation::finish` (candidate retention, proof sequence, single write); `transport_info` already returns the status; admission unchanged |
| Lines of code, with tests | 10–20; P0b 40–60 | 150–220 | 200–300 |
| If a person pressed Stop | nothing happens | the player stops requesting; the board writes nothing | `STOPPED/OK` → fail closed. Unknown: a Stop pressed after a failure-stop; if the status stays non-`OK`, the watcher would undo the Stop. The check-to-send window is one SOAP round trip |
| If a person switched to the radio | nothing happens | the player requests the radio, not us | `CurrentURI` differs → fail closed, no write |

Notes on P1. It is MP3-only unless shown otherwise. A broadcast item buffers
differently, which changes media-age and skip behavior in ways the client zoo
has not measured for this player. The probe left the speaker's AVTransport URI
pointing at our broadcast URL after cycles 2 and 3 because the harness could
not confirm the owned URI; the firmware's `owned_uri` must be the URI as sent
or the shipped checked Stop refuses. If the player retries against a 503 while
a diagnostic holds the slot, the reservation in section 2 decides the outcome.

Notes on P2. On this player the trigger set is nearly empty: every board-side
close shows `OK`, the board's own Wi-Fi loss is a lease change (terminal by
rule), and a reboot loses the in-memory intent. The only reachable trigger is
a player-side read failure while the board's lease is unchanged, which the
workstation cannot inject (no AP access, gotchas). The false positive that
matters, a Stop after a failure-stop, is exactly the case not measured.

## 2. 7vhq: bounded Sonos listener reservation or priority

### What exists today (correcting the ticket's premise)

The two slots are not first come first served. Commit 48b89d8 (2026-10-11)
added a reservation to `core/src/stream_wait.rs`:

- `Capacity::reserve` (:451-494), called at Play (`sonos.rs:1973`), installs
  one `Intent` with an absolute 60 s pending deadline that later checks never
  extend. If both slots are held by diagnostics it revokes the newest one.
- `Slot::take` (:600-624) refuses a diagnostic whenever an intent exists and
  another diagnostic already holds a slot: one slot stays free for the Sonos
  listener while the intent is pending or active.
- `Registry::claim` (:365-392) admits the tagged request only on exact intent
  id, coordinator IP, network generation and format/buffer route; a reconnect
  revokes its own stale owner. Activation ends the 60 s bound; an active intent
  holds its protection until Stop, cancel, autoplay-off, lease change or
  `fail()` clears it.
- The Play path waits at most 12 s for the revoked diagnostic's slot to free
  (`sonos.rs:1997`). With the close reaper (2026-10-18) the slot is released at
  hand-off, about 2.5 s before the socket's checked close.
- UART: `stream-capacity: reserved sonos_intent=N network_generation=G held=H`,
  then `stream: <peer> joined /stream.mp3 (H of 2) ... owner_id=O
  sonos_intent=N role=sonos|diagnostic` and `socket closed ... released_us=`.
  Sizes: `stream-capacity-layout: slot=16 owner=24 intent=64 registry=120
  mutex_registry=128 arc_allocation=136` (2026-10-11 evidence);
  `stream-output-layout: job=104 mailbox=240 reaper=160` (local UART, 2026-10-19).

Host tests cover it: `pending_reservation_revokes_newest_and_keeps_draining_place_occupied`,
`tagged_wav_wrong_buffer_wrong_peer_and_stale_identity_are_refused`,
`identity_expiry_cancel_and_old_cleanup_cannot_grant_priority` (stream_wait.rs),
`reserve_admit_cancel_and_drop_never_create_a_third_payload_owner`
(`core/tests/listener_runtime_loom.rs`). Device acceptance never passed: the
two 2026-10-11 trials failed on TLS memory and a listener-thread ENOMEM (both
causes since removed by the HTTP/UPnP switch and the permanent output
workers), and contention-01 on the HTTP image lost the older diagnostic to
media age under the exact-audio contract that live semantics replaced. The
gap is proof, not design.

Interaction with live-radio semantics: a Live listener skips instead of
departing, so a slow diagnostic no longer frees its slot by dying; only
revocation, socket death (24 s), or the client leaving frees one. Revocation
keeps precedence over a skip (live-stream plan, skip table), so the evicted
diagnostic still departs `Retired` and the survivor keeps skipping. The
reservation matters more under live semantics, not less.

### Options

**R0. Keep the reservation; run the device acceptance it never got.** No
firmware change. Bounds as above: 60 s pending, 12 s reclaim wait, active
until teardown, two slots, at most two draining sockets.

**R1. Remove the reservation (true first come first served).** Deletes the
`Intent` registry and the eviction. Simpler, and the reaper's early release
already shortens the wait, but two diagnostics then block Sonos until one
leaves and the Play fails at the 12 s reclaim wait.

**R2. Bound the active hold.** Today an active intent whose Sonos listener
has departed keeps one slot free indefinitely (until a clear). Add a second
absolute deadline after the departure, for example the 24 s live deadline
plus the player's measured retry window, after which diagnostics may use
both slots again. Only worth having with P1, since nothing else reconnects.

**R3. Priority eviction at arrival.** Evict a diagnostic when the Sonos
request arrives rather than at reserve time. Already subsumed: `Slot::take`
keeps a slot free while the intent exists, and `claim` revokes the intent's
own stale owner. The remaining variants (evict the oldest instead of the
newest; evict the survivor to protect Sonos bandwidth) have no evidence
behind them: in contention-01 the diagnostic suffered, not Sonos.

**R4. A third slot, reserved for a `sonos_intent` join.** Changes the public
two-listener contract (spec Outputs, the live-stream contract "does not add
listeners beyond two", and 7vhq's own "do not silently add a third
listener"), so it is Doctor Biz's call, not an implementation detail.

| | R0 | R1 | R2 | R4 third slot |
| --- | --- | --- | --- | --- |
| Lines of code, with tests | 0 firmware; 50–100 harness | −150 to −200 | 40–80 | 100–160 (`MAX_LISTENERS`, `owners: [Owner; 3]`, a third output worker, mailbox, `DRAINING_LIMIT`, UART `of 3`, harness parsers) |
| Memory | unchanged | −136 B registry allocation | unchanged | +4 KiB internal stack for a permanent third worker; +24 B owner, +104 B job; per socket a 46,080 B lwIP send buffer plus Wi-Fi TX use (Task-15 measurements: 17–25 KB internal per listener at a 5,760 B send buffer, 41–55 KB for two; the current profile is unmeasured and the soak's largest internal block sat near 45 KB); MP3 frame copy 960 B, FLAC 24,591 B in PSRAM; WAV 3,360 B internal |
| Timing bounds | 60 s pending, 12 s reclaim, hand-off release ~2.5 s before close | Play fails after 12 s when both slots are busy | one extra absolute deadline, checked in `refresh()` like the pending one | encoder CPU is shared (18–19% of one core at 0/1/2 FLAC listeners); a third listener adds copies and sends, unmeasured |
| Diagnostic when Sonos arrives | newest revoked at reserve; survivor skips, never departs | nothing; Sonos waits or fails | as R0, then free again after the bound | none evicted while a slot is free; eviction rule still needed when all three are held |
| Live listener semantics | revocation beats skip; survivor unaffected | no revocation | as R0 | as R0 |
| Exact test that proves the bound | device: two ffmpeg diagnostics on `/stream.mp3`, then a board Play; pass when the UART shows the newer owner's `released_us` within 12 s of `stream-capacity: reserved`, `joined ... role=sonos` with `held` ≤ 2, the older diagnostic still receiving with zero departures (skips allowed), Sonos `PLAYING` for 15 s; then Sonos reconnect, cancel during the pending window, and a failed start, each with `held` ≤ 2 on every line | the same trial, expecting the Play failure line at 12 s | host test on `Capacity` with the injected `now` (as `reserve` already takes): hold expires at the deadline, never earlier, never renewed by checks | device: three listeners held, a fourth refused 503, internal heap minima and largest block recorded across 30 churn cycles (gcq8's harness) with a stated tolerance |

`scripts/sonos-http-check` already drives Play and checked Stop and polls
the speaker to remote `STOPPED`; the R0 trial is a contention mode on it
plus the UART parsers in `scripts/network_fault.py`. Every Sonos trial
preconditions on a `STOPPED`, ungrouped coordinator and treats a refusal as
evidence (gotchas, 2026-10-14).

## 3. Recommendation

If Sonos is re-prioritized:

- **pqqq: P1 (design B), after v6vw.** It is ownership-safe by construction
  because the board writes nothing after a loss, and its one unknown is
  bounded to v6vw's single probe cycle on the current image (the response
  head changed on 2026-10-18 and was never retried under broadcast). P2 is
  not worth building on this player: its trigger never fires for a
  board-side close, and the one case that could hurt a person (Stop after a
  failure-stop) is the one nobody can inject from here. Ship P0b meanwhile
  only if a person asks for a button; it is the same Play path.
- **7vhq: R0.** The design is in and host-tested; what is missing is the
  device trial that two earlier blockers prevented. Take R2 only alongside
  P1. R4 is a contract change with unmeasured internal-RAM cost; do not take
  it to solve a problem R0 already covers.

Today, with Sonos out of focus:

- Keep **pqqq open**, with this document as its plan and v6vw as its only
  next step. Its acceptance still names the real-device resume and no-reclaim
  tests, which no other ticket holds; closing it would lose the ownership
  rules in section 1 as a gate.
- Keep **7vhq open**, with this document as its plan and the R0 trial as its
  only next step. It is not superseded by pqqq: one ticket is about writes to
  the speaker, the other about slots on the board, and 7vhq's gap closes
  without pqqq moving. Closing either as superseded by the other would
  misdescribe both.
- Neither needs code now. Post the doc path on both tickets.

### What this document could not determine from the repo

- The player's retry policy under the broadcast scheme (count, interval, give-up).
- Whether a Stop pressed after a player-side failure returns the status to `OK`.
- The current per-connection internal RAM at the 46,080 B send buffer; the
  only measurements are from the Task-15 profile.
- Whether `GetMediaInfo.CurrentURI` under broadcast matches a form the shipped
  checked Stop will accept without a firmware change.
