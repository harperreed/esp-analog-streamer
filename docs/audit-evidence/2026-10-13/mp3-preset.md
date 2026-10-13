# MP3 buffer admission on the device

The bounded e8hd preset-offset gate passes. One diagnostic boot served three
sequential debug clients without Sonos playback or settings writes. Each actual
admission selected the intended history, and each retained prefix independently
decoded to 73,728 stereo sample frames (1.536 seconds). This completes one named
acceptance gate; e8hd and wmsy remain open.

| Preset | Head W | Cursor C | First delivered F | W−C = W−F+1 | History |
| --- | ---: | ---: | ---: | ---: | ---: |
| Low | 3582 | 3540 | 3541 | 42 frames | 1.008 s |
| Medium | 3684 | 3517 | 3518 | 167 frames | 4.008 s |
| High | 3768 | 3434 | 3435 | 334 frames | 8.016 s |

A temporary 15-line probe captures head, generation, selected cursor and time
under the existing ring lock, then logs after release. The first successfully
written frame uses a one-based receipt sequence; this explains the +1. The probe
changes neither cursor selection nor payload. Its logging and fixed listener
field can affect scheduling and stack use; this is not an overhead benchmark.
The lowest observed listener stack headroom was 668 bytes, with no observed
panic or watchdog. The probe was removed after this measurement.

Actual peer/port and owner IDs join each admission to its stream, delivered
range and checked close. All generations stayed zero. Firmware recorded
67/70/72 fully written frames; the host intentionally retained only the first
64 of each. Socket acceptance beyond that prefix is not proof of host receipt.
Loss-free event exports cover each close, and fresh metrics show idle slots.
The close message reports occupancy before releasing its own slot: one held.

The complete 150-second UART acquisition contains 14 capture windows. Startup
and steady capture gates pass: no post-baseline overflow, drop, read error,
restart or discontinuity; no raw encoder overrun/reset. Eight queue overflows
preceded the startup baseline, and the ordinary startup settling discarded
24,000 frames. Neither is silently counted as zero. The acquisition and cleanup
passed on their first live attempt; original earlier overlap failures remain
unchanged and their cause is not established by this result.

FFmpeg consumed real 320 kbps, 48 kHz stereo MP3 bytes and emitted framehash
records. All three prefixes have 64 packets, exactly 73,728 decoded sample
frames and no decoded timeline gaps or duplicate timestamps. Quiet uncontrolled
ADC input cannot establish analog fidelity, gain, channel routing or exact
lossy reconstruction. MP3 timestamps alone cannot prove source continuity;
the same-generation source receipts provide the separate sequence evidence.
This does not measure Sonos latency, prove network interruption recovery,
replace a two-listener soak or establish long-term throughput.

## Reproduction and checks

Private acquisition: `target/kata-hardening/oct13-mp3-preset/`. Machine-readable
receipts, image/source hashes and payload/hash-file hashes are in
[mp3-preset.json](mp3-preset.json). Raw media and HTTP/UART stay private.
[preset-driver.py](preset-driver.py), [test_preset_driver.py](test_preset_driver.py)
and [mp3-admission.patch](mp3-admission.patch) archive the exact diagnostic method.
The Python tests need retained private fixtures named in their source; these
are audit artifacts, not an added standalone CI suite. The zero-context public
patch applies with `git apply --unidiff-zero`; its hash differs from the original
contextual private patch, and both hashes are retained.

The driver passed 20 tests, including missing/contradictory receipt rejection, actual
pre-release slot accounting, nullable fresh-boot loss summaries, actual captured
startup logs, bounded real local HTTP reads and independent MP3 decoding.
Two Sol 6.1 workers implemented/reviewed the host gate; the parent implemented
the small probe after the probe worker failed before writing files. Independent
probe/driver reviews and parent final review preceded acquisition.

All 27 canonical stages passed for the diagnostic (`run.NgETQ2CI`). Restoring
both exact original source files passed all five firmware stages
(`run.JiqGpJoJ`); the rebuilt ordinary ELF equals the saved `1d2105b` image byte
for byte and lacks the probe marker. An extra broad Cargo format check found
pre-existing formatting in `firmware/examples/encoder-faults.rs:68`; canonical
checks passed and that unrelated file remains unchanged. The UART also contains
recurring MQTT esp-tls select timeouts and boot
deprecation warnings. These service errors remain recorded; no audio fault
counter increase was observed alongside them. Both flashes emitted
the usual high-baud warning and completed successfully.

Remaining e8hd gates include delivery/backpressure diagnosis, active recovery
and manual Stop through faults, long soak, fidelity/gain/tail and license review.
No AP access or physical source was assumed. TLS and workstation audio remained
untouched. Final restoration receipts are recorded in the JSON.

Final boot `e003702e4a4d677ef79228b8353515c9` reports the ordinary image,
stopped MP3/Medium, autoplay/recognition off and idle listeners. Read-only Sonos
verification reports STOPPED/OK. Final restore capture checks pass. All owned
acquisition, flash and serial processes exited.
