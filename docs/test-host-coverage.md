# Host recovery coverage

`scripts/coverage-host --help` describes the standalone diagnostic. Install the
pinned tools through `mise install`, then run `scripts/coverage-host` when host
builds and tests will not disturb hardware measurements. The wrapper's mise
activation may install missing repository-pinned host tools. It then checks
cargo-llvm-cov 0.9.1 and Rust 1.93.0's matching LLVM tools and fails if they are
unavailable. The tagged tool requires Rust >=1.87 and Cargo >=1.60. The October
10 Linux pilot verified actual compatibility: 380 tests passed, zero failed and
two explicitly ignored; detailed JSON and HTML reports were generated. See
[pilot evidence](audit-evidence/2026-10-10/host-coverage.md) for scope and limits.

The mise pin uses rustup's canonical component name `llvm-tools`. Rustup accepts
the `llvm-tools-preview` alias, but mise 2026.9.15 reports Rust as missing when
that alias is configured even after installation. Both names select the same
Rust 1.93.0 tools; the canonical name gives clean activation.

Each invocation creates a private `target/coverage-host/run.XXXXXXXX` directory.
It holds the instrumented build, command log, tool versions, source revision,
working-tree diff/status, stage logs, detailed JSON and HTML. No prior profiles
are reused. Preserve the directory on failure and follow its named full log.
Old run directories are not automatically deleted. Do not use a symlink for
`target` or the coverage output path. Run on a clean committed tree for durable
CI provenance: untracked source contents are not captured by `git diff`.

The default host workspace includes core, tools and mp3. The run covers Rust
unit and integration tests with default features. Review capture-health restart
and reboot boundaries, raw/encoded reset errors, bounded wait retries, socket
close/deadline/flag restoration, listener-slot release, cancellation and epoch
retirement. Tests already use real loopback sockets and actual failed thread
creation. Coverage shows executed lines/regions, not assertion strength. Record
meaningful uncovered failures in the report review; add deterministic regressions
that fail for the wrong public behavior. Never set an arbitrary percentage gate.

The firmware workspace is excluded by Cargo.toml. Host integration tests include
some firmware source by path; those entries mean host execution only. No device,
ISR, DMA, I2S, FreeRTOS, ESP-IDF socket ABI, Sonos/TLS/NVS hardware recovery or
physical audio claim follows. Vendored MP3 C is not instrumented. Python and
JavaScript checks, dependencies, doctests, benches and examples are outside the
report. Stable line/region coverage does not supply nightly branch/MC/DC data.
Loom features are separate, mutually exclusive check stages; do not merge model
coverage into this production report or run `--all-features`.

`scripts/test-coverage-host` checks the CLI, unique run paths, symlink/file
rejection and fail-closed stage logging using real files and subprocesses. It
needs neither coverage tools nor builds. This command runs in `scripts/check`;
keep the instrumented run outside that fast check. The separate
[host coverage workflow](../.github/workflows/host-coverage.yml) runs on pull
requests and manual dispatch using GitHub-hosted Ubuntu 24.04. It installs all
repository-pinned host executables from mise.toml, then runs the wrapper tests
and coverage command. It never invokes the separate ESP toolchain task.
Checkout does not persist credentials; the job has contents-read permission.
Reports and provenance/failure logs upload even after test/report failure and
expire after seven days; instrumented build outputs are excluded. Bootstrap
failures before a run directory exists remain in GitHub step logs. The hosted
job has not yet run; inspect its first real artifacts before accepting CI.
Keep the uncovered-path review with durable pilot evidence. The full canonical
`scripts/check` also builds firmware; do not use it as the host-only coverage job.

Mutation testing can probe assertion strength in covered recovery logic after
an unmodified baseline passes. Fuzzing needs its own pinned nightly and tool
checks; minimized cases become ordinary stable regressions. Keep mutation,
fuzzing and coverage builds/profiles in separate directories. Neither diagnostic
replaces device acceptance.

Tool options and compatibility come from the
[cargo-llvm-cov 0.9.1 documentation](https://github.com/taiki-e/cargo-llvm-cov/blob/v0.9.1/README.md)
and [manifest](https://github.com/taiki-e/cargo-llvm-cov/blob/v0.9.1/Cargo.toml).
The tool/component pins use the documented [mise cargo backend](https://mise.jdx.dev/dev-tools/backends/cargo.html)
and [Rust components option](https://mise.jdx.dev/lang/rust.html).
