# Sonos resume probe: transport status and self-retry (pqqq, 2026-10-14)

One real Sonos player (Altec, software 97.1-80312), one board on a probe
image, read-only SOAP observation every second. Cycle 1 overturns design
A: after the board closed the stream, the player reported `STOPPED` with
`CurrentTransportStatus = OK` on our unchanged URI for the whole 90 s
window, which is exactly what our checked Stop produced 1.2 s after it was
sent. No readable status distinguishes a stream the board ended from a
Stop, because the firmware ends every listener with a clean TCP close and
the player treats that as end of track. The player made no attempt to
re-request the URL. Cycle 2 (design B, the same URL presented as a live
broadcast) is reported below. This does not establish behavior under an
unreachable board, packet loss or a Stop pressed in the Sonos app; hashes,
per-sample rows and the retained refused attempt are in
`sonos-resume-probe.json`.

## Method

Probe image `f34fce16…` = commit `0610fc0` + `close.patch` (the archived
2026-10-13 passive close, re-applied: the first Sonos-owned listener is
ended through the ordinary checked-close path 10 s after its first
payload, once per boot, receipt `passive-media-close: phase=1`) +
`broadcast.patch` (Play with `?broadcast=1` hands Sonos the same tagged URL
as `x-rincon-mp3radio://` with an `audioBroadcast` DIDL item, never saved;
the `set_uri_with_metadata` hunk was re-applied by hand after
`transport_info` moved its context; the exact applied tree diff is
`probe-tree-applied.diff`). `scripts/sonos-resume-probe` (commit
`0610fc0`): fresh boot over the serial port, attestation of image and boot,
settings gate (MP3, autoplay and recognition off), precondition that the
speaker is `STOPPED` and the coordinator of its own group, then Play via
the board, the image's own close, 90 s of observation, a checked Stop
polled to remote `STOPPED`, 60 s more. Samples: board `/api/sonos`, SOAP
`GetTransportInfo` (state and status), `GetMediaInfo`, `GetPositionInfo`,
and the UART listener export. The transport status field had never been
parsed before (`core/src/sonos_upnp.rs::transport_info`).

The first attempt (`cycle1-plain`) refused at the precondition: the
speaker was `PLAYING` a foreign source with status `OK` before any board
Play, grouped with a second player. Doctor Biz chose to wait. Retained.

## Cycle 1: plain URL

Boot `699551b6…` superseded by the retry's boot; intent 1; owned URI
`http://192.168.23.217:8000/stream.mp3?buffer=med&network_generation=2&sonos_intent=1`.
Sequence on the device clock: `STOPPED/OK` foreign (5 samples, before
Play) → `TRANSITIONING/OK` ours (2) → `PLAYING/OK` ours (12) → listener
departed `diagnostic passive media close` at 27.93 s, checked close 0.43 s
later → `STOPPED/OK` ours (145 samples through the checked Stop and
beyond). Status never left `OK`. `CurrentURI` stayed ours throughout.
Re-admissions after the close: 0 in 90 s. Checked Stop: remote `STOPPED/OK`
1.21 s after the request, URI unchanged. Pass A: false. Pass B: not
applicable (plain scheme).

What this means for design A: the firmware's own terminations (media age,
stopped reading, socket error on the board side) all end in a clean close,
so on this player and version they are indistinguishable from a Stop by
status. A status other than `OK` is plausible only when the player cannot
reach the board at all, and in that case the board cannot act either.
Design A is withdrawn.

## Cycle 2: broadcast URL

Fresh boot, intent 1, `?broadcast=1`. UART: `probe-broadcast: enabled=true
uri=x-rincon-mp3radio://192.168.23.217:8000/stream.mp3?buffer=med&network_generation=2&sonos_intent=1`
at 17.86 s, `echoed uri=` identical at 19.52 s (the player accepted the
scheme and the DIDL and reported the URI back exactly), then at 24.52 s,
in the same log instant:
`stream: <player> "GET /stream.mp3?buffer=med&network_generation=2&sonos_intent=1 HTTP/1.1": HTTP/1.1 503 Service Unavailable`
and `sonos: state=Error`. The board's `/api/sonos` `last_error` was
"No more processes (os error 11)", the `Play` SOAP read timing out. The
player's transport stayed `STOPPED/OK` on the broadcast URI throughout;
the driver failed closed after 30 s and its cleanup refused the Stop for
want of a confirmed owned URI (speaker left `STOPPED` with our URI set).

Reading: under the broadcast scheme the player connects to the stream
**before** it acknowledges `Play`. It requested our tagged URL, plain
`http`, tags intact, 5.0 s after the echo, which is exactly the board's
`Play` deadline; the deadline expired first, the intent guard cleared the
reservation, and the arriving request was refused 503. The obstacle is the
board's own timeout, not the player. Design B's actual question, whether
the player re-requests after the self-inflicted close, was not reached.
Pass A: not applicable. Pass B: not reached. A third cycle with a longer
`Play` deadline under broadcast followed.

## Cycle 3: broadcast URL, 20 s Play deadline

Image `bbe9aeeb…` = the cycle-2 tree plus one probe-only change: under
broadcast the `Play` call waits 20 s instead of `TIMEOUT` (5 s); diff
`probe-tree-applied-cycle3.diff`. Fresh boot, intent 1. UART: broadcast
URI sent at 17.41 s, echoed identically at 18.67 s, `sonos: state=Buffering`
at 26.28 s (the `Play` was acknowledged this time), and the player
**joined our stream**: `stream: <player> joined /stream.mp3 (1 of 2)
owner_id=2 sonos_intent=1 role=sonos` at device 25.03 s. It then `left
after 0.1 s: socket error kind=ConnectionReset`, checked close 0.75 ms
later, and the transport stayed `STOPPED/OK` on the broadcast URI for the
rest of the window; the board's poll saw `STOPPED` and went `Stopped` at
36.3 s. No second request from the player in 90 s. The driver failed
closed after 30 s; cleanup again refused the Stop for want of a confirmed
owned URI.

Reading: the deadline obstacle is cleared and the tagged admission works
under the broadcast scheme (same intent, same generation, `role=sonos`).
The player's broadcast client connects, reads the first bytes of our MP3
response and resets the connection. What it expects in that response
(ICY-style status line or headers, a content type, no startup history, or
something else) is not established here, and without playback the
self-retry behavior that design B depends on was not reached. Pass B:
false (joined, then reset). This is the probe's budget: one cycle per
scheme plus one for the board's own timeout.

## Restoration

After each probe cycle the patches were removed with per-path checkouts
and the ordinary firmware was rebuilt from `0610fc0` and flashed; the board
attested `4b6c4d69…` after the first restore and the same after the last.
The pre-probe board image `0ff2a7cc…` predates `transport_info`, so the
rebuilt ordinary image legitimately differs from it; `4b6c4d69…` is the
ordinary image for `0610fc0`. Settings unchanged (MP3/Medium, autoplay and
recognition off), listeners idle, Sonos `STOPPED` and ungrouped at the
end. The speaker's AVTransport URI was left pointing at our broadcast URL
after cycles 2 and 3 because the driver refused an unconfirmed Stop; that
is a stopped, idle player, not an owned session.

## Limits

One player, one software version, quiet LINE IN, MP3 only. The close is
the board's clean close, not packet loss or an unreachable board. No Stop
from the Sonos app was pressed. One cycle per scheme.
