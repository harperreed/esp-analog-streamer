# Listener ownership and runtime commit models

2026-10-10, Kata jq3a. Host instrumentation of the actual `Slot` in
`core/src/stream_wait.rs` and actual `Runtime::admit` / `Worker::commit` in
`firmware/src/stream_runtime.rs`. Production operations and atomic orders are
unchanged. This is bounded host evidence, not a deployed listener-recovery claim.

## Isolation

`listener-loom` is opt-in and uses the existing pinned Loom 0.7.2 dependency.
Only Slot's Arc/counter and runtime's Arc/mutex/atomics change under that feature.
Timed read helpers retain std locks. Ordinary tests retain their default std
synchronization and run independently. The feature is rejected on ESP-IDF and
cannot be combined with `capture-loom`.

The firmware manifest declares the feature, forwarding it to core, solely so
shared-source cfg checks recognize its name. Core enforces the host-only boundary.
The default target dependency graph gains no Loom dependency. No helper framework,
fake socket, copied admission algorithm or replacement runtime was introduced.

## Red and green

Tests and explicit Loom ownership type witnesses preceded import instrumentation.
The initial command exited 101 with E0308 mismatches: actual Slot::take accepted
std Arc/AtomicUsize, and runtime token/active mutex types were std. That is an
instrumentation failure, not evidence of a pre-existing listener behavior bug.
Full initial output: `target/kata-hardening/loom-red.log`.

After the cfg imports and ordinary-test isolation:

```sh
cargo test -p esp-shazam-core --features listener-loom --lib --test listener_runtime_loom models:: -- --nocapture
```

The integration target discovered and passed all ten models. The library target
had zero matching models, as intended; models live in the integration target.
Full output: `target/kata-hardening/loom-green.log`.

| Model | Schedules |
| --- | ---: |
| Slot saturation, three contenders | 9,896 |
| Slot release versus admission | 30 |
| Slot normal and early worker return | 35 |
| Unlaunched closure destruction versus admission | 30 |
| Token revocation versus actual commit | 20 |
| Admission versus idle commit | 26 |
| Admission versus forced commit | 20 |
| Expired request refusal | 1 |
| Completed outcome refusal | 1 |
| Outcome revocation versus actual commit | 20 |

Every model sets max_threads=4 including the parent, max_branches=1000,
preemption_bound=2, max_permutations=10000 and checkpoint_interval=1. Duration and
checkpoint paths are cleared to prevent environment-driven early termination or
resume. A std atomic outside the modeled protocol counts schedules only; it
never participates in a correctness oracle or controls a modeled thread.

Loom 0.7.2 starts its iteration index at one and checks the permutation cap before
executing that iteration. The harness conservatively rejects 9,999 schedules or
more. All runs finished below that cutoff, so the permutation cap did not truncate
them. Exhaustion applies only within the stated bounds. Saturation is close to
the cap; future model growth may require an explicit bound change.

Assertions hold admitted leases through joins, then check epoch identity,
retirement, outcome and listener count. A forced switch may retire an old leased
epoch; an idle switch cannot replace an epoch already leased. Guard/lease drops
return their distinct counters to zero. Strong counts after dropping runtime and
worker owners check old-epoch retention; Loom also checks modeled ownership leaks.

The ordinary `actual_failed_launch_drops_captured_listener_slot` test uses a real
std Builder stack rejection with a captured real Slot. Count zero and one remaining
Arc owner establish that the failed launch releases its captured guard. This
characterizes existing std behavior; the unlaunched-closure model explores its
ownership branch racing admission, not an actual pthread failure.

## Other verification

All commands used the pinned mise host environment:

- `cargo test --workspace`: 370 passed, zero failed, two existing ignored tests.
  Ignored: `decimate::tests::print_taps` (coefficient printer) and
  `song_clip_matches_upstream_byte_for_byte` (requires the song fixture created by
  `scripts/e2e-host`). Full output: `target/kata-hardening/loom-ordinary.log`.
- `cargo clippy -p esp-shazam-core --features listener-loom --all-targets -- -D warnings`:
  passed. Full output: `target/kata-hardening/loom-clippy.log`.
- `cargo test -p esp-shazam-core --features capture-loom models:: --lib`:
  both existing capture models passed independently. Full output:
  `target/kata-hardening/loom-capture-isolation.log`.
- `cargo check -p esp-shazam-core --features listener-loom,capture-loom`:
  rejected with the intended separate-invocation compile_error. Full output:
  `target/kata-hardening/loom-mutual-exclusion.log`.
- `cargo fmt --all -- --check`: passed. Full output:
  `target/kata-hardening/loom-format.log`.

Independent task and combined reviews found no blocking defects. The parent ran
all 21 canonical stages successfully, including the ESP-IDF firmware build.
That build added optional host Loom packages to firmware/Cargo.lock; review found
no existing package upgrades, and the default ESP-IDF dependency tree excludes
every added package. Log: target/kata-hardening/check-second-batch.log.
The model implementation performed no device, UART, source-audio or network
operation; the parent's separate sender measurements do not validate recovery.

## Limits

The models exclude FreeRTOS task creation, ESP-IDF close/FFI, ISR timing, network
behavior, allocator failure, audio throughput and hardware ordering completeness.
The mpsc notification is an ordinary nonblocking send whose result commit ignores;
channel delivery is not modeled. Expiry is a deterministic initial condition,
not a wall-clock scheduling proof. Token revocation is the cancellation boundary
modeled; command acceptance and final TLS sends remain ordinary-test concerns.
Rust Arc counts do not prove socket PCB or PSRAM reclamation. Real socket close
paths and deployed recovery still require their ordinary socket and HIL evidence.
