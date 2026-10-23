# Runtime input gain (settings page)

## Now
- Step: landed in `f723af0` (check green, 27 stages). Home-credentials image
  built (ELF `008bc545…`) and packaged at
  `target/kata-hardening/home-image.tgz` with a README (espflash command;
  hold BOOT, tap RST first on that board). Waiting for Doctor Biz to flash
  it at home; the office config is back in `firmware/cfg.toml`.
- Next: build the home-credentials image for the second board and hand it
  to Doctor Biz to flash (no toolchain at home known); verify on a board:
  set +12 dB, see `/api/state` RMS rise 12 dB with a steady source, reboot
  keeps it, read-back mismatch reports an error and leaves the old value.
- Open: how the home board gets flashed (office visit or laptop)
- Approved: "we hould make the input variable (configurable like sonos)"
  (2026-10-20)
- Compactions: 0

## Why
A second A1S at home with an iPod at full volume reads -47 dBFS and a
phone -41 dBFS: a line input at 0 dB PGA is far too insensitive for
consumer headphone outputs, and the input is whatever someone plugs in,
not one particular turntable. `pga_gain` is compiled in from `cfg.toml`
today (`firmware/src/codec.rs` writes register 0x09 once at boot).

## Design
- `core/src/input_gain.rs`: `InputGain` of whole 3 dB steps, 0 to +24 dB
  (the ES8388 PGA: one nibble per channel, both kept equal), `register()`
  = `0x00..0x88`, `from_register`, `parse_request` for `{"gain_db": N}`,
  `status_json`. Pure, host-tested.
- `firmware/src/input_gain.rs`: loads the persisted dB from NVS namespace
  `input` key `gain_db` (fallback: `cfg.toml`'s `pga_gain` register),
  owns the I2C driver after codec setup, `apply(gain)` writes 0x09 with
  read-back then persists, and publishes the live register in a static
  for `/api/metrics`' `settings.pga_gain` (same field, now live).
- `web.rs`: `GET /api/input` → `{"gain_db","max_db","step_db"}`;
  `POST /api/input` `{"gain_db": N}` → 200 with the new status, 400 on a
  bad body or step, 500 when the codec write or read-back fails (old
  value kept), 503 while the I2C lock is held.
- `settings.html` + `app.js`: an "Input gain" select, applied on change,
  confirmed by re-reading the status; note tells the user to watch the
  level on the Playback page and keep peaks a few dB under full scale.
- Out of scope: automatic gain, the quiet threshold, per-channel trims.

## Verification
Host: core tests (`cargo test -p esp-shazam-core input_gain`), the
harness' metrics validator unchanged, `scripts/check`. Board: the Next
section's checks; evidence under `docs/audit-evidence/<date>/input-gain.md`.
