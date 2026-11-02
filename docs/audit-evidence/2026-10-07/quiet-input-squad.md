# Quiet playback: independent review squad

Four fresh reviewers audited the low-volume complaint: Fermat (codec/analog),
Sartre (digital audio/I2S), Boole (Sonos/gain), and Ampere (experimental evidence).
Parent review checked the test-script findings and original inspiration notes.
Reviewers changed no code, hardware, playback, or audio settings.

No critical production attenuation defect was established. This does not clear
the board: its physical input-to-PCM gain remains unmeasured. The strongest
confirmed attenuation findings concern our earlier test procedures.

## Consolidated findings

| Severity | Finding and evidence | Reviewers | Limit / action |
|---|---|---|---|
| Important, confirmed | Test scripts cap group volume at 5% without restoring it: `target/sonos/play-flac.py:19–24,50–57`, `target/sonos/track-control.py:193,234`. | Sonos | Historical attenuation and recurrence hazard; later group restoration to 34 is recorded. Do not reuse these procedures unchanged. |
| Important, confirmed | Generated host fixture applies `volume=-24dB`: `target/sonos/host-flac.py:32`. | Sonos | Not the board stream; finite-file branch bypasses it. Earlier compatibility tests cannot serve as matched loudness comparisons. |
| Important, evidence gap | Original per-player volume baseline was not saved. Restoring group 34 does not prove original individual balance. | Sonos | Exact restoration cannot be reconstructed from the group values alone. Preserve settings in future comparisons. |
| Important, evidence gap | WAV, FLAC and the meter all inherit the same unpacked capture samples: `core/src/audio.rs:174`, `firmware/src/audio_in.rs:199`. | All four | Their agreement clears measured post-split conversion, not analog input, codec state or shared unpacking. Calibrate jack-to-PCM transfer. |
| Important, evidence gap | Main gain readback omits mute, ALC, gate and other ADC controls: `firmware/src/codec.rs:32`. | Codec, digital, evidence | Defaults and reset semantics support those controls being off. Missing readback is not evidence they caused the complaint. Capture state during the symptom. |
| Important, experimental flaw | Diagnostic flashing resets the codec before reading it: `firmware/src/codec.rs:14`. | Codec, evidence | Correct post-flash registers cannot clear an earlier bad-state hypothesis. Preserve current evidence and reproduce the symptom under instrumentation. |
| Important, diagnostic defect | Saved comparison script requires a two-second marker, beyond the retained 1.185-second WAV: `target/quiet-input/compare-streams.py:28`. | Evidence | Independent reviewers reproduced the published match, but the saved script does not reproduce that analysis. Preserve a separate offline replay procedure with length and alignment checks. |
| Important, evidence gap | Quiet captures lack a contemporaneous firmware/boot identifier and raw speaker-state responses. | Evidence, Sonos | Historical ELF logs and reported device snapshots do not bind all observations to one session. Label unknowns; retain raw responses next time. |
| Important, unresolved reliability | WAV closed after 1.185 seconds; a separate board status reported `ERROR_PLAYBACK_FAILED`. | Digital, evidence, Sonos | Neither identifies gain loss. Correlate transport continuity and sample amplitude rather than treating one as the other. |
| Minor, confirmed | Old design says test volume ≤5%; FLAC adjustment calls 15% latest authorization. | Sonos, parent | Superseded by Doctor Biz's undo instruction. These historical instructions must not drive new tests. |
| Minor, interpretation | `codec.rs:45` assumes 1 Vrms full scale unconditionally; zero digital-rail counts do not establish analog headroom. | Codec, digital, evidence | Datasheet full scale depends on AVDD. Measure voltage and distortion; distinguish sine RMS from sample-peak dBFS. |

## Findings that survived independent challenge

Independent reviewers reproduced the 113,760-sample equality across 56,880 stereo
frames: WAV24 shifted right eight bits equals FLAC16, offset by 416 frames.
The comparison covers 1.185 seconds; WAV delivery failed the requested duration.
The five-second capture had positive channel correlation and negligible loss
when averaged to mono. These are bounded observations, not all-session claims.

Saved peaks near -14 dBFS contradict a simple extra eight-bit right shift on
bounded signed 24-bit samples: such a mistake would cap peaks near -48.16 dBFS.
One-bit timing errors remain unmeasured but are not necessarily clean -6 dB
attenuation; sign and inserted bits can instead create severe distortion.

Pinned ESP-IDF Philips stereo settings match our configuration. Staging and
rings copy samples; FLAC metadata and conversion preserve normalized amplitude.
No production Sonos volume/normalization command was found. These findings argue
against blindly changing shifts, ReplayGain, or codec gain.

## Deferred technical lead

The codec reviewer identified a datasheet/driver discrepancy: register 0x00
value 0x12 clears bit 2 (`EnRef`), which the datasheet describes as reference
enable. Espressif's own
[ES8388 driver](https://github.com/espressif/esp-adf/blob/master/components/esp_codec_dev/device/es8388/es8388.c)
uses the same value. This is not a verified attenuation defect and does not
justify changing the bit. Retain the discrepancy for a hardware-level audit;
include control/power registers alongside ADC controls in any diagnostic dump.

## Ordered next work

1. Preserve recordings and raw logs. Bind future captures to build/boot identity,
   source settings and per-player Sonos settings; retain unknown historical state
   as unknown. Make the saved-file comparison reproducible without live playback.
2. Measure a known steady signal at the board jack and compare captured PCM with
   the expected level, initially without Sonos. Repeat at two safe amplitudes to
   distinguish fixed loss from level-dependent gain/gating. This requires a
   physical voltage reference; a mixer set to 0 dB is not that reference.
3. If the transfer is wrong, compare jack voltage, codec-input voltage and codec
   output. Obtain ADC register state during the symptom. If new firmware is
   needed, reproduce the symptom after installation; do not use its reset state
   to explain the previous run.
4. If voltage and registers are correct but samples are wrong, compare serial
   I2S words, raw DMA words and unpacked samples across both polarities. This
   separates timing/packing defects from an analog loss with a similar dB ratio.
5. If board PCM is correct, compare matched content through the same Sonos route
   with settings preserved. Correlate playback errors separately. Do not use
   different music masters or the attenuated host fixture as a gain reference.

This is a proposed diagnostic sequence, not authorization to change hardware or
speaker settings. Before shipping, close the input calibration gap, prevent
stale test-volume procedures from recurring, and resolve playback failures.
No compensating gain change is justified by this review alone.

## Preserved recordings

Recordings remain private under `target/quiet-input/`; SHA-256 fingerprints:

| File | SHA-256 |
|---|---|
| `paired-wav.s24le` | `fdccc8aeb65ce67c58ad81ceece9edae66f055f625b0a417feb8f2982dc087fb` |
| `paired-flac.s16le` | `0f68e01e0f32d052031fc55a627acf6f441d30ba5e6660bc653779b5893c0d38` |
| `review.s24le` | `c5ff47b2bcfc52d62f08e1c682f2465ace1b92f142bdcea47b7234df7cb5fede` |

The prior canonical check passed all 13 stages. This squad performed read-only
source and saved-PCM review, not a fresh hardware acceptance test.

## Primary references

- [ES8388 manufacturer datasheet](https://www.boardcon.com/download/ES8388_datasheet.pdf): reset, ADC controls and AVDD-dependent full scale.
- [Pinned ESP-IDF I2S HAL](https://github.com/espressif/esp-idf/blob/v5.5.5/components/hal/i2s_hal.c#L178): RX settings.
- [Sonos volume semantics](https://docs.sonos.com/docs/volume): group/player distinctions.
- [Sonos Port setup](https://support.sonos.com/en-ie/article/set-up-your-sonos-port): fixed output.
- [Sonos playback objects](https://docs.sonos.com/docs/playback-objects): omitted ReplayGain defaults to zero.
