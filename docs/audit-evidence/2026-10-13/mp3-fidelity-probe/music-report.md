# Bounded offline MP3 music and fade characterization

Current production Shine and the installed FFmpeg libmp3lame both encode the
same quantized 48 kHz stereo source at 320 kbps. The experiment compares actual
decoded floats without gain correction. It does not establish listening
acceptance or represent every kind of music.

| Original-source measurement | Production Shine | Default LAME | Stereo/no-reservoir LAME |
| --- | ---: | ---: | ---: |
| Music steady raw SNR, left/right | 39.186 / 39.186 dB | 58.948 / 58.948 dB | 37.697 / 37.697 dB |
| Music measured gain, left/right | 0.998959 / 0.998959 | 1.000014 / 1.000014 | 0.999865 / 0.999865 |
| Music maximum decoded float magnitude | 0.295068 | 0.294185 | 0.294787 |
| Fade steady raw SNR, left/right | 74.813 / 74.715 dB | 77.348 / 76.555 dB | Not acquired |
| Tone steady raw SNR, left/right | 75.329 / 74.805 dB | 78.667 / 77.538 dB | Not acquired |

The default LAME music advantage disappears when its stereo sharing and bit
reservoir are both disabled. Those are material configuration differences:
production emits independent stereo frames without reservoir references.
This single supplemental comparison does not separate the effects of those
two settings or establish perceptual superiority for either encoder.

The authorized music fixture contains 483,840 frames, or 10.08 seconds. Its
channels are exactly identical. The first 33,612 frames (0.70025 seconds) are
silent; the source peak is 0.294220 and RMS is about 0.037639. Its exact signed16
SHA256 is `7c113d609cd1b0f4123aada24be5c88c044c06d65bd2b70b2fb6decc1f251d51`.
The original private WAV matches the October 9 published source hash, and a
fresh extraction of the same first 10.08 seconds matches the retained raw
fixture byte for byte. October 9 did not record a separate raw-prefix hash.
No audio is published. Dualmono content can benefit from joint stereo; this
excerpt cannot assess ordinary stereo separation, difficult full-scale music,
or a broad musical repertoire.

One nonperiodic seeded calibration establishes fixed decoded delays of 1,057
samples for production and 1,105 for both LAME configurations. The same source
window is used for every calibration candidate. No music or fade segment is
realigned. Music timing checks at source frames 229,920 and 455,232 select zero
offset from each fixed delay. The first prescribed window, starting at frame
4,608, contains no source energy and provides no timing evidence; the receipt
records null and `unavailable_zero_energy`.

Each encode receives two additional blocks of known zero PCM: 2,304 frames.
All fidelity scores compare only the original source span. Production music
emits 422 MP3 frames and decodes to 486,144 frames; after its delay and original
source there are 1,247 decoded frames. Each LAME music encode emits 423 MP3
frames and decodes to 487,296 frames, leaving 2,351 after delay and source.
The extra LAME frame comes from encoder flushing. Xing and ID3 are disabled,
so decoder gapless trimming does not hide delay or padding. This probe padding
does not change production: closing the production bridge without padding
still lacks the final 1,057 source samples in the equal-length decoded stream.
Two blocks cover these measured inputs; no universal drain claim follows.

All acquired float decodes are finite. No sample exceeds or reaches magnitude
one in these modest-amplitude music, tone, fade and calibration fixtures. The
separate near-full-scale noise study remains relevant; these quiet inputs do
not clear its overshoot gate. The music all-source raw SNR is 39.218 dB for
production and 58.939 dB for default LAME. Steady scores above omit 4,608 source
frames at each edge and retain the same comparison span across encoders.

The deterministic fade rises from -90 to approximately -6 dBFS and falls back,
with 4,608 true zero-envelope frames at each endpoint. Rising -90 to -75 dBFS
segments measure production SNR 35.54 / 35.41 dB and default LAME 26.36 / 26.12
dB. At -15 to -6 dBFS those values are 74.73 / 74.86 and 77.36 / 76.60 dB.
The notes retain all six level bands in both directions. In the zero-envelope
source intervals, production error RMS is approximately -134.66 / -133.80
dBFS and default LAME -142.37 / -141.43 dBFS; source-relative SNR is undefined
there and remains null. Whole-fade SNR alone would hide this level dependence.

The primary receipt retains 0.5-second segment errors and spectral error bands.
Spectral calculations sum Hann-windowed one-sided rFFT magnitude-squared bins
without doubling interior bins. `source_power_fraction` means that bin-sum
fraction, not a strictly weighted physical band-energy fraction. Error and
reference use the same convention. These are unweighted objective diagnostics,
not perceptual quality scores. In this music excerpt, production versus default
LAME band SNR is 28.49 versus 48.22 dB at 2–8 kHz and 24.46 versus 44.67 dB at
8–16 kHz. The supplemental constrained reference supplies its own band receipt.

LAME defaults remain joint stereo enabled, reservoir enabled and ABR disabled
in the primary comparison. No explicit quality or cutoff option is supplied;
cutoff remains automatic. The supplemental reference changes only
`joint_stereo=false` and `reservoir=false`; actual headers verify stereo and
zero reservoir references. Automatic lowpass and other implementation choices
still differ from production. No model of a single codec mechanism follows.

Source hashes match before and after each acquisition and the final
verification. The probe compiles a frozen copy of the production bridge and
vendored sources. Compilation and all encoder/decoder commands exit zero with
empty diagnostics. `summarize.py` independently reloads the retained original
PCM and float decodes, verifies artifact hashes, and exactly recomputes aligned
metrics and amplitude counts. A separate reviewer verified MPEG headers,
delays, source identity and arithmetic.

The first acquisition encoded and decoded successfully but failed report
serialization because a silent timing window produced NaN correlation. It
remains retained as failed `run-01`. The corrected `run-02` uses the same timing
windows, reports unavailable evidence, and emits no warnings. Measurement
controls cover identity, uncompensated doubled gain, a known 137-sample delay,
float overshoot counts and zero-energy correlation. No production files,
hardware settings, playback state, Git history or Kata state were changed.

Reproduction requires the repository's locked Python environment and installed
FFmpeg with libmp3lame. The script accepts an optional authorized generic raw
PCM fixture; omit `--music-pcm` to run the nonprivate controls alone. Use fresh
output directories. Audio artifacts stay private and must not be committed.

```sh
uv run --quiet --locked python3 probe.py --repo /path/to/repo --music-pcm /path/to/authorized.s16le --out /path/to/fresh-run
uv run --quiet --locked python3 summarize.py --repo /path/to/repo --run /path/to/fresh-run
uv run --quiet --locked python3 supplement.py --repo /path/to/repo --run /path/to/fresh-run --out /path/to/fresh-supplement
```

This bounded work supplies music and fade comparisons missing from the current
normalized/headroom characterization. Perceptual acceptance remains untested.
I would keep that acceptance gap explicit before presenting MP3 as a finished
output profile; neither a single favorable excerpt nor an objective score
can accept the separate near-full-scale noise result.
