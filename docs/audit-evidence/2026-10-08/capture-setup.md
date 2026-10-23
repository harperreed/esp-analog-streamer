# Capture during TLS and reconnects

The local TLS fixture reproduced capture loss while all three TLS connections
completed successfully. This is a capture failure during connection setup, not
evidence that an HTTP-only stall explains every interruption. Its cause within
the scheduler, driver or shared resources remains unlocated.

Implementation checkpoint: `558d774`. Numeric observations and image identities
are in [capture-setup.json](capture-setup.json). Private UART, decoder hashes and
peer logs remain under the recorded `target/` paths.

## TLS results

The fixture runs the real Sonos receive-only TLS/WebSocket transport alongside
production capture and streaming. It trusts a dedicated local test certificate,
sends no speaker commands, and leaves production trust/settings unchanged. Its
extra controller task uses a 16 KiB internal stack, core 0, priority 2. The usual
WebSocket task uses another 6,144 bytes. These are diagnostic workloads.

All runs requested 120 seconds, or 5,760,000 stereo frames per listener.

| Run | Decoded frames per listener at summary snapshot | Result |
| --- | --- | --- |
| One listener | 5,760,000 | Pass; three TLS cycles, no added capture faults |
| First two-listener run | 925,696 / 847,872 | Premature HTTP EOF; UART stopped too soon to identify capture state during failure |
| Two-listener repeat | 5,760,000 / 5,760,000 | Pass; three TLS cycles, no added capture faults |
| Two listeners with attempted source playback | 933,888 / 851,968 | Premature EOF; retained UART proves capture faults in all three TLS cycles |

The failed-run summary snapshots precede decoder cleanup. Final retained hashes
contain 925,696 / 925,696 frames for the first failed run and 933,888 / 921,600
for the attempted-source run. Both still ended prematurely.

Physical ALSA playback was RUNNING with unmuted output controls, but the board's
level stayed around −73 to −76 dBFS. Doctor Biz later confirmed the workstation
was not yet connected to LINE IN. These are quiet-input runs, not a
music-versus-quiet comparison.

The retained failure run added 37 queue overflows, 720 software-discarded frames,
three overflow restarts/discontinuities, and 72,000 settling-discarded frames.
Its three post-handshake capture windows recorded service-delay estimates of
98,857, 150,845 and 65,217 µs. Both listeners closed after the first generation
reset. Later capture counters stopped increasing. Every TLS cycle completed,
and the original UART contained no client-destroy failure.

Those counters do not account for every lost DMA sample. The service estimate
includes processing, scheduling and I2S read time. `raw_read_max_us` measures the
encoder's raw-ring copy; it does not measure the I2S driver read. Heap minima are
sums of per-region lows, and largest-block values are snapshots.

## Preserve the failed interval

`stream-soak --failure-observe-seconds 60` kept the original UART collector and
remaining decoders alive for 60.000653 seconds after the failure, with no secondary
error. The original failure still determined the exit status. Summary counters
remain explicitly aged pre-failure snapshots; the raw retained UART supplies the
later observations. The first failed run's last counter sample was 9.588 seconds
old at failure. Reopening UART afterward reset this board and cannot repair that
missing evidence.

Eight real-process observation tests and eleven real TLS peer tests pass. The
canonical check passes all 19 stages. Independent Sol reviews covered the TLS
peer, diagnostic examples, and failure observer; parent fresh-eyes review found
no further source issue before checkpoint `558d774`. Hardware later exposed a
settling-fixture dump starvation issue; the failed recording is retained.

## Wi-Fi reconnect

The existing network-faults fixture forced one real disconnect. Production
reconnected, tracked capture fault counters stayed unchanged, and two fresh
listeners each decoded exactly 480,000 frames. The listener-cap assertion also
passed. This establishes capture and new-stream recovery for that injected
disconnect; it does not establish automatic recovery of an owned Sonos session.

## Pre-discard settling observations

After all acquisition, the initial diagnostic's long UART dump starved IDLE0 and
interleaved watchdog messages with sample records. The fix lowers reporting
priority and blocks for one tick every 16 rows. It does not change acquisition.
The corrected image produced 2,000 complete, ordered blocks: ten valid 48,000-frame
trials, with no capture errors/overflows or watchdog reports. Existing PSRAM-map
and legacy-I2C boot warnings remain; this is not a warning-free boot claim.

Five paired trials compare the real codec soft setup with I²S-only restart.
Every soft setup began with at least 50 ms of zero samples; four retained zeros
for 215–235 ms. One trial had a large transient whose right-channel magnitude
remained above 5,000 LSB through block 83, or approximately 420 ms of captured
audio. Every I²S-only trial had nonzero energy from its first block. Its first
50 ms AC RMS was within 0.96 dB of its own later reference window.

These are quiet-input observations, not proof of correct known-signal delivery.
Block moments cannot identify the intended tone frequencies. Time zero follows
I²S restart, and codec setup includes readback after ADC enable. Doctor Biz
confirmed the workstation output was not yet connected to LINE IN.
The production 500 ms discard remains unchanged.

## Task observation during another failure

A diagnostic-only core-1 task sampled the permanent capture task's state and
core 0's numeric current-task identity during seconds 35–75. It used a 4 KiB
internal stack and 98,304 bytes of preallocated PSRAM, and printed after capture
of the records. The resulting 3,926 consecutive records passed framing/timing
checks; the final END marker confirmed no buffer overflow.

The third TLS cycle failed capture again: 11 additional overflows, one restart,
240 software-discarded frames and 24,000 settling-discarded frames. Immediately
before the failure, nine samples over 79.994 ms reported capture Ready.
Each core-0 task sample had the same unmapped numeric identity. The next observation began 139.908 ms later.
The first two cycles had larger observer gaps without capture faults, so gaps
alone cannot measure capture starvation.

Task identities do not identify interrupt work. State and identity queries are
sequential, and the state API takes a kernel lock. Only core 0 advances the
shared tick, so delayed sampling on core 1 does not establish that both cores
stalled. A follow-up diagnostic adds a numeric WebSocket-task match and the
actual capture priority; no transient task pointer is dereferenced.

The task-map image then produced a passing comparison and a failed repeat.
The passing run decoded 5,760,000 frames per listener with no new capture faults.
Capture priority was 10 in every sample. Its 3,967 numeric records included one
damaged UART prefix; the label was explicitly recovered, so this is not a clean
framing claim. None of its 85 numeric WebSocket matches coincided with Ready
capture. A 241,932 µs probe did not coincide with capture loss and cannot be
treated as capture starvation.

The failed repeat had 3,638 intact records and a different loss mechanism.
Queue overflows and capture restarts stayed at 12 and 1. Software-discarded
frames increased by 4,320, and discontinuities increased by two. Staging reached
its full 2,160-frame capacity; publication window maxima were 112,352 µs
pending and 22 attempts, and the encoder's raw-ring read closure held its mutex for
110,567 µs, including possible preemption. This supports two full staging
losses while I²S continued, separate from the previous DMA overflow/restarts.
Final retained decoder hashes account for 2,273,280 frames per listener.

One sample during the failing third cycle matched the WebSocket task numerically
while capture was Ready at priority 10. The next observation began 979,970 µs
later. That single sample does not establish continuous execution, the
WebSocket task's effective priority, or which work held up the ring reader.
It does establish a concrete reason to remove reader-held locking from capture
publication, while investigating DMA service gaps separately.

## Remaining work

Trace capture task progress through the reproduced TLS failure without changing
priorities, buffers or trust. Keep the startup capture faults separate. The
pre-discard settling fixture needs verified physical input before any shorter
settling policy can be justified. Codec soft
reinitialization is not a physically cold power cycle.

## Restored production

Diagnostic follow-up source is committed as `35cf03c`. The saved production
image `3dc70b4a5c49bc5af9889ea602967b4171b5899917306d02139cb507a86f79d7`
is restored. Its final 45-second two-listener check decoded exactly 2,160,000
frames per listener, with no skipped/duplicate/discontinuous decoder frames
and no added tracked capture faults. Startup faults remain in the baseline.
Firmware configuration and Sonos configuration match their initial snapshots;
Sonos remains stopped with autoplay off. All 19 canonical check stages pass.
This short quiet-input check does not establish long-soak or TLS reliability.
