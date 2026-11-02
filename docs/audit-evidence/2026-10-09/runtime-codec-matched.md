# Matched MP3 / FLAC / MP3 transport

Both MP3 trials failed: each lost both direct receivers to retained
history exhaustion. MP3 delivery was already below real time with one receiver,
then fell further while two were connected. FLAC held both receivers through the
90-second bound, but its joint delivery also remained below real time. Keep the
MP3 reliability finding at P1; this runtime selection feature does not fix it.

The parent completed all three trials on the same deployed unified image, with
the same driven stereo-noise source. The runner reports no run or cleanup error.
This analysis only read saved evidence. The sanitized numeric receipt is
[runtime-codec-matched.json](runtime-codec-matched.json); private source trials are `0-mp3`, `1-flac`, `2-mp3`.

Rates below are complete received coded-audio seconds per host wall second.
Joint windows end at the first actual natural EOF, or at the capture deadline.
The surviving receiver's subsequent delivery is shown separately; it never
reached a rate sufficient to catch up in either MP3 trial.

| Trial | First 30 s, solo | Actual joint window | Joint listener 1 / 2 | Survivor alone, listener 2 | Natural EOF, listener 1 / 2 |
| --- | --- | --- | --- | --- | --- |
| MP3 first | 0.794 | 12.529 s | 0.475 / 0.433 | 0.877 over 44.558 s | 43.486 / 88.044 s |
| FLAC | 0.996 | 58.972 s | 0.965 / 0.955 | No natural disconnect | None through 90 s |
| MP3 repeat | 0.818 | 11.245 s | 0.480 / 0.455 | 0.805 over 33.053 s | 42.888 / 75.941 s |

EOF times are measured from first HTTP admission. Second HTTP admission occurred
at 30.957, 31.028 and 31.643 seconds. Handshake intervals are separate from solo
and joint windows. Both MP3 trials lost listener 1 first. The original probe's
named recovery phase only handles listener 2 ending first, so this analysis
recomputed the survivor's interval from saved arrival records and complete frame
ends. UART attributes all four MP3 departures to “fell a ring behind.” The first
trial's last data arrival preceded its final EOF; the EOF timestamp above is the
correct phase boundary.

The FLAC joint deficit was about 2.055 seconds of complete audio for listener 1
and 2.652 seconds for listener 2. Its successful capture verdict means bounded
socket survival and valid whole prefixes, not sustained real-time delivery.
No wider reliability conclusion follows from one FLAC trial.

| Trial / listener | Complete coded frames | Independently recounted PCM frames per channel | Decoded seconds | Excluded final bytes |
| --- | --- | --- | --- | --- |
| MP3 first / 1 | 1,289 | 1,484,928 | 30.936 | 0 |
| MP3 first / 2 | 1,855 | 2,136,960 | 44.520 | 0 |
| FLAC / 1 | 1,020 | 4,177,920 | 87.040 | 6,057 |
| FLAC / 2 | 660 | 2,703,360 | 56.320 | 13,377 |
| MP3 repeat / 1 | 1,279 | 1,473,408 | 30.696 | 0 |
| MP3 repeat / 2 | 1,322 | 1,522,944 | 31.728 | 0 |

Every complete prefix's saved framehash independently recounts to exactly
`coded_frames × block_samples`, with no decoder timestamp gap, duplicate or
sample/frame mismatch diagnostic. All 1,680 complete FLAC frames independently
pass CRC-16. The final excluded FLAC bytes remain unvalidated; a CRC-invalid final
packet does not prove truncation rather than corruption. MP3 checks establish
fixed headers, no reservoir dependency and exact decoded counts; no MP3 CRC or
sample-exact source continuity claim is made.

Across eight complete UART report windows inside each trial, encoder generation
ratios stayed near real time: 0.997–1.003 for the first MP3, 0.999–1.001 for FLAC,
and 0.994–1.007 for the MP3 repeat. Average encode wall cost per completed frame
was 21.629 ms, 7.860 ms and 21.610 ms. These averages divide summed interval
encode wall time by changes in cumulative frame counters; they include preemption
and are not isolated codec CPU times. Each format commit resets the cumulative
frame/reset counters. The JSON also records separate whole-epoch averages,
including inter-trial periods, without mixing those epochs.

No encoder reset or raw-read overrun was observed in those report windows. The
minimum reported encoder free stack was 5,496 / 5,496 / 5,320 bytes. Capture
publication advanced at approximately 48,005 / 48,002 / 48,002 frames per device
log second. Each trial's fault comparison uses a pre-admission capture report
and the first report arriving after the capture end; all eight tracked fault
counter deltas were zero. Those baselines already contained 15 queue overflows,
480 software-dropped frames, two capture restarts, two overflow restarts and two
discontinuities. This run did not reboot the board and does not establish a
fault-free boot.

Receiver frame counts and phase rates use the host clock directly. UART labels
use host serial-chunk arrival times, which include serial buffering, transmission
and scheduling delay; they are approximate and do not establish device-clock
synchronization. Capture/encoder windows crossing a boundary are not exact phase
measurements. Sender write wall time includes preemption, and receiver TCP_INFO
cannot identify the failing network hop. Valid prefixes do not prove analog
capture continuity or speaker playout.

Recommendation: keep the MP3 delivery failure open at P1 and investigate sender
readiness/ACK timing, scheduling and allocation/cache placement using these
repeated same-image failures. The current evidence does not identify a proven
fix. Preserve FLAC's smaller joint deficit as a separate failed real-time gate.

## Reproduction and deployment

Run ID: `20261010T035157Z` UTC (October 9 local time). The image is the same
unified build accepted in [runtime-codec.md](runtime-codec.md), feature commit
`fa7f610`; the final source differs from the flashed build only in a comment.
ELF and fixture hashes are retained in the JSON. No reflash, reboot, Sonos Play
or volume change occurred during these trials. The existing independent
stereo-noise source fed LINE IN through the confirmed front jack.

The same boot's earlier radio readback was protocol `0x07`, power save `0`,
channel `6`, RSSI `-40 dBm`, negotiated PHY `4`. This is boot provenance, not
a contemporaneous radio measurement for each trial. The raw receiver/UART
evidence remains private in `target/runtime-codec/matched/20261010T035157Z`.
The [archived observer](runtime-codec-probe/README.md) records its commands,
dependencies and limits. It requires no privileged packet capture. AP-side
evidence remains absent; it cannot locate the failing hop.

The runner stopped its physical source and UART collector. Independent final
API readback confirmed selected/active MP3, Medium, autoplay off and stopped.
No source player or hardware runner remained. Keep this iteration firmware
deployed. Kata `e8hd` and `pd6c` remain open.
