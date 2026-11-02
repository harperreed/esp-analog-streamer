# No-listener MP3 cold controls

Doctor Biz, the archived image kept MP3 current while the diagnostic image generated 0.9638× audio and incurred one raw overrun/reset, with no media listeners attached by the runner. This independently repeats the earlier image association before listener admission. It does not establish the cause.

| Whole-window measure | Archived old-1 | Diagnostic diag-1 |
| --- | ---: | ---: |
| Encoder elapsed | 60.089896 s | 60.089342 s |
| Successful encoded frames | 2,504 | 2,413 |
| Generated audio / elapsed | 1.000102× | 0.963765× |
| Mean encode wall / successful frame | 21.567043 ms | 22.317162 ms |
| Encoder task runtime charge | 94.5734% | 95.4168% |
| Raw overruns / encoder reset delta | 0 / 0 | 1 / 1 |
| Eight capture fault deltas | all zero | all zero |

Diagnostic mean codec-call wall rose 0.750119 ms/frame, or 3.4781%; generation fell 3.6337 percentage points. These are six consecutive whole ten-second report windows per image, independently reconstructed from original UART bytes and arrival offsets. Generation equals total frame deltas × 1,152 / 48,000 divided by summed `elapsed_us`; mean encode wall equals summed codec-call wall divided by those frame deltas. Arithmetic averages of window ratios and lifetime frame counts are not used. All six runtime windows are marked valid. The diagnostic UART independently logs the raw-ring-behind warning, matching the one raw-overrun/reset increment.

The full host measurement lasts 75 seconds after 12 seconds source settling. Whole encoder intervals end at device log times 55.682–105.752 seconds for old and 55.893–105.963 seconds for diagnostic; preceding report endpoints start them at 45.662 and 45.873 seconds. UART arrivals identify windows wholly inside the host measurement. They are read-completion timestamps, not device execution timestamps. Sum-of-report elapsed times is the denominator; device log timestamps have coarser precision. Capture reports cover separate intervals: old 41.972–102.822 seconds, diagnostic 41.823–102.743 seconds. Their fault deltas cannot be called coverage of every instant of the full 75-second host or 60-second encoder window.

Capture baseline counters were already nonzero. Old starts/ends with 18 queue overflows, 480 software-dropped frames, two restarts, two overflow restarts and two discontinuities. Diagnostic starts/ends with 15 queue overflows and the same other values. Read errors, supervisor restarts and health restart requests are zero in both. Thus all eight **interval deltas** are zero; neither run establishes a fault-free boot. Capture frame deltas are 2,920,800 / 60.850 device-log seconds and 2,924,160 / 60.920 seconds, each 48,000 frames/s at that timestamp precision. The encoder loss remains real despite unchanged capture faults.

Both use the same 300-second fixture SHA-256 `925c86a822206316a942b27a569ba5dd1804095f2c41d8ffba7776dacedc4137`, with `aplay -q -D plughw:CARD=PCH,DEV=0` and one fixture argument. Readbacks show front Headphone on, rear Line Out off, both at 0.00 dB. The helper made no mixer writes and did not read Master or PCM, so those settings are not freshly attested by these receipts. App meter baseline and final readings are −23.6 dBFS for both images; recognition is disabled in every app-state receipt. Matching fixture bytes and meter readings support the input control but do not prove sample-identical audio at the ADC.

Archived ELF SHA-256: `c3bf69b149b9386b27346cfe26e023c191054a490374a7d5f480c9b8d40159d9`. Diagnostic ELF SHA-256: `5ee637452567a4ef37af6de913a27c8a502d4f8b9b5c1a41fca8ef9fde3dbf67`. Each copied ELF was independently rehashed against the run receipt. They are operator-identified flashed images, not independently attested device images. The parent flashed each chosen ELF; the helper opened UART once, used the established RTS reset procedure and held it open through measurement. Although the boot prints POWERON_RESET, this procedure is a reset, not a verified physical power cycle.

All four Sonos status reads per run verify stopped playback, MP3 active/configured, Medium preset and autoplay false; saved config matches before/after. No runner media connection, Sonos Play or runtime payload probe was invoked. Every encoder report in the full retained UART has zero listener start/end counts, and no `stream-listener` record appears. Those observations cannot exclude extremely brief external listener churn between reports, but active payload socket probes cannot account for sustained zero-listener encoding cost in this controlled procedure.

Both source processes remain alive through final status receipts and are deliberately terminated afterward; cleanup returns zero. Both retained `aplay.log` files are empty. Diagnostic cleanup includes stderr byte-offset receipts (zero before/after); old-1 ran the earlier helper without those offsets. Old still retains source liveness and cleanup timestamps, but would not support byte-timed attribution of a nonempty stderr message. No read_header error occurred in these retained logs.

The earlier cold controls also showed archived prelistener generation 0.9996× versus diagnostic 0.9516×, with diagnostic raw loss. Together these two independent pairs support an image-associated encoder regression; they do not distinguish executable placement, heap/cache interaction, shared work or remaining context effects. See [sender controls](sender-controls.md). No arbitrary extra flash or layout padding is warranted. Keep encoder loss at P1 and avoid shipping/overhead-acceptance claims until bounded attribution separates this regression from MP3's generally narrow capacity margin.

Sanitized numeric receipts, complete selected counters, source levels, routing readbacks, relative/UTC events and original-file SHA-256 values are in [encoder-cold.json](encoder-cold.json). Private control identities and addresses are excluded. This recount ran offline; it did not build firmware, flash, open UART, play audio or contact the board/Sonos.
