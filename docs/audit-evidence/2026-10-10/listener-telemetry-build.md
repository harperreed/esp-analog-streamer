# Listener telemetry build checkpoint

The listener-only change after `0ef0e3e` moves admitted join, timeout, refusal,
departure and successful-close reporting into a fixed eight-record queue.
Producers never wait for the exporter. The existing metrics task removes at most
two records before formatting them. Socket close, Slot/Lease release and genuine
close-failure restart retain their ownership rules. Encoder/probe production
timing and playback settings are unchanged.

Independent review found three harness acceptance defects: a stale UART snapshot,
malformed receipts, and raw close waits bypassing batch completion. All three
have failing regressions and reviewed fixes. Final canonical verification passed
all 27 stages, including the target build, IRAM check and warning check. The final
harness run passed 25 probe, four noise and 18 module tests. Real ownership tests
exercise full/contended/poisoned storage, stalled export, TCP timeout, delayed
close, refusal bytes and retained failure authority. Host execution does not
prove ESP-IDF logger behavior or device headroom.

| Image | SHA256 |
| --- | --- |
| Default | `479e0af815f7509b87f960afb2874530eaf110af0e2869959fab3cb441af636d` |
| Encoder probe | `5956e4cb63623ff773f4a8c544e0321622939c2bd67f64d5c662ab38e123c3a5` |

The separate probe build and its IRAM check passed; the latter inspected 10,815
linked symbols. `shazam_mp3_receipt` remains absent from default and present in
probe. Both saved ELFs identify build artifacts, not device attestation.

The ESP32 queue symbol occupies 2,008 bytes in internal DRAM data. Against the
saved normal control, DRAM data grows from 19,826 to 21,826 bytes and BSS from
22,144 to 22,152 bytes: a net 2,008-byte section increase. This section comparison
includes linker/layout effects; it is not an isolated heap allocation estimate.
The Linux host queue is 2,016 bytes and each record 248 bytes. Device stack minima,
live heap/largest blocks, export loss and actual listener reuse remain unmeasured
at this checkpoint. No new image has been flashed yet.

Local evidence: `target/kata-hardening/listener-telemetry-device/` retains both
ELFs, symbols, section listings and build receipt; `check-listener-telemetry-final.log`
records canonical verification. The implementation and independent task/fix
reviews are `listener-telemetry-implementation.md`, `listener-telemetry-task-review.md`
and `listener-telemetry-fix1-review.md` in the same hardening directory.

One separate build login shell emitted a pre-existing missing Homebrew path
warning from the machine profile. Cargo exited zero; later commands bypassed
that login profile. No machine configuration was changed.

Keep full 21tp open for encoder reporting and measured device gates. This change
does not establish the cause of MP3 delivery failures or TLS capture disruption.
