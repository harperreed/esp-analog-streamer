# Intermittent capture stall during Sonos TLS setup

Status: unresolved P2; investigate before shipping. No root cause or fix established.
Evidence concerns saved normal image `9fd2`; current source inspection is not build provenance.

## Saved observations

- Bad run: `target/sonos/listen-1791407567/uart.log`.
  TLS setup: uptime 13912–14992 ms; encoder overrun at 14952 ms.
  Overflows 16→38, dropped frames 480→35040, restarts 2→3, discontinuities 2→19.
  Maxima: raw read 457895 µs, raw work 150187 µs, staging pending 914827 µs.
  Parent localized faults before listener joins
  at 17802/21622 ms; subsequent 120-second steady playback added no faults.
- Clean run: `target/sonos/listen-1791407833/uart.log`.
  TLS setup: uptime 13893–14833 ms; counters stayed 16/480/2/2.
  Post-connect: raw read 621 µs, raw work 1348 µs, staging pending 5237 µs.
- The failing harness baseline does not establish sustained two-listener overload.

## What the measurements mean

Core-1 encoder `stream::read_some` measures raw-read wall time inside the ring lock:
it includes preemption/stalls during the copy, but excludes mutex acquisition,
waiting for fresh audio, encoding, socket work, and the final unlock tail.
Capture uses `try_lock` and preallocated staging; a delayed lock holder can amplify
a stall into staging overflow and gaps. Pending time spans repeated attempts;
it is not CPU execution time or proof of one continuous lock hold.
Interval maxima do not timestamp each fault or identify the original stall source.

## Hypotheses and next evidence

The Core-0 application worker does not pin IDF's separate WebSocket task; current
configuration leaves it unpinned. Priority 4 alone cannot starve capture priority 11
or encoder priority 7. ESP32 AES/SHA and heap paths contain critical sections;
MPI uses a mutex. No whole-handshake interrupt blackout or cache-disable event is proven.
Raw copies/staging do not allocate per chunk; direct PSRAM allocation there is unlikely.
Next diagnostic: timestamp anomalous ring-copy/lock-hold intervals and overflow deltas
on the device clock, record TLS task core and handshake phase, and compare copy wall
time with task runtime. Use bounded in-memory records, not logging inside critical paths.
Repeat connection-only trials under the same capture load before changing affinity,
crypto, allocator, or buffer sizes; correlate critical-section/cache events if needed.
