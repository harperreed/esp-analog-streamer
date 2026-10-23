# Pipeline progress and media age (6vct)

Status: committed as f5592cf; Kata 6vct closed. Open count verified 21 → 20.
CRC oracle fix committed separately as b26cc37. Base 2edaa55.
Original `kata show 6vct` acceptance remains
authoritative. Host, bounded device policy, sample correlation and overhead
measurement are complete. Failed transport trials remain failures on pd6c.
Earlier progress entries below are a historical log, not current blockers.

## Global constraints

Use existing capture and encoded rings as the authoritative stage records.
Capture must never wait for a reader, acquire a reader-held mutex, allocate per
block, or use unsupported 64-bit atomics. Preserve sequence/era exhaustion checks
and validate sample metadata with the same publication protocol as sample bytes.
Use software acceptance timestamps, not claimed ADC timestamps or speaker delay.
Use the same boot-monotonic microsecond domain throughout the device pipeline.
No Sonos Play/Stop, capture restart or reboot may originate from encoder health.
Preserve manual Stop, format epoch ownership and checked socket reclamation.
Preserve MP3/Medium/stopped, autoplay and recognition off on the test device.
Parent alone owns canonical checks, target builds, device/UART, commits and Kata
updates. Workers use focused host tests, disjoint ownership and no extra agents.
Preserve user-owned .kata.toml, docs-ignore and the untracked device-metrics plan.

## Decisions

Ruling: use four seconds of extra accepted-source age beyond the selected rounded
startup history. The optional preference question remained unanswered while
independent design work completed. Low/Medium/High are approximately 5/8/12 s;
unbuffered listeners receive 4 s. A wrong allowance costs a policy adjustment;
it must not change selected presets or be presented as measured ADC/playout age.

The effective payload deadline is the earlier of its existing socket deadline
and oldest represented acceptance time plus media-age limit. Headers have no
media age. Reject invalid/future metadata and stale complete frames; never splice
or skip partial output. Record bytes actually accepted even if the final send
returns late; end the stream after a late completion with an explicit age reason.
lwIP may block inside a send despite nonblocking mode, so this is a bounded
application policy, not a claim that we can abort an in-flight syscall.

Encoder health uses committed raw/output identities. Two seconds of no encoded
publication while enough current raw input advances triggers one cooperative
reset request. One two-second grace allows encoder-owned reset and real output;
otherwise latch a local compressed-stream failure. Inactivity, partial staging,
unavailable coherent snapshots and generation replacement do not prove a stall.
Repeated reset acknowledgements cannot extend the recovery budget. Observe from
the existing supervisor without taking ring/runtime locks. Stream admission and
listeners must see unhealthy state without those locks too.

## Units

1. Raw stage metadata (worker raw_media_metadata_61). Extend CaptureBuffer and
   StreamRing with independently visible accepted input and committed publication
   receipts, plus timestamped block reads. Update all direct callers/tests to the
   single new API; no untimed compatibility path. Keep timestamps with the same
   PSRAM payload allocation to avoid a new per-slot internal-RAM allocation.
   Coherent header snapshots carry accepted count/time and committed count/time.
   Acceptance becomes visible before block copying; preflight all exhausted
   identities first. Partial staging keeps the oldest acceptance time. Publish
   samples with a supplied acceptance time and injected clock for commit-boundary
   timing. Device capture supplies time only after overflow/settling rejection.
   Scope includes mechanical read-result adaptation in pipeline/WAV/replay, but
   not encoded metadata or new listener policy. Tests: partial staging, concurrent
   overwrite/reset, timestamp halves, sequence/era limits, accepted-vs-published
   progress, existing property and Loom invariants.

2. Pure policies (worker media_policy_61). Add small core media-age and encoder
   health policies with deterministic tests; no runtime wiring in this unit.
   Public inputs use ordinary coherent snapshots supplied by caller; no clocks,
   locks, telemetry queues, platform dependencies or new threads in policy code.
   Cover exact deadlines, checked time arithmetic, insufficient input, silence,
   inactivity, reset/wrap, progress at deadline, one reset and latched failure.

3. Integration (after units 1/2). Carry oldest accepted timestamp and source span
   through codec accumulation into encoded-ring records under existing mutex.
   Clear accumulation on gaps/switches. Successful ring commit alone advances
   output health receipt. Wire cooperative reset/ack and lock-free health gate;
   adapt existing supervisor startup wiring without a second watchdog thread.
   Enforce age in compressed and WAV payload writes; extend existing deferred
   telemetry with authoritative stage/age/reason observations. No parallel source
   of progress, high-frequency formatting or critical-path logging.

4. Acceptance. Extend production replay with real codecs, withheld encoder steps,
   advancing accepted input with stalled publication, slow writes, generations
   and cancellation. Reuse actual decoder oracle. Parent runs scripts/check,
   reviews source, builds and runs finite default/fault device cases and matched
   overhead measurements against the archived normal image. Retain decoded sample
   counts/source ranges, fault timing, memory/stack/runtime and cleanup receipts.
   Existing unrelated failures remain failures; do not require resolving all TCP
   delivery or Sonos ownership work to validate this bounded local policy.

## Progress

- Design review: two independent Sol 6.1 reviews completed; current history is
  12 s, unlike the ticket's historical 8 s premise. Source age and queue lag are
  separate. The device send can complete late and needs explicit final age check.
- Unit 1 implemented and independently approved: 93 focused tests, bounded Loom,
  core clippy and fuzz caller compilation pass. The initial RED was a missing-API
  compiler failure, not a runtime assertion failure; report preserves that limit.
- Unit 2 implemented and independently approved after two fixes: duplicate raw
  observations preserve stall evidence, while a full inactive window and delayed
  resume start a fresh window. Three age and sixteen health policy tests pass.
  Final health tests ran standalone while integration temporarily changed APIs;
  integrated Cargo verification remains required.
- Unit 3 is implementing encoded metadata, real age enforcement, epoch-bound
  atomic health transitions and supervisor wiring. Unit 4 independently updates
  the existing encoder-fault fixture/harness for a 2.5 s cooperative pause and
  17 s terminal pause, with one boot and a 30 s recovery interval.
- Initial normal-image two-listener MP3 baseline passes 30 s / 1,440,000 decoded
  stereo frames per receiver, zero post-startup capture-fault deltas. Quiet input
  is not controlled waveform evidence. First collector attempt used the Mac
  default port and failed before reset; both receipts remain under
  target/kata-hardening/media-progress/baseline. Correct Linux override is
  ESPFLASH_PORT=/dev/ttyUSB0. The controlled MP3/FLAC/MP3 matrix passed all three 90 s staged trials using
  the retained runner and 420 s source fixture; it preserved gain and playback
  state. Matched measurements on the instrumented image remain pending.
- Integration, canonical checks, hardware acceptance, review, commit and closure
  remain. No completed subtask is sufficient to close 6vct by itself.

- Unit 3 source review approved; canonical run.hNQEKAnc exposed missed raw/Loom
  callers, lint/format errors and a replay terminal-reason migration. Those
  corrections are in progress. Firmware build and effective IRAM checks passed.
- Unit 4 harness fixes passed scoped independent review. The full decoder run
  exposed an interleaved CRC diagnostic false negative; exact regression RED,
  narrow oracle fix, 17-test GREEN and independent review are recorded. Original
  failed raw evidence remains unchanged.
- Canonical run.T5ChmZFd and run.o4BUOtRP passed every stage after source fixes.
  Final integration and amended-source reviews found no remaining code issue.
- First device run stopped on an obsolete startup-log expectation before faults.
  Second run observed bounded cooperative recovery, but the validator wrongly
  required acknowledgement and new output to share a generation. Reset ack keeps
  the last actual publication; only real output advances that receipt. Both raw
  failed runs remain under target/kata-hardening/media-progress/device-fault.
  The validator is being corrected against the actual event trace before rerun.

## Final acceptance and handoff

- All implementation and amended harness reviews approved. Canonical
  `scripts/check` run.ECoUgIPy passed every stage on final source, including the
  extended WAV harness. That does not make the actual extended WAV trial green.
- Real MP3 fault run 20261011T065216.651426Z passed bounded recovery, terminal
  admission, source/sample correlation and post-baseline capture protection.
- Matched overhead measurements completed: MP3 pass / FLAC fail / MP3 pass.
  FLAC explicitly reached its media-age limit; it remains a delivery failure.
  Encoding stayed ~1x with zero post-baseline capture fault deltas. CPU, PSRAM,
  internal heap and reduced free-stack measurements are retained in evidence.
- Same-boot extended WAV run 20261011T070109.797223Z failed continuous delivery
  on the socket deadline after decoding PCM. Both failures belong to pd6c;
  neither is silently waived, relabeled, or made a new universal transport
  requirement for this bounded-policy ticket. Independent closure review agrees.
- Normal firmware restored and live state verified MP3/Medium/stopped, autoplay
  false, recognition false. Source player and UART collector exited.
- Durable evidence: docs/audit-evidence/2026-10-11/media-progress.md and JSON;
  original CRC failure diagnostic preserved in decoder-crc-diagnostic.json.
- Fresh-eyes review found no additional source issue. Commit this phase, close
  only 6vct against its original acceptance, confirm 20 open, then retire the
  phase. Overall goal remains active. Next work must pick one original Kata
  acceptance and finish it; do not turn pd6c observations into an open-ended
  prerequisite for every other ticket. Compactions: at least three; phase ends
  here under the session-lifecycle rule.
