# Encoder telemetry: default-image device failures

Both bounded trials fail acceptance. Logging isolation does not resolve delivery;
the encoder telemetry queue also fills. Do not enlarge queues or claim a transport
fix from these results. Source is 05cf802; the later routing harness fix c2ba29a
changes no firmware. Exact image and original evidence hashes are in the paired
JSON. A saved image hash is not running-device attestation.

The same 420-second, 48 kHz stereo noise fixture fed the front jack. Settings
remained stopped MP3/Medium, autoplay and recognition off. Each test reset the
board; startup capture faults remain part of the evidence.

| Observation | Two-listener trial 1 | Slow-reader trial 2 |
| --- | --- | --- |
| Delivery | Decoded 7.368 and 9.072 seconds of requested 30 each; both fell behind | Fast peer EOF after 1,391,040 bytes accepted; no fast decoded artifact |
| Reclaimed slot | Not exercised; wrapper stopped after failure | Third connection decoded 10 frames, 11,520 samples, 0.240 seconds |
| Encoder export | One full drop; window 4 missing | Three full drops; windows 5 and 6 missing |
| Raw encoder loss | Exported windows report zero; missing data prevents clearance | Window 7 explicitly reports one overrun/reset; window 8 retains reset total |
| Observed listener export | Zero reported drops | Zero reported drops |
| Metrics minimum free stack | 1,164 bytes | 1,148 bytes |
| Listener/encoder minimum free stack | 1,672 / 5,188 bytes | 1,664 / 5,192 bytes |

Trial 1's final framehash accounting exceeds the frozen pre-failure summary for
listener 2: it continued decoding during failure observation. Use actual retained
framehash totals. Trial 2's fast accepted bytes represent 34.776 seconds of MP3
frames; those bytes are not a decoded-audio measurement.

Producer-timed windows show roughly 0.962–0.997 times real time in trial 1's
individual exported windows and 0.956–1.001 in trial 2. Missing windows permit only
cumulative frame deltas across their gaps, not reconstructed per-window costs.
Trial 2's raw reset is real despite stable capture counters.

All nine capture fault counters stay unchanged across the complete reports:
three in trial 1 and five in trial 2. Trial 1 capture frames advance 477,360 to
2,249,760; trial 2 advances 477,840 to 4,008,720. Startup overflow/restart/discard
counts remain nonzero. These are bounded relaxed observations, not a coherent
snapshot or proof against every counter wrap or unrecorded interrupt. Neither
trial reports a capture-task stack watermark.

Trial 1 ends with a 901-byte incomplete capture line. Exclude it from complete
coverage and heap minima; it cannot certify a final report. Trial 2 ends on a
complete encoder receipt, which is not a queue-drain acknowledgment for future
publications. Neither default trial has byte-arrival provenance. UART prefix or
host arrival must not replace producer timestamps. Sticky encoder loss prevents
full diagnostic acceptance regardless of observed zero listener loss.

Complete-report internal free/largest/minimum-sum minima are
66,511 / 59,392 / 55,451 bytes for trial 1 and 61,287 / 55,296 / 55,951 for trial 2.
PSRAM free/largest/minimum-sum minima are 2,770,824 / 2,359,296 / 360,008 and
2,766,440 / 2,359,296 / 363,548. Minimum sums combine per-region lows, including
startup; these are not safe allocation allowances.

The prior bb56186 slow trial reported a listener full drop and 1,156 bytes free
metrics stack. The older 3aa5b5f slow trial measured 580 bytes. Keep those source
identities distinct. Separate trials and differing listener workloads do not
establish cause or a controlled performance regression.

Both wrappers retained failed verdicts, final configuration/routing receipts and
successful source/helper cleanup. Parent reaped each session and found no UART
owner or aplay process before the next reset. Independent review verified all
original manifest hashes, complete-line filtering, decoded counts, producer
chronology, capture bounds, memory measurements and cleanup limits. No original
trial artifact was rewritten.

Severity: delivery, raw loss and diagnostic export loss block acceptance. The
probe image's separate stack/throughput gate remains distinct. Kata 21tp, wbz6
and transport/recovery gates remain open.
