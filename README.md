# esp-shazam

An always-listening Shazam box. Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit listens to its LINE IN jack, fingerprints the audio on the board (SongRec's algorithm), asks Shazam, and shows the song at `http://esp-shazam.local/`, as JSON at `/api/state`, and on MQTT topic `esp-shazam/now_playing`.

- Design: `docs/superpowers/specs/2026-10-03-esp-shazam-design.md`
- Check everything: `scripts/check`

## Linux workstation setup

With mise and uv installed, copy `firmware/cfg.toml.example` to the gitignored
`firmware/cfg.toml` and fill in the local settings. Run `scripts/setup-linux`
as your normal user in a terminal. It uses sudo to install Ubuntu/Debian build
and audio dependencies, grants access to `/dev/ttyUSB0`, installs the pinned tools,
and runs the project checks. Use `--port DEVICE` for another serial device,
`--dry-run` to preview the steps, or `--flash` to flash production firmware after
checks pass. USB access needs renewal after unplugging the board. Logs live in
`target/setup-linux/`; credentials and test media must stay local.

## Listening to LINE IN

The board also streams what plays into LINE IN, 48 kHz 24-bit stereo, on port 8000. Two listeners at most, WAV and FLAC together; a third gets `503`.

- `http://esp-shazam.local:8000/stream.flac`: lossless FLAC at about two thirds of the WAV's bitrate (1.5 Mbit/s on a record).
- `http://esp-shazam.local:8000/stream.wav`: the same audio as plain PCM, 2.3 Mbit/s, for debugging and for players without FLAC.

On the Mac, a `.local` name takes 5 s to look up (`gotchas.md`), so the board's IP is quicker.

Players tried on `/stream.flac` from the Mac on 2026-10-04:
- **ffplay 8.1.2:** `SDL_AUDIODRIVER=dummy ffplay -nodisp -autoexit <url>` played it until the board dropped the stream in a network spike. Processes started from the agent's session couldn't open CoreAudio (`AudioQueueStart` error -66681), hence the dummy audio driver.
- **VLC 3.0.23:** `VLC -I dummy --aout adummy --run-time 30 --play-and-exit <url>` used its FLAC demuxer and decoder (48000 Hz, 2 channels, 24 bits) and played the full 30 s. At the start it warned that the clock arrived late, raised its buffer to 1 s, and dropped one late buffer.
- **ffmpeg and ffprobe:** ffprobe reads `flac`, 48000 Hz, stereo, 24 bits. `scripts/stream-check --path /stream.flac` captures the stream with ffmpeg and checks it.

The deployed configuration has Shazam off (R26). One shared encoder feeds independent FLAC readers: the raw ring holds 2 s and 94 encoded slots hold 8.02 s. WAV is a debug stream with 2 s of slack. These rings request 3,079,930 heap bytes before encoder, listener and network storage. Large payloads prefer PSRAM with internal fallback; slot lengths prefer internal RAM. The board maps 4 MiB PSRAM. PSRAM is required at boot; this is not a free-memory guarantee. See the [design](docs/superpowers/specs/2026-10-03-esp-shazam-design.md) and [measurement record](docs/superpowers/plans/2026-10-04-kata-audit.md).

A slow reader or a capture gap ends its stream; reconnect to resume. Fresh-audio waits are bounded to 2 s plus a tick and scheduling delay. Each whole frame has an absolute write deadline equal to its ring's duration; checked socket close drains queued payload for 2 s plus lwIP's 500 ms poll and scheduling delay. Two worst-case FLAC streams need about 4.6 Mbit/s and remain untested on this route; FIP music measurements do not establish that limit.

Real-board checks (replace `HOST` with the board's IP):

```sh
caffeinate -is scripts/stream-check --path /stream.flac --seconds 60 HOST
caffeinate -is scripts/stream-churn HOST --path /stream.wav --trickle-bytes 128 --trickle-interval 0.5
caffeinate -is scripts/stream-soak HOST --seconds 21600 --out target/overnight
```

The soak resets the board once, requires Shazam off, and checks two real decoders and capture telemetry. Hardware fixtures replace the production image: `scripts/flash --example flac-cost` measures encoding cost; `capture-faults` and `listener-pressure` pair with `scripts/fault-check --mode capture HOST` and `--mode pressure HOST`. Always restore production with `scripts/flash` after a fixture.

Licensed GPL-3.0-or-later (the fingerprint code comes from SongRec).
