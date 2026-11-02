# Reliability first batch

The validator now counts actual decoded samples, and the bounded hardware run
passed with two listeners. Startup capture remains a separate unresolved fault.

Implementation: `ed5e26a` fixes decoder validation, the Wi-Fi baseline, the FLAC
benchmark assertion, private-media exclusion and fixture rollback. `350e497`
retains Sonos startup resources after failed thread creation and distinguishes a
successful launch from a later pthread-configuration cleanup error. Both changes
received independent Sol review and a parent review. Nothing was pushed or merged.

## Host verification

`scripts/check` passed all 15 stages: formatting, strict Clippy, 279 Rust tests
(two existing ignored tests), UI/harness tests, both new regression suites, and
the pinned ESP-IDF firmware build without project warnings. The FLAC cost example
also built explicitly. The 12 decoder tests use real FFmpeg fixtures; the six
fixture tests use real local files and subprocesses without calling Shazam.

Negative controls cover omitted, duplicated, corrupted and CRC-damaged blocks,
plus a valid prefix missing its final block. Review added regressions for the
last decoder output arriving between polls and fractional-duration rounding.
WAV24 sample hashes preserve low bits and channel order. Sonos tests exercise
real rejected thread creation, later real command delivery, retained storage,
concurrent callers, autoplay startup errors and manual Stop.

## Hardware evidence

The real ESP32 completed zero, DC, mixed and noise benchmark cases. Zero/DC frames
were 14 bytes; all frame-bound and output-storage assertions passed. Production
was restored before streaming validation.

The two-listener run requested 30 seconds and decoded exactly **1,440,000 stereo
sample frames per listener**, with no reported skipped/duplicate frames or new
capture faults after the stable baseline. Wi-Fi readback was protocol `0x07`,
power saving `0`, channel 6, RSSI -48 dBm and negotiated PHY 4 (HT20 in the pinned
IDF). Sonos remained stopped with autoplay disabled.

After flashing the final Sonos image, two real discovery requests each returned
HTTP 202, entered discovery and returned to stopped with a discovered group.
Saved configuration matched the pre-flash state exactly. This verifies ordinary
device startup and command handling; failed thread creation/retry was tested on
the host, without inducing IDF memory pressure. No speaker settings changed.

The first capture telemetry already reported **15 queue overflows, 480 dropped
frames, two restarts and two discontinuities**. Those counters did not increase
during the measured interval. This run does not establish clean startup or
explain the faults. It also does not establish analog fidelity, a transport root
cause, persistence after diagnostic radio settings, forced reconnect recovery,
or long-duration reliability.

[Machine-readable evidence](reliability-first-batch.json) records ELF hashes,
reset identity, effective radio settings, sample counts, benchmark measurements
and private raw-log locations. The two-listener evidence precedes the Sonos
startup change; its firmware hash is kept distinct from the final image.

Kata closures: 4dss, r0ve, n97a, 6sw1, ntja and j4ae. hv4x remains open for the
unobserved radio acceptance above. Capture restart evidence was added to ed4k.
