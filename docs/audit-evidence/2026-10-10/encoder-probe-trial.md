# Encoder boundary probe: actual trial 1

Doctor Biz, this bounded acquisition failed: raw overruns invalidated both developed sequences, and the original fixed-budget timeout remains recorded with collector exit 1. No complete valid 24-window OFF1/ON/OFF2 sequence exists. Acquisition, completion and valid-sequence verdicts are false. The MP3 performance defect remains P1, `wbz6`, open.

Implementation `5460cac` and collector `26d21c1` produced this run. The saved probe ELF SHA256 is `d5607b7f1e513987dd6031f9130d36fd27f044ad66e33c8b94032fa48752d66e`; this identifies the host artifact, not device attestation. Build checks are recorded in [encoder-probe-build.md](encoder-probe-build.md). This receipt was made by offline reading and hashing only; no hardware, network, playback, UART or process-control action occurred.

The PCG64 source used seed 20261010, NumPy 2.4.4, discrete uniform integers inclusive [-3277,3277], stereo signed 16-bit little-endian PCM at 48 kHz: 420 seconds, 20,160,000 frames. WAV SHA256 `9311d9ca5dfe1a5f9269631afe22647834207f2b3239571474acc8ac15f3228d` was independently checked against the saved recipe. One source launch, no loop, settled 12.003655 seconds before the one host RTS reset; assertion lasted 0.100982 seconds. The collection budget was fixed at 360 seconds. RTS reset is not a physical power cycle.

At device 166116 ms, sequence 1 failed at OFF2 interval 1 (6597 lifetime encoded frames, reset 1), after 16 paired reports. At 346626 ms, sequence 2 failed at OFF2 interval 3 (14025 frames, reset 2), after 18 reports; its two OFF2 reports were discarded settling windows. Sequence 3 had one settling report at expiry. Inactive initial FLAC sequence 0 is not MP3 acceptance evidence. First cumulative deltas remain unknown. Pre-reset buffered records are excluded using reset-release arrival timing and encounter order.

Six retained windows per phase were independently summed from raw UART cumulative frame differences and elapsed microseconds, with 1152 samples/frame and 48 kHz. These are diagnostics from invalid sequences:

| Sequence | Phase | Frames | Elapsed us | Audio/elapsed | Codec wall us/frame |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | OFF1 | 2487 | 60065156 | 0.993720885 | 21909.432 |
| 1 | ON | 2455 | 60094815 | 0.980450643 | 21957.289 |
| 2 | OFF1 | 2477 | 60070344 | 0.989639746 | 21931.510 |
| 2 | ON | 2467 | 60081749 | 0.985457331 | 21922.608 |

All four spans generate less than real-time audio. Missing OFF2 prevents a complete same-image comparison, repeatability claim or instrumentation overhead estimate. Do not stitch reset sequences together. Codec wall time includes preemption; task runtime includes charged interrupts and excludes the unfinished slice. Neither proves exclusive codec CPU cost or historical cache causation. OFF zeros represent disabled subdivisions, not free work. Background MQTT TLS select-timeout failures remain evidence without causal attribution.

Actual MP3 storage receipts show raw payload 768000 bytes, stamps 1600 bytes, chunk length/capacity 1920/1920, PCM 4608 bytes, Shine config 95888 bytes and bitstream 4096 bytes. Output required/capacity are 960/960 bytes; length is 0 at sequence 1 storage and 960 at sequences 2 and 3. Shine policy is `MALLOC_CAP_SPIRAM|MALLOC_CAP_8BIT`; addresses alone establish no cache cause. MP3 storage and accounting rows parsed without missing-field errors or accounting mismatches.

Retained capture spans have zero changes in all eight fault totals and increasing frames. Startup already had 15 queue overflows, 480 software-dropped frames, 2 restarts, 2 overflow restarts and 2 discontinuities. Later stability is not a clean boot and does not cancel encoder raw loss.

All six saved status GETs returned 200. Before, after-boot and final receipts retain MP3/med, stopped, autoplay false and recognition false. Routing readbacks retain Headphone on, Line Out off and Master/PCM 0 dB. Source monitoring retained 1862 running observations, no unexpected exit and an empty playback log. Cleanup ended 372.781236 seconds after source launch, inside its 388-second bound; source cleanup returned 0. Parent verified source and UART released. Maximum measured liveness gap was 205.211 ms against requested 200 ms, and timeout observation was 89.136 ms late; no restart extended the budget. Liveness and routing do not prove ADC samples; host arrival stamps do not prove device execution times; zero listener endpoints cannot exclude brief churn.

The immutable local acquisition is `target/kata-hardening/encoder-probe-device/trial-1`. The paired [JSON receipt](encoder-probe-trial.json) records independently computed hashes for every original artifact, collector tools, exact source recipe, metrics and verdicts. Raw UART is 89455 bytes and 359 complete lines; contiguous arrival offsets cover those bytes with no partial tail. The sanitized replay lives at `scripts/fixtures/encoder-probe/device-rejected`; its provenance maps selected lines and arrivals back to originals. Independent review in `target/kata-hardening/encoder-probe-trial-1-review.md` approved acquisition evidence while preserving the failure. The original sole error is `TimeoutError('fixed global collection budget exhausted; invalid sequences retained')`; final readbacks and cleanup did not replace it.

No private player identifiers, MAC/IP addresses, audio bytes or ELF bytes are published here. The separately planned default-ELF same-source control has no outcome in this receipt. Fix sustained generation/raw loss before release; this failed trial is diagnostic evidence, not performance acceptance.
