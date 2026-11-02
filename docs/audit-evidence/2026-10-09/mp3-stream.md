# Live MP3 streaming prototype

The real LINE IN → MP3 → HTTP/Sonos path is implemented, but **does not pass the
reliability release gate**. Keep the default FLAC build. This opt-in prototype
reduces stream payload to 320 kbps; it does not establish that lower bandwidth
fixes the existing delivery problem. Kata `e8hd` remains open.

The [machine-readable evidence](mp3-stream.json) retains successful and failed
runs, image hashes, numeric UART observations and restoration receipts. The
[probe sources](mp3-stream-probe/README.md) document bounded reproductions.
Raw UART, audio and device configuration remain private under
`target/mp3-stream/hardware`.

## Implemented behavior

`mp3-stream` selects one shared, pinned Shine encoder at 48 kHz, stereo,
320 kbps. Default builds retain FLAC. WAV remains available in either build.
HTTP routes, MIME types and Sonos queue metadata follow the selected codec;
requesting the inactive compressed format refuses the request.

The real atomic capture ring feeds complete 1,152-sample frames. A 500-frame
encoded ring retains 12 seconds in 480,000 payload bytes. Low/Medium/High select
42/167/334 frames of source history, or 1.008/4.008/8.016 seconds. These offsets
are not receiver playback latency. Existing listener limits, generation changes,
absolute write deadlines and socket cleanup remain in use. MP3 requires
recognition off; the build rejects the unsupported combination.

The C encoder uses checked per-instance PSRAM allocations and no allocation
while encoding. The 4,608-byte PCM workspace uses fallible heap construction
through the normal allocator. Upstream provenance, licenses and local patches
are recorded in [the component](../../../mp3/vendor/shine/UPSTREAM.md).

## Measured corrections

- A controlled inline-PCM build repeatedly overflowed the main task stack.
  Constructing PCM on the heap fixed that failure. Reducing the encoder task
  stack from 16 KiB to 8 KiB also removed the observed 4,437-byte Sonos TLS
  allocation failure. The final workload left 5,924 bytes of encoder stack free.
- Treating a busy atomic publication as empty audio caused a full 10 ms wait.
  Retries now share a bounded 1 ms recheck budget before the existing sleep.
  Real-ring tests verify that rejected scratch never advances the cursor or
  reaches the encoder. This change alone did not make two listeners reliable.
- MP3 encoder priority 8, with a one-tick yield every eight frames, kept measured
  production near 417 frames per ten seconds without raw overruns. Earlier
  priority-7 runs fell behind. Final busy windows charged roughly 92–94% of a
  core to this task; that accounting includes interrupts. CPU headroom is small.

Encoder metrics separate raw-copy retries, encode wall time and encoded-ring
lock time. Listener departures report complete frames/bytes, write wall time,
read wait and task runtime. Wall measurements include preemption and must not
be presented as pure socket-blocked time.

## Hardware results

The board is an original ESP32 revision 3.1 at 240 MHz with 80 MHz PSRAM and
ESP-IDF 5.5.5. Tests used the workstation front Headphone jack into LINE IN,
with rear Line Out muted. No Sonos volume changes were made.

The final MP3 image SHA-256 is
`1439b5cbbc04530a315c8bdcb583a658109aa148aa9dcf5cafda5dd5673d0380`.
The network-fault fixture has its own image hash in the JSON. The checked default
FLAC regression image is
`b83f68bca8a22d938e11e5d8658d090f944a5934b8dc5bda10ef902d8b09559c`.

| Check | Observed result |
| --- | --- |
| Default FLAC regression, two listeners | Passed: canonical 30-second soak, 1,440,000 decoded sample frames per channel for each listener, no reported skips, duplicates or discontinuities. This bounded run does not prove long-term stability. |
| Single listener, independent stereo noise | Passed: 2,500 MP3 frames, 2,880,000 decoded sample frames per channel, 60 seconds of audio. |
| Slow client alongside a reading client | Passed: the reading client decoded 1,920,384 samples per channel; the stopped reader's slot was reclaimed and a new probe decoded 11,520 samples. |
| Wi-Fi disconnect/reconnect | Passed: two fresh streams decoded 480,384 samples each after the actual disconnect/reconnect markers. Capture fault counters did not increase from the settled baseline. |
| Sonos Medium | Played for the bounded observation, and changing the setting preserved the active queue. This is not a long soak. |
| Sonos High | Failed: encoded-history overrun after roughly 19 seconds of playback observation. |
| Two listeners, 120-second request | Failed: 50.832 and 47.736 seconds decoded; encoder continued without a reset. |
| Independent two-client TCP probe, 90 seconds | Failed: one client reached EOF after 30.812 wall seconds with only 20.064 seconds of audio; the other received 89.304 seconds. |
| Exact preset-offset hardware oracle | Inconclusive/failed assertion: concurrent requests selected cursors at different times; observed Low overlap was 29 frames versus the 42-frame target. Exact arithmetic and history selection pass host tests. |

The reconnect result proves admission of fresh streams after Wi-Fi recovery.
It does not prove an active Sonos session survives an outage. Startup capture
fault counters were nonzero; unchanged counters after baseline must not be
reported as a fault-free boot. MP3 sample counts establish decoded quantity,
not sample-exact correspondence to the captured input or absence of duplicated
content. MP3 is lossy and carries no FLAC-style consecutive frame number.

The TCP probe sampled Linux TCP_INFO without privileged packet capture. Receiver
windows remained open: active minima were 65,280 and 63,191 bytes. The first
client's out-of-order counter stayed zero; the second reached 52. Thus the
failing client does not reproduce the older trace's sequence-hole evidence.
Its sender reported about 30.24 seconds in frame writes over 32.09 seconds of
listener life, with only 0.335 seconds of charged task runtime. Those measurements
narrow the investigation but do not locate a failing network hop. Correlated
sender/AP/receiver measurements are still required. No sudo permissions or
packet-capture capabilities were changed.

## Verification and remaining gates

Canonical `scripts/check` passed all 22 stages, including real host encode/decode,
allocator-failure cleanup, codec-profile tests, Clippy, firmware compilation and
IRAM checks. Two existing ignored tests remain ignored. Both default and MP3
firmware profiles built. Independent Sol reviews found no remaining source
blocker; that does not override the failed hardware tests.

P1 before production adoption: resolve sustained delivery and High-buffer
failures, verify all presets with cursor-admission evidence, then run the
24-hour two-listener/fault acceptance gate. Automatic Sonos recovery through a
network outage also needs direct testing. Avoid further buffer or scheduling
changes without a measured cause.

P2: measure the approximately +1 dB gain difference, encoder warmup and finite
stream tails, and resolve distribution license obligations. The earlier
[codec benchmark](mp3-benchmark.md) records these fidelity limits. This change
adds no hidden gain compensation and does not claim archival continuity.

Hardware runners stop their audio source and restore the saved production ELF
(SHA-256 `0a8ee33c9a74587acd05ff23e44a0ace59c666fdc40c8704b3d7c9a3c974f4cf`),
original destination, Medium, autoplay off and stopped state. Restoration receipts
are part of the evidence; the experimental MP3 image is not the default deployment.
