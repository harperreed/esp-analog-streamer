# Live listener metrics acceptance

The original amqe metrics scope is verified. The separate 48-panel dashboard and
bounded collector expose capture/publication, encoder progress, individual
listener media age/write duration, network generations/disconnects, heap geometry
and measured stack headroom. This is not a streaming release or soak verdict.

## Implementation and checks

Two anonymous reusable registry slots observe MP3, FLAC and WAV payload writes.
Owners publish with try-lock copies; failed publication invalidates the prior
revision, and Drop releases ownership without acquiring that lock. Failed writes
report Unavailable, not a successful completion. Ready retains its completion
time; Writing measures elapsed whole-write wall time, including scheduling.

The HTTP handler copies those observations after releasing metadata/cache guards,
then records served time. Cached capture/encoder families keep their original
acquisition times. This avoids making live listener status depend on the logging
task. Tests reproduce that dependency and require advancing write timing and Idle
after release while the capture cache remains unchanged. A real stalled TCP test
and held-lock tests cover invalidation and cleanup.

Canonical `scripts/check` run `target/check-logs/run.X4QTsf98` passed all 27 stages,
including host tests, Clippy, firmware build, IRAM and warnings checks. Independent
reviews approved the final source. The focused Rust set passed 97 tests. Snapshot
is 656 bytes, Cache 664 bytes, and maximal host-generated device JSON 4074/4096
bytes; target size assertions passed. Host exposition is capped at 12288 bytes,
with meaningful HELP/TYPE text and default Prometheus lint enabled. Identity stays
outside metric labels; no peer, credential or audio enters the export.

Checked and installed ELF SHA-256:
`2b28be214c224182bb736a122d33757cfd0e4cb26d43c78d6ef4e5dc4b26fbbd`.

## Focused real-device proof

`live-run-4` completed with exit 0. Its exact throwaway driver was archived before
execution. Both observed boots matched the ELF and had different boot tokens;
subsequent reads required the second token. Input was the existing synthetic
mono-correlated noise fixture, hash recorded in the structured evidence. Normal
MP3 and WAV clients used unbuffered debug URLs. Medium remained the next-Play
setting; it does not attest buffering on those debug connections.

- Both normal MP3 clients delivered payload without driver errors. Planned close
  produced an Idle observation, fixing the failed prior cleanup check.
- A separate 8192-byte effective receive-buffer client paused for 3.645 seconds.
  Two samples of the same unfinished write advanced from 257974 to 1428962 us;
  media age advanced by the same 1170988 us. One bounded sampling URLError was
  retained. Reads resumed with 4360 payload bytes, then the client closed as
  planned. This does not prove sustained recovery with that small window.
- All 30 sequential metrics requests returned 200. The largest body was 2928
  bytes, minimum HTTP stack watermark 6960 bytes, and longest request 1.082 seconds.
  This is a bounded burst, not a proof for every possible HTTP load.
- Both slots became Idle. A real WAV client reused a slot with a new generation
  and delivered 30284 bytes, with actual payload timing observed.
- The exact 1985280-byte departure receipt reached Prometheus and Grafana's
  datasource query. Rapid HTTP reads advanced served time while preserving the
  unchanged capture sample.
- Across the 50.982-second span between retained UART capture acquisitions,
  2447040 PCM frames advanced and all nine capture-fault deltas were zero.
  Startup already had 15 overflows, 480 dropped frames and two restarts. Ten-second
  phases cannot establish an overhead comparison or sample-exact continuity.

Stock Firefox rendered the actual listener and HTTP graphs in the private Grafana
13.2.3 / Prometheus 3.13.4 lab. The initial query captured two active, available
listeners. Later instant queries were empty during the collector transition;
retained panel values are history, not proof of current availability at that later
instant. See [listener screenshot](live-listener-dashboard.png) and
[HTTP stack screenshot](live-listener-http-stack.png).

The lab used rootless processes and loopback listeners after checking connectivity
and port ownership. Existing Navigator telemetry and Docker policy were untouched.
Both lab runs, browser, collectors, source process and UART/listener threads were
reaped. Final device state was MP3/Medium/stopped, autoplay and recognition off,
with both observation slots Idle. No mixer controls changed.

## Failed trials and remaining release risks

The earlier 120-second comparison remains in the evidence. A tiny receive window
first contaminated its clean phase. With normal windows, both MP3 clients then
completed 120 seconds, but live observations went stale: logging-owner work had a
54.1-second acquisition gap, and planned cleanup did not become visible. Sampled
Ready states did not prove a blocking lock. The HTTP sampling regression fixes
that dependency, without claiming the underlying scheduling delay is solved.

A third trial on the final image lost one normal MP3 client before the collector
started. Preserve that failed verdict under transport investigation pd6c; do not
claim this dashboard proves reliable streaming. The final focused test verifies
the original metrics requirements instead of making this Kata depend on a longer
playback-stability test it does not own.

P1 startup capture failures, transport interruptions and ownership-safe recovery
remain release blockers in their existing Katas. Fix those and pass the planned
continuity/recovery soaks before shipping. P2: cached logging families can still
be delayed; the dashboard exposes staleness rather than inventing progress. The
metrics task's observed minimum stack watermark was 824 bytes, not a worst-case
stack proof. The device JSON has only 22 bytes of maximal-fixture headroom; future
fields must retain the bound tests.

[Structured results and artifact hashes](live-listener-metrics.json) retain the
successful and failed trial identities. Raw UART/audio and temporary credentials
remain in ignored `target/kata-hardening/live-listener-metrics/`.
