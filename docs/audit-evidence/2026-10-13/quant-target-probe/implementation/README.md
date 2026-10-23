# Throwaway qm2a target cost probe

Doctor Biz, this is a bounded synthetic cost probe, not a production feature.
Copy only `quant-cost.rs` and `quant-cost/contract.rs` into the corresponding
`firmware/examples/` locations temporarily. The parent owns target builds,
flash/serial acquisition, restoration, canonical checks, and commits.
No target result has been acquired by this implementation agent.

Run `bash target/kata-hardening/oct13-quant-target-impl/check-host` from the
repository. `--help` describes the runner. Rust source formatting and warning-free
host compilation, five kernel tests, 30 real production encodes, independent
Python scalar arithmetic, and 30 FFmpeg sample-exact decodes passed. Logs are
`kernel-green.log`, `host-encode.log`, and `ffmpeg.log`. The initial two expected
failing stub tests remain in `kernel-red.log`. `negative-oracle.log` rejects a
changed expected PCM byte; `negative-decoder.log` rejects an invalid FLAC stream.

`host-output/receipts.jsonl` contains the expected byte totals, frame and PCM
FNV-1a-32 digests, source digest, largest frame, and final channel RNG states.
`host-output/ffmpeg-results.json` adds sample counts and SHA-256 digests. Each
retained `.flac` has a normal stream header and consecutive frame numbers;
receipt hashes cover frame-number-zero output before renumbering. The shared
contract embeds the host receipts; both host and target assert them. The host
also byte-compares every transformed truncation frame with a direct raw-input
production encode. No mocked encoder or decoder is involved.

Policies accept sign-extended capture codes, including deliberate i32 extrema:

- Truncation: clamp arithmetic `x >> 8` to signed 16-bit.
- Rounding: clamp `floor((i64(x)+128)/256)`, with ties toward positive infinity.
- Discrete TPDF: draw two successive high bytes `a,b` from each channel's
  xorshift32 state; clamp `floor((i64(x)+a-b+128)/256)`.

Each policy writes `q*256` into the reusable 480-i32 chunk, then feeds the
unchanged production encoder. This deliberately repeats the encoder's baseline
quantization. It measures preprocessing plus the existing encoder, and cannot
estimate the cost of a fused production implementation. Wider addition precedes
clamping. Xorshift uses shifts 13,17,5 and distinct nonzero seeds `0x12345678`
and `0x9abcdef1`, reset for every phase. These are approximate uniform/correlated
pseudorandom streams, not mathematically independent draws. No rejection
sampling is needed for the high-byte extraction. Seeds control only this probe.

Under ideal independent uniform bytes, discrete zero-input TPDF has counts
8128/49152/8256 for -1/0/+1 across 65,536 pairs: mean +1/512 output LSB,
second moment 1/4, variance `1/4-1/262144`. This differs from continuous TPDF.
The exhaustive arithmetic test verifies those counts; the actual seeded kernel
also passes support, mean, variance, and cross-channel correlation bounds.
Literal RNG vectors and whole-block versus even-chunk sample/state equality
cover generator and channel-state handling. The Python oracle reads raw fixture
codes and uses floor division plus separately written xorshift arithmetic.

Five 4096-frame stereo source blocks repeat: silence; opposite +/-64 capture-code
quarter-LSB DC; an opposite-channel integer triangle with peak 4096 capture codes
and a linear block fade; two separately seeded full signed 24-bit noise streams;
and a repeated extrema table containing both i32 rails, both 24-bit rails, and
-129/-128/127/128. The exact integer generators live in the shared contract.
There is no sinusoid, low-level random-noise fidelity sweep, private media,
ADC capture, or listening test. These fixtures support isolated cost comparison.

The target starts only a core-1 priority-7 thread with a 4-KiB stack. It asserts
a 32768-byte PSRAM source buffer, keeps the 1920-byte chunk in internal RAM,
reserves `MAX_FRAME_BYTES` output bytes, and reuses the unchanged encoder.
The extra logical RNG state is eight bytes. Actual heap endpoints, output
pointer/capacity, placement and stack watermark are reported; no total added
heap delta or production headroom claim follows from that logical state size.

Each fixture runs all six policy-order permutations. Each policy gets one
warmup block and 16 measured blocks in each order: six measured repeats per
policy/fixture, 180 phase lines and 1530 blocks overall. Source generation and
source hashing happen outside timing. A 10-ms sleep before phases and after
every block matches `flac-cost` runtime-counter boundaries. Phase wall time
includes those final sleeps; block wall time excludes them.

`quant_us` includes PSRAM reads, quantization, and chunk writes. `encoder_us`
sums reset, each push, and encode timings. Block wall and task runtime also
include timer calls, PCM/frame checksums, and assertions. Checksum work stays
outside preprocessing and encoder timers. Task runtime excludes sleep but
includes charged interrupts and counter sampling limits. The log marks runtime
invalid if uptime exceeds the 32-bit microsecond counter interval. There are
78 timer calls per block; setup records 256 empty timer-pair observations.
Timers remain uncorrected and their measured overhead is not subtracted.

Every phase asserts host receipts, final RNG states, equal heap endpoints,
and stable output storage. Stack watermark is cumulative for the task, not an
isolated phase peak. Compare bounded block timing with the 85,333-us audio block
budget only as an isolated observation: capture, Wi-Fi, listeners, and production
contention are absent. A complete acquisition must contain all 180 phase lines
and `quant-cost: complete phases=180 assertions_passed=true`.

Provenance: HEAD `c7eab74adf9925e7769227d58e45010a1485b097`;
production `core/src/flac.rs` SHA-256
`01f0efabe7b759d4e09ef3ab66c5d5b75a12737713bf1664b29242134d472d46`.
Host tools: rustc 1.93.0, uv 0.12.19, FFmpeg 8.0.1-3ubuntu2.
The machine-local `~/.codex/docs/local.md` was absent; observed tools supplied
these versions. No dependencies or product files were changed by this agent.
