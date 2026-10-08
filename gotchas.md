# Gotchas

Doctor Biz confirmed on October 8 that the ESP32 LINE IN cable uses the
workstation's front jack, labeled Headphone by ALSA. For board-only input use
`plughw:CARD=PCH,DEV=0`, Headphone on and rear Line Out muted; Master and PCM
at 0 dB per the source-gain preference. Rear Line Out jack detection was off,
front Headphone detection on. Muting Headphone silences the board's source.
The verified 997 Hz left / 1703 Hz right fixture arrived as 1703 Hz left /
997 Hz right; channel reversal is an open calibration finding, not a buffer fix.

When investigating unexpected sound, inspect original playback commands before
attributing it. On October 8 this session launched a 180-second Solomun DJ excerpt
through `aplay -D plughw:CARD=PCH,DEV=0` at 09:23, 09:28 and 09:45 Chicago time,
as well as later test tones. The claim that only tones were launched was wrong.
The later packet harness writes framehash files, not speaker output; the source
of music reported during that later run remains unproven. A sandbox-local `ps`
cannot exclude host audio players. Verify process visibility and physical output
routing before claiming no player exists or starting another audible test.

Check `docs-ignore/victrola-hacks.tgz` before proposing Victrola-like buffering.
Doctor Biz supplied firmware and live-test notes there. On the tested Onyx build
0.43.179.0x2aa3460, wirelessAudioDelay passes to adchls:start as adchlsLatency;
only high was live-tested, and delay durations/buffer sizes remain unproven.
The documented native Sonos route uses 320 kbps MP3 at the highest quality setting;
its separate direct FLAC endpoint is Ogg-wrapped 24-bit FLAC. Do not assume its
Sonos reliability demonstrates the same bandwidth or buffering as our FLAC path.

On this Linux workstation, invoke serial-backed harnesses with
`ESPFLASH_PORT=/dev/ttyUSB0`; `scripts/serial-log` otherwise defaults to a macOS
device path. A collector failure before stream admission is an invocation failure,
not transport evidence. Doctor Biz's report of music from workstation speakers
does not establish what signal reaches the board's LINE IN; record routing as
unverified until the ADC measurement or physical path establishes it.

Known live FLAC/WAV endpoints must select their input demuxer before FFmpeg
startup checks. Constant production FLAC can take over 12 seconds to fill the
auto-detection probe despite timely audio delivery. Keep sample, CRC and
discontinuity checks unchanged; this fix does not explain ring-overrun failures.

Doctor Biz later confirmed LINE IN is connected to LINE OUT during the October 8
capture-publication acceptance session. Treat this as the workstation connection
discussed earlier. A later decoded prefix verified driven tones, and standalone
settling acquired ten complete trials. The short network capture itself failed;
tone presence does not clear continuity or calibrate gain. Earlier quiet-input
results remain valid only for their recorded signal conditions; the cable's actual connection time is unknown.

## Atomic capture history evidence — 2026-10-08

Raw capture publication now has one unique publisher and atomic sample payloads.
Readers validate stamps, generation, retained range and era after copying; Retry
scratch is unusable. Keep the actual protocol in concurrency tests: sequence
checks around non-atomic payload would still be a Rust data race. Bounded Loom
models support the ordering argument but do not prove arbitrary target schedules.

On the pinned Xtensa build, SeqCst sample operations use direct loads/stores and
MEMW without atomic helper locks. Linked read/publish still call ROM unsigned
64-bit division/remainder; those are arithmetic, not 64-bit atomics. Objdump can
render valid Rust text as data because of Xtensa property metadata. Inspect a
separate scratch copy and verify unchanged text bytes; never flash that copy.

Delayed-reader fixture events are retrospective: acquisition records numbers,
releases buffers/ring ownership, waits for recovery, then logs with bounded yields.
Match the event timestamp, not UART arrival, to capture counters. A failed soak's
surviving decoder can keep running after its summary snapshot; final retained
framehashes establish its eventual samples. Preserve the original failed verdict.

## Keep executing authorized work — 2026-10-08

Doctor Biz corrected another premature stop at a batch boundary. A committed
phase is a checkpoint, not permission to end while authorized acceptance work
remains. Update the plan and Kata, continue the next useful experiment, and stop
only for a real blocker or completion of the requested scope.

## Capture protection measurements — 2026-10-08

Retain the original UART collector after a failed soak: reopening this board's
serial port can reset it even with DTR/RTS configured before open. The optional
stream-soak failure observation keeps recording but leaves summary counters as
explicitly aged pre-failure snapshots. Zero stale deltas cannot clear capture.

The TLS fixture reproduced three capture overflows/restarts while all TLS cycles
completed. `raw_read_max_us` measures the encoder's raw-ring copy, not I2S driver
read time. Keep that distinction when tracing long capture service intervals.
Physical ALSA RUNNING also does not prove the signal reaches LINE IN: the attempted
music run still measured only noise at the ADC. Verify the board signal too.

Long diagnostic UART dumps can starve the idle task even after acquisition ends.
Explicitly yield between bounded batches; lowering priority alone is insufficient.
Retain watchdog-contaminated runs and reject incomplete/interleaved records.

TLS diagnostics reproduced two distinct capture losses. One run added DMA queue
overflows/restarts; another kept those counters fixed but lost 4,320 staged frames
while an encoder raw-ring read held its mutex for 110,567 us, including preemption.
The latter is direct evidence for removing reader-held locking from publication;
it does not explain or fix the former. See capture-setup.md/.json in audit evidence.

Cross-core task probes have costs and limits: the global tick advances on core 0,
task lookup/state queries take the kernel lock, and current-task identity does
not identify ISR execution. Actual capture priority was 10. Long observer gaps
also occurred in clean cycles, so do not equate them with capture starvation.

Identically instrumented IRAM-off controls measured capture-task runtime at
16.63% with unconditional FIR versus 8.40% with recognition disabled correctly,
using two 45-second FLAC listeners on quiet analog input. Runtime includes charged
interrupts; the read-service delay estimate also includes work and preemption.
Both retained startup capture faults. Do not turn the CPU saving into a claim
that TLS/setup capture stalls are fixed.

Pinned IDF I2S IRAM safety needs both IRAM interrupt registration and internal
driver objects; callback placement alone is insufficient. One- and two-listener
90-second tests each survived 120 verified NVS writes without additional capture
faults. Exact decoder samples do not prove every DMA interrupt arrived. See
docs/audit-evidence/2026-10-08/capture-protection.md for scope and hashes.

Settling counters count discarded whole chunks, including interrupted budgets.
Restart counters count attempts, and supervisor/overflow causes can coincide.
The fault fixture measured 24,000 extra settling frames after recovery, but its
stream never exposes those discarded samples. It cannot justify shortening the
500 ms policy; cold codec setup and isolated I2S restart need separate physical
measurements before changing that policy.

## Linux workstation validation — 2026-10-07
Doctor Biz's correction: a failed diagnostic is a step in the investigation,
not a stopping point. Continue useful, authorized experiments and root-cause
work rather than ending each turn with a measurement and another proposal.

Remote-session changes can remove physical audio access: the analog card still
exists in /proc/asound, but its ACL may grant only the local greeter. PipeWire
then exposes Dummy Output; pw-play can silently route there even with an absent
named target. Check physical sink existence and actual playback links before
calling a run a cable/music test. A live player process alone proves nothing.

After restoring the analog control/playback ACLs, PipeWire still exposed only
Dummy Output in the remote session. Direct ALSA plughw:CARD=PCH,DEV=0 playback
ran on the real device and the two-listener120s FLAC retry passed. Verify the
PCM RUNNING state and board level. This repairs the test audio path; a single
pass with unchanged firmware does not explain the earlier network failures.

Operator scripts must not assume the agent's PATH. The first correlated-capture
runner used `rg`, which existed for the agent but not Doctor Biz's terminal.
Use standard `grep` for these simple checks and validate dependencies before
route checks or privileged work. Missing tools are not evidence of a bad route.

The workstation's `eno1` profile is link-local only; plugging into it does not
give a LAN route. `enp5s0` uses DHCP and acquired 192.168.23.153. Verified board
traffic used Ethernet before and after the two-listener comparison. It still
failed: a status request timed out at 49.424 s, with only 17.493/16.725 s decoded.
The harness then stopped the streams; the UART peer reset is cleanup, not a
natural ring-lag disconnect. This verified the Ethernet outgoing route only;
Wi-Fi remained connected on the same subnet, so the incoming path was not
isolated. A later interface-bound test raised an ARP-flux hypothesis. Do not
claim an Ethernet-only comparison until repeating with workstation Wi-Fi off.
Evidence: `docs/audit-evidence/2026-10-07/ethernet-comparison.json`.

The network-only comparison was initially misclassified: both runs actually used
b/g, including the intended normal-mode control. `esp_wifi_set_protocol` survived
source restoration and reflashing on this board. Verify protocol readback AND
the association's `phy:` log; matching an ELF hash does not restore persistent
device state. Explicitly restore bitmap7 for b/g/n before repeating the control.
See the corrected `docs/audit-evidence/2026-10-07/wired-radio-comparison.json`.

With workstation Wi-Fi off and b/g/n independently verified, the plain TCP
fixture still loses a peer at full rate. The paired production capture records
zero kernel drops, open receive windows and multi-second sequence holes; both
listeners naturally leave for ring lag. Shared pauses support checking the
ESP/AP boundary but do not locate the fault. Half-rate peers both finish, yet
one falls about seven seconds behind by host-second bins. A lower offered rate
is diagnostic evidence, not a verified streaming fix. Curated evidence:
`docs/audit-evidence/2026-10-07/wired-correlated-capture.json` and
`docs/audit-evidence/2026-10-07/wired-radio-comparison.json`.

The faster household SSID is separate from the 2.4 GHz network by design;
Doctor Biz cannot enable 2.4 GHz on it. The classic ESP32 cannot join its 5 GHz
service. A failed association is not a failed throughput comparison. Save a
private credential backup before temporary network changes; a saved firmware
ELF can restore the board but does not restore the editable build config.

Doctor Biz connected this workstation's analog output to the board's LINE IN and
supplied test music under `media-ignore/`. This supersedes the older Mac/turntable
input assumptions for this setup. USB identifies a CP2102 at `/dev/ttyUSB0`;
device access requires a local permission grant. The old board IP is not confirmed
on this network. Keep Wi-Fi credentials in the gitignored `firmware/cfg.toml`.

The pinned espflash release must select the host platform. A hardcoded macOS ARM64
asset installs successfully on Linux but cannot execute. Leaving asset selection
to mise installed a working Linux espflash 4.6.0. Use `mise run esp-toolchain` for
the pinned Xtensa toolchain; the host Rust toolchain alone cannot build firmware.

The updated image joined at `192.168.23.217`. All canonical checks passed after
workstation setup (188 Rust tests passed, two existing tests ignored). Physical
music playback produced valid 48 kHz/24-bit FLAC; a single listener decoded a full
120 s with unchanged post-startup capture fault counters. Two listeners failed:
one fell a ring behind after 47.7 s, reproducing transport trouble on direct local
Wi-Fi without the prior Mac/subnet-router path. This does not locate the loss.
The short pass is not an endurance or worst-case bandwidth guarantee.

The supplied DJ-set excerpt has identical left/right channels. Separate 440 Hz
left and 880 Hz right test tones returned reversed, with over 60 dB separation.
Doctor Biz confirmed a single 3.5 mm stereo cable, without RCA adapters.
PipeWire links preserve channel order and the codec/I2S settings match documented
defaults. The location of the reversal remains unresolved; do not blindly flip
`msb_right` to compensate. Evidence: `docs/audit-evidence/2026-10-07/linux-line-in-validation.json`.

A selective codec gain test localizes the reversal before the ADC: register
0x09 = 0x40 raised the stream's left channel by 12.74 dB, leaving right unchanged.
The left stream still contained the source's 880 Hz right tone. Codec-to-stream
channel mapping is correct; workstation analog output, cable and board input
wiring remain candidates. Normal firmware was restored and its ELF hash matches
the saved control. Do not change I2S ordering to conceal an analog routing issue.

Doctor Biz cannot select answers in the current TUI. Use plain chat for operator
questions rather than selectable question widgets. Verify captures from actual
processes and files; an answer alone is not evidence that a command ran.

The Linux two-listener packet trace confirms receiver sequence holes and pauses
even after ACKing all observed data. Valid receive windows never reached zero;
the two zero fields were cleanup resets. Local ACK generation is fast, but this
does not prove ACK arrival at ESP32. Only listener 2 naturally closed for ring
lag (27.9 s); listener 1 ended during harness cleanup. Initial capture lacked
tcpdump drop statistics, now retained by the script. Host receive coalescing
also means packet records are not on-air segments. Do not infer RF loss location
or sender silence from this one capture. Next discriminator: simultaneous AP
boundary captures; a wired workstation comparison could first remove one radio
leg. See `docs/audit-evidence/2026-10-07/linux-stream-packet-timing.json`.

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
- **Task 14b, 2026-10-04:** 48 kHz capture with a 2 s stream ring in PSRAM, 10 I2S DMA buffers of 1,920 bytes (19,200 bytes, against 15,360 before). The lowest `INTERNAL|8BIT` free was 30,056 bytes over five asks with 8 DMA buffers, and 36,064 over two asks with 10 (a short run). The 10-buffer build's worst case is the five-ask low less the two added buffers' 3,840 bytes, about 26.2 KB, which supersedes the short run's 36,064. So the margin above the 10 KB floor with Shazam on was about 16 KB (15,976 bytes with a 10 KiB floor) before the stream server, which takes some of it even with no listener (next item). The largest block before every TLS session was 47,104, with no TLS failures. With Shazam off, the lowest was 95,044.
- **Task 15, 2026-10-04:** one stream listener costs 17-25 KB at the low (median 21 KB over 11 windows), two 41-55 KB (median 48 KB over 34 windows): a 4 KiB thread each, 3.4 KB of buffers each, lwIP's queue and the Wi-Fi driver's TX buffers. Measured as each run's median idle low minus each window's low with only listeners present, per 10 s window (`heap_caps_monitor_local_minimum_free_size_start`; the since-boot low is Wi-Fi start-up's, about 97 KB, and hides them). **The server costs internal RAM even with no listener:** idle free before any listener was 125,131-125,307 bytes in Task 15's runs, against 130,967 in Task 14b's flag-off run on the same capture settings, so about 5.7-5.8 KB (the accept thread's 4 KiB stack and TCB, the listening socket, the mDNS service, the larger socket table). After the first listener, idle free settles another 0.5-1 KB lower, once rather than per connection (run D's 13 connections cost 928 bytes, run F's one 548). **So with Shazam on, the margin above the 10 KB floor is about 9-10 KB with the server idle** (15,976 less 5.8-6.8 KB), and even one listener doesn't fit.
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

## Historical Task 15 throughput settings and measurements
The following measurements used the Task 15-era 5,760-byte send buffer and dynamic TX buffers; they are not the current profile. One raw WAV listener needs 288,000 bytes/s (2.3 Mbit/s). lwIP's TCP send buffer (`CONFIG_LWIP_TCP_SND_BUF_DEFAULT`, 5,760 bytes) caps a connection at that much per round trip, so round trips must stay under 20 ms.
- From the Mac, round trips averaged 75-90 ms with Wi-Fi modem sleep on (ESP-IDF's default, `WIFI_PS_MIN_MODEM`: the AP holds the board's frames until its next beacon) and 14-34 ms with it off. The firmware turns it off since SDD Ruling R27 (`net::start_wifi`). The Mac is on Wi-Fi and reaches the board through Tailscale, so the board sees the subnet router (192.168.200.5) as the peer; `curl --interface en0` takes the routed path instead.
- On 2026-10-04 listeners sometimes fell a ring behind within 6-22 s and sometimes held for minutes, on the same image. The spec's Risks has the numbers. Test with `scripts/stream-check` by IP, and expect run-to-run variance.
- lwIP here has no per-socket send buffer. The Kconfig help mentions `TCP_SNDBUF`, but `sockets.c` leaves `SO_SNDBUF` unimplemented, so only the global default changes it.
- A full connection costs internal RAM of 3-4x its send buffer, because the Wi-Fi driver's dynamic TX buffers fill too: about 20 KB at the default 5,760 bytes, 43 KB at 11,520 and 70 KB at 23,040, at the low.

## Historical Task 15 stalled-peer reset and FIN retention
The following measurements used the Task 15-era close policy; they do not describe the current checked positive-linger path. After a zero-window probe, lwIP's `snd_nxt` is one byte past what the peer has, and an abort (`SO_LINGER` 0) resets with `snd_nxt`. A peer with a closed window drops that reset (RFC 5961). Behind Tailscale's subnet router it's worse: the router closes its NAT state on the reset, so the peer's later packets never reach the board, and a curl stopped for 15 s hung about 9 minutes after it resumed, until its own keepalive gave up. The Task 15 image closed a dropped listener with a FIN instead. That keeps the peer's unsent audio (at most 5.7 KB of internal RAM) until the FIN gets through or lwIP's 12 window probes run out, about 7.6 minutes.
- **They pile up.** A dropped listener's PCB, about 6 KB of internal RAM with its queued audio, outlives its slot by up to 7.6 minutes. It goes when the peer reads again (and gets the FIN) or closes. A player that keeps its stalled connection open while it reconnects (pause, dropped, reconnect, pause again) leaves one more each time: three such cycles within 7.6 minutes hold about 18 KB. This was the unfixed Task 15 state on 2026-10-04; it mattered most with Shazam on.

## Current transport profile and measured limits
Current source sets a 46,080-byte TCP send buffer, static TX16 and cache32,
with the prior RX/reordering bounds preserved. Listener close uses two-second
positive linger and retains the listener slot until checked close; an
unrecoverable close reboots under Doctor Biz's approved policy. Queued payload
and control PCBs remain separate resource measurements.

The real FLAC/WAV trickle collection returned both ports to LISTEN-only by log
time 214.921 s and restored the largest PSRAM block to 917,504 B. This short run
proved sampled reclamation, not a universal lifetime bound. Direct draining still
lost one connection to generation overrun with nonzero window and ACK/retransmit
stalls; the six-hour soak failed. Normal routed playback remains unverified.

The historical successful ten-minute runB used dynamic TX32, unlike static16/cache32.
IDF 5.5.5 allows dynamic TX with PSRAM preference only when IGNORE_NOTFOUND is on.
Despite its Kconfig prose, that flag does not disable allocator PSRAM preference:
lwipopts.h and esp_adapter.c gate that on TRY_ALLOCATE alone. It disables the TX
cache/by-reference path. A temporary dynamic-profile experiment must verify mapped
PSRAM; production still needs an explicit boot-memory requirement. Fresh static
tests on October 5 passed one listener for 120 s but lost one of two to ring lag
after 41.9 s. The dynamic/no-cache/copy profile also lost one of two after 71.1 s
and used more internal memory; it was rejected and static defaults restored.
One short pass cannot establish a transport fix.

An ESP-only diagnostic retried nonblocking sends after 20 ms poll slices while
preserving the absolute frame deadline. It still lost a listener to ring lag
after 85.4 s and was reverted. Do not reintroduce it as a proven low-water fix.
Large en0-bound DF ICMP probes answered, so no MTU black hole was observed in
that small sample; TCP delivery still needs packet evidence.

The separate Wi-Fi-only fixture also hit scheduled write deadlines with two
135,168 B/s peers, no audio/application workload and 181,675 B internal free
before admission. Audio load is not necessary for the stall. Native Mac TCP
counters showed out-of-order received bytes during a production reproduction;
they do not locate the loss. A relay connect-timeout failure ends downstream
HTTP without a response; distinguish that from a board handler closing it.

A native macOS default/more/default ACK-policy comparison delivered about 120 s
of valid audio in its first two runs, then lost both default peers around 84 s
with receive windows still above 64 KB and flat capture-counter deltas. The
control both passed and failed: TCP_SENDMOREACKS is not a demonstrated fix.
Public TCP_CONNECTION_INFO has no duplicate-byte field; offset 104 counts
transmitted retransmission packets. Its TX packet count excludes pure ACKs,
and RX packet counts can reflect coalesced input buffers, not wire segments.

Verify a capture by its process and file, not a dialog selection. Doctor Biz
may be away from the Mac; selecting a response does not run a Terminal command.
Owned-socket TCP statistics work without BPF administrator access, but cannot
locate where missing or reordered data disappears along the network path.

## Formatting a float to a precision takes 1 KB of stack
Rust's `{:.1}` on a float goes through `float_to_decimal_common_exact`, which puts a 1,024-byte buffer on the stack. On the 4 KiB stream threads, log integers.

## The Mac idle-sleeps through long device tests
On 2026-10-04 the Mac turned its display off at 14:27:08 and went to idle sleep at 14:27:57, then cycled between dark wakes of about 45 s and maintenance sleeps of about 5 minutes (`pmset -g log`). Processes froze in between: a 60 s stream capture stopped after 20 s of audio, its `/api/state` poller recorded nothing for 5 minutes at a time, and ffmpeg then waited 15 minutes on a connection the board had already dropped. Run long device tests under `caffeinate -is` (it holds off sleep for that one command). `scripts/stream-check` gives ffmpeg a 10 s I/O timeout (`-rw_timeout`), so a stalled stream fails the check instead of hanging it.

## A pyserial read with DTR and RTS set false still reset the board
On 2026-10-04 a read meant to leave the board running (pyserial, `dtr = False` and `rts = False` set before `open()`) logged `rst:0x1 (POWERON_RESET)` and a fresh boot, and the read itself took over 2 minutes instead of 25 s while the Mac was sleeping on and off. The mechanism isn't pinned down. Don't count on reading the serial log without resetting the board.

## Sweeping a big PSRAM buffer costs far more than reading it once
The ESP32 reads flash and PSRAM through one cache, 32 KB per core (`memory-types.rst`: "the first 64 KB block which is used for PRO and APP MMU caches"). ESP-IDF warns that data over 32 KB falls back to PSRAM speed and can "push out" cached flash, slowing the code too (`external-ram.rst:225`). On 2026-10-04 the FLAC encoder's first version took 19.4 ms per block with its 32 KB block in PSRAM against 10.5 ms in internal RAM. A plain sequential read of the same 32 KB took 1.2 ms from PSRAM and 0.7 ms from internal RAM, but the encoder swept the block four times while writing its output. It also slowed core 0: the audio thread's decimator went from 434 to 562-640 µs per chunk. Work on data while it sits in internal RAM, make one sequential pass over PSRAM (as `flac::Encoder::push` does), and time it on the board: the Mac ran the same code 120-250 times faster.

## Core 1 is shared: a slow stream thread can stall capture through the ring's lock
Stream listener threads run on core 1 at priority 7. When the first FLAC encoder (43-99 ms per 85 ms block) oversubscribed core 1 with two listeners, a listener was kept off the CPU while holding the stream ring's lock. The audio thread then waited 37.4 ms for it, and capture ran 35.6 ms late of its 45 ms queue (Task 16, run A). Keep per-listener work on core 1 well under real time; the encoder now takes about 14 ms per block.

## Reading the firmware's machine code
The esp-14 toolchain's GNU `xtensa-esp-elf-objdump` prints raw words for the Rust code, even with `XTENSA_GNU_CONFIG` pointing at `lib/xtensa_esp32.so`. To read what LLVM made of a crate, have rustc write assembly from the firmware directory: `CARGO_TARGET_DIR=<scratch> cargo +esp rustc -p esp-shazam-core --release -- --emit asm -C opt-level=3`, then look in `<scratch>/xtensa-esp32-espidf/release/deps/*.s`.

## The ESP32 has no 64-bit atomics
`std::sync::atomic::AtomicU64` doesn't exist on `xtensa-esp32-espidf`: the build fails with "cannot find `AtomicU64` in `atomic`". Use `AtomicU32`, or a mutex.

## espflash can fail with "Bad data checksum"
On 2026-10-04 one `scripts/flash` failed with `Error while running FlashDeflData command ... Bad data checksum`, a transfer corrupted on the way over the CP2102. The board then holds a partial image and boot-loops until a flash goes through. The retry worked at the same settings.

## Tell Harper the moment a feature works on the board
On 2026-10-04 `/stream.flac` played on the board at about 16:45, but the agent kept optimizing, soaking and documenting until 18:30, and Harper, who was waiting to test, heard nothing for over two hours ("you have been spiraling again?"). A background implementer should `SendMessage` the main session as soon as the feature first works on the board, and the main session tells Harper right away. Polish and measurements follow. On long runs, send Harper a one-line status every 30 minutes or so.

## Kata audit work can overlap another live session
On 2026-10-04 another session changed firmware and committed `ee68892` and `cf42331` while the Sol 6.1 agents worked on constant FLAC subframes. Keep file ownership explicit and coordinate before flashing. The board responds at `192.168.200.224`; `192.168.200.8` is the MQTT broker.

## Five audit tickets concern archived diagnostics
`hb9p`, `sznz`, `b69f`, `3db1`, and `vya4` concern the temporary module saved under `target/task15b/` and `target/task16/`. It is absent from current firmware. The audit retired these tickets as wontfix for obsolete diagnostics; it did not fix their bugs. Repair and verify any diagnostic before reusing it, and do not cite unsafe archived measurements as validation. Production capture/CPU/heap telemetry and separate hardware examples cover current tests; they do not supply linkoutput error counts, a raw throughput blast, or controlled encoder working-buffer placement comparisons. The Wi-Fi transmit-resource investigation remains open in `23mz`.

## Kata audit, 2026-10-04

The machine's global Git configuration rewrites public GitHub HTTPS URLs to SSH.
When esp-idf-sys refreshes ESP-IDF, that rewrite can make its URL check reclone the
SDK and fail SSH authentication. `GIT_CONFIG_GLOBAL=/dev/null scripts/check`
(and the same scoped variable for `scripts/flash`) uses public HTTPS without
changing global Git configuration.

A per-send `SO_SNDTIMEO` does not enforce a whole-frame deadline on macOS: one
send exceeded 700 ms against a 400 ms timeout while a peer drained in bursts.
`DeadlineWriter` now uses nonblocking sends and `poll` against one absolute
deadline, then restores the exact original flags. Callers must exclusively own
the socket and its clones during each write.

Doctor Biz corrected the audit's physical source on 2026-10-04: LINE IN is a player
streaming FIP, not the turntable from older notes. Audit playback measurements use
real FIP music. They do not establish worst-case incompressible-input throughput.

Hardware fault checks must include the fault setup interval in their capture-counter
baseline. A pre-READY baseline can hide loss during allocation; sample before START.
Later playback alone does not prove supervisor recovery: require its restart request,
the capture restart counter, and decoded audio before testing the separate reboot path.

On the 2026-10-04 overnight audit, both two-WAV and one-WAV runs through the Mac's
utun4 route hit the raw ring's 2 s write deadline. A single stream failed before
fault injection, with capture counters stable. Do not treat this route as a
reliable WAV freshness test or claim worst-case two-stream throughput from FLAC
music. Keep real loopback contract tests separate from routed playback evidence.

The fault harness originally used a 4 s ffmpeg read timeout, shorter than the
8.02 s FLAC write allowance. Use the existing stream-check's 10 s read timeout
while retaining completion/error assertions; that correction does not explain
the underlying routed stall.

On-device loopback TCP separated WAV freshness from that routed limit: both real
WAV peers received EOF at about 2.03 s during capture suspension, then both slots
admitted fresh clients. A separate real Wi-Fi disconnect/reconnect left capture
fault counters unchanged. Keep those contract tests distinct from network endurance.

Fallible pthread creation is not a complete internal-OOM guarantee: Rust allocates
thread Arc/Box metadata before the IDF call, and task stacks share the internal
reserve with DMA. PSRAM exhaustion tests prove strict frame placement, not safety
under arbitrary internal-heap exhaustion. Do not invent a reserve threshold from
one observed free-memory sample.

A full capture staging buffer must try the destination mutex before discarding
old samples. If the ring is available, flush staging and the incoming chunk in
order; the staging limit bounds contention, not delivery. A dropped-buffer count
does not establish one equally long lock hold: backlogged reads can collide often.

ESP-IDF Rust logs are synchronous and can wait indefinitely on filter/FILE/UART
locks. Default UART output also busy-waits FIFO space. High-priority health
checks must publish recovery actions independently of logs; moving only ordinary
telemetry leaves rare recovery logs capable of blocking the recovery itself.

Hardware fault fixtures must not log while a suspended task might own a logger
lock: a RAII resume guard cannot run while the observer itself is blocked. Save
event times, resume first, then report. Base task priority alone cannot make a
state-check/suspend pair atomic because the target can inherit higher priority.
The encoder fixture uses a short, pinned-IDF-verified local scheduler suspension.

Default ESP-IDF log prefixes and esp_timer_get_time marker times have different
origins. Sample esp_log_timestamp and esp_timer together before logging, then
convert clocks for comparisons; a UART marker's receipt is not exact clock sync.

Live UART files can end midway through a telemetry line while a producer writes.
Only LF-terminated byte lines are observations; decode each completed line after
framing. The overnight isolated health test falsely admitted an unfinished second
post-reset sample and failed on its absent read_errors field. Keep that failed
run, fix the reader with a real-file regression, then rerun the hardware check.

Read each decoder's stderr with its filename attached. Concatenating two files
without labels made the overnight soak's one errored decoder look like two.
Listener 2 had premature HTTP EOF, demux I/O and then invalid residual; listener 1
had empty stderr. Log byte counts can verify that attribution without guessing.

A whole-frame send deadline does not make TCP delivery atomic. The bounded raw
tee diagnosis received three CRC16-valid FLAC frames then 1,980 bytes of a fourth;
that partial tail matched the other peer's complete frame, and complete prefixes
decoded cleanly. ffmpeg reported invalid residual only when it read the truncated
tail. Check complete-frame CRCs and decode the prefix before blaming the encoder.

Pinned ESP-IDF 5.5.5 stores and reads the heap failed-allocation callback through
a plain global function pointer. One registration owner and allocator thread
safety prose do not establish safe late publication to concurrent callers.
The pressure fixture uses direct real allocation probes and actual admission
errors instead. Verify pinned source before adding a diagnostic callback.

An early post-reset level of -120 dBFS does not prove that the physical source
went silent. Keep the reading's time and compare later meter samples and decoded
RMS. The output probe began at -120 then read -86/-84.6, while decoded audio RMS
was about -82.42 dBFS and encoded frames stayed roughly 11 KB.

The pinned single-pbuf STA linkoutput path has no coalescing allocation before
Wi-Fi transmit, so its ERR_MEM result proves actual driver ESP_ERR_NO_MEM.
The output observer counts all STA ports; this does not identify a proprietary
pool or a rejected stream packet. A chained-pbuf ERR_MEM can also come from the
coalescing allocation, so keep those counters separate.

On this Mac, setting TCP_MAXSEG before connect does not clamp advertised MSS:
connection setup replaces it. TCP_NOOPT does suppress SYN options and lowered
the peer MSS in a real loopback test, but it also removes other TCP options.
Use that only as a labelled diagnostic, not as proof of a path-MTU fault.

Verify the board's actual MSS after client-option experiments. TCP_NOOPT was set
before connect in this audit, but every observed board connection still used
1440 bytes; the experiment never established smaller segments on that path.
Per-socket IP_BOUND_IF=25 on macOS can scope an upstream socket to en0 without
changing system routes. Both direct en0 stream sockets held a 60-second bound;
default-route reliability remains separate. A local relay adds buffering, so
label relayed results and do not reuse them as direct slow-reader acceptance.

The unrecoverable socket-close branch must reach restart without logging first:
the synchronous logger can wait on another task's lock. Retain a named failure
binding and the listener slot through the nonreturning restart; a wildcard can
drop a still-owned socket before recovery. Removing this log loses its diagnostic
signature. Reset observations alone do not identify the cause, and ESP-IDF's own
shutdown handlers still prevent an unconditional restart-time guarantee.

Pinned binary/source review maps the by-reference single-pbuf allocation failure
to an empty preallocated class 9 TX-cache free list; runtime does not allocate
another object on that branch. Why occupied slots fail to retire remains open.
Cache 32 → 64 → 32 repeats both had zero errors while transport still failed,
so keep cache 32 and do not infer a fix from one larger-capacity observation.
The extra 32 slots reduced sampled PSRAM free by 54,400 B, with no sampled
internal decrease; setup still permits internal fallback.


Held sockets are not proof of realtime playback. Match CRC-valid frame ends to
actual cumulative receive timestamps: the en0 control held 60 s but delivered
only 14.7627 s PCM in its final 20 s, with 128 driver ERR_MEM results. Preserve
late delivery rate and raw tails alongside socket lifetime. Its unchanged
default-route repeat lost both peers to ring lag; en0 success does not satisfy
routed acceptance or establish the transport cause.


The en0 cache64 control removed observed driver errors but both peers still
fell a ring behind near 22 s and delivered only10.752 s PCM. Earlier cache 32 held
60 s with 128 errors. Cache-error counts and playback acceptance measure different
things; neither zero errors nor a larger cache establishes a fix. Keep cache 32.


Final en0-relayed production checks also failed: two peers lost API response
within 32.159 s; one peer naturally fell a ring behind after 71.3 s and decoded
59.050667 s. Per-socket interface control is useful evidence, not a proven
solution. Keep state timeouts, natural UART close reasons and cleanup decoder
errors distinct, and do not launch a long soak from socket lifetime alone.


Read HTTP framing completion rather than waiting for TCP EOF. A final en0 state
probe received all 240 chunked-response bytes but an EOF-sensitive throwaway
collector called it a timeout. The canonical framing-aware probe then passed
in 828.055 ms with Shazam false. Preserve the collector failure separately; it
is not proof of an API blackout.

Doctor Biz confirmed audible host FLAC on Altec + 3, then requested board-only
playback tests. Keep the workstation as the physical analog source; Sonos must
fetch the ESP32 stream. Latest authorized listening volume is 15%.

For board listening, the workstation Master mixer at56/87 attenuates line out by
23.25dB. A Sonos PLAYING state and advancing position did not mean Doctor Biz
could hear the board: decoded audio measured about-43dBFS RMS. Raising Master
to72/87 (-11.25dB) raised decoded RMS to-30.77dBFS, peak-18.85dBFS, with zero
clipped samples in that check. Keep speaker volume and analog-source gain
separate in test reports. Doctor Biz confirmed the board music sounds great
with Master72/87 and Sonos15%; retain those settings for listening.

Sonos eviction events carry session identity in the header; the observed body
contains only the object type, ERROR_SESSION_EVICTED, and reason. Validate the
header against our owned session before decoding/applying ownership loss. A
foreign session created without loading media returns ERROR_COMMAND_FAILED on
suspend even with the board uninvolved. Close that unused test connection and
observe playback independently; do not mistake its cleanup failure for retake.

Doctor Biz corrected the test-source gain preference: use workstation line out
at unity gain, with both ALSA Master and PCM at 0 dB. This supersedes the earlier
Master72/87 listening setting. Control listening loudness at Sonos rather than
attenuating the analog test source. Unity gain does not certify an analog voltage.

Doctor Biz requested undoing Sonos test changes after confusing low-volume
behavior. Restored the saved pre-test group volume of 34% and disabled board
autoplay; verified current non-board playback continued unchanged. Individual
speaker volumes were not saved, so exact original speaker balance is unknown.
Do not reapply the old 5/10/15% test caps. Read live Sonos state before claiming
its volume or source; the later live group was at 49% on a non-board session.

Doctor Biz corrected the quiet-audio investigation: audit attenuation in our
chain before blaming the source or asking for its model. A 0 dB codec setting
is not proof of jack-to-PCM gain. The 2026-10-07 review matched 113,760 live
FLAC samples exactly to raw WAV after bit-depth conversion and found positive
channel correlation, but ADC mute/ALC/noise-gate readback and physical input gain
remain unverified. See docs/audit-evidence/2026-10-07/quiet-input-review.md.
Old Sonos test scripts still contain 5/15% volume writes; do not rerun those
scripts unchanged after Doctor Biz requested undoing test volume changes.

The quiet-input review squad found that diagnostic flashing would reset the
codec state under investigation. Preserve evidence first; post-reset readback
cannot clear a pre-reset fault. A known signal with measured jack voltage is
the missing absolute gain reference. The old generated host FLAC fixture uses
-24 dB attenuation and must not serve as a unity-gain comparison. See
docs/audit-evidence/2026-10-07/quiet-input-squad.md for ranked findings and tests.

FLAC validation must count decoded samples, not FFmpeg's `out_time_us`: timestamps
can span omitted blocks. A valid stream prefix can also lose its final block
without a CRC error or frame-number gap, so a symmetric duration tolerance can
hide loss. Keep exact sample counts for output-duration decoders; packet-copy
captures may include a final whole packet, but must never accept underflow.

A thread may start successfully before resetting the caller's pthread settings
fails. Keep creation and cleanup outcomes distinct: returning a retryable launch
error after success can create duplicate workers. Log cleanup failures separately
and retain startup resources only when creation actually failed.

Doctor Biz confirmed LINE IN was disconnected during the earlier October 8
quiet-input settling and TLS experiments. Physical ALSA playback does not prove
signal at the ADC. Those quiet soft-reinit trials included a transient through
about 420 ms. Later connected-input trials verified driven tones and measured
soft-reinit anomalies through 490 ms; retain the 500 ms policy. Neither set proves
physical cold-start behavior or calibrates gain.
