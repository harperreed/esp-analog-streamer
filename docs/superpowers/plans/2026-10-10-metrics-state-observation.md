# Observe delayed metrics exports

Kata 21tp/wbz6; base 073251e, firmware base eb38c65. Doctor Biz authorized
continued hardening with clean-context Sol 6.1 workers. Parent owns integration,
commits, canonical checks and all hardware. No subagent hardware operations.

## Question and evidence

The default slow-reader trial loses an encoder record while exported windows
are ten seconds apart. The canceled probe repeats raw overruns without media
listeners. Stack isolation improved observed probe metrics headroom to 908 bytes;
default remains 1148. These minima do not certify safety or explain the delay.
Determine whether low-rate samples see metrics Ready or Blocked during an export.

Pinned source/linked review is target/kata-hardening/metrics-delay-design-review.md.
Its final narrower contract supersedes earlier broader diagnostic proposals.
The permanent metrics loop cannot return; pinned ESP-IDF panic unwinding aborts.
Publishing its task handle once is safe only under that lifetime invariant.
vTaskGetInfo with pdFALSE/eInvalid performs a real kernel critical section and
nested state lookup. The call has no hard latency bound; measure observer cost.

## Implementation unit

One worker owns a small actual production core observation module and its tests,
core/src/lib.rs registration, and firmware/src/audio_in.rs wiring. No other
production files unless the parent approves an explicit dependency. Preserve
priorities, stacks, export order, queue capacities, capture and stream policies.

- Metrics publishes its permanent task handle once using the existing capture
  identity pattern, and one independent AtomicU32 phase. Never query null.
- Phases: Init (setup/housekeeping), HealthWarning, Capture, RestartExport,
  ListenerExport, EncoderExport, DiagnosticExport, Sleep. Set Init before setup
  and immediately after waking. Cover every existing synchronous logger; mark
  Capture only when log_capture actually runs. Do not move existing operations.
- Supervisor handles capture health first. At most once per elapsed second,
  with no catch-up, read phase, query the nonnull task using existing bindings
  and pdFALSE/eInvalid, then read phase again. Keep TaskStatus storage on the
  supervisor. Reboot handling must not wait for diagnostic sampling.
- Count per-phase Ready/Blocked/Other samples. Different or invalid phases are
  unassigned; never index unchecked. Equal before/after values do not prove
  continuous occupancy (ABA is permitted and documented). Ready can include a
  task preempted by this higher-priority same-core sampler.
- Keep attempted/unavailable/unassigned counts, last raw state, maximum query
  wall microseconds, and saturation/invalidation flags. No runtime, priority,
  task identity, wall-context tuples, histories, queues or timestamps per phase.
  Deleted/changed identity invalidates further queries; lifetime safety comes
  from the permanent-task proof, not this after-the-fact check.
- One supervisor writer uses local saturating arithmetic and atomic stores.
  No CAS retry loops, reset-on-read, seqlock or coherent snapshot framework.
  All shared payloads are atomic. Unknown states remain Other with raw value.
- Export cumulative observations from metrics after existing exporters, at
  most once every ten seconds. Use explicit primitive formatting in out-of-line
  leaves to preserve stack margin. Keep receipt text compact; avoid nested Debug.
  Define a fixed schema and a completion line with board acquisition start/end
  times for the dump, so incomplete output cannot masquerade as full evidence.
  Values are independent atomic observations: sums need not equal attempted.
  The dump is neither a coherent snapshot nor time spent in state.
- Preserve existing sticky-loss receipts and validators. New observations are
  diagnostic only. For an interval used for inference, the later complete dump
  must start acquisition after that interval ends; completion time alone does
  not cover earlier-loaded fields. Absent output, invalidation or saturation
  leaves evidence incomplete.
- Measure the added supervisor stack use: publish its permanent task handle,
  with the same nonreturning/abort invariant, and append supervisor stack validity
  and minimum free bytes to the completion line. Only low-priority metrics calls
  the existing uxTaskGetStackHighWaterMark on a nonnull handle. Pinned Xtensa
  StackType_t is u8; the direct fill-byte scan is preemptible and does not take
  the kernel critical section. Never add a stack scan to the supervisor sampler.

## Tests and gates

TDD on the actual production module: phase/state attribution, changed/invalid
phase, unavailable identity, saturation, invalidation and at-most-once/no-catch-up
deadline arithmetic. Use direct numeric inputs, not fake FreeRTOS or mocked
end-to-end claims. Preserve existing health tests and manually review wiring.
Run focused host tests/Clippy/formatting, then freeze for independent task review.

Parent runs scripts/check and both default/probe target builds and IRAM checks.
Inspect exact new static bytes, metrics/supervisor entry and nested call frames,
and atomic lowering. No stack size increase to accommodate the observer.
Only after build/layout review, run matched bounded default/probe hardware with
the existing source and routing, exact ELF/config hashes, original failed trials
retained and cleanup proven. Measure query wall maximum and supervisor/capture
health against control; assess receipt completeness separately from delivery.
Raw loss, failed delivery and existing telemetry loss remain failures.

Status: implemented and independently reviewed. All 27 canonical checks, both
target builds/IRAM checks and exact linked review pass. Parent fresh-eyes clear.
Build receipt: docs/audit-evidence/2026-10-10/metrics-state-build.md. Diagnostic
usefulness and observer overhead require hardware; neither Kata closes from
compilation. Parent proceeds with bounded matched device measurements.
