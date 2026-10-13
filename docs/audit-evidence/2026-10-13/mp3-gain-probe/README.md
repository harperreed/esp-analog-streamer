# Throwaway MP3 gain probes

Copy the five source files from this directory into a fresh
`target/kata-hardening/oct13-mp3-gain/` under the repository root. Their relative
paths intentionally target the retained October 9 synthetic fixtures in
`target/mp3-benchmark/host/` and the exact upstream Git checkout at
`target/mp3-benchmark/source/shine/`. The corresponding hashes and compile/run
commands are in ../mp3-gain.json. Check those inputs before a replay.

Use the repository's existing `uv run --quiet --locked python3`. Run
`cause-experiments.py`, then `boundary-coverage.py`. These compile host probes
and decode files with FFmpeg; they never play audio. `decoder-controls.py`
compares the two FFmpeg MP3 decoders on retained tones. It intentionally refuses
an already-existing output file; retain separate acquisitions rather than
silently overwriting a result. The cause experiment writes a fixed named result
set, so archive the directory before repeating it.

`current-bridge.c` compiles with the current production bridge and all nine
vendored C units using `cc -O3 -fsigned-char -Wall -Wextra -Werror`, the vendor
include directory and `-lm`. It accepts input.s16le and output.mp3 and requires
whole 1,152-frame stereo input blocks. Compare its output bytes with the retained
upstream tone stream. It is not a Rust API, firmware, or analog-path test.

The direct-quantizer source is generated in scratch space from the pinned
upstream C file. It changes arithmetic only for a hypothesis test and must not
be copied into production. These probes are archived diagnostic artifacts;
they are not installed commands or added canonical tests.
