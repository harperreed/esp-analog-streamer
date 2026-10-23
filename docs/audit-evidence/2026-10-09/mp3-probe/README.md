# Disposable MP3 benchmark sources

These are the experiment sources behind [the benchmark](../mp3-benchmark.md),
not a supported MP3 feature or a new production check suite. They preserve
measured code, including the original pacing variants. They are intentionally
outside firmware examples and production manifests. Do not run the hardware
runners as generic tests: they flash a specific board, reset it, start a physical
analog source, and restore a saved ELF.

## Layout and prerequisites

Use production source `9550f65`. Copy this directory's `host/`, `device/` and
top-level Python files into `target/mp3-benchmark/` at the repository root.
Paths in these historical probes assume that layout and this workstation;
adjusting paths for another machine creates a new experiment receipt. The
standalone FLAC Cargo manifest points to the repository core crate.

Clone `https://github.com/toots/shine` into
`target/mp3-benchmark/source/shine` and check out
`ab5e3526b64af1a2eaa43aa6f441a7312e013519`. Obtain the pinned project tools through
`mise`, the installed Espressif Rust environment, FFmpeg, and Python via `uv`.
The third-party source and audio are not vendored here. See the host README for
host commands and fixture requirements.

Keep audio, private firmware configuration, certificates/keys, full UART output
and ELFs under ignored `target/`. The committed JSON contains measurements and
hashes; filtered measurement lines retain the relevant raw telemetry without
network identities. `source-sha256.json` identifies archived probe source bytes.
Derived results carry their own measured-artifact hashes.

## Board build

Run the canonical production build first. Locate its actual cached IDF
`compile_commands.json` under `firmware/target/xtensa-esp32-espidf/release/build/`.
From the repository root:

```sh
uv run --no-project python target/mp3-benchmark/device/compile-probe.py \
  --commands PATH_TO_ACTUAL_IDF_COMPILE_COMMANDS_JSON
```

The script derives the compiler, ABI and include flags from the `heap_caps.c`
entry, replaces optimization flags with `-O3`, adds `-fsigned-char`, compiles
Shine plus the diagnostic allocator/timer wrapper, and writes a compile receipt.
The signed-char contract matters for this encoder on Xtensa.

Temporarily copy `device/mp3-cost.rs` to `firmware/examples/mp3-cost.rs`.
From `firmware/`, after loading the existing Espressif toolchain:

```sh
cargo +esp rustc --release --example mp3-cost -- \
  -L native=/ABSOLUTE/REPO/target/mp3-benchmark/device
```

The load examples additionally require `--features sonos-fixture` and build-time
`SONOS_FIXTURE_URI=wss://WORKSTATION_IP:9443/websocket/api` plus
`SONOS_FIXTURE_CA=/ABSOLUTE/PATH/TO/CERTIFICATE.pem`. Use the existing
`scripts/tls-capture-peer` fixture with a matching valid IP SAN certificate.
The test connects to that controlled peer, not a Sonos player. Keep autoplay
disabled and the current saved destination/preset unchanged.

`mp3-load-original.rs` is the actual mixed-load source; copy it as
`firmware/examples/mp3-load.rs` to reproduce that run. The corrected
`mp3-load.rs` is archived for follow-up but **was not measured on hardware**.
Likewise `mp3-capture-original.rs` is the first exclusive run; the corrected
`mp3-capture.rs` produced the explicit-yield repeat. Remove temporary example
files after saving the built ELFs and their hashes.

## Hardware runners and cleanup

Before any flash, save a known production ELF as
`target/mp3-benchmark/hardware/production-before.elf` and verify its SHA256.
Record the API configuration and actual analog routing. The archived runners
assume `/dev/ttyUSB0`, board `192.168.23.217`, workstation `192.168.23.153`,
the existing 4 MB partition table and 80 MHz flash. Inspect these paths and
addresses before reuse. Each run reuses its output directory, so preserve a
completed run before repeating it.

The load runner needs `target/tls-capture/peer-cert.pem` and `peer-key.pem`, and
`target/flac-buffer/hardware/stereo-noise.wav`. Our cable used the front
Headphone jack (`plughw:CARD=PCH,DEV=0`), with rear Line Out muted and Master/PCM
at unity gain. Verify that routing before starting the physical source.

Run from the repository root with `ESPFLASH_BAUD=460800`:

```sh
uv run --no-project --with pyserial python target/mp3-benchmark/run-device.py
uv run --no-project --with pyserial python target/mp3-benchmark/run-load.py
uv run --no-project --with pyserial python target/mp3-benchmark/run-load.py --exclusive
uv run --no-project --with pyserial python target/mp3-benchmark/run-load.py --exclusive --yield-repeat
```

Run only the command matching the currently built example. `--yield-repeat`
selects a separate output directory; it does not select or rebuild firmware.
The mixed runner starts two real FLAC decoders; exclusive runners expose no
audio HTTP stream. All MP3 input is generated PCM. The load runner bounds the
experiment, stops its source/clients/peer and restores production in nested
cleanup. Hardware disconnection or failed flashing can still prevent restore;
inspect the restore log, boot hash and live API rather than relying on `finally`.

Afterward, verify stopped/autoplay false/Medium and unchanged destination,
workstation PCM closed, rear Line Out muted, and no test source left running.
Do not reuse legacy listening scripts that write Sonos volume.

## Validation and limits

`check-saved-decoders.py --self-test` reproduces the original final-read backlog
failure and checks omission/duplication negative controls against the actual
production decoder validator. Its historical self-test requires the private
`target/flac-buffer/hardware/music/20261008T234644.884950Z/listener-1.framehash`
120-second capture; it is not a standalone generated fixture. The archived load runner contains the subsequent
incremental-drain correction. The original failed verdict remains in the device
receipt alongside independent verification of the completed files.

The original pacing sources use sub-tick sleeps that busy-wait on pinned IDF.
Use the corrected explicit `vTaskDelay(1)` source for further measurements.
Preserve original timings; do not silently replace historical sources with fixes.
No probe establishes MP3-over-Wi-Fi continuity, Sonos support in this application,
complete capture-to-decoder sample accounting, a worst-case encoding bound,
perceived fidelity, or license approval.
