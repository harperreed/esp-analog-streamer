# Actual capture-health mutation pilot

The real pilot completed successfully with exit 0 on October 10,
2026. All five generated candidates were attempted: four caught, zero missed,
one unviable, zero timed out, zero unattempted. Unviable receives no assertion
strength credit. There are no meaningful survivors and no new regression test
is warranted by this run.

Input revision: `823c0cc56a2736f1bf70910acdafb11df93f8539`.
Immutable archive SHA256:
`3d7b7cb031ab6a6a4f1e871bbe0e90e84de988603a55d03a44d25b3ac71477fd`.
Cargo.lock SHA256:
`63b3ed56764b6dfef2f93d200ad82cdc5e24d35289d66fa45266996d71de0040`.

Invoked the actual `scripts/mutation-host`, redirecting stdout/stderr to
`target/kata-hardening/mutation-pilot.log`. Artifacts are retained under
`target/mutation-host/run.VRAKTpBA/`. `status.txt` is `complete`;
`mutants.exit` is `0`. Tool outcome timestamps span
`2026-10-10T18:31:57.929828375Z` to `2026-10-10T18:32:14.170127083Z`.
Both ordinary and tool baselines passed eight tests, zero failed/ignored, 233
filtered out. Real cargo-mutants 27.1.0, Rust/Cargo 1.93.0, explicit
x86_64-unknown-linux-gnu host, default features and locked dependencies ran.
No hardware or firmware operations occurred. No download/update lines appeared
in inspected Cargo logs.

## Inspected outcomes

Each diff and matching log below is relative to
`target/mutation-host/run.VRAKTpBA/results/mutants.out/`.

| Mutation | Class | Actual evidence |
| --- | --- | --- |
| observe body → Default::default() | Unviable | `log/core__src__capture_health.rs_line_39_col_9.log:250`, E0277: Action does not implement Default; build exit 101; no test phase |
| frames != → == | Caught | `log/core__src__capture_health.rs_line_42_col_19.log:524`, startup grace expected Restart at 1100 but got None; six tests fail |
| timeout < → == | Caught | `log/core__src__capture_health.rs_line_53_col_49.log:524`, startup grace expected None at 1099 but got Restart; five tests fail |
| timeout < → > | Caught | `log/core__src__capture_health.rs_line_53_col_49_001.log:524`, startup grace expected None at 1099 but got Restart; five tests fail |
| timeout < → <= | Caught | `log/core__src__capture_health.rs_line_53_col_49_002.log:524`, startup grace expected Restart at 1100 but got None; recovery boundary also expected Reboot at 1250 but got None (line 510); five tests fail |

Matching mutation diffs share each log's basename in `diff/` with `.diff`.
All four caught mutants compiled successfully and failed test phase exit 101
at intended Action equality assertions, rather than an incidental panic. The
unviable log's build-failure warning is expected; unmodified baselines contain
no warning lines. Full JSON is `results/mutants.out/outcomes.json`; commands,
versions, inventory, stage statuses, hashes and baseline remain in run root.

## Limits and recommendation

This is five mutations in CaptureHealth::observe, only four of them viable.
It establishes detection for those four comparison changes. It does not prove
all recovery invariants, constructors (no constructor candidate generated),
ring generations, listeners, codecs, Python assertions, firmware correctness,
network timing, or hardware recovery. No survivor explanations or survivor
regressions were omitted: missed.txt is empty and missed count is zero.

Recommend accepting the bounded initial pilot and preserving this scope limit.
Before claiming wider capture protection, expand ordinary scenarios and bounded
mutation targets for those wider invariants.

Execution bounds: one mutation worker; 60-second test and 180-second build
limits; 300-second ordinary baseline, 60-second inventory, 1500-second mutation
stage and 1800-second overall process limits. No timeout occurred. The tracked
clean-tree archive excludes ignored/untracked input, including user-owned local
configuration. The runner preserved full commands. This receipt uses repository-relative
paths.
