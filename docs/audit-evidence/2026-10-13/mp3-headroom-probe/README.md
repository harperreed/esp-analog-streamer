# MP3 headroom probes

These throwaway host diagnostics do not play audio or access hardware. Normal
regressions live in `mp3/tests/decode.rs` and `core/tests/stream_codec.rs`.
Use the repository's locked uv environment, NumPy, a C compiler and FFmpeg on
a little-endian host. JSON receipts preserve the original commands and hashes.

`bounds/check-bounds.py` runs beside its retained `tables.i32le`, including
directly in this archive. It recomputes exact bounds and writes `results.json`
and a synthetic witness. Copy the directory into scratch space first to keep
the archived receipt unchanged. The baseline table has 3,224 integers; its
earlier 3,208-integer dump lacked the 16 alias constants. This is a schema
extension, not a source change. `candidate-tables.i32le` records the corrected
table. `dump-tables.c` can regenerate tables from a selected source tree.

The fixture scripts resolve the repository from their scratch paths. Restore
`probe.py` in both `target/kata-hardening/oct13-mp3-headroom/` and sibling
`oct13-mp3-headroom-fixed/`; restore `check-candidate.py` in the latter. Do not
run these scripts directly from this archive. Prepare an exact `db8a086` source
snapshot under the first directory's `baseline-source/` and the corrected
source under the second's `candidate-source/`, preserving `mp3/` paths. Then:

```sh
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-headroom/probe.py --source-tree target/kata-hardening/oct13-mp3-headroom/baseline-source
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-headroom-fixed/probe.py --source-tree target/kata-hardening/oct13-mp3-headroom-fixed/candidate-source --include-lowlevel
```

Use fresh output directories; do not overwrite another running probe's files.
Each command generates its own fixtures, actual MP3, float decodes and receipt.
The second command's `tones_independent_nearfull.s16le` begins with the minimal
reproducer: copy its first 9,216 bytes into the baseline directory as
`minimal-tones-nearfull-2-blocks.s16le` before running `check-candidate.py`.
That checker validates all fixture hashes/counts, compares low-level controls,
and runs the real corrected codec under full UBSan. It requires the baseline
`actual-bridge` binary and both generated `headroom.json` receipts.

For the independent golden comparison, restore `check-correction.py` to the
baseline directory. It uses the exact upstream checkout and synthetic fixture
paths described in the adjacent normalization-probe README. The upstream pin
is `ab5e3526b64af1a2eaa43aa6f441a7312e013519`; its scratch MDCT and gain changes
are independent of the production patch. It records equality on all three
fixtures. This compiler comparison is separate from full UBSan validation of
the actual corrected production source.

`baseline.json` and `candidate.json` retain full metrics and acquisition hashes;
`correction.json` identifies the independently derived golden. The retained
red/green regression logs and UBSan witness logs document the failing controls.
The shared-directory acquisition failure is described in `acquisition-note.md`
and the parent report; transitional measurements are never the baseline.

`verify-board.py` is a separate read-only collector, restored to the baseline
scratch directory. It checks the deployed image and unchanged stopped settings
against `check.json` and `before-sonos.json`. If discovery is empty, it uses the
previous normalization run's preflight address and revalidates the selected UDN
before GetTransportInfo. It needs those retained local receipts and access to
the recorded board/Sonos addresses. It sends no playback or settings writes.

The archived green regression log omits its final blank line for Git whitespace
checks; the private raw log and its hash remain in the parent receipt.
