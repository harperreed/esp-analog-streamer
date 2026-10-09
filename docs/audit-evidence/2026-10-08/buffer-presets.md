# Configurable buffer acceptance

Source: `9964baf`. Firmware ELF SHA256:
`0a8ee33c9a74587acd05ff23e44a0ace59c666fdc40c8704b3d7c9a3c974f4cf`.
The tests ran October 8 Chicago time, October 9 UTC. Machine-readable results
and receipt hashes are in [buffer-presets.json](buffer-presets.json).

Doctor Biz selected Low 1 second / Medium 4 seconds / High 8 seconds. Medium
is the default; saving another preset changes the next Play. Existing Sonos
queues keep their original URL. The feature uses the existing FLAC allocation.
Recognition-enabled firmware keeps live streaming and explicitly rejects presets.

## Verified behavior

All 20 stages of `scripts/check` passed. The recognition-enabled profile also
compiled, was never flashed, and its temporary configuration change was restored
byte-for-byte before the production rebuild. Sol reviews and the final review
found no outstanding source defects. Tests cover settings migration, failed saves,
queue immutability, manual Stop, route validation, retained frame selection,
resets, overruns, bounded fill/freshness, socket failure and request half-close.
Claxon and FFmpeg decode all three retained histories exactly after ring wrap.

Chromium exercised the actual board page and API. Low, Medium and High saved and
survived page reloads with no page errors. Destination and autoplay were preserved.
Real Sonos reached Playing on Medium; saving High left its current queue and URL
unchanged. Stop succeeded, the next Play used High, and Stop succeeded again.
These were bounded playback checks, not a speaker-output recording or an endurance
test. No Sonos volume writes occurred.

For each preset, two concurrent FFmpeg processes decoded 12 seconds from the live
and buffered endpoints while independent stereo noise fed the physical LINE IN.
Each output contained exactly 576,000 stereo sample frames and no decoder warnings.
The live prefix matched the buffered PCM at these offsets; every overlapping
sample matched exactly:

| Preset | Retained blocks | Matched offset | Exact overlap |
| --- | ---: | ---: | ---: |
| Low | 12 | 49,152 samples / 1.024 s | 526,848 samples |
| Medium | 47 | 192,512 samples / 4.010667 s | 383,488 samples |
| High | 94 | 385,024 samples / 8.021333 s | 190,976 samples |

The front Headphone jack fed the ESP32; rear Line Out remained muted. Master,
PCM and Headphone stayed at unity gain. The known channel reversal does not
invalidate a comparison of two streams sharing the same capture path, but it
remains an unresolved fidelity issue.

High survived a board reset. A connection opened 5.46 seconds after reset and
waited 8.22 seconds for HTTP 200 while the history filled. The controller stayed
stopped with autoplay off. Medium was then restored and confirmed through the API.

## Capture acceptance remains red

During the first Sonos TLS/setup sequence, measured counters changed as follows:

| Counter | Before, boot 51.47 s | After, boot 61.56 s | Increase |
| --- | ---: | ---: | ---: |
| Queue overflows | 14 | 23 | 9 |
| Software-dropped stereo frames | 480 | 720 | 240 |
| Capture restarts | 2 | 3 | 1 |
| Discontinuities | 2 | 3 | 1 |
| Settling-discarded stereo frames | 26,400 | 50,400 | 24,000 |

This reproduces the existing setup failure; buffering does not protect the capture
producer from it. Later metrics during the paired PCM comparisons showed no further
counter increases. Their last sample preceded decoder cleanup, and counters cannot
exclude undetected DMA loss. The lowest observed internal minimum sum was 15,659
bytes; it sums per-region lows and is not a contiguous-allocation measurement.
The reset test also retained startup faults. Existing MQTT TLS failures remained.

Recommendation: fix setup capture loss before calling the firmware robust, then
run the controlled network interruptions and long-duration release gate. These
checks prove retained source history, not a guaranteed Sonos playback cushion,
maximum media age or extra network bandwidth. Recognition-enabled behavior has
host tests and a target compile, but no device run in this checkpoint.

Kata `bzgt` covers this bounded feature. `dctv` retains broader continuity and
receiver-cushion validation; `dp08` retains capture acceptance. MP3 feasibility
is `e8hd`; no MP3 encoder or dependencies were added.

Raw private artifacts and throwaway device runners live in
`target/buffer-presets/`. The first UART collector was deliberately terminated
after the browser and PCM checks, then a separate collector captured the reset
test. Its exit 143 is collector cleanup, not a firmware restart. Final state:
production firmware, Medium, saved destination preserved, autoplay off, controller
stopped, workstation analog playback closed, rear Line Out muted.
