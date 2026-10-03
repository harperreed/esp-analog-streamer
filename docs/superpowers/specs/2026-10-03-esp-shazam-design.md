# esp-shazam design

An always-listening Shazam box. Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit listens to its 3.5 mm LINE IN, builds a Shazam fingerprint on the board, asks Shazam what is playing, and shows the answer on a web page, a JSON endpoint, and MQTT. No helper server.

Date: 2026-10-03. Status: design approved in chat; this written spec awaits Harper's review.

Items marked **(verify)** come from reading docs and source, not from running anything. The bring-up and implementation steps check them.

## Success criteria

1. A song plays from the Mac's headphone jack into LINE IN. Within 45 s of the music starting, `GET http://esp-shazam.local/api/state` shows the right title and artist. That leaves room for three tries: 8 s of audio, then one try every 8 s.
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
- Stock firmware: Ai-Thinker v1.1.0 (ESP-IDF v3.3.2, built 2021-06-08) with a single 1 MB factory partition. It gets backed up before the first flash.

Not yet confirmed:

- **PSRAM (verify).** Listings say the A1S has 8 MB. The classic ESP32 maps 4 MB directly, which is enough. The ESP-IDF boot log reports it at bring-up. Without PSRAM this design does not fit, because the fingerprint alone needs about 2.1 MB.
- **Codec and pin revision (verify).** Three revisions exist, with different pins (source: [pschatzmann/arduino-audio-driver `src/AudioBoards/`](https://github.com/pschatzmann/arduino-audio-driver/tree/main/src/AudioBoards)):

  | Revision | I2C SCL / SDA | I2S MCLK / BCLK / WS / DOUT / DIN | LINE IN detect | Codec I2C address |
  |---|---|---|---|---|
  | ES8388 v1 | 32 / 33 | 0 / 27 / 25 / 26 / 35 | GPIO12, active low | 0x10 |
  | ES8388 v2 | 23 / 18 | 0 / 5 / 25 / 26 / 35 | GPIO12, active low | 0x10 |
  | AC101 | 32 / 33 | 0 / 27 / 26 / 25 / 35 | none listed | 0x1A |

  An I2C probe of both pin sets at bring-up identifies the revision. Only that revision gets implemented. Any other probe result stops the firmware with a clear log line.
- **LINE IN input pins (verify).** The usual wiring is mics on ES8388 LIN1/RIN1 and the LINE IN jack on LIN2/RIN2. Bring-up confirms it by checking which input shows signal while music plays.
- Shared pins: the speaker amp enable is on GPIO21 (left off), the LED on GPIO22, and headphone detect on GPIO39. GPIO0 serves as both MCLK and a boot strapping pin; I2S takes it over after boot.

## Architecture

```
LINE IN ─► ES8388 ─I2S 16 kHz─► audio_in ─► 12 s ring buffer (PSRAM) + loudness
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

- **`fingerprint`**: copied from songrec-lib 0.5.4 (`src/fingerprinting/{algorithm,signature_format,hanning}.rs`, GPL-3.0-or-later, by marin-m). It keeps SongRec's attribution. Besides fixing module paths, it gets two edits that change behavior:
  1. Remove `make_signature_from_file`, which depends on rodio. Keep `make_signature_from_buffer(&[i16]) -> DecodedSignature`, which already takes 16-bit samples.
  2. Allocate the big buffers straight on the heap. The two 256 × 1025 f32 rings are about 1 MB each, and `Box::new([[0.0; 1025]; 256])` builds the array on the stack first. ESP32 task stacks hold a few KB.

  Dependencies: chfft (the FFT songrec-lib uses), byteorder, crc32fast, base64. All are pure Rust **(verify they build for `xtensa-esp32-espidf`)**.
- **`shazam`**: builds the recognize request and parses the response.
  - It takes the signature, the wall-clock time, two UUIDs, and a user-agent string as inputs, so tests can pin them.
  - The request format is in [Shazam protocol](#shazam-protocol).
- **`policy`**: the listen policy as a pure state machine. Time and loudness come in as inputs; the decision comes out. See [Listen policy](#listen-policy).

### `firmware`: esp-idf-svc app

Its own cargo project on the Xtensa toolchain. It path-depends on `core`.

- **`codec`**: probes I2C, then sets up the ES8388 for ADC from LINE IN, in I2S slave mode at 16 bit.
  - No Rust ES8388 crate exists (checked on crates.io, 2026-10-03), so this is about 80 lines of register writes following Espressif's Apache-2.0 ES8388 driver.
  - The PGA gain gets set at bring-up so that peaks stay below clipping.
- **`audio_in`**: runs I2S0 as master in RX mode: 16 kHz, 16-bit stereo, MCLK at 256 × fs on GPIO0.
  - It averages left and right into mono and writes into a 12 s ring buffer (384 KB, in PSRAM).
  - It keeps the RMS loudness of the last second in dBFS.
  - It runs as a high-priority thread.
- **`net`**: Wi-Fi station mode with credentials from `cfg.toml`, SNTP, and the mDNS hostname `esp-shazam`.
- **`recognizer`**: a thread on core 1 that runs the policy. When the policy says ask, it:
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

### `tools/shazam-file`: Mac CLI

Takes a 16 kHz mono 16-bit WAV, runs the `core` fingerprint and request builder, sends the request to the real Shazam with `ureq`, and prints the parsed song.

The Mac E2E test uses it, and it saves real responses as parser fixtures.

### Memory

| Item | Size | Where |
|---|---|---|
| Fingerprint rings | about 2.1 MB | PSRAM |
| Audio ring | 384 KB | PSRAM |
| 12 s snapshot | 384 KB | PSRAM |
| Signature, base64 and body | under 64 KB | heap |
| Response | capped at 128 KB | heap |

Large allocations go to PSRAM through ESP-IDF's malloc (`CONFIG_SPIRAM_USE_MALLOC`) **(verify the size threshold)**. Wi-Fi and TLS keep internal RAM.

Flash gets one factory app partition sized for the binary, with no OTA.

## Listen policy

Pure logic in `core::policy`. Every number below is a `cfg.toml` setting with this default:

| Setting | Default | Why |
|---|---|---|
| `quiet_threshold_dbfs` | -50 | Tune at bring-up against the LINE IN noise floor |
| `window_secs` | 12 | SongRec's buffer length |
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
  - `track.url` for the Shazam link **(verify the field against a captured response)**

  No `track` object means a miss.

## Outputs

### `GET /api/state`

```json
{
  "state": "matched",
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
- `now` is `null` when nothing is playing.
- `identified_at` is when the song first became `now`. Later re-checks of the same key don't change it.
- `history` lists every song that became `now`, newest first, so the current song is `history[0]`. It lives in RAM and empties on reboot.

### `GET /`

One self-contained HTML page. It shows:
- the current song with its cover art (loaded from Shazam's CDN by the browser),
- the state and loudness,
- the history list.

### MQTT

- Topic `esp-shazam/now_playing`, retained, QoS 1.
- The payload is the `now` object above, or `null` when it clears.
- It publishes when `now` changes and again on every broker (re)connect, so the retained value can't go stale after a broker restart.
- The broker is `mqtt://192.168.200.8:1883` with no auth; port 1883 answered on 2026-10-03.

### Config

- `firmware/cfg.toml` holds the settings and is gitignored. A committed `cfg.toml.example` lists them:
  - `wifi_ssid`, `wifi_pass`
  - `mqtt_url`
  - `hostname`
  - the policy settings above
- The firmware reads it at build time with the `toml-cfg` crate, the pattern the esp-rs std training uses **(verify the crate's current state)**.

## Toolchain and versions

- **Rust toolchain:** `espup` 0.18.0 installs the Xtensa Rust toolchain (the `esp` channel) and LLVM. `ldproxy` links. `espflash` flashes and runs the serial monitor; esptool 5.2.0 stays as a backup.
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
  - `fingerprint` encoding: our encoded signature decodes through songrec-lib's own `decode_from_uri` back to the same peaks.
- **Golden test:** our `fingerprint` and songrec-lib 0.5.4's `make_signature_from_buffer` must produce identical bytes from the same PCM. Same code, same chfft, same f32, so any difference means one of our two edits broke something.
  - The PCM comes from synthetic tones and chirps generated inside the test, plus the local song clip when one is present.
  - songrec-lib is pinned as a dev-dependency with `=0.5.4`.
  - If songrec-lib won't build on the Mac **(verify)**, the reference becomes an unedited copy of its three files kept under `core/tests/`.
- **Mac E2E** (`scripts/e2e-host <clip> "<expected title>"`): runs `shazam-file` against the real Shazam.
- **Device E2E** (`scripts/e2e-device <song> "<expected title>"`):
  1. `afplay` plays the song through the headphone jack into LINE IN.
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

1. Back up the stock flash: `esptool read-flash 0 0x400000 backups/stock-flash.bin`. The `backups/` folder is gitignored.
2. Flash a minimal firmware that does three things:
   - logs PSRAM size from the ESP-IDF boot log;
   - probes I2C on both pin sets and logs the codec address and revision;
   - times one 12 s fingerprint of synthetic audio.
3. Set up the ES8388 and I2S RX. Log loudness with music on LINE IN, pick LIN1 or LIN2, and set the PGA gain.
4. If a 12 s fingerprint takes over 6 s, raise `search_interval_secs` to cover it. Switching the FFT crate is a later option, only if the numbers demand it.
5. Build the full app, then run the device E2E.

## Risks

- **Shazam's API is unofficial**, and SongRec already handles 429s, so rate limits are real. The policy limits requests, backs off on 429, and the code follows SongRec's request shape.
- **The esp-idf crates are mid-move.** Versions are pinned. If the toolchain blocks progress, the fallback is C on ESP-IDF with the same module split, and switching needs Harper's approval.
- **The classic ESP32 is slower than an S3, and its PSRAM is slower.** The fingerprint touches about 16 KB of PSRAM for every 128 samples of audio. Bring-up step 2 measures it.
- **PSRAM could be missing.** That would break the design, and bring-up step 2 finds out first.
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

GPL-3.0-or-later, because `core::fingerprint` comes from songrec-lib (GPL-3.0+).

## Sources

- SongRec: https://github.com/marin-m/SongRec (`src/core/fingerprinting/`, `src/core/microphone_thread.rs`, `src/core/preferences.rs`, `src/core/http_task.rs`)
- songrec-lib 0.5.4: https://crates.io/crates/songrec-lib
- ShazamIO host fix (amp.shazam.com still serves discovery/v5): https://github.com/shazamio/ShazamIO/pull/186
- Audio Kit pins: https://github.com/pschatzmann/arduino-audio-driver/tree/main/src/AudioBoards
- Audio Kit USB ports: https://probots.co.in/esp32-a1s-audio-development-board-kit-ai-thinker-wifi.html
- esp-idf-sys changelog: https://github.com/esp-rs/esp-idf-sys/blob/master/CHANGELOG.md
- esp-idf-svc status: https://github.com/esp-rs/esp-idf-svc
- ESP-IDF releases: https://github.com/espressif/esp-idf/releases
