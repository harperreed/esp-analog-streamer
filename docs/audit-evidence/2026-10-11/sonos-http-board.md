# Sonos HTTP board acceptance

Doctor Biz authorized checked Stop and real Sonos playback on October 11.
Firmware commit `87848e3` is installed and its reported ELF hash matches the
archived candidate:
`36d61e57439e102cc9b3c5b256ffeec8a7dddec9743a672b670a4aadce8fa2c3`.
The workstation front headphone jack supplied the existing 48 kHz stereo noise
fixture. No mixer or volume settings changed.

## Results

| Run | Verdict | Evidence |
| --- | --- | --- |
| playback-01 | Failed precondition | Actual Sonos was already playing FIP Cultes; no test source or Play started. |
| playback-02 | Failed gate | FLAC progressed, but the harness checked remote Stop too early. Cleanup confirmed STOPPED. |
| playback-03 | Passed bounded playback gate | FLAC/Medium, deferred MP3/High settings, MP3/High playback and checked Stop passed. |
| contention-01 | Failed coexistence | Sonos displaced the newer debug listener, but the older debug listener ended for media age. |

The initial radio playback was stopped as explicit, identity-checked test setup,
separate from automatic recovery and production ownership policy. Every run
completed cleanup without errors. The two failed playback receipts remain failed.

In playback-03, FLAC position advanced from 10 to 24 seconds and MP3 from 5 to
19 during their respective 15-second observation windows. Owner-bound UART
receipts recorded 372 FLAC frames / 3,362,340 bytes and 1,328 MP3 frames /
1,274,880 bytes. Saving MP3/High while FLAC played preserved the active FLAC URI;
the next Play used MP3/High. MP3 reset its first connection and rejoined after
checked release. Position and payload do not prove gap-free or audible playback.

Contention opened two actual FLAC debug readers and received HTTP 503 for a third.
The newer debug owner checked-closed at device time 20,958,524 us, before Sonos
joined at 26,607,553 us. All observed joins/releases held at most two slots.
The older debug reader then ended at 29,625,873 us for media age. Its longest
frame write took 3,535,870 us; its longest socket poll took 3,528,569 us, while
its maximum send call took 17,109 us. The sender spent most of that long write
waiting in socket polling. This evidence does not locate the cause in the
sender, AP, radio, or receiver. Sonos reached Playing and its position advanced;
that does not repair the failed older-listener assertion.

Both active runs had zero deltas across nine capture fault counters from the
first published post-reset snapshot to final cleanup. Those first snapshots
already contained 15/17 queue overflows respectively, 480 software-dropped
frames, two overflow restarts, 26,400 settling-discarded frames and two
discontinuities. Startup capture acceptance remains unmet. The passing playback
run observed listener stack headroom as low as 812 bytes and metrics stack
headroom of 792 bytes; these short runs do not establish worst-case headroom.

## Harness correction and verification

UART open can reset the board and clear its discovery cache. The gate now runs
board discovery before resolving the selected coordinator. Local Stopped can
become visible when cancellation revokes the intent, before the worker finishes
its checked remote Stop. The gate now sends one Stop and polls remote transport,
rechecking the exact URI, coordinator, image and boot on each iteration. It
rejects takeover without another write. The 15-second polling window is soft
across in-flight reads; the existing main and cleanup alarms bound execution.

Real loopback HTTP tests reproduce delayed completion, takeover, timeout,
post-reset discovery and configuration drift. The pre-fix tests fail; all 16
focused tests pass after correction. The throwaway contention driver's real
socket test passes. Sol 6.1 review and parent fresh-eyes review found no material
code blocker. Canonical `scripts/check` passed all 27 stages in
`target/check-logs/run.HvfuxuB4`, including firmware compilation and IRAM checks.
Only evidence and plan documentation changed after that verified source snapshot.

## Remaining acceptance and final state

Keep 7vhq and pqqq open. Coexistence failed, and this phase did not test controlled
Sonos reconnection, canceled startup, post-reservation startup failure, or
ownership-safe automatic media restart. Client retries during startup are not
controlled outage recovery. No AP/forwarding control point for selective faults
on the actual board-to-Sonos path was established; workstation packet rules
would not validate that path. Keep n4jb and pd6c's original requirements.

Before release, resolve the failed coexistence and startup capture gates, then
run controlled interruption/recovery and sample-accounted long soaks. Increasing
the age budget alone would not diagnose the observed socket wait. No Kata was
closed from these partial results.

Independent final checks observed stopped board and Sonos, active/saved MP3,
Medium buffer, autoplay and recognition off, no aplay process, and the same
candidate image on boot `392f8ea5024169db6a12018576a17c2a`. The candidate remains
installed. Raw private receipts, image, driver, tests and hashes live under
`target/kata-hardening/sonos-http-board/`; the companion JSON retains result and
artifact hashes, capture deltas, and final state.
