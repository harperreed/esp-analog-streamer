# Encoder exporter stack follow-up

Continue authorized Kata 21tp after a18f08b. The probe Storage/Window workload
measured only 428 bytes free on the existing 4 KiB metrics task. Default listener
trials measured 1164/1148 bytes. Delivery, raw loss and queue drops remain separate
failed gates. This unit addresses simultaneous formatting stack use only.

## Bounded implementation

Inspect actual export_encoder_events and its render call chain. Keep queue
capacity, record shape, timing, task stacks/priorities and all output text/field
semantics unchanged. Separate the batch receipt's formatting workspace from the
record dequeue/render lifetime. Flatten Boundary-to-Storage nesting where it
retains formatting frames unnecessarily, preserving exact boundary, warning,
invalidation, storage and active-format output order. Small reference-taking
out-of-line helpers are allowed; no generic framework, heap buffer, new task,
static mutable scratch, queue redesign or parser contract change.

Measure before/after linked default and probe frames and actual nested paths.
Do not infer target changes from annotations or host sizeof. Preserve original
ELFs; inspect copies and explicit objcopy outputs only. Reject variants that add
new ownership/locking/allocation or cannot demonstrate reduced relevant nesting.
A target stack-layout experiment does not need a host test that merely asserts
inline attributes. Existing production/parser tests remain mandatory; where a
functional helper changes, test its real behavior rather than a mirror.

## Verification and acceptance

Independent source/output-order review, default/probe host checks, canonical
scripts/check, both release images and IRAM checks precede parent device use.
Parent repeats the same bounded probe paths and measures actual stack/heap and
raw/export-loss behavior. No stack-margin acceptance from static arithmetic.
Preserve failed/canceled original trials. Restore default unified MP3/Medium,
autoplay/recognition off and stopped after diagnostics; no FLAC policy rollback.

The separate exporter-delay investigation continues read-only. Reducing stack
use does not explain shared stdout waiting, scheduling delay or transport loss.
