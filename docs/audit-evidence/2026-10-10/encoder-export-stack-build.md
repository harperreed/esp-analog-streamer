# Encoder export stack build

Source base a18f08b plus the single stream.rs change committed with this receipt.
The record dequeue/render helper now returns before batch formatting; rendering
borrows the returned Option record instead of copying it. Boundary returns before
Storage formatting. Failure formatting also has a separate leaf. All 27 logging
strings, arguments, conditions and output order remain unchanged. No queue shape,
stack size, priority, timing policy or parser contract changed.

## Actual linked application frames

| Path | Default before → after | Probe before → after |
| --- | --- | --- |
| Window | 1152 → 928 | 1344 → 1024 |
| Boundary | 1008 → 784 | 1200 → 880 |
| Failure | 848 → 720 | 1040 → 816 |
| Batch receipt | 752 → 528 | 944 → 528 |
| Probe window | absent | 1232 → 912 |
| Storage | absent | 1360 → 880 |

These include the metrics caller but exclude logger/libc/formatter callbacks and
register-window spills. They establish smaller application reservations, not
runtime headroom. Actual disassembly confirms the removed 200/296-byte record
copy and sequential returns. Queue/static DRAM sizes remain unchanged; receipt
query symbols remain probe-only. Independent linked inspection verified original
image hashes and code sections against separate inspection copies.

## Verification

All 27 canonical scripts/check stages pass, log
target/kata-hardening/check-encoder-export-stack.log. Host suites pass: default
443 and probe 465, each with two existing ignored tests. Both target builds pass
without warnings. Exact-image IRAM checks inspect 10830/10845 symbols and pass.
Independent source and linked reviews are clear; parent fresh-eyes found no
outstanding issue. Host tests do not compile platform stream.rs; the actual
target builds and linked checks cover that source.

Saved deployable images under target/kata-hardening/encoder-export-stack:

- default.elf: d88a65965ff456c7500df1ae05d5ac548ef4290b35524215bf3e27175378b991
- probe.elf: ee104f52b50f2d3726834dced5d77580ff15a052deefde3134b6c59166067c80

Canonical default output matches the saved default hash. Source stream.rs hash
is 0e094fcd6bacd3642d000a9d1a00a31f800168158047ca644090740dc6f3e03a. Intermediate
experimental images and the original failed images remain separate and intact.

The previous probe measured 428 bytes minimum free metrics stack. Parent must
repeat actual Storage/Window paths before accepting runtime margin. This code
change does not explain or clear encoder raw loss, exporter delay, queue drops
or listener delivery failures. Those release gates remain open.
