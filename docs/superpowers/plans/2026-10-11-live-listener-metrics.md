# Live listener metrics acceptance

Continue amqe from c1a012b under Doctor Biz's existing finish-all authorization.
The original audit requests individual listener lag/write duration. Completed
receipts do not meet that requirement. Preserve existing two-listener admission,
capture behavior, source ownership and transport deadlines.

## Contract and design

Add two anonymous reusable observation slots, separate from admission. A listener
may proceed even if observation registration/publication fails. No peers, URLs,
task handles or session identifiers enter metrics. Register on the compressed
listener path; WAV coverage must be explicit rather than inferred from codec data.

Use the simplest bounded synchronization that preserves coherent receipts:
short try-lock copies plus atomic ownership/validity are acceptable. No capture
path access. Registration, publication, snapshot and owner cleanup must not wait
for a metrics reader. Owner cleanup invalidates its observations even if a copy
lock is held. Failed completion publication cannot leave a previous write looking
as if it is still running. Ownership/version exhaustion fails observations closed
without refusing audio. Old owners or departure records cannot clear reused slots.

Device JSON adds `listeners`, exactly two objects with fields:

- `generation`: u32 anonymous occupant generation, not a label or boot identity.
- `state`: `unavailable`, `idle`, `waiting`, `writing` or `ready`.
- `sample_us`: u64 source observation timestamp.
- `media_age_us`: optional u64; sampled pending-frame age while writing, or age
  recorded at the last completed payload write. Never extrapolate a completed age.
- `write_us`: optional u64; elapsed payload-write wall time when writing, or last
  completed payload-write duration when ready. This is not TCP blocked-time alone.

Idle/unavailable/waiting have null age/duration. Active generations differ on reuse.
Ready retains the completion timestamp. Writing reflects a fresh coherent registry
sample, computed after copying with no timer or formatting under a guard. Sample the two live listener observations in the metrics HTTP handler after the
cache and metadata guards end, before taking the response timestamp. This removes
the dependency on the delayed logging owner. No added task, heap scan, stack scan
or capture source query. Observations of completed writes preserve
their recorded times even when the owner reads them again.

Keep Cache and Snapshot <=768 bytes, device JSON <=4096 bytes. Serialize a reference
to the already-copied Snapshot instead of a second owned snapshot if needed; this
preserves the wire contract and guard lifetime. Check actual worst-case size before
adding fields. Host exposition stays bounded. The fixed series measured 9490 bytes
with HELP and TYPE metadata; allow 12288 host bytes rather than omitting descriptions
or disabling Prometheus lint. Device JSON and identity limits stay unchanged.

HTTP headroom uses an optional top-level `http_stack_free_bytes` u32 sampled by
the live HTTP task before preparing its response. This is one bounded self-scan
per request outside I2S. It is the sole exception to the no-new-scans constraint
above and closes the named HTTP stack gate without a different diagnostic image.
It reports a historical watermark before the current response write; the next
request sees that write's peak. Host tests retain null versus real zero semantics.

## Work units

1. Firmware owner: production registry, deterministic lifecycle/contention tests,
   real stalled-socket test, wrapper integration, bounded cache/JSON tests.
2. Host owner: validate and copy exactly two observations, fixed slot labels,
   independent sample freshness, dashboard query/unit/missing-data tests.
3. Parent: independent review, canonical checks, target size/stack review, exact
   image hardware check with two listeners, slot reuse and bounded stalled reader;
   preserve manual Stop and finish MP3/Medium stopped. Use actual rendered Grafana
   verification if the browser sandbox can run without a policy change.

All implementation changes require observed RED/GREEN tests. Agents own disjoint
files; parent alone builds/flashes/owns UART/audio, runs scripts/check and commits.
Retain failed trials; do not substitute metrics acceptance for continuity/soaks.
Remaining amqe HTTP stack/saturation and browser gates stay explicit.

## Verification checkpoint

Implementation and independent firmware/host review are complete. Review found
that failed writes could publish Ready; a regression now requires Unavailable
with null timings before owner Drop. Both production wrappers pass actual success.
Canonical `scripts/check` run `target/check-logs/run.3C2qY3sb` passed all 27 stages.
The earlier `run.QE1J4a5e` failed module-order formatting and remains recorded.
The canonical formatter also touched an unrelated example; that formatting-only
change was restored after the check, leaving the checked application unchanged.

Checked and flashed ELF SHA-256:
`29579d760971213e09e6e9a1a011bb01bc117cab1e592e7c8f85a539c19db920`.
Host maximal JSON is 4074/4096 bytes, Snapshot 752 and Cache 760/768 bytes.
Target size assertions and IRAM checks passed. Two stale dashboard descriptions
were corrected to include WAV; six dashboard tests passed afterward.

The first live trial failed because its fault client used a tiny receive window
during the clean comparison. It reached media-age termination while the normal
client continued. Preserve `target/kata-hardening/live-listener-metrics/live-run`.
The retry separates normal clients from the briefly paused fault client. This
trial is metrics acceptance, not sample-continuity or sustained-overload recovery.

Compaction count for this phase: 1. Original amqe acceptance remains open until
the final live trial, rendered panels and evidence review finish.

### Evidence-driven sampling correction

The second real trial completed 120 seconds with two normal MP3 clients, but
failed its planned-close observation: the logging owner's cached listener sample
did not become Idle within five seconds. Browser queries saw source ages grow
from 19 to 49 seconds while HTTP remained fresh. A diagnostic export took 21.5
seconds; capture acquisition receipts were 54.1 seconds apart. Task samples
reported Ready and no Blocked states, so this does not prove a mutex or UART lock.
The ON phase has no interior capture receipts and cannot establish an overhead
comparison. Preserve the failed `live-run-2` and its browser evidence.

Revise the sampling location within the original metrics scope: HTTP copies the
two nonblocking listener registry observations independently from the cached
logging families. The response clock follows the copies. No new task, capture
work, public schema, admission behavior or transport deadline. Regression must
advance a write and observe owner release while cached capture stays unchanged.
The next exact-image device run must verify this fix.

## Completed acceptance

The HTTP-owned sampling fix passed 97 focused tests, independent review and all
27 canonical stages in run.X4QTsf98. Final Snapshot is 656 bytes and Cache 664;
JSON remains 4074/4096. Exact final ELF is
`2b28be214c224182bb736a122d33757cfd0e4cb26d43c78d6ef4e5dc4b26fbbd`.

Focused live-run-4 passed two live MP3 observations, stalled-write timing, idle
cleanup, WAV generation reuse, 30 HTTP requests, exact departure propagation,
rendered Grafana graphs and resource cleanup. It uses ten-second phases and makes
no overhead, stability or continuity claim. The prior final-image stream failure
before collector startup remains open under transport work. Do not make amqe's
original metrics acceptance depend on that separate stability investigation.

Original amqe acceptance is complete. See
`docs/audit-evidence/2026-10-11/live-listener-metrics.md` and `.json` for actual
results, failed trials and remaining release risks. Next work is the remaining
recovery/transport Katas; do not reopen passed metrics acceptance without a new
defect. Commit this phase and start the next with a fresh context.
