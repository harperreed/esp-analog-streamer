# Sonos HTTP control implementation plan

Doctor Biz explicitly selected the proven HTTP/UPnP playback approach on October
11: "ok. let's skip tls for now." This supersedes the Sonos TLS allocator
experiment. It authorizes replacing the production control transport, preserving
the existing standalone board controls, formats and buffering.

## Design and constraints

Reuse the selected-player discovery and existing command worker, NVS settings,
command cancellation, network leases and two-slot listener reservation. Replace
the production secure WebSocket/session commands with the AVTransport HTTP
commands exercised by the retained successful Sonos FLAC test. Audio still comes
directly from the board; no workstation controller or relay is required.

Use one bounded HTTP request at a time, an absolute request deadline, capped
responses, parsed XML and verified device/coordinator identity. Endpoints must
remain on the discovered numeric peer; no redirects or arbitrary destinations.
Reuse the existing media URL constructor so format, buffer, network generation
and playback intent have one source of truth. No volume writes.

Manual Play loads the current configured stream and starts it. Poll actual
transport and current URI for status. Doctor Biz authorized checked Stop and
real board playback on October 11: "proceed with board tests. you can play
sonos, etc". Use fresh coordinator and exact URI checks followed by SOAP Stop;
the observation-to-command source-switch race remains explicit.
Relinquish local stream capacity on every terminal path. Preserve command
cancellation at final media sends.

HTTP AVTransport does not offer the old session-scoped command authority.
Observation followed by Play or Stop has a race with another controller. Do not
describe these observations as atomic ownership. Do not automatically reload or
send Play after a control failure. Ownership-safe automatic recovery remains an
open requirement, not a completed feature or silently relaxed Kata criterion.
Existing explicit autoplay policy remains; source changes relinquish its intent.

Keep any retained TLS adversarial fixture diagnostic-only; no runtime transport
toggle or automatic TLS fallback. Do not change TLS policy for recognition or
MQTT. MP3/FLAC and Low 1s / Medium 4s / High 8s settings remain available and
saved format/buffer changes apply at the next Stop-to-Play boundary.

## Execution

Use clean-context Sol 6.1 subagents on the existing WIP branch. Parent owns
integration, hardware, canonical checks, commits and Kata updates. The explicit
transport choice supplies authorization; do not request approval again solely
because a skill normally stages design and plan approval. Review the concrete
implementation and report the changed ownership limits.

- [x] Add bounded, host-tested HTTP/SOAP protocol and response handling, using
      actual retained device XML as protocol evidence. Tests precede production
      code; cover malformed/oversized responses, faults, endpoint identity,
      deadlines, URI escaping and correct MP3/FLAC media URLs.
- [x] Prepare the HTTP transport in the existing firmware worker with tests
      for settings, cancellation, reservation lifecycle, takeover observations
      and terminal cleanup. Preserve public API/UI contracts.
- [x] Independent code review, focused fixes and canonical `scripts/check`.
- [x] Run actual board Play/Stop for both formats, settings transitions and
      listener contention. Save exact image/boot, status, URI, payload and
      cleanup evidence. Do not count Sonos PLAYING as sample-exact continuity.
- [x] Restore stopped MP3/Medium, autoplay/recognition off, no source process;
      commit verified work and update existing Katas against original criteria.

## State

Doctor Biz approved checked Stop and real playback. Candidate `87848e3` remains
installed with ELF SHA-256
`36d61e57439e102cc9b3c5b256ffeec8a7dddec9743a672b670a4aadce8fa2c3`.
Actual FLAC/Medium and MP3/High playback, deferred settings and remote Stop passed
in playback-03. The failed baseline and premature-Stop gate receipts remain.
The gate now re-discovers after UART reset and waits for actual remote STOPPED;
16 focused tests and all 27 canonical stages pass (`run.HvfuxuB4`).

Contention-01 failed: the newer debug listener checked-closed before Sonos joined,
but the older debug stream later exceeded its media-age budget after a long
socket wait. The two-slot ceiling held; this does not satisfy coexistence or
recovery. Active capture fault deltas were zero, with startup faults retained.
Final checks confirmed stopped MP3/Medium, autoplay/recognition off, same candidate
and no source player. No volume/mixer change or firmware rollback occurred.

See `docs/audit-evidence/2026-10-11/sonos-http-board.md` and its JSON for exact
boots, receipts, checks and limits. Keep 7vhq/pqqq open. The next hardware work
must address the failed coexistence gate and establish actual-path network fault
injection; neither another unchanged trial nor a larger age budget diagnoses the
observed socket delay. Controlled recovery and sample-accounted soaks remain.
