# Bounded host fuzz acceptance

The committed f57aa3e snapshot completed all three AddressSanitizer builds,
seed replays and bounded mutation runs with exit status 0. The machine-readable
receipt is [host-fuzz.json](host-fuzz.json). Raw evidence remains in
`target/fuzz-host/run.pCkUt7aR`, including the immutable source archive, exact
commands, compiler/dependency versions, seeds, corpus, logs and status.

| Target | Original seeds | Mutation runs | Reported seconds | Final reported RSS |
| --- | ---: | ---: | ---: | ---: |
| FLAC frames | 21 | 20,788 | 61 | 383 MB |
| HTTP head | 12 | 1,000,000 | 23 | 498 MB |
| Stream operations | 7 | 1,000,000 | 24 | 492 MB |

The budget was 60 seconds or one million iterations per target. LibFuzzer checks
time between inputs, so FLAC reported 61 seconds; the runner's outer stage bound
was 90 seconds. RSS values are sampled reports, not hard memory containment.
Each input also had a five-second limit and a 64 MB allocation limit.

All 40 original seeds have recorded exact bytes and hashes. The replay command
passed both the copied corpus and cargo-fuzz's default source corpus, replaying
duplicate inputs. This is 40 distinct committed seeds, not 80. Mutation runs used
the separate writable run corpus. No production failure or minimized crash was
found in this bounded run, so no crash regression is claimed.

Before the snapshot, independent review and all 27 canonical stages passed,
including nine stable fuzz-helper tests and real supervisor/guard checks.
Standalone all-target Clippy, formatting and workflow lint also passed. Review
found and fixed missing route/write assertions, incomplete timeout cleanup and
child-shell failure propagation. Ordinary process regressions prove a failed
stage stops, retains its evidence and records a nonzero result; no intentional
crash target is included.

The nightly is scoped to `fuzz/mise.toml`; normal Rust 1.93.0 and the ESP compiler
remain unchanged. The acceptance build used C++ 15.2.0 on Linux x86_64. It did not
change device firmware, audio routing or playback.

## Limits

This establishes a local bounded pilot for real host APIs. It does not establish
full firmware concurrency, DMA/radio timing, playback recovery or analog fidelity.
The root/user Cargo configuration can still influence compilation. The archived
source and tool receipts define this run; it is not a hermetic machine image.
The old moving-source/mixed-toolchain run is excluded from acceptance.

The PR/scheduled workflow is committed and linted but has not run on GitHub.
Local cleanup tests used uutils timeout 0.10.0; hosted GNU timeout execution is
still pending. The process-group supervisor requires stages to stay in its group
and does not claim to contain deliberately detached processes. Longer diagnostic
runs and hosted artifact upload remain separate evidence.
