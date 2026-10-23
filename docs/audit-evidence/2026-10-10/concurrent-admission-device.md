# Concurrent admission device evidence — 2026-10-10

Both bounded production trials failed. Partial results support concurrent request-head handling, but do not clear admission acceptance, audio delivery, or resource gates. Task 3/Kata remains open.

## Frozen source and image

Core commit: `81df0833477debb2ede0d76b13ce74be9017f75a`. The production receipt names canonical run `target/check-logs/run.QSVJJTto` (27 stages passed); independent source review passed separately. Default features; two admission workers; admission and listener priority 7. Flashed ELF SHA256: `01d2607f71524fc7b980fc37cc3516aa1f0e987f5ea0352bd2f3dca7bf113f04`. Host flash and saved-image hashes are provenance, not device attestation. The separate resource diagnostic was not flashed and is not the source of either trial.

## Trial 1 — preserved failure

The parent flashed the frozen default image successfully and retained one 150.186-second boot UART capture. Initial, partial, silent, and final state cases passed. The trickle case failed with an unhandled `ConnectionResetError`; its companion stream had already returned payload. The request-head timeout reset exposed a driver terminal-handling defect. This correction does not change the original failed verdict. Remaining workload cases did not run.

The exact original driver is retained as `admission_check.trial-1.py`. The fixed driver distinguishes FIN and reset, and only accepts slow-peer terminal closure after validating the companion stream. Its implementation receipt records 16 passing host tests, including real loopback FIN/reset seams; the independent reset review accepted the scoped correction without rerunning tests.

The full UART tail contains **12** encoder Overrun boundaries, at device event times (seconds): 16.534893, 28.273058, 40.155418, 52.255811, 64.184582, 76.186132, 88.139734, 100.238066, 112.340427, 124.175332, 136.148586, 148.250003. Startup and Switched boundaries are separate. A shorter snapshot ending before the last boundary would report 11. The first audio metric already has queue_overflows=15, software_dropped_frames=480, restarts=2, overflow_restarts=2 and discontinuities=2; those values persist through the final audio metric. read_errors and supervisor_restarts remain 0 in those metrics. These are boot counters, not workload-attributed deltas. The first encoder overrun precedes the admission workload; later ones continue through the tail. Capture and audio gates failed independently of the driver defect.

## Trial 2 — header-only replacement failure

The parent ran the corrected driver against the same installed candidate, with no flash, reset, or UART acquisition. Parent session 67716 exited 1. Initial/state checks, partial, silent, trickle, fragmented, halfclose, invalid-UTF8, oversized, full-capacity, pending-release, pending-natural, and recovery-1 passed. recovery-2 failed; recovery-3 did not run. Final state verification passed.

Peer 30 retained exactly 89 bytes: the expected HTTP 200 MP3 response head, followed by FIN and **no audio payload**. Its first byte arrived 367.892 ms after its first request send; complete head at 367.897 ms. Peer 29 had delivered payload while the replacement failed. The failure was `ValueError: exact MP3 head plus nonempty payload required`. Without trial-2 UART, these receipts cannot assign the cause to admission, an encoder reset, or another device failure.

## Timing, cleanup, and limits

The JSON companion retains per-case results and every peer’s observed timings relative to connection and first send. Repeated trickle/fragment sends use the first recorded send as the baseline; later sends do not overwrite it. These host observations establish neither device acceptance order nor exact server timeout duration.

Both trials report no cleanup errors and all driver-owned sockets closed. State checks preserve stopped playback, MP3, medium buffer, autoplay off, and Shazam disabled; private player and network identifiers are omitted. No audio source player was started. The candidate was kept because boot/settings HTTP remained responsive; this does not mean audio was healthy.

No decoded audio or sustained delivery acceptance. No trial-2 UART: header-only FIN cause is unassigned. Host connection/send order does not establish device acceptance order. Host owned-socket cleanup is not a whole-device socket census or resource-reclamation proof. No complete heap/task/watermark accounting, TLS overlap, fault injection, long soak or device image attestation. The separate resource diagnostic was not flashed and is not the source of either trial. Task 3/Kata remains open; no automatic retry follows.

## Evidence SHA256

Paths below are relative to the private acquisition root `target/kata-hardening/concurrent-admission-device/`. Raw state responses, IP addresses, player IDs, and endpoint tuples stay private. Hashes allow matching the artifacts used; they do not make private receipts publicly available.

- `production/receipt.json`: `6145c80f374e3e9137248952d360f32bbed95e45152b2c1b1a47f9011c6a8846`
- `production/esp-shazam`: `01d2607f71524fc7b980fc37cc3516aa1f0e987f5ea0352bd2f3dca7bf113f04`
- `admission_check.trial-1.py`: `8e5778c37372526f3fe440a37002efbffc80f1134e4d4af34e63acf30d51da35`
- `admission_check.py`: `cd4c257930b92d1f047fdd209ab57256a3639e9d08a554ded358dbb356c243be`
- `test_admission_check.py`: `79997a4745c297ce062f25e6716fa6ab794d05236862db49353501c2b6a2d0d4`
- `implementation-report.md`: `c2e85a0cd0bfe66142b7015e3bdd44d0400f28d29a1cac7629f22b3ad5daed28`
- `reset-review.md`: `4d8efb48e2ff01c3df75de1b5474dc8bb4e4e9943d3daf7c8eae7fdde8c792ec`
- `green-reset.log`: `cb911b310ccb3daadcab815b43af6edf4c098f9348c2c9dd39e6de244666d0ae`
- `production-trial-1/parent-result.json`: `ff4b09126938c649efd22de5cb160562b51384f341ce6bc32c491fff184c4c12`
- `production-trial-1/uart.log`: `c8ead24d05c9a560c8113224696dcff9edc1c4f7f8b1cd84569ab76a05b25fa6`
- `production-trial-1/admission/events.json`: `e7c37a5eca4171ed16402696b0aae5ded4817a35ef345326a12a22b6ee05b0a7`
- `production-trial-1/admission/result.json`: `b3619bff89e03660fec41573f4a645fe9f6f66dda688750c9f78edfe66a44abe`
- `production-trial-2/result.json`: `3dbb85a55a5de31b843cd5d9830a956cdc8f13d3c7bc66348bd6563301a3192a`
- `production-trial-2/events.json`: `5ec599eae82c8d033051ae50674612129230939475a8a86f388f6dd3f14ad6c7`
- `production-trial-2/recovery-2.json`: `3dd7eaa701401b65b8604f40895d4fc1e9a9582c38a4d5bc26ea6769e680adb2`
- `production-trial-2/peer-030.wire`: `90a088a1753887216a8a13ee4226536639d90145535167936cb513027796706d`
