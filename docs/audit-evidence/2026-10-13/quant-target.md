# Standalone ESP32 quantization cost — qm2a

Doctor Biz, keep the current truncation policy. This bounded synthetic experiment
measures the CPU and compression cost of rounding and one discrete TPDF candidate;
it does not establish an analog or audible improvement. No production source or
quantization policy changed. The temporary example starts no capture, Wi-Fi or
playback. The existing qm2a issue stays open.

## Measured result

All 180 target phases match the host source, PCM, encoded-frame and RNG receipts.
Each 4,096-frame stereo block represents 85.333 ms at 48 kHz. Five fixtures run all
six policy-order permutations, with one warmup and 16 measured blocks per phase.
The 90 measured phases cover 1,440 blocks, or 5,898,240 stereo sample frames.

Values below are median per-block preprocessing microseconds across the six
measured phases. Encoded bytes are per block, averaged across 16 blocks.

| Synthetic fixture | Truncation µs | Rounding µs | TPDF µs | Bytes: trunc / round / TPDF |
| --- | ---: | ---: | ---: | ---: |
| Silence | 3,141.69 | 3,489.56 | 5,604.12 | 14 / 14 / 1,420.875 |
| Quarter-LSB DC | 3,141.69 | 3,515.00 | 5,631.81 | 14 / 14 / 1,518.125 |
| Triangle fade | 2,705.81 | 3,099.94 | 5,555.81 | 1,425 / 1,425 / 1,857.0625 |
| Independent 24-bit noise | 3,145.56 | 3,493.81 | 5,915.88 | 16,394 / 16,394 / 16,394 |
| Signed extrema | 3,145.56 | 3,498.81 | 5,945.12 | 16,394 / 16,394 / 16,394 |

Rounding adds 348–394 µs/block to preprocessing; TPDF adds 2,462–2,850 µs.
The largest measured block wall time is 20.583 ms; including warmup gives
20.593 ms. Both are below the block's represented audio duration in isolation.
These wall times include checksums, assertions and timer overhead. Capture,
network traffic, listeners and production contention are absent, so this is
not loaded-system headroom or real-time acceptance.

All phase heap endpoints match: 263,439 internal and 4,109,580 external bytes
free. The source uses 32,768 PSRAM bytes and the reusable chunk uses 1,920 internal
bytes. Output storage stays at the same pointer and 16,399-byte capacity. Minimum
free task stack is 1,348 bytes. RNG state is logically eight bytes; this does not
measure total incremental production heap cost or prove absence of transient
allocations between observations.

## Contract and limits

The probe transforms samples into a reusable chunk, then feeds the unchanged
production FLAC encoder. It therefore repeats baseline quantization inside the
encoder; these costs do not predict a fused implementation. Timers remain
uncorrected, with 78 calls per block and an empty-pair calibration in the UART.
Source generation and source hashing are outside timing. Task runtime excludes
sleep; phase wall time includes a 10-ms yield after every block.

Rounding uses signed-wide addition and saturation, with ties toward positive
infinity. TPDF uses two successive high bytes from a per-channel xorshift32
state. These streams are approximate pseudorandom draws, not proven independent.
Ideal uniform-byte zero input has mean +1/512 output LSB and variance
1/4 − 1/262144. This discrete candidate differs from October 12's continuous
NumPy candidate; their quality and byte results must not be combined.

Dither greatly enlarges perfectly silent/DC fixtures and adds 30.3% to the
triangle fixture. Real analog noise may change that tradeoff. These fixtures
are not a full tone/noise-spectrum sweep, ADC calibration or listening test.
Policy adoption still needs those checks and loaded Sonos compatibility.

## Verification and provenance

Base source is c7eab74. Diagnostic ELF SHA-256:
`68791e0ca604919dc5fbd458d222ba2c487f5b486acf64080565ac75b8da80e6`.
The single recorded boot reports its matching SHA prefix and completes all rows.
Host verification includes five kernel tests and 30 real production encodes,
checked against an independent Python scalar oracle and actual FFmpeg decodes.
Transformed truncation is byte-identical to direct raw-input production encoding.
Corrupt oracle and decoder controls reject; the parent independently reran decoding.

The parser rejects all 45 synthetic negative controls and both corruptions of the
actual UART acquisition. Synthetic parser rows are explicitly test data and do
not count as board evidence. Two Sol 6.1 workers implemented/reviewed the probe;
independent raw-row review confirms the numerical result.

The first explicit example build failed E0282 because a buffer's element type was
unresolved before its pointer cast. Adding the explicit u8 type fixes the probe;
the retained final example build has no warnings. The 27-stage canonical run
`run.v8h8Edp2` passed. After removing exactly the two temporary source files,
`run.ldXR4onj` passes all five firmware stages and rebuilds the exact saved
ordinary ELF. No tests or checks were disabled.

See [probe archive](quant-target-probe/README.md) for sources, receipts, raw UART,
negative controls, restoration evidence and reproduction instructions.

The collector restored ordinary ELF
`0ff2a7ccb677fb8045f6faa1d040b930f8adaaf38913946c1847e1f3eb14faf2`.
A new boot reports that exact identity, preserved MP3/Medium settings, idle
listeners and stopped board/Sonos transport. The two diagnostic startup warnings
(4-MB PSRAM mapping and deprecated I2C driver) also occur in the ordinary boot;
neither is new to this probe. No workstation audio was started.
