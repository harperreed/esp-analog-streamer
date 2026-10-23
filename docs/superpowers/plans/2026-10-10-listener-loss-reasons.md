# Attribute listener telemetry loss

Trial 3 lost one listener record. Its timeline permits either a full eight-slot
queue or transient lock contention; the aggregate counter cannot distinguish
them. Preserve that failed result and the current queue/publication policy.

Add three wrapping AtomicU32 counters to the existing listener Queue: full,
contended and poisoned. Each rejected publication increments exactly one reason
plus the existing total and sticky flag. Successful publication changes none.
Distinguish actual TryLockError variants; do not wait, allocate, retry, enlarge
the ring or add per-frame events. Existing checked close and Slot/Lease release
remain independent of publication success.

Append numeric reason counters to the existing exporter receipt. Keep its nine
existing fields and their meanings intact. The existing acceptance parser and
loss rejection must still work on archived receipts. Reason attribution is an
additional diagnostic: a separate strict helper can require all three unique
bounded reason fields when that information is requested. Missing reason fields
mean attribution is unavailable, never that loss did not happen. Do not infer
atomic equality between total and reason sums from separately sampled atomics.

Use actual production queue red/green tests for full, held and poisoned storage,
successful publication, wrap and sticky loss. Retain real stalled-exporter socket
cleanup tests. Test strict reason parsing on complete real-file records, including
missing, duplicate and out-of-range fields; aggregate loss still rejects the run.
Do not add a payload histogram or change logging frequency, task sizes, priority,
codec, buffering, recovery or capture behavior.

Parent owns canonical verification, independent review, target default/probe
builds, queue/static/stack measurements and bounded hardware work. Preserve all
previous image and trial identities. This diagnostic cannot turn lost evidence
into a pass or establish a transport cause. Finish the stack-repeat evidence
checkpoint before flashing a different image.
