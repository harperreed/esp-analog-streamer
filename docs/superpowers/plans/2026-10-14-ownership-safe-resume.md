# Ownership-safe Sonos resume over HTTP/UPnP (pqqq)

## Now
- Step: Task 1, the probe. Host side (status parser + driver, TDD) and
  firmware side (two temporary patches, built, not flashed) delegated to
  clean-context subagents; parent owns flash, run, restore, evidence
- Next: review both, flash the probe image, run cycles 1 and 2 (and the
  app-Stop cycle if Doctor Biz presses it), restore the ordinary image,
  evidence pair, then pick the design with proof
- Open: whether Doctor Biz will press Stop in the Sonos app for one cycle
- Approved: "pqqq: ownership-safe Sonos resume (Recommended)"
  (2026-10-14, build-target question); "Yes, run the probe (Recommended)"
  (2026-10-14, probe-first question). Decision 1 (fail closed on `OK`)
  stands unless the probe overturns it.
- Compactions: 0

## Where three weeks of work left this

Sonos control is HTTP/UPnP SOAP; TLS and the cloud-queue session protocol
were abandoned on 2026-10-11, so `joinSession`, `sessionStatus` and
`loadCloudQueue` are moot. The firmware hands Sonos our stream URL directly
with `SetAVTransportURI` (empty metadata), so Sonos never calls our queue
routes; there is no inbound attachment signal. After our stream closes,
Sonos reports STOPPED about 6 s later and never retries the URL (103 s
observed, one trial). The recorded contract limit: "HTTP recovery cannot
infer restart authority from matching URI alone. An owner can reuse a URI,
and a Stop can leave it unchanged." Everything the firmware can read today
is bundled in `observe()`: same device UDN, same group coordinator at the
same host, `GetMediaInfo.CurrentURI` equal to our tagged URI, and
`GetTransportInfo.CurrentTransportState`. There is no GENA subscription
code. The read-only `ControlRecovery` (Degraded → Recovering → Playing on a
control-read failure, 3 attempts, 1/2/4 s ±20%, 30 s) is shipped and
proven on a control-thread stall; it never writes.

## The one unread field

`GetTransportInfo` also returns `CurrentTransportStatus` (schema
`target/kata-hardening/oct13-next/recovery-scpd/AVTransport1.xml`,
`TransportStatus`, UPnP values `OK` / `ERROR_OCCURRED`). The firmware
parses only `CurrentTransportState` (`core/src/sonos_upnp.rs:526-536`), and
no retained observation recorded the status. Sonos also keeps
`TransportErrorDescription`, `TransportErrorURI`, `TransportErrorHttpCode`
and `TransportErrorHttpHeaders`, but those are delivered only inside the
evented `LastChange`, so they need a GENA subscription and an HTTP callback
on the board; parked.

The hypothesis this design rests on, to be measured, not assumed: after
our stream dies under Sonos, the player reports `STOPPED` with
`CurrentTransportStatus` other than `OK` on our unchanged URI; after a Stop
from the Sonos app or from our checked Stop, it reports `STOPPED` with
`OK`. A community-captured Sonos event for a failed stream shows the vendor
value `ERROR_CANT_REACH_SERVER` with `TransportErrorDescription` and
`TransportErrorURI` set (the UPnP spec allows vendor extensions of
`TransportStatus`), so the status field is written on failure; whether our
player writes it on our failure, and leaves `OK` on a Stop, is the probe.
If that holds, the ambiguity the contract limit names ("a Stop can leave
the URI unchanged") has a readable discriminator. If it does not hold,
design A has no proof and the product stays as it is: Degraded → Error →
manual Play.

## Design B: make Sonos retry its own source

Sonos re-requests a stream by itself only when it classifies the item as a
live broadcast ("treats that stream like a radio station and reconnects
when it ends"; serving 404 ends the attempts; community evidence from
music-assistant, not Sonos documentation). Track-type items are never
re-requested, and the one observation on record (no reconnect in 103 s
after a clean close) says ours is a track today: a plain `http://` URL with
empty metadata. If the firmware hands Sonos the same tagged URL as a
broadcast (`x-rincon-mp3radio://host:8000/stream.mp3?…` and a DIDL
`object.item.audioItem.audioBroadcast` item), a media failure becomes the
player's own reconnect to its own current source. That is ownership-safe
by construction: a player only retries what it is already playing, so a
takeover means it is no longer requesting our URL, and a user Stop means
the same. The firmware writes nothing after a loss. `admit_request`
already re-admits the same intent on the same network generation, and
`claim` evicts the stale slot, so the board side needs no change beyond
observing the re-admission and reporting Playing.

Unknowns for B, all probe questions: whether Sonos accepts our MP3 stream
under that scheme (FLAC is unlikely to, so B is MP3-only unless shown
otherwise); how many times and how fast it retries and whether a refused
or failed retry ends it; how its buffering changes (broadcast items buffer
longer, which may help continuity and will change media-age behavior);
whether `GetMediaInfo.CurrentURI` still equals our tagged URL so the
shipped checked Stop keeps working; and what the app shows.

A and B are not exclusive: B handles the common failure with no write, and
A's status discriminator remains the only honest answer for "STOPPED on our
URI" when B does not fire. The probe below measures both in one session.

## Ownership proof, in order, all read-only

Within one boot an intent id is unique per Play, so URI equality against
this boot's tagged URI is sound; cross-boot repetition (gotchas) is a
harness problem, and the candidate below is boot-scoped anyway.

1. The command token is still current and the network lease unchanged (any
   Stop, Select or autoplay-off since the loss revokes everything).
2. `verify`: same UDN, our player still coordinator of the same group at
   the same host.
3. `GetMediaInfo.CurrentURI` equals our tagged URI exactly. Any other value
   is takeover: terminal, never a write.
4. `GetTransportInfo`:
   - `PLAYING` on our URI: Sonos is delivering (it reconnected itself);
     state Playing, no action.
   - `TRANSITIONING`: wait within the budget.
   - `STOPPED` with `ERROR_OCCURRED`: the failure-stop. Eligible for one
     guarded `Play` (not `SetAVTransportURI`; the URI is still ours).
   - `STOPPED` or `PAUSED_PLAYBACK` with `OK`: a controller stopped it.
     Terminal, fail closed, manual Play required. **Decision 1.**
5. Immediately before the single `Play`: repeat 1–4 under `run_if_current`.
   The check-to-send window is a non-atomic race and is documented as such;
   it is the same race the shipped checked Stop already accepts.
6. After `Play`: the existing poll must observe `PLAYING` on our URI within
   the remaining budget, and our listener admission must see the tagged
   request re-admitted (`admit_request`, same intent, same generation).
   Otherwise Error, candidate cleared, manual Play.

## State machine delta

Only the media-loss entry and the one write change; everything else is the
shipped `ControlRecovery`.

- Eligibility: state Playing or Buffering with an active intent when our
  listener for that intent departs with a transport reason (`MediaAge`,
  `StoppedReading`, `Failed(io)`, `FellBehind`); not `Retired`,
  `NetworkChanged` or any cancellation. `fail()` stops wiping the candidate
  for this one path; it still wipes on every other.
- Degraded: candidate retained (owned URI, group, intent, token, lease,
  format, buffer, departure reason and device time).
- Recovering: proof attempts 1–4 with the shipped budget (3 attempts,
  1/2/4 s ±20%, 30 s). Step 5–6 may run once per candidate.
- Playing: proof saw PLAYING on our URI, or `Play` was followed by PLAYING
  and re-admission.
- Error: exhaustion, `OK` status, foreign URI, group change, lease change,
  cancellation. The reconnect `Retry` stays observe-only.

Autoplay sampling stays frozen during recovery; a resume is not a new
autoplay intent. No volume, grouping or source writes anywhere.

## Tasks

1. **Probe** (bounded, no production behavior change). Extend
   `transport_state` to return status as well, loopback-tested with the
   existing `serve_replies` server. A read-only driver over
   `scripts/sonos-http-check`'s helpers records state, status,
   `CurrentURI` and our listener admissions every second through, per
   cycle: a real Play, a self-inflicted listener close (the
   passive-reconnect shape), 90 s of observation, then a checked Stop,
   then 60 s. Cycle 1: today's plain URL (design A's question). Cycle 2:
   the same tagged URL as `x-rincon-mp3radio://` with broadcast DIDL, sent
   by a temporary, archived-and-removed patch to the Play path (design B's
   question). One more cycle with a Stop from the Sonos app if Doctor Biz
   can press it. Pass for A: the failure-stop shows a status other than
   `OK` and both Stops show `OK`, on our unchanged URI. Pass for B: Sonos
   re-requests the tagged URL after the close, is re-admitted on the same
   intent, and reports PLAYING again, with the retry count and timing
   recorded. Either, both or neither may pass; the evidence says which,
   and the design that follows is the one with proof. **Decision 2.**
2. Core state machine: candidate retention, proof sequence, single guarded
   Play, re-admission check. TDD in `firmware/src/sonos.rs` tests (host via
   `core/tests/sonos_adapter.rs`) and `core/src/sonos_upnp.rs` loopback
   tests covering: stale replies, partial replies, status `OK` after
   failure, foreign URI between proof and send, cancellation during
   backoff, lease change, exhaustion, a Play that never reaches PLAYING,
   re-admission by a different intent.
3. Independent review, `scripts/check`.
4. Real Sonos acceptance: resume after a self-inflicted close; no resume
   after app Stop; no reclaim after another source is selected while
   disconnected; cancel during backoff. The 0.5/1/2.5/4 s interruption
   matrix on the Sonos path remains unreachable from this workstation
   (no AP access) and is recorded as the gap it is.
5. Evidence pair, kata comment, close-or-block.

## Limits

This is a measured discriminator, not a Sonos contract; firmware updates
can change it, so the status check must stay fail-closed and the evidence
must say which Sonos software version was observed. Takeover between the
last check and the `Play` send is a window of one SOAP round trip. No
GENA, so `TransportError*` detail is not available. The AP-side
interruption matrix stays open.
