# Bounded heap and UART crash diagnostics

The dedicated ESP32 fixture passed its heap assertions and produced a real UART
crash dump. The official decoder resolved the named intentional abort with the
matching ELF; replay passed, and a different real ELF failed application SHA-256
validation. All 27 canonical stages pass after the build-config selection fix.
Numeric measurements and artifact hashes are in
[device-diagnostics.json](device-diagnostics.json).

## Scope and reproducibility

Kata cxbw uses the pinned ESP-IDF 5.5.5/classic ESP32 APIs, generated heap-trace
bindings, esp-coredump 1.17.2 and GDB 17.1_20260402. The isolated example starts
no application services or Wi-Fi and does not read saved NVS credentials. It
allocates a fixed 64-record internal trace buffer, validates one live and one
freed allocation, and runs 8,000 allocation/free pairs in each OFF/ON/OFF phase.
Both final images use the same optimized release profile with full debug data.

The diagnostic overlay enables UART ELF dumps with CRC32, a 1,792-byte dump
stack and at most 16 tasks. Whole-DRAM capture and the heap-trace hash map stay
off. No SystemView transport or flash dump partition was added. The partition
CSV and both generated partition images retain their recorded normal hashes.
The normal configuration again selects no core dump and no heap tracing.

The baseline finished without a crash. The diagnostic reported both known
allocation checks, 8,000 traced allocations and frees, high-water two records,
zero overflow, and successful completion before the deliberate abort. Both
images verified a 128-byte external-memory sentinel with checksum 21,120.
This bounded checksum and separate heap counters supplement the dump; they do
not reconstruct PSRAM ring contents, most of which a core dump omits.

## Measured cost

| Allocated sections | Baseline | Diagnostic | Added |
| --- | ---: | ---: | ---: |
| DRAM data and BSS | 14,466 B | 21,026 B | 6,560 B |
| IRAM text | 51,847 B | 52,915 B | 1,068 B |
| Flash text and read-only data | 327,312 B | 335,728 B | 8,416 B |
| All allocated sections | 394,968 B | 411,012 B | 16,044 B |

Trace records account for 2,560 bytes and the reserved dump stack for 1,792 bytes
of added DRAM. Other support accounts for the remaining 2,208 bytes. Boot free
internal heap fell by exactly 6,560 bytes. The largest internal block fell from
147,456 to 139,264 bytes. After trace setup, free internal heap stayed at 268,747
bytes through all phases; PSRAM free heap stayed at 4,192,024 bytes in both images.
Minimum observed main-task free stack was 14,868 bytes in the baseline and 14,804
in the diagnostic. The dump-stack figure is its reservation, not a measured
panic-path high-water mark; the actual controlled dump completed.

| Phase | Baseline task runtime | Diagnostic task runtime | Diagnostic maximum batch |
| --- | ---: | ---: | ---: |
| OFF1 | 157,024 µs | 214,306 µs | 245 µs |
| ON | 132,895 µs | 190,409 µs | 285 µs |
| OFF2 | 133,092 µs | 179,535 µs | 241 µs |

The baseline never enables tracing, including its phase named ON. Each phase
took about five seconds because the fixture yields one 100 Hz RTOS tick after
each batch. Maximum observed yield duration was 9,830 µs in the baseline and
9,759 µs in the diagnostic. This measures yield wall time, not runnable delay.
Task runtime can include charged interrupts and excludes an unfinished slice.

Tracing ON used 6.06% more task runtime than the diagnostic OFF2 phase, but OFF1
was higher than either. One sequence does not establish a stable overhead rate.
Differences between images include changed configuration and executable layout.
These isolated allocation measurements do not certify overhead during I2S,
TLS or streaming; keep that limit when using the tools to diagnose those paths.

## Retained failures and regression evidence

The first crash capture failed its 60-second decoder bound. The release ELF had
no DWARF, and pinned GDB could not resolve `TCB_t`. Bounded stack sampling showed
the decoder waiting for failed task-type queries, each with a ten-second timeout.
The corrected command retains full debug information and preflights both the
DWARF sections and actual GDB type lookup before flashing. A fresh matching
capture and saved replay passed without increasing any deadline. The original
failed receipt remains private and unchanged.

The first final canonical run also failed: retaining both normal and diagnostic
profiles exposed the I2S checker's assumption that exactly one generated config
exists. Its failure remains in `target/check-logs/run.9rGfto1g`. The correction
selects the SDK output from the successful Cargo build receipt and verifies its
normal-firmware executable path, preserving both artifacts. Focused tests cover
ambiguous, failed, incomplete and wrong-executable receipts; actual ELF placement
checks still reject a disabled IRAM option or a misplaced callback.

The host tests exercise framing, bounds, private files, pseudo-terminal capture,
real child-process cleanup, interruption and an installed official-decoder
negative control. Those tests do not replace the real-board decode and wrong-ELF
checks above. The procedure is in [device-diagnostics.md](../../device-diagnostics.md).
The final canonical run and its stage-manifest hash are recorded in the JSON.
Independent Sol reviews found no blocking source or acceptance-evidence defects.

## Privacy and restoration

Raw UART, dumps, ELF images, SDK configs, decoder reports and temporary files stay
under ignored `backups/diagnostics`. Reviewed run directories are 0700 and files
0600. Capture is bounded to 120 seconds/2 MiB, encoded body to 1 MiB, and decoding
to 60 seconds/2 MiB output. No decoder temporary files remain in completed runs.
The public JSON contains measurements, hashes and fixed verdicts, not memory,
registers, stack contents or credentials.

Normal firmware `e7404b41` and its saved bootloader were restored. HTTP readback
confirms active and saved MP3, Medium, stopped, autoplay false and recognition
false. No source playback or Sonos Play command was used. Restoration identity
comes from the flash tool and HTTP state, not a full flash readback.
No UART or source-playback process remains.
