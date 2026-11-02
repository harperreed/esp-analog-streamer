# SIFT Codebase Audit

**Status:** `COMPLETE`
**Repository:** esp-analog-streamer (esp-shazam), `/home/harper/workspace/personal/esp-analog-streamer`
**Revision:** `49ad53e` on `wip/kata-capture-protection`
**Scope:** the firmware as requested ("a SIFT exercise against this firmware"): the `firmware/` crate, the `core` and `mp3` crates it links, the web assets and the host tests that pin their behaviour. Host harness scripts, the Mac CLI, fuzz targets and the diagnostic example binaries are explicit scope exclusions (section 5).
**Audit mode:** read-only; tests not run; no builds. Thirteen bounded reviewers, each confined to one subsystem, followed by a coordinator re-read of every cited line.
**Date:** 2026-10-22

## 1. Executive summary

The firmware's organizing models are mostly sound and deliberately so: the capture publisher, the raw ring, the network lease, the stream runtime's immutable epochs and the close reaper all have one authority each, with proptest, Loom and replay tests behind them. The audit found no subsystem whose core model needs replacing. What it found is drift at the seams, where a value is encoded by hand in one place and decoded by hand in others, or where a block was copied when a second instance was needed and the copies then diverged.

Five themes, in the order they matter:

1. **Two values standing in for one fact.** OTA's `VALIDATED` static beside otadata; the stream runtime's `block_frames == 1152` standing in for the active codec; the health token's bit layout re-derived in three files; the listen policy's `(Outcome, Option<Song>)` pair; the Sonos worker's six session `Option`s. Each admits a contradictory state that the code holds off only by discipline. The OTA one is the only place where that discipline already fails: the health thread records "validated" on its own failure path and then exits, and an upload overlapping the 60 s mark makes that failure reachable (SIFT-S05-01).
2. **Copied blocks that drifted.** The web server's three bounded-body POST paths (two body-size limits, two wordings for the same timeout); the listener and encoder telemetry queues (identical except for a capacity literal) and their two exporter loss-receipt blocks; the stream request line parsed up to five times per admission; the 24-to-16-bit quantizer written twice.
3. **Authority split across layers in the Sonos stack.** The "Sonos is off" gate lives only in the HTTP handler; the buffer-preset rule is checked three times on a boot-time constant; "autoplay implies enabled" is defended in three subsystems (closed today, so recorded as a note rather than a finding).
4. **A transport the project retired but kept.** The WebSocket session-command model in `core::sonos` and its TLS fixture in the firmware worker (about 850 lines plus roughly 600 lines of tests) have no production caller; the decision to drop it is recorded in `gotchas.md`, the code is not.
5. **Firmware-only modules with no host tests.** `ota.rs`, the Sonos `device` module and the web handlers can only be checked on a board. This is not a finding by itself, but it raises the migration cost of SIFT-S05-01/02, SIFT-S11-01 and SIFT-S11-02, and it is why those are sequenced after the small, host-testable items.

Numbers: 20 ledger rows: 13 subsystems reviewed, 12 with accepted recommendations, 1 explicit skip (network lifecycle), 7 scope exclusions; 16 accepted findings after the audit-of-audit (18 before it: 2 demoted as speculative), 1 merged, 4 demoted, 22 rejected candidates recorded.

**Highest-value first slice:** SIFT-S05-01, derive `validated` from the running slot's state and keep the health thread alive after a failed mark. About 15 lines in one file, high confidence, closes a reachable rollback hazard.

**Material uncertainty:** none on coverage. On findings, the Sonos worker restructure (SIFT-S11-01) and the stream request parse (SIFT-S07-01) are the two with real migration cost, and both say so in their schema.

## 2. Coverage contract

| ID | Subsystem | Exact ownership boundary | Key files/interfaces/tests | Status |
|---|---|---|---|---|
| S01 | Audio capture path | I2S driver, codec bring-up, the capture thread, the raw publisher and raw ring, capture health, PSRAM buffer | `firmware/src/{audio_in,i2s_capture,codec,capture_timing,psram}.rs`; `core/src/{capture_buffer,capture_health,audio,stream_ring}.rs`; the `capture:` UART line; `core/tests/{capture_publication,capture_publication_events,capture_settling_stats,capture_timing,audio_diagnostics}.rs` | `recommend: SIFT-S01-01` |
| S02 | Recognition | decimation, listen policy, Shazam request/response, the recognizer thread, admission, the FIR feed | `core/src/{decimate,policy,shazam}.rs`; `firmware/src/{recognizer,recognition_admission,recognition_feed,shazam_http}.rs`; `core/tests/{recognition_admission,recognition_feed,golden,shazam_fixtures}.rs`, `test/fixtures/shazam/*.json` (the Shazam response contract) | `recommend: SIFT-S02-01` |
| S03 | Reported state and web UI | the store, port 80 API routing and body handling, the three pages, app.js | `core/src/{store,time}.rs`; `firmware/src/web.rs`, `*.html`, `app.js`, `style.css`; `test/sonos-web.test.cjs`, `core/tests/sonos_web_http.rs` | `recommend: SIFT-S03-01` |
| S04 | Build-time config and runtime settings | cfg.toml schema, the NVS-persisted input gain (namespace `input`, key `gain_db`) | `firmware/src/{config,input_gain}.rs`, `cfg.toml.example`; `core/src/input_gain.rs` | `recommend: SIFT-S04-01` |
| S05 | Over-the-air updates | image inspection, upload into the other slot, health mark, rollback config | `core/src/ota_image.rs`; `firmware/src/ota.rs`, `partitions.csv`, the rollback lines of `sdkconfig.defaults` (the rest of that file is S13), the `ota-rollback-probe` feature; `scripts/test_ota_push.py` as contract evidence | `recommend: SIFT-S05-01, SIFT-S05-02` |
| S06 | Network lifecycle | Wi-Fi station, reconnect supervisor, SNTP, mDNS, the lease/generation model, radio policy readback | `firmware/src/{net,net_policy,network_observation}.rs`; `core/src/network_lifecycle.rs`; `core/tests/{net_policy,network_observation,stream_network_query}.rs` | `skip` |
| S07 | Stream server and listener lifecycle | port 8000 admission, slots, writes, departure, checked close, the reaper | `firmware/src/{stream,stream_admission,stream_workers,socket_close,close_reaper}.rs`; `core/src/{stream_http,stream_wait,stream_write,wav}.rs`; the `admission-resources` feature and its UART line; `core/tests/{close_reaper,listener_workers,listener_skip,listener_runtime_loom,listener_policy,poll_departure,stream_admission,stream_properties,stream_replay,stream_write_replay,replay_wait,wait_characterization}.rs`; `fuzz/fuzz_targets/{http_head,stream_ops}.rs` | `recommend: SIFT-S07-01, SIFT-S07-02` |
| S08 | Encoder pipeline and runtime | the encoder worker, codec/buffer epochs, the encoded ring, encoder health | `firmware/src/{stream_pipeline,stream_runtime,stream_health,stream_codec,stream_limits}.rs`; `core/src/{flac_ring,stream_buffer,encoder_health}.rs`; `core/tests/{runtime_format,runtime_switch,stream_codec,stream_codec_profile,stream_limits,stream_network_query,buffer_presets,replay_pipeline,flac_ring_ffmpeg,flac_join_numbering}.rs` | `recommend: SIFT-S08-01, SIFT-S08-02` |
| S09 | Audio codecs | the FLAC encoder, the MP3 wrapper, the WAV header contract | `core/src/flac.rs`; `mp3/src/{lib.rs,bridge.c}`, `mp3/build.rs`, `mp3/Cargo.toml`; `core/tests/{flac_cost,flac_ffmpeg,flac_stereo}.rs`, `mp3/tests/*`, `fuzz/fuzz_targets/flac_frames.rs` | `recommend: SIFT-S09-01` |
| S10 | Sonos protocol and command model (core) | discovery/control messages, UPnP HTTP client, the one-item queue, web command parsing, `SonosConfig` | `core/src/{sonos,sonos_queue,sonos_upnp,sonos_web}.rs` (the `SonosConfig` schema of the NVS blob); `core/tests/{sonos_upnp_http,sonos_web_http}.rs`, `core/tests/fixtures/sonos` | `recommend: SIFT-S10-01` |
| S11 | Sonos controller worker (firmware) | the worker thread: discovery, HTTP control, playback intent, autoplay, the NVS blob (namespace `sonos`, key `config`) and the `/api/sonos` `Snapshot` JSON, the TLS fixture and its orphaned root bundle | `firmware/src/sonos.rs`, `firmware/assets/sonos-roots.pem` (+ `.PROVENANCE.md`, referenced by nothing); `core/tests/sonos_adapter.rs`; `scripts/sonos_http_check.py` as contract evidence | `recommend: SIFT-S11-01, SIFT-S11-02` (its third candidate merged into SIFT-S10-01) |
| S12 | Telemetry, device metrics, feature-gated diagnostics | UART receipts, bounded event stores, the `/api/metrics` cache (`schema_version: 1`), the observers behind the `startup-capture-trace`, `listener-frame-state`, `metrics-state-crossover`, `encoder-probe` and `capture-fixture` features | `firmware/src/{device_metrics,device_metrics_identity,listener_telemetry,encoder_telemetry,listener_observation,capture_failure_telemetry,listener_frame_state,encoder_probe,metrics_state_crossover,startup_capture_trace,capture_fixture}.rs`, `firmware/src/capture_fixture/events.rs`; `core/src/{task_observation,media_age}.rs`; `core/tests/{device_metrics*,*_telemetry,listener_observation,listener_frame_state,encoder_probe,metrics_state_crossover,startup_capture_trace,capture_failure_telemetry}.rs` | `recommend: SIFT-S12-01` |
| S13 | Boot, threads and build | module wiring in both crates, task sizes/priorities/cores, the cargo feature tables, the ESP-IDF build glue | `firmware/src/{lib,main,threads}.rs`, `firmware/build.rs`, `firmware/build_support/mp3_iram.rs`, `firmware/Cargo.toml`, `firmware/.cargo/config.toml`, `firmware/rust-toolchain.toml`, `firmware/components_esp32.lock`, `firmware/sdkconfig.defaults` (except the rollback lines), root `Cargo.toml`, `core/src/lib.rs`, `core/Cargo.toml` (features `capture-fixture`, `capture-loom`, `listener-loom`, `encoder-probe`), `mise.toml`; `core/tests/{thread_spawn,mp3_iram_linker}.rs` | `recommend: SIFT-S13-01` |
| X1 | Vendored fingerprinting | `core/src/fingerprinting/*` (songrec-lib copy) | golden tests in `core/tests/golden.rs` | `skip` (vendored) |
| X2 | Vendored Shine MP3 | `mp3/vendor/shine` | `mp3/tests` | `skip` (vendored) |
| X3 | Host harness scripts | `scripts/*` | `scripts/test_*.py` | `skip` (out of requested scope) |
| X4 | Mac CLI | `tools/src/main.rs` | none | `skip` (out of requested scope; consumer of S02) |
| X5 | Fuzz targets | `fuzz/` | corpora under `fuzz/corpus` | `skip` (harness; read as S07/S09 evidence) |
| X6 | Diagnostic example binaries and their build inputs | `firmware/examples/*`, `firmware/bindings/{diagnostics,tcp-retention}.h`, `firmware/sdkconfig.diagnostics.defaults` (used only by the isolated examples per `firmware/Cargo.toml:79-87`) | none | `skip` (not in the production image) |
| T1 | Host test support | `core/src/signal.rs` (deterministic test audio), `core/tests/support/{replay,listener_runtime_models,alloc_count}.rs`, `test/fixtures/` | consumed by the S02, S07, S08 and S09 tests | `skip` (test support; read as evidence) |

A test file named in two rows belongs to the row that lists it explicitly; the explicit lists above replace the globs the reviewers worked from.

## 3. Prioritized recommendations

| Priority | Finding | Subsystem | Impact | Confidence | Effort | Blast radius | Prerequisites |
|---:|---|---|---|---|---|---|---|
| 1 | SIFT-S05-01 otadata is the one authority for "validated"; the health thread retries | S05 OTA | high | high | small | local | none |
| 2 | SIFT-S05-02 one `Mutex<EspOta>` owner replaces the `UPLOADING` flag | S05 OTA | medium | medium | small | local | none (same file as S05-01; land together) |
| 3 | SIFT-S04-01 typed outcome for `input_gain::Handle::apply` | S04 settings | medium | high | small | local | with or after SIFT-S03-01 (same `match` in `web.rs:267-307`) |
| 4 | SIFT-S03-01 one bounded-body path and one `RequestError` status map | S03 web | medium | high | small | local | none |
| 5 | SIFT-S08-01 typed code for the active `OutputFormat` | S08 encoder | medium | high | small | subsystem | none |
| 6 | SIFT-S07-01 parse the stream request line once into a typed record | S07 stream | medium | high | medium | cross-subsystem (S08 signature) | after SIFT-S08-01 (both edit `Runtime::admit`) |
| 7 | SIFT-S12-01 one generic bounded queue and one loss-receipt policy | S12 telemetry | medium | high | medium | cross-subsystem (S07 file) | none |
| 8 | SIFT-S10-01 retire the WebSocket session layer in both crates | S10 Sonos core | medium | high | medium | cross-subsystem (S11, a Cargo feature, an example) | Harper's permission to delete |
| 9 | SIFT-S11-02 `Handle::submit` is the single Sonos admission authority | S11 Sonos worker | medium | medium | medium | cross-subsystem (S03 handler) | easier after SIFT-S10-01 |
| 10 | SIFT-S11-01 one `Session` enum for the Sonos worker's connection state | S11 Sonos worker | high | medium | large | subsystem | hardware check; easier after SIFT-S10-01 |
| 11 | SIFT-S08-02 `Token` newtype for the health token bits | S08 encoder | low | medium | small | subsystem | none |
| 12 | SIFT-S02-01 `Outcome::Match(Song)` replaces the sidecar tuple | S02 recognition | low | medium | small | local | none |
| 13 | SIFT-S07-02 one departure-check authority per listener turn | S07 stream | low | medium | small | subsystem | none |
| 14 | SIFT-S01-01 one capture-window snapshot feeds JSON and UART | S01 capture | low | medium | small | cross-subsystem (S12 struct) | none |
| 15 | SIFT-S09-01 one 24-to-16-bit quantizer | S09 codecs | low | medium | small | subsystem (S08 adapter file) | none |
| 16 | SIFT-S13-01 one spawn path for every production thread; the priority ladder stated in code | S13 boot | low | medium | small | cross-subsystem | after SIFT-S05-01/02, to edit `ota.rs` once |

Kata tickets (epic `yzgs`, label `sift-audit`): S05-01 `dvqk`, S05-02 `690e`, S04-01 `z818`, S03-01 `t8wq`, S08-01 `s2e1`, S07-01 `hyvz` (blocked by `s2e1`), S12-01 `hsqv`, S10-01 `4m9h`, S11-02 `62a5`, S11-01 `rmve`, S08-02 `jmwa`, S02-01 `3k3g`, S07-02 `r9r6`, S01-01 `6bce`, S09-01 `64yy`, S13-01 `dhhp`.

### SIFT-S05-01

- **Finding ID:** SIFT-S05-01
- **Authoritative subsystem:** S05 Over-the-air updates
- **Title:** Make otadata the single authority for "validated" and keep the health thread alive after a failed mark
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/ota.rs:14-15` (the `VALIDATED` static), `:76` (`validated: VALIDATED || state == Valid`), `:89-92` (set when the slot is already Valid), `:118-127` (set to `true` on both the `Ok` and the `Err` arm of `mark_running_slot_valid`, then `return`).
- **Interfaces and call sites:** `ota::status` serialised by `GET /api/ota` (`firmware/src/web.rs:153-173`); `ota::mark_valid_when_healthy(network)` from `firmware/src/lib.rs:127`; the System page prints `running_state` and appends ", validated" (`firmware/src/app.js:195`).
- **Tests and intent evidence:** no tests exist in `ota.rs`. `docs/audit-evidence/2026-10-21/ota.md` records the measured round trip (upload 7 to 12 s, mark at 60 s) and a rollback. `firmware/sdkconfig.defaults` sets `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`, so an image still pending at the next reset is rolled back.
- **Current representation:** three authorities for one fact: otadata (read through `SlotState`), the crate's process-wide `EspOta` singleton, and a module `AtomicBool` that mirrors the first.
- **Current complexity or invalid states:** `Status` can report `validated: true` with `running_state: "pending_verify"`. That state is reachable: `stream()` holds an `EspOta` for the whole upload (`ota.rs:176-212`), the crate refuses a second instance while one lives (`esp-idf-svc-0.53.0/src/ota.rs:110, 455-459`), so a health mark that fires during an upload fails, is logged, sets `VALIDATED`, and the thread exits for the rest of the boot. If that upload then fails (the 500 path, no restart), the board sits pending, the page says validated, and the next reset rolls back.
- **Why it is material:** it is the one place in the audit where a mirrored value already produces a wrong answer, and the consequence is an unintended rollback.
- **Proposed representation:** `validated` is derived from `running.state` (or dropped, since the state word already says "valid"); the health thread keeps ticking after a failed mark so the next 5 s tick retries; the static and its doc comment go.
- **Why it is simpler:** one source of truth, one fewer static, and the failure path no longer records success.
- **Implementation scope:** `firmware/src/ota.rs` lines 14-15, 76, 89-92, 118-127; optionally `app.js:195`. The JSON field can stay (derived) so no contract change.
- **Smallest credible slice:** derive the field and change the `Err` arm to `continue` instead of falling through to `store(true)` and `return`. About 15 lines.
- **Regression risks:** a mark that fails for a persistent reason now retries every 5 s on a 6 KiB thread; bound the log (warn once) or accept it, since the thread exists only while pending. USB-flashed images (state `Unknown`) are unchanged.
- **Migration concerns:** none; no persisted format changes.
- **Existing validation:** none on host; the hardware round trip in the evidence doc.
- **Additional validation required:** on a board, poll `GET /api/ota` through a push and confirm `pending_verify` then `valid` with no early `validated: true`; provoke the overlap (push at second 55 after a restart) and confirm the serial log shows the retry and the later "marked valid". Verify at implementation what the bootloader does when rolling back onto a slot that is itself still pending.
- **Impact:** high (reachable rollback). **Confidence:** high (the `Err` arm and the singleton are both in plain sight). **Implementation effort:** small. **Blast radius:** local. **Prerequisites:** none. **Priority:** 1.

### SIFT-S05-02

- **Finding ID:** SIFT-S05-02
- **Authoritative subsystem:** S05 Over-the-air updates
- **Title:** One owner for the `EspOta` handle, doubling as the busy flag
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/ota.rs:12-13` (`UPLOADING: AtomicBool`), `:68` (`status()` opens a handle per call), `:87` and `:119` (the health thread opens two more), `:149-153` (swap/store around `stream`), `:176` (`stream()` opens a fourth and holds it for the upload).
- **Interfaces and call sites:** `GET /api/ota` maps any `status()` error to 500 with the error text (`web.rs:154-160`); the System page polls it every 5 s and renders `?` on failure (`app.js:233,240`); `upload()` returns `Refusal::Busy` for a second concurrent POST.
- **Tests and intent evidence:** `esp-idf-svc-0.53.0/src/ota.rs:455-465` (`EspOta::new` returns `ESP_ERR_INVALID_STATE` while another instance lives). No firmware tests.
- **Current representation:** a module flag re-implements the exclusion the crate already enforces, with a different scope.
- **Current complexity or invalid states:** during an upload `UPLOADING` says busy and the crate says taken, so `GET /api/ota` answers 500 "OTA handle: ESP_ERR_INVALID_STATE" instead of "upload in progress", and the health mark fails (the trigger for SIFT-S05-01). Three functions each believe they can construct the singleton.
- **Why it is material:** it removes the contention that makes S05-01's bad state reachable, and gives the page a truthful answer during a push.
- **Proposed representation:** `static OTA: Mutex<Option<EspOta>>` (or `OnceLock<Mutex<EspOta>>`); `status()` and the health thread `try_lock`/`lock` briefly; `upload()` uses `try_lock`, and a failed `try_lock` is `Refusal::Busy`; `stream()` borrows `&mut EspOta` from the guard.
- **Why it is simpler:** one handle, one busy signal, no second flag to keep in step with the crate's own.
- **Implementation scope:** `firmware/src/ota.rs` only (lines 12-13, 67-71, 87, 119, 149-153, 176-177). `web.rs` unchanged unless a `uploading` field is added to `Status`.
- **Smallest credible slice:** the mutex plus `try_lock` in `upload()` and `status()`; leave the health thread's call as is until S05-01 lands.
- **Regression risks:** a `std::sync::Mutex` on ESP-IDF allocates on first use (`lib.rs:100-101` records this); the first lock would be at boot in `mark_valid_when_healthy`, which is fine. `status()` must not block inside an httpd handler for 12 s: use `try_lock` and report busy.
- **Migration concerns:** none.
- **Existing validation:** hardware round trip and rollback in `docs/audit-evidence/2026-10-21/ota.md`.
- **Additional validation required:** `GET /api/ota` during a push returns 200 (busy) not 500; a second concurrent POST still gets 409; the 60 s mark succeeds when a push overlaps it; a full round trip and rollback as in the evidence doc.
- **Impact:** medium. **Confidence:** medium (the contention is proven from crate source; no evidence row records the 500 because the page swallows it). **Implementation effort:** small. **Blast radius:** local. **Prerequisites:** none; it edits the same function as S05-01, so land the two together. **Priority:** 2.

### SIFT-S04-01

- **Finding ID:** SIFT-S04-01
- **Authoritative subsystem:** S04 Build-time config and runtime settings
- **Title:** A typed outcome for `input_gain::Handle::apply` instead of an error the web layer matches by string
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/input_gain.rs:65-86` (`apply`: `try_lock` → `anyhow!("input gain busy")` at 69; codec write 70; `LIVE_REGISTER.store` 71; `gain` updated 72; NVS persist 74-78 with `?`). `firmware/src/web.rs:270-282` (`Err(error) if error.to_string() == "input gain busy" => 503`; every other `Err` → 500 `"codec did not take the gain; previous value kept"`).
- **Interfaces and call sites:** `POST /api/input` (the settings page); `GET /api/input` and `/api/metrics` read `LIVE_REGISTER` and `gain()`.
- **Tests and intent evidence:** `core/src/input_gain.rs` unit tests cover the dB/register mapping; nothing covers `apply` (I2C and NVS). The doc comment on `apply` promises "a failed write leaves the old gain".
- **Current representation:** one `anyhow::Error` for three distinct outcomes (busy, codec refused, persisted-failed), discriminated downstream by comparing the message text.
- **Current complexity or invalid states:** rewording the message silently turns a 503 into a 500. On a persist failure the codec and the live register already hold the new gain, so the 500 body "previous value kept" is false while `GET /api/input` reports the new value: applied-in-codec, reported-live, not-persisted, reported-as-rejected.
- **Why it is material:** a user-facing contradiction and a cross-module contract held by a string literal.
- **Proposed representation:** `enum ApplyError { Busy, Codec(anyhow::Error), Persist { applied: InputGain, source: anyhow::Error } }`; the handler matches variants to 503 / 500 / an honest answer for the persisted-failed case (S03 picks the status).
- **Why it is simpler:** the type that owns the sequencing names its outcomes; the string compare disappears.
- **Implementation scope:** `firmware/src/input_gain.rs` (`apply` signature and body, ~15 lines), `firmware/src/web.rs:270-282` (~10 lines). Optional tidy: `Handle` holds the gain twice (`gain: Mutex<InputGain>` at 47 and `LIVE_REGISTER` at 15); one `AtomicU8` with `gain()` derived via `InputGain::from_register` leaves a single authority.
- **Smallest credible slice:** the enum and the three match arms; keep the current 500 body for `Codec`.
- **Regression risks:** the only behaviour change is the persist-failure response; the settings page does not branch on error text (`test/sonos-web.test.cjs:292-297`), but confirm before changing the code.
- **Migration concerns:** none.
- **Existing validation:** `core/src/input_gain.rs` tests; `test/sonos-web.test.cjs` settings cases.
- **Additional validation required:** on a board, two overlapping POSTs (expect one 503), a normal POST (200 and the value survives a reboot).
- **Impact:** medium. **Confidence:** high. **Implementation effort:** small. **Blast radius:** local. **Prerequisites:** none, but it rewrites arms of the same `match` that SIFT-S03-01 edits (`web.rs:267-307`); do them as one edit or S03-01 first. **Priority:** 3.

### SIFT-S03-01

- **Finding ID:** SIFT-S03-01
- **Authoritative subsystem:** S03 Reported state and web UI
- **Title:** One bounded-body request path and one HTTP mapping for `RequestError`
- **Verdict:** `recommend`
- **Primary evidence:** socket/header/read boilerplate copied three times: `firmware/src/web.rs:244-266` (`POST /api/input`), `:322-344` (`POST /api/sonos`), `:455-477` (`POST .../timePlayed`). `RequestError → (status, message)` mapped three times: `:292-307`, `:385-395`, `:487-495`, already drifting ("Body read timed out" vs "Body read deadline exceeded"; the timePlayed arm collapses every variant to one message). The body-size limits (64 vs 512 bytes) differ per endpoint by design and are not drift. `/api/input` encodes "body fine, content invalid" as `Err(None)` on a `Result<_, Option<RequestError>>` (`:267-269, 285`) while `read_action` folds it into `RequestError::BadRequest` (`:581-594`). The JSON response write is repeated in ten handlers (`:85,131,146,168,221,237,315,402,443,504`).
- **Interfaces and call sites:** every POST handler on port 80; `read_json_body`/`read_action` (host-tested).
- **Tests and intent evidence:** `core/tests/sonos_web_http.rs` and `web.rs:617-754` cover `read_json_body`/`read_action` only; no test covers the per-endpoint status mapping, which is why the wordings drifted unnoticed. History: each POST endpoint arrived by copying the previous one (f723af0 input gain, 5fd28f5 OTA).
- **Current representation:** three hand-maintained copies of the same framing-error semantics plus a nullable sentinel for a named error.
- **Current complexity or invalid states:** the next POST endpoint copies all of it again; the status for a given `RequestError` depends on which handler you are in.
- **Why it is material:** recurring implementation cost on a file that grows one handler per feature, and an already-visible drift in a user-facing contract.
- **Proposed representation:** `impl RequestError { fn status(&self) -> u16 }` as the single authority; an `Invalid` variant (or `BadRequest`) so `/api/input` drops `Option<RequestError>`; one device-side `bounded_json_body::<LIMIT>(request, budget) -> Result<(Vec<u8>, RequestSocket), RequestError>` owning the fd/header/`transfer_encoding`/timeout dance and returning the guard so the handler still controls the `SHUT_RD` drop point; one `fn json(request, code, body)` for the response write.
- **Why it is simpler:** each POST handler becomes read body → domain parse → domain result → status; the duplicated semantics are deleted, not wrapped.
- **Implementation scope:** `firmware/src/web.rs` only (the `device` module plus the host-visible `RequestError`/`read_json_body`). No route or JSON change. Roughly −90/+40 lines.
- **Smallest credible slice:** `RequestError::status()` plus a host test asserting every variant; retarget the three mappings. The body helper can follow.
- **Regression risks:** `RequestSocket`'s `Drop` does `SHUT_RD`; today the guard lives until after the response write (`:409, 509`). Keep the guard in the handler so the half-close does not move earlier. Status codes must not change; the 400 message for a malformed gain comes from `GainError::Malformed`'s Display and the page shows it verbatim.
- **Migration concerns:** none.
- **Existing validation:** `core/tests/sonos_web_http.rs`, `web.rs` unit tests, `test/sonos-web.test.cjs`.
- **Additional validation required:** a host test for `RequestError::status()`; on a board, curl each POST with a missing `Content-Length`, chunked body, oversized body and slow drip, comparing codes against the current firmware (`gotchas.md:575` has the curl form).
- **Impact:** medium. **Confidence:** high. **Implementation effort:** small. **Blast radius:** local. **Prerequisites:** none. **Priority:** 4.

### SIFT-S08-01

- **Finding ID:** SIFT-S08-01
- **Authoritative subsystem:** S08 Encoder pipeline and runtime
- **Title:** One typed encoding for the lock-free active `OutputFormat`
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/stream_runtime.rs:98` (`block_frames: AtomicU32`), `:131` (initialised from `format.block_frames()`), `:363-366` (stored on commit), `:156-160` (decoded back with `if block_frames == 1152 { Mp3 } else { Flac }`). `firmware/src/stream_health.rs:107` stores the format as `u32::from(format == OutputFormat::Mp3)` and `:132-136` decodes `words[1] == 1`. `:219` is the only other reader of `block_frames`, and it converts it back into a frame count the format already knows. The authoritative enum is `core/src/stream_buffer.rs:41-58`.
- **Interfaces and call sites:** `Runtime::admit` (the unhealthy path answers from the atomic without the `active` mutex), `Supervisor::observe`, `HealthGate::publish/progress`.
- **Tests and intent evidence:** `stream_runtime.rs:406-424` (`unhealthy_compressed_admission_does_not_wait_for_active_mutex`) fixes the intent; `core/tests/replay_pipeline.rs` health tests and `runtime_switch.rs` exercise the transitions.
- **Current representation:** the active codec is represented as a frame count and reconstructed by comparing against a literal copied from `stream_buffer.rs:56`; the health gate uses a second, unrelated bool encoding for the same enum.
- **Current complexity or invalid states:** two encodings of one two-variant enum in two files with the enum in a third crate; a third codec or a codec sharing a block size decodes wrongly with no compiler or test signal, and the unhealthy admit path would then answer `NotFound` instead of `ServiceUnavailable`.
- **Why it is material:** it is a lookup by side effect on a hot admission path and a silent trap for the next format.
- **Proposed representation:** `OutputFormat::code(self) -> u32` / `from_code(u32) -> Option<Self>` (or a `#[repr(u8)]` discriminant) in `core/src/stream_buffer.rs`; `Runtime.active_code: AtomicU32`; `admit` decodes directly; `observe` decodes then calls `.block_frames()`; the health gate stores and loads the same code.
- **Why it is simpler:** one authority, no magic numbers, exhaustive `match` for any new variant.
- **Implementation scope:** `core/src/stream_buffer.rs` (+~10 lines), `firmware/src/stream_runtime.rs` (`:98, :131, :156-160, :363-366`), `firmware/src/stream_health.rs` (`:107, :132-136, :219`). About 30 lines. No wire or settings change.
- **Smallest credible slice:** the `code`/`from_code` pair and the runtime atomic; the health gate's bool can follow.
- **Regression risks:** the atomic must still be stored before `active.retired` flips at commit (`:363-368`), and the unhealthy admit path must keep avoiding the `active` mutex. Treat a `None` decode as impossible by construction and say so.
- **Migration concerns:** none.
- **Existing validation:** the inline tests named above, `replay_pipeline.rs`, `runtime_switch.rs`.
- **Additional validation required:** one assertion that unhealthy admission refuses the active codec's path with `ServiceUnavailable` for both formats (today only MP3 is checked).
- **Impact:** medium. **Confidence:** high. **Implementation effort:** small. **Blast radius:** subsystem (shared enum gains a method). **Prerequisites:** none. **Priority:** 5.

### SIFT-S12-01

- **Finding ID:** SIFT-S12-01
- **Authoritative subsystem:** S12 Telemetry, device metrics, diagnostics
- **Title:** One bounded receipt queue and one loss-receipt policy for listener and encoder telemetry
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/listener_telemetry.rs:245-313` (`Queue`: `Mutex<([Option<Record>; 8], head, len)>`, `dropped`, `dropped_full`, `dropped_contended`, `dropped_poisoned`, `loss_occurred`; `initialize`/`publish`/`take`/getters) and `firmware/src/encoder_telemetry.rs:136-212` (the same struct with `8` replaced by `2`, the same `publish` branch on `TryLockError`, plus one extra `invalidate()` at 183). Exporters `firmware/src/stream.rs:1510-1581` and `:1584-1631` each run the same two-batch drain and the same loss-receipt policy with their own `static LAST_DROPPED` / `static LOSS_REPORTED` (`:1561/1564` and `:1610/1611`), the same emit guard, the same stack sample, a `device_metrics::Loss` publish and a log line differing only in prefix.
- **Interfaces and call sites:** `listener_telemetry::EVENTS` and `encoder_telemetry::EVENTS` (initialised in `lib.rs:100-105`, fed from `stream.rs` and `stream_pipeline.rs`); `/api/metrics` families `listener_loss` and `encoder_loss` already share one `Loss` struct (`device_metrics.rs:84-90`).
- **Tests and intent evidence:** `listener_telemetry.rs:415-520` and `encoder_telemetry.rs:411-440` pin the same semantics twice; `core/tests/listener_telemetry.rs:46-82`, `core/tests/encoder_telemetry.rs:423-431, 475-489`; `gotchas.md` "Listener loss receipts follow event batches, not a periodic clock". History: 3aa5b5f built the listener queue, bb56186 added the loss counters to it, 05cf802 copied the evolved queue and the exporter block for the encoder.
- **Current representation:** two hand-rolled copies of one algorithm and two copies of one policy, in four places.
- **Current complexity or invalid states:** any change to what counts as loss, how the sticky flag resets or when the receipt line is emitted must be made in four places and verified by two near-identical test sets; the surfaces have already drifted (`invalidate()` on one; `#[inline(never)]` split on one exporter only).
- **Why it is material:** the JSON contract treats the two as one concept; the firmware does not, and the gotcha's rule has two implementations.
- **Proposed representation:** one `Queue<T: Copy, const N: usize>` (storage `Mutex<([Option<T>; N], usize, usize)>` with `[const { None }; N]`, the five atomics, the same methods) with `listener_telemetry::EVENTS: Queue<Record, 8>` and `encoder_telemetry::EVENTS: Queue<Record, 2>` as aliases; one `LossReceipt { last_dropped, reported }` whose `observe(queue, batch_nonempty, now, stack_free) -> Option<Loss>` owns the emit rule; the two exporters call it and differ only in which `CACHE.publish_*_loss` they hit and the log prefix.
- **Why it is simpler:** one of the two queue implementations and one of the two policy blocks are deleted; the gotcha's rule lives in one function with one test.
- **Implementation scope:** `listener_telemetry.rs`, `encoder_telemetry.rs`, a small shared module (or host it in `listener_telemetry.rs`, which `encoder_telemetry` already imports from), `stream.rs:1510-1631` (S07's file), the `#[path]` lists in `core/tests/{device_metrics,device_metrics_http,listener_telemetry,encoder_telemetry,close_reaper}.rs`. No JSON or UART line change. `rust-version = "1.82"` covers the inline-const array repeat (`startup_capture_trace::Bank<N>` already uses it).
- **Smallest credible slice:** the generic queue with the two aliases and the collapsed queue tests; the `LossReceipt` extraction second.
- **Regression risks:** `encoder_telemetry::Queue::invalidate()` has a second, producer-side caller (`encoder_telemetry.rs:285`, probe window exhaustion), so the encoder's `loss_occurred` carries a meaning the listener's lacks; `LossReceipt` must not assume `loss_occurred` implies `dropped > 0`. The encoder exporter's `#[inline(never)]` split exists for stack reasons; a shared drain helper must keep that split and must not grow the listener path's frame either. `queue_bytes=`/`record_bytes=` on the UART lines are `size_of` values the harness compares against the host; same `T`/`N` yields the same layout but verify on target. `LOSS_REPORTED` must stay per-queue.
- **Migration concerns:** the edit touches several `#[path]`-included files at once; freeze the tree during `scripts/check` (gotchas).
- **Existing validation:** the tests listed above plus `core/tests/device_metrics.rs` (`loss_receipts_are_independent`) and `close_reaper.rs`.
- **Additional validation required:** one test for `LossReceipt::observe` (nonempty batch; dropped changed with empty batch; first sticky loss with unchanged counter; no-op otherwise); one soak with `scripts/stream_soak.py` and a `scripts/device_metrics.py` pass, checking both receipt lines still appear per batch and `queue_bytes` matches the host print.
- **Impact:** medium. **Confidence:** high. **Implementation effort:** medium. **Blast radius:** cross-subsystem (the exporter lives in S07's file). **Prerequisites:** none. **Priority:** 7.

### SIFT-S07-01

- **Finding ID:** SIFT-S07-01
- **Authoritative subsystem:** S07 Stream server and listener lifecycle
- **Title:** Parse the stream request line once into a typed `StreamRequest`
- **Verdict:** `recommend`
- **Primary evidence:** three copy-pasted query parsers, each re-splitting the target and walking `&`-fields with its own duplicate-key rule: `core/src/stream_http.rs:463-485` (`request_listener_policy`), `:578-601` (`request_network_generation`), `:603-630` (`request_sonos_intent`); `target_route` (`:658-712`) calls all three again for validity (`:672-676`) and hard-codes the same key names a fourth time in its filter (`:681-688`). `firmware/src/stream.rs` `answer` parses the line three times (`:477`, `:488`, `:496`) then `runtime.admit(&request_line)` (`:503`) runs `route_for` up to twice (`stream_runtime.rs:161`, `:172`). `Capacity::admit_request` (`core/src/stream_wait.rs:532-562`) parses intent, generation and `route_for` again, and `answer` calls it inside a 10 ms polling loop for up to 12 s (`stream.rs:538-568`).
- **Interfaces and call sites:** `answer`, `Runtime::admit` (S08), `Capacity::admit_request`, `route_for`; fuzz oracle `fuzz/src/lib.rs:193,228`.
- **Tests and intent evidence:** `core/tests/{listener_policy,stream_network_query,stream_codec_profile,stream_admission,listener_telemetry}.rs`; `fuzz/fuzz_targets/http_head.rs`. History shows the drift mechanism: each tag arrived as a new clone of the loop plus an edit to the filter (e5cef5b generation, 48b89d8 sonos_intent, aae0089 exact).
- **Current representation:** a string that several owners interrogate independently.
- **Current complexity or invalid states:** refusal precedence (400 malformed tag, 404, 503 stale generation, 503 intent mismatch) is derived independently in `answer`, `target_route` and `admit_request`, and only `route_for` is host-tested and fuzzed; `answer`'s ordering is board-only code. The set of tag keys that ride beside `buffer=` exists in four places. About five re-parses per 10 ms turn while a Sonos reconnect waits.
- **Why it is material:** duplicated semantic logic on the admission path, with a history of growing by cloning.
- **Proposed representation:** `StreamRequest { method, path, generation: Option<u32>, sonos_intent: Option<u64>, policy: ListenerPolicy, buffer: Option<BufferPreset> }` with one `parse(line) -> Result<StreamRequest, Status>` holding the one list of recognised keys; `route_for(&req, active)`, `Runtime::admit(&req)` and `Capacity::admit_request(&req, ...)` take the parsed value; `answer` reads fields and the `unreachable!()` re-match at `stream.rs:514-525` collapses.
- **Why it is simpler:** the request is a record with optional tags; parsing it once is the organising model.
- **Implementation scope:** `core/src/stream_http.rs`, `core/src/stream_wait.rs:532-562`, `firmware/src/stream.rs:441-632`, `firmware/src/stream_runtime.rs:154-172` (signature only), and the tests listed above plus `fuzz/fuzz_targets/http_head.rs:36-37`. Roughly 150 lines net removed.
- **Smallest credible slice:** the `StreamRequest` type and `parse` in `stream_http.rs` with the three old parsers as thin accessors over it, and the fuzz oracle pointed at the new parse. Call-site migration second.
- **Regression risks:** refusal precedence is observable to clients and harnesses; `route_for`'s HEAD rewrite (`:646-655`) must run after the single parse; `admit_request` must keep returning `BadRequest` for a malformed intent before checking the registry.
- **Migration concerns:** none persisted.
- **Existing validation:** the tests and fuzz target above.
- **Additional validation required:** a host test driving the parsed request through `admit_request` with each tag malformed; one asserting the key list is single-sourced; the `http_head` fuzz target run through the new parse.
- **Impact:** medium. **Confidence:** high. **Implementation effort:** medium. **Blast radius:** cross-subsystem (S08 signature). **Prerequisites:** SIFT-S08-01 first, since both edit `Runtime::admit`. **Priority:** 6.

### SIFT-S10-01

- **Finding ID:** SIFT-S10-01
- **Authoritative subsystem:** S10 Sonos protocol and command model (merged with S11's second candidate; the firmware half is listed here)
- **Title:** Retire the WebSocket session-command layer in both crates, or quarantine the receive-only fixture
- **Verdict:** `recommend`
- **Primary evidence:** `core/src/sonos.rs:173-233` (`Scope`, `Command`, `encode_command`), `:350-366` (`PendingCommand`, `ReplyMatch`), `:368-468` (five event structs, `Event`, `impl Message { match_reply, event }`), tests `:834-1203`. Repo-wide: `encode_command`, `match_reply`, `PendingCommand`, `ReplyMatch` have zero callers outside that file; `parse_message`/`MessageAssembler` are used only by the firmware fixture (`firmware/src/sonos.rs:312, 1481`, inside `#[cfg(any(test, feature = "sonos-fixture"))]` regions at `:297-493` and the `fixture` module `:1049-1616`) and by tests (`:2553, 2874`). Firmware `#[cfg(test)]`-only helpers with no production caller: `with_create_send` `:149-160`, `scope_matches` `:444-456`, `release_session_after_error` `:836-851`, `queue_load_body` `:853-857`, and about ten tests that pin behaviour no production path has.
- **Interfaces and call sites:** `firmware/Cargo.toml:48` (`sonos-fixture`), `firmware/src/lib.rs:23-24` (`pub use sonos::check_transport`), `firmware/examples/sonos-check.rs` and `firmware/examples/tls-capture.rs:216` (both call `check_transport`; `scripts/check` builds no examples, so they would rot silently), `firmware/assets/sonos-roots.pem` (+ `.PROVENANCE.md`), the TLS root bundle referenced by nothing in any crate or build script. Production control is `core::sonos_upnp::Client`.
- **Tests and intent evidence:** `gotchas.md:1141-1146` ("Doctor Biz selected HTTP/UPnP Sonos control on October 11 … do not resume the failed TLS experiments") and `:73-75`; `docs/superpowers/plans/2026-10-11-sonos-http.md:11-13`.
- **Current representation:** two control models side by side, one with no production path, inside the modules whose ABOUTME lines describe a single bounded controller.
- **Current complexity or invalid states:** roughly 850 lines of firmware (about 22 % of `sonos.rs`) and about 250 lines of core model plus ~370 lines of core tests exist for a transport the project retired; every host run compiles and executes the dead layer; a reader must work out which transport is live from `cfg` attributes.
- **Why it is material:** it is the largest comprehension cost in the Sonos stack and the precondition for SIFT-S11-01 being a readable change.
- **Proposed representation:** one transport. Delete the session command/reply/event model in core, the fixture and the four test-only helpers in firmware, their tests, the `sonos-fixture` feature, the `lib.rs` re-export and the `sonos-check` example. If the fixture is wanted as a hardware diagnostic, the smaller move is `firmware/src/sonos_fixture.rs` behind the feature, keeping `MessageHeader`/`parse_message`/`MessageAssembler` in core for it; that still removes the dead helpers and tests.
- **Why it is simpler:** `sonos_upnp.rs` becomes the single control authority; the worker file holds only the live path.
- **Implementation scope:** `core/src/sonos.rs`, `firmware/src/sonos.rs` (deletions only on the live path), `firmware/src/lib.rs:23-24`, `firmware/Cargo.toml:48`, `firmware/examples/{sonos-check,tls-capture}.rs`, `firmware/assets/sonos-roots.pem` and its provenance note, the spec's test-strategy note on `--features sonos-fixture`. No NVS, JSON or web contract change. Ownership note: most deleted lines are in S11's file; the finding sits in S10 because the session types it removes are defined in core and the firmware code exists only to drive them.
- **Smallest credible slice:** delete the core session model (`:173-233, 350-468`) and the four firmware test-only helpers with their tests; decide on the fixture separately.
- **Regression risks:** none at runtime (no production caller). The archived UART evidence the fixture produced lives in `docs/audit-evidence` and is not regenerated. Deleting a module's worth of code needs Doctor Biz's explicit permission per CLAUDE.md.
- **Migration concerns:** none.
- **Existing validation:** `scripts/check`; `core/tests/sonos_adapter.rs` includes the firmware module.
- **Additional validation required:** a firmware build without the feature; if the fixture is kept, a compile of `--features sonos-fixture`.
- **Impact:** medium. **Confidence:** high. **Implementation effort:** medium (deletion with care). **Blast radius:** cross-subsystem (S10, S11, a Cargo feature, two examples, an asset). **Prerequisites:** Doctor Biz's permission to delete. **Priority:** 8.

### SIFT-S11-01

- **Finding ID:** SIFT-S11-01
- **Authoritative subsystem:** S11 Sonos controller worker
- **Title:** One `Session` enum for the worker's connection state instead of six `Option`s torn down by hand
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/sonos.rs:1676-1696` (`struct Worker { socket_lease, socket, group, owned_uri, recovery, retry, .. }`); set-together site `connect()` `:1931-1941`; five partial teardowns each clearing a different subset: `fail()` `:1811-1838` (all six, `retry = Some`, state `Error`), `stop()` `:2111-2151`, `cancel_local_if_stale()` `:2153-2174` (clears `retry`/`recovery` only, keeps socket/group/owned_uri), `poll()`'s Stopped branch plus `ControlObservation::finish` `:790-815`, and the reconnect-error branch in `run()` `:1785-1797` (clears socket/lease/group, leaves `retry` and state). The loop re-derives its phase from the `Option`s every 50 ms (`:1770`, `:1788`, `:1796`); "is recovery in flight" is asked two ways (`recovery.is_none()` vs `matches!(state, Degraded | Recovering)` in `save_config` `:236`).
- **Interfaces and call sites:** `sonos::start` (`lib.rs:122`), the `Command` arms, `check_network()` `:1802` (reads `socket_lease`), `Snapshot.state` as served by `/api/sonos`.
- **Tests and intent evidence:** the `device` module (`:859-2253`) is `#[cfg(target_os = "espidf")]` and has no host tests; `core/tests/sonos_adapter.rs` covers only the extracted helpers. Hardware coverage is `scripts/sonos-http-check`; `gotchas.md:50-54` (local Stopped can precede remote STOPPED) and `:10-75` set the ownership rules. Coordinator note: the reviewer's cited line numbers were about 60 lines high; the ranges above are from the coordinator's re-read.
- **Current representation:** six independent `Option`s whose joint meaning is the phase the comments describe ("Reconnect observes status only", "Retain URI evidence for its queued Stop").
- **Current complexity or invalid states:** `socket: Some` with `socket_lease: None` or `group: None`; `owned_uri: Some` with `socket: None`; `recovery: Some` with `owned_uri: None`; `retry: Some` with `socket: Some`. None is reachable today, only because three separate sites keep the discipline by hand; `stop()` matches a 3-tuple whose `take()` runs even when `socket` is `None`, so an `owned_uri`-without-socket state would silently drop the URI evidence and skip the checked Stop.
- **Why it is material:** the worker is the one component with hardware-only validation; making its phases a type is the cheapest way to stop a future teardown edit from leaving a half-torn session.
- **Proposed representation:** `enum Session { Idle, Reconnecting(Retry), Connected { client, lease, group }, Owned { client, lease, group, uri, recovery: Option<ControlRecovery> } }` owned by `Worker`; `connect()` returns `Connected`; `play_current` upgrades to `Owned`; each teardown becomes a `match` arm consuming the old variant so "keep the socket" or "arm retry" is explicit.
- **Why it is simpler:** the four phases the comments already name become the type; the three `is_some` probes become one `match`; the `(uri, client, lease)` tuple disappears.
- **Implementation scope:** `firmware/src/sonos.rs` `device::Worker` only; `ControlObservation::finish` can keep its signature. `Snapshot`, `State`, `Command`, `Handle`, NVS and `/api/sonos` untouched. Roughly 150-250 changed lines.
- **Smallest credible slice:** introduce the enum for `socket`/`socket_lease`/`group` only (`Idle`/`Reconnecting`/`Connected`), leaving `owned_uri`/`recovery` as fields; fold them into `Owned` second.
- **Regression risks:** no host coverage of `Worker`; a transition mistake surfaces only on hardware. The wire contract asserted by `scripts/sonos_http_check.py:213-438` (`state` values and when they change) must hold; `fail()`'s ordering (recovery → queue.clear → policy OwnershipLost → drop socket) should be preserved.
- **Migration concerns:** none persisted.
- **Existing validation:** host tests of the extracted helpers via `core/tests/sonos_adapter.rs`; `scripts/sonos-http-check`.
- **Additional validation required:** extract the transition functions from `Worker` the way `revoke_cancelled_play`/`stop_completion` already are and host-test them; then one `scripts/sonos-http-check` run covering Play → Stop, Play → cancel during play, and a forced control-read timeout (Degraded → Recovering → Playing) on the office board.
- **Impact:** high. **Confidence:** medium. **Implementation effort:** large. **Blast radius:** subsystem. **Prerequisites:** none required; much easier after SIFT-S10-01 removes the dead layer from the same file. It does not overlap SIFT-S11-02 (session fields and teardowns versus `Handle::submit` and three `Command` arms). **Priority:** 10.

### SIFT-S11-02

- **Finding ID:** SIFT-S11-02 (drafted as SIFT-S03-02; reassigned because the change lands in `Handle::submit` and the worker, S11's code)
- **Authoritative subsystem:** S11 Sonos controller worker
- **Title:** Make `sonos::Handle::submit` the single admission authority; `web.rs` only maps its refusal to a status
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/web.rs:345-364` refuses three cases before submitting (any action except `Enable`/`Stop` while `config.enabled` is false; `Format{Mp3}` and `Buffer{..}` while `!buffer_available`), calling `sonos.snapshot()` up to three times per request. The same rules live downstream: `Handle::submit` re-checks the buffer rule (`firmware/src/sonos.rs:987`), the worker checks it a third time (`:1906-1911`) and the format rule a second time (`:1891-1893`); any refusal that slips past `web.rs` surfaces as 503 "controller unavailable or busy" (`web.rs:378-382`). The `enabled` gate exists only in `web.rs`: no `.enabled` check in `submit` or the worker (`core/src/sonos.rs:46-50` only neutralises autoplay). `web.rs:366-375` then maps `Action → Command` one-to-one.
- **Interfaces and call sites:** `POST /api/sonos`; `Handle::submit`; the worker's `SetFormat`/`SetBufferPreset` arms; autoplay submits internally and gets no `enabled` gate.
- **Tests and intent evidence:** `test/sonos-web.test.cjs` matches the 409 bodies (`/recognition/`, `/MP3.*next Play/`); `firmware/src/sonos.rs:67-73` (Stop and Enable admitted while disabled).
- **Current representation:** admission policy for one command stream split across the HTTP layer, the handle and the worker with no single owner; a 409 can only come from the HTTP layer.
- **Current complexity or invalid states:** a later caller of `submit` gets no `enabled` gate; a TOCTOU window between the `web.rs` snapshot checks and `submit` is open by construction; `buffer_available` is a boot-time constant (`:1033`, `!CONFIG.shazam_enabled`) guarded three times.
- **Why it is material:** it clarifies ownership enough to remove cross-file coordination on every new Sonos command.
- **Proposed representation:** `Handle::submit(Command) -> Result<(), Refusal>` with `Refusal { Disabled, BufferUnavailable, Mp3Unavailable, QueueFull, WorkerStopped, InvalidPlayer }`, all gates evaluated under one `status` lock; `web.rs` parses `Action`, converts, submits, and maps `Refusal → (409|503, message)`; the worker's duplicate checks go or become debug asserts.
- **Why it is simpler:** one authority, one lock, one place to add the next rule.
- **Implementation scope:** `firmware/src/web.rs:345-382`, `firmware/src/sonos.rs` `submit` and the three worker arms; possibly `core/src/sonos_web.rs` if `Action` and `Command` merge. The 409 messages must survive verbatim.
- **Smallest credible slice:** move the `enabled` gate into `submit` (before `ensure_started()` so a refused request does not spawn the worker) and return a typed `Refusal`; leave the buffer checks for a second pass.
- **Regression risks:** `Stop` and `Enable` must stay admitted while disabled; the worker-side checks are also autoplay's path, so the submit-side check must cover that caller.
- **Migration concerns:** none.
- **Existing validation:** none on `Handle::submit` itself: it sits inside the `device` module (`#[cfg(target_os = "espidf")]`), so the host-run tests in the file exercise `Commands::submit`, not the handle. `core/src/sonos_web.rs` tests and `test/sonos-web.test.cjs` cover the parse and the 409 bodies.
- **Additional validation required:** extract the gate into a host-testable function the way `revoke_cancelled_play` is, with one test per `Refusal`; on a board, POST each refused action and confirm the 409 bodies are unchanged. While in the file: route the three raw `config.autoplay` reads (`firmware/src/sonos.rs:265, 911, 958`) through one `SonosConfig::autoplay()` accessor that applies `&& enabled`, so the `Disabled` refusal on `Autoplay` stops carrying the invariant (see the demoted candidate in section 7).
- **Impact:** medium. **Confidence:** medium (the right lock and `ensure_started` ordering are the worker owner's call). **Implementation effort:** medium. **Blast radius:** cross-subsystem (the handler in S03). **Prerequisites:** easier after SIFT-S10-01. **Priority:** 9.

### SIFT-S07-02

- **Finding ID:** SIFT-S07-02
- **Authoritative subsystem:** S07 Stream server and listener lifecycle
- **Title:** One departure-check authority per listener turn
- **Verdict:** `recommend`
- **Primary evidence:** 24 `slot.revoked()` / `network.is_current()` sites in `firmware/src/stream.rs`: `stream_wav` at `:801-806`, inside the `cursor_before` cancel closure `:812`, re-evaluated to pick the `End` `:814-818`, then three more pairs per loop turn (`:831-836`, `:867-872`, `:877-882`) plus `read_some`'s closure (`:755`); `stream_compressed` repeats the pair three times per turn (`:1215-1220`, `:1238-1243`, `:1287-1292`). Each `Slot::revoked()` (`core/src/stream_wait.rs:634-640`) calls `Capacity::refresh()` (`:414-436`): registry lock, token clone, token lock, `network.is_current` (its own mutex). The pipeline's `step_network` checks `network.is_current` again (`stream_pipeline.rs:543`).
- **Interfaces and call sites:** `ListenerConnection` (`stream.rs:160-191`), the WAV and compressed loops, `CompressedListener::step_network` (S08).
- **Tests and intent evidence:** `core/tests/listener_skip.rs` asserts end precedence for the compressed listener; `close_reaper.rs` ordering; replay fixtures `ownership.json`, `disconnect-stale.json`; Loom `listener_runtime_loom.rs`.
- **Current representation:** the departure reason is a derived value computed by hand in eleven places with the `Retired` > `NetworkChanged` precedence implicit in the ordering of every pair.
- **Current complexity or invalid states:** no invalid state. The precedence rule is reconstructed by hand at eleven sites, and a third departure cause means touching every one. (A WAV chunk is 5 ms, so one WAV listener also makes several hundred `refresh()` calls a second across three mutexes; that is from reading, not a measurement, and is not the basis of this finding.)
- **Why it is material:** it removes duplicated control flow on the listener loop without hiding it; the departure reason gets one owner.
- **Proposed representation:** `ListenerConnection::departed(&self) -> Option<End>` encoding the precedence once; each loop turn checks it once at the top and once after the socket write; the cancel closure becomes `|| guard.departed().is_some()`; `step_network` drops its copy. Keep `Slot::revoked()`'s `refresh()` semantics for now; whether another path already refreshes on token revocation is "verify at implementation".
- **Why it is simpler:** one authority for the reason, and the mid-turn copies go.
- **Implementation scope:** `firmware/src/stream.rs:795-882` and `:1196-1300`, `ListenerConnection`; optionally `core/src/stream_wait.rs:634-640`. About 60 lines removed, 15 added.
- **Smallest credible slice:** the guard method and the WAV loop; the compressed loop second.
- **Regression risks:** a revocation or network change is noticed up to one chunk or one write later; the departure `End` on the UART line must not change for the harness parsers; the `Retired` vs `NetworkChanged` precedence must be preserved exactly.
- **Migration concerns:** none.
- **Existing validation:** the tests above; the `freeze-N` and `network-fault` hardware trials measure departure timing.
- **Additional validation required:** a host test on the guard (revoked+stale yields `Retired`; stale-only yields `NetworkChanged`); a before/after `stream-listener` line comparison on hardware.
- **Impact:** low (the lock-traffic claim is unmeasured and the edit changes departure latency the `freeze-N` trials time). **Confidence:** medium. **Implementation effort:** small. **Blast radius:** subsystem (S08's `step_network`). **Prerequisites:** none. **Priority:** 13.

### SIFT-S08-02

- **Finding ID:** SIFT-S08-02
- **Authoritative subsystem:** S08 Encoder pipeline and runtime
- **Title:** A `Token` newtype for the health token instead of `& 3` / `>> 2` arithmetic in three files
- **Verdict:** `recommend`
- **Primary evidence:** layout defined in `firmware/src/stream_health.rs:24-83` (low 2 bits = state, high 30 = operator epoch) with typed `state()`/`healthy()` accessors at `:40-50`; bypassed by raw bit ops in `firmware/src/stream_pipeline.rs:136` (`& 3 == 3`), `:147` (`& 3 == 1`), `:171` (`>> 2`); `stream_health.rs:182-183`, `:229`, `:241`; `firmware/src/stream.rs:1389` (`token() >> 2`). `transition(expected: u32, State)` `:51-66` and `replace()` `:67-83` re-encode the layout.
- **Interfaces and call sites:** `EncoderPump::step_read` (every 5 ms chunk), `Supervisor::observe`, the metrics exporter.
- **Tests and intent evidence:** `stream_health.rs:314-321` and `core/tests/replay_pipeline.rs:884-906` pin the layout directly.
- **Current representation:** a `u32` whose meaning four readers each know by heart.
- **Current complexity or invalid states:** the pump compares against literal `3` and `1` rather than `State::Terminal`/`State::ResetRequested`; nothing stops a reader mixing `state()` with a raw compare that drifts if the state set or bit width changes.
- **Why it is material:** the layout assumption is removed from the hot path and from three files.
- **Proposed representation:** `struct Token(u32)` with `state()`, `operator_epoch()` and private pack/unpack used by `transition(Token, State)`, `replace()`, `state()`, `healthy()`; `HealthGate::token()` returns `Token`.
- **Why it is simpler:** the layout lives in exactly one `impl`.
- **Implementation scope:** `stream_health.rs` (~25 lines), `stream_pipeline.rs:135-137, 147, 171`, `stream_health.rs` Supervisor sites, `stream.rs:1389`, the two test fixtures. About 40-50 lines.
- **Smallest credible slice:** the newtype with accessors and `transition(Token, State)`; call sites follow mechanically.
- **Regression risks:** `transition` must keep comparing the whole token in its CAS, or a stale-epoch supervisor could win; make `Token` the only argument type. Keep `SeqCst`.
- **Migration concerns:** none.
- **Existing validation:** `stream_health.rs` inline tests, `replay_pipeline.rs` health tests, `stream_runtime.rs` inline tests.
- **Additional validation required:** one unit test round-tripping every `State` through pack/unpack with a nonzero epoch.
- **Impact:** low. **Confidence:** medium. **Implementation effort:** small. **Blast radius:** subsystem. **Prerequisites:** none. **Priority:** 11.

### SIFT-S02-01

- **Finding ID:** SIFT-S02-01
- **Authoritative subsystem:** S02 Recognition
- **Title:** `Outcome::Match` carries the `Song`; drop the `(Outcome, Option<Song>)` sidecar tuple
- **Verdict:** `recommend`
- **Primary evidence:** `core/src/policy.rs:45-54` (`Outcome::Match { key: String }`); `firmware/src/shazam_http.rs:15-24` (`recognize` returns `(Outcome, Option<Song>)`, building `Match { key: song.key.clone() }` beside `Some(song)`); `firmware/src/recognizer.rs:67, 73-94` (`ask_shazam` returns the same tuple; `run` matches `(policy.on_outcome(..), song)` with a `_ => {}` catch-all); `core/src/policy.rs:259-268` (the test `Board::answer` reconstructs the pairing rule).
- **Interfaces and call sites:** `Policy::on_outcome`, `SongStore::set_now`; the only other `Outcome` constructors are `core/tests/recognition_admission.rs:77,165`.
- **Tests and intent evidence:** the 20 unit tests in `policy.rs`; spec R16 on store freezing during asks.
- **Current representation:** a tuple whose two halves must agree by construction in one function.
- **Current complexity or invalid states:** `(Match{key}, None)`, `(Match{key: a}, Some(song{key: b}))` and `(Miss, Some(song))` are representable; `(SetNow, None)` would put the policy in "matched, recheck every 30 s" while the store shows nothing, silently. `key` is a copy of `song.key`.
- **Why it is material:** it removes representable invalid states on the path that decides what the page shows.
- **Proposed representation:** `enum Outcome { Match(Song), Miss, RateLimited, Failed }`; `on_outcome` reads `song.key`; `recognize` and `ask_shazam` return `Outcome`; the recognizer matches `(SetNow, Match(song))`. `Song` already lives in `core::shazam`, which `core::store` imports, so `policy` stays host-testable.
- **Why it is simpler:** one value, one key.
- **Implementation scope:** `core/src/policy.rs` (enum, `on_outcome`, ~12 test constructors via a `song(key)` helper as `store.rs:230` does), `firmware/src/shazam_http.rs`, `firmware/src/recognizer.rs` (`run`, `ask_shazam`, the `"shazam: {:?}"` log at 147-150 should print the variant or key, not a whole `Song`). About 40 lines. No JSON change.
- **Smallest credible slice:** the enum change with the test helper; the firmware edits are mechanical.
- **Regression risks:** behaviour identical; only the log line changes. `Song` derives `PartialEq, Eq`, so `Outcome` keeps its test comparisons.
- **Migration concerns:** none.
- **Existing validation:** `policy.rs` unit tests, `core/tests/recognition_admission.rs`.
- **Additional validation required:** on a board, one matched ask and one miss showing the same "now playing:" lines.
- **Impact:** low. **Confidence:** medium (no evidence of a defect having occurred). **Implementation effort:** small. **Blast radius:** local. **Prerequisites:** none. **Priority:** 12.

### SIFT-S01-01

- **Finding ID:** SIFT-S01-01
- **Authoritative subsystem:** S01 Audio capture path
- **Title:** One capture-window snapshot feeds both `/api/metrics` and the UART line
- **Verdict:** `recommend`
- **Primary evidence:** `firmware/src/audio_in.rs:777-874` (`log_capture`): window-max resets at `:826-829` (`READ_SERVICE_DELAY_US`, `RAW_WORK_US`, `METER_WORK_US`, `STORE_WORK_US` swapped before `publish_capture`) versus `:869-871` (`STAGED_MAX_FRAMES`, `STAGING_PENDING_MAX_ATTEMPTS`, `STAGING_PENDING_MAX_US` swapped inside the `log::info!` argument list, after `publish_capture`). `device_metrics::Capture` (`firmware/src/device_metrics.rs:8-33`) carries none of the staging fields, the runtime window, the deficit estimate or the window-start heap values; the staging fields appear in no parser.
- **Interfaces and call sites:** `CACHE.publish_capture`, the UART `capture:` line read by the evidence parsers under `docs/audit-evidence/**`, `scripts/device_metrics.py`, `scripts/fault_check.py`, `app.js`.
- **Tests and intent evidence:** `device_metrics` unit tests and the two `Capture` test constructors (~`:499`, `:571`); `docs/superpowers/plans/2026-10-13-channel-diagnostics.md:15-30`.
- **Current representation:** 30 module-level statics (`:69-100`) are the authority; no single value ever holds a coherent window; two serialisations with different field sets.
- **Current complexity or invalid states:** the ordering of `publish_capture` between the two groups of `swap(0)` is load-bearing: move it and JSON and log disagree on which window a maximum belongs to. A staging field added to `Capture` has to notice a swap buried in a format-argument list.
- **Why it is material:** it removes an ordering hazard and a duplicate field list on the line every soak evaluates.
- **Proposed representation:** one plain `CaptureWindow` struct filled once at the top of `log_capture` by a single pass that loads the counters and performs every `swap(0)`; `publish_capture(window)` (make `Capture` that struct, or `From<CaptureWindow>`) and the `log::info!` read fields from `window`.
- **Why it is simpler:** the log formatter stops owning window resets, and the window is one value. The JSON contract stays as it is: `Capture` keeps its fields and is filled from the window; adding the staging fields to `/api/metrics` would be a schema change and is not proposed.
- **Implementation scope:** `firmware/src/audio_in.rs:777-874`; `firmware/src/device_metrics.rs:8-33` plus the two test constructors; JSON readers only if field names change (keep them). Roughly 60-90 lines.
- **Smallest credible slice:** hoist the three staging swaps out of the log arguments into the same block as the other four and pass them through `Capture`; the struct rename can follow.
- **Regression risks:** `log_capture` is `#[inline(never)]` to keep the formatting frame off the metrics loop and the metrics task runs on 6 KiB after an LTO overflow (`:147-149`); a ~40-field struct is a few hundred bytes, but verify the high-water mark on hardware. Log-line field order must stay identical for the evidence parsers.
- **Migration concerns:** none; the JSON and the UART line keep their fields and order.
- **Existing validation:** `device_metrics` unit tests; `scripts/device_metrics.py` and `scripts/fault_check.py` against a board.
- **Additional validation required:** one UART capture diffed field-for-field against the pre-change line; `metrics_stack_free_bytes` on the UART after the change.
- **Impact:** low. **Confidence:** medium. **Implementation effort:** small. **Blast radius:** cross-subsystem (S12 owns the struct). **Prerequisites:** none. **Priority:** 14.

### SIFT-S09-01

- **Finding ID:** SIFT-S09-01
- **Authoritative subsystem:** S09 Audio codecs
- **Title:** One authority for the 24-to-16-bit stream quantizer
- **Verdict:** `recommend`
- **Primary evidence:** `core/src/flac.rs:539-542` (`fn quantize(sample: i32) -> i32 { (sample >> 8).clamp(SAMPLE_MIN, SAMPLE_MAX) }`, private, called from `push` at `:268` and `:270`); `firmware/src/stream_codec.rs:100-102` (`Mp3Encoder::push` repeats it inline: `(source >> 8).clamp(-32_768, 32_767) as i16`).
- **Interfaces and call sites:** FLAC `push`, MP3 adapter `push`; the recognition path's 16-bit conversion (`core/src/decimate.rs:273`) uses a different rounding policy and is not part of this contract.
- **Tests and intent evidence:** `core/tests/stream_codec.rs:77-110` (`mp3_adapter_uses_flac_24_to_16_scaling_without_gain_changes`) exists only to assert the two copies agree, which is the workaround that evidences the defect class; `docs/audit-evidence/2026-10-20/flac16-quantization-policy.md` weighs replacing truncation with saturated rounding and describes the policy by pointing at `flac.rs` alone; `gotchas.md:1256-1260`.
- **Current representation:** the stream's 16-bit output policy implemented twice in two crates with a test as the drift guard.
- **Current complexity or invalid states:** a policy change (the memo's open question) must land in two implementations and the cross-check test or the FLAC and MP3 streams diverge by up to one LSB of DC.
- **Why it is material:** the duplication guards an open decision.
- **Proposed representation:** make the quantizer `pub` in `core` (or a five-line `core::pcm16` module with the rails and `quantize`) and have `Mp3Encoder::push` call it, casting to `i16` after; the FLAC `push` keeps its fused loop.
- **Why it is simpler:** one function to change when the policy changes; the drift test shrinks to a call-equivalence check or goes.
- **Implementation scope:** `core/src/flac.rs` visibility (or a new tiny module plus `core/src/lib.rs`), `firmware/src/stream_codec.rs:100-102`, `core/tests/stream_codec.rs:80-110`. About 10 lines; output bytes identical.
- **Smallest credible slice:** the whole change is the slice.
- **Regression risks:** `flac::push` is a measured hot loop (`gotchas.md:692`); a `pub fn` in the same crate inlines as before, but confirm with the flac-cost benchmark. Document that the recognition path intentionally differs.
- **Migration concerns:** none.
- **Existing validation:** the bit-exact decode tests (`flac_ffmpeg.rs`, `flac_stereo.rs`, `flac_join_numbering.rs`), the FLAC unit tests on quantisation and rails, `core/tests/stream_codec.rs`, `mp3/tests/decode.rs`.
- **Additional validation required:** one flac-cost run on a board.
- **Impact:** low. **Confidence:** medium. **Implementation effort:** small. **Blast radius:** subsystem (the adapter file `stream_codec.rs` is S08's by the ledger). **Prerequisites:** none. **Priority:** 15.

### SIFT-S13-01

- **Finding ID:** SIFT-S13-01
- **Authoritative subsystem:** S13 Boot, threads and build
- **Title:** One spawn path for every production thread, with the priority ladder stated where the compiler can read it
- **Verdict:** `recommend`
- **Primary evidence:** every production thread but two goes through `ThreadSpec` (`firmware/src/threads.rs:14-53`; commit fb98593 "spawn the pinned threads through one helper"); `firmware/src/ota.rs:105-108` (`ota-health`) and `:214-217` (`ota-restart`) use `std::thread::Builder` directly, so they get no core pin, the IDF default priority, and a std `.name()` that `threads.rs:1-2` says ESP-IDF does not use. The ladder exists only as prose and has drifted: `firmware/src/stream.rs:246-248` says the stream threads sit "below the audio thread (10)"; the capture thread runs at 24 (`gotchas.md:1172-1174` records the move). `docs/superpowers/specs/2026-10-10-device-metrics-design.md:15` says metrics has a 4 KiB stack and capture priority 10; `audio_in.rs:150-161` is 6 KiB and 24. `threads.rs:18` documents the 15-byte FreeRTOS name limit; `audio-supervise` is exactly 15 with no check.
- **Interfaces and call sites:** the 13 spawn sites in the task table below; `scripts/check --firmware` compiles the device build.
- **Tests and intent evidence:** `core/tests/thread_spawn.rs`; `stream.rs:243` already uses `const _: () = assert!(...)` for the worker count, so the pattern is established; `gotchas.md:1442-1448`, `:207-209`, `:1009-1016` record the recurring stack-sizing defect class.
- **Current representation:** two spawn paths; a priority ordering three modules depend on (stream above recognizer so a fingerprint cannot starve a listener; supervisor above metrics; capture above all) that nothing checks; a name-length limit documented but not enforced.
- **Current complexity or invalid states:** the prose is already wrong in two places; a priority change in one module cannot be checked against the others.
- **Why it is material:** it clarifies ownership of scheduling facts and eliminates a second spawn path; small, but it is the kind of invariant this board has already paid for (the LTO stack overflows).
- **Proposed representation:** route `ota-health` and `ota-restart` through `ThreadSpec` and fix the two stale numbers (the required half); then, optionally, make the specs `pub(crate) const` and add in `threads.rs` a short block of `const _: () = assert!(...)` ladder facts plus a `name.to_bytes().len() <= 15` check per spec, replacing the prose at `stream.rs:246-248` with a pointer to them. The name-length assert is a real guard; the ladder asserts are prose-drift repair and the optional half.
- **Why it is simpler:** the specs stay where they are; the invariant gains one home and the second spawn path goes.
- **Implementation scope:** `threads.rs`, `audio_in.rs`, `recognizer.rs`, `sonos.rs`, `ota.rs`, the `stream.rs` comment, the device-metrics spec line. About 40 lines. The asserts are device-build only (`ThreadSpec` is `cfg(target_os = "espidf")`), which `scripts/check --firmware` compiles.
- **Smallest credible slice:** fix the two stale numbers and route the two `ota.rs` spawns through `ThreadSpec`; the ladder asserts second.
- **Regression risks:** giving the OTA threads a core pin and explicit priority changes their scheduling; `ota-restart` only sleeps 1 s then restarts, so the risk is low, but confirm an OTA push still restarts and a pending slot still marks valid.
- **Migration concerns:** none.
- **Existing validation:** `scripts/check --firmware`; `core/tests/thread_spawn.rs`.
- **Additional validation required:** one `scripts/ota-push` cycle on a board; the UART stack watermarks (`metrics_stack_free_bytes`, `listener_stack_free_bytes`, `reaper_stack_free_bytes`) after the image change.
- **Impact:** low. **Confidence:** medium (the drift and the second path are plain; the asserts are a judgement call). **Implementation effort:** small. **Blast radius:** cross-subsystem (most changed lines land in `ota.rs`, `audio_in.rs`, `recognizer.rs`, `sonos.rs` and `stream.rs`). **Prerequisites:** after SIFT-S05-01/02, so `mark_valid_when_healthy` is edited once; the OTA owner picks the two threads' core and priority. **Priority:** 16.

Production task table from the code (S13's reviewer), for the record: `audio` core 0 prio 24 8 KiB; `audio-supervise` core 1 prio 11 4 KiB; `audio-metrics` core 1 prio 3 6 KiB; `stream-in-1/2`, `stream-out` x2, `stream-reap` core 1 prio 7 5 KiB; `stream-encode` core 1 prio 7 8 KiB; `recognizer` core 1 prio 5 32 KiB; `sonos` core 0 prio 2 16 KiB (lazy); httpd unpinned 10 KiB; `ota-health` 6 KiB and `ota-restart` 2 KiB with IDF defaults. All stacks are internal RAM; `ThreadSpec` has no PSRAM option.

## 4. Smallest credible implementation slices

1. **SIFT-S05-01 (OTA validated).** Boundary: `firmware/src/ota.rs` only. Interface: `Status.validated` becomes derived from `running.state`. Migration: none. Validation: on-board push with `GET /api/ota` polling; the overlap case at second 55; check the bootloader's behaviour on rollback onto a pending slot. Deferred: the `EspOta` owner (S05-02), logging the spawn result at `:105`.
2. **SIFT-S05-02 (one `EspOta` owner).** Boundary: the same file. Interface: `UPLOADING` replaced by `try_lock`; optionally `Status.uploading`. Migration: none. Validation: 200-busy during a push, 409 on a second POST, mark succeeds across an overlap, full round trip and rollback. Deferred: any `Status` field addition unless S03 wants it.
3. **SIFT-S04-01 (`ApplyError`).** Boundary: `input_gain.rs` `apply` and the three match arms in `web.rs`. Interface: `apply` returns `Result<(), ApplyError>`. Migration: none. Validation: overlapping POSTs, reboot persistence. Deferred: collapsing `gain`/`LIVE_REGISTER` into one atomic.
4. **SIFT-S03-01 (`RequestError::status`).** Boundary: `web.rs`. Interface: one status map, one host test over every variant. Migration: none. Validation: the curl matrix against current codes. Deferred: the shared body helper and the JSON writer.
5. **SIFT-S08-01 (`OutputFormat::code`).** Boundary: `stream_buffer.rs` and `stream_runtime.rs`. Interface: `active_code: AtomicU32`. Migration: none. Validation: the inline runtime tests plus the both-formats assertion. Deferred: the health gate's bool (same change, second file).
7. **SIFT-S12-01 (generic queue).** Boundary: the two telemetry files plus the test `#[path]` lists. Interface: `Queue<T, N>` with two aliases. Migration: tree frozen during `scripts/check`. Validation: the collapsed queue tests, `queue_bytes` on target. Deferred: `LossReceipt` and the exporter edit in `stream.rs`.
6. **SIFT-S07-01 (`StreamRequest`).** Boundary: `stream_http.rs`. Interface: `parse(line)` with the three old functions as accessors. Migration: none. Validation: existing route tests and the fuzz oracle through the new parse. Deferred: moving `answer`, `admit` and `admit_request` onto the type.
8. **SIFT-S10-01 (dead session layer).** Boundary: `core/src/sonos.rs:173-233, 350-468` and the four firmware test-only helpers with their tests. Interface: none removed that production uses. Migration: none. Validation: `scripts/check`, firmware build, and a `cargo +esp build --examples` (the check script builds none). Deferred: the fixture's fate (delete or `sonos_fixture.rs`) and the orphaned `sonos-roots.pem`, needing Doctor Biz's permission either way.

## 5. Explicit skips

- **S06 Network lifecycle.** Inspected: all four owned files, the nine `network_lifecycle` unit tests, `net_policy.rs`, `network_observation.rs`, `stream_network_query.rs`, every consumer of `Network`/`Lease` (stream admission rebinds on lease change; stream paths bail on `!is_current`; Sonos captures a lease per queued command; OTA reads only "has an address"; device metrics treats `None` as unavailable evidence), the b/g/n readback and office-network gotchas. Reason: `Network` is one `Arc<Mutex<Snapshot>>` with a CAS-style `observe` so a disconnect event racing the poll always wins; `Lease` is the only currency; `is_current` is strict equality; every consumer agrees on its meaning. The reconnect supervisor is 35 lines with one `Option<Instant>` and two bools whose only impossible pair is unreachable by construction. Sub-materiality: `supervise` seeds `up` from `is_up()` but updates it from `address.is_some()` (one spurious "disconnected" line on a 0.0.0.0 boot); the `u32::MAX` exhaustion sentinel is unreachable in practice but tested. Cross-subsystem: `NetworkMetadata` is assembled in `firmware/src/sonos.rs:895-902` because the Sonos handle happens to hold a `Network` clone (owner S12 or S06); `core/src/sonos_queue.rs:13-14` stores `host` and `network_generation` as two fields and rebuilds a `Lease` at `sonos.rs:559-562` (owner S11).
- **X1 Vendored fingerprinting.** `core/src/fingerprinting/*` is copied from songrec-lib and kept byte-identical for the golden tests (`core/src/lib.rs:13-22`); the repo owns no modification beyond attribution. Skip.
- **X2 Vendored Shine.** `mp3/vendor/shine` is upstream C; the repo owns only the wrapper (`mp3/src`), which S09 audited. Skip.
- **X3 Host harness scripts.** `scripts/*` are host tooling outside "this firmware"; they were read only as consumers of the `/api/metrics`, `/api/ota` and UART contracts. Skip by scope.
- **X4 Mac CLI.** `tools/src/main.rs` is a consumer of `core::shazam`; read as a call site for S02. Skip by scope.
- **X5 Fuzz targets.** `fuzz/` is a harness; its oracle (`fuzz/src/lib.rs`) is evidence for SIFT-S07-01. Skip by scope.
- **X6 Diagnostic example binaries.** `firmware/examples/*` are one-off hardware diagnostics not in the production image; `sonos-check.rs` is named in SIFT-S10-01 as a touch point. Skip by scope.

## 6. Cross-cutting patterns

- **Hand-rolled lock-free encodings decoded in several places** (SIFT-S08-01, SIFT-S08-02, SIFT-S01-01). Three subsystems store a fact in an atomic word and let readers re-derive its meaning with a literal or a shift. Each fix is local; there is no shared abstraction, only the same habit. Worth one line in `gotchas.md`: a value published through an atomic gets its encode and decode in one `impl`.
- **Copies that drifted** (SIFT-S03-01, SIFT-S12-01, SIFT-S07-01, SIFT-S09-01). All four have a git history of "needed a second one, copied the first", and in three the copies already disagree. They do not share an abstraction; each wants its own small authority. Sequencing: none between them.
- **Authority split across the Sonos stack** (SIFT-S11-02, SIFT-S11-01, the thrice-guarded `buffer_available`, and the demoted autoplay accessor). One source-of-truth problem: admission and invariants are enforced wherever a handler happened to be written. Sequencing: SIFT-S10-01 first (removes the dead layer from the file), then SIFT-S11-02 and SIFT-S11-01 in either order (they do not overlap).
- **Options and tuples standing in for enums** (SIFT-S02-01, SIFT-S04-01, SIFT-S11-01, SIFT-S05-01). Four subsystems let a pair of values disagree; only OTA's does so in practice. Independent fixes.
- **Firmware-only modules without host tests** (`ota.rs`, the Sonos `device` module, the `web.rs` handlers, `input_gain::Handle`). Not a representation problem, but it sets the migration risk for SIFT-S05-01/02, S11-01, S11-02 and S04-01, which is why each of them names a board check (SIFT-S11-02's `Handle::submit` has no host test at all). The repo's convention of `#[path]`-including firmware files into `core` tests (34 test files) is what makes the rest host-testable; extending it to extracted transition functions is the cheapest way to lower that risk (SIFT-S11-01's validation plan).
- **Telemetry code living in S07's file.** `firmware/src/stream.rs:898-1110` (the encoder worker) and `:1548-1720` (telemetry renderers) belong to S08 and S12 by the ledger's own boundaries; about 500 of the file's 1,720 lines. Not a finding on its own (moving code is not simplification), but SIFT-S12-01 edits the exporter half and is the natural moment to relocate it.

## 7. Rejected, merged, and superseded candidates

| Candidate | Disposition | Reason | Authoritative finding/subsystem, if any |
|---|---|---|---|
| S10: `SonosConfig::autoplay()` accessor as the one "autoplay implies enabled" rule (`core/src/sonos.rs:31-50`; raw reads at `firmware/src/sonos.rs:265, 911, 958`; `effective()` only at `:1016`) | demoted | The invalid pair `{enabled: false, autoplay: true}` is closed today by `effective()` at load, `SetEnabled(false)` clearing it, and the web gate; the accessor leaves it representable and persistable and keeps all three guards. Speculative as a finding; folded into SIFT-S11-02's "while in the file" | SIFT-S11-02 |
| S04: express the cfg gain fallback in dB (`config.rs:14-16`, `input_gain.rs:22` silent `unwrap_or(ZERO)`, `core/src/input_gain.rs:60-63`) | demoted | A `u8` of dB admits 256 values of which 9 are valid, exactly as the register byte does; renaming the key only moves the validator. The 3-line fix (warn when `from_register` fails, as the NVS branch at `:24-30` already does) is a bug fix worth doing, not a representation change | S04 |
| S11: remove the WSS/TLS fixture and test-only helpers from `firmware/src/sonos.rs` | merged | Same dead layer as S10's candidate, in the other crate; one change, one permission | SIFT-S10-01 |
| S13: export the modules the diagnostic examples need instead of 13 `#[path]` re-includes (duplicate statics such as `DRIVER_DISCONNECT_COUNT` in one image, a hand-written `recognizer` shim in `tls-capture.rs:25-28`, two routes to `check_transport`, no `--examples` build in `scripts/check`) | demoted | Real and cheap (about 60 lines of visibility), but the benefit lands in the diagnostic example binaries, which are outside the requested scope. Do it when an example is next touched, and add `cargo +esp build --examples` to the check | S13 |
| S01: retire the mono `LevelMeter` and `stereo_to_mono` (`core/src/audio.rs:183-238, 382-390`, no callers) | demoted | Dead code with no caller; a deletion, not a representation change. A standalone removal (`core/src/audio.rs` is not in SIFT-S01-01's scope) | S01 |
| S01: `CaptureHealth` `restarting`/`reboot` bools → enum | rejected | One unreachable combination, 60 lines, fully tested | S01 |
| S01: gap fan-out in `capture()` resets nine things | rejected | A method would only move the list; no evidence of a missed reset | S01 |
| S02: `Policy` `loud: bool` beside `loud_since_ms` | rejected | Derived boolean in a 200-line module with 20 table tests; no defect evidence | S02 |
| S02: admission magic numbers (`* 16`, `46 * 128`) | rejected | Constant sharing, not a model change; inputs are fixed by Shazam and the pinned fingerprinter | S02 |
| S02: move the 200/429/other → `Outcome` mapping into core | rejected | A testability gain, not a simplification | S02 |
| S03: fold `shazam_enabled`/`state`/`now` into a tagged union | rejected | Contract migration for a glitch bounded to one 250 ms tick; the page already treats `shazam_enabled` as overriding | S03 |
| S03: `app.js` action→config map written twice | rejected (below gate) | Five-entry literal; worth doing only while in the file | S03 |
| S04: unify the scalar NVS setting and the Sonos JSON blob | rejected | Different kinds, owners and lifetimes; a shared blob would couple codec bring-up to the Sonos decoder | S04 |
| S04: policy defaults duplicated between `config.rs` and `PolicyConfig::default()` | rejected | `toml-cfg` requires literal attribute defaults; no representation removes it | S04 |
| S07: `Route::is_compressed()` for the predicate written twice in `Runtime::admit` | rejected (below gate) | Two lines; falls out of SIFT-S07-01 | S07 |
| S08: pump restart duplication, `pending_source` derived state, `stream_limits.rs` forwarding-only, `SwitchStart`/`SwitchPoll` twins | rejected (below gate) | Each ~10-15 lines with consistent effect today | S08 |
| S09: a trait over the FLAC and MP3 encoders | rejected | Would un-fuse the FLAC loop and hide real contract differences; the enum dispatch is the honest model | S09 |
| S09: `Mp3Encoder.codec: Option<..>` after a failed reset | rejected | Every call site returns `Stopped` on reset error; the alternative is what the comment avoids | S09 |
| S09: MP3 frame-geometry literals in four places | rejected | `core` cannot depend on the C crate; the const assert is the cheapest sound guard | S09 |
| S10: merge `sonos_web::Action` and `Command` (eight identical variants) | rejected (below gate) | Compiler-checked and exhaustive; fold only if S03-02 merges them anyway | S03/S10 |
| S10: shared quoted/escaped/depth JSON pre-scan in `parse_message` and `validate_time_played` | rejected | They count different things; ~20 lines saved for an indirection | S10 |
| S11: `Commands.sender` 3-tuple; `QueueState { snapshot, intent }` `(Some, None)` | rejected | Deliberate token-as-mutex design with a test; the second is a two-field instance of S11-01 | S11 |
| S12: collapse the `device_metrics` projection layer onto the UART record types | rejected | The duplication is the versioned JSON schema boundary (`schema_version: 1`, body-size test, field allowlists) | S12 |
| S12: `runtime_valid: bool` beside `task_runtime_us: Option<u32>` | rejected | Unreachable at the source; removing it is a schema bump for one field | S12 |
| S12: unify the other bounded stores (`Bank<N>`, `Events<T,N>`, `Registry`, `Cache`, `Diagnostic`, `Handoff`) | rejected | Different ownership and lifecycle models; a trait would hide the differences | S12 |
| S06: `supervise` seeds `up` from `is_up()` but updates from the address | rejected (below gate) | One spurious log line on a 0.0.0.0 boot; cosmetic | S06 |

## 8. Audit-of-audit results

Two fresh reviewers with no part in the subsystem reviews read the draft and the cited code.

- **Coverage and missing boundaries.** Found: `core/src/signal.rs`, `core/tests/support/*`, `core/src/lib.rs`, `core/Cargo.toml`, `mp3/build.rs`, `mp3/Cargo.toml`, `firmware/.cargo/config.toml`, `firmware/components_esp32.lock` and `firmware/assets/sonos-roots.pem` were in no row; `firmware/bindings/*.h` and `sdkconfig.diagnostics.defaults` sat in S13 though they serve only the excluded examples; `firmware/src/capture_fixture/events.rs` was unlisted; the cargo features `ota-rollback-probe`, `admission-resources` and `listener-loom` had no owner; several persisted formats (NVS `input`, NVS `sonos/config`, the `/api/sonos` `Snapshot` JSON, the `capture:` UART line, the Shazam fixture JSON) had no named owner; nine test files matched two rows' globs. Resolved: row T1 added, X6 widened, S13/S09/S12/S05/S07/S04/S10/S11/S01/S02 rows made explicit, globs replaced by explicit lists, the orphaned root bundle added to SIFT-S10-01's scope.
- **Duplication and ownership overlap.** Found: four finding pairs edit the same function without saying so (S03-01/S04-01 in `web.rs:267-307`; S07-01/S08-01 in `Runtime::admit`; S05-01/S05-02/S13-01 in `mark_valid_when_healthy`; S03-02/S10-02 on the autoplay rule); S03-02 was owned by S03 but lands in S11's code; S10-01 deletes more lines in S11's file than in S10's. Resolved: prerequisites added to the table and the schemas; S03-02 renamed SIFT-S11-02 under S11; S10-01 kept in S10 with the reason stated (the session types it removes are defined in core); blast radius corrected on S09-01 and S13-01.
- **Materiality and over-abstraction.** Verdicts on the 18 drafted findings: 16 pass, 2 fail as speculative. SIFT-S10-02 (the autoplay accessor) leaves the invalid pair representable and all three guards in place; demoted into SIFT-S11-02. SIFT-S04-02 (cfg gain in dB) relocates the validator without shrinking the invalid range; demoted to a 3-line warning fix. SIFT-S07-02's lock-traffic argument was struck as unmeasured and its impact lowered to low; it stands on the precedence-authority argument alone. SIFT-S01-01's claim that JSON would gain fields was struck as contradicting the rejected projection-collapse; the JSON contract stays unchanged. SIFT-S03-01's "64 vs 512 bytes" was removed from the drift list (per-endpoint by design). SIFT-S13-01's ladder asserts are marked the optional half. No rejected candidate was promoted.
- **Schema completeness.** All 24 fields present on every finding. Two thin fields were strengthened: SIFT-S11-02's existing validation now says `Handle::submit` has no host test; SIFT-S12-01's risks now record `invalidate()`'s second caller (`encoder_telemetry.rs:285`).
- **Dependency-aware ranking.** Corrections applied: SIFT-S07-01 above SIFT-S12-01 (same effort, the admission path over a 10 s telemetry tick); SIFT-S11-02 above SIFT-S11-01 (medium versus large effort, no overlap, same hardware gate); SIFT-S07-02 down to 13; SIFT-S13-01's prerequisite reworded (nothing in S05-01/02 picks a core or priority; the point is editing `ota.rs` once).
- **Citation check.** Four spot-checked findings had line numbers off by one to seven lines (SIFT-S09-01, S04-01, S07-01, S02-01); all corrected from the coordinator's own re-read. SIFT-S11-01's reviewer citations were about 60 lines high and were replaced during coordinator verification before the draft.

## 9. Repository integrity

- **Baseline status** (captured before any review): branch `wip/kata-capture-protection` at `49ad53edf876c6ef3c7ae79ce6acb9e3bf53a4ac`; `git status --porcelain=v1 --untracked-files=all` listed five untracked paths and no modifications: `.kata.toml`, `docs-ignore/victrola-hacks.tgz`, `docs/superpowers/plans/2026-10-10-device-metrics.md`, `kata-export-20261011T161210Z.jsonl`, `output.txt`.
- **Final status:** captured 2026-10-22 after the audit-of-audit and before this file was written: HEAD `49ad53edf876c6ef3c7ae79ce6acb9e3bf53a4ac` on `wip/kata-capture-protection`; `git status --porcelain=v1 --untracked-files=all` lists the same five untracked paths and no modified tracked files; `git diff --stat` is empty.
- **Comparison:** unchanged
- **Integrity verdict:** pass
- **Post-audit writes:** `docs/sift-audit-2026-10-22.md` (this report), written after the comparison above. Nothing was committed, staged or pushed.
