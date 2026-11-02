# Local Sonos playback for the analog streamer

Status: design approved by Doctor Biz; live FLAC compatibility probe pending.

## Outcome

Doctor Biz wants the analog source to play through a chosen Sonos destination,
configured from the board's web app, eventually starting when audio appears.
The device must work without Home Assistant or a workstation controller.
The first useful test is real music from the workstation's analog jack through
the ESP32 and into the existing Altec + 3 group. Doctor Biz authorized that group
and requested quiet playback. Test group volume must be at most 5%, retaining a
lower existing setting; do not raise volume to troubleshoot silence.

## Evidence and open questions

The primary project reference is `docs-ignore/victrola-hacks.tgz`, especially
`sonos-integration.md` and `live-stream-discovery.md`. It establishes local
SmartSpeaker SSDP discovery, secure WebSocket control, owned playback sessions,
and an independent MP3 source proof. It does not establish live FLAC playback.
Victrola's FLAC endpoint is Ogg-wrapped; our endpoint is native FLAC.

A read-only discovery here found one group, Altec + 3, with four advertisements
and one leader. Private results are in `target/sonos/discovery.json`.
Current Sonos documentation lists FLAC but its file-format guidance does not
prove acceptance of our continuous 24-bit HTTP stream. Test the actual stream.
The previous two-listener 120-second FLAC retry passed, but earlier intermittent
network failures remain unexplained. A Sonos test is not a network fix.

## Approach

Use the local WebSocket/session API demonstrated in the research. This matches
the intended standalone product and supplies session ownership and playback
error events. A workstation-only controller is useful for the first format
probe but is not the final architecture. Cloud control would introduce account
and service dependencies; UPnP is a different control path not established by
this project's reference proof. Neither is an automatic fallback.

1. Run a bounded, throwaway host control probe against the authorized group.
   Sonos must fetch `http://<board-IP>:8000/stream.flac` directly. No relay or
   transcoding. Record command results, playback events, board connections and
   audible confirmation separately. Release only the session created by the
   probe. Stop on playback errors. If native FLAC is rejected, report the actual
   error and decide the codec/container change before building that into firmware.
2. Add manual Sonos controls to the board: Discover, destination selection,
   Play, Stop, and readable connection/playback/error status. Persist a stable
   destination identity; resolve the current group and leader at each start.
3. Add opt-in audio-triggered playback after manual playback is verified.
   Reuse the same controller and state machine; do not create a second path.

The existing FLAC URL and port remain the test contract for this increment.
Port consolidation, browser audio playback, MP3 encoding and WAV removal are
separate changes, not prerequisites for Sonos control.

## Firmware boundaries

A single Sonos worker owns discovery, the control socket and session state.
The capture and encoder tasks never wait for Sonos or perform network calls.
Web handlers validate small bounded inputs and enqueue commands; they do not
run discovery or wait for WebSocket responses. Status reads take short locks.

Keep protocol parsing and state transitions in the host-testable core. Keep
ESP networking, TLS and NVS adapters in firmware. Use the pinned ESP-IDF stack
and its supported WebSocket client component rather than hand-writing TLS or
WebSocket framing. Adding the component must not upgrade the existing IDF pins.

Discovery accepts the SmartSpeaker service, deduplicates followers, and uses
only group leaders. Bound datagram size, group count, field lengths and expiry.
Constrain the advertised control endpoint to the responding peer and expected
local WSS port/path. Do not accept arbitrary browser-supplied stream URLs;
the firmware constructs its own stream URL from its current LAN address.

Use one control connection, bounded messages and a bounded command queue.
Correlate replies by command ID; unsolicited events are independent. Reject
oversized or malformed messages, expire pending commands, and disconnect on
protocol errors. Reconnect with bounded backoff without repeatedly taking over
playback. Every connection/session path must have a clear cleanup owner.

## TLS and identity

Verify the speaker certificate chain using public Sonos trust roots with recorded
provenance. Do not use Victrola's embedded API key or private material. Use this
project's own session/app identity and the public local-client identifier proven
by the reference research, subject to verification on the actual target.

LAN IP addresses do not match Sonos device certificate names. The proposed
exception is limited to this Sonos connection: retain chain verification but
skip ordinary IP hostname matching, then correlate device/group identity with
discovery and Sonos info. Do not disable TLS verification globally. Legacy
certificate compatibility, if required, must retain chain verification and be
recorded explicitly. TLS policy is an architectural/security review item.

## Manual and automatic behavior

Selecting a destination does not start playback. Play creates our session and
loads the board URL. Display connecting, buffering, playing, stopped and error
states from observed results. Stop releases/suspends only our owned session;
it must never send an unconditional group pause after another source takes over.
Do not alter group membership. The initial product controls preserve volume;
the quiet volume cap applies to the authorized live test.

Autoplay defaults off. Proposed audio detection: signal above the configured
threshold for 500 ms starts once; sustained quiet below a lower threshold for
10 seconds ends our session. Compute activity from both channels' energy so
opposite-phase stereo cannot cancel. Exact defaults must be checked against
real line-in noise and music levels. Keep timing/state logic separate from
Sonos I/O and cover transitions with deterministic tests.

After boot, manual Stop, session eviction or another source taking over, require
quiet followed by new audio before automatic playback can start again. Do not
fight the Sonos app. A disconnect does not authorize a fresh takeover; reconcile
ownership and group topology before acting. Store only the selected stable
identity and explicit autoplay setting in NVS, not transient session/group IDs.

## Acceptance

- Unit tests cover discovery/parser bounds, fragmented messages, correlation,
  timeout/eviction handling, and audio-trigger state transitions.
- Integration tests exercise the real protocol/adapter boundaries; canned events
  may test parsing, but mocked playback is not end-to-end evidence.
- Real Sonos playback fetches the board's FLAC URL, reports our item playing,
  sustains at least two minutes, and is audibly confirmed at the quiet setting.
- Verify Stop, another controller taking ownership, unavailable destination,
  malformed/oversized data and reconnect behavior without capture stalls.
- Compare free/largest internal heap, PSRAM and stack margins before/after the
  controller connects and during streaming. Set measured memory acceptance
  before enabling autoplay; no unbounded accumulation or leaked listener slots.
- Run `scripts/check`, firmware build and real browser controls against the board.

## Sources

- Project archive: `docs-ignore/victrola-hacks.tgz`.
- Sonos stream command: https://docs.sonos.com/reference/playbacksession-loadstreamurl-sessionid
- Sonos format guidance: https://docs.sonos.com/docs/supported-audio-formats
- Local protocol reference pinned by project research:
  https://github.com/music-assistant/aiosonos/tree/89615a67013c5504cac9289c1efa9a95635d4eb4
