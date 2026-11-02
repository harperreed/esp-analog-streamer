# Raw MP3 tail probes

These throwaway offline diagnostics measure combined encoder/decoder delay,
finite terminal-signal truncation, optional zero-input padding and actual
firmware codec reset behavior. They use the production C bridge, pinned Shine
sources, actual Rust codec adapter and FFmpeg. They do not access hardware,
play audio, change Stop behavior or add a finite-export API.

The [report](../mp3-tail.md) and [receipt](../mp3-tail.json) describe the final
current implementation. The receipt records 32 source hashes and hashes of
synthetic PCM, MP3 output and decoded PCM. It does not retain or claim a
before/after tail-fix baseline. Its Git HEAD identifies the checkout base;
the recorded source hashes identify the measured working-tree implementation.

Run from the repository root on a little-endian host with a C compiler,
FFmpeg, the existing locked uv environment and an offline Cargo dependency
cache. Restore the files to the exact scratch paths before running:

```sh
mkdir -p target/kata-hardening/oct13-mp3-tail/adapter/src
cp docs/audit-evidence/2026-10-13/mp3-tail-probe/check-tail.py target/kata-hardening/oct13-mp3-tail/check-tail.py
cp docs/audit-evidence/2026-10-13/mp3-tail-probe/adapter/Cargo.toml target/kata-hardening/oct13-mp3-tail/adapter/Cargo.toml
cp docs/audit-evidence/2026-10-13/mp3-tail-probe/adapter/Cargo.lock target/kata-hardening/oct13-mp3-tail/adapter/Cargo.lock
cp docs/audit-evidence/2026-10-13/mp3-tail-probe/adapter/src/main.rs target/kata-hardening/oct13-mp3-tail/adapter/src/main.rs
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-tail/check-tail.py
```

The Python checker resolves the repository root from its scratch location.
The standalone Cargo manifest and Rust source use relative production paths.
Running directly from this archive resolves the wrong paths. Preserve any
existing scratch receipt and outputs before replacing them for a new run.

The checker regenerates synthetic fixtures and writes `tail.json`, raw MP3,
decoded float PCM and command logs under the scratch directory. It compiles
with warning checks and rejects failed commands or unexpected stderr. Twelve
finite-input decodes compare zero, one, two and eight appended zero blocks;
three further decodes verify reset, fresh-epoch and continuous-history outputs.
The eight-block reference bounds residual filter energy for these fixtures;
two-block recovery does not prove a universal drain length.

The standalone adapter uses production path dependencies and its retained
Cargo lockfile. The permanent regression is in
`core/tests/stream_codec.rs`; run its existing target separately:

```sh
cargo test -p esp-shazam-core --test stream_codec
```
