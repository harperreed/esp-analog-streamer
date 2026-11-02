# Stream property replay

`core/tests/stream_properties.rs` exercises production `CaptureBuffer`,
`StreamRing`, `FlacRing`, `Slot` and firmware `Runtime`. The ring oracles use
append-only logs rather than circular storage. Two readers have separate log
positions and generation identities. Generated operations cover capacities of
one through six blocks/frames, raw block sizes of one through five stereo
frames, partial and empty reads, half stereo inputs, invalid encoded frames,
wrap, overrun, reset, retained-history joins and reconnects.

Listener sequences use real admission guards and runtime leases. They check
that cancellation releases a temporary guard, a third guard cannot enter,
same-codec Play respects cancellation and full admission, and the final lease
releases the epoch allocation. Format-switch races remain covered by
`core/tests/runtime_switch.rs`; sequential properties do not establish thread
scheduling or device behavior. No property opens a network socket or device.

The normal budget is 512 cases per property with seed `15966833`. Each ring
sequence has fewer than 180 operations; guard sequences fewer than 240; runtime
sequences fewer than 40. Environment overrides use the pinned proptest 1.11.0
`PROPTEST_CASES` and decimal `PROPTEST_RNG_SEED` settings:

```sh
mise exec -- cargo test -p esp-shazam-core --test stream_properties
PROPTEST_CASES=512 PROPTEST_RNG_SEED=15966833 mise exec -- cargo test -p esp-shazam-core --test stream_properties
```

`config()` preserves default shrinking limits and uses
`FileFailurePersistence::WithSource("proptest-regressions")`. Failures persist
beside the test as `core/tests/stream_properties.proptest-regressions`; keep
real failure files in version control. Each subsequent invocation replays those
cases before generating fresh cases. A successful run creates no failure file.
Do not set `PROPTEST_DISABLE_FAILURE_PERSISTENCE` for normal verification.

Use a separate larger scheduled budget, with changing recorded seeds:

```sh
PROPTEST_CASES=4096 PROPTEST_RNG_SEED=20261010 mise exec -- cargo test -p esp-shazam-core --test stream_properties
```

For a failure, retain the printed minimal input, seed, budget, Rust version,
commit and regression file. Replay the original command and then narrow to the
reported property (for example `raw_history_matches_log -- --exact`). The
canonical `scripts/check` runs `cargo test --workspace`, which discovers these
properties automatically. The host dev dependency leaves firmware dependencies
unchanged.

The October 10 validation used Rust 1.93.0. Four temporary negative controls
corrupted one oracle assertion per property and each failed with a shrunk
counterexample. Those runs disabled persistence only to avoid recording
intentionally false expectations. All changes were restored before the passing
suite. This is coverage of existing behavior: no production implementation was
written before a failing test, and no production mutation was needed.
