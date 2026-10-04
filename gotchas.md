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

## espflash rewrites the flash speed in the bootloader's header
espflash 4.6.0 rewrites byte 3 of the bootloader's image header (flash size and speed) and recomputes the bootloader's SHA-256, even when `--bootloader` names the IDF-built file. The speed comes from `--flash-freq`, or else the chip default, which is 40 MHz on the ESP32 (`src/image_format/idf.rs`, `Chip::default_flash_frequency`). The app's header gets the same value.
- esp-idf-sys builds an 80 MHz bootloader (header byte `0x2f`), but without the flag its 2nd stage would set up flash at 40 MHz while the app's sdkconfig says 80.
- `scripts/flash` passes `--flash-freq 80mhz`. Change it together with `CONFIG_ESPTOOLPY_FLASHFREQ_*` in `sdkconfig.defaults`.
- The boot log's `boot.esp32: SPI Speed : 80MHz` shows the header; `esp_psram: Speed: 80MHz` shows the app's sdkconfig.
- `firmware/.cargo/config.toml` has no cargo `runner` for the same reason: `espflash flash --monitor` would write a 40 MHz header, its own bootloader and its default partition table. Flash with `scripts/flash`.

## A 12 s fingerprint takes about 11 s on this board
On 2026-10-03 it took 12618 ms at ESP-IDF's default 160 MHz, and 10526 ms in a throwaway 240 MHz build. About half the time scales with the clock and half doesn't (PSRAM or flash fetch at 40 MHz); the firmware runs at 240 MHz (SDD Ruling R11). `search_interval_secs` stays 8, against the plan's "over 6 s" rule, because the policy starts the interval when the answer arrives. The spec's Risks section has the numbers.
- Since Task 11's fix round (SDD Ruling R19), the FFT's 8 KB tables and buffers live in PSRAM (`CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=8191`; see "Internal RAM is the budget"). With PSRAM at 40 MHz, a fingerprint then took 16.1–16.9 s, and a re-check cycle while matched about 48 s.
- Since Task 11b (SDD Ruling R20), flash and PSRAM run at 80 MHz. Over 9 asks a fingerprint took 9.75–10.35 s (mean 10.2 s), and a cycle while matched 40.8–41.4 s. The 12 s snapshot holds the ring lock about 50 ms, down from about 90 ms.
- Since Task 14b, core 0 captures 48 kHz stereo and runs the decimator, and the two cores share flash and PSRAM. Over 7 asks a fingerprint took 10.84–11.28 s (mean 11.1 s), the snapshot 58–62 ms, and a cycle while matched 42.2–42.8 s.
- Generating the test signal, `chord_and_chirp(12)`, takes about 19 s on the ESP32 at 160 MHz (libm `sinf` on large arguments). The probe doesn't time it; keep it out of anything timed.

## Long work on the main task trips the task watchdog
`main` runs on core 0, so a fingerprint there starves IDLE0. The watchdog then prints a backtrace every 5 s (no panic by default). The prints double as a rough profiler: decode the addresses with `xtensa-esp32-elf-addr2line -fC -e firmware/target/xtensa-esp32-espidf/release/esp-shazam`; the tool sits under `firmware/.embuild/espressif/tools/xtensa-esp-elf/`. Long-running work belongs on core 1, where Task 9's recognizer runs, because `sdkconfig.defaults` watches only CPU 0's idle task.

## LINE IN is wired to Harper's turntable, not the Mac
`afplay` can't reach the board. Device audio checks need Harper to play a record, or to move the 3.5 mm cable to the Mac's headphone jack for a test. On its LINE output the turntable read -22 to -28 dBFS at PGA 0 dB (a 3 s sample), so the ES8388 PGA stays at 0 dB. Harper's continuous test source reads about -43 dBFS, and idle sits about -79 dBFS, so the board's quiet threshold is -60 (`firmware/cfg.toml`, SDD Ruling R12). Device checks run against whatever plays.

## Turntable output must be on LINE (fixed), not VARIABLE
VARIABLE was turned down, and music reached the board at about -64 dBFS. That's under the -50 quiet threshold, so the board would have heard silence. The codec was never at fault: LIN2/RIN2 carried the music (the level rose about 24 dB with the PGA), and LIN1/RIN1 read only the ADC's own -86 dBFS. When levels look low, check the source's output mode and volume first. Also confirm the source is stopped before calling a capture a noise floor: a capture logged as idle at 16:54 read the same -64 dBFS and most likely held the turned-down music.

## Park the board after a device test, or it keeps asking Shazam
From Task 9 on, the firmware asks Shazam about every 41 s while music plays, for as long as the board has power. When a test ends, run `espflash board-info --port /dev/cu.usbserial-0001 --after no-reset`. The chip then stays in its serial bootloader ("Staying in bootloader"), and nothing gets written. The next reset, `scripts/serial-log` or `scripts/flash` boots the app again.
Since 2026-10-04 Harper's `firmware/cfg.toml` has `shazam_enabled = false` (Harper: "focusing on the streaming"). The board then sends nothing to Shazam (only SNTP, MQTT and mDNS traffic) and stays running, so Harper can watch the level on the page. Park it only after a test that turns the flag back on.

## ESP-IDF's I2S receive queue holds one buffer fewer than you configure
`i2s_common.c` (IDF 5.5.5) creates the receive queue with `dma_buffer_count - 1` slots. When the reader falls behind, the ISR drops the oldest buffer without logging anything.
- At 48 kHz with two 32-bit slots a frame (Task 14b), a 240-frame buffer is 5 ms and 1,920 bytes of internal RAM. `audio_in` uses 10, so it can run 45 ms late.
- The audio thread never waits for the recognizer: it `try_lock`s the 12 s ring and keeps its samples while the 12 s snapshot (about 60 ms) holds it (SDD Ruling R25).
- Core 0 still slows down: the start of each fingerprint (1–20 ms after the snapshot; it fills about 2 MB of fresh PSRAM buffers) put the thread 26–31 ms behind over 11 asks, and Wi-Fi start-up 15–25 ms at boot. Otherwise it ran at most 2.2 ms late. At 35 ms of queue the margin was 4 ms, hence 10 buffers.
- To catch a drop, count frames against `esp_timer` time: a lost buffer leaves a lasting 240-frame step. Task 14b's throwaway diagnostic does this (`target/task14b/diag.patch`, kept locally).

## On the classic ESP32, an I2S slot is as many bits as the data width
`hal/esp32/include/hal/i2s_ll.h` sets `rx_bits_mod` to the data width ("on ESP32, sample_bit should equals to data_bit"), while `i2s_std.c` computes BCLK from `slot_bit_width`. So 24-bit data in 32-bit slots would run at the wrong sample rate (read from the source, not tried). For 24-bit audio either use a 32-bit data width, as `audio_in` does (the sample arrives in the top 24 bits; shift right by 8), or 24-bit slots with MCLK at 384 x fs (`i2s_std.c` refuses 24-bit slots otherwise) and the codec's MCLK/LRCK ratio to match. On the board the low byte of each 32-bit slot reads 0: the ES8388 drives zeros after its 24th bit.

## The audio thread's decimator costs about 9% of core 0
Measured in Task 14b: about 440 µs per 240-frame (5 ms) chunk over a 5-minute run, against Task 14a's estimate of 1–2% of a core. The whole capture loop took about 0.8 ms per chunk (16%), the diagnostic's own counting included. The FIR is f32 on the FPU, but each of its 63 taps is a dependent `add.s` (Rust doesn't fuse into `madd.s`). Folding the symmetric taps or splitting the sum would cut it, if core 0 runs short.

## toml-cfg drops attributes and pastes values into Rust source
`#[toml_cfg::toml_config]` rebuilds the struct from field names and types alone. Doc comments and `#[allow]` on the struct or its fields vanish, so use `//` comments in `config.rs`. Each `cfg.toml` value is printed as TOML and parsed as a Rust expression. A string containing `"`, `\`, a newline or a control character comes out in a TOML form that isn't Rust (`'...'`, triple quotes, or TOML escapes). The build then fails, and its panic message prints the value: a password among them lands in `target/check-logs/firmware-build.log`. A float setting needs a decimal point (`-60.0`, not `-60`).
It also ignores unknown keys, so a misspelled key in `cfg.toml` silently falls back to its default.

## Boot logs show the Wi-Fi name and the password's length
ESP-IDF's Wi-Fi driver logs `wifi:connected with <SSID>` at every connect. With station OWE support on (`CONFIG_ESP_WIFI_ENABLE_WPA3_OWE_STA=y`, the IDF default), `libnet80211.a` also warns "OWE is enabled, but authmode threshold is 3 or password length is N", where N is the real length of `wifi_pass`. Redact both before pasting a log anywhere. Turning OWE off would remove the warning; that waits on Harper (plan `## Now`, Open).
Update: the password-length warning appears in Task 9's logs but in none from Tasks 10, 11 or 11b. Check a fresh boot log before acting on this.

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
mbedTLS allocates only from internal RAM (`CONFIG_MBEDTLS_INTERNAL_MEM_ALLOC`, the IDF default), and `mbedtls_ssl_setup` needs one contiguous block of about 16.7 KB for its 16 KB input buffer. Thread stacks come out of internal RAM too (ESP-IDF pthreads use `MALLOC_CAP_INTERNAL`), and malloc tries internal RAM first for anything up to `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` bytes.
- **The big consumer is the fingerprint, not TLS.** chfft's `RFft1D::new(2048)` keeps five 8 KB tables and a 4 KB index table. SongRec's generator adds 8 KB and 4 KB, and every FFT allocates 8,200 bytes. With the old threshold of 16384, all of that sat in internal RAM for the whole fingerprint. Internal free fell to 860 bytes during one, with no TLS session open, and to 5,472 bytes during another.
- **MQTT added to it.** Task 11's first build ran a pump thread and a publisher thread with 6 KiB stacks each and took 25.2 KB. One boot's first ask then failed with `mbedtls_ssl_setup returned -0x7F00` (`MBEDTLS_ERR_SSL_ALLOC_FAILED`).
- **Fixes, 2026-10-03 (SDD Ruling R19):**
  - `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=8191` sends the 8 KB FFT buffers to PSRAM.
  - The recognizer drops the signature and its URI before the HTTPS call.
  - `mqtt_out` uses `new_cb` (no pump thread), a 4 KiB publisher and a 4 KiB esp-mqtt task. That's 15.8 KB measured, minus the 2 KiB task-stack cut. The task used 2.1 KB at most.
- **Measured over 10 asks at 40 MHz flash/PSRAM while `/api/state` was polled every 3 s.** In `MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT`, which is what mbedTLS asks for:
  - lowest free since boot: 30,844 bytes
  - largest block before every TLS session: 47,104 bytes
  - TLS failures: none
  - Before the threshold change, the same figures were 860 and 25,600 bytes. Plain `MALLOC_CAP_INTERNAL` reads about 29 KB higher, because it also counts 32-bit-only IRAM that mbedTLS can't use.
- **The cost:** with PSRAM at 40 MHz, a fingerprint took about 16.5 s instead of 10.7 s. Flash and PSRAM at 80 MHz (SDD Ruling R20) bring it to about 10.2 s, and Task 14b's 48 kHz capture to about 11.1 s (see "A 12 s fingerprint takes about 11 s"). At 80 MHz, over 9 asks with the same 3 s poller, the lowest free was 31,496 bytes, the largest block before every TLS session was 47,104, and no TLS session failed.
- **Task 14b, 2026-10-04:** 48 kHz capture with a 2 s stream ring in PSRAM, 10 I2S DMA buffers of 1,920 bytes (19,200 bytes, against 15,360 before). The lowest `INTERNAL|8BIT` free was 30,056 bytes over five asks with 8 DMA buffers, and 36,064 over two asks with 10 (a short run). The largest block before every TLS session was 47,104, with no TLS failures. With Shazam off, the lowest was 95,044.
- **Task 15, 2026-10-04:** each stream listener costs about 20 KB at the low (a 4 KiB thread, 3.4 KB of buffers, lwIP's queue and the Wi-Fi driver's TX buffers), two about 45-50 KB (lowest per 10 s window, `heap_caps_monitor_local_minimum_free_size_start`; the since-boot low is Wi-Fi start-up's, about 97 KB, and hides it). Against the Shazam-on margin of about 16 KB, even one listener doesn't fit.
Measure the internal heap before adding anything that lives in internal RAM. If the margin shrinks again, letting mbedTLS use PSRAM (`CONFIG_MBEDTLS_DEFAULT_MEM_ALLOC`) removes the 16.7 KB need. ESP-IDF calls internal-only the more secure choice, though, so that is Harper's call.

## esp-mqtt holds its API lock through each connect attempt
`esp_mqtt_client_enqueue` and `_publish` wait for that lock, so a publish can block through a whole connect attempt while the broker is away: the TCP connect and the CONNACK wait take up to `network_timeout` (10 s by default) each. Publish only from `mqtt_out`'s own thread; the recognizer just sends on a channel. Callbacks from `EspMqttClient::new_cb` run on esp-mqtt's task, which needs no pump thread. `EspMqttClient::new`'s `connection.next()` does need one: without it, esp-mqtt stalls. Queued messages expire after 30 s (`OUTBOX_EXPIRED_TIMEOUT_MS`), and the publish on every connect covers longer outages.

## esp-idf-svc's MQTT error event always says ESP_FAIL
esp-idf-svc 0.53 maps every `MQTT_EVENT_ERROR` to a fixed `ESP_FAIL` (a TODO in its `mqtt/client.rs`), so `mqtt: ESP_FAIL (error code -1)` carries no cause. esp-mqtt's own `E (…) mqtt_client:` and `esp-tls:` lines just before it give the cause, for example `transport_read(): EOF` or `Connection reset by peer`.

## Reading the retained MQTT topic
- The retained message outlives the board. A parked or unplugged board leaves its last song on the broker, because there is no Last Will. After a reset, compare the payload with `/api/state`, or wait for the board's first publish (`null` right after it connects).
- A live subscriber gets the board's publishes with `retain=0`; only a new subscription shows `retain=1`. `mosquitto_sub -q 1 -F '%I retain=%r qos=%q %t %p'` shows the flags.
- The board reaches a broker on the Mac at its en0 address (192.168.99.151:1884), although the Mac reaches the board through Tailscale: the gateway routes between 192.168.99.0/24 and 192.168.200.0/24 (`ping -b en0 192.168.200.8`: 3–6 ms). Task 11's broker-restart test used that path.

## With shazam_enabled = false, `state` stays "quiet"; the page says "Shazam is off"
Only the recognizer thread sets `state` and `now`, so a board that doesn't start it reports `state: quiet` and `now: null` for good. `level_dbfs` stays live: since Task 14b the audio thread measures it every 250 ms, Shazam or not (SDD Ruling R24), so it shows whether audio arrives. The page reads `shazam_enabled` and shows "Shazam is off · <level> dBFS" in place of the state, and hides "Nothing playing.". Anything else reading `/api/state` should check `shazam_enabled` before trusting `state`.

## The flag-off image is smaller: only one path ships
`CONFIG.shazam_enabled` is a constant, so both branches of `if CONFIG.shazam_enabled` type-check and lint in every build, but the optimizer and linker drop the dead one. On 2026-10-03 espflash's "App/part. size" read 1,485,248 bytes with the recognizer and 1,258,816 without, and the flag-off build had no warnings. `scripts/check` builds whatever `cfg.toml` says, so it links only that image.

## `scripts/serial-log` execs into uv, so `pgrep -f serial-log` finds nothing
To wait for a backgrounded capture, match the python command line: `pgrep -f "usbserial-0001 <secs>"`. A wait loop on `serial-log` ends at once, and the capture is still running.

## "Flag out" means off on Harper's board
On 2026-10-03 Harper asked to "feature flag out the shazam bits" and go for the stream. The flag shipped defaulting to on, and Task 14b was briefed to test the Shazam path on the board. Harper corrected that on 2026-10-04: "i thought we had talked about putting that behind a feature flag and focusing on the streaming". When Harper flags a feature out, turn it off in `cfg.toml` and spend no device runs on it until Harper asks.

## ESP-IDF's usleep busy-waits anything under one tick
`CONFIG_FREERTOS_HZ=100`, so a tick is 10 ms. ESP-IDF's `usleep` (`newlib/src/time.c`) spins in `esp_rom_delay_us` for anything shorter, and Rust's `std::thread::sleep` calls `usleep` on ESP-IDF. A loop that waits should sleep at least 10 ms, as `stream.rs` does, or it burns its core.

## The LINE IN stream's throughput depends on the network path, and it swings
One listener needs 288,000 bytes/s (2.3 Mbit/s). lwIP's TCP send buffer (`CONFIG_LWIP_TCP_SND_BUF_DEFAULT`, 5,760 bytes) caps a connection at that much per round trip, so round trips must stay under 20 ms.
- From the Mac, round trips averaged 75-90 ms with Wi-Fi modem sleep on (ESP-IDF's default, `WIFI_PS_MIN_MODEM`: the AP holds the board's frames until its next beacon) and 14-34 ms with it off. The Mac is on Wi-Fi and reaches the board through Tailscale, so the board sees the subnet router (192.168.200.5) as the peer; `curl --interface en0` takes the routed path instead.
- On 2026-10-04 listeners sometimes fell a ring behind within 6-22 s and sometimes held for minutes, on the same image. The spec's Risks has the numbers. Test with `scripts/stream-check` by IP, and expect run-to-run variance.
- lwIP here has no per-socket send buffer. The Kconfig help mentions `TCP_SNDBUF`, but `sockets.c` leaves `SO_SNDBUF` unimplemented, so only the global default changes it.
- A full connection costs internal RAM of 3-4x its send buffer, because the Wi-Fi driver's dynamic TX buffers fill too: about 20 KB at the default 5,760 bytes, 43 KB at 11,520 and 70 KB at 23,040, at the low.

## A reset from lwIP doesn't reach a stalled peer
After a zero-window probe, lwIP's `snd_nxt` is one byte past what the peer has, and an abort (`SO_LINGER` 0) resets with `snd_nxt`. A peer with a closed window drops that reset (RFC 5961). Behind Tailscale's subnet router it's worse: the router closes its NAT state on the reset, so the peer's later packets never reach the board, and a curl stopped for 15 s hung about 9 minutes after it resumed, until its own keepalive gave up. `stream.rs` closes a dropped listener with a FIN instead. That keeps the peer's unsent audio (at most 5.7 KB of internal RAM) until the FIN gets through or lwIP's 12 window probes run out, about 7.6 minutes.

## Formatting a float to a precision takes 1 KB of stack
Rust's `{:.1}` on a float goes through `float_to_decimal_common_exact`, which puts a 1,024-byte buffer on the stack. On the 4 KiB stream threads, log integers.
