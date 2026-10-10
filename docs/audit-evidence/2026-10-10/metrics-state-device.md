# Metrics-state device evidence, 2026-10-10

Doctor Biz: runtime acceptance fails. The new `55d5416` default image produces about **0.785x** audio in five complete windows versus about **1.0x** on the exact archived `eb38c65` control. Both fail fast-listener survival before the reclamation probe. The diagnostic probe was canceled after actual raw overruns; it is not a completed or passed OFF/ON/OFF acquisition. These separate executions show an association, not its cause.

| Retained trial | Complete producer windows | Frame deltas | Generation ratios | Actual encoder loss |
|---|---:|---|---|---|
| New default | 5 | 329, 328, 326, 329, 329 | 0.784962, 0.787163, 0.780429, 0.788851, 0.789197x | Six complete Overrun boundaries; resets 1→6; each window reports one raw overrun |
| Exact archived control | 5 | 416, 417, 415, 417, 420 | 0.996198, 0.998709, 0.994465, 0.999667, 1.006416x | No overrun boundary; complete windows raw overruns 0 / resets 0 |
| New probe, canceled | 13 | 387–392 per complete window | 0.928372–0.940766x | Four Overrun boundaries; resets 1→4 |

Generation uses actual producer timestamps, 1152 samples per frame at 48000 Hz, and cumulative frame differences. Default/control MP3 baselines reset to 0 on format switch; prior Flac frames 2 are excluded. Probe retains frames across replacement: its first sequence baseline is 2 and later sequence baselines are 1233, 2434, 3618 and 4839. Subtracting these actual counts avoids inventing audio. Default/control W6 are truncated and excluded. Both first two complete default/control windows precede listener joins and have zero listener endpoints. Startup capture overflow baselines differ: 15 versus 16; they are not treatment deltas.

Default fast listener departs at 33.745541 s after 9.002135 s, “fell a ring behind,” reporting 179 frames / 171840 bytes sent. Control fast departs at 46.362385 s after 20.771309 s with the same reason. Both wrapper/helper passed values are false with `fast listener ended before reclamation probe`. Neither retains decoded MP3/framehash proof or a third-client join. Sent or accepted socket bytes are not decoded audio. Longer control survival does not clear its failed gate.

Five complete default and 13 owned probe diagnostic dumps pass receipt structure checks. All report unavailable=0 and unassigned=0 and saturated=false/invalidated=false. Query maxima reach 8 µs for default and 12 µs for probe, measuring the targeted query only. Acquisition spans include logging, scheduling and scanning; neither those spans nor query maxima measure full observer overhead or establish a hard bound.

| Final complete owned dump | Attempted | Exporter-attributed Ready samples | Exporter-attributed Blocked/Other | Supervisor free stack |
|---|---:|---|---|---:|
| Default, 55.975652–57.177283 s | 50 | Listener 7, Encoder 12 | 0/0 | 3108 bytes |
| Probe, 133.186757–134.613872 s | 121 | Encoder 23; Listener 0 | 0/0 | 3108 bytes |

Exporter-attributed samples are Ready throughout retained dumps. Same-core supervisor preemption can produce Ready. Independent fields need not sum to attempted (final phase totals are 51 for default and 122 for probe). No individual sample timestamps identify continuous occupancy, time in state, lock ownership or samples within an exact producer interval. Zero phase counts do not prove an operation never ran or blocked. This evidence does not identify scheduling, observer-overhead or code-cache root cause.

Coverage requires a complete later diagnostic acquisition whose **start** follows the producer endpoint. Default dumps cover all five complete windows and both departures, but not the final 63.633198 s overrun or terminal tail. Probe dumps cover W1–W12 and all four overruns, but not W13 ending at 138.783489 s or cancellation tail. JSON retains each explicit interval and later starts. Complete capture counters bracket default/control W2–W5 and probe W2–W12; first windows lack preceding capture, final probe window lacks following capture. All nine retained capture fault counters stay at startup baselines, which does not clear explicit encoder raw loss.

Probe strict `analyze_acquisition` replay matches saved analysis and boot selection. Owned ROM banner starts byte 4201; reset marker starts byte 4229 / line 22, after recorded reset release at host monotonic 1128356.450277389. Prior output remains intact and excluded from owned summaries, including its 876-byte metrics watermark. Six fresh Storage paths (Flac plus five MP3 sequences) and 13 Window paths run; five MP3 storage receipts validate. Sequence counts 3/3/3/3/1 remain partial OFF1, without ON/OFF2/completion. Whole UART totals are 36488 bytes, 252 complete lines and 181 tail bytes for default; 25705 bytes, 184 complete lines and 645 tail bytes for control; and 73012 bytes, 396 complete lines and 168 tail bytes for probe. Probe selects 375 complete owned lines; its tail is an incomplete capture report, not a receipt.

Observed free stack minima for default are metrics 1132, listener 1664, encoder 5196 and supervisor 3108 bytes. Control has metrics 1164, listener 1668 and encoder 5188 bytes; its supervisor watermark is absent. Probe has metrics 908, encoder 4932 and supervisor 3108 bytes; its listener path was unexercised. Capture task watermark is absent. Historical eb38 probe also reached metrics 908 bytes (encoder 4940 bytes), with generation 0.903348–0.913575x and actual overruns. These separate-run observations do not establish improved margin or future stack safety. Complete observed encoder/listener export receipts report zero dropped/full/contended/poisoned and sticky=false; probe has no listener receipt. No terminal drain acknowledgment exists.

Exact copied image SHA256 values:

- New default: `ced962b2136d7c3fbe8dd97dd4de2d47f1c39e72bef6dee76d92fa5c59218d1c`
- Archived eb38 default/control and final restored image: `d88a65965ff456c7500df1ae05d5ac548ef4290b35524215bf3e27175378b991`
- New probe: `409849952981fe7dec9c62b6d04bd558bdbc7bea915b4207666a55e9a6513c31`

Copied images match saved acquisition provenance; full device attestation remains false. The 420 s stereo 48 kHz fixture hash is `9311d9ca5dfe1a5f9269631afe22647834207f2b3239571474acc8ac15f3228d`. JSON records every original trial/parent-stop hash, checked helper/image/fixture provenance, source analysis hashes and successful flash-log hashes. Independent paired review confirms default/control arithmetic, receipts and limits. Original raw artifacts and existing reports remain intact; the probe prose correction distinguishes ROM from reset offset.

Both default/control supervision receipts report cleaned=true/errors=[]; source cleanup ends with return code 0. Probe records owned SIGTERM / SystemExit(15) and source cleanup return code 0, with initial broad ownership lookup rejected before signaling. Parent reports acquisition sessions reaped. Final archived-default restore log records flash completion at 2026-10-10T22:24:47Z. Later final-resting-state readback at host monotonic 1128819.582919507 reports active unified MP3/Medium, autoplay=false, recognition=false, Sonos stopped and quiet at −65.7 dBFS. Routing reads Headphone on, Line Out off, Master mono at 0 dB/on, and PCM at 0 dB; fuser UART and exact aplay lookup both return 1 with empty output. Source process/routing evidence does not prove ADC fidelity; quiet resting level follows source cleanup.

Recommendation: retain failed delivery/raw-loss gates and canceled probe status. Diagnose the default/control production-rate association before runtime acceptance. Phase sampling narrows observations but does not supply root cause, overhead clearance, worst-case stack margin or successful delivery.
