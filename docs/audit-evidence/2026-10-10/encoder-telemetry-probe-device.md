# Encoder telemetry: partial probe failure

This is a canceled diagnostic with real failures, not a completed OFF/ON/OFF
acquisition. Parent stopped through the existing SIGTERM cleanup handler after
repeated raw overruns and an observed 428-byte metrics-stack minimum. The original
result remains completed=false, acquisition_complete=false and SystemExit(15).
No 360-second completion or throughput acceptance is claimed.

The exact probe ELF built from 05cf802 has SHA256
d3d435b39c98991c9c4715f5abf35b850abeae0ab3fcc18332274e6093416653. The harness includes
c2ba29a's read-only routing validation. The source is the same 420-second noise
fixture used for the default trials. Original artifact hashes are in the paired
JSON; running-device image attestation is unavailable.

Strict parser replay equals the saved rejection and identifies actual raw-loss
and reset/invalidation evidence. Arrival offsets cover the original UART bytes.
Filtering by recorded reset release excludes 12 complete lines from the preceding
boot. Telemetry epoch alone cannot distinguish those boots.

Five actual overrun boundaries occur at 20.185962, 35.935124, 51.875292, 67.571368
and 83.865172 seconds of producer time. Cumulative resets advance from one to five.
Each reset invalidates OFF1 interval 2 and starts a fresh sequence. Sequences 1–5
contain only their first, discarded OFF1 window; sequence 6 has storage only.
There are no retained windows or ON/OFF2 spans. Local window valid=true cannot
clear later sequence invalidation. All six MP3 storage receipts pass validation.

Output ratios for the five complete, baseline-backed windows are 0.869612,
0.868581, 0.870518, 0.869709 and 0.877746 times real time. Frame deltas subtract
the actual replacement boundary's cumulative count; subtracting the previous
window would mix pre-reset work into a fresh interval. These failures do not
establish a cause or a controlled default-versus-probe performance difference.

All 12 observed encoder receipts report zero loss. No listener receipts or media
listeners exist in this trial. No final drain acknowledgment exists. Eight
complete capture snapshots advance from 476,160 to 4,000,560 frames, with all nine
fault counters unchanged after startup. Windows 2–5 have full capture bracketing;
window 1 lacks a preceding snapshot. The final reset follows the last capture
snapshot, so its later capture coverage is unknown. Capture stability does not
clear encoder raw loss.

Actual Storage and Window export paths ran. Metrics minimum free stack is 428
bytes of its 4,096-byte allocation, observed already after MP3 Storage export;
encoder minimum is 4,936 bytes. Capture watermark is absent and listener rendering
was not exercised. This is a narrow observed margin, not proof of overflow or a
complete worst-case bound. Resolve it before accepting or adding exporter work.

Complete-report internal free/largest/minimum-sum minima are
66,783 / 65,536 / 65,803 bytes; PSRAM values are
2,770,828 / 2,359,296 / 363,584. Minimum sums combine per-region lows and include
startup. Rare restart-failure rendering was not exercised.

Preflight and final routing readbacks pass, including mono Master on at 0 dB.
Final status remains stopped MP3/Medium, autoplay and recognition off. The source
was alive through final receipts, then exited successfully during cleanup with
an empty aplay log. Parent reaped the session and checked UART/source release,
then flashed the exact default unified image and verified those same settings
and routing again. This restores the normal image while retaining MP3 selection.
It does not clear the failed diagnostic or default delivery failures.

Severity: probe throughput/raw loss and stack margin block acceptance. Retain
this partial failure alongside the default trials; do not mark Kata 21tp or wbz6
complete from passing builds or zero observed queue loss in this workload.
