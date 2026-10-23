# Sender diagnostic matched-run readout

Doctor Biz, the diagnostic run reproduces MP3 delivery failure, but it does **not** reproduce the baseline's MP3 joint workload: every first MP3 listener departed before listener 2 joined. The dominant measured MP3 boundary is wall time inside `send`, with almost no WouldBlock/poll activity. This does not identify the cause of that time or establish instrumentation overhead.

Run `20261010T174257Z` completed all three 90-second captures and cleanup without runner error. Image commit `0d9d014`, ELF SHA-256 `5ee637452567a4ef37af6de913a27c8a502d4f8b9b5c1a41fca8ef9fde3dbf67`. Numeric evidence: [sender-analysis.json](sender-analysis.json). Analyzer reads retained local receipts, independently recounts decoder framehash records and complete FLAC CRCs; it runs no new decode or hardware operation.

## Actual receiver intervals

Rates are complete received audio seconds per host wall second. Solo intervals end at actual EOF; they do not extend across disconnected time.

| Trial | Listener 1 solo | Listener 2 later solo | Actual joint interval | Joint listener 1 / 2 | Natural EOF seconds from first admission |
|---|---|---|---|---|---|
| MP3 first | 0.594 over 26.147 s | 0.522 over 30.213 s | None | Not measured | 26.147 / 60.663 |
| FLAC | 0.984 over 30.787 s | — | 59.213 s | 0.991 / 0.938 | Neither through 90 s |
| MP3 repeat | 0.695 over 23.798 s | 0.559 over 24.931 s | None | Not measured | 23.798 / 55.381 |

Second admission was 30.450 s in each MP3 trial. Neither MP3 trial has a survivor recovery window from a joint phase. UART attributes all four MP3 departures to “fell a ring behind.” FLAC survived the bound; listener 2 still accumulated about 3.66 seconds of complete-audio deficit during joint reception. No sustained real-time success follows from its survival verdict.

## Payload departure aggregates

| Trial / listener | Send / payload wall | Poll / payload wall | Send / frame-write wall | EAGAIN / poll calls | Send maximum | Send calls over 10 ms |
|---|---|---|---|---|---|---|
| MP3 first / 1 | 87.70% | 0% | 90.92% | 0 / 0 | 234.059 ms | 114 / 647 |
| MP3 first / 2 | 88.17% | 2.77% | 92.25% | 1 / 1 | 260.533 ms | 130 / 659 |
| FLAC / 1 | 9.26% | 76.92% | 10.64% | 343 / 343 | 27.501 ms | 181 / 1735 |
| FLAC / 2 | 10.25% | 63.79% | 13.70% | 186 / 186 | 26.305 ms | 171 / 1031 |
| MP3 repeat / 1 | 82.46% | 0% | 91.58% | 0 / 0 | 242.970 ms | 94 / 689 |
| MP3 repeat / 2 | 74.05% | 0% | 87.93% | 0 / 0 | 448.670 ms | 85 / 581 |

MP3 accepted 621,120 / 630,720 / 661,440 / 557,760 payload bytes, exactly the corresponding completed-frame byte totals. Its only poll lasted 808.572 ms; the subsequent ready-to-send gap was 131 µs. No repeated EAGAIN after readiness occurred. MP3 frame-write wall minus send/poll wall was 2.220 / 1.354 / 1.775 / 2.394 seconds. That residual includes descriptor mode operations, wrapper work, clock accounting and scheduling; it is not isolated scheduler time.

FLAC poll maxima were 3.506 / 2.706 seconds, readiness-to-next-send maxima 10.090 / 5.150 ms, and no ready-then-EAGAIN. Each FLAC peer has one error-ready poll and one send error at departure after the bounded receiver closes. The final failed writes retained 8,295 / 5,949 accepted bytes beyond complete sent frames. These are stack acceptance counts, not reader delivery. Sender complete-frame counts (1046 / 654) exceed receiver complete prefixes (1043 / 651); do not equate those measurements.

## Encoder, capture and prefix checks

MP3 encoder complete report-window generation ratios were 0.936–0.956 and 0.913–0.955, versus retained baseline 0.997–1.003 and 0.994–1.007. Mean frame encode wall was 22.657 / 23.235 ms, versus 21.629 / 21.610 ms baseline. Each MP3 trial observed two raw-read overruns and reset counter reaching two. Zero-listener windows were already around 0.96× generation, so payload socket probes alone cannot explain the encoder slowdown. FLAC generation stayed 0.998–1.002 with zero observed raw-read overruns/resets; mean frame encode wall was 8.152 ms versus baseline 7.860 ms.

All eight tracked capture fault deltas were zero per trial; publication advanced at 48,000 / 48,000 / 48,002 frames per device-log second. Encoder overruns are a separate observation and must not be hidden by the capture counters.

| Trial / listener | Complete frames | Independently recounted PCM frames per channel | Excluded final bytes |
|---|---|---|---|
| MP3 first / 1 | 647 | 745344 | 0 |
| MP3 first / 2 | 657 | 756864 | 0 |
| FLAC / 1 | 1043 | 4272128 | 5877 |
| FLAC / 2 | 651 | 2666496 | 918 |
| MP3 repeat / 1 | 689 | 793728 | 0 |
| MP3 repeat / 2 | 581 | 669312 | 0 |

Every recounted prefix equals complete coded frames × block samples and passes the saved decoder-log integrity checks. All 1694 complete FLAC frames pass CRC-16. Excluded FLAC tails remain unvalidated; MP3 does not establish CRC or source sample continuity. Valid prefixes do not negate the raw encoder overruns or establish analog continuity/playout.

## Interpretation and next experiment

Keep MP3 delivery reliability at P1. For these four failed peers, measured send-call wall dominates and repeated socket writable pressure does not. The pinned lwIP nonblocking send still posts TCP/IP work and waits for completion; scheduling, mailbox service, copy/cache work and other stack processing remain possible contributors. Clock spans include errno capture and rescheduling. No CPU-time attribution follows, and no buffer-size, Nagle, priority or PSRAM change is justified yet.

This run differs materially from retained baseline `20261010T035157Z`: cold boot/new image, slower MP3 encoding even without listeners, raw encoder overruns, and no MP3 joint overlap. Parent reports boot radio RSSI −49 dBm versus baseline −40 dBm; those readbacks are not contemporaneous per-packet measurements. Baseline has no separated send/poll counters. These differences prevent claiming diagnostic overhead or a matched transport regression.

The smallest next experiment is the parent's archived-baseline-image cold MP3 control followed by exact diagnostic-image cold MP3 repeat, using the same source, 30-second second admission, 90-second bound, MP3 selected throughout, and unchanged transport settings. Compare pre-admission zero-listener encoder rate, actual overlap, EOFs and capture/encoder faults before interpreting the send boundary. If the new image alone repeats slow zero-listener encoding, examine the bounded build/code-placement difference and probe cost separately before transport tuning. If both images share the slowdown, investigate changed run context. If diagnostic MP3 send dominance persists with recovered encoder throughput, a separately scoped timestamp at lwIP mailbox dispatch/completion is the next direct boundary measurement.

UART host timestamps are serial-chunk arrivals and approximate phase placement. Whole-departure aggregates cannot separate solo and joint sender cost for FLAC. No AP packet evidence is present; no failing hop or proven root cause is identified.
