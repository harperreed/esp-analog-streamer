# Reproduce the bounded fidelity comparison

These are throwaway offline diagnostics. They encode/decode files and never
play audio. They require the locked repository uv environment, a C compiler,
and FFmpeg with libmp3lame. The receipts record the measured versions. Keep raw
PCM, MP3 and decoded floats private; none is included here.

`manifest.json` maps byte-identical archived files to their measured local
sources and hashes. `music.json`, `music-notes.json` and `music-constrained.json`
are original receipts. `reference.json` is explicitly derived from the full
local reference receipt: verbose FFmpeg help is replaced with exact relevant
lines and hashed log metadata, preserving measurements and tool versions.
The source hash of that full receipt remains recorded.

## Music and fade controls

Restore `music-probe.py`, `music-summarize.py` and `music-supplement.py` together
in scratch space under names `probe.py`, `summarize.py` and `supplement.py`.
The latter two import the first by that name. Pass the actual repository root
and a fresh output directory explicitly:

```sh
uv run --quiet --locked python3 /scratch/probe.py --repo "$PWD" --music-pcm /path/to/authorized.s16le --out /scratch/run
uv run --quiet --locked python3 /scratch/summarize.py --repo "$PWD" --run /scratch/run
uv run --quiet --locked python3 /scratch/supplement.py --repo "$PWD" --run /scratch/run --out /scratch/constrained
```

The optional music input must contain exactly 483,840 interleaved stereo signed16
frames, little-endian. The measured fixture hash is in the receipt. Omitting
`--music-pcm` runs nonprivate seeded calibration, tone and fade controls; skip
the supplement in that case because it requires the music result. No public
fixture is substituted for the private recording or labeled equivalent to it.

`music-report.md` explains alignment, the limited repertoire and the spectral
bin convention. `music-notes.json` adds per-level fades, silent endpoints and
the failed first acquisition. The one constrained comparison disables LAME's
joint stereo and reservoir together; it does not isolate each feature's effect.

## Synthetic reference

Restore `reference-compare.py` as `compare.py` in
`target/kata-hardening/oct13-mp3-fidelity-reference/`. Its repository root depends
on that depth. It uses the six retained corrected fixtures and frozen source
from `target/kata-hardening/oct13-mp3-headroom-fixed/`, checked against the
archived headroom candidate receipt. Recreate those first using the earlier
headroom-probe README if absent. A different source/tool version may change
hashes; it is a new measurement, not verification of this acquisition.

```sh
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-fidelity-reference/compare.py
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-fidelity-reference/compare.py --verify
```

The first command refuses to overwrite an existing comparison. Preserve a prior
directory before another acquisition, with no worker using it. To derive the
compact receipt, restore `reference-publication.py` alongside it as
`prepare-publication.py` and run it after acquisition. `reference-report.md`
records exact common windows, finite-tail omissions and spectral weighting.

`verify-music.py` is the parent's separate arithmetic/hash check. Restore it as
`target/kata-hardening/oct13-mp3-fidelity-parent/verify.py`; it reads the recorded
music worker's sibling `run-02/`. `parent-verification.json` and `review.json`
record parent and independent checks. Verification of original raw hashes needs
the retained private artifacts; the archive alone cannot recreate private music.
