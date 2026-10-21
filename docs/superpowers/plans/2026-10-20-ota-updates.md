# Over-the-air firmware updates

## Now
- Step: proven on the home board 2026-10-21 (`docs/audit-evidence/2026-10-21/ota.{md,json}`):
  round trip 12 s upload, restart into `ota_1`, health mark at 60 s;
  rollback from a panicking probe image back to `ota_1` in 10 s. The
  first OTA-layout image crashed every boot (fat LTO left `audio-metrics`
  56 B of stack); fixed by raising it to 6 KiB, output workers to 5 KiB,
  the health thread to 6 KiB; the push script now requires a new boot
  token. `scripts/check` and commit pending (tree carries the stack fix,
  the Sonos opt-in and browser player, the probe feature, the script).
- Next: read stack headroom from a UART after the change; test rollback
  on a hang; office board onto the OTA layout when it is back on USB.
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
