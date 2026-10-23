# Local MP3 source rebuild and relink proof

The packaged first-party source builds a complete ESP32 firmware image with
public configuration. A one-line Shine change reaches the final target ELF
and changes the intended bit in actual host-encoded MP3 frames. This completes
the local engineering demonstration; it does not establish a complete release
source bundle or outside-recipient acceptance.

Source revision: `bfea1d40adc401205ac67ec76d9ad411b9f78215`.
Three clean-context GPT-6.1 Sol workers prepared the input inventory, host
probe and independent review. Parent ran both target builds and independently
verified archive/source hashes, actual decodes and linked instructions.

## Package and build boundary

The deterministic archive contains 230 committed files, a generated build
guide and its manifest. Its positive allowlist includes first-party build
sources, locks, public example configuration, licenses/notices and the exact
scripts needed by `scripts/check --firmware`. It excludes private configuration,
recordings, journals, Git history, production binaries and build caches.

Archive SHA-256:
`9f8ca3d84b3d369844800d938e7d596a5e36eeec766c5f2ee2b5cd561158690f`.
Repeated packaging at the explicit source revision produced identical bytes.
Fresh extraction verified every member, mode and hash. Corrupted-manifest and
unsafe-path controls rejected invalid input.

The recipient tree started without a firmware target directory. Its generated
`cfg.toml` has empty Wi-Fi/MQTT settings and recognition disabled. The SDK and
native tools were copied independently; the Python environment was created
there. Installed Rust/mise tools and Cargo caches were reused, with native
dependency downloads allowed. This was neither offline nor archive-only.
SDK commit: `b774170ff46c393eeb5e495ea37936038d3f4f4f` (IDF 5.5.5).
Rust: esp 1.98.1.0; target GCC: esp-14.2.0_20260121.

The first fresh-target build linked and passed all five stages, but its
canonical run `VG4kYsui` **failed** the final input-stability comparison:
creating the SDK Python environment added seven cryptography Cargo manifests.
That failure is retained. With dependency setup complete, unchanged source
passed all five stages and boundary checks in `ZAtxchL1`. No check was removed
or changed. The modified source passed the same scope in `EtYxJOgq`.

| Image | ELF SHA-256 | Source difference |
| --- | --- | --- |
| Public baseline | `871e3fd5321a0ef65edb35f7c003f28d120cc6a26004775e4a273287297bc574` | None from archive |
| Public modified | `f06bf899de36cb9446a2619b8a0de9b49fd972fe2ed4194d0f60dc8507c82637` | `mpeg.ext = 0` becomes `1` |

Cargo/component locks, public configuration and generated SDK configuration
match across the pair. The modified build reran the library build and linked
the full firmware. Neither image was flashed. Production source and the
deployed firmware were not changed.

## Observable library change

The change is in `mp3/vendor/shine/layer3.c`, inside `shine_initialise`.
The production bridge permits this MP3 private header bit. A target-compiled
`offsetof` assertion verifies that `mpeg.ext` lies at byte 88.

The final baseline ELF stores zero at that offset at `0x400844ff`; the modified
ELF stores one at `0x4008450e`. Both disassemblies show the bridge calling that
linked initializer. All 26 linked function symbols with `shine_` or
`shazam_mp3_` prefixes remain in `.iram0.text`. The generated linker script
still selects text and literals from all ten MP3 C members. The canonical I2S
checks pass and `CONFIG_I2S_ISR_IRAM_SAFE=y` remains effective. These are static
placement checks, not target runtime or allocation-capability tests.

The real host C bridge and nine Shine units encoded 96 stereo frames in each
state. Every 960-byte frame differs only at byte 2, bit 0. FFmpeg decoded
110,592 stereo sample frames from each file, with identical PCM and no errors.
Unchanged-library and extra-payload-change negative controls both rejected.
The decoded PCM SHA-256 is
`2f7e977fc1490e8f2b72bbdba75edce5b67757512dea18bc8ccf2d0c9f5f33cc`.

The changed bit intentionally changes exact-output golden bytes. This probe
does not claim the modified library passes the unmodified host golden suite;
the measured checks are the explicit host comparison and firmware-only scope.

## Receipts and limits

Scripts, manifests, public settings, patch, build receipts, disassemblies and
independent review are archived in [mp3-relink-probe](mp3-relink-probe/README.md).
Large raw logs, source archive and public proof ELFs remain under
`target/kata-hardening/oct13-relink-{inputs,parent,recipe}/`.
The initial Git import exposed existing whitespace in upstream source and
licenses; those exact bytes were preserved. A collector's assumed count of 27
MP3 function symbols failed; the corrected check compares the observed sets
of 26 and requires the relevant entry points. Both limitations are recorded.

This proves local source modification and full target relinking using external
tools/dependencies. It does not prove correspondence to a credential-bearing
production binary, a complete corresponding-source distribution, offline or
byte-reproducible builds, outside-recipient usability, or license compatibility
of the complete firmware. Resolve those distribution requirements before
shipping binaries. No license conversion or publication occurred.

e8hd remains open for its broader playback recovery, listening/physical-source,
delivery/endurance and distribution gates. No whole Kata child closes:
wmsy remains 23 of 38 closed, with 15 open.
