# Host mutation diagnostic

`scripts/mutation-host` checks whether the capture-health tests detect changed
production behavior. It needs a clean committed tracked tree and the exact
mise-managed cargo-mutants 27.1.0 pin. Run `mise install` to provision tools,
then run the script from the repository. Mise activation may install missing
repository-pinned host tools; provision them before time-sensitive work.

The pilot mutates only `core/src/capture_health.rs` and runs its real unit tests
with host Rust 1.93.0 and default features. These tests cover progress, the stall
and recovery deadlines, one restart, terminal reboot, recovery rearming,
wrapping counters, and positive constructor durations. No board access occurs.

Each run archives committed HEAD into a private fresh directory below
`target/mutation-host/`. Ignored and untracked files are omitted. Builds and the
tool's additional scratch copies use run-local directories. The normal checkout
is never a mutation input directory. Tracked changes must be committed first;
this prevents accidentally measuring stale source or tests. Run artifacts are
not firmware build or flash inputs.

The runner retains versions, host platform, revision, archive and lockfile hashes,
full commands, an ordinary unmodified baseline, the generated inventory, tool
baseline, mutation diffs, logs, and exit statuses. It rejects zero or more than
60 candidates. One mutation worker runs with explicit 60-second command and
180-second build limits, a 1,500-second mutation-stage limit, and a 1,800-second
overall limit. The ordinary cold baseline has a 300-second limit. A timeout can
reflect a budget that needs adjustment; inspect logs before drawing conclusions.
GNU `timeout`, `sha256sum`, Git, tar, Node, mise and the pinned Rust tools are
required. This pilot supports Linux only and fails with a useful message on
other platforms. Its lightweight canonical check runs only on Linux too.

`summary.txt` keeps caught, missed, unviable and timeout counts separate. It
compares per-scenario classifications with the tool's totals and the inventory.
A failed baseline, missing outcome, unknown class or incomplete run fails.
`status.txt` stays `incomplete` until results reconcile. `complete` describes
execution, not test quality: missed mutants and timeouts still return the tool's
nonzero exit status. Unviable mutants cannot establish assertion strength.

Inspect each meaningful survivor's diff and log. Add a real input sequence and
expected actions to ordinary production tests; show it fails with the mutation
in an isolated copy and passes without it. Commit the regression and rerun this
pilot. Keep equivalent or out-of-contract survivor explanations in the report,
with the actual diff and reason. Do not suppress survivors to improve a score.
For representative caught mutations, confirm the intended assertion caused the
failure rather than an incidental panic.

`scripts/test-mutation-host` tests the runner's safeguards with real temporary
repositories, files and child processes. It does not substitute a fake Cargo
binary or compile mutants. The real pilot separately validates pinned-tool
compatibility and execution. Keep the diagnostic separate from ordinary checks;
only its lightweight safeguards belong in `scripts/check`. A separate manual or
scheduled CI job can call the diagnostic and retain its artifacts on failure.

This pilot does not test ring generations, listener limits, codec checks,
network/hardware timing, Python decoder assertions, or every recovery invariant.
Keep direct harness tests and real negative audio fixtures. Expand the bounded
scope after reviewing the initial pilot. Mutation testing measures assertion
strength; it does not prove the unmutated program correct.

Tool references: [27.1.0 CLI](https://github.com/sourcefrog/cargo-mutants/blob/v27.1.0/src/main.rs),
[27.1.0 MSRV manifest](https://github.com/sourcefrog/cargo-mutants/blob/v27.1.0/Cargo.toml),
[outcome meanings](https://mutants.rs/using-results.html), and
[timeouts](https://mutants.rs/timeouts.html). The tool declares Rust 1.88 minimum,
below this repository's host 1.93.0 pin; installation and a real pilot remain
necessary compatibility checks.
