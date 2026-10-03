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
