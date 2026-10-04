# Gotchas

## Confirm the exact board before pinout research
"ESP32-S3 AI Smart Speaker" matched a Waveshare S3 board, but the hardware is an Ai-Thinker ESP32-A1S Audio Kit (classic ESP32). Ask for the listing or module label first. Once USB works, confirm with esptool's chip-id / flash-id and an I2C probe (ES8388 answers at 0x10, AC101 at 0x1A).

## The Audio Kit has two micro-USB ports; only "UART" carries data
The other port is power only. macOS 26 ships a CP2102 driver (AppleUSBSLCOM), so the board shows up as `/dev/cu.usbserial-*` with nothing to install.

## ES8388 boots as I2S master with the ADC at -96 dB
Without `0x08 = 0x00` (slave) and `0x10 = 0x11 = 0x00` (ADC volume 0 dB), the input looks dead. The codec has no reset pin and survives ESP32 resets, so start setup with a soft reset (`0x00 = 0x86`, then `0x06`).

## GPIO12 is not a line-in detect on the Audio Kit
Drivers label GPIO12 "aux detect", but that constant came from Espressif's LyraT board. On this kit GPIO12 is SD card DAT2 and the MTDI strapping pin, which sets flash voltage at boot. Don't drive it. LINE IN feeds ES8388 LIN2/RIN2 (`0x0A = 0x50`).

## The toolchain comes from mise, and mise's RUSTUP_TOOLCHAIN beats rust-toolchain.toml
Install tools with `mise trust && mise install`, then `mise run esp-toolchain`. Don't use brew, `rustup toolchain install` or `cargo install`. mise exports `RUSTUP_TOOLCHAIN=1.93.0`, which overrides `firmware/rust-toolchain.toml`, so build firmware with `cargo +esp` (or `scripts/flash`). Scripts load the pinned tools with `eval "$(mise env -s bash)"`.

## Don't over-plan this project
Harper stopped a planning spiral on 2026-10-03 ("lost in the planning sauce"). Write only the plan text the next task needs, and execute.

## songrec-lib's library crate is named `songrec`
The package is `songrec-lib = "=0.5.4"`, but its `[lib] name = "songrec"`, so Rust code writes `songrec::SignatureGenerator`, not `songrec_lib::`. It also pulls about 274 crates (cpal, rodio, reqwest) into the dev-dependency tree. That's fine on the Mac, and the firmware never builds it.

## Sessions started from Ghostty can't read ~/Downloads
`ls`, `ffprobe` and `open()` on anything under `~/Downloads` fail with "Operation not permitted", while `stat` works and Desktop, Documents and Music read fine. It looks like macOS's per-app Files & Folders privacy setting for Ghostty (verify in System Settings > Privacy & Security). The test songs therefore come from `mdfind`:
- A: "P.Y.T. (Pretty Young Thing)"
- B: "Blame It On the Boogie"

Filter the results with `grep -E '\.m4a$'`, because mdfind also returns `test/fixtures/songs/clip.wav`, which carries the song's title tag. The plan's Tasks 4–12 already do this (SDD Ruling R7).

## Stock firmware backup
`backups/stock-flash-2026-10-03.bin` (gitignored) is the full 4 MB of Ai-Thinker's v1.1.0 firmware. Restore with
`esptool --port /dev/cu.usbserial-0001 write-flash 0 backups/stock-flash-2026-10-03.bin`.
- Size 4194304 bytes, sha256 `9bf7d88933f91eae99175f07314d28dc957097e4d27f566ff2dd7547a2eeacf6`.
- `esptool verify-flash 0 backups/stock-flash-2026-10-03.bin` matched the board's own MD5 of all 4 MB before the first flash.

## Read flash at 115200 baud; faster reads fail on this CP2102
`esptool read-flash` at 921600 or 460800 dies at once with "Invalid head of packet (0xFF)". A `--trace` run shows that commands at those rates work, but the first byte of the stub's 4 KB data frame arrives as 0x7F instead of 0xC0. At the default 115200 the 4 MB read takes about 6.5 minutes and passes its MD5 check.

## ldproxy's release zip carries no executable bit
The `ldproxy-aarch64-apple-darwin.zip` asset (esp-rs/embuild tag `ldproxy-v0.3.2`) was zipped on a FAT host and stores `rw-r--r--`. mise 2025.10.17 keeps archive file modes, so ldproxy installed but `command -v ldproxy` found nothing. `mise.toml` fixes it with a per-tool `postinstall` that runs `chmod +x "$MISE_TOOL_INSTALL_PATH/ldproxy"`. If you install the tool before that line exists, rerun `mise install --force github:esp-rs/embuild`.

## ldproxy 0.3.2's "Running ldproxy" line fails the firmware warnings check
rustc reports any linker stderr as a `linker_messages` warning, and ldproxy 0.3.2 logs that line at info level. `firmware/.cargo/config.toml` sets `LDPROXY_LOG = "warn"`, so ldproxy's real warnings and errors still show. Upstream moved the line to debug in ldproxy 0.3.5 (esp-rs/embuild commit 093676c0), but 0.3.5 ships no prebuilt binary.

## espflash 4.6.0 always warns "Monitor options were provided" without --monitor
It's an espflash bug, and it's harmless: `flash()` sets `monitor_args.elf` to the image path just before `check_monitor_args()` tests `elf.is_some()`. `scripts/flash` passes no monitor options.

## A 12 s fingerprint takes 12.6 s on this board
On 2026-10-03 it took 12618 ms at ESP-IDF's default 160 MHz, and 10526 ms in a throwaway 240 MHz build. About half the time scales with the clock and half doesn't (PSRAM or flash fetch at 40 MHz); the firmware runs at 240 MHz (SDD Ruling R11). `search_interval_secs` stays 8, against the plan's "over 6 s" rule, because the policy starts the interval when the answer arrives. The spec's Risks section has the numbers.
- Generating the test signal, `chord_and_chirp(12)`, takes about 19 s on the ESP32 at 160 MHz (libm `sinf` on large arguments). The probe doesn't time it; keep it out of anything timed.

## Long work on the main task trips the task watchdog
`main` runs on core 0, so a fingerprint there starves IDLE0. The watchdog then prints a backtrace every 5 s (no panic by default). The prints double as a rough profiler: decode the addresses with `xtensa-esp32-elf-addr2line -fC -e firmware/target/xtensa-esp32-espidf/release/esp-shazam`; the tool sits under `firmware/.embuild/espressif/tools/xtensa-esp-elf/`. Long-running work belongs on core 1, where Task 9's recognizer runs, because `sdkconfig.defaults` watches only CPU 0's idle task.

## LINE IN is wired to Harper's turntable, not the Mac
`afplay` can't reach the board. Device audio checks need Harper to play a record, or to move the 3.5 mm cable to the Mac's headphone jack for a test. On its LINE output the turntable read -22 to -28 dBFS at PGA 0 dB (a 3 s sample), so the ES8388 PGA stays at 0 dB. Harper's continuous test source reads about -43 dBFS, and idle sits about -79 dBFS, so the board's quiet threshold is -60 (`firmware/cfg.toml`, SDD Ruling R12). Device checks run against whatever plays.

## Turntable output must be on LINE (fixed), not VARIABLE
VARIABLE was turned down, and music reached the board at about -64 dBFS. That's under the -50 quiet threshold, so the board would have heard silence. The codec was never at fault: LIN2/RIN2 carried the music (the level rose about 24 dB with the PGA), and LIN1/RIN1 read only the ADC's own -86 dBFS. When levels look low, check the source's output mode and volume first. Also confirm the source is stopped before calling a capture a noise floor: a capture logged as idle at 16:54 read the same -64 dBFS and most likely held the turned-down music.

## Park the board after a device test, or it keeps asking Shazam
From Task 9 on, the firmware asks Shazam about every 42 s while music plays, for as long as the board has power. When a test ends, run `espflash board-info --port /dev/cu.usbserial-0001 --after no-reset`. The chip then stays in its serial bootloader ("Staying in bootloader"), and nothing gets written. The next reset, `scripts/serial-log` or `scripts/flash` boots the app again.

## ESP-IDF's I2S receive queue holds one buffer fewer than you configure
`i2s_common.c` (IDF 5.5.5) creates the receive queue with `dma_buffer_count - 1` slots. When the reader falls behind, the ISR drops the oldest buffer without logging anything. With the default 6 buffers of 240 frames at 16 kHz, that leaves 75 ms. The recognizer's 12 s snapshot holds the ring lock for about 90 ms, so `audio_in` uses 16 buffers (225 ms).

## toml-cfg drops attributes and pastes values into Rust source
`#[toml_cfg::toml_config]` rebuilds the struct from field names and types alone. Doc comments and `#[allow]` on the struct or its fields vanish, so use `//` comments in `config.rs`. Each `cfg.toml` value is printed as TOML and parsed as a Rust expression. A string containing `"`, `\`, a newline or a control character comes out in a TOML form that isn't Rust (`'...'`, triple quotes, or TOML escapes). The build then fails, and its panic message prints the value: a password among them lands in `target/check-logs/firmware-build.log`. A float setting needs a decimal point (`-60.0`, not `-60`).
It also ignores unknown keys, so a misspelled key in `cfg.toml` silently falls back to its default.

## Boot logs show the Wi-Fi name and the password's length
ESP-IDF's Wi-Fi driver logs `wifi:connected with <SSID>` at every connect. With station OWE support on (`CONFIG_ESP_WIFI_ENABLE_WPA3_OWE_STA=y`, the IDF default), `libnet80211.a` also warns "OWE is enabled, but authmode threshold is 3 or password length is N", where N is the real length of `wifi_pass`. Redact both before pasting a log anywhere. Turning OWE off would remove the warning; that waits on Harper (plan `## Now`, Open).

## mDNS comes from the espressif/mdns component, pinned to 1.8.2
`esp_idf_svc::mdns` exists only when that managed component is in the build: `[[package.metadata.esp-idf-sys.extra_components]]` in `firmware/Cargo.toml`. 1.8.2 builds on IDF 5.5.5; the registry's newest on 2026-10-03 was 1.14.0. The component manager downloads it into esp-idf-sys's out dir under `firmware/target/` and writes `firmware/components_esp32.lock` (committed). No `firmware/managed_components/` appears.
esp-idf-sys reruns its build script only when a file it tracks changes, such as `sdkconfig.defaults`, never `Cargo.toml`. After a change to `package.metadata.esp-idf-sys`, an existing `firmware/target` fails with "could not find `mdns` in `esp_idf_svc`". Run `touch firmware/sdkconfig.defaults` and build again.

## On this Mac, every esp-idf-sys build-script run clones ESP-IDF again
`~/.gitconfig` rewrites `https://github.com/` to `git@github.com:` (`url.insteadOf`), so `git remote get-url origin` in `firmware/.embuild/espressif/esp-idf/v5.5.5` returns the SSH form. embuild 0.33.5 compares that with the https URL, finds no match, deletes the checkout and clones it again over SSH. The build still passes: on 2026-10-03 the whole build, clone included, took 2 minutes. Ordinary builds don't rerun the script. A change to `sdkconfig.defaults` does, and so does the `touch` a metadata change needs (previous entry).

## Lookups of esp-shazam.local take 5 s on the Mac; use `curl -4`
The board has no IPv6 address (esp-idf-svc never creates a link-local one), so it can't answer AAAA queries. The mdns component never sends NSEC to say there is none. A dual-stack lookup, curl's default, then waits 5 s for the AAAA before returning the A record. Measured 2026-10-03: 5.02 s cold and 5.01 s warm dual-stack, against 0.01 s IPv4-only. `curl --max-time 5` by name fails; `curl -4` answers in 0.3 s. A browser will likely wait the same 5 s on its first lookup (not verified).
The Mac (192.168.99.0/24, en0) reaches the board's 192.168.200.0/24 through Tailscale (`utun4`). mDNS answers still arrive on en0, so something on the network repeats mDNS between the subnets. Requests over that path took 0.3–1.4 s.

## The board's pages answer GET only
`/` and `/api/state` have GET handlers and nothing else. `curl -I` sends HEAD, which ESP-IDF's httpd answers with `405 Method Not Allowed` (text/html), so check headers on a GET: `curl -s -D - -o /dev/null <url>`.

## Internal RAM is the budget: TLS to Shazam needs one 16.7 KB block of it
mbedTLS allocates only from internal RAM (`CONFIG_MBEDTLS_INTERNAL_MEM_ALLOC`, the IDF default), and `mbedtls_ssl_setup` needs one contiguous block of about 16.7 KB for its 16 KB input buffer. Thread stacks come out of internal RAM too (ESP-IDF pthreads use `MALLOC_CAP_INTERNAL`), and so does every malloc under 16 KB. Task 11's first MQTT build ran a pump thread and a publisher thread with 6 KiB stacks each. It took 25.2 KB of internal RAM, and one boot's first ask failed with `mbedtls_ssl_setup returned -0x7F00` (`MBEDTLS_ERR_SSL_ALLOC_FAILED`). The shipped `mqtt_out` takes 15.8 KB. Measured 2026-10-03 with a throwaway build:
- Before an ask: 62–72 KB internal free, largest block 25.6 KB.
- Lowest internal free since boot: 860 bytes, reached during an ask.
- Stack use: esp-mqtt's task about 2.1 KB of 6 KiB, the publisher 1.9 KB of 4 KiB.
Measure the internal heap before adding anything that lives in internal RAM. Letting mbedTLS use PSRAM (`CONFIG_MBEDTLS_DEFAULT_MEM_ALLOC`) would remove the 16.7 KB need, but ESP-IDF calls internal-only the more secure choice, so that waits on Harper.

## esp-mqtt holds its API lock through each connect attempt
`esp_mqtt_client_enqueue` and `_publish` wait for that lock, so a publish can block for up to `network_timeout` (10 s by default) while the broker is away. Publish only from `mqtt_out`'s own thread; the recognizer just sends on a channel. Callbacks from `EspMqttClient::new_cb` run on esp-mqtt's task, which needs no pump thread. `EspMqttClient::new`'s `connection.next()` does need one: without it, esp-mqtt stalls. Queued messages expire after 30 s (`OUTBOX_EXPIRED_TIMEOUT_MS`), and the publish on every connect covers longer outages.

## esp-idf-svc's MQTT error event always says ESP_FAIL
esp-idf-svc 0.53 maps every `MQTT_EVENT_ERROR` to a fixed `ESP_FAIL` (a TODO in its `mqtt/client.rs`), so `mqtt: ESP_FAIL (error code -1)` carries no cause. esp-mqtt's own `E (…) mqtt_client:` and `esp-tls:` lines just before it give the cause, for example `transport_read(): EOF` or `Connection reset by peer`.

## Reading the retained MQTT topic
- The retained message outlives the board. A parked or unplugged board leaves its last song on the broker, because there is no Last Will. After a reset, compare the payload with `/api/state`, or wait for the board's first publish (`null` right after it connects).
- A live subscriber gets the board's publishes with `retain=0`; only a new subscription shows `retain=1`. `mosquitto_sub -q 1 -F '%I retain=%r qos=%q %t %p'` shows the flags.
- The board reaches a broker on the Mac at its en0 address (192.168.99.151:1884), although the Mac reaches the board through Tailscale: the gateway routes between 192.168.99.0/24 and 192.168.200.0/24 (`ping -b en0 192.168.200.8`: 3–6 ms). Task 11's broker-restart test used that path.
