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

The board captures LINE IN at 48 kHz, 24-bit stereo and streams it on port 8000. It accepts at most two listeners across WAV and the selected compressed format; a third gets `503`. FLAC is the default build.

- `http://esp-shazam.local:8000/stream.flac`: 48 kHz, 16-bit stereo FLAC. The encoder discards the lowest eight capture bits, then losslessly encodes those 16-bit samples. Compression depends on the input; no music bitrate is guaranteed.
- `http://esp-shazam.local:8000/stream.wav`: the original 48 kHz, 24-bit stereo PCM, 2.304 Mbit/s before headers, for debugging and for players without FLAC.

On the Mac, a `.local` name takes 5 s to look up (`gotchas.md`), so the board's IP is quicker.

Historical results: players tried on the previous **24-bit FLAC** output from the Mac on 2026-10-04. These do not validate the current 16-bit output:

- **ffplay 8.1.2:** `SDL_AUDIODRIVER=dummy ffplay -nodisp -autoexit <url>` played it until the board dropped the stream in a network spike. Processes started from the agent's session couldn't open CoreAudio (`AudioQueueStart` error -66681), hence the dummy audio driver.
- **VLC 3.0.23:** `VLC -I dummy --aout adummy --run-time 30 --play-and-exit <url>` used its FLAC demuxer and decoder (48000 Hz, 2 channels, 24 bits) and played the full 30 s. At the start it warned that the clock arrived late, raised its buffer to 1 s, and dropped one late buffer.
- **ffmpeg and ffprobe:** ffprobe read `flac`, 48000 Hz, stereo, 24 bits. `scripts/stream-check --path /stream.flac` captured the stream with ffmpeg and checked it.

With Shazam off, one shared encoder feeds independent readers. Raw history holds
2 seconds; the FLAC ring holds 141 frames (12.032 seconds), using 16,399-byte
slots. Its payload allocation is 2,312,259 bytes, separate from raw samples,
slot metadata, encoder state, listeners and network storage. PSRAM is required
at boot. The Sonos Low/Medium/High settings request 1/4/8 seconds of retained
source audio, rounded up to complete frames, and apply on the next Play.
They do not specify source-to-speaker latency.

A slow reader or capture gap ends its stream. Fresh-audio waits are bounded to
2 seconds plus scheduling delay. Compressed frame writes have a separate roughly
8-second absolute deadline, independent of the longer history. Socket cleanup
retains its bounded drain policy. Two worst-case 16-bit FLAC listeners require
3.072 Mbit/s of sample payload before framing; this is a calculated requirement,
not a measured Wi-Fi throughput result.

### Format and buffer settings

One firmware includes FLAC and experimental 48 kHz stereo **320 kbps MP3**.
Open `http://HOST/settings` to save the output format, Low/Medium/High startup
buffer, and autoplay. The main page keeps destination selection and Play/Stop.
Format and buffer changes during playback apply on the next Stop → Play; the
current queue keeps its original settings. When stopped with no compressed
listeners, a format change can apply immediately. The API and page distinguish
the selected format from the active encoder.

MP3 requires recognition off. Only one encoder runs, and only its compressed
route accepts listeners; WAV remains available. Switching formats starts a fresh
audio history. MP3 is lossy and uses the same 24-to-16-bit input conversion.
The [benchmark](docs/audit-evidence/2026-10-09/mp3-benchmark.md)
records CPU, fidelity and delay limits; lower bandwidth is not a proven fix for
Wi-Fi stalls. The [live-pipeline results](docs/audit-evidence/2026-10-09/mp3-stream.md)
include successful single-listener playback and reconnect checks, but sustained
two-listener and High-buffer Sonos runs still overrun. FLAC is the default when
no format has been saved; MP3 remains experimental. Kata `e8hd` tracks the failed
reliability gates. Runtime selection does not resolve those delivery failures.

```sh
ESPFLASH_PORT=/dev/ttyUSB0 scripts/flash
# Select MP3 on /settings before checking its stream.
ESPFLASH_PORT=/dev/ttyUSB0 scripts/stream-soak HOST --path /stream.mp3 --seconds 120
```

Use `http://HOST:8000/stream.mp3` for live output, or append `?buffer=low`,
`?buffer=med` or `?buffer=high`. The 500-frame ring holds 12 seconds in 480,000
payload bytes. Whole-frame preset offsets are 1.008/4.008/8.016 seconds.
Sonos queue URLs and media types capture the active format at Play. Format and
buffer settings survive a reboot; changing codecs needs no reflash.

The Shine source is pinned and vendored with its original license and notices;
see [component provenance](mp3/vendor/shine/UPSTREAM.md). Encoder warmup, finite
stream tails and the measured gain difference remain explicit prototype limits.

Real-board checks (replace `HOST` with the board's IP and select FLAC before
running these examples):

```sh
caffeinate -is scripts/stream-check --path /stream.flac --seconds 60 HOST
caffeinate -is scripts/stream-churn HOST --path /stream.wav --trickle-bytes 128 --trickle-interval 0.5
caffeinate -is scripts/stream-soak HOST --seconds 21600 --out target/overnight
```

`stream-check` validates the captured codec, rate, channels and bit depth: FLAC16 or WAV24, both 48 kHz stereo. The soak resets the board once, requires Shazam off, and checks two real decoders and capture telemetry. Each decoder also records its input format in its private log for a check at completion, using the same board connection. `scripts/test-stream-format` checks these format rules with local ffmpeg fixtures and no hardware. Hardware fixtures replace the production image: `scripts/flash --example flac-cost` measures encoding cost; `capture-faults` and `listener-pressure` pair with `scripts/fault-check --mode capture HOST` and `--mode pressure HOST`. Always restore production with `scripts/flash` after a fixture.

Licensed GPL-3.0-or-later (the fingerprint code comes from SongRec).
