# Proposed Sonos FLAC adjustment

Status: approved by Doctor Biz in chat ("perfect"); implemented, with direct-board playback and audible confirmation verified. Real autoplay and eviction checks passed; remaining capture-setup risks are recorded in the implementation plan.

## Evidence

The authorized Altec + 3 group played host-generated live 16-bit/48 kHz native
FLAC for 120 seconds through `loadCloudQueue`, with 120451 ms of playhead
advancement. The queue and audio stayed on this workstation's LAN HTTP server;
the audio response had no Content-Length and the queue omitted duration.
Cleanup suspended only the test's session. Group volume stayed at 5%.
Doctor Biz later confirmed audible host playback at 10%; a five-minute host
run advanced 300384 ms without restarting. Direct ESP32 playback subsequently passed 121 seconds and Doctor Biz confirmed
the board audio sounds great after raising the physical test source level. The
latest authorized listening volume is 15%.

The same live format restarted through `loadStreamUrl`. Finite 16-bit files
also passed through the track path at both 44.1 and 48 kHz; their radio tests
failed. Tested 24-bit/48 kHz files interrupted on both paths. This is evidence
for this group and these encodings, not a universal claim about Sonos FLAC.

## Recommended change

- Keep 24-bit/48 kHz stereo capture and the existing activity meter. Produce
  16-bit/48 kHz native FLAC from the one shared encoder, reducing each captured
  sample to 16 bits before encoding. This discards eight bits of source precision;
  FLAC remains lossless with respect to the resulting 16-bit samples.
- Use Sonos's track/queue API instead of its radio API. Serve the small queue
  responses from the board's existing port-80 web server. Its sole current item
  points to the existing `/stream.flac` endpoint on port 8000, with no advertised
  duration. No workstation or Internet service is needed in the product.
- Keep the approved selection, verified TLS, session ownership, manual controls
  and opt-in autoplay behavior. Queue handlers read a bounded snapshot and never
  wait for Sonos. Stop still suspends only our owned session.
- Do not add MP3, a second encoder, automatic format fallback or a quality menu
  in this increment. The FLAC output precision changes for every FLAC listener.

## Verification before acceptance

Test the 24-to-16-bit conversion and FLAC output with independent decoders,
including negative/full-scale samples and both channels. Cover actual queue
request/response parsing and stale item IDs. Then verify direct board-to-Sonos
playback for two minutes, quiet audible playback, Stop, eviction, autoplay and
heap/stack margins. Host success alone does not satisfy these checks.

The alternative is to retain 24-bit output and continue investigating a compatible
Sonos path. Existing radio-path controls are not accepted as working playback.
