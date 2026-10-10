# Separate capture formatting from listener export

The listener trials measured only 420 bytes of minimum free stack on the 4096-byte
metrics task. Linked inspection found capture formatting inlined into its loop:
an 800-byte persistent frame remained beneath the 992-byte listener exporter.
This measured margin warrants investigation; it does not establish overflow.

The change keeps existing `log_capture` out of line. No statements, sampling
order, log fields, task sizes or priorities change. Before the edit, a check of
the actual linked image failed to find a separate capture-report boundary.
Both new builds have that boundary and call capture reporting before export.

| Linked frame reservation | Before | After |
| --- | ---: | ---: |
| Metrics loop | 800 | 208 |
| Capture reporting | Inlined | 528 |
| Listener exporter | 992 | 992 |
| Caller plus exporter | 1792 | 1200 |
| Caller plus capture reporting | 800 | 736 |

These values match in the default and encoder-probe builds. They describe actual
Xtensa entry-frame reservations, not total stack demand or a predicted runtime
watermark. Inspection copies preserve executable section bytes exactly; only
their Xtensa disassembly metadata was removed. Those copies must never be flashed.

| Build artifact | SHA256 |
| --- | --- |
| Default | `7d440f936a3e71bf7f4b78fd1680323a22c3120143e036e2ec2e632381d67453` |
| Encoder probe | `a261aefdda84f6846f95fa05fd8ea2e5525df1beae3f10622c3a6a28448e1c8e` |

Independent source and linked-call review found no actionable defect. Canonical
`scripts/check` passed all 27 stages; the separate probe IRAM check passed too.
The canonical default ELF matches the saved artifact hash. Local logs and frame
inspection are retained under `target/kata-hardening/metrics-stack-*` and
`target/kata-hardening/check-metrics-stack.log`.

At this build checkpoint, the matched hardware repeat is pending. Encoder export
remains gated on measured headroom and unresolved listener telemetry loss. The
change does not establish or fix the cause of MP3 delivery failure.
