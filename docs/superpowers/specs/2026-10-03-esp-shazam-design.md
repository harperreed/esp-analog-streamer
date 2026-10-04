# esp-shazam design

An always-listening Shazam box. Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit listens to its 3.5 mm LINE IN, builds a Shazam fingerprint on the board, asks Shazam what is playing, and shows the answer on a web page, a JSON endpoint, and MQTT. No helper server.

Date: 2026-10-03. Status: design approved in chat; this written spec awaits Harper's review.

Items marked **(verify)** come from reading docs and source, not from running anything. The bring-up and implementation steps check them.

## Success criteria

1. A song plays into LINE IN (Harper's turntable or continuous source). Within 45 s of the music starting, `GET http://esp-shazam.local/api/state` shows the right title and artist. At 240 MHz, with flash and PSRAM at 80 MHz, that leaves room for two tries: 8 s of audio, a ~10 s fingerprint and the request, then 8 s more and another fingerprint.
2. The same song arrives as a retained message on topic `esp-shazam/now_playing` at `mqtt://192.168.200.8:1883`.
3. When the song changes, both outputs update within 60 s. A 30 s re-check can land while the 12 s window still holds the old song, so the next re-check catches it.
4. When the music stops, "now playing" clears within 75 s, and the board sends no Shazam requests while the input stays quiet.
5. `scripts/check` passes, and both E2E scripts pass against the real Shazam API and the real broker.

## Decisions

Harper's answers, 2026-10-03, quoted:

| Question | Answer |
|---|---|
| Where the fingerprint is computed | "On the board (Recommended)" |
| When to listen | "Always listening" |
| Where results show up | "Web page on the board", "MQTT, and json endpoint" |
| Design section 1 | "Yes, but use Rust" |
| Which board | "ESP32-Audio-Kit Audio Development Board, ESP32-A1S Audio Development Board Module ... that is the board." |
| Revised design | "Yes, write the spec (Recommended)" |
| Logistics | UART cable "done"; Mac → LINE IN cable "done"; broker "192.168.200.8 (no auth)"; song fixture "ok." |

Consequences:

- Rust on the std ESP-IDF crates was chosen over C so the firmware can reuse SongRec's Rust fingerprint code rather than port it.
- LINE IN is the input. The onboard mics, LEDs, speaker, and buttons stay unused.
- The LED ring and serial output were not chosen as outputs. Serial carries debug logs only.

## Hardware

Ai-Thinker ESP32-Audio-Kit (sold by EC Buying) with the ESP32-A1S module.

Confirmed on Harper's board with esptool 5.2.0:

- Chip: ESP32-D0WD-V3, revision v3.1. Dual core, 240 MHz, Wi-Fi + BT, 40 MHz crystal.
- Flash: 4 MB (manufacturer 0x5e, device 0x4016).
- MAC: 70:4b:ca:87:18:3c.
- USB: the UART micro-USB port goes to a CP2102 (0x10C4:0xEA60). macOS 26 drives it with the built-in AppleUSBSLCOM driver, so the board shows up as `/dev/cu.usbserial-0001`. Auto-reset over RTS works, so flashing needs no button presses. The other micro-USB port carries power only.
- Stock firmware: Ai-Thinker v1.1.0 (ESP-IDF v3.3.2, built 2021-06-08) with a single 1 MB factory partition. Backed up to `backups/stock-flash-2026-10-03.bin` before the first flash; `gotchas.md` has its sha256 and the restore command.

Confirmed at bring-up on 2026-10-03 by the Task 7 probe on ESP-IDF v5.5.5 (log in `target/bringup.log`):

- **PSRAM: 8 MB chip, 4 MB usable.** The boot log says "Found 8MB PSRAM device" at 40 MHz, then "Virtual address not enough for PSRAM, map as much as we can. 4MB is mapped". 4096 KB joins the heap; 4093 KB is free at boot. That covers the fingerprint's 2.1 MB. Task 11b moved PSRAM and flash to 80 MHz (below).
- **Codec: ES8388 v1** (the first row of the table below). 0x10 ACKed on SDA 33 / SCL 32. Register 0x08 read 0x00, not the 0x80 power-up value: the codec kept older settings through the ESP32 reset, as expected.
- **CPU clock: 160 MHz** during this probe, ESP-IDF's default (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ=160`). Task 8 moved the firmware to 240 MHz; see Risks for what that buys.
- **Revisions and pins.** Three revisions exist, with different pins (source: [pschatzmann/arduino-audio-driver `src/AudioBoards/`](https://github.com/pschatzmann/arduino-audio-driver/tree/main/src/AudioBoards)):

  | Revision | I2C SCL / SDA | I2S MCLK / BCLK / WS / DOUT / DIN | Codec I2C address |
  |---|---|---|---|
  | ES8388 v1 (this board) | 32 / 33 | 0 / 27 / 25 / 26 / 35 | 0x10 |
  | ES8388 v2 | 23 / 18 | 0 / 5 / 25 / 26 / 35 | 0x10 |
  | AC101 | 32 / 33 | 0 / 27 / 26 / 25 / 35 | 0x1A |

  The probe tried 0x10 and then 0x1A on the v1 pins, then 0x10 on the v2 pins. The silkscreen can't tell the revisions apart: "2957" appears on all three. Only the revision found (ES8388 v1) gets implemented. Any other probe result stops the firmware with a clear log line.

Confirmed at bring-up on 2026-10-03 by Task 8 (logs in `target/`, kept locally):

- **CPU clock: 240 MHz** (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_240=y`, SDD Ruling R11). The boot log says "cpu freq: 240000000 Hz".
- **The codec setup reads back as written**, including `0x0A = 0x50`, `0x09` (PGA), `0x10`/`0x11 = 0x00` and `0x08 = 0x00`.
- **LINE IN feeds ES8388 LIN2/RIN2** (`0x0A = 0x50`), as the A1S pin table, the Audio Kit schematic and Ai-Thinker's SDK say.
  - With music on the jack, LIN2/RIN2 carried it, while LIN1/RIN1 read only the ADC's own noise: -86 dBFS of white noise at PGA 0 dB.
  - Turning the codec's mic bias off changed nothing on LIN2, so the A1S's MIC2 pads add no measurable sound.
- **LINE IN is wired to Harper's turntable**, not the Mac. The turntable's output must be on LINE (fixed level). On VARIABLE, turned down, music arrived at about -64 dBFS, under the -50 quiet threshold.
- **Levels at PGA 0 dB** (`0x09 = 0x00`), one log line per second:
  - Turntable on LINE: -22.5 to -28.2 dBFS, with peaks up to 11186 (-9.3 dBFS). That's only 3 s, at 17:18, before the record stopped. A minute earlier the same music clipped at +24 dB.
  - Harper's continuous source at 17:23 (45 s): median -43.5 dBFS (-49.7 to -41.8), peaks up to 1027.
  - Idle, cable plugged in and nothing playing: typically -75 to -82 dBFS (median about -79), with brief excursions to -62/-64. Handling can hold -35 to -50 dBFS for up to 3 s in a row.
  - Those idle seconds came while Harper was switching sources, so -79 is provisional. An earlier "idle" capture at 16:54 read -64 dBFS, the same as the turned-down music, so it most likely held music.
  - The board's `quiet_threshold_dbfs` is tuned to -60 for this setup (SDD Ruling R12). Typical idle sits 15–20 dB under it and the worst idle second (-62.2) just under. The continuous source's lowest second (-57.9) stays just above, so it counts as sound.
- **The PGA stays at 0 dB** (`pga_gain = 0`, the cfg.toml default; SDD Ruling R12). The turntable on LINE already peaks about 9 dB under full scale, and +9 dB would put it at about -0.3 dBFS. The quieter continuous source is handled by the -60 threshold instead of more gain.
- **Not measured:** the 1 kHz tone check needs the Mac on LINE IN.
  - Live Shazam matches stand in for it. On 2026-10-03 (Task 9) the board's own captures matched three real tracks, which a wrong sample rate or garbled samples would prevent. A capture one bit late would only double every sample (+6 dB) and could still match, so the exact bit alignment rests on the register readback (`0x0C`: Philips, 16-bit; 24-bit since Task 14b, below).
  - The L/R interleave order is unknown too. It doesn't matter to Shazam, because the feed averages the two channels; it does matter to the stream (below).

Confirmed on 2026-10-03 by Task 11b (logs in `target/task11b/`, kept locally):

- **Flash and PSRAM clocks: 80 MHz** (`CONFIG_ESPTOOLPY_FLASHFREQ_80M=y`, `CONFIG_SPIRAM_SPEED_80M=y`, SDD Ruling R20). The flash mode stays DIO. The boot log says "SPI Speed : 80MHz", which the 2nd stage bootloader reads from its image header, and "esp_psram: Speed: 80MHz".
  - espflash rewrites the flash speed in both image headers, 40 MHz unless told otherwise, so `scripts/flash` passes `--flash-freq 80mhz`.
  - A 6-minute run with music playing showed no panic or reset. The PSRAM memory test at boot passed in 0.49 s (0.88 s at 40 MHz). Risks has the timings.

Confirmed on 2026-10-04 by Task 14b (logs in `target/task14b/`, kept locally):

- **48 kHz, 24-bit stereo capture** (SDD Ruling R21).
  - I2S0 runs as master with two 32-bit slots a frame (BCLK 3.072 MHz). The classic ESP32 clocks as many bits per slot as the data width, so the data width is 32. MCLK is 12.288 MHz, half of an APLL that ESP-IDF's coefficients put at 24,575,996 Hz (-0.15 ppm).
  - The ES8388 sends 24-bit Philips words (`0x0C = 0x00`) at 256 x LRCK (`0x0D = 0x02`); both read back.
  - Against `esp_timer`, frames arrive at 480,000 ± 240 per 10 s, with about 1 frame of drift in 130 s.
- **The words carry 24 bits.** The low 8 bits of the samples are nonzero in 99.6% of samples, and each slot's low byte is always 0 (the codec drives zeros after its last bit).
- **Levels through the decimator:** Harper's continuous source read -42 to -56 dBFS (1 s readings). With the source off, the feed read -85 to -87.7 dBFS and each channel about -82 to -83 dBFS full band, close to the ADC's own -86 dBFS that bring-up measured on LIN1/RIN1.
- **Not verified:** which channel is left. Left and right differ (equal in under 0.1% of frames), and the I2S standard puts left on WS low, but proving it takes a one-sided signal. The stream (Task 15) is the first thing it matters to.

Not yet confirmed:

- **No LINE IN detect pin.** GPIO12 shows up as "aux detect" in drivers, but that constant was copied from Espressif's LyraT board. On this kit GPIO12 is SD card DAT2 and the MTDI strapping pin, so firmware must not drive it. The jack's switch pins aren't wired.
- **Other shared pins:**
  - Speaker amp enable on GPIO21 (left off).
  - LED on GPIO22.
  - Headphone detect on GPIO39 (reads 0 when plugged in).
  - GPIO0 doubles as MCLK and a boot strapping pin; I2S takes it over after boot.
- **ES8388 facts the codec setup depends on** (source: Everest datasheet Rev 5.0 and Espressif's esp_codec_dev driver):
  - No reset pin, and an ESP32 reset doesn't reset the codec, so setup starts with a soft reset.
  - It powers up as I2S master (`0x08 = 0x80`) and must be switched to slave.
  - Its ADC digital volume powers up at -96 dB (`0x10`/`0x11 = 0xC0`) and must be set to 0 dB.
  - Full scale is 1.0 Vrms with the PGA at 0 dB, which suits line level.
  - The ADC high-pass filter (DC removal) is on by default.
  - There is no chip-ID register. The 0x10 ACK plus a write-then-readback of `0x08` proves it's alive.

## Architecture

```
LINE IN ─► ES8388 ─I2S 48 kHz─► audio_in ─► 12 s ring buffer (PSRAM) + loudness
                                                  │
                                 policy: "ask Shazam now?"
                                                  ▼
                fingerprint (SongRec) ─► shazam request ─► HTTPS amp.shazam.com
                                                  ▼
                                    song_store (one source of truth)
                                     │                         │
                     web: / and /api/state          mqtt_out: retained publish
```

### `core`: Rust library, no ESP dependencies

It builds on the Mac for tests and on the ESP32 for the firmware.

- **`fingerprinting`**: copied from songrec-lib 0.5.4 (`src/fingerprinting/{algorithm,signature_format,hanning,user_agent}.rs`, GPL-3.0-or-later, by marin-m). It keeps SongRec's attribution and module name, so internal paths stay unchanged. Three edits:
  1. Remove `make_signature_from_file`, which depends on rodio. Keep `make_signature_from_buffer(&[i16]) -> DecodedSignature`, which already takes 16-bit samples.
  2. Store each 256 × 1025 f32 ring as one contiguous `Vec<[f32; 1025]>` (1 MB), not 256 separate 4 KB `Vec`s.
     - ESP-IDF's malloc puts allocations of up to `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` bytes (8191 in this firmware) in internal RAM first. 512 small 4 KB rows would fill the ESP32's ~300 KB of internal RAM that Wi-Fi and TLS need.
     - One 1 MB allocation goes to PSRAM.
     - The math doesn't change, so the output bytes don't either.
  3. Replace the deprecated `base64::encode` and `base64::decode` with base64 0.22's `Engine` API. Output is the same; this only removes the deprecation warnings.

  The copied module carries `#[allow(clippy::all)]`, so clippy doesn't push us to restyle upstream code. Behavior-identical copying matters more than lint style here.

  Dependencies: chfft (the FFT songrec-lib uses), byteorder, crc32fast, base64. All are pure Rust, and all built for `xtensa-esp32-espidf` and ran on the board at bring-up (2026-10-03).

  From reading the code: chfft allocates an 8 KB `Vec` per FFT, about 1,500 times per 12 s signature, and frees each right away.
- **`shazam`**: builds the recognize request and parses the response.
  - It takes the signature, the wall-clock time, two UUIDs, and a user-agent string as inputs, so tests can pin them.
  - The request format is in [Shazam protocol](#shazam-protocol).
- **`policy`**: the listen policy as a pure state machine. Time and loudness come in as inputs; the decision comes out. See [Listen policy](#listen-policy).

### `firmware`: esp-idf-svc app

Its own cargo project on the Xtensa toolchain. It path-depends on `core`.

- **`codec`**: probes I2C, then sets up the ES8388 for ADC from LINE IN, in I2S slave mode with 24-bit words.
  - No Rust ES8388 crate exists (checked on crates.io, 2026-10-03), so this is about 80 lines of register writes following Espressif's Apache-2.0 ES8388 driver.
  - The PGA gain gets set at bring-up so that peaks stay below clipping.
- **`audio_in`**: runs I2S0 as master in RX mode: 48 kHz stereo, 24-bit samples in 32-bit slots, MCLK at 256 × fs (12.288 MHz) from the APLL on GPIO0 (SDD Ruling R21).
  - It keeps the newest 2 s of raw frames in a stream ring (768,000 bytes, in PSRAM) for the LINE IN stream.
  - `core`'s decimator averages left and right, low-passes and keeps one sample in three. That 16 kHz mono feed goes into a 12 s ring buffer (384 KB, in PSRAM). The thread never waits for the recognizer's lock on that ring; it keeps its samples until the lock is free (SDD Ruling R25).
  - It writes the RMS loudness of the feed's last second to `song_store` every 250 ms, from sums it keeps per 250 ms (SDD Ruling R24).
  - It runs as a high-priority thread on core 0.
- **`net`**: Wi-Fi station mode with credentials from `cfg.toml`, SNTP, and the mDNS hostname `esp-shazam`. Wi-Fi modem sleep is off (SDD Ruling R27): with it on, the AP holds the board's incoming frames, TCP ACKs included, until its next beacon, and the stream needs round trips under 20 ms.
- **`recognizer`**: a thread on core 1 that runs the policy. It starts only when `shazam_enabled` is true. When the policy says ask, it:
  1. copies the last 12 s,
  2. builds the fingerprint,
  3. POSTs it with the esp-idf-svc HTTP client (TLS against the ESP-IDF certificate bundle, HTTP/1.1),
  4. feeds the outcome back to the policy and updates `song_store`.
- **`song_store`**: the one source of truth. It holds the policy state, loudness, the current song, the last 20 songs (newest first), and the rate-limit end time.
  - A mutex guards it.
  - It notifies `mqtt_out` when the current song changes.
- **`web`**: an `EspHttpServer` that serves two routes:
  - `GET /`: one embedded HTML file with no external scripts. It polls `/api/state` every 3 s.
  - `GET /api/state`: the JSON endpoint.
- **`mqtt_out`**: an `EspMqttClient` connected to the broker in `cfg.toml`. When the current song changes, it publishes retained to `esp-shazam/now_playing`. A blank broker URL disables MQTT.
- **`stream`**: the LINE IN stream on port 8000, a plain `std::net::TcpListener` of its own (SDD Ruling R21). An accept thread and one thread per listener (at most 2) run on core 1 at priority 7, above the recognizer (5). They copy at most 240 frames (5 ms) per lock of the stream ring and never hold it during socket work, so capture never waits on a listener. [Outputs](#line-in-stream-port-8000) has the protocol.

### `tools/shazam-file`: Mac CLI

Takes a 16 kHz mono 16-bit WAV, runs the `core` fingerprint and request builder, sends the request to the real Shazam with `ureq`, and prints the parsed song.

The Mac E2E test uses it, and it saves real responses as parser fixtures.

### Memory

| Item | Size | Where |
|---|---|---|
| Fingerprint rings | about 2.1 MB | PSRAM |
| Audio ring (12 s at 16 kHz) | 384 KB | PSRAM |
| 12 s snapshot | 384 KB | PSRAM |
| Stream ring (2 s at 48 kHz, stereo, `i32`) | 768,000 bytes | PSRAM |
| I2S DMA buffers (10 × 1,920 bytes) | 19 KB | internal (DMA can't reach PSRAM) |
| Stream server: accept thread, and a thread per listener (4 KiB stacks) | 4 KiB + 4 KiB per listener | internal (thread stacks) |
| Per stream listener: read and send buffers | 3,360 bytes | internal (under the 8,191-byte threshold) |
| Per stream listener, at its low: the above, lwIP's send queue and the Wi-Fi driver's TX buffers | about 20 KB (measured, Task 15) | internal |
| Signature, base64 and body | under 64 KB | heap |
| Response | capped at 128 KB | heap |

Large allocations go to PSRAM through ESP-IDF's malloc (`CONFIG_SPIRAM_USE_MALLOC`). Confirmed at bring-up: with `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=16384` (since Task 11 it is 8191), the probe's 384,000-byte test buffer took 376 KB of PSRAM, and the 2.1 MB fingerprint ran with only 344 KB of internal RAM free. Wi-Fi and TLS keep internal RAM.

Flash gets one factory app partition sized for the binary, with no OTA.

## Listen policy

Pure logic in `core::policy`. Every number below except `window_secs` is a `cfg.toml` setting with this default. `window_secs` is fixed at 12 s (SDD Ruling R14):

| Setting | Default | Why |
|---|---|---|
| `quiet_threshold_dbfs` | -50 | Tune at bring-up against the LINE IN noise floor. This board runs -60 (floor about -79 dBFS, SDD Ruling R12) |
| `window_secs` | 12 | SongRec's buffer length (fixed: the ring size is a compile-time constant) |
| `search_interval_secs` | 8 | SongRec's default `request_interval_secs_v3` |
| `slow_search_interval_secs` | 30 | After 5 misses in a row |
| `recheck_interval_secs` | 30 | Catches song changes while matched |
| `clear_after_misses` | 3 | While matched |
| `clear_after_quiet_secs` | 60 | Music stopped |
| `rate_limit_backoff_secs` | 300 | After HTTP 429 |

The rules:

1. At most one request is in flight.
2. No request goes out until SNTP has synced, because the request carries a timestamp.
3. **Quiet**: loudness is below the threshold. No requests. After 60 s of continuous quiet, the current song clears.
4. **Searching**: loud, with no current song.
   - The first request waits until 8 s of loud audio has built up. Requests then repeat every 8 s.
   - After 5 misses in a row, they repeat every 30 s instead.
   - Going quiet and coming back resets the miss count.
5. **A match** moves the policy to **Matched** with that Shazam track key. If the key differs from the current one, the current song changes and gets published.
6. **Matched**: a request goes out every 30 s.
   - The same key leaves things as they are.
   - A new key switches the song and publishes it.
   - A miss adds to a count. Three misses in a row clear the current song and send the policy back to Searching.
7. **HTTP 429**: pause for 300 s, keeping the current song, then resume in the earlier state.
8. **Other failures** (network, 5xx, unparseable body): log them and retry at the state's normal interval. They don't count as misses.

## Shazam protocol

Mirrors SongRec's `src/core/fingerprinting/communication.rs` (marin-m/SongRec, last pushed 2026-09-05).

- `POST https://amp.shazam.com/discovery/v5/en/US/android/-/tag/{UUID1}/{uuid2}?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&video=v3`
  - UUID1 is a fresh v4 UUID in upper case; uuid2 is a fresh v4 UUID in lower case.
- Headers:
  - `Content-Type: application/json`
  - `Content-Language: en_US`
  - `User-Agent`: a random pick from SongRec's list of Android user agents
  - Forced HTTP/1.1.
- Body:
  ```json
  {"geolocation":{"altitude":300,"latitude":45,"longitude":2},
   "signature":{"samplems":12000,"timestamp":<ms since epoch, truncated to u32>,
                "uri":"data:audio/vnd.shazam.sig;base64,<signature>"},
   "timestamp":<same>,"timezone":"Europe/Paris"}
  ```
- HTTP 429 means "Your IP has been rate-limited" (SongRec's message).
- Response fields read (SongRec's `http_task.rs`):
  - `track.key`
  - `track.title`
  - `track.subtitle` (the artist)
  - `track.images.coverart`
  - `track.sections[].metadata[]`, where `title == "Album"`, for the album
  - `track.url` for the Shazam link (confirmed in the real capture, `test/fixtures/shazam/match.json`)

  No `track` object means a miss.

## Outputs

### `GET /api/state`

```json
{
  "state": "matched",
  "shazam_enabled": true,
  "level_dbfs": -18.4,
  "rate_limited_until": null,
  "now": {
    "key": "12345678",
    "title": "Song Title",
    "artist": "Artist",
    "album": "Album or null",
    "cover_url": "https://... or null",
    "shazam_url": "https://... or null",
    "identified_at": "2026-10-03T19:22:01Z"
  },
  "history": [ { "...": "same shape as now, newest first, at most 20" } ]
}
```

- `state` is one of `quiet`, `searching`, `matched`, or `rate_limited`.
- `shazam_enabled` is the `shazam_enabled` setting from `cfg.toml`. When it's `false` the board starts no recognizer, so `state` stays `quiet` and `now` stays `null` (SDD Ruling R21). `level_dbfs` stays live either way.
- `level_dbfs` is the loudness of the last second, updated every 250 ms by the audio thread, asks included (SDD Ruling R24; this retires R16's freeze for the level).
- `state` updates every 250 ms between asks and holds still during each ask (~12 s, while the board fingerprints and calls Shazam), because the recognizer thread sets it (SDD Ruling R16).
- `now` is `null` when nothing is playing.
- `identified_at` is when the song first became `now`. Later re-checks of the same key don't change it.
- `history` lists every song that became `now`, newest first, so the current song is `history[0]`. It lives in RAM and empties on reboot.

### `GET /`

One self-contained HTML page. It shows:
- the current song with its cover art (loaded from Shazam's CDN by the browser),
- the state and loudness; with `shazam_enabled` false, "Shazam is off" in place of the state, and no "Nothing playing.",
- the history list.

### MQTT

- Topic `esp-shazam/now_playing`, retained, QoS 1.
- The payload is the `now` object above, or `null` when it clears.
- It publishes when `now` changes and again on every broker (re)connect, so the retained value can't go stale after a broker restart.
- With `shazam_enabled = false` it publishes `null` on every connect and nothing else.
- The broker is `mqtt://192.168.200.8:1883` with no auth; port 1883 answered on 2026-10-03.

### LINE IN stream (port 8000)

`http://esp-shazam.local:8000/stream.wav` (Task 15, SDD Ruling R21). mDNS advertises it as "esp-shazam stream" (`_http._tcp`, port 8000, TXT `path=/stream.wav`), next to the page's "esp-shazam".
- `GET /stream.wav` answers `200 OK` with `Content-Type: audio/wav`, `Cache-Control: no-store`, `Connection: close` and no `Content-Length`. The body is a 44-byte WAV header, then the stream ring's frames as they arrive, from the moment the listener joins. A query string is ignored.
  - The header says PCM, 48000 Hz, 2 channels, 24 bits packed little-endian, with RIFF and data sizes of 0xFFFFFFFF. ffmpeg reads that data size as "until the stream ends" and reports `pcm_s24le`, 48000 Hz, 2 channels; a strict reader such as hound refuses it, because it isn't a whole number of 6-byte frames. With format tag 1 there's no channel mask, so ffprobe shows `channel_layout=unknown`.
- Any other path gets `404`, any other method `405` (with `Allow: GET`), and a malformed request line `400`. Each is a complete response, and the connection closes. Only GET is answered on either port.
- At most 2 listeners; a third gets `503`. A slot frees when its listener's connection ends.
- A listener that falls a ring (2 s) behind is closed: a read finds its place overwritten, or a write waits 2 s. The close is a FIN after any unsent audio, so a player that resumes gets a clean end of stream. A reset wouldn't reach a stalled player (`gotchas.md`).
- `scripts/stream-check` captures the stream with ffmpeg and checks its length, its level against `/api/state`, both channels and gaps.

### Config

- `firmware/cfg.toml` holds the settings and is gitignored. A committed `cfg.toml.example` lists them:
  - `wifi_ssid`, `wifi_pass`
  - `mqtt_url`
  - `hostname`
  - `pga_gain`, the ES8388 PGA register value (0 is 0 dB)
  - `shazam_enabled` (default `true`): `false` starts no recognizer and sends no Shazam traffic (SDD Ruling R21)
  - the policy settings above, except `window_secs`: the audio ring and snapshot are sized for a fixed 12 s
- The firmware reads it at build time with `toml-cfg` 0.2.0, the newest release on 2026-10-03 and the pattern the esp-rs std training uses.
  - `TOML_CFG = "require_cfg_present"` in `firmware/.cargo/config.toml` turns a missing `cfg.toml`, or one without an `[esp-shazam-firmware]` table, into a build error.
  - toml-cfg pastes each value into Rust source, so values can't contain `"` or `\`.

## Toolchain and versions

- **Installed by mise** (Harper's call), pinned in the repo's `mise.toml`:
  - Rust 1.93.0 for the Mac side.
  - cmake 3.30.2 and ninja 1.12.1, the versions ESP-IDF v5.5.5's `tools.json` pins. The Homebrew cmake is 4.3.4.
  - Prebuilt GitHub release binaries of espup 0.18.0, espflash 4.6.0, and ldproxy 0.3.2. Compiling espflash 4.6.0 would need Rust ≥ 1.95.
- **Xtensa Rust toolchain:** a mise task (`mise run esp-toolchain`) runs `espup install --targets esp32 --std --toolchain-version 1.98.1.0`. That version is the newest esp-rs toolchain released before esp-idf-sys 0.38.1.
- **Building with the right toolchain:** mise exports `RUSTUP_TOOLCHAIN`, which outranks `firmware/rust-toolchain.toml`, so firmware builds run `cargo +esp`.
- **Flashing:** `ldproxy` links and `espflash` flashes and monitors; esptool 5.2.0 stays as a backup. espflash 4.6.0 bundles an ESP-IDF 6.1 bootloader, so flashing passes the 5.5.5 `bootloader.bin` that esp-idf-sys builds next to the ELF.
- **Crates:** esp-idf-svc 0.53.0, esp-idf-hal 0.47.0, esp-idf-sys 0.38.1 and embuild 0.33.5, all pinned with `=`.
  - Their repos were archived on 2026-09-19 when they moved into the community-maintained monorepo `esp-rs/esp-idf`.
  - esp-idf-sys 0.38.0 added "basic compatibility" for ESP-IDF 6.0.
- **ESP-IDF:** v5.5.5, set by `ESP_IDF_VERSION` in `firmware/.cargo/config.toml`. esp-idf-sys downloads and builds it on the first build, which is slow and takes a few GB.
- **Target:** `xtensa-esp32-espidf`.
- **Mac side:** Rust 1.93.0 (installed), ffmpeg (installed, for test clips), `mosquitto_sub` and `jq` (installed).

## Repo layout

```
esp-shazam/
├── Cargo.toml        workspace: core, tools (firmware is excluded)
├── core/             fingerprint, shazam, policy
├── tools/            shazam-file
├── firmware/         esp-idf-svc app: own .cargo/config.toml, rust-toolchain.toml,
│                     sdkconfig.defaults, partitions.csv, cfg.toml.example
├── scripts/          check, flash, e2e-host, e2e-device
├── test/fixtures/    shazam/ (captured real responses, committed)
│                     songs/ (local audio, gitignored)
├── docs/superpowers/ specs/, plans/
├── CLAUDE.md, gotchas.md, README.md
└── LICENSE           GPL-3.0
```

## Testing

There are no mocks anywhere. The E2E tests use the real Shazam API and the real broker.

- **Unit tests** (`cargo test` on the Mac):
  - `policy`: table-driven scenarios of time, loudness and outcome sequences, covering every rule in the listen policy.
  - `shazam` request: the URL shape, headers, and body fields match SongRec's.
  - `shazam` response: parses real responses (one match, one miss) that the Mac E2E captured into `test/fixtures/shazam/`.
  - `fingerprinting` encoding: our encoded signature decodes through songrec-lib's own `decode_from_uri` back to the same peaks.
- **Golden test:** our `fingerprinting` and songrec-lib 0.5.4's `make_signature_from_buffer` must produce identical bytes from the same PCM. Same code, same chfft, same f32, so any difference means one of our three edits broke something.
  - The PCM comes from synthetic tones and chirps generated inside the test, plus the local song clip when one is present.
  - songrec-lib is pinned as a dev-dependency with `=0.5.4`.
  - If songrec-lib won't build on the Mac **(verify)**, the reference becomes an unedited copy of its three files kept under `core/tests/`.
- **Mac E2E** (`scripts/e2e-host <clip> "<expected title>"`): runs `shazam-file` against the real Shazam.
- **Device E2E** (`scripts/e2e-device <song> "<expected title>"`):
  1. Whatever plays into LINE IN (Harper's source) is the music. The script resets the board first. `afplay` can't reach LINE IN, which is wired to the source (SDD Ruling R13).
  2. Poll `/api/state` until the title appears, with a 45 s timeout (success criterion 1).
  3. `mosquitto_sub` must deliver the same title on `esp-shazam/now_playing`.
  4. Stop playback and confirm `now` clears within 75 s (success criterion 4).
- **Canonical check** (`scripts/check`):
  1. `cargo fmt --check`
  2. `cargo clippy -- -D warnings`
  3. `cargo test` for `core` and `tools`
  4. a release build of `firmware`

  The E2E scripts run separately because they need network and hardware.
- **Song fixture:** one track from the Mac's music library (Harper: "ok."), converted with ffmpeg to 16 kHz mono 16-bit WAV in `test/fixtures/songs/`, which is gitignored.

## Bring-up sequence

1. Back up the stock flash: `esptool read-flash 0 0x400000 backups/stock-flash-2026-10-03.bin` (it read reliably only at 115200 baud). The `backups/` folder is gitignored.
2. Flash a minimal firmware that does three things:
   - logs PSRAM size from the ESP-IDF boot log;
   - probes I2C on both pin sets and logs the codec address and revision;
   - times one 12 s fingerprint of synthetic audio.
3. Set up the ES8388 and I2S RX. Read back the codec registers, log loudness with music on LINE IN to confirm LIN2/RIN2 carries the jack, and set the PGA gain.
4. If a 12 s fingerprint takes over 6 s, raise `search_interval_secs` to cover it. Switching the FFT crate is a later option, only if the numbers demand it. (Measured 12.6 s; the interval was not raised. Risks explains why, and the CPU clock is settled at 240 MHz (SDD R11), with flash and PSRAM at 80 MHz (SDD R20).)
5. Build the full app, then run the device E2E.

## Risks

- **Shazam's API is unofficial**, and SongRec already handles 429s, so rate limits are real. The policy limits requests, backs off on 429, and the code follows SongRec's request shape.
- **The esp-idf crates are mid-move.** Versions are pinned. If the toolchain blocks progress, the fallback is C on ESP-IDF with the same module split, and switching needs Harper's approval.
- **The classic ESP32 is slower than an S3, and its PSRAM is slower.** The fingerprint touches about 16 KB of PSRAM for every 128 samples of audio. Bring-up measured it on 2026-10-03:
  - One 12 s fingerprint of `chord_and_chirp(12)` takes **12618 ms at the default 160 MHz** (6572-byte signature). That's slower than real time. Three task-watchdog prints landed inside that window and cost about 0.16 s of it, because the probe ran on the main task on core 0.
  - A throwaway 240 MHz build (not committed) took 10526 ms. 50% more clock cut the time by only 17%. That fits about 6 s that scales with the clock and about 6 s that doesn't. The log can't tell whether the fixed half is PSRAM (40 MHz) or code fetched from flash (40 MHz). Watchdog backtraces sampled the 160 MHz run in peak spreading, chfft's FFT, and the code around the FFT.
  - `search_interval_secs` stays 8 for now, against bring-up step 4's rule (which would make it 15). The policy starts that interval when the answer arrives, so the fingerprint already sits in front of it. Raising it would add idle time after each answer and push the first ask from 8 s to 15 s.
  - With 8 s at 160 MHz, asks come about 21 s apart, plus the HTTPS time. The second answer lands near 43–46 s, at the edge of success criterion 1's 45 s; with 15 s only one answer fits. Running at 240 MHz moves the second answer to about 40 s.
  - Decided (SDD Ruling R11): the firmware runs at 240 MHz (`CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_240=y`, Task 8). R11 parked faster PSRAM and flash (80 MHz) until the timings missed the success criteria. After Task 11 they did, and Task 11b adopted 80 MHz (SDD Ruling R20, below).
  - Task 9 measured six live asks on 2026-10-03 (logs `target/recognize.log` and `target/recognize-dma6.log`, kept locally):
    - Fingerprints of 12 s of LINE IN took 10336–10736 ms, and whole asks 11494–14620 ms. The request, TLS handshake included, added 0.8–1.4 s, except 4.1 s for the first request after one boot.
    - The first match came 25.6 s and 28.5 s after reset, boot, Wi-Fi and SNTP included. Criterion 1's 45 s held, so 80 MHz PSRAM and flash stayed parked then.
    - While matched, asks came every 41.9–42.2 s: the 30 s re-check plus the ask itself.
    - Matches: "DON'T WAIT 4 ME (feat. Kara Manansala) [Oliver Night Remix]" by Alexander Flood; then "Take Me Higher" by WheelUP & Tiawa, caught by the first re-check after the source changed songs; and, after a reboot, "Dinero (feat. P Wavey) [Samtheman Remix]" by IZCO.
  - Task 11 (SDD Ruling R19) moved the fingerprint's 8 KB FFT tables and buffers to PSRAM, still at 40 MHz, to free internal RAM for TLS. Over 10 asks, fingerprints took 16126–16941 ms, the first match came 30.8 s after reset, and asks came every 47.8–48.1 s while matched. By the reckoning below, the worst-case song change grew to about 65 s and the worst-case clear to about 78 s, past criteria 3 and 4.
  - Task 11b (SDD Ruling R20) runs flash and PSRAM at 80 MHz. It measured nine live asks on 2026-10-03 while `/api/state` was polled every 3 s (logs in `target/task11b/`, kept locally):
    - Fingerprints took 9754–10350 ms (mean 10168), and whole asks 10420–11132 ms. The request, TLS handshake included, added 0.67–0.84 s.
    - The first match came 23.7 s after reset. While matched, asks came every 40.8–41.4 s.
    - For a clean change (one that doesn't straddle a snapshot window). A window holding both songs can still return the old one, and that change can then pass 60 s. When the song changes just after a snapshot, the next snapshot comes one cycle later and its answer one ask after that, about 41.4 + 11.1 ≈ 53 s (criterion 3: 60 s). When the music stops as an ask starts, no quiet ticks arrive during that ask, so the clear lands about 11.1 + 60 ≈ 71 s after the music stops (criterion 4: 75 s).
    - Internal RAM held up: the lowest `INTERNAL|8BIT` free since boot was 31,496 bytes (30,844 at 40 MHz), the largest block before every TLS session was 47,104 bytes, and no TLS session failed.
    - Matches: "Someone Else" by Raphael Meleteas; then "BLUE" by Billie Eilish, caught by a re-check after the source changed songs.
  - Task 14b moved capture to 48 kHz stereo on core 0 (SDD Ruling R21). It measured seven live asks on 2026-10-04 with a poller on `/api/state` every 2–3 s (logs in `target/task14b/`, kept locally):
    - Fingerprints took 10,839–11,280 ms (mean 11.1 s), 9% slower than Task 11b's: core 0's capture now shares flash and PSRAM with them. Whole asks took 11,769–14,471 ms.
    - The first match came 25.2 s and 28.1 s after reset. While matched, answers came every 42.2–42.8 s, and once 44.7 s around the slowest ask.
    - By the reckoning above, a clean song change lands about 42.8 + 12.5 ≈ 55 s after it happens (criterion 3: 60 s), and a clear about 12.5 + 60 ≈ 73 s after the music stops (criterion 4: 75 s). The slowest ask seen (14.5 s, the TLS session took 3.2 s) would make those about 59 s and 74.5 s.
    - Internal RAM: the lowest `INTERNAL|8BIT` free since boot was 30,056 bytes over five asks with 8 I2S DMA buffers, and 36,064 over two asks with the final 10. The largest block before every TLS session was 47,104 bytes, and no TLS session failed. PSRAM's lowest free, during a fingerprint, was 397,372 bytes.
    - Matches: "Zhaba" by DakhaBrakha; "Trem Batucada" by Favela Chic & Os Reis Du Batuque, caught by a re-check after the source changed songs; after a reboot, "Salam" by Amina.
- **Asks slow the audio thread.** The recognizer copies 384 KB out of the PSRAM ring under its lock: 87–91 ms with PSRAM at 40 MHz (Task 9), 48–52 ms at 80 MHz (Task 11b), and 58–62 ms with 48 kHz capture running (Task 14b).
  - ESP-IDF 5.5.5's I2S driver queues `dma_buffer_count - 1` DMA buffers and drops the oldest when the reader falls behind (`i2s_common.c`). At 16 kHz the default 6 buffers of 15 ms gave 75 ms, so each ask lost about 15 ms of audio, and Task 9 went to 16 buffers (225 ms).
  - Since Task 14b the audio thread doesn't wait for that lock: it `try_lock`s and keeps its 16 kHz samples until the lock is free (SDD Ruling R25).
  - Core 0 still slows down when a fingerprint starts. Over 11 asks the audio thread fell 26–31 ms behind, 1–20 ms after the snapshot ended, and lost no frames. At 48 kHz a 240-frame buffer is 5 ms, and `audio_in`'s 10 buffers queue 45 ms in 19,200 bytes of internal RAM.
  - The likely cause, not proven: the fingerprint fills about 2 MB of new PSRAM buffers on core 1 while core 0 runs its code from flash over the same bus. Allocating those buffers once would need an edit to SongRec's copied files beyond the spec's three.
- **Wi-Fi throughput for the LINE IN stream.** One listener needs 2.3 Mbit/s (288,000 bytes/s). Task 15 measured it on 2026-10-04 from Harper's Mac, which is on Wi-Fi and reaches the board through Tailscale's subnet router. The board was on channel 1, 802.11n at 20 MHz, RSSI -51 dBm, DTIM 1. Logs are in `target/task15/`, kept locally.
  - **With ESP-IDF's defaults the headroom is negative to small, and it swings.** Between 10:42 and 11:18 local time, 10 s of sending as fast as TCP allowed reached 1.2-2.4 Mbit/s, and listeners fell a ring behind within 6-22 s. From 11:40, on the same settings, two listeners held 60 s each at full rate, then 5 minutes each with every `scripts/stream-check` check passing, no I2S buffer lost and the audio thread at most 9.5 ms late. At 11:53 listeners fell behind again within 8-13 s; the Mac's own round trip to Tailscale's subnet router then averaged 53 ms.
  - **The limit is lwIP's 5,760-byte TCP send buffer.** A connection moves at most that much per round trip, so 288,000 bytes/s needs round trips under 20 ms. With Wi-Fi modem sleep on (ESP-IDF's default), round trips averaged 75-90 ms; with it off, 14-34 ms. Since SDD Ruling R27 the firmware turns it off (`net::start_wifi`). With only 4 segments in flight, a lost segment waits out lwIP's retransmission timeout (1.5 s at first), where a fast retransmit would take one round trip.
  - **A bigger send buffer helps one listener and costs internal RAM.** At 23,040 bytes (a diag build) throughput roughly doubled, to 4.0-5.5 Mbit/s, and one listener held 60 s; of two, one fell behind in 3.3 s. One saturated connection then took about 70 KB of internal RAM at its low, and two listeners plus a test connection left 1,559 bytes free. At 11,520 bytes it reached 3.8 Mbit/s, and two listeners fell behind together after 21 s.
  - **Capture never suffered.** No I2S buffer was lost in any run, through full-speed sends and the 1,559-byte low, and the audio thread ran at most 9.9 ms late after boot (the queue holds 45 ms).
  - **Changed:** modem sleep is off (SDD Ruling R27). It costs no internal RAM and cut round trips, but it didn't keep a 30 s stream going through a bad stretch by itself (Task 15, run A).
  - **Not changed (Harper's call):** the send buffer size, and PSRAM for Wi-Fi and lwIP buffers (`CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP`). A client on the board's own LAN would show whether the limit is the board or the path. With Shazam on, even one listener (about 20 KB at its low) is over the internal RAM margin (about 16 KB).
- **PSRAM could be missing.** Bring-up found 8 MB, 4 MB of it usable, which settles this risk.
- **Line level vs ADC range**: the gain gets set in bring-up step 3.
- **The board revision could be AC101.** That means a different register set but the same `codec` interface and about the same size.

## Out of scope

- LED, speaker and amp, buttons, onboard mics
- Home Assistant discovery
- OTA updates
- Wi-Fi provisioning
- persistent history
- web UI auth
- MQTT over TLS

## Licensing

GPL-3.0-or-later, because `core::fingerprinting` comes from songrec-lib (GPL-3.0+).

## Sources

- SongRec: https://github.com/marin-m/SongRec (`src/core/fingerprinting/`, `src/core/microphone_thread.rs`, `src/core/preferences.rs`, `src/core/http_task.rs`)
- songrec-lib 0.5.4: https://crates.io/crates/songrec-lib
- ShazamIO host fix (amp.shazam.com still serves discovery/v5): https://github.com/shazamio/ShazamIO/pull/186
- Audio Kit pins: https://github.com/pschatzmann/arduino-audio-driver/tree/main/src/AudioBoards
- Audio Kit USB ports: https://probots.co.in/esp32-a1s-audio-development-board-kit-ai-thinker-wifi.html
- esp-idf-sys changelog: https://github.com/esp-rs/esp-idf-sys/blob/master/CHANGELOG.md
- esp-idf-svc status: https://github.com/esp-rs/esp-idf-svc
- ESP-IDF releases: https://github.com/espressif/esp-idf/releases
