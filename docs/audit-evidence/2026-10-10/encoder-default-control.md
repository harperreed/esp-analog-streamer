# Default-image control analysis

Doctor Biz, the saved normal unified MP3 image stayed current through this bounded no-listener measurement. This is an independent normal-firmware result, not a probe harness pass. The unchanged collector exits 1 with completed=false and acquisition_complete=false because the default image intentionally emits no probe sequence. Its sole error is the expected fixed-global-budget timeout; there are no additional recorded acquisition or cleanup errors.

## Provenance and controls

Saved ELF SHA256 is 9894ceef57743a15b000f88a79eda35ad168753943118d1f614532f96b7efa11. Driven 420-second fixture SHA256 is 9311d9ca5dfe1a5f9269631afe22647834207f2b3239571474acc8ac15f3228d. These hashes identify saved artifacts, not independent device attestation. Source starts at host monotonic 1118837.105529870; reset releases at 1118849.209628701, about 12.104 seconds later. This preserves source-before-reset ordering. Budget exhaustion occurs at 1119209.219904328, about 360.010 seconds after release; final state/source checks occur during the bounded cleanup allowance.

All six HTTP receipts return 200 without errors. Before/after saved configuration agrees; final receipts preserve stopped MP3, Medium, autoplay false and recognition disabled. Routing readbacks match exactly, including Headphone on and Line Out off. All 1,862 source-liveness samples report the player alive, including the final status event; cleanup deliberately terminates it and returns zero. aplay stderr is empty, with zero-byte receipts before/after cleanup. Source liveness and routing do not prove sample-identical ADC input.

Raw UART explicitly logs active format Mp3 at device 5465 ms. All 35 post-reset encoder reports have zero listener start/end counts; no stream-listener join/departure record occurs after reset. Brief unlogged external churn cannot be excluded solely by endpoint counters.

## Independent measurement

The recount filters lines by their original host arrival at or after reset release, excludes the unknown first cumulative frame delta, and discards the first two report windows for settling. Later frames/resets use differences of successive cumulative counters. Summed elapsed_us is the denominator. Capture coverage uses original raw-line encounter order: the last capture report before the preceding encoder report and the first capture report after the ending encoder report.

| Measure | First six retained, reports 3–8 | All covered, reports 3–34 |
| --- | ---: | ---: |
| Encoder endpoint range, device seconds | 25.475–85.535 | 25.475–345.825 |
| Complete windows | 6 | 32 |
| elapsed_us | 60058901 | 320287614 |
| Encoded-frame delta | 2503 | 13345 |
| Generated audio seconds | 60.072 | 320.280 |
| Audio/elapsed | 1.000218103× | 0.999976228× |
| task_runtime_us | 57280550 | 305971228 |
| Runtime charge/elapsed | 95.37395631% | 95.53014685% |
| encode_wall_total_us | 54164099 | 288805928 |
| Codec wall us/frame | 21639.671994 | 21641.508280 |
| raw_wait_total_us | 1939402 | 10595606 |
| Publication lock-wait us | 134976 | 721500 |
| Publication held us | 174542 | 934239 |
| Raw attempts / successes / retries / empty | 16448 / 14017 / 2399 / 32 | 86924 / 74732 / 12056 / 136 |
| Raw overruns / reset deltas | 0 / 0 | 0 / 0 |
| Runtime accounting | all valid | all valid |
| Maximum individual codec wall | 29617 us | 29617 us |

The 27 rolling six-window covered summaries range from 0.999707969× to 1.000218103× generation, 95.24024850% to 96.16015640% task runtime charge, and 21.630358513 to 21.648501399 ms codec wall/frame. These intervals remain consecutive: no restart or fault interval is silently removed.

First-six capture bracket: raw lines 142–176, device 21.615–92.405 seconds, 3,398,160 frames advanced. All-covered bracket: lines 142–303, device 21.615–355.555 seconds, 16,029,360 frames advanced. All intermediate capture frame counters increase strictly; all eight fault counters remain constant throughout. Baseline already contains 14 queue overflows, 480 dropped frames, two restarts, two overflow restarts and two discontinuities; read errors, supervisor restarts and health restart requests are zero. This establishes interval stability, not fault-free startup.

The final complete encoder report, number 35, spans 345.825–355.835 seconds and advances 416 frames over 10.010637 seconds, with zero raw overruns/reset delta and valid runtime accounting. It is explicitly excluded from the covered aggregate because no later capture report brackets its ending encoder report. Its counters remain in the machine receipt. All 35 encoder reports have zero cumulative resets and zero per-window raw overruns, and the UART contains no fell-behind warning.

[Sanitized numeric receipt](encoder-default-control.json) records exact report/line ranges, component sums, per-window capture coverage and all rolling summaries. The offline script `analyze_default_control.py` rebuilds these numbers solely from saved UART, arrival, event and result files.

## Interpretation and next step

This current default image does not reproduce the sustained no-listener encoder loss seen in the feature probe or earlier diagnostic cold controls. Its first-six codec wall, 21.639672 ms/frame, is close to the archived cold control's 21.567043 ms/frame. Probe OFF1 measured 21.909432 ms/frame, 97.793195% runtime charge and 0.993720885× generation; ON measured 21.957289 ms/frame and 0.980450643×. Feature OFF therefore cannot stand in for normal default firmware. These are sequential image-associated measurements, not proof that timers, cache placement, executable layout, MQTT or report ordering caused the difference. The probe lacks OFF2 and cannot establish timing overhead acceptance.

The smallest justified next change targets the diagnostic measurement path, not production codec/yield/memory policy. First audit the feature's OFF path against default for added branches, observations and changed report ordering, then design one bounded attribution experiment that changes a single measured diagnostic cost while preserving the full OFF1/ON/OFF2 policy and raw-loss rejection. Maintain the current default control as the production reference. A repeated default control around a revised probe would test repeatability before any causal claim. Do not reduce the eight-publication yield, add padding, change allocation policy or shorten phases on these data.

P1 wbz6 stays open: this control does not clear the earlier diagnostic-image regression, explain it, or validate listener delivery. Task runtime remains near 95.5% of one task's accounting budget and includes charged interrupts; codec wall includes preemption. Before shipping, resolve the diagnostic perturbation and retain the separate listener throughput/raw-loss acceptance gates. No automatic FLAC fallback is justified.

The numeric receipt includes SHA256 and byte counts for every saved acquisition artifact, including raw UART/arrival records, source liveness, status/events, result and ELF. Raw control identities are excluded. Parent post-run readbacks are separate from collector final receipts; their hashes identify saved readbacks without claiming that this offline analysis performed those requests.
