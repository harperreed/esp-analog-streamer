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

For a Linux packet trace during a streaming failure, run
`scripts/capture-stream-packets BOARD_IPV4 INTERFACE 360` in a terminal
(`tcpdump` must be installed). After it reports `listening on`, run the streaming
test separately. The command uses sudo, captures the board's TCP ports 80
and 8000 plus ARP address resolution, and stops after the requested duration. Private traces stay under
`target/packet-capture/`; review them before sharing because snapshots can include
partial audio or HTTP data. `--dry-run` previews the scope without sudo.

The separate `wifi-throughput` firmware example tests transport without audio.
After flashing it, `scripts/wifi-throughput-check BOARD_IPV4 --interface enp5s0
--out target/throughput.json` verifies two finite transfers on Linux; use the
actual Ethernet interface name and a new report filename. Disconnect workstation
Wi-Fi for a wired comparison when both interfaces share a subnet. Restore
production firmware with `scripts/flash` after testing.

## Sonos

Open the board's web page on port 80. Under Sonos, click **Discover**, choose a
speaker group, click **Select**, then **Play**. **Stop** releases the session
started by this board. Set listening volume in the Sonos app.

Automatic playback is off by default. When enabled, it waits for ten seconds of
quiet before arming, then starts after sustained input audio. Ten seconds of
quiet stops playback. Manual Stop or another controller taking over requires
quiet followed by new audio before another automatic start.

The board controls speakers locally over verified TLS and serves one live FLAC
item through its local queue endpoints. Speakers fetch audio directly from the
board on port 8000; no workstation service is required. The selected destination
and autoplay setting persist across restarts.

## Listening to LINE IN

The board captures LINE IN at 48 kHz, 24-bit stereo and streams it on port 8000. Two listeners at most, WAV and FLAC together; a third gets `503`.

- `http://esp-shazam.local:8000/stream.flac`: 48 kHz, 16-bit stereo FLAC. The encoder discards the lowest eight capture bits, then losslessly encodes those 16-bit samples. Compression depends on the input; no music bitrate is guaranteed.
- `http://esp-shazam.local:8000/stream.wav`: the original 48 kHz, 24-bit stereo PCM, 2.304 Mbit/s before headers, for debugging and for players without FLAC.

On the Mac, a `.local` name takes 5 s to look up (`gotchas.md`), so the board's IP is quicker.

Historical results: players tried on the previous **24-bit FLAC** output from the Mac on 2026-10-04. These do not validate the current 16-bit output:

- **ffplay 8.1.2:** `SDL_AUDIODRIVER=dummy ffplay -nodisp -autoexit <url>` played it until the board dropped the stream in a network spike. Processes started from the agent's session couldn't open CoreAudio (`AudioQueueStart` error -66681), hence the dummy audio driver.
- **VLC 3.0.23:** `VLC -I dummy --aout adummy --run-time 30 --play-and-exit <url>` used its FLAC demuxer and decoder (48000 Hz, 2 channels, 24 bits) and played the full 30 s. At the start it warned that the clock arrived late, raised its buffer to 1 s, and dropped one late buffer.
- **ffmpeg and ffprobe:** ffprobe read `flac`, 48000 Hz, stereo, 24 bits. `scripts/stream-check --path /stream.flac` captured the stream with ffmpeg and checked it.

With Shazam off (R26), one shared encoder feeds independent FLAC readers: the raw ring holds 2 s and 94 encoded slots hold 8.02 s. WAV is a debug stream with 2 s of slack. The encoder retains a conservative 24,591-byte slot bound despite its 16-bit output. On the 32-bit board these rings request 3,079,930 heap bytes: 96,000 raw frames × 8 bytes, plus 94 × (24,591 payload bytes + 4 length bytes). This excludes encoder, listener and network storage. Large payloads prefer PSRAM with internal fallback; slot lengths prefer internal RAM. The board maps 4 MiB PSRAM. PSRAM is required at boot; this is not a free-memory guarantee. See the [design](docs/superpowers/specs/2026-10-03-esp-shazam-design.md) and [measurement record](docs/superpowers/plans/2026-10-04-kata-audit.md).

A slow reader or a capture gap ends its stream; reconnect to resume. Fresh-audio waits are bounded to 2 s plus a tick and scheduling delay. Each whole frame has an absolute write deadline equal to its ring's duration; checked socket close drains queued payload for 2 s plus lwIP's 500 ms poll and scheduling delay. Two worst-case 16-bit FLAC streams carry 3.072 Mbit/s of sample payload (48,000 × 16 × 2 channels × 2 listeners), plus small FLAC headers and network overhead. This is a calculated bandwidth requirement, not a measured throughput result.

Real-board checks (replace `HOST` with the board's IP):

```sh
caffeinate -is scripts/stream-check --path /stream.flac --seconds 60 HOST
caffeinate -is scripts/stream-churn HOST --path /stream.wav --trickle-bytes 128 --trickle-interval 0.5
caffeinate -is scripts/stream-soak HOST --seconds 21600 --out target/overnight
```

`stream-check` validates the captured codec, rate, channels and bit depth: FLAC16 or WAV24, both 48 kHz stereo. The soak resets the board once, requires Shazam off, and checks two real decoders and capture telemetry. Each decoder also records its input format in its private log for a check at completion, using the same board connection. `scripts/test-stream-format` checks these format rules with local ffmpeg fixtures and no hardware. Hardware fixtures replace the production image: `scripts/flash --example flac-cost` measures encoding cost; `capture-faults` and `listener-pressure` pair with `scripts/fault-check --mode capture HOST` and `--mode pressure HOST`. Always restore production with `scripts/flash` after a fixture.

Licensed GPL-3.0-or-later (the fingerprint code comes from SongRec).
