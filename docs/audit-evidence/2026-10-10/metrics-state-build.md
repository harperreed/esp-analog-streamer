# Metrics state observation build

The diagnostic samples metrics at most once per elapsed second after supervisor
health handling. Independent cumulative Ready/Blocked/Other counts identify the
sampled phase; they do not measure time in state or identify a lock owner.
Exports run from metrics at most once per ten seconds after existing exporters.
Each dump requires its totals, eight phase rows and matching completion. A dump
used to cover an interval must start acquisition after that interval ends.

Permanent task handles rely on nonreturning tasks and the pinned ESP-IDF abort
path. The state query disables stack scanning but takes the actual kernel lock;
its latency has no hard bound. Supervisor watermark scanning runs only from
low-priority metrics, using a nonnull handle and the pinned byte-unit API.

Both target builds and IRAM checks pass. Default ELF SHA-256:
`ced962b2136d7c3fbe8dd97dd4de2d47f1c39e72bef6dee76d92fa5c59218d1c`.
Probe ELF SHA-256:
`409849952981fe7dec9c62b6d04bd558bdbc7bea915b4207666a55e9a6513c31`.
Saved originals and inspection copies have identical flash/IRAM text; originals'
hashes and modification times remain unchanged. Images and linked reports live
under target/kata-hardening/metrics-state/v2.

Compared with eb38c65, supervisor entry frame grows from 96 to 272 bytes and
metrics from 224 to 256. Diagnostic totals/phase/completion leaves use
160/128/112 bytes. Existing capture/listener/encoder leaf frames stay unchanged.
Named observation statics total 132 bytes. Atomic publication lowers to ordinary
loads/stores and barriers, with no new CAS loops or atomic helpers. The existing
health-request increment still has its original CAS loop. Partial application
frame sums exclude logger, library, interrupt and thread-wrapper costs.

All 27 canonical checks pass on this revision. Five new tests exercise the actual
numeric accumulator and monotonic gate; full core tests pass 245 with one existing
ignore. Source and linked reviews are clear, followed by parent fresh-eyes.
The initial target build caught a never-returning function passed directly to a
unit-returning thread callback; a closure fixes the type mismatch while retaining
the lifetime invariant. The failed build log remains alongside the corrected run.

Task priorities, stack allocations, queues, capture behavior and existing export
order remain unchanged. Runtime overhead, stack headroom and receipt completeness
are pending bounded hardware measurement. Earlier raw loss, diagnostic drops and
delivery failures remain failed gates; Kata 21tp/wbz6 stay open.
