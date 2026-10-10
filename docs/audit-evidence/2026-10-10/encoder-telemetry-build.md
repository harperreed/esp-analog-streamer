# Encoder and capture telemetry build checkpoint

Source base 7caffe2 plus the reviewed integration committed with this receipt.
No new device trial has run at this checkpoint. Kata21tp and delivery remain open.

Encoder logging now uses a two-record numeric queue consumed by metrics. Capture
restart warnings use two cumulative counters with independently sampled signed
error codes. Capture reports bound their counter observations with producer-clock
timestamps. Parsers require paired identities, complete lifecycle and capture
coverage, reject export loss, and retain the original raw-loss negative fixture.
Both ESP lazy mutexes initialize before capture/metrics/stream startup. Existing
codec failure formatting remains outside this extraction.

## Verification

- scripts/check: all27 stages pass; log target/kata-hardening/check-encoder-telemetry.log.
- Final parser refinement: scripts/check-harnesses passes again; encoder-telemetry-final-harnesses.log.
- Host suites: default443 and probe465 passed, each with two existing ignored tests. Both cfg Clippy checks pass with warnings denied; formatting/diff checks pass.
- Parser/shared tests52 pass, including33 parser tests. Capture handoff5 and listener initialization suite16 pass.
- Independent Rust, parser, capture wiring and lazy-mutex initialization reviews clear. Parent final fresh-eyes found no additional code defect; corrected a plan claim about target atomic retries.
- Default and encoder-probe release builds pass; IRAM checks inspect10826 and10841 symbols respectively. Receipt query symbol shazam_mp3_receipt is absent in default and present in probe.

Scratch reports/logs live under target/kata-hardening. Preserve the initial failed
host logs: their eight-turn test bound was too small for the real16-frame raw
fixture to produce1152-frame MP3 output. A96-turn bound fixes only the test.
Parser review reproduced and fixed false passes for generic loss/stopped
boundaries, mismatched lifecycle identities and malformed storage fields.

## Exact images

| Variant | Saved image under target/kata-hardening/encoder-telemetry-device | SHA256 |
| --- | --- | --- |
| Default | default.elf | 1eca73c09675d41d74a2c1d5b76477f20871548a6a9b1f50014576a938b19f71 |
| Probe | probe.elf | d3d435b39c98991c9c4715f5abf35b850abeae0ab3fcc18332274e6093416653 |

The canonical default output matches the saved default hash. Linked inspection
used separate copies and verified executable sections and original file hashes.
No runtime device-image attestation is claimed.

## Measured static cost and remaining gates

Listener queue stays2024 bytes. Encoder queue adds440 default/632 probe; capture
handoff occupies16 BSS bytes. Initialized data rises448/640; BSS rises24. This
excludes mutex setup heap allocations. Metrics entry frame rises208 to224;
log_capture528 to576. Largest inspected nested application formatting path is
1232 default and1360 probe, versus1216 baseline. These omit logger/libc/formatter
callbacks and spills: they are not complete stack bounds.

Capture stores/loads lower directly; increments use local CAS retry loops. The
record operation has no fixed instruction bound. Both queue initialization calls
precede spawn_capture in the linked run function and use the same statics as
producer/exporter paths. Sequential exporter returns were inspected.

Severity: runtime stack headroom and export loss remain acceptance blockers.
Exercise default listeners and probe Storage/Window paths on their exact images
with unchanged4KiB metrics stack, source/settings controls, capture/raw/reset
checks, heap and stack measurements, and verified cleanup. Rare driver-failure
branches have host seam coverage, not an induced hardware-failure pass. The
prior bb56186 device remains the last tested image at this checkpoint; its failed
delivery and full listener queue rejection are not cleared by this source work.
