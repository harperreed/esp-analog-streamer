# esp-shazam Implementation Plan

## Now
- Step: Task 15b (send buffer, Wi-Fi and lwIP buffers in PSRAM, about 4 s of ring slack; SDD R29) is running. Its implementer pings the controller the moment FLAC holds 2 minutes, so Harper can test. Task 16 (`/stream.flac`) is complete (85b2a01..c7a3aa9; review approved, 10 minor notes queued for a cleanup round after 15b). Our own RFC 9639 encoder is bit-exact in claxon, ffmpeg and libFLAC, at 64-71% of the WAV bitrate. Harper on FLAC in VLC: "seems to time out after a 10-15 seconds". Task 15 (`/stream.wav`) is complete (d551902..cde2d7d), with modem sleep off (SDD R27). Task 14b is complete (d7e7d0e..b06e6a0; review approved, 5 minor notes). Capture works on the board with 32-bit slots, APLL MCLK, no lost buffers and a 45 ms I2S queue. With the flag off, the page reads "Shazam is off · <level> dBFS" and the level is live. `cfg.toml` has `shazam_enabled = false`. Navigator was initialized (76a45b7), with the judge on (2f1520c). Task 14a is complete (9d02a98, 615d277, 8ac51b4; review approved).
- Next: finish Task 15b (with audit issues 6yb1, 7j8s, kjfx folded in), then Task 17 (the rest of the auditor's kata issues, plus Task 16's queued minors), Task 12 (device E2E and README), then the final whole-branch review.
- Open (Harper):
  - Does the stream sound right, and is left on the left?
  - Should a phone browser play the stream? An endless WAV or FLAC can't do that in Safari, which needs HLS (a separate feature). VLC on the phone would work without it.
  - Shazam and the stream don't fit in internal RAM together: one listener costs 17-25 KB and two 41-55 KB, against about 9-10 KB spare with Shazam on once the idle stream server (5.7-5.8 KB) is counted. Candidate fixes: Wi-Fi and lwIP buffers in PSRAM (untested), or mbedTLS in PSRAM (a security call).
  - The code default of `shazam_enabled` is still `true`; only `cfg.toml` turns it off. Flip the default too?
  - Task 12 was written around Shazam matches. With Shazam off, it should test the stream instead. Keep a Shazam-on scenario in it, or not?
  - Security calls:
    - A core-level https-only URL guard (SDD R18).
    - mbedTLS in PSRAM (`CONFIG_MBEDTLS_DEFAULT_MEM_ALLOC`). Likely unneeded now, with 31.5 KB free.
    - Station OWE: the Wi-Fi password-length warning showed in Task 9's logs but in none since Task 10.
  - Task 12's silence and Wi-Fi-drop checks.
  - A look at http://esp-shazam.local/ (the board runs with Shazam off and stays up).
  - Whether to add an MQTT Last Will.
- Approved: "On the board (Recommended)" (2026-10-03), fingerprint computed on the board
- Approved: "Always listening" (2026-10-03), trigger
- Approved: "Web page on the board", "MQTT, and json endpoint" (2026-10-03), outputs
- Approved: "Yes, but use Rust" (2026-10-03), design section 1
- Approved: "Yes, write the spec (Recommended)" (2026-10-03), revised design
- Approved: "approve" (2026-10-03), written spec
- Approved: "you were getting lost in the planning sauce. i update super powers .now go" (2026-10-03), start execution (subagent-driven, per CLAUDE.md preference)
- Approved: "can we feature flag out the shazam bits, adn go for the stream out of the board?" (2026-10-03), the Shazam flag plus the LINE IN stream (Tasks 13–15)
- Approved: "lossless as much as we can" (2026-10-03), stream quality: 48 kHz/24-bit PCM (SDD R21)
- Approved: "can we do flac?" then "ok. cool." (2026-10-03), the FLAC stream (Task 16) and the Tasks 13→16→12 order
- Direction: "i see you are still doing the ingerprinting. i thought we had talked about putting that behind a feature flag and focusing on the streaming" (2026-10-04), Shazam off on the board, the stream first (SDD R26)
- Approved: "let's do 15" (2026-10-04), skip finishing 14b's report and clean flash; go to Task 15
- Approved: "Yes, flash now" (2026-10-04), flash Task 15's fix round (modem sleep off) and run its device checks
- Approved: "why not just do the flac part, and then we can tune it from there" (2026-10-04), Task 16 (FLAC) before Task 15b's send-buffer tuning
- Approved: "1" (2026-10-04), audit 3pz5's capture-stall policy: restart capture by itself, and reboot if that doesn't fix it
- Approved: "i think we shoudl architect for scenario 2, but realistically it is 1" (2026-10-04), audit f8rn: build for 3-4 listeners (FLAC encoded once and shared), while expecting 2 or fewer
- Compactions: 1

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit that listens to LINE IN all the time, fingerprints the audio on the board with SongRec's algorithm, asks Shazam, and publishes the song on a web page, `GET /api/state`, and MQTT.

**Architecture:** Pure logic lives in the `esp-shazam-core` crate, which has no ESP dependencies and is tested on the Mac:
- the copied SongRec fingerprinting code,
- the Shazam request builder and response parser,
- the listen policy,
- the audio math,
- the song store.

The `firmware` crate is an esp-idf-svc app that wires the codec, I2S capture, Wi-Fi, HTTP client, web server and MQTT around one `SongStore`. A Mac CLI (`tools/shazam-file`) sends real requests to Shazam, so the whole recognize path is proven before any hardware is involved.

**Tech Stack:**
- Toolchain installed by mise from the repo's `mise.toml`:
  - Rust 1.93.0, cmake 3.30.2, ninja 1.12.1
  - espup 0.18.0, espflash 4.6.0, ldproxy 0.3.2 (prebuilt GitHub binaries)
  - the Xtensa `esp` Rust toolchain 1.98.1.0, installed through espup
- esp-idf-svc 0.53.0 / esp-idf-hal 0.47.0 / esp-idf-sys 0.38.1 on ESP-IDF v5.5.5.
- songrec-lib 0.5.4 (copied), chfft, serde_json, toml-cfg.
- On the Mac: ureq and hound; ffmpeg, jq, mosquitto clients.

**Spec:** `docs/superpowers/specs/2026-10-03-esp-shazam-design.md`. Read it before any task. It holds the hardware table, the Shazam protocol, the `/api/state` schema, and the policy rules.

## Global Constraints

- **Board:** Ai-Thinker ESP32-Audio-Kit, ESP32-D0WD-V3 rev v3.1, 4 MB flash. Serial port `/dev/cu.usbserial-0001`.
- **The toolchain comes from mise** (Harper's call). Tools are pinned in `mise.toml`; install them with `mise install`, never with brew, `rustup toolchain install` or `cargo install`.
  - Scripts begin by loading `mise env -s bash` and exit with `FAIL  mise` if that fails, so they never run on unpinned tools.
  - mise exports `RUSTUP_TOOLCHAIN`, which outranks `firmware/rust-toolchain.toml`. Firmware builds therefore run `cargo +esp ...`.
- **Target and versions:**
  - Target `xtensa-esp32-espidf`, ESP-IDF `v5.5.5`.
  - `esp-idf-svc = "=0.53.0"`, `esp-idf-hal = "=0.47.0"`, `esp-idf-sys = "=0.38.1"`, `embuild = "=0.33.5"`.
  - Fingerprint source: `songrec-lib = "=0.5.4"`.
- **License:** GPL-3.0-or-later. The four copied SongRec files keep their attribution and get only the spec's three edits:
  1. drop `make_signature_from_file`,
  2. store the two rings as `Vec<[f32; 1025]>`,
  3. use the base64 `Engine` API.
- **Never commit:** `firmware/cfg.toml`, `test/fixtures/songs/`, `backups/`.
- **No mocks.** E2E tests hit the real Shazam API and the real broker `mqtt://192.168.200.8:1883` (no auth).
- **Shazam request:** URL, headers and body exactly as in the spec's "Shazam protocol" section.
- **Policy defaults:**
  - quiet below -50 dBFS, window 12 s
  - search every 8 s; every 30 s after 5 misses in a row
  - recheck every 30 s while matched
  - clear after 3 misses in a row or 60 s of quiet
  - 300 s backoff after HTTP 429
- **Outputs:**
  - `GET /` and `GET /api/state`, with the schema in the spec.
  - MQTT topic `esp-shazam/now_playing`, retained, QoS 1. The payload is `now` or `null`, republished on every (re)connect.
  - mDNS hostname `esp-shazam`.
- **Success timings:** first ID ≤ 45 s after music starts, song change ≤ 60 s, clear ≤ 75 s after music stops.
- **ABOUTME headers.** Every hand-written source file starts with two `ABOUTME:` comment lines. The copied SongRec files don't.
- **`scripts/check` passes before every commit,** with zero new warnings.
- **Shazam etiquette:** tests never loop requests, and each E2E run sends a handful at most.

## Review Focus

1. **Non-ASCII titles and artists** ("Beyoncé", "夜に駆ける") must come through `/api/state`, MQTT and the web page intact.
   - Tests: Task 4 (parser), Task 6 (store JSON), Task 10 (page declares UTF-8; the JSON content type carries `charset=utf-8`).
2. **Shazam answering with something other than a track** (an HTML error page, an empty body, a 5xx, a 429, a huge body) must not crash or count as a miss.
   - Tests: Task 4 (parser errors), Task 5 (`Failed` and `RateLimited` outcomes), Task 9 (128 KB read cap, status mapping).
3. **LINE IN with a cable plugged in but nothing playing:** hum or DC offset must still read as quiet, or the board queries Shazam forever.
   - Tests: Task 6 (loudness removes DC), Task 8 (noise floor measured and the threshold confirmed).
4. **The MQTT broker unreachable at boot, or restarting mid-run:** recognition and the web page keep working, and the retained value comes back after reconnect.
   - Test: Task 11 (local mosquitto stopped and started under a running board).
5. **Wi-Fi dropping and coming back:** the board reconnects and recognition resumes.
   - Tests: Task 9 (reconnect loop), Task 12 (Harper cuts the board's Wi-Fi for 30 s; the script checks `/api/state` comes back).

---

### Task 1: Workspace, canonical check, time formatting

**Files:**
- Create: `mise.toml`, `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`, `core/src/time.rs`, `scripts/check`, `LICENSE`, `README.md`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `esp_shazam_core::time::rfc3339_utc(unix_secs: u64) -> String`.
  - `scripts/check`, which prints one `ok`/`FAIL` line per stage and exits non-zero on any failure. Logs go to `target/check-logs/<stage>.log`.

- [ ] **Step 1: Pin the host toolchain with mise, create the workspace and an empty core crate**

`mise.toml` (Task 7 adds the ESP32 tools):
```toml
# Toolchain for esp-shazam. `mise install` sets it up; scripts load it with `eval "$(mise env -s bash)"`.
[tools]
rust = "1.93.0" # host toolchain for core, tools and tests
```
Then run `mise trust && mise install`. mise refuses an untrusted project config, so `mise env` fails without the trust step.

`Cargo.toml`:
```toml
[workspace]
members = ["core"]
exclude = ["firmware"]
resolver = "2"
```

`core/Cargo.toml` (explicit fields, no workspace inheritance, because the firmware project path-depends on this crate from outside the workspace):
```toml
[package]
name = "esp-shazam-core"
version = "0.1.0"
edition = "2021"
license = "GPL-3.0-or-later"
description = "Pure logic shared by the esp-shazam firmware and its Mac tools"
publish = false

[dependencies]

[dev-dependencies]
```

`core/src/lib.rs`:
```rust
// ABOUTME: Pure logic shared by the esp-shazam firmware and the Mac tools.
// ABOUTME: No ESP-IDF dependencies, so `cargo test` on the Mac covers all of it.

pub mod time;
```

- [ ] **Step 2: Write the failing test**

`core/src/time.rs`:
```rust
// ABOUTME: Formats Unix time as RFC 3339 UTC without pulling in a date library.
// ABOUTME: Used for `identified_at` and `rate_limited_until` in /api/state.

pub fn rfc3339_utc(unix_secs: u64) -> String {
    let _ = unix_secs;
    String::new()
}

#[cfg(test)]
mod tests {
    use super::rfc3339_utc;

    // Expected values computed with Python's datetime (UTC).
    #[test]
    fn formats_known_instants() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(1_791_062_521), "2026-10-03T21:22:01Z");
        assert_eq!(rfc3339_utc(4_102_444_800), "2100-01-01T00:00:00Z");
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p esp-shazam-core time`
Expected: FAIL. `formats_known_instants` fails with `left: ""`.

- [ ] **Step 4: Implement**

Replace the body of `rfc3339_utc` and add `civil_from_days` (keep the test module):
```rust
pub fn rfc3339_utc(unix_secs: u64) -> String {
    let (year, month, day) = civil_from_days(unix_secs / 86_400);
    let secs_of_day = unix_secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3_600,
        secs_of_day / 60 % 60,
        secs_of_day % 60
    )
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's
/// `civil_from_days` (public domain), restricted to dates from 1970 on.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097; // day of era, [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // year of era, [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year counted from March 1
    let mp = (5 * doy + 2) / 153; // month counted from March, [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p esp-shazam-core time`
Expected: PASS (1 test).

- [ ] **Step 6: Add the canonical check, license and README**

`scripts/check` (then `chmod +x scripts/check`):
```bash
#!/usr/bin/env bash
# ABOUTME: Canonical project check: formatting, lints, Mac tests, and the firmware build once it exists.
# ABOUTME: Prints one ok/FAIL line per stage; full output lands in target/check-logs/<stage>.log.
set -uo pipefail
cd "$(dirname "$0")/.."
mise_env="$(mise env -s bash)" || { echo "FAIL  mise   (run: mise trust && mise install)"; exit 1; }
eval "$mise_env" # the toolchain pinned in mise.toml
LOG_DIR=target/check-logs
mkdir -p "$LOG_DIR"
status=0

stage() {
  local name="$1"
  shift
  if "$@" >"$LOG_DIR/$name.log" 2>&1; then
    echo "ok    $name"
  else
    echo "FAIL  $name   (full log: $LOG_DIR/$name.log)"
    tail -n 40 "$LOG_DIR/$name.log" | sed 's/^/      /'
    status=1
  fi
}

stage fmt cargo fmt --all -- --check
stage clippy cargo clippy --workspace --all-targets -- -D warnings
stage test cargo test --workspace

exit $status
```

`LICENSE`: run `gh api /licenses/gpl-3.0 --jq .body > LICENSE` and confirm the first line reads `GNU GENERAL PUBLIC LICENSE`.

`README.md`:
```markdown
# esp-shazam

An always-listening Shazam box. Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit listens to its LINE IN jack, fingerprints the audio on the board (SongRec's algorithm), asks Shazam, and shows the song at `http://esp-shazam.local/`, as JSON at `/api/state`, and on MQTT topic `esp-shazam/now_playing`.

- Design: `docs/superpowers/specs/2026-10-03-esp-shazam-design.md`
- Check everything: `scripts/check`

Licensed GPL-3.0-or-later (the fingerprint code comes from SongRec).
```

- [ ] **Step 7: Run the canonical check**

Run: `scripts/check`
Expected: `ok    fmt`, `ok    clippy`, `ok    test`; exit code 0.

- [ ] **Step 8: Commit**

```bash
git add mise.toml Cargo.toml Cargo.lock core/Cargo.toml core/src/lib.rs core/src/time.rs scripts/check LICENSE README.md
git commit -m "feat: add workspace, canonical check, and RFC 3339 time formatting"
```

---

### Task 2: Copy in SongRec's fingerprinting, with golden tests

**Files:**
- Create (copied from songrec-lib 0.5.4, then edited): `core/src/fingerprinting/{algorithm,signature_format,hanning,user_agent}.rs`
- Create: `core/src/fingerprinting/UPSTREAM.md`, `core/src/signal.rs`, `core/tests/golden.rs`
- Modify: `core/Cargo.toml`, `core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `esp_shazam_core::fingerprinting::algorithm::SignatureGenerator::make_signature_from_buffer(&[i16]) -> DecodedSignature` (16 kHz mono).
  - `esp_shazam_core::fingerprinting::signature_format::DecodedSignature`, with pub fields `sample_rate_hz: u32` and `number_samples: u32`, plus `encode_to_binary() -> Result<Vec<u8>, Box<dyn Error>>` and `encode_to_uri() -> Result<String, Box<dyn Error>>`.
  - `esp_shazam_core::fingerprinting::user_agent::USER_AGENTS: [&'static str; 100]`.
  - `esp_shazam_core::signal::chord_and_chirp(seconds: u32) -> Vec<i16>` and `esp_shazam_core::signal::SAMPLE_RATE_HZ: u32 = 16_000`.

- [ ] **Step 1: Copy the upstream files unchanged**

```bash
mkdir -p target/vendor core/src/fingerprinting
curl -sL -A 'esp-shazam (harper@2389.ai)' -o target/vendor/songrec-lib-0.5.4.crate \
  https://static.crates.io/crates/songrec-lib/songrec-lib-0.5.4.crate
tar -xzf target/vendor/songrec-lib-0.5.4.crate -C target/vendor
cp target/vendor/songrec-lib-0.5.4/src/fingerprinting/{algorithm,signature_format,hanning,user_agent}.rs core/src/fingerprinting/
```

- [ ] **Step 2: Add the test signal and the failing golden tests**

`core/src/signal.rs`:
```rust
// ABOUTME: Deterministic test audio at 16 kHz: a three-note chord plus a rising chirp.
// ABOUTME: Used by the golden fingerprint tests and by the on-device fingerprint timing check.

pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// `seconds` of mono 16-bit audio whose peaks land in all four fingerprint bands.
pub fn chord_and_chirp(seconds: u32) -> Vec<i16> {
    let n = (seconds * SAMPLE_RATE_HZ) as usize;
    let two_pi = 2.0 * std::f32::consts::PI;
    (0..n)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE_HZ as f32;
            let chord = 0.25 * (two_pi * 440.0 * t).sin()
                + 0.20 * (two_pi * 1_200.0 * t).sin()
                + 0.15 * (two_pi * 2_600.0 * t).sin();
            // Instantaneous frequency 300 Hz + 400 Hz/s: sweeps 300 Hz to 5100 Hz over 12 s.
            let chirp = 0.20 * (two_pi * (300.0 * t + 200.0 * t * t)).sin();
            ((chord + chirp) * 0.9 * 32_767.0) as i16
        })
        .collect()
}
```

`core/src/lib.rs` becomes:
```rust
// ABOUTME: Pure logic shared by the esp-shazam firmware and the Mac tools.
// ABOUTME: No ESP-IDF dependencies, so `cargo test` on the Mac covers all of it.

/// Shazam signature generation, copied from songrec-lib 0.5.4 (GPL-3.0-or-later, by marin-m).
/// Kept as close to upstream as possible so the golden tests can demand identical bytes,
/// which is why clippy's style suggestions are switched off for it. See UPSTREAM.md.
#[allow(clippy::all)]
pub mod fingerprinting {
    pub mod algorithm;
    pub mod hanning;
    pub mod signature_format;
    pub mod user_agent;
}
pub mod signal;
pub mod time;
```

`core/Cargo.toml`: replace the `[dependencies]` and `[dev-dependencies]` sections with:
```toml
[dependencies]
base64 = "0.22"
byteorder = "1.3.4"
chfft = "0.3.4"
crc32fast = "1.2.0"

[dev-dependencies]
hound = "3.5"
songrec-lib = "=0.5.4"
```

`core/tests/golden.rs`:
```rust
// ABOUTME: Golden tests: our copy of SongRec's fingerprint code must match songrec-lib 0.5.4 byte for byte.
// ABOUTME: Synthetic audio always runs; the real song clip runs with `cargo test -- --ignored`.

use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::signal::chord_and_chirp;

fn ours(pcm: &[i16]) -> Vec<u8> {
    SignatureGenerator::make_signature_from_buffer(pcm)
        .encode_to_binary()
        .expect("our encoder failed")
}

fn upstream(pcm: &[i16]) -> Vec<u8> {
    songrec::SignatureGenerator::make_signature_from_buffer(pcm)
        .encode_to_binary()
        .expect("upstream encoder failed")
}

#[test]
fn synthetic_audio_matches_upstream_byte_for_byte() {
    let pcm = chord_and_chirp(12);
    let ours = ours(&pcm);
    assert!(ours.len() > 200, "suspiciously small signature: {} bytes", ours.len());
    assert_eq!(ours, upstream(&pcm));
}

#[test]
fn our_encoding_decodes_with_upstream_decoder() {
    let pcm = chord_and_chirp(12);
    let signature = SignatureGenerator::make_signature_from_buffer(&pcm);
    let uri = signature.encode_to_uri().expect("encode_to_uri failed");
    assert!(uri.starts_with("data:audio/vnd.shazam.sig;base64,"));
    let decoded = songrec::DecodedSignature::decode_from_uri(&uri).expect("upstream decode failed");
    assert_eq!(decoded.sample_rate_hz, 16_000);
    assert_eq!(decoded.number_samples, pcm.len() as u32);
    // Identical re-encoding means every peak survived the round trip.
    assert_eq!(decoded.encode_to_binary().unwrap(), signature.encode_to_binary().unwrap());
}

#[test]
#[ignore = "needs test/fixtures/songs/clip.wav; scripts/e2e-host creates it and runs this"]
fn song_clip_matches_upstream_byte_for_byte() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../test/fixtures/songs/clip.wav");
    let mut reader = hound::WavReader::open(path).expect("open clip.wav");
    let spec = reader.spec();
    assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, 16_000, 16));
    let pcm: Vec<i16> = reader.samples::<i16>().map(|s| s.expect("bad sample")).collect();
    assert_eq!(ours(&pcm), upstream(&pcm));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p esp-shazam-core --test golden`
Expected: FAIL to compile. The unedited copies use `rodio` (not a dependency of our crate) and `base64::encode`/`base64::decode`.

If `songrec-lib` itself fails to build on the Mac (it pulls in cpal, rodio and reqwest):
- Stop and tell Harper.
- The spec's fallback is to keep an unedited copy of the four files under `core/tests/upstream/` as the reference instead.
- Don't switch on your own.

- [ ] **Step 4: Make the three spec edits, plus attribution**

1. `algorithm.rs`: delete the whole `make_signature_from_file` function and the imports only it used (`use std::error::Error;`, `use std::io::BufReader;`). Leave every other line alone.
2. `algorithm.rs`: change the two ring fields and their four initializers (in `make_signature_from_buffer` and in `new`):
   - `fft_outputs: Vec<Vec<f32>>,` → `fft_outputs: Vec<[f32; 1025]>,`
   - `spread_fft_outputs: Vec<Vec<f32>>,` → `spread_fft_outputs: Vec<[f32; 1025]>,`
   - `vec![vec![0.0f32; 1025]; 256]` → `vec![[0.0f32; 1025]; 256]` (all four occurrences)

   Nothing else changes. Indexing, `copy_from_slice` and `.clone()` work the same on `[f32; 1025]` rows.
3. `signature_format.rs`: add `use base64::Engine;` with the other imports, then:
   - `base64::decode(&uri[DATA_URI_PREFIX.len()..])` → `base64::engine::general_purpose::STANDARD.decode(&uri[DATA_URI_PREFIX.len()..])`
   - `base64::encode(self.encode_to_binary()?)` → `base64::engine::general_purpose::STANDARD.encode(self.encode_to_binary()?)`
4. Put this header at the very top of each of the four files:
   ```rust
   // Copied from songrec-lib 0.5.4 (https://github.com/marin-m/SongRec), GPL-3.0-or-later, by marin-m.
   // esp-shazam changes are listed in core/src/fingerprinting/UPSTREAM.md; keep this file close to upstream.
   ```

`core/src/fingerprinting/UPSTREAM.md`:
```markdown
# Upstream: songrec-lib 0.5.4

Source: https://static.crates.io/crates/songrec-lib/songrec-lib-0.5.4.crate, `src/fingerprinting/`.
License: GPL-3.0-or-later, by marin-m (https://github.com/marin-m/SongRec).

Files: algorithm.rs, signature_format.rs, hanning.rs, user_agent.rs.

Changes for esp-shazam (the golden tests in core/tests/golden.rs prove output bytes are unchanged):
1. algorithm.rs: removed `make_signature_from_file` (needs rodio) and its imports.
2. algorithm.rs: the two 256 x 1025 rings are `Vec<[f32; 1025]>` (one 1 MB allocation each)
   instead of `Vec<Vec<f32>>` (256 allocations of 4 KB). ESP-IDF keeps allocations under 16 KB in
   internal RAM; one big allocation lands in PSRAM instead.
3. signature_format.rs: deprecated `base64::encode/decode` replaced by base64 0.22's `Engine` API.
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p esp-shazam-core --test golden`
Expected: PASS for 2 tests, 1 ignored.

Run: `cargo build -p esp-shazam-core 2>&1 | grep -c '^warning'`
Expected: `0`. If rustc warns inside the copied files (unused imports left over from step 4.1), delete the unused import. Never change math.

- [ ] **Step 6: Run the canonical check**

Run: `scripts/check`
Expected: all stages `ok`.

- [ ] **Step 7: Commit**

```bash
git add core/Cargo.toml core/src/lib.rs core/src/signal.rs core/src/fingerprinting core/tests/golden.rs Cargo.lock
git commit -m "feat(core): copy SongRec fingerprinting from songrec-lib 0.5.4 with golden tests"
```

---

### Task 3: Shazam request builder and the Mac CLI that captures real responses

**Files:**
- Create: `core/src/shazam.rs`, `tools/Cargo.toml`, `tools/src/main.rs`, `scripts/e2e-host`
- Create (by running the script): `test/fixtures/shazam/match.json`, `test/fixtures/shazam/miss.json`, `test/fixtures/shazam/match.expected.json`
- Modify: `Cargo.toml` (workspace members), `core/Cargo.toml` (serde_json), `core/src/lib.rs` (`pub mod shazam;`)

**Interfaces:**
- Consumes: `SignatureGenerator::make_signature_from_buffer`, `DecodedSignature::{encode_to_uri, number_samples, sample_rate_hz}`, and `USER_AGENTS` (Task 2).
- Produces (all in `esp_shazam_core::shazam`):
  - `Request { url: String, headers: Vec<(&'static str, String)>, body: String }`
  - `RequestContext<'a> { unix_ms: u64, uuid_a: &'a str, uuid_b: &'a str, user_agent: &'a str }`. The UUIDs are lower-case v4; `uuid_a` gets upper-cased in the URL, as SongRec does.
  - `build_request(signature_uri: &str, sample_ms: u32, ctx: &RequestContext) -> Request`
  - `sample_ms(number_samples: u32, sample_rate_hz: u32) -> u32`
  - `uuid_v4(bytes: [u8; 16]) -> String`
  - `user_agent(index: usize) -> &'static str`: wraps around the 100 SongRec agents.
  - The Mac binary `shazam-file <clip.wav> [--save-response <path>]`.
  - `scripts/e2e-host <song-file> "<expected title>"`.

**Test song:**
- Default: `~/Downloads/24bit WAV IARC0029 Jeff Parker - Suite for Max Brown/05 Metamorphoses.wav`, expected title `Metamorphoses`.
- Backup if Shazam doesn't match it: a well-known pop track from Harper's disk. Find candidates with `mdfind 'kMDItemContentTypeTree == "public.audio" && kMDItemDurationSeconds > 90'`.
- Clips stay in the gitignored `test/fixtures/songs/`.

- [ ] **Step 1: Write the request builder with stubs and failing tests**

`core/Cargo.toml` `[dependencies]` gains `serde_json = "1"`. `core/src/lib.rs` gains `pub mod shazam;`.

`core/src/shazam.rs`:
```rust
// ABOUTME: Builds Shazam recognize requests and parses their responses.
// ABOUTME: Mirrors SongRec's communication.rs; callers pass in time, UUIDs and user agent so tests can pin them.

use serde_json::json;

const RECOGNIZE_URL: &str = "https://amp.shazam.com/discovery/v5/en/US/android/-/tag/";
const RECOGNIZE_QUERY: &str =
    "?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&video=v3";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// Everything a request needs besides the signature.
pub struct RequestContext<'a> {
    pub unix_ms: u64,
    /// Lower-case v4 UUIDs. SongRec puts the first one in the URL upper-cased.
    pub uuid_a: &'a str,
    pub uuid_b: &'a str,
    pub user_agent: &'a str,
}

pub fn build_request(_signature_uri: &str, _sample_ms: u32, _ctx: &RequestContext) -> Request {
    Request { url: String::new(), headers: vec![], body: String::new() }
}

/// SongRec's `samplems`: the signature's length in milliseconds.
pub fn sample_ms(_number_samples: u32, _sample_rate_hz: u32) -> u32 {
    0
}

/// Formats 16 random bytes as a lower-case version-4 UUID.
pub fn uuid_v4(_bytes: [u8; 16]) -> String {
    String::new()
}

/// One of SongRec's 100 Android user agents; any index works.
pub fn user_agent(_index: usize) -> &'static str {
    ""
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprinting::user_agent::USER_AGENTS;
    use serde_json::{json, Value};

    #[test]
    fn request_matches_songrec_shape() {
        let ctx = RequestContext {
            unix_ms: 1_791_062_521_123,
            uuid_a: "0f0e0d0c-0b0a-4908-8706-050403020100",
            uuid_b: "00112233-4455-4677-8899-aabbccddeeff",
            user_agent: "Dalvik/2.1.0 (test)",
        };
        let request = build_request("data:audio/vnd.shazam.sig;base64,AAAA", 12_000, &ctx);
        assert_eq!(
            request.url,
            "https://amp.shazam.com/discovery/v5/en/US/android/-/tag/\
             0F0E0D0C-0B0A-4908-8706-050403020100/00112233-4455-4677-8899-aabbccddeeff\
             ?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true&video=v3"
        );
        assert_eq!(
            request.headers,
            vec![
                ("Content-Type", "application/json".to_string()),
                ("Content-Language", "en_US".to_string()),
                ("User-Agent", "Dalvik/2.1.0 (test)".to_string()),
            ]
        );
        // 1_791_062_521_123 truncated to u32, as SongRec does (`timestamp_ms as u32`).
        let ts = 61_158_691u32;
        let body: Value = serde_json::from_str(&request.body).expect("body is JSON");
        assert_eq!(
            body,
            json!({
                "geolocation": {"altitude": 300, "latitude": 45, "longitude": 2},
                "signature": {"samplems": 12_000, "timestamp": ts, "uri": "data:audio/vnd.shazam.sig;base64,AAAA"},
                "timestamp": ts,
                "timezone": "Europe/Paris"
            })
        );
    }

    #[test]
    fn sample_ms_is_signature_length() {
        assert_eq!(sample_ms(192_000, 16_000), 12_000);
        assert_eq!(sample_ms(100_000, 16_000), 6_250);
    }

    #[test]
    fn uuid_v4_sets_version_and_variant_bits() {
        assert_eq!(uuid_v4([0; 16]), "00000000-0000-4000-8000-000000000000");
        assert_eq!(uuid_v4([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
    }

    #[test]
    fn user_agent_wraps_around_songrecs_list() {
        assert_eq!(user_agent(0), USER_AGENTS[0]);
        assert_eq!(user_agent(100), USER_AGENTS[0]);
        assert_eq!(user_agent(199), USER_AGENTS[99]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p esp-shazam-core shazam`
Expected: FAIL (4 tests).

- [ ] **Step 3: Implement the request side**

Replace the four stubs:
```rust
pub fn build_request(signature_uri: &str, sample_ms: u32, ctx: &RequestContext) -> Request {
    let timestamp = ctx.unix_ms as u32; // SongRec truncates the same way
    let body = json!({
        "geolocation": {"altitude": 300, "latitude": 45, "longitude": 2},
        "signature": {"samplems": sample_ms, "timestamp": timestamp, "uri": signature_uri},
        "timestamp": timestamp,
        "timezone": "Europe/Paris"
    });
    Request {
        url: format!(
            "{RECOGNIZE_URL}{}/{}{RECOGNIZE_QUERY}",
            ctx.uuid_a.to_uppercase(),
            ctx.uuid_b
        ),
        headers: vec![
            ("Content-Type", "application/json".to_string()),
            ("Content-Language", "en_US".to_string()),
            ("User-Agent", ctx.user_agent.to_string()),
        ],
        body: body.to_string(),
    }
}

pub fn sample_ms(number_samples: u32, sample_rate_hz: u32) -> u32 {
    (number_samples as f32 / sample_rate_hz as f32 * 1000.0) as u32
}

pub fn uuid_v4(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

pub fn user_agent(index: usize) -> &'static str {
    let agents = &crate::fingerprinting::user_agent::USER_AGENTS;
    agents[index % agents.len()]
}
```
The `use serde_json::json;` at the top of the file stays; the test module imports its own.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p esp-shazam-core shazam`
Expected: PASS (4 tests).

- [ ] **Step 5: Create the Mac CLI**

`Cargo.toml`: `members = ["core", "tools"]`.

`tools/Cargo.toml`:
```toml
[package]
name = "esp-shazam-tools"
version = "0.1.0"
edition = "2021"
license = "GPL-3.0-or-later"
publish = false

[[bin]]
name = "shazam-file"
path = "src/main.rs"

[dependencies]
anyhow = "1"
esp-shazam-core = { path = "../core" }
hound = "3.5"
ureq = "3"
```

`tools/src/main.rs`:
```rust
// ABOUTME: Mac command-line tool: fingerprints a 16 kHz mono WAV and asks the real Shazam what it is.
// ABOUTME: scripts/e2e-host runs it, and it saves real responses as parser fixtures.

use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::shazam::{self, RequestContext};

const USAGE: &str = "usage: shazam-file <clip.wav> [--save-response <path>]";
const WINDOW_SAMPLES: usize = 12 * 16_000;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (wav_path, save_path) = match args.as_slice() {
        [wav] => (wav.as_str(), None),
        [wav, flag, path] if flag == "--save-response" => (wav.as_str(), Some(path.as_str())),
        _ => bail!(USAGE),
    };

    let pcm = read_wav_16k_mono(wav_path)?;
    let signature = SignatureGenerator::make_signature_from_buffer(middle_window(&pcm));
    let uri = signature.encode_to_uri().map_err(|e| anyhow!("encoding signature: {e}"))?;

    let uuid_a = shazam::uuid_v4(random_bytes()?);
    let uuid_b = shazam::uuid_v4(random_bytes()?);
    let ctx = RequestContext {
        unix_ms: unix_ms()?,
        uuid_a: &uuid_a,
        uuid_b: &uuid_b,
        user_agent: shazam::user_agent(usize::from(random_bytes()?[0])),
    };
    let request = shazam::build_request(
        &uri,
        shazam::sample_ms(signature.number_samples, signature.sample_rate_hz),
        &ctx,
    );

    let (status, body) = send(&request)?;
    if let Some(path) = save_path {
        std::fs::write(path, &body).with_context(|| format!("writing {path}"))?;
    }
    println!("HTTP {status} ({} bytes)", body.len());
    if status != 200 {
        bail!("Shazam answered HTTP {status}");
    }
    Ok(())
}

fn read_wav_16k_mono(path: &str) -> Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("opening {path}"))?;
    let spec = reader.spec();
    if (spec.channels, spec.sample_rate, spec.bits_per_sample) != (1, 16_000, 16) {
        bail!(
            "{path}: need 16 kHz mono 16-bit, got {} ch / {} Hz / {} bit",
            spec.channels,
            spec.sample_rate,
            spec.bits_per_sample
        );
    }
    reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("reading samples from {path}"))
}

/// The middle 12 s, as SongRec takes from files. Shorter input is used whole.
fn middle_window(pcm: &[i16]) -> &[i16] {
    if pcm.len() <= WINDOW_SAMPLES {
        return pcm;
    }
    let start = (pcm.len() - WINDOW_SAMPLES) / 2;
    &pcm[start..start + WINDOW_SAMPLES]
}

fn random_bytes() -> Result<[u8; 16]> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn unix_ms() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64)
}

/// POSTs over HTTPS (ureq speaks HTTP/1.1 only, matching SongRec's forced HTTP/1.1).
/// 4xx and 5xx come back as statuses, not errors, so the caller sees 429s.
fn send(request: &shazam::Request) -> Result<(u16, Vec<u8>)> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut call = agent.post(&request.url);
    for (name, value) in &request.headers {
        call = call.header(*name, value.as_str());
    }
    let mut response = call.send(request.body.as_str())?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_vec()?;
    Ok((status, body))
}
```
The ureq 3 calls above (`Agent::config_builder`, `http_status_as_error`, `timeout_global`, `header`, `send`, `body_mut().read_to_vec()`) **(verify against the ureq version Cargo resolves; adjust the calls, not the behavior)**.

- [ ] **Step 6: Write the Mac E2E script**

`scripts/e2e-host` (then `chmod +x scripts/e2e-host`):
```bash
#!/usr/bin/env bash
# ABOUTME: Mac end-to-end test: fingerprints a real song clip and checks that the real Shazam names it.
# ABOUTME: Refreshes test/fixtures/shazam/ (real responses, lyrics stripped) that the parser tests read.
set -euo pipefail
cd "$(dirname "$0")/.."
mise_env="$(mise env -s bash)" || { echo "FAIL  mise   (run: mise trust && mise install)"; exit 1; }
eval "$mise_env" # the toolchain pinned in mise.toml

if [ $# -ne 2 ]; then
  echo "usage: scripts/e2e-host <song-file> \"<expected title>\"" >&2
  exit 2
fi
song="$1"
expected="$2"
songs=test/fixtures/songs
fixtures=test/fixtures/shazam
mkdir -p "$songs" "$fixtures"

# 12 s from the middle of the song, and 12 s of white noise, both 16 kHz mono 16-bit.
duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$song")
start=$(awk -v d="$duration" 'BEGIN { s = d / 2 - 6; if (s < 0) s = 0; print s }')
ffmpeg -v error -y -ss "$start" -t 12 -i "$song" -ac 1 -ar 16000 -sample_fmt s16 "$songs/clip.wav"
ffmpeg -v error -y -f lavfi -i "anoisesrc=d=12:c=white:r=16000:a=0.3" -ac 1 -sample_fmt s16 "$songs/noise.wav"

cargo build -q -p esp-shazam-tools
tool=target/debug/shazam-file

# Keep copyrighted lyrics out of git; the parser never reads them.
strip_lyrics() {
  jq 'if (.track.sections | type) == "array"
      then .track.sections |= map(select(.type != "LYRICS")) else . end' "$1" > "$1.tmp"
  mv "$1.tmp" "$1"
}

echo "== song clip"
"$tool" "$songs/clip.wav" --save-response "$fixtures/match.json"
strip_lyrics "$fixtures/match.json"
jq -e '.track.title' "$fixtures/match.json" >/dev/null || { echo "FAIL: Shazam found no track in the song clip"; exit 1; }
jq -n --arg title "$expected" '{title: $title}' > "$fixtures/match.expected.json"

echo "== white noise"
"$tool" "$songs/noise.wav" --save-response "$fixtures/miss.json"
jq -e 'has("track") | not' "$fixtures/miss.json" >/dev/null || { echo "FAIL: white noise matched a track"; exit 1; }

echo "== golden test on the song clip"
cargo test -q -p esp-shazam-core --test golden -- --ignored

echo "PASS e2e-host"
```

- [ ] **Step 7: Run it against the real Shazam (2 requests)**

Run: `scripts/e2e-host "$HOME/Downloads/24bit WAV IARC0029 Jeff Parker - Suite for Max Brown/05 Metamorphoses.wav" "Metamorphoses"`
Expected: `HTTP 200` twice, then `PASS e2e-host`. `jq '.track.title' test/fixtures/shazam/match.json` prints a title containing "Metamorphoses".

If the song doesn't match, try the backup song once. If that fails too, stop and report the raw `match.json`; don't retry in a loop.

- [ ] **Step 8: Run the canonical check and commit**

Run: `scripts/check`. Expected: all `ok`.
```bash
git add Cargo.toml Cargo.lock core/Cargo.toml core/src/lib.rs core/src/shazam.rs tools scripts/e2e-host test/fixtures/shazam
git commit -m "feat: build Shazam requests and capture real responses with shazam-file"
```

---

### Task 4: Shazam response parser

**Files:**
- Modify: `core/src/shazam.rs` (add `Song`, `ParseError`, `parse_response`), `tools/src/main.rs` (print the song), `scripts/e2e-host` (check the printed title)
- Create: `core/tests/shazam_fixtures.rs`

**Interfaces:**
- Consumes: `test/fixtures/shazam/{match,miss,match.expected}.json` (Task 3).
- Produces (in `esp_shazam_core::shazam`):
  - `Song { key: String, title: String, artist: String, album: Option<String>, cover_url: Option<String>, shazam_url: Option<String> }`, deriving `Clone, Debug, PartialEq, Eq, Serialize`.
  - `ParseError { NotJson, NotAnObject, MissingField(&'static str) }`, deriving `Debug, PartialEq, Eq` and implementing `Display` and `std::error::Error`.
  - `parse_response(body: &[u8]) -> Result<Option<Song>, ParseError>`. `Ok(None)` means a miss (no `track`).

- [ ] **Step 1: Add the types, a stub, and failing tests**

In `core/src/shazam.rs`, add below `user_agent`:
```rust
/// The song fields esp-shazam reports. Field paths follow SongRec's http_task.rs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Song {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub cover_url: Option<String>,
    pub shazam_url: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    NotJson,
    NotAnObject,
    MissingField(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NotJson => write!(f, "response is not JSON"),
            ParseError::NotAnObject => write!(f, "response JSON is not an object"),
            ParseError::MissingField(field) => write!(f, "response track lacks {field}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// `Ok(None)` is a miss: valid JSON object without a `track`.
pub fn parse_response(_body: &[u8]) -> Result<Option<Song>, ParseError> {
    Err(ParseError::NotJson)
}
```
Change the top import to `use serde::Serialize;` plus `use serde_json::{json, Value};`.

Add to the `tests` module in `core/src/shazam.rs`:
```rust
    #[test]
    fn html_error_page_is_not_json() {
        assert_eq!(parse_response(b"<html><body>502 Bad Gateway</body></html>"), Err(ParseError::NotJson));
    }

    #[test]
    fn empty_body_is_not_json() {
        assert_eq!(parse_response(b""), Err(ParseError::NotJson));
    }

    #[test]
    fn json_that_is_not_an_object_is_rejected() {
        assert_eq!(parse_response(b"[]"), Err(ParseError::NotAnObject));
    }

    #[test]
    fn object_without_track_is_a_miss() {
        assert_eq!(parse_response(br#"{"matches": [], "retryms": 12000}"#), Ok(None));
    }

    #[test]
    fn track_without_title_is_rejected() {
        let body = br#"{"track": {"key": "1", "subtitle": "Artist"}}"#;
        assert_eq!(parse_response(body), Err(ParseError::MissingField("track.title")));
    }

    #[test]
    fn minimal_non_ascii_track_parses() {
        let body = r#"{"track": {"key": "42", "title": "Beyoncé", "subtitle": "夜に駆ける"}}"#;
        assert_eq!(
            parse_response(body.as_bytes()),
            Ok(Some(Song {
                key: "42".into(),
                title: "Beyoncé".into(),
                artist: "夜に駆ける".into(),
                album: None,
                cover_url: None,
                shazam_url: None,
            }))
        );
    }

    #[test]
    fn album_cover_and_link_come_from_their_paths() {
        let body = r#"{"track": {"key": "1", "title": "T", "subtitle": "A",
            "url": "https://www.shazam.com/track/1/t",
            "images": {"coverart": "https://is1-ssl.mzstatic.com/x.jpg"},
            "sections": [{"type": "LYRICS"},
                         {"type": "SONG", "metadata": [{"title": "Label", "text": "L"},
                                                       {"title": "Album", "text": "The Album"}]}]}}"#;
        let song = parse_response(body.as_bytes()).unwrap().unwrap();
        assert_eq!(song.album.as_deref(), Some("The Album"));
        assert_eq!(song.cover_url.as_deref(), Some("https://is1-ssl.mzstatic.com/x.jpg"));
        assert_eq!(song.shazam_url.as_deref(), Some("https://www.shazam.com/track/1/t"));
    }
```

`core/tests/shazam_fixtures.rs`:
```rust
// ABOUTME: Parser tests against real Shazam responses captured by scripts/e2e-host.
// ABOUTME: match.json must yield the expected song; miss.json must yield no song.

use esp_shazam_core::shazam::parse_response;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/../test/fixtures/shazam/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e} (scripts/e2e-host creates it)"))
}

#[test]
fn real_match_response_yields_the_expected_song() {
    let expected: serde_json::Value = serde_json::from_slice(&fixture("match.expected.json")).unwrap();
    let expected_title = expected["title"].as_str().unwrap().to_lowercase();
    let song = parse_response(&fixture("match.json")).expect("parses").expect("is a match");
    assert!(song.title.to_lowercase().contains(&expected_title), "title {:?}", song.title);
    assert!(!song.key.is_empty() && !song.artist.is_empty(), "{song:?}");
    assert!(song.cover_url.as_deref().is_some_and(|u| u.starts_with("https://")), "{song:?}");
    // The spec marks track.url "(verify)": this assertion is that check.
    assert!(song.shazam_url.as_deref().is_some_and(|u| u.starts_with("https://")), "{song:?}");
}

#[test]
fn real_miss_response_yields_no_song() {
    assert_eq!(parse_response(&fixture("miss.json")), Ok(None));
}
```
`core/Cargo.toml` `[dev-dependencies]` doesn't need serde_json; it's already a normal dependency.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p esp-shazam-core shazam`
Expected: FAIL. Every new parser test except the two `NotJson` ones fails, and both fixture tests fail.

- [ ] **Step 3: Implement the parser**

Replace the `parse_response` stub:
```rust
pub fn parse_response(body: &[u8]) -> Result<Option<Song>, ParseError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ParseError::NotJson)?;
    let object = value.as_object().ok_or(ParseError::NotAnObject)?;
    let Some(track) = object.get("track") else {
        return Ok(None);
    };
    let text = |v: &Value| v.as_str().map(str::to_owned);
    let album = track["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|section| section["type"] == "SONG")
        .flat_map(|section| section["metadata"].as_array().into_iter().flatten())
        .find(|item| item["title"] == "Album")
        .and_then(|item| text(&item["text"]));
    Ok(Some(Song {
        key: text(&track["key"]).ok_or(ParseError::MissingField("track.key"))?,
        title: text(&track["title"]).ok_or(ParseError::MissingField("track.title"))?,
        artist: text(&track["subtitle"]).ok_or(ParseError::MissingField("track.subtitle"))?,
        album,
        cover_url: text(&track["images"]["coverart"]),
        shazam_url: text(&track["url"]),
    }))
}
```

If `real_match_response_yields_the_expected_song` fails only on `shazam_url`:
- Open `test/fixtures/shazam/match.json` and find where the Shazam web link lives.
- Change the `shazam_url` path to it.
- Update the spec's "Shazam protocol" response field list.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p esp-shazam-core`
Expected: PASS (all core tests, including 7 new unit tests and 2 fixture tests).

- [ ] **Step 5: Make shazam-file print the song, and e2e-host check it**

In `tools/src/main.rs`, replace the last lines of `main` (from `println!("HTTP ...` to `Ok(())`) with:
```rust
    println!("HTTP {status} ({} bytes)", body.len());
    if status != 200 {
        bail!("Shazam answered HTTP {status}");
    }
    match shazam::parse_response(&body)? {
        Some(song) => println!("MATCH {} — {}", song.title, song.artist),
        None => println!("NO MATCH"),
    }
    Ok(())
```

In `scripts/e2e-host`, replace the `echo "== song clip"` block with:
```bash
echo "== song clip"
out=$("$tool" "$songs/clip.wav" --save-response "$fixtures/match.json")
echo "$out"
strip_lyrics "$fixtures/match.json"
grep -q -- '^MATCH ' <<<"$out" && grep -i -F -q -- "$expected" <<<"$out" \
  || { echo "FAIL: expected a match containing \"$expected\""; exit 1; }
jq -n --arg title "$expected" '{title: $title}' > "$fixtures/match.expected.json"
```
Then add `cargo test -q -p esp-shazam-core --test shazam_fixtures` right before the golden test line.

- [ ] **Step 6: Run the Mac E2E once more (2 requests), then the canonical check**

Run: `scripts/e2e-host "$(mdfind -name 'P.Y.T. (Pretty Young Thing)' 2>/dev/null | grep -E '\.m4a$' | head -1)" "P.Y.T."`
Expected: `MATCH P.Y.T. (Pretty Young Thing) — Michael Jackson` (or Shazam's spelling of it), `NO MATCH` for the noise, then `PASS e2e-host`. Songs come from mdfind because this session can't read `~/Downloads` (ledger Ruling R7).

Run: `scripts/check`. Expected: all `ok`.

- [ ] **Step 7: Commit**

```bash
git add core/src/shazam.rs core/tests/shazam_fixtures.rs tools/src/main.rs scripts/e2e-host test/fixtures/shazam
git commit -m "feat(core): parse Shazam responses, tested against real captures"
```

---

### Task 5: Listen policy

**Files:**
- Create: `core/src/policy.rs`
- Modify: `core/Cargo.toml` (serde with derive), `core/src/lib.rs` (`pub mod policy;`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces (all in `esp_shazam_core::policy`):
  - `PolicyConfig { quiet_threshold_dbfs: f32, search_interval_ms: u64, slow_search_interval_ms: u64, slow_after_misses: u32, recheck_interval_ms: u64, clear_after_misses: u32, clear_after_quiet_ms: u64, rate_limit_backoff_ms: u64 }` with `Default` = the spec's values
  - `State { Quiet, Searching, Matched, RateLimited }` (serde: `"quiet"`, `"searching"`, `"matched"`, `"rate_limited"`)
  - `Outcome { Match { key: String }, Miss, RateLimited, Failed }`
  - `Decision { Idle, Ask, ClearNow }`
  - `Change { Keep, SetNow, ClearNow }`
  - `Policy::new(PolicyConfig)`
  - `Policy::tick(&mut self, now_ms: u64, level_dbfs: f32, clock_synced: bool, has_now: bool) -> Decision`
  - `Policy::on_outcome(&mut self, now_ms: u64, outcome: &Outcome, current_key: Option<&str>) -> Change`
  - `Policy::state(&self, now_ms: u64, has_now: bool) -> State`
  - `Policy::rate_limited_until(&self, now_ms: u64) -> Option<u64>`

  The policy holds timers and counters only. Whether a song is "now" lives in `SongStore` (Task 6), and the caller passes it in. That keeps one source of truth.

- [ ] **Step 1: Add serde and the module with types, a stub, and the failing tests**

`core/Cargo.toml` `[dependencies]` gains:
```toml
serde = { version = "1", features = ["derive"] }
```

`core/src/lib.rs`: add `pub mod policy;` (keep the module list alphabetical).

`core/src/policy.rs`:
```rust
// ABOUTME: Decides when the board asks Shazam, from loudness, the clock and earlier answers.
// ABOUTME: A pure state machine; the firmware feeds it time and applies its decisions to the song store.

use serde::Serialize;

/// Listen policy settings. `Default` holds the spec's values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolicyConfig {
    pub quiet_threshold_dbfs: f32,
    pub search_interval_ms: u64,
    pub slow_search_interval_ms: u64,
    pub slow_after_misses: u32,
    pub recheck_interval_ms: u64,
    pub clear_after_misses: u32,
    pub clear_after_quiet_ms: u64,
    pub rate_limit_backoff_ms: u64,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            quiet_threshold_dbfs: -50.0,
            search_interval_ms: 8_000,
            slow_search_interval_ms: 30_000,
            slow_after_misses: 5,
            recheck_interval_ms: 30_000,
            clear_after_misses: 3,
            clear_after_quiet_ms: 60_000,
            rate_limit_backoff_ms: 300_000,
        }
    }
}

/// The state /api/state reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Quiet,
    Searching,
    Matched,
    RateLimited,
}

/// The answer to one Shazam request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Match { key: String },
    Miss,
    RateLimited,
    /// Network error, a status other than 200/429, or a body that didn't parse. Not a miss.
    Failed,
}

/// What `tick` asks the caller to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Idle,
    Ask,
    ClearNow,
}

/// What `on_outcome` asks the caller to do to the song store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Keep,
    SetNow,
    ClearNow,
}

#[derive(Debug)]
pub struct Policy {
    cfg: PolicyConfig,
    loud: bool,
    loud_since_ms: Option<u64>,
    quiet_since_ms: Option<u64>,
    next_ask_ms: Option<u64>,
    misses: u32,
    in_flight: bool,
    rate_limited_until_ms: Option<u64>,
}

impl Policy {
    pub fn new(cfg: PolicyConfig) -> Self {
        Self {
            cfg,
            loud: false,
            loud_since_ms: None,
            quiet_since_ms: None,
            next_ask_ms: None,
            misses: 0,
            in_flight: false,
            rate_limited_until_ms: None,
        }
    }

    pub fn tick(&mut self, _now_ms: u64, _level_dbfs: f32, _clock_synced: bool, _has_now: bool) -> Decision {
        Decision::Idle
    }

    pub fn on_outcome(&mut self, _now_ms: u64, _outcome: &Outcome, _current_key: Option<&str>) -> Change {
        Change::Keep
    }

    pub fn state(&self, _now_ms: u64, _has_now: bool) -> State {
        State::Quiet
    }

    pub fn rate_limited_until(&self, _now_ms: u64) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOUD: f32 = -20.0;
    const QUIET: f32 = -70.0;
    const TICK_MS: u64 = 250;

    /// Drives a Policy the way the firmware's recognizer does, standing in for the song store.
    struct Board {
        policy: Policy,
        now_ms: u64,
        now_key: Option<String>,
        synced: bool,
    }

    impl Board {
        fn new() -> Self {
            Self { policy: Policy::new(PolicyConfig::default()), now_ms: 0, now_key: None, synced: true }
        }

        /// Ticks every 250 ms through `until_ms`. Stops at the first Ask and returns its time;
        /// the clock stays put so `answer` responds at that instant. Applies ClearNow like the firmware.
        fn run_until_ask(&mut self, level: f32, until_ms: u64) -> Option<u64> {
            while self.now_ms <= until_ms {
                match self.policy.tick(self.now_ms, level, self.synced, self.now_key.is_some()) {
                    Decision::Ask => return Some(self.now_ms),
                    Decision::ClearNow => self.now_key = None,
                    Decision::Idle => {}
                }
                self.now_ms += TICK_MS;
            }
            None
        }

        fn answer(&mut self, outcome: Outcome) -> Change {
            let change = self.policy.on_outcome(self.now_ms, &outcome, self.now_key.as_deref());
            match (change, &outcome) {
                (Change::SetNow, Outcome::Match { key }) => self.now_key = Some(key.clone()),
                (Change::ClearNow, _) => self.now_key = None,
                _ => {}
            }
            self.now_ms += TICK_MS;
            change
        }

        fn matched(&mut self, key: &str) {
            assert!(self.run_until_ask(LOUD, self.now_ms + 60_000).is_some());
            self.answer(Outcome::Match { key: key.into() });
        }
    }

    #[test]
    fn quiet_input_never_asks() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(QUIET, 600_000), None);
    }

    #[test]
    fn first_ask_waits_for_eight_seconds_of_sound() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(LOUD, 60_000), Some(8_000));
    }

    #[test]
    fn no_ask_until_the_clock_is_synced() {
        let mut b = Board::new();
        b.synced = false;
        assert_eq!(b.run_until_ask(LOUD, 20_000), None);
        b.synced = true;
        assert_eq!(b.run_until_ask(LOUD, 30_000), Some(20_250));
    }

    #[test]
    fn searching_asks_every_8s_then_every_30s_after_five_misses() {
        let mut b = Board::new();
        let mut asks = vec![];
        for _ in 0..6 {
            asks.push(b.run_until_ask(LOUD, 200_000).expect("expected an ask"));
            b.answer(Outcome::Miss);
        }
        assert_eq!(asks, vec![8_000, 16_000, 24_000, 32_000, 40_000, 70_000]);
    }

    #[test]
    fn a_match_sets_now_then_rechecks_every_30s() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(LOUD, 60_000), Some(8_000));
        assert_eq!(b.answer(Outcome::Match { key: "k1".into() }), Change::SetNow);
        assert_eq!(b.policy.state(b.now_ms, true), State::Matched);
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(38_000));
        assert_eq!(b.answer(Outcome::Match { key: "k1".into() }), Change::Keep);
        assert_eq!(b.now_key.as_deref(), Some("k1"));
    }

    #[test]
    fn a_new_key_while_matched_sets_now() {
        let mut b = Board::new();
        b.matched("k1");
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(38_000));
        assert_eq!(b.answer(Outcome::Match { key: "k2".into() }), Change::SetNow);
        assert_eq!(b.now_key.as_deref(), Some("k2"));
    }

    #[test]
    fn three_misses_while_matched_clear_now_then_search_again() {
        let mut b = Board::new();
        b.matched("k1");
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(38_000));
        assert_eq!(b.answer(Outcome::Miss), Change::Keep);
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(68_000));
        assert_eq!(b.answer(Outcome::Miss), Change::Keep);
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(98_000));
        assert_eq!(b.answer(Outcome::Miss), Change::ClearNow);
        assert_eq!(b.now_key, None);
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(106_000));
    }

    #[test]
    fn sixty_seconds_of_quiet_clear_now() {
        let mut b = Board::new();
        b.matched("k1"); // answered at 8_000; clock now 8_250
        assert_eq!(b.run_until_ask(QUIET, 68_000), None);
        assert_eq!(b.now_key.as_deref(), Some("k1"), "cleared before 60 s of quiet");
        assert_eq!(b.run_until_ask(QUIET, 68_250), None);
        assert_eq!(b.now_key, None, "not cleared after 60 s of quiet");
    }

    #[test]
    fn rate_limit_pauses_five_minutes_and_keeps_now() {
        let mut b = Board::new();
        b.matched("k1");
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(38_000));
        assert_eq!(b.answer(Outcome::RateLimited), Change::Keep);
        assert_eq!(b.policy.state(b.now_ms, true), State::RateLimited);
        assert_eq!(b.policy.rate_limited_until(b.now_ms), Some(338_000));
        assert_eq!(b.run_until_ask(LOUD, 337_999), None);
        assert_eq!(b.run_until_ask(LOUD, 400_000), Some(338_000));
        assert_eq!(b.now_key.as_deref(), Some("k1"));
        assert_eq!(b.policy.rate_limited_until(b.now_ms), None);
    }

    #[test]
    fn failures_keep_the_search_pace_and_are_not_misses() {
        let mut b = Board::new();
        let mut asks = vec![];
        for _ in 0..6 {
            asks.push(b.run_until_ask(LOUD, 200_000).expect("expected an ask"));
            b.answer(Outcome::Failed);
        }
        assert_eq!(asks, vec![8_000, 16_000, 24_000, 32_000, 40_000, 48_000]);
    }

    #[test]
    fn going_quiet_and_back_resets_the_miss_count() {
        let mut b = Board::new();
        for _ in 0..5 {
            b.run_until_ask(LOUD, 200_000).expect("expected an ask");
            b.answer(Outcome::Miss);
        }
        // Clock is 40_250; the next ask would be the slow one at 70_000.
        assert_eq!(b.run_until_ask(QUIET, 45_000), None);
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(53_250));
        b.answer(Outcome::Miss);
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(61_250));
    }

    #[test]
    fn only_one_request_in_flight() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(LOUD, 60_000), Some(8_000));
        b.now_ms += TICK_MS; // never answer
        assert_eq!(b.run_until_ask(LOUD, 600_000), None);
    }

    #[test]
    fn state_follows_loudness_and_now() {
        let mut b = Board::new();
        assert_eq!(b.policy.state(0, false), State::Quiet);
        b.policy.tick(0, LOUD, true, false);
        assert_eq!(b.policy.state(0, false), State::Searching);
        assert_eq!(b.policy.state(0, true), State::Matched);
        b.policy.tick(250, QUIET, true, true);
        assert_eq!(b.policy.state(250, true), State::Quiet);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p esp-shazam-core policy`
Expected: FAIL. Every test except `quiet_input_never_asks` fails, because the stubs never ask.

- [ ] **Step 3: Implement**

Replace the four stub methods with:
```rust
    /// Call about every 250 ms. `has_now` says whether the song store shows a song.
    pub fn tick(&mut self, now_ms: u64, level_dbfs: f32, clock_synced: bool, has_now: bool) -> Decision {
        self.loud = level_dbfs >= self.cfg.quiet_threshold_dbfs;
        if !self.loud {
            self.loud_since_ms = None;
            let quiet_since = *self.quiet_since_ms.get_or_insert(now_ms);
            if has_now && now_ms - quiet_since >= self.cfg.clear_after_quiet_ms {
                return Decision::ClearNow;
            }
            return Decision::Idle;
        }
        self.quiet_since_ms = None;
        if self.loud_since_ms.is_none() {
            // Quiet to loud is a fresh start: the first ask waits a full search interval of sound.
            self.loud_since_ms = Some(now_ms);
            self.misses = 0;
            self.next_ask_ms = Some(now_ms + self.cfg.search_interval_ms);
        }
        if self.in_flight || !clock_synced || self.rate_limited_until(now_ms).is_some() {
            return Decision::Idle;
        }
        match self.next_ask_ms {
            Some(at) if now_ms >= at => {
                self.in_flight = true;
                Decision::Ask
            }
            _ => Decision::Idle,
        }
    }

    /// Report the answer to the last Ask. `current_key` is the key of the song the store shows.
    pub fn on_outcome(&mut self, now_ms: u64, outcome: &Outcome, current_key: Option<&str>) -> Change {
        self.in_flight = false;
        match outcome {
            Outcome::Match { key } => {
                self.misses = 0;
                self.next_ask_ms = Some(now_ms + self.cfg.recheck_interval_ms);
                if current_key == Some(key.as_str()) {
                    Change::Keep
                } else {
                    Change::SetNow
                }
            }
            Outcome::Miss => {
                self.misses += 1;
                if current_key.is_none() {
                    self.next_ask_ms = Some(now_ms + self.search_wait_ms());
                    Change::Keep
                } else if self.misses >= self.cfg.clear_after_misses {
                    self.misses = 0;
                    self.next_ask_ms = Some(now_ms + self.cfg.search_interval_ms);
                    Change::ClearNow
                } else {
                    self.next_ask_ms = Some(now_ms + self.cfg.recheck_interval_ms);
                    Change::Keep
                }
            }
            Outcome::RateLimited => {
                let until = now_ms + self.cfg.rate_limit_backoff_ms;
                self.rate_limited_until_ms = Some(until);
                self.next_ask_ms = Some(until);
                Change::Keep
            }
            Outcome::Failed => {
                let wait = if current_key.is_some() {
                    self.cfg.recheck_interval_ms
                } else {
                    self.search_wait_ms()
                };
                self.next_ask_ms = Some(now_ms + wait);
                Change::Keep
            }
        }
    }

    pub fn state(&self, now_ms: u64, has_now: bool) -> State {
        if self.rate_limited_until(now_ms).is_some() {
            State::RateLimited
        } else if !self.loud {
            State::Quiet
        } else if has_now {
            State::Matched
        } else {
            State::Searching
        }
    }

    /// When the 429 backoff ends, if it is still running.
    pub fn rate_limited_until(&self, now_ms: u64) -> Option<u64> {
        self.rate_limited_until_ms.filter(|&until| now_ms < until)
    }

    fn search_wait_ms(&self) -> u64 {
        if self.misses >= self.cfg.slow_after_misses {
            self.cfg.slow_search_interval_ms
        } else {
            self.cfg.search_interval_ms
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p esp-shazam-core policy`
Expected: PASS (13 tests).

- [ ] **Step 5: Run the canonical check and commit**

Run: `scripts/check`. Expected: all `ok`.
```bash
git add core/Cargo.toml core/src/lib.rs core/src/policy.rs Cargo.lock
git commit -m "feat(core): add the listen policy state machine"
```

---

### Task 6: Audio math and the song store

**Files:**
- Create: `core/src/audio.rs`, `core/src/store.rs`
- Modify: `core/Cargo.toml` (serde_json, if Task 4 hasn't added it), `core/src/lib.rs` (`pub mod audio;`, `pub mod store;`)

**Interfaces:**
- Consumes:
  - `esp_shazam_core::shazam::Song` (Task 4): `{ key, title, artist: String, album, cover_url, shazam_url: Option<String> }`, deriving `Clone, Debug, PartialEq, Serialize`.
  - `esp_shazam_core::policy::State` (Task 5).
- Produces:
  - `audio::SampleRing` with:
    - `new(capacity: usize)`, `capacity()`, `len()`, `is_empty()`
    - `push(&mut self, &[i16])`
    - `copy_latest(&self, n: usize, out: &mut Vec<i16>)`: the newest `n` samples, oldest first, capped at `len()`.
  - `audio::loudness_dbfs(&[i16]) -> f32`: DC removed first; returns `SILENCE_DBFS` for silence or empty input.
  - `audio::SILENCE_DBFS: f32 = -120.0`
  - `audio::stereo_to_mono(&[i16], &mut Vec<i16>)`
  - `audio::zero_crossing_hz(&[i16], sample_rate_hz: u32) -> f32`
  - `store::NowPlaying { song: Song /* flattened in JSON */, identified_at: String }`
  - `store::SongStore` with:
    - `new(history_cap: usize)`
    - `now_key() -> Option<&str>`, `has_now() -> bool`
    - `set_now(Song, identified_at: String) -> bool`: false if it's already the current key.
    - `clear_now() -> bool`: false if nothing was playing.
    - `set_status(State, level_dbfs: f32, rate_limited_until: Option<String>)`
    - `state_json() -> String`: the `/api/state` body.
    - `now_json() -> String`: the `now` object or `null`.

- [ ] **Step 1: Write the audio module with stubs and failing tests**

`core/src/lib.rs`: add `pub mod audio;` and `pub mod store;`.

`core/src/audio.rs`:
```rust
// ABOUTME: Audio helpers for the capture path: a fixed-size sample ring, loudness, and a tone check.
// ABOUTME: Pure code, so the Mac tests cover what the firmware runs.

pub const SILENCE_DBFS: f32 = -120.0;

/// Fixed-capacity ring of mono samples that keeps the newest audio.
#[derive(Debug)]
pub struct SampleRing {
    buf: Vec<i16>,
    write: usize,
    filled: usize,
}

impl SampleRing {
    pub fn new(capacity: usize) -> Self {
        Self { buf: vec![0; capacity], write: 0, filled: 0 }
    }
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }
    pub fn len(&self) -> usize {
        self.filled
    }
    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }
    pub fn push(&mut self, _samples: &[i16]) {}
    pub fn copy_latest(&self, _n: usize, out: &mut Vec<i16>) {
        out.clear();
    }
}

/// Loudness of a block in dBFS after removing its DC offset. Silence and empty input give SILENCE_DBFS.
pub fn loudness_dbfs(_samples: &[i16]) -> f32 {
    0.0
}

/// Averages interleaved stereo frames (L, R, L, R, ...) into mono. A trailing odd sample is dropped.
pub fn stereo_to_mono(_interleaved: &[i16], out: &mut Vec<i16>) {
    out.clear();
}

/// Frequency of a clean tone estimated from zero crossings. Bring-up plays a 1 kHz tone into
/// LINE IN; reading ~1000 proves the I2S format and sample rate are right.
pub fn zero_crossing_hz(_samples: &[i16], _sample_rate_hz: u32) -> f32 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq_hz: f32, amplitude: f32, seconds: f32, dc: i16) -> Vec<i16> {
        let n = (16_000.0 * seconds) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / 16_000.0;
                let v = amplitude * 32_767.0 * (2.0 * std::f32::consts::PI * freq_hz * t).sin();
                (v as i32 + dc as i32) as i16
            })
            .collect()
    }

    #[test]
    fn ring_returns_newest_samples_oldest_first() {
        let mut ring = SampleRing::new(5);
        let mut out = vec![];
        ring.push(&[1, 2, 3]);
        ring.copy_latest(10, &mut out);
        assert_eq!(out, vec![1, 2, 3]);
        ring.push(&[4, 5, 6, 7]);
        assert_eq!(ring.len(), 5);
        ring.copy_latest(5, &mut out);
        assert_eq!(out, vec![3, 4, 5, 6, 7]);
        ring.copy_latest(2, &mut out);
        assert_eq!(out, vec![6, 7]);
    }

    #[test]
    fn silence_and_empty_input_read_as_silence() {
        assert_eq!(loudness_dbfs(&[]), SILENCE_DBFS);
        assert_eq!(loudness_dbfs(&[0; 16_000]), SILENCE_DBFS);
    }

    #[test]
    fn half_scale_sine_reads_minus_nine_dbfs() {
        // RMS of a sine is amplitude / sqrt(2): 0.5 -> -9.03 dBFS.
        let level = loudness_dbfs(&sine(1_000.0, 0.5, 1.0, 0));
        assert!((level - -9.03).abs() < 0.05, "got {level}");
    }

    #[test]
    fn dc_offset_alone_reads_as_silence() {
        // A plugged-in cable with nothing playing can sit at a DC offset; it must not look loud.
        assert_eq!(loudness_dbfs(&[10_000; 16_000]), SILENCE_DBFS);
        let level = loudness_dbfs(&sine(1_000.0, 0.1, 1.0, 10_000));
        assert!((level - -23.01).abs() < 0.05, "got {level}");
    }

    #[test]
    fn stereo_frames_average_to_mono() {
        let mut out = vec![];
        stereo_to_mono(&[100, 300, -100, -300, 7], &mut out);
        assert_eq!(out, vec![200, -200]);
    }

    #[test]
    fn zero_crossings_find_a_one_khz_tone() {
        let hz = zero_crossing_hz(&sine(1_000.0, 0.5, 1.0, 500), 16_000);
        assert!((hz - 1_000.0).abs() < 2.0, "got {hz}");
    }
}
```

- [ ] **Step 2: Run the audio tests to verify they fail**

Run: `cargo test -p esp-shazam-core audio`
Expected: FAIL in 5 tests. `silence_and_empty_input_read_as_silence` and the others fail against the stubs.

- [ ] **Step 3: Implement the audio functions**

Replace the stubs:
```rust
    pub fn push(&mut self, samples: &[i16]) {
        let cap = self.buf.len();
        for &s in samples {
            self.buf[self.write] = s;
            self.write = (self.write + 1) % cap;
        }
        self.filled = (self.filled + samples.len()).min(cap);
    }

    pub fn copy_latest(&self, n: usize, out: &mut Vec<i16>) {
        out.clear();
        let n = n.min(self.filled);
        let cap = self.buf.len();
        let start = (self.write + cap - n) % cap;
        out.extend((0..n).map(|i| self.buf[(start + i) % cap]));
    }
```
```rust
pub fn loudness_dbfs(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return SILENCE_DBFS;
    }
    // Integer sums keep this cheap on the ESP32, which has no double-precision FPU.
    let (sum, sum_sq) = samples.iter().fold((0i64, 0i64), |(s, q), &x| {
        let x = i64::from(x);
        (s + x, q + x * x)
    });
    let n = samples.len() as f64;
    let mean = sum as f64 / n;
    let variance = sum_sq as f64 / n - mean * mean;
    // Anything under one LSB of RMS is rounding error from the DC removal, not sound.
    if variance < 1.0 {
        return SILENCE_DBFS;
    }
    let rms = variance.sqrt() / 32_768.0;
    ((20.0 * rms.log10()) as f32).max(SILENCE_DBFS)
}

pub fn stereo_to_mono(interleaved: &[i16], out: &mut Vec<i16>) {
    out.clear();
    out.extend(
        interleaved
            .chunks_exact(2)
            .map(|frame| ((i32::from(frame[0]) + i32::from(frame[1])) / 2) as i16),
    );
}

pub fn zero_crossing_hz(samples: &[i16], sample_rate_hz: u32) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let mean = samples.iter().map(|&s| i64::from(s)).sum::<i64>() / samples.len() as i64;
    let crossings = samples
        .windows(2)
        .filter(|w| (i64::from(w[0]) < mean) != (i64::from(w[1]) < mean))
        .count();
    crossings as f32 * sample_rate_hz as f32 / (2.0 * samples.len() as f32)
}
```

- [ ] **Step 4: Run the audio tests to verify they pass**

Run: `cargo test -p esp-shazam-core audio`
Expected: PASS (6 tests).

- [ ] **Step 5: Write the store with stubs and failing tests**

`core/src/store.rs`:
```rust
// ABOUTME: The single source of truth for what the board reports: state, loudness, now playing, history.
// ABOUTME: Serializes the /api/state body and the MQTT now_playing payload.

use std::collections::VecDeque;

use serde::Serialize;

use crate::audio::SILENCE_DBFS;
use crate::policy::State;
use crate::shazam::Song;

/// A song plus when it first became "now". Serializes flat: song fields, then identified_at.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NowPlaying {
    #[serde(flatten)]
    pub song: Song,
    pub identified_at: String,
}

#[derive(Debug)]
pub struct SongStore {
    now: Option<NowPlaying>,
    history: VecDeque<NowPlaying>,
    history_cap: usize,
    state: State,
    level_dbfs: f32,
    rate_limited_until: Option<String>,
}

#[derive(Serialize)]
struct StateView<'a> {
    state: State,
    level_dbfs: f32,
    rate_limited_until: Option<&'a str>,
    now: Option<&'a NowPlaying>,
    history: &'a VecDeque<NowPlaying>,
}

impl SongStore {
    pub fn new(history_cap: usize) -> Self {
        Self {
            now: None,
            history: VecDeque::new(),
            history_cap,
            state: State::Quiet,
            level_dbfs: SILENCE_DBFS,
            rate_limited_until: None,
        }
    }
    pub fn now_key(&self) -> Option<&str> {
        None
    }
    pub fn has_now(&self) -> bool {
        false
    }
    pub fn set_now(&mut self, _song: Song, _identified_at: String) -> bool {
        false
    }
    pub fn clear_now(&mut self) -> bool {
        false
    }
    pub fn set_status(&mut self, _state: State, _level_dbfs: f32, _rate_limited_until: Option<String>) {}
    pub fn state_json(&self) -> String {
        String::new()
    }
    pub fn now_json(&self) -> String {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn song(key: &str, title: &str, artist: &str) -> Song {
        Song {
            key: key.into(),
            title: title.into(),
            artist: artist.into(),
            album: Some("An Album".into()),
            cover_url: Some("https://example.com/cover.jpg".into()),
            shazam_url: None,
        }
    }

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).expect("valid JSON")
    }

    #[test]
    fn empty_store_reports_quiet_and_nothing_playing() {
        let store = SongStore::new(20);
        assert_eq!(
            parse(&store.state_json()),
            json!({"state": "quiet", "level_dbfs": -120.0, "rate_limited_until": null, "now": null, "history": []})
        );
        assert_eq!(store.now_json(), "null");
    }

    #[test]
    fn set_now_fills_now_and_the_top_of_history() {
        let mut store = SongStore::new(20);
        assert!(store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:22:01Z".into()));
        assert_eq!(store.now_key(), Some("1"));
        let state = parse(&store.state_json());
        let expected = json!({
            "key": "1", "title": "Title", "artist": "Artist", "album": "An Album",
            "cover_url": "https://example.com/cover.jpg", "shazam_url": null,
            "identified_at": "2026-10-03T21:22:01Z"
        });
        assert_eq!(state["now"], expected);
        assert_eq!(state["history"][0], expected);
        assert_eq!(parse(&store.now_json()), expected);
    }

    #[test]
    fn the_same_key_twice_is_not_a_change() {
        let mut store = SongStore::new(20);
        assert!(store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:22:01Z".into()));
        assert!(!store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:23:01Z".into()));
        let state = parse(&store.state_json());
        assert_eq!(state["history"].as_array().unwrap().len(), 1);
        assert_eq!(state["now"]["identified_at"], "2026-10-03T21:22:01Z");
    }

    #[test]
    fn history_keeps_the_newest_twenty() {
        let mut store = SongStore::new(20);
        for i in 0..25 {
            store.set_now(song(&i.to_string(), "T", "A"), "2026-10-03T21:22:01Z".into());
        }
        let state = parse(&store.state_json());
        let history = state["history"].as_array().unwrap();
        assert_eq!(history.len(), 20);
        assert_eq!(history[0]["key"], "24");
        assert_eq!(history[19]["key"], "5");
    }

    #[test]
    fn clear_now_keeps_history() {
        let mut store = SongStore::new(20);
        store.set_now(song("1", "Title", "Artist"), "2026-10-03T21:22:01Z".into());
        assert!(store.clear_now());
        assert!(!store.clear_now());
        assert!(!store.has_now());
        let state = parse(&store.state_json());
        assert_eq!(state["now"], Value::Null);
        assert_eq!(state["history"].as_array().unwrap().len(), 1);
        assert_eq!(store.now_json(), "null");
    }

    #[test]
    fn non_ascii_titles_survive_unescaped() {
        let mut store = SongStore::new(20);
        store.set_now(song("7", "Beyoncé", "夜に駆ける"), "2026-10-03T21:22:01Z".into());
        let json = store.now_json();
        assert!(json.contains("Beyoncé") && json.contains("夜に駆ける"), "got {json}");
        assert_eq!(parse(&json)["artist"], "夜に駆ける");
    }

    #[test]
    fn status_reports_state_rounded_level_and_rate_limit() {
        let mut store = SongStore::new(20);
        store.set_status(State::RateLimited, -18.437, Some("2026-10-03T21:27:01Z".into()));
        let state = parse(&store.state_json());
        assert_eq!(state["state"], "rate_limited");
        assert_eq!(state["level_dbfs"], json!(-18.4));
        assert_eq!(state["rate_limited_until"], "2026-10-03T21:27:01Z");
    }
}
```

- [ ] **Step 6: Run the store tests to verify they fail**

Run: `cargo test -p esp-shazam-core store`
Expected: FAIL (all 7 tests).

- [ ] **Step 7: Implement the store**

Replace the stub methods:
```rust
    pub fn now_key(&self) -> Option<&str> {
        self.now.as_ref().map(|n| n.song.key.as_str())
    }

    pub fn has_now(&self) -> bool {
        self.now.is_some()
    }

    /// Makes `song` the current song and puts it at the top of the history.
    /// Returns false, changing nothing, when it is already the current song.
    pub fn set_now(&mut self, song: Song, identified_at: String) -> bool {
        if self.now_key() == Some(song.key.as_str()) {
            return false;
        }
        let entry = NowPlaying { song, identified_at };
        self.history.push_front(entry.clone());
        self.history.truncate(self.history_cap);
        self.now = Some(entry);
        true
    }

    /// Returns false when nothing was playing.
    pub fn clear_now(&mut self) -> bool {
        self.now.take().is_some()
    }

    pub fn set_status(&mut self, state: State, level_dbfs: f32, rate_limited_until: Option<String>) {
        self.state = state;
        self.level_dbfs = (level_dbfs * 10.0).round() / 10.0;
        self.rate_limited_until = rate_limited_until;
    }

    pub fn state_json(&self) -> String {
        let view = StateView {
            state: self.state,
            level_dbfs: self.level_dbfs,
            rate_limited_until: self.rate_limited_until.as_deref(),
            now: self.now.as_ref(),
            history: &self.history,
        };
        serde_json::to_string(&view).expect("state view always serializes")
    }

    pub fn now_json(&self) -> String {
        serde_json::to_string(&self.now).expect("now always serializes")
    }
```

`level_dbfs` gets stored rounded to 0.1 dB. `-18.4_f32` serializes as `-18.4` because serde_json prints the shortest f32 representation **(verify: the `status_reports_state_rounded_level_and_rate_limit` test checks it)**.

- [ ] **Step 8: Run the tests to verify they pass, then the canonical check**

Run: `cargo test -p esp-shazam-core`
Expected: PASS, including 7 store tests and 6 audio tests.

Run: `scripts/check`. Expected: all `ok`.

- [ ] **Step 9: Commit**

```bash
git add core/src/lib.rs core/src/audio.rs core/src/store.rs core/Cargo.toml Cargo.lock
git commit -m "feat(core): add sample ring, loudness, tone check, and the song store"
```

---

### Task 7: Toolchain, stock-flash backup, firmware skeleton, bring-up probe

This task settles the spec's open hardware items on the real board: PSRAM, codec revision, and fingerprint time.

`firmware/src/board.rs` and this version of `main.rs` exist only to probe. Task 8 replaces them.

API facts in Tasks 7–11 come from reading the published sources of the pinned crates, plus ESP-IDF v5.5.5, on 2026-10-03. Nothing was compiled.

**Files:**
- Create in `firmware/`:
  - `Cargo.toml`, `.cargo/config.toml`, `rust-toolchain.toml`, `build.rs`
  - `sdkconfig.defaults`, `partitions.csv`
  - `src/main.rs`, `src/board.rs`
- Create: `scripts/flash`, `scripts/serial-log`
- Modify:
  - `mise.toml` (the ESP32 tools and the `esp-toolchain` task)
  - `CLAUDE.md` (the toolchain setup commands)
  - `scripts/check` (firmware stages)
  - the spec's Hardware section and `gotchas.md` (results)
  - this plan's `## Now`

**Interfaces:**
- Consumes: `esp_shazam_core::fingerprinting::algorithm::SignatureGenerator` and `esp_shazam_core::signal::chord_and_chirp` (Task 2).
- Produces:
  - `scripts/flash [--monitor]`: release build, then flash with the ESP-IDF 5.5.5 bootloader and `partitions.csv`.
  - `scripts/serial-log [seconds]`: resets the board and prints its serial output non-interactively.
  - The facts recorded in the spec: PSRAM size, codec revision, and fingerprint milliseconds.

- [ ] **Step 1: Install the ESP32 toolchain with mise**

Replace `mise.toml` with:
```toml
# Toolchain for esp-shazam. `mise install` sets it up; scripts load it with `eval "$(mise env -s bash)"`.
# Then `mise run esp-toolchain` installs the Xtensa Rust toolchain ("esp") the firmware builds with.
[tools]
rust = "1.93.0"  # host toolchain for core, tools and tests
cmake = "3.30.2" # the versions ESP-IDF v5.5.5's tools.json pins
ninja = "1.12.1"
# Prebuilt macOS arm64 binaries from GitHub releases: nothing to compile.
"github:esp-rs/espup" = "0.18.0"
"github:esp-rs/espflash" = { version = "4.6.0", asset_pattern = "espflash-aarch64-apple-darwin.zip" }
"github:esp-rs/embuild" = { version = "0.3.2", version_prefix = "ldproxy-v" } # ships ldproxy

[tasks.esp-toolchain]
description = "Install the Xtensa Rust toolchain 'esp' (plus LLVM) that the firmware builds with"
# 1.98.1.0 is the newest esp-rs toolchain released before esp-idf-sys 0.38.1.
run = "espup install --targets esp32 --std --toolchain-version 1.98.1.0"
```
Notes:
- The espflash release also ships `cargo-espflash-…` zips, hence the explicit `asset_pattern`.
- ldproxy's prebuilt binaries live in the esp-rs/embuild repo under `ldproxy-v*` tags.
- `--std` skips GCC, which esp-idf-sys installs itself.

```bash
mise trust && mise install
mise run esp-toolchain   # writes ~/export-esp.sh and links ~/.espup/esp-clang (esp-idf-sys finds it by default)
eval "$(mise env -s bash)"
espflash --version                  # expect espflash 4.6.0
command -v ldproxy cmake ninja      # all under ~/.local/share/mise/installs
cmake --version | head -1           # expect 3.30.2
rustup toolchain list | grep -w esp # expect an "esp" entry
```
If `mise install` rejects an option, the local mise is older than the docs (it's 2025.10.17; 2026.10.1 is out). Ask Harper before running `mise self-update`.

Firmware builds always run `cargo +esp ...`: mise exports `RUSTUP_TOOLCHAIN=1.93.0`, which outranks `firmware/rust-toolchain.toml`.

Add to `CLAUDE.md` under "Rules for this repo":
```markdown
- **Toolchain from mise.** Run `mise trust && mise install`, then `mise run esp-toolchain`. Never install tools with brew, `rustup toolchain install` or `cargo install`. Build firmware with `cargo +esp` (or `scripts/flash`), because mise's `RUSTUP_TOOLCHAIN` outranks `firmware/rust-toolchain.toml`.
```

- [ ] **Step 2: Back up the stock firmware (irreversible flash erase follows in step 5)**

```bash
mkdir -p backups
esptool --port /dev/cu.usbserial-0001 --baud 921600 read-flash 0 0x400000 backups/stock-flash-2026-10-03.bin
stat -f %z backups/stock-flash-2026-10-03.bin   # expect 4194304
shasum -a 256 backups/stock-flash-2026-10-03.bin
```
Add to `gotchas.md`:
```markdown
## Stock firmware backup
`backups/stock-flash-2026-10-03.bin` (gitignored) is the full 4 MB of Ai-Thinker's v1.1.0 firmware. Restore with
`esptool --port /dev/cu.usbserial-0001 write-flash 0 backups/stock-flash-2026-10-03.bin`.
```

- [ ] **Step 3: Create the firmware project**

`firmware/Cargo.toml`:
```toml
[package]
name = "esp-shazam-firmware"
version = "0.1.0"
edition = "2021"
resolver = "2"
rust-version = "1.82"
license = "GPL-3.0-or-later"
publish = false

[[bin]]
name = "esp-shazam"
harness = false # no test harness on the device

[profile.release]
opt-level = "s"

# The fingerprint is the hot path: optimize it for speed, everything else for size.
[profile.release.package.esp-shazam-core]
opt-level = 3

[profile.release.package.chfft]
opt-level = 3

[profile.dev]
debug = true
opt-level = "z"

[dependencies]
anyhow = "1"
esp-idf-hal = "=0.47.0"
esp-idf-svc = "=0.53.0"
esp-idf-sys = "=0.38.1"
esp-shazam-core = { path = "../core" }
log = "0.4"

[build-dependencies]
embuild = "=0.33.5"

# Its own workspace: the host workspace at the repo root excludes firmware/.
[workspace]
```

`firmware/.cargo/config.toml` (the esp-idf-template's, for esp32 and IDF v5.5.5):
```toml
[build]
target = "xtensa-esp32-espidf"

[target.'cfg(target_os = "espidf")']
linker = "ldproxy"
runner = "espflash flash --monitor"
rustflags = ["--cfg", "espidf_time64"]

[unstable]
build-std = ["std", "panic_abort"]

[env]
MCU = "esp32"
ESP_IDF_VERSION = "v5.5.5"
ESP_IDF_TOOLS_INSTALL_DIR = "workspace"
```

`firmware/rust-toolchain.toml`:
```toml
[toolchain]
channel = "esp"
```

`firmware/build.rs`:
```rust
// ABOUTME: Build script: passes the ESP-IDF build environment from esp-idf-sys on to cargo.
// ABOUTME: Unchanged from the esp-idf-template.

fn main() {
    embuild::espidf::sysenv::output();
}
```

`firmware/sdkconfig.defaults` (every key exists in ESP-IDF v5.5.5):
```
# Rust needs more stack than C's defaults.
CONFIG_ESP_MAIN_TASK_STACK_SIZE=16384
CONFIG_ESP_SYSTEM_EVENT_TASK_STACK_SIZE=4096
CONFIG_FREERTOS_IDLE_TASK_STACKSIZE=4096
CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT=4096

# The ESP32-A1S has 4 MB of flash.
CONFIG_ESPTOOLPY_FLASHSIZE_4MB=y

# Rev v3.1 silicon has the PSRAM cache fix in hardware (Rust code never gets the compiler workaround).
CONFIG_ESP32_REV_MIN_3=y

# External PSRAM. Allocations of 16 KB and up (fingerprint rings, audio ring) go there.
CONFIG_SPIRAM=y
CONFIG_SPIRAM_USE_MALLOC=y
CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=16384
# Boot and report missing PSRAM instead of refusing to start.
CONFIG_SPIRAM_IGNORE_NOTFOUND=y

# A fingerprint keeps core 1 busy for seconds; watch only core 0's idle task.
# CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU1 is not set
```

`firmware/partitions.csv` (passed to espflash at flash time; esp-idf-sys ignores partition settings in sdkconfig):
```
# Name,   Type, SubType, Offset, Size,   Flags
nvs,      data, nvs,     ,       0x6000,
phy_init, data, phy,     ,       0x1000,
factory,  app,  factory, ,       3M,
```

- [ ] **Step 4: Write the bring-up probe**

`firmware/src/board.rs`:
```rust
// ABOUTME: Bring-up only: probes both known Audio Kit I2C pin sets to find which codec revision is fitted.
// ABOUTME: Pin sets come from the Hardware table in the spec.

use anyhow::Result;
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_svc::hal::units::FromValueType;

const ES8388_ADDR: u8 = 0x10;
const AC101_ADDR: u8 = 0x1A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Revision {
    /// ES8388; I2C on SDA 33 / SCL 32; BCLK on GPIO27.
    Es8388V1,
    /// ES8388; I2C on SDA 18 / SCL 23; BCLK on GPIO5.
    Es8388V2,
    /// AC101; I2C on SDA 33 / SCL 32.
    Ac101,
}

fn open<'d>(
    i2c: impl I2c + 'd,
    sda: impl InputPin + OutputPin + 'd,
    scl: impl InputPin + OutputPin + 'd,
) -> Result<I2cDriver<'d>> {
    // I2cConfig defaults to 1 MHz; the codecs want 100 kHz.
    Ok(I2cDriver::new(i2c, sda, scl, &I2cConfig::new().baudrate(100.kHz().into()))?)
}

/// Reads N bytes from `reg`; None when nothing ACKs.
fn read<const N: usize>(bus: &mut I2cDriver<'_>, addr: u8, reg: u8) -> Option<[u8; N]> {
    let mut value = [0u8; N];
    bus.write_read(addr, &[reg], &mut value, TickType::new_millis(50).ticks())
        .ok()
        .map(|()| value)
}

/// Asks the v1 pin set (on I2C0) for an ES8388 or AC101, then the v2 pin set (on I2C1) for an ES8388.
pub fn probe<'d>(
    i2c0: impl I2c + 'd,
    sda_v1: impl InputPin + OutputPin + 'd,
    scl_v1: impl InputPin + OutputPin + 'd,
    i2c1: impl I2c + 'd,
    sda_v2: impl InputPin + OutputPin + 'd,
    scl_v2: impl InputPin + OutputPin + 'd,
) -> Result<Option<Revision>> {
    let mut v1 = open(i2c0, sda_v1, scl_v1)?;
    if let Some([reg08]) = read::<1>(&mut v1, ES8388_ADDR, 0x08) {
        log::info!("bring-up: ES8388 answered on SDA 33 / SCL 32 (reg 0x08 = 0x{reg08:02x})");
        return Ok(Some(Revision::Es8388V1));
    }
    if let Some(id) = read::<2>(&mut v1, AC101_ADDR, 0x00) {
        let id = u16::from_be_bytes(id);
        log::info!("bring-up: 0x1A answered on SDA 33 / SCL 32 (reg 0x00 = 0x{id:04x}; an AC101 reads 0x0101)");
        return Ok((id == 0x0101).then_some(Revision::Ac101));
    }
    let mut v2 = open(i2c1, sda_v2, scl_v2)?;
    if let Some([reg08]) = read::<1>(&mut v2, ES8388_ADDR, 0x08) {
        log::info!("bring-up: ES8388 answered on SDA 18 / SCL 23 (reg 0x08 = 0x{reg08:02x})");
        return Ok(Some(Revision::Es8388V2));
    }
    Ok(None)
}
```

`firmware/src/main.rs`:
```rust
// ABOUTME: Bring-up firmware: logs PSRAM and internal RAM, probes for the codec, and times one fingerprint.
// ABOUTME: Its serial output settles the "(verify)" items in the spec's Hardware section.

mod board;

use std::time::Instant;

use anyhow::Result;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::sys;
use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::signal::chord_and_chirp;

/// The fingerprint needs about 2.1 MB; leave room for the audio ring and buffers.
const PSRAM_NEEDED_BYTES: usize = 3 * 1024 * 1024;

fn main() -> Result<()> {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;

    let psram = log_memory("boot");
    let revision = board::probe(
        peripherals.i2c0,
        peripherals.pins.gpio33,
        peripherals.pins.gpio32,
        peripherals.i2c1,
        peripherals.pins.gpio18,
        peripherals.pins.gpio23,
    )?;
    log::info!("bring-up: codec revision {revision:?}");

    if psram < PSRAM_NEEDED_BYTES {
        log::error!("bring-up: only {} KB of PSRAM; skipping the fingerprint timing", psram / 1024);
        return Ok(());
    }
    let pcm = chord_and_chirp(12);
    let started = Instant::now();
    let signature = SignatureGenerator::make_signature_from_buffer(&pcm);
    let took_ms = started.elapsed().as_millis();
    let bytes = signature.encode_to_binary().map(|b| b.len()).unwrap_or(0);
    log::info!("bring-up: fingerprint of 12 s took {took_ms} ms ({bytes} bytes)");
    log_memory("after fingerprint");
    Ok(())
}

/// Logs memory and returns the PSRAM size in bytes (0 when PSRAM didn't initialize).
fn log_memory(when: &str) -> usize {
    // SAFETY: read-only queries of ESP-IDF's PSRAM and heap state.
    let (psram, psram_free, internal_free) = unsafe {
        (
            sys::esp_psram_get_size(),
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM),
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL),
        )
    };
    log::info!(
        "bring-up: memory at {when}: PSRAM {} KB ({} KB free), internal {} KB free",
        psram / 1024,
        psram_free / 1024,
        internal_free / 1024
    );
    psram
}
```

- [ ] **Step 5: Add the flash and serial-log scripts, build, flash**

`scripts/flash` (then `chmod +x`):
```bash
#!/usr/bin/env bash
# ABOUTME: Builds the release firmware and flashes it with the ESP-IDF 5.5.5 bootloader and partitions.csv.
# ABOUTME: Usage: scripts/flash [--monitor]. Port: $ESPFLASH_PORT, default /dev/cu.usbserial-0001.
set -euo pipefail
cd "$(dirname "$0")/../firmware"
mise_env="$(mise env -s bash)" || { echo "FAIL  mise   (run: mise trust && mise install)"; exit 1; }
eval "$mise_env" # cmake, ninja, ldproxy, espflash from mise.toml
if [ -f "$HOME/export-esp.sh" ]; then . "$HOME/export-esp.sh"; fi
# +esp outranks the RUSTUP_TOOLCHAIN that mise exports
cargo +esp build --release
out=target/xtensa-esp32-espidf/release
# espflash's bundled bootloader comes from IDF 6.1; esp-idf-sys builds the 5.5.5 one next to the ELF.
args=(--port "${ESPFLASH_PORT:-/dev/cu.usbserial-0001}" --chip esp32 --flash-size 4mb
      --partition-table partitions.csv --bootloader "$out/bootloader.bin")
if [ "${1:-}" = "--monitor" ]; then args+=(--monitor); fi
espflash flash "${args[@]}" "$out/esp-shazam"
```

`scripts/serial-log` (then `chmod +x`):
```bash
#!/usr/bin/env bash
# ABOUTME: Resets the board and prints its serial output for N seconds (default 20), for scripted checks.
# ABOUTME: Non-interactive stand-in for `espflash monitor`; uv supplies pyserial on the fly.
set -euo pipefail
secs="${1:-20}"
port="${ESPFLASH_PORT:-/dev/cu.usbserial-0001}"
exec uv run --quiet --with pyserial python3 - "$port" "$secs" <<'PY'
import serial, sys, time
port = serial.Serial(sys.argv[1], 115200, timeout=0.2)
port.dtr = False   # IO0 high: normal boot
port.rts = True    # EN low: hold in reset
time.sleep(0.1)
port.rts = False   # release reset
end = time.time() + float(sys.argv[2])
while time.time() < end:
    chunk = port.read(4096)
    if chunk:
        sys.stdout.write(chunk.decode("utf-8", "replace"))
        sys.stdout.flush()
PY
```

Run:
```bash
scripts/flash             # first build downloads and compiles ESP-IDF v5.5.5 into firmware/.embuild: slow, a few GB
scripts/serial-log 40 | tee target/bringup.log
grep 'bring-up:' target/bringup.log
```
Expected lines:
- `bring-up: memory at boot: PSRAM 4096 KB ...`. Any non-zero PSRAM of at least 3 MB passes.
- `bring-up: codec revision Some(Es8388V1)` (or `Es8388V2`).
- `bring-up: fingerprint of 12 s took N ms (... bytes)`.

If the build fails on the pinned crates, record the exact error in the plan's `## Now` → Open and stop. The spec's fallback, C on ESP-IDF, needs Harper's approval.

- [ ] **Step 6: Decision gate. Record the results, stop if the design doesn't fit**

Update the spec's Hardware section: move PSRAM and the codec revision from "Not yet confirmed" to confirmed, with the logged numbers. Add the fingerprint time to the Risks section.

Then:
- **PSRAM under 3 MB, or 0:** stop. Tell Harper the fingerprint can't fit, and don't continue.
- **Codec `Ac101` or `None`:** stop and tell Harper. Task 8 covers the ES8388 only.
- **`Es8388V2`:** in Tasks 8–11, use `gpio18` (SDA), `gpio23` (SCL) and `gpio5` (BCLK) instead of `gpio33`, `gpio32` and `gpio27`.
- **Fingerprint over 6000 ms:** set the default `search_interval_secs` to the measured seconds rounded up plus 2. Change it in `core/src/policy.rs` `Default` (and its tests) and in the spec's policy table, and note it in `gotchas.md`.

- [ ] **Step 7: Add the firmware to the canonical check**

In `scripts/check`, before `exit $status`:
```bash
if [ -d firmware ]; then
  stage firmware-fmt rustfmt --edition 2021 --check firmware/build.rs firmware/src/*.rs
  stage firmware-build bash -c 'cd firmware && { [ -f "$HOME/export-esp.sh" ] && . "$HOME/export-esp.sh"; true; } && cargo +esp build --release'
  # cargo's summary line names our crates when they produce warnings
  stage firmware-warnings bash -c '! grep -E "^warning: .esp-shazam" target/check-logs/firmware-build.log'
fi
```

Run: `scripts/check`
Expected: every stage `ok`. A no-op firmware rebuild takes about a minute.

- [ ] **Step 8: Commit**

```bash
git add mise.toml CLAUDE.md firmware/Cargo.toml firmware/Cargo.lock firmware/.cargo/config.toml firmware/rust-toolchain.toml \
  firmware/build.rs firmware/sdkconfig.defaults firmware/partitions.csv firmware/src \
  scripts/flash scripts/serial-log scripts/check gotchas.md docs/superpowers
git commit -m "feat(firmware): add ESP32 firmware skeleton and bring-up probe; record board facts"
```

---

### Task 8: ES8388 setup and LINE IN capture

**Files:**
- Create: `firmware/src/codec.rs`, `firmware/src/audio_in.rs`
- Modify: `firmware/src/main.rs` (replace the bring-up version), `firmware/sdkconfig.defaults` (CPU 240 MHz, ledger Ruling R11)
- Delete: `firmware/src/board.rs` (bring-up only; its findings are in the spec)

**Interfaces:**
- Consumes:
  - `esp_shazam_core::audio::{SampleRing, stereo_to_mono, loudness_dbfs, zero_crossing_hz}` (Task 6).
  - The codec revision from Task 7. The code below uses the ES8388 v1 pins; for v2, swap the pins as Task 7 step 6 says.
- Produces:
  - `codec::setup_line_in(i2c: &mut I2cDriver<'_>, pga_gain: u8) -> anyhow::Result<()>` and `codec::PGA_0DB: u8 = 0x00`.
  - `audio_in::start_i2s(i2s, bclk, din, mclk, ws) -> anyhow::Result<I2sDriver<'d, I2sRx>>`: clocks running on return.
  - `audio_in::spawn_capture(rx: I2sDriver<'static, I2sRx>) -> anyhow::Result<SharedRing>`
  - `audio_in::SharedRing = Arc<Mutex<SampleRing>>`
  - `audio_in::SAMPLE_RATE_HZ: u32 = 16_000` and `audio_in::WINDOW_SAMPLES: usize = 192_000`

- [ ] **Step 1: Write the codec setup**

`firmware/src/codec.rs`. The register sequence follows Espressif's esp_codec_dev ES8388 driver, trimmed to the ADC path, with LINE IN on LIN2/RIN2 per the Ai-Thinker A1S pin table and the Audio Kit schematic:
```rust
// ABOUTME: ES8388 setup for recording the LINE IN jack (LIN2/RIN2) at 16 kHz, 16-bit, as an I2S slave.
// ABOUTME: Register sequence from Espressif's esp_codec_dev ES8388 driver, trimmed to the ADC path, checked by readback.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::i2c::I2cDriver;

const ADDR: u8 = 0x10;

/// PGA register value for 0 dB on both channels (3 dB per step per nibble; 0x88 is +24 dB).
/// Line level wants 0 dB: full scale is then 1.0 Vrms.
pub const PGA_0DB: u8 = 0x00;

/// (register, value) in order, up to the state-machine restart.
const SETUP: &[(u8, u8)] = &[
    (0x00, 0x86), // soft-reset pulse: no reset pin, and an ESP32 reset leaves the codec as it was
    (0x00, 0x06), // back to defaults
    (0x01, 0x50), // analog and bias on
    (0x02, 0xFF), // chip power all off...
    (0x02, 0x00), // ...then all on
    (0x08, 0x00), // I2S slave: the chip powers up as master
    (0x00, 0x12), // same sample rate for ADC and DAC, VMID 500k
    (0x2B, 0x80), // ADC and DAC share the one LRCK pin
    (0x03, 0xFF), // ADC off while configuring
    (0x0A, 0x50), // input LIN2/RIN2: the LINE IN jack
    (0x0B, 0x02), // stereo; ADC data output driven
    (0x0C, 0x0C), // I2S Philips format, 16-bit words
    (0x0D, 0x02), // single speed, MCLK = 256 x LRCK
    (0x10, 0x00), // left ADC digital volume 0 dB (powers up at -96 dB)
    (0x11, 0x00), // right ADC digital volume 0 dB
];

/// Final values that prove the setup took.
const EXPECTED: &[(u8, u8)] = &[
    (0x02, 0x00),
    (0x03, 0x00),
    (0x08, 0x00),
    (0x0A, 0x50),
    (0x0C, 0x0C),
    (0x10, 0x00),
    (0x11, 0x00),
];

/// Needs MCLK, BCLK and LRCK already running: the codec locks onto them when its state machines restart.
pub fn setup_line_in(i2c: &mut I2cDriver<'_>, pga_gain: u8) -> Result<()> {
    for &(reg, value) in SETUP {
        write(i2c, reg, value)?;
    }
    write(i2c, 0x09, pga_gain)?;
    write(i2c, 0x02, 0xF0)?; // restart the state machines with the final configuration
    std::thread::sleep(Duration::from_millis(2));
    write(i2c, 0x02, 0x00)?;
    write(i2c, 0x03, 0x00)?; // ADC, analog inputs and bias on

    for &(reg, want) in EXPECTED {
        expect(i2c, reg, want)?;
    }
    expect(i2c, 0x09, pga_gain)?;
    log::info!("codec: ES8388 set up for LINE IN, PGA 0x{pga_gain:02x}");
    Ok(())
}

fn expect(i2c: &mut I2cDriver<'_>, reg: u8, want: u8) -> Result<()> {
    let got = read(i2c, reg)?;
    if got != want {
        bail!("ES8388 register 0x{reg:02x} reads 0x{got:02x}, expected 0x{want:02x}");
    }
    Ok(())
}

fn write(i2c: &mut I2cDriver<'_>, reg: u8, value: u8) -> Result<()> {
    i2c.write(ADDR, &[reg, value], TickType::new_millis(50).ticks())
        .with_context(|| format!("ES8388 write 0x{reg:02x} = 0x{value:02x}"))
}

fn read(i2c: &mut I2cDriver<'_>, reg: u8) -> Result<u8> {
    let mut value = [0u8; 1];
    i2c.write_read(ADDR, &[reg], &mut value, TickType::new_millis(50).ticks())
        .with_context(|| format!("ES8388 read 0x{reg:02x}"))?;
    Ok(value[0])
}
```

- [ ] **Step 2: Write the capture path**

`firmware/src/audio_in.rs`:
```rust
// ABOUTME: Captures LINE IN over I2S at 16 kHz, averages left and right to mono, and keeps the newest 12 s.
// ABOUTME: Runs on its own high-priority thread on core 0; everything else reads the shared ring.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::delay::TickType;
use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2s::config::{DataBitWidth, StdConfig};
use esp_idf_svc::hal::i2s::{I2s, I2sDriver, I2sRx};
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_shazam_core::audio::{stereo_to_mono, SampleRing};

pub const SAMPLE_RATE_HZ: u32 = 16_000;
pub const WINDOW_SAMPLES: usize = 12 * SAMPLE_RATE_HZ as usize;

pub type SharedRing = Arc<Mutex<SampleRing>>;

/// I2S RX with clocks running (MCLK 256 x 16 kHz = 4.096 MHz on GPIO0, BCLK 512 kHz).
/// Start it before the codec setup.
pub fn start_i2s<'d>(
    i2s: impl I2s + 'd,
    bclk: impl InputPin + OutputPin + 'd,
    din: impl InputPin + 'd,
    mclk: impl InputPin + OutputPin + 'd,
    ws: impl InputPin + OutputPin + 'd,
) -> Result<I2sDriver<'d, I2sRx>> {
    let config = StdConfig::philips(SAMPLE_RATE_HZ, DataBitWidth::Bits16);
    let mut rx = I2sDriver::<I2sRx>::new_std_rx(i2s, &config, bclk, din, Some(mclk), ws)?;
    rx.rx_enable()?; // mandatory: read() needs the channel running
    Ok(rx)
}

/// Spawns the capture thread and returns the ring it fills. The first 500 ms are dropped
/// while the codec's reference voltage settles.
pub fn spawn_capture(mut rx: I2sDriver<'static, I2sRx>) -> Result<SharedRing> {
    let ring: SharedRing = Arc::new(Mutex::new(SampleRing::new(WINDOW_SAMPLES)));
    let writer = ring.clone();
    ThreadSpawnConfiguration {
        name: Some(c"audio"),
        priority: 10,
        pin_to_core: Some(Core::Core0),
        ..Default::default()
    }
    .set()?;
    // Only Builder::stack_size sets a std thread's stack; ThreadSpawnConfiguration's is ignored.
    let spawned = std::thread::Builder::new().stack_size(8 * 1024).spawn(move || {
        let mut bytes = vec![0u8; 1280]; // 20 ms of 16-bit stereo
        let mut frames: Vec<i16> = Vec::with_capacity(640);
        let mut mono: Vec<i16> = Vec::with_capacity(320);
        let mut settle = SAMPLE_RATE_HZ as usize / 2;
        loop {
            let n = match rx.read(&mut bytes, TickType::new_millis(1000).ticks()) {
                Ok(n) => n - n % 4, // whole stereo frames only
                Err(e) => {
                    log::warn!("audio: I2S read failed: {e}");
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            frames.clear();
            frames.extend(bytes[..n].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
            stereo_to_mono(&frames, &mut mono);
            if settle > 0 {
                settle = settle.saturating_sub(mono.len());
                continue;
            }
            writer.lock().expect("audio ring lock").push(&mono);
        }
    });
    ThreadSpawnConfiguration::default().set()?;
    spawned?;
    Ok(ring)
}
```

- [ ] **Step 3: Run the CPU at 240 MHz, and replace main with a level meter**

Add to `firmware/sdkconfig.defaults` (ESP-IDF's ESP32 default is 160 MHz; Task 7 measured a 12 s fingerprint at 12.6 s on 160 MHz and 10.5 s on 240 MHz):
```
# Full speed: the fingerprint is CPU- and PSRAM-bound (ESP-IDF's ESP32 default is 160 MHz).
CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ_240=y
```


Delete `firmware/src/board.rs`. `firmware/src/main.rs`:
```rust
// ABOUTME: Firmware entry point: sets up LINE IN capture and logs level, peak and pitch once a second.
// ABOUTME: Wi-Fi, Shazam, the web page and MQTT join in later.

mod audio_in;
mod codec;

use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::units::FromValueType;
use esp_shazam_core::audio::{loudness_dbfs, zero_crossing_hz};

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;

    // I2S first: the codec locks onto MCLK and LRCK when its state machines restart.
    let rx = audio_in::start_i2s(
        peripherals.i2s0,
        peripherals.pins.gpio27, // BCLK (v2 boards: gpio5)
        peripherals.pins.gpio35, // DIN
        peripherals.pins.gpio0,  // MCLK
        peripherals.pins.gpio25, // WS
    )?;
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio33, // SDA (v2 boards: gpio18)
        peripherals.pins.gpio32, // SCL (v2 boards: gpio23)
        &I2cConfig::new().baudrate(100.kHz().into()),
    )?;
    codec::setup_line_in(&mut i2c, codec::PGA_0DB)?;
    let ring = audio_in::spawn_capture(rx)?;

    let mut second = Vec::with_capacity(audio_in::SAMPLE_RATE_HZ as usize);
    loop {
        std::thread::sleep(Duration::from_secs(1));
        ring.lock()
            .expect("audio ring lock")
            .copy_latest(audio_in::SAMPLE_RATE_HZ as usize, &mut second);
        let peak = second.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        log::info!(
            "audio: level {:.1} dBFS, peak {peak}, pitch {:.0} Hz",
            loudness_dbfs(&second),
            zero_crossing_hz(&second, audio_in::SAMPLE_RATE_HZ)
        );
    }
}
```

- [ ] **Step 4: Flash and verify capture with a 1 kHz tone, music, and silence**

The Mac's headphone jack is cabled to LINE IN. Every Mac audio output goes to the board while the cable is in.
```bash
ffmpeg -v error -y -f lavfi -i "sine=frequency=1000:sample_rate=44100:duration=15" -ac 2 target/tone-1k.wav
old=$(osascript -e 'output volume of (get volume settings)'); osascript -e 'set volume output volume 50'
scripts/flash
(sleep 6; afplay target/tone-1k.wav) & scripts/serial-log 25 | tee target/tone.log
(sleep 6; afplay -t 15 "$(mdfind -name 'P.Y.T. (Pretty Young Thing)' 2>/dev/null | grep -E '\.m4a$' | head -1)") & scripts/serial-log 25 | tee target/music.log
scripts/serial-log 15 | tee target/silence.log
osascript -e "set volume output volume $old"
grep -E 'codec:|audio:' target/tone.log target/music.log target/silence.log
```
Expected:
- `codec: ES8388 set up for LINE IN, PGA 0x00` in each log.
- Tone: `pitch 1000 Hz` (±5) and a level between -30 and -3 dBFS, with `peak` under 32000.
- Music: levels mostly between -35 and -6 dBFS.
- Silence: level at or below -60 dBFS. That's the noise floor, and it must sit at least 10 dB below `quiet_threshold_dbfs` (-50). Measure it with the cable plugged in and nothing playing: loudness removes DC but not hum, so any hum counts toward the floor.

If capture is silent or the pitch is wrong, check in this order:
1. Readback errors in the log.
2. The BCLK pin (27 vs 5).
3. Registers `0x10`/`0x11`.
4. `0x2B`.
5. `0x0A` = `0x00` (LIN1) as a last resort, to see whether the jack is wired to the other input.

Report what you found; don't guess.

If the tone clips (peak ≥ 32000) at volume 50, use volume 35 for every later test, and record that. If music stays under -45 dBFS, raise the PGA to `0x33` (+9 dB).

Record in the spec's Hardware section and `gotchas.md`:
- the noise floor, tone and music levels,
- the PGA value and Mac test volume chosen,
- the sample interleave order, if the tone's pitch revealed it.

- [ ] **Step 5: Run the canonical check and commit**

Run: `scripts/check`. Expected: all `ok`.
```bash
git rm firmware/src/board.rs
git add firmware/src gotchas.md docs/superpowers
git commit -m "feat(firmware): set up the ES8388 for LINE IN and capture 16 kHz mono into a ring"
```
---

### Task 9: Wi-Fi, clock, and the recognizer thread

The board identifies songs on its own and logs them over serial. The web page and MQTT come in Tasks 10 and 11.

**Files:**
- Create in `firmware/`:
  - `src/config.rs`, `cfg.toml.example`
  - `src/net.rs`, `src/shazam_http.rs`, `src/recognizer.rs`
- Modify in `firmware/`: `Cargo.toml` (add toml-cfg), `.cargo/config.toml` (`TOML_CFG`), `src/main.rs`
- Harper creates `firmware/cfg.toml` from the example. It holds the Wi-Fi password and stays gitignored. Don't print it, and don't commit it.

**Interfaces:**
- Consumes:
  - `audio_in::{start_i2s, spawn_capture, SharedRing, SAMPLE_RATE_HZ, WINDOW_SAMPLES}` and `codec::setup_line_in` (Task 8).
  - From core: `policy::*` (Task 5), `store::SongStore` (Task 6), and `shazam::{build_request, sample_ms, uuid_v4, user_agent, RequestContext, parse_response, Song}` (Tasks 3–4).
  - `time::rfc3339_utc` (Task 1) and `fingerprinting::algorithm::SignatureGenerator` (Task 2).
- Produces:
  - `config::CONFIG` (fields below) and `config::policy_config() -> PolicyConfig`
  - `net::start_wifi(...)`, `net::supervise(wifi) -> !`, `net::start_sntp()`, `net::clock_synced() -> bool`, `net::unix_secs()`, `net::unix_ms()`
  - `shazam_http::recognize(&Request) -> (Outcome, Option<Song>)`
  - `recognizer::SharedStore = Arc<Mutex<SongStore>>`
  - `recognizer::spawn(ring, store, config, on_change: impl Fn() + Send + 'static) -> anyhow::Result<()>`

- [ ] **Step 1: Config from cfg.toml**

`firmware/Cargo.toml` `[dependencies]` gains `toml-cfg = "0.2"`.

`firmware/.cargo/config.toml` `[env]` gains this line. It turns a missing `cfg.toml` into a build error instead of silent defaults:
```toml
TOML_CFG = "require_cfg_present"
```

`firmware/cfg.toml.example`:
```toml
# Copy to firmware/cfg.toml (gitignored) and fill in. Values must not contain " or \ (a toml-cfg limit).
# After creating cfg.toml for the first time, run `touch src/config.rs` so cargo picks it up.
[esp-shazam-firmware]
wifi_ssid = "your-network"
wifi_pass = "your-password"
mqtt_url = "mqtt://192.168.200.8:1883"
hostname = "esp-shazam"
pga_gain = 0
quiet_threshold_dbfs = -60.0 # tuned at bring-up: floor about -79 dBFS (SDD Ruling R12)
search_interval_secs = 8
slow_search_interval_secs = 30
recheck_interval_secs = 30
clear_after_quiet_secs = 60
rate_limit_backoff_secs = 300
```
Use whatever `pga_gain` and `quiet_threshold_dbfs` Task 8 recorded in the spec, if they differ from these.

`firmware/src/config.rs`:
```rust
// ABOUTME: Build-time settings from firmware/cfg.toml (gitignored): Wi-Fi, MQTT, codec gain, listen-policy timings.
// ABOUTME: Defaults match the spec; cfg.toml.example lists every key.

use esp_shazam_core::policy::PolicyConfig;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_pass: &'static str,
    #[default("")]
    mqtt_url: &'static str,
    #[default("esp-shazam")]
    hostname: &'static str,
    #[default(0)]
    pga_gain: u8,
    #[default(-50.0)]
    quiet_threshold_dbfs: f32,
    #[default(8)]
    search_interval_secs: u32,
    #[default(30)]
    slow_search_interval_secs: u32,
    #[default(30)]
    recheck_interval_secs: u32,
    #[default(60)]
    clear_after_quiet_secs: u32,
    #[default(300)]
    rate_limit_backoff_secs: u32,
}

/// The listen policy with cfg.toml's timings; counts the spec doesn't expose stay at their defaults.
pub fn policy_config() -> PolicyConfig {
    let secs = |s: u32| u64::from(s) * 1_000;
    PolicyConfig {
        quiet_threshold_dbfs: CONFIG.quiet_threshold_dbfs,
        search_interval_ms: secs(CONFIG.search_interval_secs),
        slow_search_interval_ms: secs(CONFIG.slow_search_interval_secs),
        recheck_interval_ms: secs(CONFIG.recheck_interval_secs),
        clear_after_quiet_ms: secs(CONFIG.clear_after_quiet_secs),
        rate_limit_backoff_ms: secs(CONFIG.rate_limit_backoff_secs),
        ..PolicyConfig::default()
    }
}
```
Ask Harper to create `firmware/cfg.toml` from the example and fill in the Wi-Fi name and password. Wait until `test -f firmware/cfg.toml && git check-ignore -q firmware/cfg.toml` succeeds.

- [ ] **Step 2: Wi-Fi supervisor and clock**

`firmware/src/net.rs`:
```rust
// ABOUTME: Wi-Fi station with a reconnect supervisor, plus SNTP wall-clock time.
// ABOUTME: ESP-IDF never reconnects by itself, so `supervise` owns the link on the main thread.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// Before SNTP sets it, the clock starts at 1970. Anything earlier than 2026-01-01T00:00:00Z means "not synced".
const CLOCK_SANE_AFTER_SECS: u64 = 1_767_225_600;

pub fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn unix_secs() -> u64 {
    unix_ms() / 1_000
}

/// True once SNTP has set the wall clock; Shazam requests carry a timestamp.
pub fn clock_synced() -> bool {
    unix_secs() > CLOCK_SANE_AFTER_SECS
}

/// Starts Wi-Fi and tries one connect. A failed first connect is logged, not fatal: `supervise` keeps trying.
pub fn start_wifi(
    modem: impl WifiModemPeripheral + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    ssid: &str,
    pass: &str,
) -> Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().map_err(|_| anyhow!("wifi_ssid is longer than 32 bytes"))?,
        password: pass.try_into().map_err(|_| anyhow!("wifi_pass is longer than 64 bytes"))?,
        auth_method: if pass.is_empty() { AuthMethod::None } else { AuthMethod::WPA2Personal },
        ..Default::default()
    }))?;
    wifi.start()?;
    connect(&mut wifi);
    Ok(wifi)
}

fn connect(wifi: &mut BlockingWifi<EspWifi<'static>>) -> bool {
    match wifi.connect().and_then(|()| wifi.wait_netif_up()) {
        Ok(()) => {
            match wifi.wifi().sta_netif().get_ip_info() {
                Ok(info) => log::info!("wifi: connected, IP {}", info.ip),
                Err(e) => log::info!("wifi: connected (IP unknown: {e})"),
            }
            true
        }
        Err(e) => {
            log::warn!("wifi: connect failed: {e}");
            false
        }
    }
}

/// Never returns. Checks the link every 5 s and reconnects when it has dropped.
pub fn supervise(mut wifi: BlockingWifi<EspWifi<'static>>) -> ! {
    let mut was_up = wifi.is_connected().unwrap_or(false);
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if wifi.is_connected().unwrap_or(false) {
            was_up = true;
            continue;
        }
        if was_up {
            log::warn!("wifi: disconnected");
            was_up = false;
        }
        if connect(&mut wifi) {
            log::info!("wifi: reconnected");
            was_up = true;
        }
    }
}

/// Keep the returned handle alive: dropping it stops SNTP.
pub fn start_sntp() -> Result<EspSntp<'static>> {
    Ok(EspSntp::new_default()?)
}
```

- [ ] **Step 3: The Shazam HTTPS call**

`firmware/src/shazam_http.rs`:
```rust
// ABOUTME: Sends a Shazam recognize request over HTTPS and turns the reply into a listen-policy Outcome.
// ABOUTME: Verifies TLS against the ESP-IDF certificate bundle and reads at most 128 KB of body.

use std::time::Duration;

use anyhow::{bail, Result};
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_shazam_core::policy::Outcome;
use esp_shazam_core::shazam::{parse_response, Request, Song};

const MAX_BODY_BYTES: usize = 128 * 1024;

/// What Shazam said, as the policy sees it, plus the song on a match.
pub fn recognize(request: &Request) -> (Outcome, Option<Song>) {
    match post(request) {
        Ok((200, body)) => match parse_response(&body) {
            Ok(Some(song)) => (Outcome::Match { key: song.key.clone() }, Some(song)),
            Ok(None) => (Outcome::Miss, None),
            Err(e) => {
                log::warn!("shazam: unusable 200 response ({e}), {} bytes", body.len());
                (Outcome::Failed, None)
            }
        },
        Ok((429, _)) => {
            log::warn!("shazam: rate-limited (HTTP 429)");
            (Outcome::RateLimited, None)
        }
        Ok((status, body)) => {
            log::warn!("shazam: HTTP {status}, {} bytes", body.len());
            (Outcome::Failed, None)
        }
        Err(e) => {
            log::warn!("shazam: request failed: {e:#}");
            (Outcome::Failed, None)
        }
    }
}

fn post(request: &Request) -> Result<(u16, Vec<u8>)> {
    let mut conn = EspHttpConnection::new(&Configuration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        timeout: Some(Duration::from_secs(20)),
        buffer_size: Some(4096),
        buffer_size_tx: Some(1024), // the request line carries a ~250-byte URL
        ..Default::default()
    })?;
    let length = request.body.len().to_string();
    let mut headers: Vec<(&str, &str)> = request.headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    // Without Content-Length, esp_http_client sends the body chunked.
    headers.push(("Content-Length", length.as_str()));
    conn.initiate_request(Method::Post, &request.url, &headers)?;
    conn.write_all(request.body.as_bytes())?;
    conn.initiate_response()?;
    let status = conn.status();
    let mut body = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if body.len() + n > MAX_BODY_BYTES {
            bail!("response larger than {MAX_BODY_BYTES} bytes");
        }
        body.extend_from_slice(&buf[..n]);
    }
    Ok((status, body))
}
```

- [ ] **Step 4: The recognizer thread**

`firmware/src/recognizer.rs`:
```rust
// ABOUTME: The recognizer thread: every 250 ms it feeds loudness to the listen policy; when the policy says ask,
// ABOUTME: it fingerprints the last 12 s, asks Shazam, and applies the answer to the song store.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_shazam_core::audio::loudness_dbfs;
use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::policy::{Change, Decision, Outcome, Policy, PolicyConfig};
use esp_shazam_core::shazam::{self, RequestContext, Song};
use esp_shazam_core::store::SongStore;
use esp_shazam_core::time::rfc3339_utc;

use crate::audio_in::{SharedRing, SAMPLE_RATE_HZ, WINDOW_SAMPLES};
use crate::{net, shazam_http};

pub type SharedStore = Arc<Mutex<SongStore>>;

const TICK: Duration = Duration::from_millis(250);

/// Runs on core 1, leaving core 0 to Wi-Fi and audio while a fingerprint takes seconds.
/// `on_change` runs after every change to "now" (a new song or a clear).
pub fn spawn(
    ring: SharedRing,
    store: SharedStore,
    config: PolicyConfig,
    on_change: impl Fn() + Send + 'static,
) -> Result<()> {
    ThreadSpawnConfiguration {
        name: Some(c"recognizer"),
        priority: 5,
        pin_to_core: Some(Core::Core1),
        ..Default::default()
    }
    .set()?;
    // Fingerprint, JSON and the TLS handshake all run on this stack.
    let spawned = std::thread::Builder::new()
        .stack_size(32 * 1024)
        .spawn(move || run(ring, store, config, on_change));
    ThreadSpawnConfiguration::default().set()?;
    spawned?;
    Ok(())
}

fn run(ring: SharedRing, store: SharedStore, config: PolicyConfig, on_change: impl Fn()) {
    let boot = Instant::now();
    let now_ms = || boot.elapsed().as_millis() as u64;
    let mut policy = Policy::new(config);
    let mut second = Vec::with_capacity(SAMPLE_RATE_HZ as usize);
    let mut window = Vec::with_capacity(WINDOW_SAMPLES);
    loop {
        std::thread::sleep(TICK);
        ring.lock().expect("audio ring lock").copy_latest(SAMPLE_RATE_HZ as usize, &mut second);
        let level = loudness_dbfs(&second);
        let has_now = store.lock().expect("store lock").has_now();
        match policy.tick(now_ms(), level, net::clock_synced(), has_now) {
            Decision::Idle => {}
            Decision::ClearNow => {
                if store.lock().expect("store lock").clear_now() {
                    log::info!("now playing: cleared after quiet");
                    on_change();
                }
            }
            Decision::Ask => {
                ring.lock().expect("audio ring lock").copy_latest(WINDOW_SAMPLES, &mut window);
                let (outcome, song) = ask_shazam(&window);
                let current_key = store.lock().expect("store lock").now_key().map(str::to_owned);
                match (policy.on_outcome(now_ms(), &outcome, current_key.as_deref()), song) {
                    (Change::SetNow, Some(song)) => {
                        log::info!("now playing: {} — {}", song.title, song.artist);
                        store.lock().expect("store lock").set_now(song, rfc3339_utc(net::unix_secs()));
                        on_change();
                    }
                    (Change::ClearNow, _) => {
                        store.lock().expect("store lock").clear_now();
                        log::info!("now playing: cleared after misses");
                        on_change();
                    }
                    _ => {}
                }
            }
        }
        let now = now_ms();
        let mut store = store.lock().expect("store lock");
        let rate_limited_until = policy
            .rate_limited_until(now)
            .map(|until| rfc3339_utc(net::unix_secs() + (until - now) / 1_000));
        let state = policy.state(now, store.has_now());
        store.set_status(state, level, rate_limited_until);
    }
}

fn ask_shazam(window: &[i16]) -> (Outcome, Option<Song>) {
    let started = Instant::now();
    let signature = SignatureGenerator::make_signature_from_buffer(window);
    let uri = match signature.encode_to_uri() {
        Ok(uri) => uri,
        Err(e) => {
            log::error!("shazam: encoding the signature failed: {e}");
            return (Outcome::Failed, None);
        }
    };
    let fingerprint_ms = started.elapsed().as_millis();
    let uuid_a = shazam::uuid_v4(random_bytes());
    let uuid_b = shazam::uuid_v4(random_bytes());
    let ctx = RequestContext {
        unix_ms: net::unix_ms(),
        uuid_a: &uuid_a,
        uuid_b: &uuid_b,
        user_agent: shazam::user_agent(usize::from(random_bytes()[0])),
    };
    let request = shazam::build_request(
        &uri,
        shazam::sample_ms(signature.number_samples, signature.sample_rate_hz),
        &ctx,
    );
    let answer = shazam_http::recognize(&request);
    log::info!(
        "shazam: {:?} (fingerprint {fingerprint_ms} ms, total {} ms)",
        answer.0,
        started.elapsed().as_millis()
    );
    answer
}

fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    // SAFETY: esp_fill_random writes exactly `len` bytes into the buffer.
    unsafe { esp_idf_svc::sys::esp_fill_random(bytes.as_mut_ptr().cast(), bytes.len()) };
    bytes
}
```
If `esp_fill_random` isn't in the generated bindings, declare it yourself:
```rust
unsafe extern "C" { fn esp_fill_random(buf: *mut core::ffi::c_void, len: usize); }
```
The C symbol exists on ESP-IDF.

- [ ] **Step 5: Wire it up in main**

`firmware/src/main.rs`:
```rust
// ABOUTME: Firmware entry point: LINE IN capture, Wi-Fi, clock, and the recognizer around one song store.
// ABOUTME: The main thread ends up supervising Wi-Fi; everything else runs on its own thread.

mod audio_in;
mod codec;
mod config;
mod net;
mod recognizer;
mod shazam_http;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::units::FromValueType;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_shazam_core::store::SongStore;

use config::CONFIG;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // I2S first: the codec locks onto MCLK and LRCK when its state machines restart.
    let rx = audio_in::start_i2s(
        peripherals.i2s0,
        peripherals.pins.gpio27, // BCLK (v2 boards: gpio5)
        peripherals.pins.gpio35, // DIN
        peripherals.pins.gpio0,  // MCLK
        peripherals.pins.gpio25, // WS
    )?;
    let mut i2c = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio33, // SDA (v2 boards: gpio18)
        peripherals.pins.gpio32, // SCL (v2 boards: gpio23)
        &I2cConfig::new().baudrate(100.kHz().into()),
    )?;
    codec::setup_line_in(&mut i2c, CONFIG.pga_gain)?;
    let ring = audio_in::spawn_capture(rx)?;

    let wifi = net::start_wifi(peripherals.modem, sysloop, nvs, CONFIG.wifi_ssid, CONFIG.wifi_pass)?;
    let _sntp = net::start_sntp()?;

    let store: recognizer::SharedStore = Arc::new(Mutex::new(SongStore::new(20)));
    let log_store = store.clone();
    recognizer::spawn(ring, store, config::policy_config(), move || {
        log::info!("now_playing payload: {}", log_store.lock().expect("store lock").now_json());
    })?;

    net::supervise(wifi)
}
```

- [ ] **Step 6: Flash and watch it identify what's playing (a few Shazam requests)**

LINE IN is wired to Harper's audio source, which plays continuously; the Mac can't play into it. `firmware/cfg.toml` already carries `quiet_threshold_dbfs = -60.0` (Ruling R12). Never print that file.
```bash
touch firmware/src/config.rs   # make toml-cfg re-read cfg.toml
scripts/flash
scripts/serial-log 120 | tee target/recognize.log
grep -E 'wifi:|shazam:|now playing' target/recognize.log
```
Expected:
- `wifi: connected, IP …` within about 10 s of boot.
- `shazam: Match …` with its fingerprint and total time, then `now playing: <title> — <artist>`, within 45 s of boot. The source is already playing, so the first ask comes 8 s after the 500 ms settle.
- While matched, one `shazam:` re-check about every 30 s plus the fingerprint time, and no other requests.

Record the title and the fingerprint and total milliseconds in the report and in the spec's Risks section.

If two minutes pass with only misses or failures, report the `shazam:` lines and stop with DONE_WITH_CONCERNS. Never loop requests.

Clearing after quiet needs the source stopped, so Task 12 tests it with Harper.

- [ ] **Step 7: Run the canonical check and commit**

Run: `scripts/check`. Expected: all `ok`.
```bash
git add firmware/Cargo.toml firmware/Cargo.lock firmware/.cargo/config.toml firmware/cfg.toml.example firmware/src docs/superpowers
git commit -m "feat(firmware): recognize songs on the board: Wi-Fi, clock, Shazam over HTTPS, listen policy"
```
`git status` must not list `firmware/cfg.toml`; it is gitignored.

---

### Task 10: Web page, JSON endpoint, and mDNS

**Files:**
- Create: `firmware/src/web.rs`, `firmware/src/index.html`
- Modify:
  - `firmware/src/net.rs` (add `start_mdns`)
  - `firmware/src/main.rs`
  - `firmware/Cargo.toml` (the mdns component)

**Interfaces:**
- Consumes: `recognizer::SharedStore` and `SongStore::state_json()` (Tasks 6 and 9), and `config::CONFIG.hostname` (Task 9).
- Produces:
  - `web::start(store: SharedStore) -> anyhow::Result<EspHttpServer<'static>>`, serving `GET /` and `GET /api/state`.
  - `net::start_mdns(hostname: &str) -> anyhow::Result<EspMdns>`, which publishes `<hostname>.local` and `_http._tcp` on port 80.

- [ ] **Step 1: Pull in the mDNS component**

Append to `firmware/Cargo.toml`. The `espressif/mdns` managed component is what turns on `esp_idf_svc::mdns`:
```toml
[[package.metadata.esp-idf-sys.extra_components]]
remote_component = { name = "espressif/mdns", version = "1.8.2" }
```
- The first build fetches the component over the network.
- If it produces a lock file (`components_esp32.lock`) in `firmware/`, commit it.
- If it creates `firmware/managed_components/`, gitignore that directory.

Add to `firmware/src/net.rs` (it needs `use esp_idf_svc::mdns::EspMdns;`):
```rust
/// Publishes `<hostname>.local` and an HTTP service on port 80. Keep the handle alive.
pub fn start_mdns(hostname: &str) -> Result<EspMdns> {
    let mut mdns = EspMdns::take()?;
    mdns.set_hostname(hostname)?;
    mdns.set_instance_name("esp-shazam")?;
    mdns.add_service(Some("esp-shazam"), "_http", "_tcp", 80, &[("path", "/")])?;
    Ok(mdns)
}
```

- [ ] **Step 2: The HTTP server**

`firmware/src/web.rs`:
```rust
// ABOUTME: HTTP server: the now-playing page at / and the JSON state at /api/state.
// ABOUTME: Both read the one song store; the page polls the JSON every 3 s.

use anyhow::{anyhow, Result};
use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;

use crate::recognizer::SharedStore;

const INDEX_HTML: &str = include_str!("index.html");

/// Keep the returned server alive: dropping it stops serving.
pub fn start(store: SharedStore) -> Result<EspHttpServer<'static>> {
    // Handlers serialize up to 20 history entries; the default 6 KB stack is tight for that.
    let mut server = EspHttpServer::new(&Configuration { stack_size: 10 * 1024, ..Default::default() })?;
    server.fn_handler::<anyhow::Error, _>("/", Method::Get, |request| {
        request
            .into_response(200, Some("OK"), &[("Content-Type", "text/html; charset=utf-8")])?
            .write_all(INDEX_HTML.as_bytes())?;
        Ok(())
    })?;
    server.fn_handler::<anyhow::Error, _>("/api/state", Method::Get, move |request| {
        let body = store.lock().map_err(|_| anyhow!("song store lock poisoned"))?.state_json();
        request
            .into_response(
                200,
                Some("OK"),
                &[("Content-Type", "application/json; charset=utf-8"), ("Cache-Control", "no-store")],
            )?
            .write_all(body.as_bytes())?;
        Ok(())
    })?;
    Ok(server)
}
```
If `?` can't convert `EspIOError` into `anyhow::Error`, use `esp_idf_svc::io::EspIOError` as the handler's error type and map the lock error with `EspIOError(EspError::from_infallible::<{ esp_idf_svc::sys::ESP_FAIL }>())`. Don't use `unwrap`.

- [ ] **Step 3: The page**

`firmware/src/index.html` (self-contained: no external scripts or styles; cover art loads from Shazam's CDN):
```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>esp-shazam</title>
<style>
  :root { color-scheme: light dark; --bg: #f6f5f2; --fg: #1b1b1b; --muted: #6b6b6b; --card: #ffffff; --accent: #0a66d8; }
  @media (prefers-color-scheme: dark) { :root { --bg: #121212; --fg: #ececec; --muted: #9a9a9a; --card: #1d1d1d; --accent: #6aa8ff; } }
  [hidden] { display: none !important; }
  body { margin: 0; font: 16px/1.45 system-ui, sans-serif; background: var(--bg); color: var(--fg); }
  main { max-width: 34rem; margin: 0 auto; padding: 1.5rem 1rem; }
  .now { display: flex; gap: 1rem; align-items: center; background: var(--card); border-radius: 12px; padding: 1rem; }
  .now img { width: 96px; height: 96px; border-radius: 8px; object-fit: cover; background: var(--muted); flex: none; }
  .title { font-size: 1.3rem; font-weight: 600; }
  .muted { color: var(--muted); font-size: .9rem; }
  h2 { font-size: 1rem; margin-top: 2rem; }
  ol { padding-left: 1.2rem; }
  li { margin: .35rem 0; }
  a { color: var(--accent); }
</style>
</head>
<body>
<main>
  <p class="muted" id="status">connecting…</p>
  <section class="now" id="now" hidden>
    <img id="cover" alt="">
    <div>
      <div class="title" id="title"></div>
      <div id="artist"></div>
      <div class="muted" id="album"></div>
      <a id="link" target="_blank" rel="noopener">Open in Shazam</a>
    </div>
  </section>
  <p class="muted" id="idle">Nothing playing.</p>
  <h2 class="muted">Earlier</h2>
  <ol id="history"></ol>
</main>
<script>
const $ = (id) => document.getElementById(id);
async function refresh() {
  try {
    const s = await (await fetch('/api/state', { cache: 'no-store' })).json();
    const paused = s.rate_limited_until ? ` · paused until ${s.rate_limited_until}` : '';
    $('status').textContent = `${s.state.replace('_', ' ')} · ${s.level_dbfs.toFixed(1)} dBFS${paused}`;
    const n = s.now;
    $('now').hidden = !n;
    $('idle').hidden = Boolean(n);
    if (n) {
      $('title').textContent = n.title;
      $('artist').textContent = n.artist;
      $('album').textContent = n.album || '';
      if (n.cover_url) { $('cover').src = n.cover_url; } else { $('cover').removeAttribute('src'); }
      $('link').hidden = !n.shazam_url;
      if (n.shazam_url) { $('link').href = n.shazam_url; }
    }
    $('history').replaceChildren(...s.history.map((h) => {
      const li = document.createElement('li');
      li.textContent = `${h.title} — ${h.artist} (${h.identified_at.replace('T', ' ').replace('Z', ' UTC')})`;
      return li;
    }));
  } catch (e) {
    $('status').textContent = 'board unreachable, retrying…';
  }
}
refresh();
setInterval(refresh, 3000);
</script>
</body>
</html>
```
Every song field goes through `textContent`, never `innerHTML`, so titles can't inject markup.

- [ ] **Step 4: Wire it up**

In `firmware/src/main.rs`:
- add `mod web;`
- right after `let _sntp = net::start_sntp()?;`, add `let _mdns = net::start_mdns(CONFIG.hostname)?;`
- after creating the store, and before `recognizer::spawn`, add `let _server = web::start(store.clone())?;`

All three handles must stay bound until `net::supervise(wifi)`, which never returns.

- [ ] **Step 5: Flash and check both routes**

```bash
scripts/flash
sleep 20
curl -s --max-time 5 http://esp-shazam.local/api/state | jq .
curl -sI --max-time 5 http://esp-shazam.local/api/state | grep -i '^content-type: application/json; charset=utf-8'
curl -s --max-time 5 http://esp-shazam.local/ | grep -c -F '<meta charset="utf-8">'
```
Expected:
- The JSON has the spec's shape: `state`, `level_dbfs`, `rate_limited_until`, `now`, `history`.
- The content type matches `application/json; charset=utf-8`.
- The page check prints `1`.

With the source playing, within 45 s of boot `curl -s http://esp-shazam.local/api/state | jq -r .now.title` prints the title the serial log shows. Ask Harper to open http://esp-shazam.local/ for a look.

- [ ] **Step 6: Run the canonical check and commit**

Run: `scripts/check`. Expected: all `ok`.
```bash
git add firmware/Cargo.toml firmware/Cargo.lock firmware/src .gitignore
# plus firmware/components_esp32.lock if the build created it
git commit -m "feat(firmware): serve the now-playing page and /api/state; advertise esp-shazam.local"
```

---

### Task 11: MQTT publishing

**Files:**
- Create: `firmware/src/mqtt_out.rs`
- Modify: `firmware/src/main.rs`

**Interfaces:**
- Consumes: `recognizer::SharedStore`, `SongStore::now_json()`, `CONFIG.mqtt_url`, and `recognizer::spawn`'s `on_change` hook (Task 9).
- Produces:
  - `mqtt_out::start(url: &str, store: SharedStore) -> anyhow::Result<Publisher>`
  - `Publisher::publish_now(&self)`: never blocks. A blank URL gives a Publisher that does nothing.
  - `mqtt_out::TOPIC = "esp-shazam/now_playing"`

- [ ] **Step 1: The publisher**

`firmware/src/mqtt_out.rs`:
```rust
// ABOUTME: Publishes the current song to MQTT, retained, whenever it changes and on every (re)connect.
// ABOUTME: Recognition and the web page never wait on the broker; a dead broker only delays publishes.

use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::mqtt::client::{EspMqttClient, EventPayload, MqttClientConfiguration, QoS};

use crate::recognizer::SharedStore;

pub const TOPIC: &str = "esp-shazam/now_playing";

/// Asks the MQTT thread to publish the store's current `now`. Cheap to clone; never blocks.
#[derive(Clone)]
pub struct Publisher(Option<mpsc::Sender<()>>);

impl Publisher {
    pub fn publish_now(&self) {
        if let Some(tx) = &self.0 {
            let _ = tx.send(()); // only fails if the MQTT thread is gone, which already logged why
        }
    }
}

/// An empty `url` disables MQTT.
pub fn start(url: &str, store: SharedStore) -> Result<Publisher> {
    if url.is_empty() {
        log::info!("mqtt: disabled (mqtt_url is empty)");
        return Ok(Publisher(None));
    }
    let (mut client, mut connection) = EspMqttClient::new(
        url,
        &MqttClientConfiguration {
            client_id: Some("esp-shazam"),
            keep_alive_interval: Some(Duration::from_secs(30)),
            reconnect_timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
    )?;
    let (tx, rx) = mpsc::channel::<()>();

    // esp-mqtt stalls unless its events are pumped. Every connect asks for a publish,
    // so the retained value is right even after a broker restart.
    let on_connect = tx.clone();
    std::thread::Builder::new().stack_size(6 * 1024).spawn(move || {
        while let Ok(event) = connection.next() {
            match event.payload() {
                EventPayload::Connected(_) => {
                    log::info!("mqtt: connected");
                    let _ = on_connect.send(());
                }
                EventPayload::Disconnected => log::warn!("mqtt: disconnected"),
                EventPayload::Error(e) => log::warn!("mqtt: {e:?}"),
                _ => {}
            }
        }
        log::warn!("mqtt: event loop ended");
    })?;

    std::thread::Builder::new().stack_size(6 * 1024).spawn(move || {
        while rx.recv().is_ok() {
            let payload = store.lock().expect("store lock").now_json();
            // enqueue keeps a QoS 1 message while the broker is away; esp-mqtt sends it on reconnect.
            match client.enqueue(TOPIC, QoS::AtLeastOnce, true, payload.as_bytes()) {
                Ok(_) => log::info!("mqtt: queued {TOPIC} = {payload}"),
                Err(e) => log::warn!("mqtt: enqueue failed: {e}"),
            }
        }
    })?;
    Ok(Publisher(Some(tx)))
}
```

- [ ] **Step 2: Wire it up**

In `firmware/src/main.rs`, add `mod mqtt_out;`. Replace the logging `on_change` closure with the publisher (keep the web server and mDNS lines from Task 10):
```rust
    let publisher = mqtt_out::start(CONFIG.mqtt_url, store.clone())?;
    recognizer::spawn(ring, store, config::policy_config(), move || publisher.publish_now())?;
```
`store.clone()` for `web::start` must happen before `store` moves into `recognizer::spawn`.

- [ ] **Step 3: Flash and check the retained topic**

```bash
scripts/flash
sleep 25
mosquitto_sub -h 192.168.200.8 -t esp-shazam/now_playing -v -C 1 -W 15   # expect: null, or the song already playing
```
With the source playing, within 45 s of boot the same `mosquitto_sub` prints the song's JSON. Its `key` must match `curl -4 -s http://esp-shazam.local/api/state | jq -r .now.key`.

- [ ] **Step 4: Broker restart under a running board (Review Focus 4)**

A local broker you control stands in for Harper's broker. Only the `mqtt_url` line of `cfg.toml` changes, and the line gets restored afterwards. Never print the file.
```bash
mac_ip=$(ipconfig getifaddr en0)
printf 'listener 1884 0.0.0.0\nallow_anonymous true\n' > target/mosquitto-test.conf
/opt/homebrew/sbin/mosquitto -c target/mosquitto-test.conf & broker=$!
sed -i '' "s#^mqtt_url = .*#mqtt_url = \"mqtt://$mac_ip:1884\"#" firmware/cfg.toml
touch firmware/src/config.rs && scripts/flash
sleep 25
mosquitto_sub -h "$mac_ip" -p 1884 -t esp-shazam/now_playing -C 1 -W 15   # expect: null
kill "$broker"; sleep 20
curl -4 -s --max-time 5 http://esp-shazam.local/api/state | jq -r .state      # the web keeps answering
/opt/homebrew/sbin/mosquitto -c target/mosquitto-test.conf & broker=$!      # fresh broker: no retained messages
sleep 20
mosquitto_sub -h "$mac_ip" -p 1884 -t esp-shazam/now_playing -C 1 -W 15   # expect: null again, republished on connect
kill "$broker"
sed -i '' 's#^mqtt_url = .*#mqtt_url = "mqtt://192.168.200.8:1883"#' firmware/cfg.toml
touch firmware/src/config.rs && scripts/flash
```
If macOS asks whether mosquitto may accept incoming connections, Harper has to click Allow.

Pass criteria:
- The board's serial log shows `mqtt: disconnected`, then `mqtt: connected`. Check it with `scripts/serial-log` in a second run if needed.
- The second `mosquitto_sub` receives the payload from a broker that started empty.

- [ ] **Step 5: Run the canonical check and commit**

Run: `scripts/check`. Expected: all `ok`.
```bash
git add firmware/src
git commit -m "feat(firmware): publish now playing to MQTT, retained, on change and on reconnect"
```
---

### Task 11b: PSRAM and flash at 80 MHz (SDD Ruling R20)

Task 11's RAM fix moved the fingerprint's FFT buffers into 40 MHz PSRAM. That fixed the 860-byte internal-RAM low, but a fingerprint now takes about 16.5 s, up from 10.5 s. The re-check cycle grows to about 48 s, so the worst-case song change (about 65 s) and clear (about 78 s) overshoot success criteria 3 and 4.

Ruling R11 parked PSRAM and flash at 80 MHz until timings missed, and they now miss.
- It isn't a security change.
- A failed boot is recoverable over the ROM bootloader: reflash at 40 MHz.

**Files:**
- Modify: `firmware/sdkconfig.defaults`, `scripts/flash`, the spec's Hardware and Risks sections, `gotchas.md`, this plan's `## Now`

**Interfaces:**
- Consumes: the Task 11 firmware.
- Produces: the same firmware, with flash and PSRAM at 80 MHz, if the board proves stable.

- [ ] **Step 1: Switch both clocks to 80 MHz**

`firmware/sdkconfig.defaults` gains these lines. Both keys exist in ESP-IDF v5.5.5: `spi_flash/esp32/Kconfig.flash_freq`, and `esp_psram/esp32/Kconfig.spiram`, where `SPIRAM_SPEED_80M` depends on `ESPTOOLPY_FLASHFREQ_80M`.
```
# Flash and PSRAM at 80 MHz: the fingerprint's FFT buffers live in PSRAM (Task 11), so its speed is PSRAM-bound.
CONFIG_ESPTOOLPY_FLASHFREQ_80M=y
CONFIG_SPIRAM_SPEED_80M=y
```
In `scripts/flash`, add `--flash-freq 80mhz` to the espflash arguments, so the image header matches the bootloader.

- [ ] **Step 2: Prove it on the board (about 8 Shazam requests)**

1. Flash.
2. Confirm boot: the serial log shows the 2nd stage bootloader and `SPI Speed : 80MHz`. Then confirm the app reports PSRAM, and that nothing panics or resets during a 5-minute run with the source playing.
3. Use the same throwaway diagnostic Task 11 used (kept uncommitted). Over at least 5 asks with a 3 s `curl -4` `/api/state` poller running, record:
   - fingerprint ms
   - min free internal RAM
   - the largest internal block before TLS
   - TLS failures
4. Acceptance: no boot failure, panic or reset; 0 TLS failures; min free internal at least 10 KB; fingerprint meaningfully faster than 16.5 s.

If acceptance fails, revert both lines and the flag, reflash at 40 MHz, confirm the board runs, and report the numbers. The controller then rules on tuning instead (for example `clear_after_quiet_secs` 55, `recheck_interval_secs` 24).

- [ ] **Step 3: Record and commit**

1. Update the spec's Hardware (clock rates) and Risks (timings) sections, `gotchas.md`, and the plan's `## Now`.
2. Park the board in its bootloader.
3. Run `scripts/check`: all `ok`.
4. Commit by name:
   ```bash
   git add firmware/sdkconfig.defaults scripts/flash gotchas.md docs/superpowers
   git commit -m "perf(firmware): run flash and PSRAM at 80 MHz"
   ```

---

### Task 13: `shazam_enabled` flag (SDD Ruling R21)

Spec: `docs/superpowers/specs/2026-10-03-line-in-stream-idea.md`, "Decisions". Lean task: acceptance criteria, not transcribed code.

**Files:** `firmware/src/config.rs`, `firmware/cfg.toml.example`, `firmware/src/main.rs`, `core/src/store.rs` (+ tests), docs.

**Build:**
- `shazam_enabled: bool` in `Config`, `#[default(true)]`.
- When it's false, main doesn't spawn the recognizer, and logs `shazam: disabled by cfg` once.
- SNTP may stay; it's harmless.
- `SongStore` gains a `shazam_enabled` value (constructor or setter) that `state_json()` emits as a top-level `"shazam_enabled": bool`. TDD in core, with tests for both values; existing JSON tests are updated for the new key.

**Accept:**
- `scripts/check` is all `ok`.
- On the board, with the source playing:
  - flag on: matches as before;
  - flag off: no `shazam:` lines over 90 s, `/api/state` shows `"shazam_enabled": false` and `"now": null`, and the web page and MQTT still answer.
- Park the board when done.

---

### Task 14a: Core pieces for the stream: decimator, stream ring, live level (SDD Rulings R21, R24)

Pure Rust in `esp-shazam-core`, TDD on the Mac. Lean task: acceptance criteria, not transcribed code.

**Build:**
- **`audio::Decimator` (or `decimate` module):** 48 kHz interleaved stereo with 24-bit samples in `i32` → 16 kHz mono `i16`.
  - Average L and R, apply an anti-alias FIR low-pass (cutoff ≤ 7.2 kHz, linear phase, fixed-point or f32), and decimate by 3.
  - It keeps filter state across calls, so chunk boundaries don't click.
  - Tests:
    - a 1 kHz tone keeps its level within 0.5 dB and reads 1000 Hz ±5 on `zero_crossing_hz`;
    - a 7.5 kHz tone is attenuated at least 40 dB;
    - feeding a signal in odd-sized chunks gives output identical to feeding it in one call;
    - full-scale input doesn't overflow.
- **`stream_ring::StreamRing`:** a fixed-capacity ring of interleaved stereo `i32` frames with one writer and several readers, each with its own cursor.
  - `read` returns available frames, or `Overrun` when the writer has lapped that reader.
  - Readers that join start at "now", not at the oldest frame.
  - Tests: wraparound, two readers at different speeds, overrun detection, the join-at-now rule.
- **`SongStore` level split (R24):**
  - `set_level(level_dbfs)` and `level_dbfs()` are added.
  - `set_status(state, rate_limited_until)` no longer takes a level.
  - The level stays rounded to 0.1 dB in `state_json`.
  - Existing tests are updated, not weakened, and new tests cover the split.

**Accept:**
- All new and updated tests pass; `scripts/check` is all `ok` (the firmware still builds).
- Update the firmware's `set_status` call sites minimally so it compiles: the recognizer calls `set_level` itself until Task 14b moves the meter.

---

### Task 14b: Capture at 48 kHz / 24-bit stereo; audio thread meters loudness; honest page with Shazam off (SDD Rulings R21, R24)

Lean task: acceptance criteria, not transcribed code. Research the pinned crates and ESP-IDF v5.5.5 sources, and mark anything unverified.

**Build:**
- **I2S RX at 48 kHz, 24-bit data, stereo.** Choose the slot width and MCLK multiple that ESP-IDF v5.5.5 and esp-idf-hal 0.47 support on the classic ESP32: 24-bit may need 384×fs MCLK or 32-bit slots; check the driver docs and source.
  - Set the ES8388 ADC to match: word length `0x0C`, MCLK/LRCK ratio `0x0D`, with readbacks updated.
  - Fallback per R21: 48 kHz 16-bit, recorded with the reason.
- **The audio thread:**
  - pushes raw frames into a `StreamRing` (about 2 s, in PSRAM);
  - runs the `Decimator` into the existing 12 s `SampleRing`, so the recognizer is unchanged;
  - every 250 ms, computes `loudness_dbfs` over the last 1 s of the 16 kHz feed and calls `SongStore::set_level`.
- **The recognizer** reads the level from the store for `policy.tick`, instead of computing its own (one source of truth). The level stays live during asks, which retires R16's freeze. State still holds during an ask.
- **The page (`index.html`):** when `shazam_enabled` is false, show "Shazam is off · <level> dBFS" in place of the state text, and hide "Nothing playing.". Everything else is unchanged.
- **Docs:** reword the four places that recorded the misleading flag-off page as accepted: the idea doc's Decisions, gotchas, spec Outputs, and plan Open. Note R16 is retired for level.

**Accept:**
- **Flag on:** Shazam still matches the source. Min free internal RAM stays at least 10 KB during asks, and the fingerprint time is recorded.
- **Flag off:** `/api/state` shows a live `level_dbfs` that moves with the music (several reads a few seconds apart), and the page code shows "Shazam is off".
- **Capture:** the audio thread keeps up. No I2S read errors in a 5-minute run, and level logs look sane.
- `scripts/check` is all `ok`. Park the board when done.

---

### Task 15: Stream server on port 8000 (SDD Ruling R21)

Lean task: acceptance criteria, not transcribed code.

**Build:**
- A `std::net::TcpListener` on port 8000 in its own thread (core 0, below audio priority).
- On `GET /stream.wav` it answers `200` with `Content-Type: audio/wav` and no Content-Length, then a WAV header: RIFF/data sizes set to the maximum (`0xFFFFFFFF`), PCM, 48000 Hz, 2 channels, 24-bit packed (or 16-bit per Task 14's outcome).
- Then it streams frames from the stream ring as they arrive.
- At most 2 concurrent listeners; a third gets `503`.
- A listener whose reader overruns the ring gets disconnected; capture never blocks on a socket.
- Any other path gets `404`.
- Also advertise `_http._tcp` on port 8000 with the instance name `esp-shazam stream`, if the mdns API allows a second service; otherwise skip and note it.

**Accept (from the Mac, with the source playing):**
- `ffprobe -v error -show_streams http://esp-shazam.local:8000/stream.wav` reports 48000 Hz, 2 channels and the chosen bit depth (resolve with `-4` or the IP if mDNS is slow).
- `ffmpeg -t 60 -i http://…:8000/stream.wav target/stream-60s.wav` captures 60 s whose duration is 60.0 s ± 0.1, with no gaps: a sample count of 60 × 48000 ± 4800.
  - Then `ffmpeg -i target/stream-60s.wav -ac 1 -ar 16000 -sample_fmt s16 target/stream-clip.wav`, and `shazam-file` on its middle 12 s matches the same song `/api/state` shows.
- Two listeners at once both receive audio; a third gets 503; killing a listener frees its slot.
- With Shazam on and a listener connected, the board still matches, with 0 TLS failures over 5 asks.
- Record Wi-Fi throughput headroom in the spec.
- Park the board when done.

---

### Task 15b: Carry the stream through real round trips (SDD Ruling R29)

On 2026-10-04, VLC on Harper's Mac played `/stream.wav` for about 12 s, then the board dropped it for falling 2 s behind. lwIP keeps at most `TCP_SND_BUF` (5,760 B) in flight per connection, so 288 kB/s needs round trips under 20 ms. Harper's paths run 10-80 ms, with spikes to 1 s. Lean task: acceptance criteria, not transcribed code.

**Build:**
- **Bigger send buffer.** Raise lwIP's TCP send buffer (`CONFIG_LWIP_TCP_SND_BUF_DEFAULT`) until one listener holds at the round trips Task 15 measured. Task 15's run B carried one listener at 23,040 B. Pick the size from measurements.
- **Internal RAM with two listeners.** Try `CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP`, which takes Wi-Fi and lwIP buffers from PSRAM first. Verify what it moves on the classic ESP32 under ESP-IDF v5.5.5, and what it costs in throughput. If internal RAM still doesn't hold, cap the stream at one listener and say so.
- **Ring slack.** Task 16 measured round-trip spikes of 1.5-2 s, and a listener is dropped once it falls a ring (2 s) behind or a write blocks that long. Grow the stream ring so a spike doesn't drop a listener, to about 4 s (PSRAM, with Shazam off). The write timeout already follows the ring's capacity.
- Change nothing else unless a measurement shows it's needed.

**Accept (Shazam off, R26):**
- From Harper's Mac, VLC or `scripts/stream-check` plays `/stream.flac` and `/stream.wav` for 10 minutes each, and the board never drops them. Record ping round trips alongside, so the run shows the conditions it held through. If the path stayed good the whole time, say so.
- At the listener cap (two, or one if it comes to that), listeners hold for 5 minutes.
- With the cap reached plus a 10 s blast:
  - the lowest free internal RAM (`INTERNAL|8BIT`) stays at 20 KB or more;
  - no I2S buffer is lost;
  - the audio thread's lag stays inside its 45 ms queue.
- `/api/state` and the page keep answering.
- `scripts/check` is all `ok`, and the final image ends running, not parked.

---

### Task 16: FLAC stream at `/stream.flac` (SDD Ruling R22)

Harper: "can we do flac?" This is lossless compression of the same 48 kHz stereo frames, at roughly 60% of the WAV bitrate. Lean task: acceptance criteria, not transcribed code.

**Build:**
- **An encoder in `core`,** host-tested:
  - First try `flacenc = { version = "0.5.1", default-features = false }`. It must build for `xtensa-esp32-espidf` and must be able to encode one fixed-size block at a time (4096 frames) without whole-stream buffering.
  - If either fails, or it allocates per frame beyond a few KB, write a minimal encoder per RFC 9639 instead:
    - fixed predictors of order 0–4, choosing the best per subframe
    - Rice-coded residual partitions
    - frame headers with CRC-8 and footers with CRC-16
    - STREAMINFO with total samples = 0 (unknown)
  - Record which path was taken and why.
- **Host tests (TDD):**
  - Encode 48 kHz stereo PCM at the stream's bit depth (a chord-and-chirp signal plus silence plus a full-scale square wave), decode with `claxon` (a dev-dependency), and assert sample-exact equality.
  - Also run `ffmpeg -i out.flac -f s32le -` on a test file and compare the samples.
- **Stream:** `GET /stream.flac` on the port-8000 server.
  - `Content-Type: audio/flac`.
  - It sends the `fLaC` marker and STREAMINFO, then one encoded frame per 4096 frames from the stream ring.
  - The same listener cap and overrun-drop rules as WAV apply. WAV stays as the debug and fallback endpoint.

**Accept:**
- The host round-trip tests pass bit-exact.
- On the board:
  - `ffprobe` reads the stream as FLAC, 48000 Hz, 2 channels.
  - A 60 s `ffmpeg` capture is gapless (sample count 60 × 48000 ± 4800), decodes cleanly, and Shazams to the same song as `/api/state`.
  - Measured stream bitrate is clearly below WAV's.
  - Encoder CPU is measured: ms per 4096-frame block, which must stay well under the block's 85 ms of audio.
  - With Shazam on and a FLAC listener, 0 TLS failures over 5 asks, and min free internal RAM ≥10 KB.
- Note in the README which players were tried (ffmpeg/ffplay at least; VLC if available).

---

### Task 17: The auditor's kata issues (after Task 15b)

On 2026-10-04 Harper had an auditor review the code. It filed 17 kata issues labelled `flac-audit` (`kata list`, `kata show <id>`); each issue holds its own acceptance criteria. Task 15b takes 6yb1, 7j8s and kjfx, plus zqnh's stale comment. The diag-only issues (hb9p, sznz, b69f, 3db1, vya4) concern code that never ships. This task takes the rest:
- 53gv: count capture overflows in the production build, and bound the audio thread's lock waits.
- 5es1: release listeners when capture stops producing audio.
- nr6z: fallible PSRAM allocations for listener buffers, and a clean refusal when they fail.
- zqnh: a bounded reclamation policy for closing connections.
- 3py4: the supported envelope at worst-case FLAC bitrate.
- 23mz: if Task 15b doesn't settle it.
- r759: constant subframes.
- Task 16's queued minors (ledger).
- 3pz5: supervise capture progress. Harper's policy (2026-10-04): restart capture by itself, and reboot if that doesn't fix it. Avoid false trips during network stalls or long fingerprints.
- f8rn: encode FLAC once and share the encoded frames between listeners, so CPU doesn't grow with listeners. Each listener keeps its own cursor, and slow listeners are still dropped. The cap stays 2 by default, and raising it is a one-constant change. Record internal RAM, PSRAM and Wi-Fi cost per listener, so a higher cap can be set from data. Harper (2026-10-04): architect for 3-4, realistically 2 or fewer.

Close each kata issue with its commit and test evidence (`kata close <id> --done --message … --commit … --test …`).

---

### Task 12: Device end-to-end test, Wi-Fi drop check, README

LINE IN is wired to Harper's audio source, which plays continuously and which nothing on the Mac controls. The device test therefore works with whatever plays (ledger Ruling R13).

**Files:**
- Create: `scripts/e2e-device`
- Modify: `README.md`, `gotchas.md`, the spec's Hardware section (anything bring-up confirmed)

**Interfaces:**
- Consumes: the running board (Tasks 7–11), `GET /api/state`, MQTT topic `esp-shazam/now_playing`, and `scripts/serial-log`, which resets the board.
- Produces: `scripts/e2e-device [--expect "<title>"] [--song-change] [--silence-check]`. It checks:
  - success criterion 1: identification within 60 s of a reset (boot, Wi-Fi and clock included),
  - criterion 2: MQTT matches,
  - optionally criterion 3, a song change,
  - optionally criterion 4, clearing after silence (needs a human).

- [ ] **Step 1: Write the script**

`scripts/e2e-device` (then `chmod +x scripts/e2e-device`):
```bash
#!/usr/bin/env bash
# ABOUTME: Device end-to-end test against whatever plays into LINE IN: reset, identify, MQTT, then optional checks.
# ABOUTME: Usage: scripts/e2e-device [--expect "<title>"] [--song-change] [--silence-check]; the source must be playing.
set -euo pipefail
cd "$(dirname "$0")/.."

board="${BOARD_URL:-http://esp-shazam.local}"
broker="${MQTT_HOST:-192.168.200.8}"
topic="esp-shazam/now_playing"
expect="" song_change=0 silence=0
while [ $# -gt 0 ]; do
  case "$1" in
    --expect) expect="${2:?--expect needs a title}"; shift 2 ;;
    --song-change) song_change=1; shift ;;
    --silence-check) silence=1; shift ;;
    *) echo "usage: scripts/e2e-device [--expect \"<title>\"] [--song-change] [--silence-check]" >&2; exit 2 ;;
  esac
done

state() { curl -4 -s --max-time 3 "$board/api/state" || echo '{}'; }
now_field() { state | jq -r ".now.$1 // empty"; }
mqtt_now() { mosquitto_sub -h "$broker" -t "$topic" -C 1 -W 10 2>/dev/null || true; }
# Waits up to $1 seconds for the shell condition in $2 to hold.
wait_for() {
  local limit="$1" t0
  t0=$(date +%s)
  while [ $(( $(date +%s) - t0 )) -le "$limit" ]; do
    if eval "$2"; then return 0; fi
    sleep 2
  done
  return 1
}

echo "== reset the board (the source must already be playing)"
scripts/serial-log 1 >/dev/null
t0=$(date +%s)
wait_for 60 '[ -n "$(now_field key)" ]' || { echo "FAIL: nothing identified within 60 s of reset"; state; exit 1; }
key=$(now_field key)
echo "ok    identified \"$(now_field title)\" by $(now_field artist) after $(( $(date +%s) - t0 )) s (boot, Wi-Fi and clock included)"
if [ -n "$expect" ]; then
  grep -i -F -q -- "$expect" <<<"$(now_field title)" || { echo "FAIL: expected a title containing \"$expect\""; exit 1; }
fi
mqtt_now | jq -e --arg k "$key" '.key == $k' >/dev/null || { echo "FAIL: MQTT now_playing doesn't match /api/state"; exit 1; }
echo "ok    MQTT retained the same song"

if [ "$song_change" = 1 ]; then
  echo "== song change (waits up to 10 min for the next track)"
  wait_for 600 '[ -n "$(now_field key)" ] && [ "$(now_field key)" != "$key" ]' \
    || { echo "FAIL: no song change seen in 10 min"; exit 1; }
  echo "ok    changed to \"$(now_field title)\""
  state | jq -e --arg k "$key" '.history[1].key == $k' >/dev/null || { echo "FAIL: history[1] is not the previous song"; exit 1; }
  echo "ok    history lists the new song, then the previous one"
fi

if [ "$silence" = 1 ]; then
  echo "== silence: stop the source now, then press Enter"
  read -r _
  t0=$(date +%s)
  wait_for 75 '[ "$(state | jq -r .now)" = "null" ]' \
    || { echo "FAIL: now playing still set 75 s after the source stopped"; exit 1; }
  echo "ok    cleared after $(( $(date +%s) - t0 )) s"
  [ "$(mqtt_now)" = "null" ] || { echo "FAIL: MQTT still holds a song"; exit 1; }
  echo "ok    MQTT retained null"
fi

echo "PASS e2e-device"
```

- [ ] **Step 2: Run it against the playing source**

Run `scripts/e2e-device --song-change`. It sends a few Shazam requests per song, and the song-change wait can take up to 10 minutes. Close any serial monitor first, because the script resets the board over the port.

Expected:
- `ok    identified "…" … after N s` with N ≤ 60
- `ok    MQTT retained the same song`
- `ok    changed to "…"` and `ok    history lists …`
- `PASS e2e-device`

If the source plays one long track and no change comes within 10 minutes, report that rather than looping.

- [ ] **Step 3: Checks that need Harper (list them in the report; don't fake them)**

1. **Silence:** Harper runs `scripts/e2e-device --silence-check` and stops the source when prompted. Expected: cleared within 75 s, MQTT `null`.
2. **Wi-Fi drop:**
   - Keep `espflash monitor --port /dev/cu.usbserial-0001` running.
   - Harper cuts the board's Wi-Fi for about 30 s: block MAC 70:4b:ca:87:18:3c in the router, or toggle 2.4 GHz.
   - Expected: `wifi: disconnected`, then `wifi: reconnected` within about 10 s of service returning, and no reset (`rst:` line).
   - Then `scripts/e2e-device` passes again with no reflash.

- [ ] **Step 4: Finish the README and record what bring-up learned**

`README.md`: replace with the following. Fill in what Tasks 7–9 found where it says so.
````markdown
# esp-shazam

An always-listening Shazam box. Rust firmware for the Ai-Thinker ESP32-A1S Audio Kit (ES8388 v1) listens to LINE IN, fingerprints the audio on the board (SongRec's algorithm), asks Shazam, and publishes the song.

- Web page: http://esp-shazam.local/
- JSON: `GET http://esp-shazam.local/api/state`
- MQTT: `esp-shazam/now_playing` (retained; the song as JSON, or `null`)

## Build and flash

```bash
mise trust && mise install && mise run esp-toolchain  # toolchain, pinned in mise.toml
cp firmware/cfg.toml.example firmware/cfg.toml        # then fill in Wi-Fi and MQTT
scripts/check                                         # fmt, clippy, Mac tests, firmware build
scripts/flash --monitor                               # flash and open the serial monitor
```

Line-level sources only: set a turntable's output to LINE (fixed), not VARIABLE.

## Tests

- `scripts/check`: canonical check
- `scripts/e2e-host <song> "<title>"`: real Shazam from the Mac
- `scripts/e2e-device [--expect "<title>"] [--song-change] [--silence-check]`: the board end to end, against whatever plays into LINE IN

Design and hardware notes: `docs/superpowers/specs/2026-10-03-esp-shazam-design.md`.

Licensed GPL-3.0-or-later (the fingerprint code comes from SongRec).
````

Update the spec's Hardware section so every "(verify)" item that bring-up settled shows its measured value. Add new hardware surprises to `gotchas.md`.

- [ ] **Step 5: Run the canonical check and commit**

Run: `scripts/check`. Expected: all `ok`.
```bash
git add scripts/e2e-device README.md gotchas.md docs/superpowers/specs/2026-10-03-esp-shazam-design.md
git commit -m "test: add the device end-to-end test and finish the README"
```
