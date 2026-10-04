# esp-shazam

An always-listening Shazam box. Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit listens to its LINE IN jack, fingerprints the audio on the board (SongRec's algorithm), asks Shazam, and shows the song at `http://esp-shazam.local/`, as JSON at `/api/state`, and on MQTT topic `esp-shazam/now_playing`.

- Design: `docs/superpowers/specs/2026-10-03-esp-shazam-design.md`
- Check everything: `scripts/check`

## Listening to LINE IN

The board also streams what plays into LINE IN, 48 kHz 24-bit stereo, on port 8000. Two listeners at most, WAV and FLAC together; a third gets `503`.

- `http://esp-shazam.local:8000/stream.flac`: lossless FLAC at about two thirds of the WAV's bitrate (1.5 Mbit/s on a record).
- `http://esp-shazam.local:8000/stream.wav`: the same audio as plain PCM, 2.3 Mbit/s, for debugging and for players without FLAC.

On the Mac, a `.local` name takes 5 s to look up (`gotchas.md`), so the board's IP is quicker.

Players tried on `/stream.flac` from the Mac on 2026-10-04:
- **ffplay 8.1.2:** `SDL_AUDIODRIVER=dummy ffplay -nodisp -autoexit <url>` played it until the board dropped the stream in a network spike. Processes started from the agent's session couldn't open CoreAudio (`AudioQueueStart` error -66681), hence the dummy audio driver.
- **VLC 3.0.23:** `VLC -I dummy --aout adummy --run-time 30 --play-and-exit <url>` used its FLAC demuxer and decoder (48000 Hz, 2 channels, 24 bits) and played the full 30 s. At the start it warned that the clock arrived late, raised its buffer to 1 s, and dropped one late buffer.
- **ffmpeg and ffprobe:** ffprobe reads `flac`, 48000 Hz, stereo, 24 bits. `scripts/stream-check --path /stream.flac` captures the stream with ffmpeg and checks it.

Over Wi-Fi the stream stops when round trips stay above about 30 ms (FLAC) or 20 ms (WAV); the player then has to reconnect. See the spec's Risks.

Licensed GPL-3.0-or-later (the fingerprint code comes from SongRec).
