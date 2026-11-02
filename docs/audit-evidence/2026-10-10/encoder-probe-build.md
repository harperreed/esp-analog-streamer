# Encoder boundary diagnostic build

The diagnostic accompanying this receipt is disabled by default. Independent
source review, normal and feature host tests, scoped Clippy and all 27 canonical
stages passed. The explicit ESP32 feature build and its I2S IRAM check also passed.
No image in this receipt has been flashed or measured on the device yet.

| Image | Saved ELF | SHA256 |
| --- | --- | --- |
| Default | `target/kata-hardening/encoder-probe-device/default.elf` | `9894ceef57743a15b000f88a79eda35ad168753943118d1f614532f96b7efa11` |
| Probe | `target/kata-hardening/encoder-probe-device/probe.elf` | `d5607b7f1e513987dd6031f9130d36fd27f044ad66e33c8b94032fa48752d66e` |

Both builds used the existing pinned ESP environment. The diagnostic used
`cargo +esp build --release --features encoder-probe`. `readelf --wide --syms`
finds `shazam_mp3_receipt` only in the diagnostic ELF; its symbol is absent from
the default ELF. Separate host archive checks established the same gating.

The feature observes actual read, conversion, codec, publication and task-delay
wall time. It queries the two live Shine allocations without allocating or
changing output. One image runs OFF1/ON/OFF2, discarding two windows and retaining
six in each phase. Normal firmware keeps its original report/delay ordering.
The diagnostic records delay before each report. Neither build changes codec,
buffer sizes, priorities, yield cadence or allocation policy.

Tests exercise real codecs and tracking allocators, exact ON/OFF clock calls,
storage/output stability, shared report/delay order, phase boundaries, checked
arithmetic and reset state. A failing test caught an invalid interval that could
otherwise allow a later valid completion; invalidity now persists until a new
sequence. Failed initialization ownership tests use the real C codec. Supported
capture/listener Loom feature combinations were checked separately; combining
both model features is intentionally rejected by existing code.

Logs: `target/kata-hardening/check-encoder-probe.log`,
`probe-firmware-feature.log`, `probe-feature-iram.log`,
`probe-{default,feature}-green.log` and `probe-clippy-green.log`.

## Remaining acceptance

P1: the underlying MP3 throughput regression remains unresolved. The first phase
needs 80 seconds; earlier raw loss may invalidate it before completion. The
device trial must have a fixed deadline, preserve failed sequences and never
count restarted intervals as a successful continuous run. Source audio must be
settled before reset, with enough duration for collection and final receipts.

Host tests do not execute ESP-IDF metric snapshot/reset plumbing or force the
fixed production adapter's reset allocation to fail. Source review covers those
paths; this is not an executed end-to-end reset-failure test. Timer overhead,
task scheduling and same-image performance require device evidence. Address and
wall-time receipts alone establish neither cache causality nor exclusive codec
CPU cost. No performance or release-readiness claim follows from these builds.
