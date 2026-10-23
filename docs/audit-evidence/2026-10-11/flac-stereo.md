# FLAC adaptive stereo — 45k9

The encoder now chooses independent left/right or reversible left/side per block.
Identical varying stereo saves 49.94% and correlated stereo saves 32.26% in the
matched synthetic benchmark. The other eight cases save no bytes and generally
cost more CPU. Transport still failed. A bounded real Sonos codec check passed through workstation
SOAP control; native board TLS control remains failed.

## Correctness and storage

[RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html#section-4.2) defines assignment
8 as left and left−right; right reconstructs as left−side. PCM precision remains
16 bits, and side constants, warmups and verbatim samples use 17 bits. Renumbering
accepts only assignments 1/8, with minimum frames 14/15 bytes.

Independent plans never exceed two 16-bit verbatim subframes. Side replaces right
only below its conservative bit estimate, including its extra bit. Byte rounding
therefore stays within 16,386 subframe bytes; maximum header 11 plus footer 2
preserves the exact 16,399-byte bound. Heap payload grows 35,392 → 36,736 bytes;
inline predictor state grows 36 bytes. The existing two sample columns remain.
Reserved output storage is reused; heap endpoint checks do not count temporary
allocations.

Claxon and FFmpeg passed 128 exact PCM comparisons: eight fixtures × eight chained
numbering steps × two decoders, with CRC8/16 and bounds checked. Cases include
identical/opposite/hard-panned/extreme/independent/correlated samples and positive
side 32768. Selection RED receipts remain preserved. The original transport
capture helper now accepts valid left/side output; real-decoder regression tests
also reject corrupt CRCs and unsupported assignments.

## Matched ESP32 benchmark

Both images use the corrected flac-cost example: one warmup then 128 copies per
case, frame number zero, 240-frame copied input chunks, core 1, priority 7, 4 KiB
stack. Mean/max call wall time includes reset, copies, push and encode; it excludes
inter-block sleeps. CPU is task runtime per block, including charged interrupts.

| Case | Bytes/block | Saved | Mean wall ms | Max wall ms | CPU ms/block |
| --- | --- | --- | --- | --- | --- |
| zero | 14 → 14 | 0.00% | 6.500 → 8.580 | 6.530 → 8.596 | 6.529 → 8.604 |
| dc | 14 → 14 | 0.00% | 6.500 → 8.580 | 6.530 → 8.596 | 6.529 → 8.604 |
| mixed | 8204 → 8204 | 0.00% | 8.705 → 10.896 | 8.745 → 10.940 | 8.735 → 10.923 |
| noise | 16394 → 16394 | 0.00% | 11.997 → 14.115 | 12.036 → 14.164 | 12.029 → 14.144 |
| identical | 13263 → 6639 | 49.94% | 12.447 → 12.852 | 12.488 → 12.905 | 12.479 → 12.879 |
| opposite | 13263 → 13263 | 0.00% | 12.447 → 14.447 | 12.491 → 14.504 | 12.479 → 14.477 |
| hard_left | 6639 → 6639 | 0.00% | 9.216 → 11.335 | 9.263 → 11.399 | 9.247 → 11.364 |
| hard_right | 6639 → 6639 | 0.00% | 8.929 → 12.837 | 8.973 → 12.898 | 8.959 → 12.865 |
| extremes | 16394 → 16394 | 0.00% | 11.998 → 14.115 | 12.035 → 14.164 | 12.029 → 14.144 |
| correlated | 13263 → 8984 | 32.26% | 12.447 → 14.209 | 12.491 → 14.261 | 12.479 → 14.238 |

Output pointer/capacity stayed fixed. Internal heap endpoints decrease exactly
1,344 bytes between images; phase endpoints stay unchanged. Final cumulative
benchmark stack watermark is 1,716 → 1,668 bytes free. These isolated runs exclude
capture/network load and do not prove production scheduling or allocation counts.

## Checks and load evidence

The first target compile failed with LLVM `Cannot select ... Constant<-16384>` in
encode_block. Keeping the identical subtraction in an out-of-line helper fixed
the source formulation; the precise compiler cause remains inferred. The final
`scripts/check` run passed all 27 stages, including target build and warnings
checks (`target/check-logs/run.FQL4yLHq`). The original failed run remains retained.

The first load attempt failed preflight with connection refused and collected no
trial. The ready retry collected the requested 90-second staged trial but failed
transport: listener 1 ended at 33.644 s, shortly after listener 2 joined at 30.930 s.
Its solo delivery was 0.9614×; during the 2.714-second overlap, listener 1 delivered
0.0629× and listener 2 0.9747×. Listener 2's full post-join interval delivered
0.9650×. Complete decoded prefixes remain valid; excluded final bytes are unverified.

Eight complete encoder windows during that trial ran at
0.998241–1.001101×, with no encoder resets or raw overruns.
Maximum reported encode call was 34,585 µs, below the 85,333 µs block duration;
encoder stack had at least 4,932 bytes free. Post-baseline capture counters stayed
unchanged. Boot already recorded 16 queue overflows, 480 dropped frames, two
restarts and two discontinuities; this does not establish a fault-free boot.
Cleanup reported no errors and restored stopped MP3 settings. Compression and
encoder keep-up do not clear the transport failure or locate its cause.

## Native Sonos control — failed

The bounded trial failed before playback: the board never reached Playing,
and reported `Reconnect: Sonos send failed`. No left-side Sonos playback result
was acquired. UART included one POWERON_RESET startup marker; this trial cannot
distinguish a serial-open reset from buffered startup output. It does not identify
a codec failure. There were no crash/watchdog markers or UART reader errors.

The raw UART narrows the immediate failure: after the first WebSocket connection,
`Dynamic Impl: alloc(4437 bytes) failed` precedes TLS write error `-0x7F00`
and the WebSocket header-write failure. Pinned mbedTLS defines that error as
`MBEDTLS_ERR_SSL_ALLOC_FAILED`; its dynamic TLS write path allocates the outgoing
record from internal byte-addressable memory. Three later attempts show the same
allocation failure. The earlier connected snapshot reports 17,779 bytes free and
a 9,216-byte largest block, so it cannot explain the later failure by itself.
The following capture window's regional minimum sum is 2,995 bytes. Transient
internal-memory pressure is established; the allocating competitor or exact
fragmentation history is not. Overlapping MQTT attempts are not causal proof.
The encoder's additional 1,344 internal bytes narrow that budget. There is no
paired native-control baseline here to determine their contribution; do not
classify this allocation failure as pre-existing or unrelated to the change.

Cleanup succeeded: stopped MP3/Medium, autoplay and recognition disabled, source
terminated and serial closed. No mixer or volume writes ran. This native-control failure remains open. The separate codec check below uses the
same production FLAC route with workstation control, preserving this failed trial. The independent final source
and harness reviews found no remaining code blockers; all 27 canonical stages
passed. This is not streaming release acceptance.

## Real Sonos codec compatibility — passed through SOAP

The selected stopped Sonos coordinator played the same production
`/stream.flac?buffer=med` route under workstation SOAP control. Live device identity,
coordinator, transport state and unchanged source were checked before writes;
cleanup guarded its writes by the test URI. No board Play, volume or mixer writes
ran. This separates the codec check from the failed native TLS control path.

The corrected bounded run observed PLAYING for 30 seconds and monotonic position
from 0 to 29 seconds. Its concurrent debug capture contained 254 complete frames,
all assignment 8, decoding to the expected 1,040,384 stereo sample frames
(21.674667 s) with no decoder-reported omissions, duplicates or discontinuities
in that complete prefix. This does not establish ADC source-sample continuity.
Startup history explains why decoded history can exceed the 18-second acquisition.
The final 8,640 bytes lack a whole-frame CRC and remain unvalidated; this is not a
whole-run sample-continuity claim or proof of audible output. No UART was opened,
so this trial does not clear capture fault counters or measure encoder latency.

The first SOAP attempt remains FAILED: the debug reader received only the 42-byte
stream header before its two-second timeout. Sonos still advanced 0 to 29 seconds.
Cleanup restored all four saved URI/metadata fields, then an unnecessary empty
NextURI write returned HTTP 500. The corrected driver allows nine seconds per
read within an 18-second total acquisition cap, covering production's six-second
Medium admission and 8.021-second frame-write budgets. It skips an already-restored
NextURI, retains the decoder/assignment checks and verifies all four saved fields.
This is one corrected acquisition, not a relabeled first result.

The corrected run passed with no cleanup errors: prior current/next URI and
metadata restored, Sonos stopped, volume unchanged, board MP3/Medium/stopped with
autoplay and recognition off, source process reaped. SOAP ownership checks have
an observation/write race; this bounded test does not implement ownership-safe
production recovery. Original 45k9 codec acceptance is satisfied together with
the correctness, allocation and loaded timing evidence above. Native control,
transport continuity and release acceptance remain open under their existing Katas.

## Image identity

Host ELF SHA256 values (not device attestation):

- independent: `3de9105076e1e5ee31c46038ec878312c5ca1841f98fba9264d145199bae0415`
- adaptive: `76bd5c40f4daddba06ccb6eb3f853da20496b06b93b4fcc47f84ff3183154d22`
- production: `32a09ef74ea266905b59048a67705f5a428f196c3656045efe2ca1c6cb4aa0be`

The companion JSON contains the measured values without player identifiers,
private URIs or raw audio. Private acquisition artifacts remain under
`target/kata-hardening/flac-stereo/`.
