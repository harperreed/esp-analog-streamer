# Runtime MP3/FLAC settings — October 9, 2026

The unified firmware passed short feature acceptance on the ESP32: saved format
and buffer selection, real browser use, reboot persistence, immutable Sonos queue
metadata, Stop cancellation, and retry while retired listeners held both slots.
This does not clear the existing sustained-delivery failures. An earlier run of
the same image failed its High-buffer MP3 decode check; retain both outcomes.

Machine-readable results: [runtime-codec.json](runtime-codec.json).
Kata `ckcc` covers this feature; parent `e8hd` retains reliability work.

## Build and setup

Source base `a45d881` plus the runtime-codec change on
`wip/kata-capture-protection`. All 20 canonical `scripts/check` stages passed,
including host tests, Clippy, firmware build, warning checks and I2S IRAM checks.
Independent Sol review closed cancellation, capacity-retry, recognition-mode UI,
and browser acceptance-deadline findings. Parent fresh-eyes review corrected a
stale build-time comment; no executable source changed after the tested build.

ELF SHA-256:
`c3bf69b149b9386b27346cfe26e023c191054a490374a7d5f480c9b8d40159d9`.
The first run flashed that artifact; the second used the same deployed image.
This is host artifact provenance, not cryptographic device attestation.

ESP32 rev 3.1, 240 MHz, 4 MB PSRAM at 80 MHz, ESP-IDF 5.5.5. The workstation's
front headphone jack fed LINE IN with the existing independent stereo-noise
fixture. Rear Line Out was muted; gain and Sonos volume were unchanged.
Private evidence is under `target/runtime-codec/hardware/`; run IDs use UTC
(October 10), while this report uses the local October 9 date.

## Retained outcomes

| Check | First run, 03:37:10 UTC | Second run, 03:44:11 UTC |
|---|---|---|
| Live MP3 prefix | 96,000 sample frames/channel | Passed in repeated switches |
| High MP3 prefix | Premature EOF at 67,200 encoded bytes; failed | 96,000 sample frames/channel |
| Codec cycles and High FLAC | Not reached | Eight two-second decodes, each 96,000 sample frames/channel |
| Real Chromium Settings page | Not reached | Passed; no page or request failures |
| Reboot persistence | Not reached | MP3/Medium/autoplay off retained; 96,000-sample decode |
| Actual Sonos queue and Stop→Play | Not reached | Both codec directions passed |
| Occupied-slot cancellation/retry | Not reached | Passed with two old slots demonstrably occupied |

The Sonos test changed format and buffer during playback, checked the old queue
remained unchanged, then used Stop→Play to obtain the new URL and content type.
A final Play immediately followed by Stop remained stopped for 15 seconds.
The browser exercised shipped assets and the real API; host DOM tests remain
unit/integration coverage rather than browser E2E claims.

The occupied-slot test held two MP3 responses unread and received HTTP 503 for
a third listener. Selecting FLAC remained pending. Play committed FLAC; Stop
canceled that Play while old slots remained occupied, confirmed by another 503.
An immediate same-format Play waited and then established the FLAC/Medium queue.
UART independently records the two old writes exhausting 8,016 ms budgets in
8,021 and 8,030 ms, followed by release of their sockets. The JSON field
`http_probe_proves_sender_blocking: false` describes the HTTP-only assertions;
the separate `uart_frame_write_stall_observed` records the additional evidence.
The deadline writer uses nonblocking sends and writable waits; these are stalled
frame writes, not a claim that a libc send call blocked. They do not identify
an RF/AP/TCP failure hop.

Both runs finished stopped with MP3 selected, Medium buffer, autoplay off and
no firmware rollback. The complete run did not change volume.

## Capture, memory and limits

The passing run's two boot baselines recorded 14 and 15 queue overflows,
480 software-dropped sample frames, and two capture restarts/discontinuities.
Every subsequent sampled fault-counter delta was zero within each boot.
This proves no additional reported faults in those windows, not fault-free
startup or exact continuity from the analog jack.

During the second boot, the lowest sampled internal free memory was 18,379 B
and the smallest sampled largest block was 10,752 B. The minimum reported sum
of per-region low-water values was 4,375 B; that is not simultaneous free memory.
PSRAM reached a sampled 343,312 B during overlapping format storage. Encoder
stack headroom stayed at least 5,496 B; the longest observed encode call was
118,643 microseconds. Repeated switches released storage, but this short run
cannot establish a long-term leak bound. Internal heap pressure merits follow-up.

MP3 encoding under noise varied: the first run was about 23 ms/frame, while the
same-image rerun was about 21.8–22.1 ms/frame, against 24 ms of source audio per
frame. Prior prototype noise windows were about 20.9–21.1 ms/frame. These are
separate workload windows, not a controlled performance attribution. PSRAM/cache
placement is a hypothesis; no encoder optimization is justified by these data
alone.

The first High failure and slow writes in the passing High check keep transport
reliability at P1. Two-second decodes prove selected-format decoding and actual
sample counts; they do not prove real-time sustained delivery or sample-exact
MP3 content. No long soak, analog calibration, or successful 24-hour release gate
is claimed. Fix sustained delivery and capture startup before a reliability release.
