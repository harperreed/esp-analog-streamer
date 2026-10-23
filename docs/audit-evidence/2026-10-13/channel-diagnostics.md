# Channel diagnostics and measured capture cost

The existing stereo meter now supplies per-channel AC RMS, raw sample peak,
rolling digital-rail counts and signed DC offset. `/api/state.last_measurement`
adds the completed window size and measurement age. One stamped reading supplies
both that view and the existing combined level. Capture keeps its nonblocking
publication, original receipt time and reset behavior.

The last measurement is historical: it can remain visible through capture
reset or settling, with increasing age. A new completed interval replaces it.
Age does not certify continuity. Channel names describe software order; they do
not locate the previously observed end-to-end channel reversal.

## Verification

- All 27 canonical stages pass in `run.qBh9KzSn`, including firmware compilation,
  Clippy, host tests, concurrency models, harnesses and linked I2S IRAM checks.
- Fourteen diagnostic integration tests use real I2S slot conversion, meter,
  store and JSON. They cover signed extrema, DC, channel independence, rolling
  expiry, chunk boundaries, reset, age and the maximum accepted sample rate.
  The thirteen initial tests first failed against the original implementation.
- Independent Sol 6.1 review and parent fresh-eyes review found no remaining
  code defects. Pending publication discard was source-reviewed; the integration
  test does not execute the firmware capture loop.
- Thirty real API snapshots passed with complete one-second windows and ages
  of 5–240 ms. Board and Sonos settings remained unchanged.

## Resource measurements

Each quiet boot retained 100 seconds of UART and nine valid runtime windows.
The baseline and original candidate then each received the same 60-second HTTP
collection. Workstation source playback, ALSA access, mixer writes and speaker
playback were not used.

| Image | Capture CPU, one core | Maximum meter work | Maximum store work |
| --- | ---: | ---: | ---: |
| Baseline `e93f8f7…` | 9.883722% | 702 µs | 288 µs |
| Retained candidate `1d2105b…` | 11.067786% | 763 µs | 252 µs |
| Rejected u32 counter `e1222fd…` | 11.901087% | 752 µs | 284 µs |

The retained candidate misses the proposed one-percentage-point CPU target by
0.184064 points. Narrowing the counter made measured total CPU worse despite
slightly lower maximum meter work; it was reverted. Both results remain in the
receipt. These are whole-task observations from different linked images, not
isolated instruction benchmarks. Runtime includes charged interrupts; wall-work
maxima include preemption. The rejected trial has its recorded reset and UART
image prefix, but no separate API boot-token receipt.

Retain the smaller implementation for iteration with this cost explicit. The
original cn4m acceptance requires measured overhead and capture isolation; the
one-point target was a design proposal. It did not pass, and this decision is
not a release-load or endurance approval.

The target-compiled [layout probe](channel-layout.rs) measures a 336-byte meter,
40-byte reading, 176-byte store and 56-byte pending value. A conservative bound
for those objects plus one transient reading gives at most 190 additional bytes.
It uses lower bounds on original field storage rather than trusting
reconstructed Rust padding. The probe was never flashed. It does not measure
total capture call-stack depth; no capture-stack watermark was acquired. Meter
and store publication add no heap operations by source inspection.

## Bounded streaming check and final state

`scripts/stream-soak --seconds 60 --listeners 2 --path /stream.mp3
--require-clean-startup` passed, decoding to null. Both listeners produced exactly
2,880,000 audio frames, with zero reported decoder discontinuities. Nine capture
records passed the strict startup gate; steady-state fault-counter deltas were
zero. The full acquisition took 93.219 seconds, including startup. Maximum meter
work was 869 µs during this run. The uncontrolled ADC input supplies no
source-exact oracle; successful decoding does not prove calibrated fidelity,
real-time delivery under every workload, or a long soak.

Final rebuilt ELF equals the measured retained candidate exactly:
`1d2105b115ca0423097ff742193890d8329c0e366d6f52076d6655f2c25c9eb3`.
Final boot `7ad6a71e4450b1307826935ce157ad56` is stopped MP3/Medium,
autoplay and recognition off, listeners idle, with Sonos STOPPED/OK and preserved
settings. All owned collectors, decoders and flash processes ended. Existing
MQTT connection errors occurred on both baseline and candidate boots.

Keep cn4m open for known-voltage jack calibration, gain/headroom, distortion,
noise and routing measurements. Doctor Biz cannot access the workstation, so
those physical gates remain unavailable. No gain compensation or channel swap
was added. The parent audit remains 23/38 children closed, 15 open.

The [machine-readable receipt](channel-diagnostics.json) records exact images,
measurements, limits and hashes. Private raw evidence remains under
`target/kata-hardening/oct13-channel/`; no private recordings or player settings
are published here.
