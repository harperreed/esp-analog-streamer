# Startup capture loss during Wi-Fi setup

The October 12 quiet boot places the two post-flush overflow observations inside
Wi-Fi construction and start. It does not establish the scheduling or interrupt
cause. Kata 93hm remains open. No source audio, Sonos Play, debug listener or
test-requested configuration change ran during this acquisition. Normal firmware
initialization still ran, including its Wi-Fi configuration calls.

## Measurement

The opt-in `startup-capture-trace` feature retains two bounded, single-writer
atomic banks and exports them through the existing metrics task. Capture records
its initial flush/baseline, first driver read and first two overflow recoveries.
Main records initialization boundaries on the same `esp_timer` clock. No ISR,
priority, buffer, settling or recovery policy changes are included.

| Boundary | Device time, microseconds |
| --- | ---: |
| Initial flush entry / result | 146328 / 146460 |
| Baseline observation (overflow count 8) | 146473 |
| `EspWifi::new` entry / result | 146677 / 240403 |
| First overflow observation, 8 → 9 | 202460 |
| `wifi.start` entry / result | 250983 / 323315 |
| Second overflow observation, 9 → 14 | 322364 |

The first fault's previous driver read spans 151627–202410 us (50783 us).
The second spans 250922–322255 us (71333 us). Those reads were discarded by the
existing overflow check. Restart calls returned success; subsequent accepted
reads were observed at 207686 and 328829 us. Accepted reads still precede the
settling discard and raw publication: these events do not certify published audio.
The second read began 61 us before `wifi.start`; its full duration cannot be
attributed to that call.

The current counter-sample brackets are 202447–202472 and 322348–322384 us.
Fault observations upper-bound when the overflow occurred; they are not ISR
timestamps. Call duration includes any time the capture task could not execute.
No measurement here distinguishes preemption, interrupt masking, driver waiting
or memory contention. UART log-prefix time describes delayed export, not the
startup event; comparisons above use only the embedded device clock.

All 10 main and 14 capture records arrived with consecutive indices, zero bank
drops and completion set. The first periodic capture snapshot reported 14 total
queue overflows, 480 software-discarded frames, two overflow restarts and 26400
settling frames. Counters stayed unchanged through the final 30-second snapshot.
Eight overflows preceded the production baseline; six followed it. Software
discards and settling counts do not measure all DMA audio lost.

## Verification and cost limits

Three host tests exercise the actual bank/observer implementation, including real
producer/reader threads, immutable records, bounded drops and read-error history.
They do not execute the firmware restart-error or accepted-read integration paths;
those placements received independent source review. All 27 canonical stages
passed in `target/check-logs/run.TvABvkgv`. A separate feature build and its exact
generated-config/linked I2S IRAM check passed.

The linked banks and completion flag occupy 3220 bytes of internal DRAM. Normal
firmware has no trace symbols. The capture function's fixed stack frame grows
by 560 bytes within its 8192-byte allocation; metrics grows by 16 bytes. Linked
formatter comparison supported this bounded run, not a whole-program stack
proof. Capture's runtime stack watermark is not exposed by this firmware.

| Observation | Ordinary quiet baseline | Diagnostic quiet boot |
| --- | ---: | ---: |
| First snapshot free internal bytes | 48555 | 45339 |
| First snapshot largest internal block | 47104 | 43008 |
| First interval maximum read-service estimate, us | 63831 | 66414 |
| Minimum observed metrics free stack, bytes | 1128 | 1104 |
| Overflow restarts through final snapshot | 2 | 2 |

This single pair is an overhead observation, not a controlled timing benchmark.
It reproduces the same loss pattern; it does not prove the probe has no effect.
There was no panic, watchdog or stack-failure diagnostic in the retained run.

## Artifacts and remaining work

Raw UART, Cargo/config receipts, exact ELFs and independent reviews remain in
`target/kata-hardening/oct12-startup/`; raw device/network details stay outside Git.
The accompanying JSON retains hashes and machine-readable observations.

Next, use the identified construction/start intervals to distinguish capture
service delay from driver/interrupt behavior. Do not move the overflow baseline,
suppress the recorded startup losses, or enlarge buffers as a substitute for a
demonstrated fix. Exact-sample setup/reconnect/configuration acceptance remains
outstanding. Doctor Biz cannot sign into the workstation now; audio-dependent
tests stay blocked by the desktop audio-device permissions.

Ordinary unified firmware was restored after the run, image
`94fc9e630a06362586d01ff09e40dd11e6eeeb79289a4ec97f5aecefcf56647f`.
Fresh device identity, board state and coordinator SOAP checks confirm stopped
MP3/Medium, autoplay/recognition off and both listener slots idle. Discovery and
read-only coordinator queries performed this verification; no Play or source
audio ran. The diagnostic remains opt-in and is not installed on the board.
