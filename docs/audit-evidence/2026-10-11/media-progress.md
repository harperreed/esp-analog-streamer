# Pipeline progress and media-age acceptance (6vct)

The original bounded-policy acceptance is satisfied. This does not clear the
remaining transport failures: the instrumented-image FLAC delivery trial and the
extended WAV continuity trial both failed. Keep those on `pd6c` before a streaming
release. Machine-readable measurements and artifact hashes are in
[media-progress.json](media-progress.json).

## Implemented policy

Accepted capture, committed raw blocks and encoded output carry coherent source
receipts. Capture never waits for readers; timestamps use split 32-bit atomics
and existing bounded storage. Software acceptance age is distinct from queue
lag, write duration and speaker latency. Raw commit time follows sample copying.

Payload age is limited to four seconds plus selected rounded startup history:
approximately 5/8/12 seconds for Low/Medium/High, four seconds for direct live
endpoints. The earlier socket or media deadline wins. Partial output is never
spliced; late successful sends retain actual byte accounting and end explicitly.
An in-flight lwIP call cannot be forcibly interrupted by this policy.

Two seconds without encoded progress while valid raw input advances requests
one cooperative reset. Real output must follow within a fixed two-second grace;
otherwise compressed admission fails locally. The latch survives capture/ring
resets and clears only on operator epoch replacement. Stale supervisors cannot
act on replacement epochs. This path does not restart capture, reboot or call
Sonos Play, and uses the existing supervisor and deferred telemetry exporter.

## Verification

`scripts/check` passed every stage in `target/check-logs/run.ECoUgIPy`, including
host tests, property/Loom/replay checks, real decoder fixtures, lint/format,
fuzz/mutation checks, firmware build and effective I2S IRAM checks. Source was
frozen during this run; later changes only preserve evidence and plan status.
The diagnostic `encoder-faults` target also built successfully.

Independent Sol 6.1 reviews covered raw metadata, policies, integration, harness
amendments, real fault evidence and final acceptance. Parent fresh-eyes review
found no additional source issue. Implementation tests cover timestamp halves,
wrap/exhaustion, generation changes, accepted-but-unpublished input, unavailable
receipts, slow/late/partial writes, recovery and operator replacement.

Fault run `20261011T065216.651426Z` passed the original bounded MP3 gates:

- A 2.503-second encoder hold recovered with real publication 0.635980 seconds
  after the reset request. Two fresh decoders each produced 480,000 stereo
  frames with no gaps; the third listener received 503.
- A 17.002-second hold reached terminal state 2.090 seconds after its request
  (supervisor observation includes scheduling delay). Two later admissions
  received 503. Capture continued without any post-baseline fault increments.

Startup counters were not zero: 17 queue overflows, 480 dropped frames, two
restarts and two discontinuities predated the fault baseline. The original
listeners' source spans were 1,194,624 and 1,186,560 samples; decoded prefixes
were 1,193,472 and 1,185,408. Each reserved final packet accounts for exactly
1,152 samples and 960 bytes. Those tails are not certified decoded audio.
All 19 exporter receipts were lossless; checked closes released admission slots.
No later reboot or Play appeared in the retained boot.

## Measured overhead and failed delivery trials

The matched workload uses 48 kHz stereo noise, 90-second captures and a second
listener joining at 30 seconds. The baseline matrix passed MP3/FLAC/MP3. The
instrumented matrix passed MP3, failed FLAC, then passed MP3. This verdict means
bounded capture and valid complete prefixes, not every source sample delivered
by the requested stop time. Every trial had zero post-baseline capture fault
increments, eight valid measurement windows and no telemetry loss.

| Median or observed minimum | Baseline | Instrumented |
| --- | --- | --- |
| Encoder production rate, all phases | ~1.000x | ~1.000x |
| Capture task runtime fraction | 9.7–10.0% | 10.7–10.8% |
| MP3 encoded-ring push wall time/frame | ~82 µs | 181–185 µs |
| FLAC encoded-ring push wall time/frame | 1,184 µs | 1,248 µs |
| Encoder free stack | 5,188 B | 4,996 B |
| Metrics free stack | 1,084 B | 840 B |
| Supervisor free stack | 3,060 B | 2,448 B |
| Listener free stack | 1,512 B | 1,192 B |

Encoded metadata adds 24,000 PSRAM bytes for the 500-slot MP3 ring, 6,768 for
the 141-slot FLAC ring, or 1,152 for recognition's 24-slot FLAC ring. Raw block
timing adds 16 bytes per slot. All storage remains bounded. Observed heap minima
are retained per trial in JSON. These two-image measurements include code-layout,
scheduling and workload effects; they do not isolate individual instruction
costs. Task runtime includes charged interrupts, wall time includes preemption,
and stack minima are observations rather than worst-case proofs.

FLAC listener 1 ended after 62.675 seconds with `media age exceeded`. Its pending
frame waited 2.639 seconds despite an 8.021-second socket allowance, consistent
with the earlier four-second live age deadline. Completed frames reached a
maximum age of 3.788 seconds; this excludes the incomplete frame. The pending
frame's acceptance timestamp was not retained, so exact expiry arithmetic cannot
be independently reconstructed from the departure log. Checked close took
444 ms. Its 688 complete frames decode to 2,818,048 samples, exactly the recorded
source span. Accepted payload bytes plus the 42-byte header equal received bytes;
the 11,026-byte tail remains uncertified. Encoding and capture kept advancing.
This supports explicit age enforcement but remains failed continuous delivery.

Extended fault run `20261011T070109.797223Z` admitted WAV in the same boot after
compressed terminal state, then decoded 73,728 samples before EOF at 466,604
body bytes. The device reported `stopped reading` on its socket deadline.
The ten-second WAV test remains failed in the harness. Neither this run nor
the matched matrix locates the failing network hop or rules out indirect timing
effects from the change. Solving universal transport delivery is outside 6vct's
original acceptance; the failures remain recorded on `pd6c`.

## Retained failures and final state

Earlier acquisitions remain failed: obsolete startup marker, incorrect recovered
generation validation, and querying HTTP before registration. Each harness defect
was corrected with focused regression coverage. A separate decoder-oracle defect
missed an interleaved `CRC error at PTS` on an info line; the exact regression
failed before the fix and all 17 decoder tests then passed. Its original log,
framehash and raw artifact hashes are in
[decoder-crc-diagnostic.json](decoder-crc-diagnostic.json). No prior failed run
has been relabeled as passing.

The normal production image SHA256 is
`8fb4228ff4abff83cc39b5a63eea2a301865f83947f3ec6181989cdd857ac81a`.
It was flashed successfully at 07:06:39 UTC; this is saved-image/flash provenance,
not full flash readback attestation. After all trials, live API readback confirms
MP3/Medium/stopped, autoplay false and recognition false. The source player and
UART collector exited. Front-headphone routing was checked without changing gain.

Before shipping: resolve the P1 transport/continuity failures on `pd6c`; assess
the P2 reduced stack headroom under the longer release workload. These bounded
trials do not establish a 24-hour soak, analog fidelity or speaker latency.
