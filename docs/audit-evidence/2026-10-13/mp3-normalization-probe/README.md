# MP3 normalization probes

These are throwaway host diagnostics, retained to reproduce the normalization
derivation and independent corrected-output comparison. The regression that
runs in normal checks lives in `mp3/tests/decode.rs`.

Run from the repository root using the locked uv environment. Restore
`check-normalization.py` and `stage-probe.c` under
`target/kata-hardening/oct13-mp3-normalization/model/`; restore
`check-correction.py` one directory above. The model resolves those paths
deliberately; running it directly from this archive uses the wrong root.

The model requires the clean Shine checkout at
`target/mp3-benchmark/source/shine/`, pinned to
`ab5e3526b64af1a2eaa43aa6f441a7312e013519`. It reads the eight FFmpeg n8.0
files listed in `reference-manifest.json` from the sibling `reference/`
directory. Download the exact URLs and verify their SHA256 values before use.
The generated C wrapper extracts reference calculation bodies without editing
their arithmetic. No FFmpeg reference source is part of application firmware.

```sh
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-normalization/model/check-normalization.py
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-normalization/check-correction.py
```

The correction comparison also needs the retained synthetic
`target/mp3-benchmark/host/{tones,noise}.s16le` fixtures. Their hashes appear in
the public receipt. The October 9 host probe describes their generation; do not
run its whole experiment blindly because it also references private music.
The 16-frame xorshift golden input is regenerated and hash-checked by this
script. Only a scratch upstream MDCT file is changed; original upstream files
stay untouched. The production bridge is compiled from the working tree.

Each command writes bounded fixtures, raw arrays, encoded output and logs under
the private target directory. Failed compiler/decoder commands or unexpected
stderr fail the diagnostic. The model has no quantizer. The correction test
uses the real production quantizer and FFmpeg decoder; it asserts tone gain
and sample counts, but records noise error without declaring it acceptable.

The two host diagnostics require a little-endian host, C compiler, FFmpeg and NumPy from
the existing uv environment. They neither access hardware nor play audio.

`verify-board.py` is a separate read-only hardware receipt collector. Restore it
beside `check-correction.py`; it requires the retained preflight Sonos snapshot
and checked-image receipt. It verifies the board at its recorded address and
revalidates the recorded Sonos UDN before reading transport state. It never
issues playback or settings writes. Its original inline predecessor failed
on an empty post-reboot discovery list; the public receipt retains that failure.
