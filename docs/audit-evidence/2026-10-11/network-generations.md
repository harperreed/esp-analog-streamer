# Wi-Fi lifecycle generations — October 11

Kata ewvq's same-IP device acceptance passes. Implementation, independent source
review and all 27 canonical stages pass (`run.p3FPXV1J`). The network-faults
example also builds with the pinned ESP toolchain and passes the linked I2S
IRAM check. The first manual example build accidentally targeted the host and
failed; the corrected build ran from `firmware` with `cargo +esp`.

Device 3 changed generation 2 → unavailable 3/4 → ready 5 at the same address.
The host observed readiness 4.205 seconds after injection. Both old listeners
reported network change, completed checked close in 0–1 ms, and released their
slots. The stale URL returned 503. Both fresh listeners decoded 417 MP3 frames,
480,384 stereo sample frames each. All four peers have one join, departure and
successful checked-close receipt, with held-slot snapshots at most two.

Capture advanced from 1,443,840 to 3,876,720 frames while all nine fault counters
remained unchanged. There was one expected boot marker and no unexpected restart
or telemetry loss. Startup faults remained: 14 queue overflows, 480 software
discarded frames, two overflow restarts, 26,400 settling discards and two
discontinuities. They are not cleared by this active-test result.

Old host sockets remained open until after the fresh streams completed and then
received explicit host cleanup; neither reported remote EOF. Their complete
prefixes decoded 989/983 MP3 frames (1,139,328/1,132,416 stereo sample frames).
All four retained captures had zero partial tail bytes. The runner's 14 host
regressions pass. Full counts, exact image/source/fixture hashes, retained failed
run results and artifact hashes are in [network-generations.json](network-generations.json).

The normal updated production image was restored successfully. HTTP readback
confirms saved/active MP3, Medium buffering, stopped playback, autoplay off and
recognition off. No UART or source-playback process remains. This is flash-tool
and application-state evidence, not a full flash readback attestation.

One shared boot-local generation now follows Wi-Fi association and address
changes. Old stream listeners and queued Sonos network commands expire;
published media URLs carry their generation. The existing supervisor publishes
the current address, and stream admission drops obsolete listening backlogs
before rebinding. Capture and encoding do not depend on this state. Reconnection
restores Sonos observation only; it does not create or reclaim playback.

Host checks cover unchanged and changed addresses, stale observations, no-IP
startup, generation exhaustion, queued command cancellation, late real socket
replies, stale queue URLs, parser validation, stream retirement and real TCP
backlog replacement while an accepted peer remains alive. Existing Stop and
configuration behavior remain covered.

## Device method and retained failures

The diagnostic example injects one public `esp_wifi_disconnect()` call 60 seconds
after startup. The normal supervisor alone reconnects. The host plays the same
420-second independent stereo noise fixture once through the verified front
headphone output to LINE IN. No mixer settings or Sonos Play commands change.
Saved and active format remain MP3, Medium buffering, stopped playback, autoplay
off and recognition off. Test requests use the direct MP3 stream with generation
tags; this run does not measure buffered playback latency.

Two old streams stay open through injection. Acceptance checks successful local
socket close, current address/generation publication, a 503 response for the old
URL, and exact sample counts from two fresh streams. It retains old host peers
until the fresh admissions finish, checks capture progress and all nine fault
counter deltas, and rejects unexpected boots or missing lifecycle receipts.

Device 1 remains failed. Both old listeners reported network change and
successful local close, but the runner incorrectly required remote EOF within
30 seconds. It then failed partial analysis with a missing `start` field. Raw
captures and the original result survive. The corrected runner records each
milestone immediately and distinguishes explicit host cleanup from remote EOF.

Device 2 remains failed. It reached the new generation, then connected before
the admission worker had rebound its listening socket. That immediate attempt
received connection refused; a later request returned the expected stale-URL
503. The runner now retries only connection refusal within the existing
30-second reconnect deadline. A real delayed-listener regression and a refusal
deadline regression cover this readiness distinction.

Pinned IDF's disconnect handler lowers the interface; clearing its address causes
lwIP to abort old-IP connections and free their queued segments and PCBs. A reset
packet lost during radio loss cannot notify the remote host. Checked local close
plus fresh admission therefore tests board cleanup; remote EOF is a separate
observation. No firmware change was needed for either harness correction.

## Scope limits

This tests a same-IP reconnect. Changed-IP and delayed-observation semantics are
host-tested, not a real DHCP reassignment. Checked closes and fresh admissions
demonstrate released capacity, not a census of every TCP PCB or peak task count.
The two permanent admission workers remain fixed in source. Decoded complete
MP3 frames prove sample accounting for those bytes, not analog fidelity or audio
continuity across the intentional outage; partial final frames stay excluded and
reported. Startup capture faults remain separate from active test deltas.

Real Sonos TLS playback recovery and long-term network endurance remain separate
open work. This change adds invalidation, not automatic playback recovery.
