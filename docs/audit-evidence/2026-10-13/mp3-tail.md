# Raw MP3 delay, finite tail and epoch reset

Date: 2026-10-13. Scope: bounded offline characterization for `e8hd`.

The delay/padding measurement subcriterion is characterized separately from
fidelity. This does not accept codec quality or create a finite-export contract.
The diagnostic does not change production source, hardware state, audio output
or authentication. Its final rerun uses the separately corrected one-bit
transform headroom implementation; the [receipt](mp3-tail.json) identifies its sources.

The current API is a live shared encoder. `mp3/src/lib.rs` explicitly supplies
no tail drain; one call accepts exactly 1,152 stereo sample frames and produces
one raw 960-byte MP3 frame. Drop frees its state. The firmware adapter rejects
an incomplete block, recreates the encoder and clears staged input on reset.
The pipeline retains partial PCM on a temporary read stall and discards it
after capture overrun. Sonos Stop revokes delivery and Play intent, then performs
the existing checked remote Stop; it does not stop the shared encoder. Therefore
the finite cutoff measured here must not be described as source loss on every
manual Stop. Adding a drain to Stop would change that cancellation contract.
There is no current finite file/export API requiring complete terminal PCM.

The probe compiles the actual production C bridge and pinned encoder with
warning checks, synthesizes known PCM, and decodes raw output with real FFmpeg
`mp3float`. An early impulse independently measures a 1,057-sample peak delay
in both channels (22.0208 ms at 48 kHz). This is combined encoder/decoder delay,
not an isolated encoder delay or a gain/fidelity score.

Each finite fixture has 18,432 stereo sample frames. The terminal impulse is
at source index 18,431; its decoded peak is at 19,488. The terminal burst covers
source indices 18,176–18,431. The table compares decoded energy against the same
fixture followed by eight actual zero blocks. No fitted gain correction is used.

| Actual zero blocks appended | Decoded sample frames | Terminal impulse energy, L/R | Terminal burst energy, L/R |
|---|---:|---:|---:|
| 0 | 18,432 | 0 / 0 | 0.000000525% / 0.000002307% |
| 1 | 19,584 | 97.953850% / 98.303033% | 99.979023% / 99.995141% |
| 2 | 20,736 | 100% / 100% | 100% / 100% |
| 8, reference | 27,648 | 100% / 100% | 100% / 100% |

Equal decoded and source counts do not preserve a terminal event: the unpadded
impulse decodes to exact silence. One added block recovers the impulse peak but
leaves 2.046150%/1.696967% of reference energy after the cutoff. Its omitted
peak is 0.038536/0.029445 on the normalized float PCM scale. Thus one block is
not a complete filter-tail drain even though it exceeds the measured delay.
For both tested terminal fixtures, two blocks recover the entire eight-block
reference; all later decoded samples are exactly zero. This is a bounded
fixture result, not a guarantee for every source or decoder.

Every shorter encoded result is a byte-identical prefix of the longer one.
Every shorter decoded result exactly matches the longer decoded prefix. The
test therefore exposes truncation of real terminal energy, not a change in
earlier encoding caused by padding. Raw MP3 contains no gapless trim metadata.
Appending one block adds 1,152 synthetic sample frames and 960 encoded bytes;
after a hypothetical 1,057-sample leading trim, 95 samples remain beyond the
original input duration. Two blocks add 2,304 sample frames and 1,920 bytes,
leaving 1,247 beyond the original duration after that leading trim. Padding
does not itself apply either trim.

Alignment accounting leaves source positions 17,375–18,431 without matching
decoded positions in the unpadded result. Those 1,057 positions describe delay
coverage; lossy filters spread energy, so this is not an exact missing-waveform
count. The terminal-signal negative controls establish actual missing content.

The second probe imports the actual firmware Rust codec adapter and real MP3
crate. It encodes a terminal impulse, stages 137 further sample frames, verifies
incomplete encode fails, resets, and encodes four silence frames. Those frames
are byte-identical to a fresh encoder and decode to exact zero. Keeping the
same encoder instead produces nonzero old impulse energy during those silence
frames. Reset therefore clears both pending filter history and staged input.
At most 1,151 sample frames can form a nonempty incomplete MP3 block; this
probe measures 137 specifically. A temporary input stall has a different
production rule and preserves that staged input.

The minimal permanent regression is
`core/tests/stream_codec.rs::mp3_reset_discards_nonzero_filter_history_and_partial_pcm`.
It retains the real no-reset control. The existing tests only reset silence
history or check ring generation, so this adds distinct filter-history coverage.
The whole stream codec test target passes: five tests, zero failures, zero
ignored, no new warnings. Formatting checks pass after formatting the scratch
Rust source. These bounded checks do not establish the full canonical check.

Restore the [archived probe sources](mp3-tail-probe/README.md) into the scratch
paths below. Reproduce from the repo root on a little-endian host with the existing locked uv
environment, offline Cargo dependency cache, C compiler and FFmpeg:

```sh
uv run --quiet --locked python3 target/kata-hardening/oct13-mp3-tail/check-tail.py
cargo test -p esp-shazam-core --test stream_codec
```

Keep `check-tail.py`, `adapter/Cargo.toml` and `adapter/src/main.rs` at these
relative scratch paths. The script regenerates all synthetic fixtures and
outputs, checks compiler/decoder failures and stderr, and writes `tail.json`
with source, input, encoded and decoded hashes, exact counts and residuals.
No private recordings, services or hardware are required. The archived lockfile
pins the standalone adapter build; it uses production path
dependencies. Full probe stdout/stderr and focused test logs live beside the
receipt. The first scratch build's missing feature declaration warning was
fixed before final reruns; final build/decoder stderr is empty.

Recommendation: close the delay/padding measurement subcriterion against this
bounded evidence. Preserve immediate Stop and epoch isolation. Keep fidelity
and any future finite-export requirement separate; no user acceptance of
terminal truncation or audible quality is inferred here.
