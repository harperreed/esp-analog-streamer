# FLAC history buffer verification

Source commit `53b7d67` tightens the encoded-frame allocation to 16,399 bytes and
retains 141 frames with recognition off: 12.032 seconds at 48 kHz, versus 8.021
seconds before. Ring payload plus ESP32 length metadata grows by 893 bytes.
Recognition on retains 24 frames. Socket deadlines stay at 8.021/2.048 seconds;
startup still joins the current head. This is retained history, not a tested
Sonos startup cushion or additional network bandwidth.

All 20 canonical `scripts/check` stages passed after device/network access was
restored. Both recognition profiles compile; the enabled profile was never
flashed, and the configuration file was restored byte for byte. Tests include
worst-case allocation and renumbering, independent CRC/PCM decoding, an actual
11.008-second delayed ring reader and bounded overrun, plus real FFmpeg negative
controls. Independent review caught the diagnostic marker timeout; the corrected
30-second wait includes lookup, the 17-second suspension and delivery slack.
Final source review found no further issues.

Production ELF SHA256:
`cbd998fc43ed6feebeac113443f19b455b0105f2ab554a414a3c6da8d4dbfdf9`.
Private receipts are under `target/flac-buffer/`; original UART, decoder logs,
frame hashes and source recordings remain there, not in this evidence document.

Doctor Biz confirmed the ESP32 input cable uses the workstation front jack,
labeled Headphone in ALSA. Test input used that jack with rear Line Out muted,
Master/PCM at 0 dB and direct `plughw:CARD=PCH,DEV=0` playback. Before that
confirmation, rear-only playback produced no driven signal at the board.
The corrected routing delivered both tones, but 997 Hz left / 1703 Hz right
arrived reversed. This is recorded in Kata `cn4m`; neither wiring nor the
software stage responsible has been identified. These observations do not
calibrate analog voltage or gain.

The two-listener music run passed 120 seconds: each decoder produced exactly
5,760,000 stereo frames, with zero reported skipped/duplicate frames or decoded
discontinuities. Capture counter deltas were zero. Startup baseline already
contained 15 queue overflows, 480 software-dropped frames, two restarts and two
discontinuities; the pass does not erase those faults. Minimum telemetry sums
were 69,003 internal bytes and 875,872 PSRAM bytes; sums of regional minima are
not simultaneous allocation snapshots. Effective Wi-Fi protocol was `0x07`,
power save `0`, channel 11, initial reported association RSSI -46 dBm.

The second test used independent stereo noise at 48 kHz, amplitude
0.1, seeds 42 and 73, stored as signed 16-bit PCM. It exercises a different
compression load from music. Do not infer a maximum-bitrate bound from this
analog fixture: ADC noise, gain and conversion remain part of the path.

That noise run also passed 120 seconds with exactly 5,760,000 stereo frames per
listener, zero skipped/duplicate frames or decoded discontinuities and unchanged
capture fault counters. Its baseline had 16 queue overflows, 480 dropped frames,
two restarts and two discontinuities. Sampled minimum sums were 68,559 internal
bytes and 781,780 PSRAM bytes. No allocation failure appeared in either run.
Repeated MQTT/TLS connection attempts timed out during the music run; streaming
continued. These tests do not certify MQTT connectivity.

The real encoder-stall fixture passed with a 17-second suspension. Both original
listeners closed for the two-second freshness deadline, rather than a frame-write
timeout. Capture advanced during suspension and recovery with no added tracked
faults or board reset. Both replacement decoders produced exactly 480,000 stereo
frames (ten seconds), and a third listener received HTTP 503 while both slots
were occupied. EOF arrived about 1.78 seconds after the receipt-based estimate
of suspension; UART timing uncertainty prevents treating that as an exact
device deadline measurement. The runner stopped the input player and restored
production after the fixture. This establishes the tested encoder-failure isolation;
it does not exercise automatic Sonos reconnection.

The restored production image matched the tested ELF hash. A final two-listener
30-second noise check passed at 1,440,000 stereo frames each, with zero added
tracked capture faults. Build configuration and saved Sonos configuration matched
their originals, the controller was stopped and autoplay remained off. Test
playback ended; the analog playback device reported closed. Front-jack routing
remains enabled for the confirmed ESP32 cable, with rear Line Out muted.

Kata `qh4t` is closed for the bounded allocation/history work. `dctv` and `dp08`
received the results and remain open; `cn4m` records the channel reversal.
Machine-readable results and receipt hashes are in [flac-buffer.json](flac-buffer.json).

This bounded validation cannot establish long-soak reliability, behavior under
controlled RF/network interruptions, automatic Sonos recovery, or absence of
unreported capture gaps. Startup capture faults and channel order remain open.
