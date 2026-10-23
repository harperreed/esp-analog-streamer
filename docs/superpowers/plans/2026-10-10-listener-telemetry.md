# Listener-only 21tp implementation brief

Doctor Biz: remove synchronous logging from admitted listener progress/cleanup.
Defer encoder telemetry, probe export and capture restart logs to later units.
This follows the existing 21tp acceptance; it does not change recovery policy.

## Files and production types

- New firmware/src/listener_telemetry.rs: fixed records, queue, publication,
  short dequeue and host-callable production cleanup/report seam. No ESP-IDF or
  formatter calls. Import actual ConnectionResources if the seam closes it.
- firmware/src/lib.rs: module declaration.
- firmware/src/stream.rs: ListenerMetrics snapshot, ListenerConnection cleanup,
  listen departure, stream_compressed timeout and admitted buffered refusal.
  Keep EncoderMetrics and encoder/probe log/report ordering untouched.
- firmware/src/audio_in.rs: drain listener records in existing metrics task.
- core/tests/listener_telemetry.rs: import actual production modules, as replay
  tests do. Minimal stream_pipeline.rs seam only if needed; preserve its close.
- Existing fault/sender/probe parser modules and tests: loss marker and explicit
  delayed-event semantics. Locate current modules before edits; no guessed paths.

## Fixed representation

Use a capacity-eight array ring behind Mutex, with try_lock-only producers and
AtomicU32 wrapping drop counters. Full, contended and poisoned publication must
return immediately and count loss. No queue allocation after initialization,
new task, blocking send, producer formatting or per-frame event.

Record envelope: event_us u64 sampled at producer, peer SocketAddr, and fixed
payload enum. Replace ListenerConnection.peer String with SocketAddr if useful;
render only in exporter. No socket/Slot/Lease/guards/heap-backed errors in records.

Payloads:

- Departure: owner runtime delta/validity, elapsed_us, frames_sent, bytes_sent,
  frame_write total/max, frame_read_wait, payload_elapsed, block_samples,
  WriterDiagnostics, EndCode, optional io ErrorKind/raw OS error. Fixed end code
  covers existing Retired/FellBehind/StoppedReading/CaptureStalled/Failed/
  InvalidFrame cases. Inspect concrete invalid-frame error type before choosing
  its numeric/fixed representation; do not stringify it on producer.
- Left: elapsed tenths and same fixed end description, if not folded into the
  departure record. WAV must still report departure and close.
- Timeout: elapsed_ms, limit_ms, bytes.
- Refusal: fixed admitted-buffered-startup reason, status code; separate numeric
  response-write failure if one occurs.
- Close success: elapsed_ms and pre-release held count captured by actual close.

Keep departure's three existing stream-listener/stream-sender/stream-sender-poll
lines and existing left/close text in the consumer, adding event_us provenance
without changing old field names. Report actual size_of<Record>, queue/BSS cost
and target producer/metrics stack headroom. Capacity eight is concrete initial
bounded retention, not permission to exceed available internal RAM.

## Producer and consumer rules

Snapshot listener runtime on listener owner at its existing boundary; exporter
must never query itself and label that runtime as the listener's. Publication
must never call a logger. Consumer dequeues one record into a local, unlocks,
then formats/logs it; drain at most two records per metrics iteration. Do not
copy the whole ring onto the stack. A permanently blocked consumer can lose
records, but cannot delay checked close or encoder/capture progress.

listen currently logs before drop(connection). Replace that dependency with
snapshot/nonwaiting publication, then actual checked close. Successful
ConnectionResources::close already drops Slot and Lease before returning: retain
that fact and publish close result afterward. Genuine close failure retains
RetainedClose and follows the existing log-free esp_restart path. Preserve
spawn-failure cleanup and uncertain-descriptor ownership.

stream_compressed logs timeout before consuming its write result: publish fixed
values without waiting, then continue unchanged. Also cover its terminal
refuse_buffered -> send_refusal path, which currently logs before response and
again on write failure. Use a dedicated admitted-refusal helper or explicit
reporting mode so this path never logs synchronously. Preserve HTTP response,
HEAD_TIMEOUT and write policy. Audit the admitted send_refusal at admission
allocation failure as well. Pre-admission accept-thread logging is outside this
unit unless sharing the helper makes a small consistent change necessary.

## Truthful asynchronous evidence

Export cumulative listener-telemetry loss with a distinct machine-readable marker,
including observed event-time range where available. A delayed UART arrival is
never the event timestamp. Fault/sender/probe acceptance consumers must explicitly
reject relevant export loss, missing required close/departure evidence and
ambiguous measurement windows; never accept zero deltas from absent telemetry.
Preserve startup/probe log ordering because those records remain synchronous.
The real probe invalidation replay regression must remain unchanged and pass.
Do not make acceptance assume close happened when its delayed line arrived.

## Required red/green tests

1. Exercise actual production queue with full/held/poisoned storage, wrap and
   fixed record contents. Verify bounded loss and no producer formatting.
2. Real exporter thread dequeues then blocks on a synchronization barrier. Drive
   actual changed listener cleanup/report helper using ConnectionResources,
   real Slot and runtime Lease, and a local TCP pair. Verify checked close and
   Slot/Lease release while consumer is blocked. Reuse actual checked-close
   primitive where host-compatible; otherwise use owned TcpStream shutdown/drop
   with real observed peer EOF, not a mocked success return. No production
   caller may retain a hidden synchronous log before reaching this helper.
3. Fill/contend queue and repeat success cleanup, delayed close, and genuine
   failure ownership. RetainedClose must retain admission authority on failure.
   Existing replay ownership tests remain authoritative.
4. Actual timeout-result and buffered-refusal terminal production helper paths
   complete their existing bounded response policy and reach close while consumer
   remains blocked. Local real sockets/read EOF; no mocked logger/network.
5. WAV/compressed departures retain fields, failed publication never retains
   resources, Stop/format lease semantics stay unchanged. Production integration
   must use tested helper, not duplicate tested logic only in tests.
6. Parser regressions cover delayed arrival/event_us, explicit export loss,
   missing close evidence and unchanged sender fields. Re-run saved real probe
   invalidation replay; no encoder/probe production edits in this unit.

## Review and parent gates

Independent source review; parent canonical scripts/check; default/probe host
suites, target builds and existing linked-symbol/IRAM checks. Parent freezes
current encoder-control evidence before new device experiment. Record image
hashes, queue/static-memory delta, listener/metrics stack headroom, internal and
PSRAM free/largest/minima and export drops. Bounded matched device trial verifies
actual departure/timeout, slot reuse and capture counters; retain failures.
No extra task, suppressed diagnostics, watchdogs, allocation failures or invented
RAM allowance. Full 21tp remains open pending encoder slice and measured gates.
This patch alone does not claim wbz6/93hm root cause or completion.

References: Kata 21tp; active Kata hardening plan;
2026-10-10-deterministic-replay.md close ownership; current ConnectionResources
implementation and core/tests/replay_pipeline.rs regressions. Parent accepted
this bounded implementation under Doctor Biz's existing audit authorization.
Joined logging also moves off the admitted path because it currently runs after
slot acquisition and before spawning the listener. Unknown peers remain explicit
through Option<SocketAddr>; no fabricated address or admission change.
