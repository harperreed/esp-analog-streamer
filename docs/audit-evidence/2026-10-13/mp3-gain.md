# MP3 gain: decoder controls and quantizer hypothesis

The approximately +1.02 dB tone gain is reproduced in the pinned upstream MP3
path. It is not explained by our allocation/framing patches, reference PCM
normalization, or the choice between FFmpeg's fixed and floating decoders.
A throwaway quantizer replacement did not remove it. No production correction
is justified by this result, and no firmware or physical audio was changed.

## Controls

The retained synthetic stereo input contains 483,840 sample frames at 48 kHz,
with half-scale 997/1703 Hz tones. The gain fit uses actual signed-16-bit PCM
normalized by 32768, a previously measured 1,057-sample alignment, and a steady
window excluding 4,608 samples at either end of the aligned overlap.

| Control | Result |
| --- | --- |
| Current production C bridge versus retained upstream MP3 | Exactly the same 403,200 bytes |
| FFmpeg floating MP3 decoder | Channel gains 1.12484290 / 1.12484652 |
| FFmpeg fixed MP3 decoder | Channel gains 1.12484203 / 1.12484686 |
| Repository FLAC16 control through FFmpeg float PCM output | 479,232 source-prefix samples exactly equal; gains 1.0 / 1.0 |

Both MP3 decoder paths produce 483,840 actual decoded sample frames. They share
FFmpeg infrastructure, so this is not a comparison of independent libraries.
The byte-equivalence control compiles the nine current C units plus the actual
production bridge with `-O3 -fsigned-char -Wall -Wextra -Werror`. It does not
exercise the board's capture or measure the analog path. The Rust wrapper passes
its fixed PCM buffer directly to that bridge; no fitted gain is applied there.

## Quantizer experiment

An observational probe records the stored MDCT coefficient, signed quantizer
index, step and global gain after each encode. Its baseline output matches the
retained upstream tone and noise streams at both 320 and 192 kbps. The model
reconstructs spectral values from these internal fields. This is a stored-state
measurement, not an independently decoded spectral bitstream.

For tones, the fitted stored quantizer gain is 0.99991054 while the final PCM
gain remains about 1.12484. Replacing the quantizer
function with a direct floating expression produces a stored gain of
0.99997774 and decoded gain about 1.12492. That experiment fails to explain the
common tone increase as integer quantizer rounding. The experimental code stays
outside the application and is not a proposed replacement. It replaces the
whole quantizer function, including its early range shortcut and the separate
floating fallback's rounding; it is broader than a table-only precision change.

Noise behaves differently: the baseline stored quantizer gains are about
1.0197 at 320 kbps and 1.0364 at 192 kbps. The direct variant changes the noise
output substantially. It provides no general quality improvement or safe
constant correction. The 192 kbps variant is a host diagnostic; the application
still supports only 320 kbps MP3.

The observed tone gain is near 9/8. That resemblance is a hypothesis about
transform/synthesis normalization, not proof of a wrong coefficient or a reason
to multiply production samples by 8/9. Next work must derive the analysis and
synthesis scaling independently and verify it on the actual transform path.
The measured finite-stream tail, decoder warmup, noise error and physical
fidelity gates remain separate.

## Evidence and limits

Private raw inputs, decoded PCM, boundary arrays, commands and logs live in
`target/kata-hardening/oct13-mp3-gain/`. Public JSON records hashes and numeric
results; archived probe sources describe the measurement. Inputs are existing
synthetic fixtures, with no private music published. Encoder compilations and
FFmpeg controls succeeded; all retained experiment logs are checked explicitly.
Independent Sol 6.1 review recomputed every gain and checked the raw hashes
and counts. A side-information/source-formatter check finds no nonzero stored
coefficient omitted by table-zero or trailing-zero rules; this is not an
independent Huffman decoder. Parent fresh-eyes review found no blocking error
in these scoped claims. The ordinary firmware remains unchanged. No Sonos
playback, workstation audio, network configuration or authentication operation
was needed.

This is progress on e8hd's unresolved gain question, not a gain fix, listening
acceptance or a Kata closure. Keep the production gain policy unchanged until
a cause and a reviewed correction are established.

Machine-readable results: [mp3-gain.json](mp3-gain.json).
[Archived probes](mp3-gain-probe/README.md) require the retained synthetic
fixtures and pinned upstream checkout; they are diagnostic artifacts, not CI
tests or application code. Existing canonical checks remain those for unchanged
production commit `65db167`; they were not repeated for this evidence-only work.
