# MP3 delivery with a second listener joining

The unchanged experimental MP3 firmware reproduced the two-listener failure.
Both receivers fell below real-time delivery while both connections were active.
The second listener exhausted the 12-second encoded history and disconnected;
the first then caught up. Keep MP3 experimental and Kata `e8hd` open at P1.

This run follows [the live prototype tests](mp3-stream.md). It changes the
measurement, not firmware, priorities, buffer sizes or Wi-Fi configuration.
See [numeric evidence](mp3-staged.json) and [reproduction helpers](mp3-staged-probe/README.md).

## Controlled procedure

The same first receiver drained throughout. The second began connecting after
30 seconds and received its HTTP response at 30.356 seconds. Collection ended
at about 90 seconds. Separate receiver threads and a 100 ms TCP_INFO sampler
continued during the second handshake. Real FFmpeg decoding ran after capture.
A paced local server supplied actual Shine output for positive and early-EOF
observer checks before the board test; both checks behaved as expected.

The board received independent stereo noise through the verified front Headphone
jack into LINE IN. Rear Line Out remained muted, and Sonos remained stopped.
The firmware was the committed prototype `e66f885`, experimental ELF SHA-256
`1439b5cbbc04530a315c8bdcb583a658109aa148aa9dcf5cafda5dd5673d0380`.
The JSON records the input fixture hash and private evidence hashes.

UART bytes were paired with host monotonic chunk-arrival timestamps. These share
the receiver's host clock, but include serial transmission, a 200 ms read buffer
and scheduling delay. Encoder windows crossing a phase boundary are classified
separately. The observed arrival-minus-boot offsets span 3.684 seconds, so the
serial timeout is not an accuracy bound. UART phase labels remain approximate;
receiver phase rates use the host clock directly. This is not synchronized
device-clock measurement.

## Result

The first listener received 89.952 seconds of complete coded audio and decoded
4,317,696 sample frames per channel without EOF. Its timed capture also retained
a 480-byte partial final frame, excluded from decoded counts. The second received
24.072 seconds and decoded 1,155,456 sample frames per channel, then reached EOF at 66.712 seconds
from the first admission, 36.356 seconds after its own HTTP admission.

| Interval | First listener | Second listener |
| --- | --- | --- |
| First 30 seconds, solo | 0.995 seconds of audio delivered per wall second | Not connected |
| Both connections active, sampled boundaries | Approximately 0.710 seconds per wall second | Approximately 0.663; exhausted retained history |
| After the second reached EOF, sampled boundaries | 1.457 seconds per wall second while catching up | Disconnected |
| Full planned interval after the second joined | 0.999 seconds per wall second, including catch-up after the other disconnected | 0.403, including time after its EOF |

The full planned interval hides the first client's deficit. The actual joint
window must be reported separately from the catch-up period. Phase rates use
coded bytes/whole frames and sampled boundaries; they are not independently
decoded phase PCM or proof of sample-exact source continuity.

The board logged the second listener leaving because it “fell a ring behind.”
Its server lifetime was 36.134 seconds, with 35.122 seconds in frame-write calls,
a maximum frame-write call of 2.345 seconds, and 0.389 seconds of charged task
runtime. It sent 1,003 complete frames, matching the receiver's 962,880 bytes.
Write wall time includes preemption, so it must not be called pure blocked time.

The encoder continued producing approximately 417 frames per ten seconds,
with no resets or raw overruns. Capture fault counters did not increase from
the settled baseline through the following capture report. Startup counters
were nonzero; this is not evidence of a fault-free boot. Receiver windows stayed
open. TCP_INFO observations describe these receiving sockets and do not locate
the failing hop or measure ESP32 retransmissions.

## Interpretation and next measurement

Adding the second receiver causes a delivery deficit in this run while capture
and encoding keep pace. The first receiver recovers delivery rate when the
second leaves. This strengthens the case for investigating shared transport
capacity, scheduling and backpressure under two connections. It does not isolate
ESP32 TCP processing, radio/AP queues or return-path behavior.

The next useful evidence is correlated sender send/readiness/ACK timing and AP
packet observations during this same staged test. Enlarging history would only
postpone failure while delivery remains below the generated bitrate. No codec,
buffer or scheduling change was made on the basis of this single run.

The trial runner retained the failed capture result and restored the saved
production ELF and exact original stopped Sonos configuration. The workstation
source was terminated. Restoration is verified in the JSON receipt. These
experiment helpers were reviewed independently; no product code changed, and
the previous 22-stage canonical check remains the software validation receipt
for the exact firmware under test.
