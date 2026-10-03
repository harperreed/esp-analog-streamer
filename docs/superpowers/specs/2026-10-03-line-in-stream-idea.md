# Idea: stream LINE IN while it's being identified

Harper (2026-10-03): "can we set up a stream out of the line in WHILE the shazaming is happening. so its effectively a ANALOG -> STREAM with a metadata sidecar?"

Status: idea, not approved or designed. Pick it up after the current plan (Tasks 8–12) lands; Tasks 9–11 build the sidecar this needs anyway.

## What already exists after the current plan

- **Metadata sidecar:**
  - `GET /api/state`: now playing, history, state, as JSON.
  - MQTT `esp-shazam/now_playing`: retained, published on every change.
- **Audio:** LINE IN captured over I2S, today at 16 kHz mono, which is all the fingerprint needs.

## Recommended v1 (my proposal, needs Harper's call)

- **Capture at 48 kHz stereo** instead of 16 kHz mono.
  - One copy goes to the stream.
  - One copy goes through a low-pass filter, a 3:1 decimation and a mono mix down to 16 kHz for the existing fingerprint ring. That's cheap: about 16,000 outputs/s × ~32 filter taps.
  - The fingerprint code doesn't change.
- **Serve the stream as an endless WAV over HTTP on its own port,** for example `http://esp-shazam.local:8000/stream.wav`.
  - It's uncompressed 16-bit PCM, so the board does no encoding work. The fingerprint already takes ~10.5 s of CPU every ~20 s.
  - 48 kHz stereo is 1.5 Mbit/s. That should sit well under the ESP32's Wi-Fi throughput, but verify it under load alongside the HTTPS calls to Shazam.
  - Its own port and thread (a plain `std::net::TcpListener`), because a never-ending response would tie up the web server that answers `/api/state` **(verify how esp_http_server schedules long handlers)**.
  - VLC, ffplay/ffmpeg and Music Assistant can all play an HTTP WAV URL.
- **The metadata sidecar stays out of band:** `/api/state` plus MQTT. Optionally add a Server-Sent Events endpoint (`/api/events`) that pushes each song change the moment it happens.

Rough size: 300–400 lines.
- Capture rework and the decimator, with host tests against a known tone: ~150.
- The stream server: ~100.
- Docs and a device test: ~100.

## Decisions for Harper

1. **Quality:**
   - 48 kHz stereo (CD-like, 1.5 Mbit/s)
   - or 16 kHz mono (what the fingerprint already gets, 256 kbit/s, telephone-ish)
2. **Who listens:**
   - players on the LAN pulling the URL directly (simplest)
   - or the board pushing to an Icecast server, which fans out to many listeners and can put titles into the stream itself (ICY metadata)
3. **In-band metadata:**
   - Sidecar only (`/api/state` and MQTT).
   - Or also ICY titles inside the stream.
     - Players read ICY reliably for MP3/AAC/Ogg streams. Support with raw WAV is doubtful **(verify)**.
     - So in-band titles probably mean encoding on the board, or an Icecast relay that re-encodes. Both are much heavier than v1.

My recommendation: 48 kHz stereo WAV on port 8000, pulled by players on the LAN, with the existing sidecar (plus SSE if a player wants push). Add Icecast and ICY only if a specific player needs titles inside the stream.

## Risks

- **Wi-Fi.** Throughput and jitter at 1.5 Mbit/s while TLS requests to Shazam run.
- **Slow listeners.** A listener that stalls must never stall audio capture. Use a bounded per-client buffer and drop the client when it falls behind.
- **RAM.** Internal RAM for TCP buffers with Wi-Fi, TLS, the web server and MQTT all running (bring-up measured 344 KB free at boot).
