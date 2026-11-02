# Current-format stream failure with packet evidence

The production two-listener test failed while a receiver-side packet capture
recorded multi-second TCP delivery holes. Capture fault counters stayed unchanged
through the full failure observation. This confirms delayed delivery in the
current 16-bit format, but does not locate the cause in the ESP32, radio path or
access point. Kata dp08 and pd6c remain open.

The test used restored image `36c559ca`, the corrected known-demuxer harness and
two listeners requesting 45 seconds. No firmware, Wi-Fi, buffering, timeout or
speaker setting changed. Doctor Biz reported music from workstation speakers;
the signal entering LINE IN during this run is unestablished. No controlled tone
was launched. This run does not establish audio fidelity or calibrated input.

## Results

| Measurement | Listener 1 | Listener 2 |
| --- | --- | --- |
| Decoded stereo frames | 2,160,000 | 2,072,576 |
| Decoded duration | 45.000 s | 43.179 s |
| Required frames | 2,160,000 | 2,160,000 |
| Result | Passed sample count | Failed: premature HTTP EOF |
| Longest observed missing-byte-range delay | 3.151 s | 3.377 s |
| Longest interval without received TCP payload | 2.553 s | 2.604 s |
| Receiver advertised window | 64,109–65,535 bytes | 64,109–65,535 bytes |
| Zero-window advertisements | 0 | 0 |

Each flow had six observed missing subranges. These are not six disjoint stall
episodes. Neither receiver acknowledged those missing ranges before their observed
arrival. Both flows experienced overlapping holes roughly 25.5 seconds after their
initial SYNs, lasting about three seconds. Some previously observed byte ranges
also appeared again; a receiver trace cannot distinguish every sender retransmit
from other duplication or show whether the ESP32 received outgoing ACKs.

The failed flow contained 3,328,532 unique TCP body bytes. Removing its 89-byte
HTTP header gives 3,328,443 bytes, exactly the premature-EOF byte count reported
by the failed decoder. This ties the trace to that listener. The board logged
that listener falling behind the encoded ring after 51.2 seconds of stream life.

Capture advanced from 475,440 to 9,155,040 frames across retained telemetry windows.
Queue overflows/software discards/restarts/discontinuities stayed at 16/480/2/2;
read errors stayed zero. Those baseline startup faults remain real. The harness
kept the original UART open for 120.000 seconds after failure, with no watchdog
or secondary observation error. The accepted decoded prefixes contain no timeline
gaps; the short stream still fails the required sample count.

## Capture and interpretation limits

Doctor Biz started the bounded capture in a local terminal. It recorded 7,945
packets on the wired receiver interface, with 7,945 received by the filter and
zero kernel-reported drops. Capture stopped cleanly after the test and observation.
The private pcap SHA-256 is
`ede826b618875b0fa35363a1e0290077bfb3d52817deb3072621219b5824fff7`.
Numeric results, source hashes and retained artifact paths are in the
[companion JSON](stream-packet-correlation.json).

Both TCP handshakes are present. The client offered scaling and selective ACKs;
the server did not, so neither was negotiated. Window values above are unscaled.
Length accounting uses original IP/TCP lengths, not the 128-byte captured prefix.
Host records contain up to 7,200 payload bytes, consistent with aggregation;
record counts are not radio segment counts. Truncated snapshots cannot validate
full payload checksums or reconstruct audio; the independent decoder checks do
the audio accounting.

Packet timestamps share a capture clock. UART device milliseconds and harness
monotonic elapsed time have separate origins; no exact cross-clock mapping is
claimed. The flow byte count links the failed decoder without inventing clock
synchronization. The initial invocation failed before opening streams because
the Linux serial-port setting was omitted. That failure remains recorded; the
replacement used `ESPFLASH_PORT=/dev/ttyUSB0`.

Post-test readback confirms the saved Sonos configuration still matches, board
controller stopped, autoplay off and recognition off. No speaker playback or
volume commands were sent. The next useful measurement pairs sender TCP/Wi-Fi
progress with an AP-side capture. Receiver buffer exhaustion was not observed;
this trace alone does not justify changing buffer sizes or timeouts.
