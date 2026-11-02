# Concurrent admission resource evidence — 2026-10-10

Doctor Biz: both saved acquisitions failed with `sustained stream terminated: 1` and exit code 1. Neither establishes Task 3 acceptance or production readiness. This report retains the failures and reports the bounded resource observations available before and after them.

One-worker acquisition failed while acquiring the second stream: the first peer received 9,689 bytes including headers and then FIN; the second received its 89-byte header without host-observed payload. The two-worker acquisition reached nonempty payload on both peers, then failed about 2.478 seconds after its two-stream drain phase started. Its peers received 50,009 and 25,049 total bytes. Both runs stopped before the pending-head phase; neither completed the planned stream baseline or TLS workload.

Each acquisition retained a full approximately 150.167-second owned boot UART and 14 valid resource samples. Every sample reported readiness and all expected admission workers found. Every owned host socket closed; cleanup errors and signals were empty. Final settings reads retained MP3, Medium, stopped, autoplay false and recognition false. Those ownership receipts do not prove a whole-device socket census or zero slot/lease growth across completed cycles.

## Resource observations

Idle rows use the third/fourth audio and resource records, surrounded by idle UART activity before stream joins. The conservative cleanup tail uses seventh through fourteenth records, after both listener termination/checked-close receipts and settled cleanup. Audio and resource records are separate snapshots and are not simultaneous. The JSON retains exact records and phase boundaries.

| Observation | One worker | Two workers |
| --- | --- | --- |
| Idle internal free bytes | 66,455; 66,467 | 61,587; 61,587 |
| Idle largest internal block bytes | 65,536; 65,536 | 59,392; 59,392 |
| Cleanup-tail internal free bytes | 65,899–66,023 | 60,875–61,011 |
| Cleanup-tail largest internal block bytes | 65,536 | 55,296 |
| Task count, all 14 samples | 19 | 20 |
| Idle worker historical untouched stack bytes | 2,208 | 2,208 / 2,632 |
| Minimum worker historical untouched stack bytes, full capture | 1,664 | 1,664 / 1,672 |
| Minimum metrics historical untouched stack bytes, full capture | 1,084 | 1,100 |
| Idle query elapsed microseconds | 318; 231,388 | 372; 381 |
| Cleanup-tail query elapsed microseconds | 167–183 | 274–278 |
| Full-capture query elapsed microseconds | 167–231,388 | 274–389 |

The paired idle free-memory difference is 4,868–4,880 bytes (about 4.76 KiB, or 4.9 kB), consistent in scale with an added 4,096-byte task stack plus other costs. This single pair with different executable layouts does not isolate causal overhead or establish a memory margin. Largest-block geometry is a separate observation: its idle difference is 6,144 bytes and its tail difference is 10,240 bytes. Do not equate those differences to allocation sizes.

No sampled task count caught the live listener tasks; the ten-second cadence missed the brief stream interval. The cleanup tails remain below their earlier idle free-memory values. Their bounded snapshots do not establish a leak, its absence, peak use or complete recovery. Stack watermarks retain historical untouched fill bytes, not a worst-case stack bound. Query elapsed time includes preemption and other scheduling delay; the one-worker 231,388-microsecond outlier must remain visible and is not evidence of 231 milliseconds of CPU scanning work.

## Audio and termination evidence

The full UARTs contain 13 one-worker and 12 two-worker encoder `reason=Overrun` boundaries. Overruns already occur before the workload; these totals must not be attributed to concurrent admission. The first audio snapshots report queue overflows 16/15 respectively, 480 software-dropped frames each, two overflow restarts each, 26,400 settling-discarded frames each and two discontinuities each; read errors and supervisor restarts are zero. These cumulative counters remain unchanged in the retained later audio snapshots.

In the one-worker UART the first listener exits with `encoded ring generation changed`; the nearby encoder boundary records an overrun at device event 47,891,080 microseconds, followed by listener exit at 47,892,576. Its checked close reports releasing a slot. The second listener later reports ConnectionReset (raw OS error 104) and checked close. Host cleanup had closed that peer, but the logs alone do not prove the reset's origin. The two-worker UART reports both listeners exiting with `encoded ring generation changed` at device events 51,644,935 and 51,647,911 microseconds, followed by checked closes releasing the two held slots. These receipts support the observed termination reason; they do not establish why the encoder overran or assign it to the added worker.

Host reset monotonic time plus device microseconds gives approximate alignment only. UART formatting/export can lag recorded device events, and reset-to-device-clock offset is not calibrated. Use device event order and surrounding UART activity for these observations; host connection order does not prove device acceptance order. Background TLS/MQTT connection errors appear in the UART, but no controlled concurrent TLS acceptance phase ran.

## Provenance and restoration

Source checkpoint: `06029036a2c7032a6c7511f6f7d1a272e7259a30`. The two-worker frozen build receipt records base `81df083` plus the diagnostic changes; the one-worker receipt records the later source checkpoint. Frozen compiled source copies differ only in the worker-list line; audio source, Cargo manifest, bootloader and partitions match. Their executable layouts differ.

- One-worker ELF SHA256: `81f0624018491a839b41695ba00cc06c7a8ed87039726b9955c5c21daf9383a7`.
- Two-worker ELF SHA256: `f5abf81f4441335bfe8d3dc62c00ca6a43a283baa4841b3b80b69cee3feaee02`.
- Restored default ELF SHA256: `01d2607f71524fc7b980fc37cc3516aa1f0e987f5ea0352bd2f3dca7bf113f04`.

`restore-default-1` retained exit code 0, flash exit 0, empty errors/signals, a bounded boot UART and final MP3/Medium/stopped state reads with autoplay and recognition false. This supersedes the acquisition receipts' temporary warning that a diagnostic remained installed. Flash identity hashes and state reads do not provide independent device image attestation; receipts explicitly retain image_attestation=false.

The companion JSON includes SHA256 for each raw result/events/UART/flash/wire receipt, frozen image/source/build files, sanitized endpoint events, resource records and listener receipts. Private addresses, ports and player identifiers are omitted. Raw receipt hashes identify private artifacts without publishing their contents. The disposable analysis script verifies frozen image file hashes and UART receipt hashes before producing the JSON. Process exits come from a hashed parent-observed receipt of reaped host sessions, not device telemetry. A review correction replaced hardcoded exit codes and restored format/preset with their saved receipt fields; observed values are unchanged. Source/linked reviews accepted bounded diagnostic acquisition, not live resource acceptance.

Recommendation: keep Task 3 and audio gates open. Before shipping, resolve sustained MP3 termination and obtain the missing pending-head/two-stream/TLS resource coverage, completed finite-cycle recovery evidence and remaining target fault-path acceptance. These acquisitions provide useful idle/tail measurements, with failed workload acceptance.
