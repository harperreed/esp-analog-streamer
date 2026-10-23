# Quiet input review

Doctor Biz reported very quiet playback and requested an audit of attenuation
inside the streaming chain. No source, codec, or Sonos controls were changed
for this review. Workstation playback remained stopped.

## Findings

1. **No FLAC gain loss in the measured overlap.** Concurrent board WAV and FLAC
   captures were independently decoded by FFmpeg. After aligning by 416 stereo
   frames, all 113,760 samples matched exactly: signed 24-bit WAV shifted right
   eight bits equals decoded signed 16-bit FLAC. This preserves normalized
   amplitude. The overlap was 56,880 frames / 1.185 seconds. WAV ended early;
   FLAC completed the requested eight seconds. Neither the early WAV close nor
   the board's separately observed `ERROR_PLAYBACK_FAILED` is resolved here.
2. **No stereo cancellation in the five-second sample.** Left/right RMS was
   -26.398/-26.410 dBFS, peaks -13.939/-14.054 dBFS, correlation +0.960.
   Averaging the channels gave -26.491 dBFS. No samples reached full scale.
   These are different program windows from the earlier -32.6 dBFS FLAC capture;
   their different levels do not measure a gain change.
3. **I2S settings match the pinned driver defaults.** `firmware/src/i2s_capture.rs`
   matches ESP-IDF 5.5.5's ESP32 Philips stereo macro at 32-bit width, including
   bit shift, slot width, WS polarity, and `msb_right`. The driver forwards these
   settings to hardware. This supports the intended top-24-bit interpretation;
   it does not replace a physical bus or calibrated input measurement.
4. **The stream does not use the mono recognition feed.** Capture publishes
   interleaved samples before decimation. Staging and the raw ring copy samples;
   FLAC applies only the required 24-to-16-bit conversion. Metadata declares
   16-bit samples, including frame headers. No stream gain, normalization,
   silence gate, or fade was found in this path.
5. **Codec checks do not cover every gain control.** Setup and saved boot
   readback verify PGA 0 dB and ADC digital volume 0 dB. The reset sequence
   should reset all controls, with automatic level control disabled by default.
   Startup does not read back ADC mute, ALC, or noise-gate controls. Unexpected
   state is therefore unverified, not an identified fault. The statement
   "line level wants 0 dB" is a design assumption, not jack-to-PCM calibration.

6. **Earlier Sonos volume changes exist, but no production volume commands.**
   Saved replies confirm test group-volume writes of 5, 5, 10, 15, then 34 for
   restoration. Old scratch scripts still contain 5/15% caps and must not be
   rerun unchanged. Production playback/queue payloads contain no gain, volume,
   EQ, or normalization fields. Searches found no logged or scripted changes
   to loudness, EQ, mute, or fixed-output mode; this does not establish an
   exhaustive history of unlogged commands.
7. **Live Sonos state differs by device.** Read-only queries on this review date
   found Altec and Klipsch are Sonos Ports with fixed output, volume 100,
   unmuted, bass/treble 0 and loudness off. West/East are Play:5 devices with
   variable output, volume 51, unmuted, bass/treble 0 and loudness on. A Port's
   fixed-output setting makes its reported 100% unsuitable as a measure of
   downstream amplifier volume. Advertised GetVolumeDB/GetVolumeDBRange calls
   returned error 800; a query using the guessed EQ type `VolumeLimit` returned error 402.
   These errors do not prove that a volume limit is absent. No settings changed.

## Evidence and limits

Local raw captures and throwaway analysis scripts remain under
`target/quiet-input/`. Curated numerical results are in
`quiet-input-measurements.json`; recordings are not committed.

The first exact-comparison attempt required a marker at two seconds and failed
because WAV delivered only 1.185 seconds. Analysis of the saved recordings with
an earlier marker produced the exact match above. This is a shorter comparison,
not a successful eight-second WAV test.

The manufacturer [ES8388 datasheet](https://www.boardcon.com/download/ES8388_datasheet.pdf)
defines register reset, gain controls, and chip-input full scale. The available
[Ai-Thinker carrier drawing](https://github.com/RaceNJason/ESP32_A1S_Audio_Kit_Media_Player/blob/main/ESP32-Audio-Kit%20v2.2%20Schematic.pdf)
shows AC coupling without a resistive attenuation pad, but its correspondence
to this physical board and full module input circuitry is not verified.

## Next board-side evidence

The subsequent [independent squad review](quiet-input-squad.md) changes the test
order: preserve current evidence and prioritize calibrated jack-to-PCM gain.
Readback remains useful for ADC registers 0x09 through 0x16, including mute, ALC
and noise gate, but must be associated with the reproduced symptom.
The current firmware exposes no live register-read endpoint, so this requires
a diagnostic firmware change and an interruption of capture. Startup resets the
codec, so post-flash values cannot establish its previous state. Measure jack-to-PCM gain with a known input level before treating register
readback as the leading diagnostic.
Do not compensate with arbitrary gain or change Sonos settings to conceal an
unlocated loss.

The review does not establish the cause of the quiet playback. Before shipping,
verify the physical gain path and resolve the observed playback failure.

Validation: canonical `scripts/check` passed all 13 stages, including host tests,
UI/script checks and firmware build. This verifies the current code; no firmware
fix or gain change was made.
