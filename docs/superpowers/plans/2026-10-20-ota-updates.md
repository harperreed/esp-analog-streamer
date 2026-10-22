# Over-the-air firmware updates

## Now
- Step: landed and in use. Home board on `e68d1f7e…` (System page at
  `/system`, MQTT removed in `3b99cb3`) after five over-the-air updates
  tonight, each validated. Earlier: `bb1a7945…` via OTA (7 s push),
  the third over-the-air update of the night after the round trip and the
  rollback proof (`docs/audit-evidence/2026-10-21/ota.{md,json}`). Memory
  finding on the way: Sonos playback died at 6.6 KB internal free; MQTT
  (about 30 KB with a broker URL) is now off by config on both boards and
  the OTA health thread exists only for a pending slot; Sonos now plays
  with 24.5 KB free (ticket g5x6 carries the budget). Sonos is opt-in,
  the page leads with the stream and a browser player, format and buffer
  changes are allowed while Sonos is off, history hides and the status
  line reports audio presence when recognition is off.
- Next: g5x6 (PSRAM stacks for flash-free tasks, a 24 KB floor flagged
  on the telemetry line; the stack table is in
  `docs/audit-evidence/2026-10-22/stack-headroom.md`); rollback on a
  hang; the S3 port (p6s9) when the parts land. The office board is on
  the OTA layout since 2026-10-21 and took its first OTA on 2026-10-22.
- Open: none
- Approved: "can we add OTA firmware updates" and "Shared token in cfg.toml
  (Recommended)" (2026-10-20)
- Compactions: 0

## Why
The second board lives at Doctor Biz's house; every fix so far has needed
a USB cable and a BOOT+RST dance. The input-gain image is waiting on
exactly that.

## Design
- **Layout**: `nvs 0x6000, phy 0x1000, otadata 0x2000, ota_0 <S>, ota_1 <S>`
  with `<S>` 64 KB aligned and at least 15 percent above the image, which
  decides whether trims are needed (see Sizing). The change of layout needs
  one last USB flash per board; after that every update goes over HTTP.
- **Upload**: `POST /api/ota` with header `X-Ota-Token: <token>` and the
  raw app image (`espflash save-image` output, the same bytes the USB
  path writes) as the body, `Content-Length` required, streamed into the
  update slot through `esp_idf_svc::ota::EspOta` in chunks sized to the
  httpd stack. Before the first write the image header is checked (magic
  `0xE9`, the embedded app descriptor's project name, and the version
  string is reported back). The token comparison is constant time and
  the token comes from `cfg.toml` (`ota_token`, empty disables OTA).
  Responses: 202 with the new image's app description, 401 on a bad or
  missing token, 400 on a bad image, 409 while another upload runs, 413
  over the slot size, 500 on a write failure (slot abandoned, running
  image untouched). The board restarts one second after the response.
- **Rollback**: `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`. A freshly
  booted image is pending until it has joined Wi-Fi, taken a clean
  capture baseline and answered one HTTP request, at which point it marks
  itself valid; a panic, watchdog or reset before that boots the previous
  slot. `GET /api/ota` reports the running slot, the app version and
  sha, and whether the slot is still pending.
- **Push**: `scripts/ota-push HOST IMAGE` (token from `OTA_TOKEN` or
  `firmware/cfg.toml`), builds nothing, verifies the header locally,
  streams the body, waits for the board to come back and compares
  `/api/metrics` `build_elf_sha256` with the ELF it was built from.
- **Out of scope**: pulling from a URL, signed images (a later ticket),
  updating the bootloader or partition table over the air.

## Sizing
| Build | app.bin |
| --- | --- |
| release (opt-level s, core at 3) | 1,957,568 B |
| plus fat LTO, one codegen unit | 1,673,200 B (17.6 percent under the 2,031,616 B slot) |

## Verification
Host: core tests for the header check, token comparison and chunk
accounting; a harness test for the push script's argument handling and
local header check; `scripts/check`. Board: USB-flash the new layout on
the office board, OTA the same image onto the other slot, confirm the
running slot swapped and `build_elf_sha256` matches; OTA an image that
panics at boot and confirm the bootloader returns to the previous slot;
then the home board.
