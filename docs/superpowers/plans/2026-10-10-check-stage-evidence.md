# 97bk actual canonical-stage evidence

Doctor Biz: feasible without a broad refactor. Replace the rejected review-record validator with machine-readable evidence emitted by the actual `scripts/check` execution. This directly improves README's canonical-check gate, which currently asks for every applicable stage's result, logs and source revision. It cannot establish hosted/HIL or streaming release acceptance.

## Existing behavior and smallest change

`scripts/check` already owns a single `stage` function and all 27 Linux/firmware stages. It accumulates failures and continues; final exit is 0 or 1. It prints `ok`/`FAIL`, shows the last 40 failure-log lines, and writes fixed `target/check-logs/<stage>.log` paths. Those files overwrite previous runs. The firmware-warning stage hard-codes that directory for its firmware-build input. `rg` found no other scripts reading these fixed stage-log paths; documentation links the command, not individual fixed log paths.

Keep the existing 27 stage commands, order, platform/directory conditions, console result lines, error tails and nonzero failure semantics, with one narrow correctness exception: repair firmware-warning's negated grep, which currently treats grep input errors as success. Allocate `target/check-logs/run.XXXXXXXX` with `mktemp -d` before mise activation. Set existing LOG_DIR to this directory. Parent accepts unique paths without a compatibility alias and TSV. Existing failures still print their concrete full-log path.

Emit one append-only `stages.tsv` in that run directory, with header `stage\texit_code\tlog`. Before executing a stage, explicitly open its log and check that operation. If log opening fails, abort with a nonzero infrastructure error and leave incomplete; emit no child-exit receipt for a command that never ran. Execute the command using that already-open log descriptor; capture `$?` as the first operation in the failure branch. Append the actual child code in both branches, explicitly checking the write; a failed append aborts nonzero/incomplete. Ordinary command failures still accumulate status=1 and allow later stages to execute. Stage IDs and log basenames are existing fixed tokens. Omit commands.log and any new schema machinery.

The shell retains `set -uo pipefail`, without global errexit. Every evidence operation (creation, open, write, metadata command, comparison and rename) therefore needs an explicit checked branch whose failure propagates to a nonzero main exit; bare failing commands are insufficient. A comparison's expected different result must be distinguished from comparison I/O error, though either prevents completion.

Firmware-warning receives the actual current firmware-build log path as a positional argument to its existing child shell (or explicitly exported LOG_DIR), never an unexported shell-local name. Require a regular readable input, then map grep 0 (warning found) to stage failure, grep 1 (no match) to stage success, and grep >1 to stage failure. Do not negate grep. Keep this as the existing warning stage, not a new stage. This repairs an existing false-success path rather than expanding scope.

Use `run.status` containing `incomplete` initially. Initialize it before mise activation; capture activation stderr and eval diagnostics in run-owned setup.log. Explicitly check both `mise env` and `eval`; setup is not a canonical stage and supplies no successful stage receipt. If run-directory/status initialization itself fails, return nonzero with stderr explanation; do not promise a receipt in an unwritable directory.

Finalize in this exact order: all applicable stages return; record/check end snapshots and applicability; compare boundary snapshots; successfully write aggregate `exit-code` (0/1); write a temporary complete-status file and atomically rename it over run.status as the last evidence operation. A complete failed run retains exit-code 1. Any earlier evidence failure aborts nonzero with status incomplete. Complete means the evidence transaction finished, not that stages passed or that the shell subsequently exited successfully.

Signals must cause nonzero/incomplete; no trap may finalize. If the shell regains control after interruption, it may record the child's actual 128+signal exit; if it dies first, a missing stage receipt is honest. Do not fabricate a terminal receipt. Production descendant supervision is outside this unit. Tests use an isolated actual process session/group and bounded cleanup so interruption cannot leave their children running.

Do not create a new stage merely for these tests. Add the receipt tests to the existing `harnesses` stage, preserving its existing tests and the 27-stage count. macOS/firmware-absent runs retain their existing smaller conditional stage set; their receipts list only stages actually invoked. No universal 27-stage success claim.

## Source and config identity, with honest limits

At run start and end, save local `git rev-parse HEAD`, tracked/untracked `git status --porcelain=v1`, and `git diff HEAD --binary` snapshots. HEAD comparison deliberately includes staged and unstaged tracked changes; an unstaged-only diff is insufficient. Check every metadata command's real exit. Untracked porcelain records names/status, not file content: untracked bytes remain unverified, and unchanged snapshots cannot establish a stable candidate including untracked source.

Save a deterministic path/presence/Git-blob-ID manifest for `scripts/check` and actual canonical config inputs: mise.toml, workspace/firmware manifests and locks, pyproject.toml, uv.lock, and Cargo config files. Inventory actual paths before implementation; record absence for known optional paths and deterministic entries for discovered config paths, so addition/removal changes the manifest. Use existing Git `hash-object -- <file>`; label values Git blob IDs, never SHA-256 attestations. Compare actual snapshot/manifest content at the boundaries, not just HEAD/status.

These are limited boundary source/config snapshots, not a clean-commit attestation or complete effective-environment identity. Do not invent firmware codec/radio/config fields: scripts/check does not run a board. Record Linux/firmware applicability when the existing branch decisions actually occur; do not add a second platform decision or a universal 27-stage requirement. Explicitly label effective environment unverified; gather no arbitrary environment values. Resolving ambient Cargo/mise/tool overrides is outside this unit.

If start/end source or config snapshots differ, retain actual stage results, exit nonzero and leave incomplete with an explanatory small error.txt where writable. Equal snapshots prove only equal observed boundary content, not absence of intervening mutation or unchanged untracked bytes. A dirty run may complete with dirty snapshots, preserving existing CLI semantics; downstream review must not call it verified evidence of HEAD alone. Do not require a clean tree or change canonical commands.

Run directories are append-only by this command, retained separately and never reused or removed. This is operational immutability, not filesystem protection or cryptographic attestation. No freezing/chmod of entire directories or new cleanup manager.

## Real negative controls and verification

Expose actual stage, initialization, snapshot and finalization operations for host tests: place executable setup/stage calls in check_main and invoke it only when executed directly. Use BASH_SOURCE[0] for script location. Sourcing must not cd, activate mise or create a run. Each test can source functions in its own shell to contain shell options/globals. No mini-runner or stage registry. Tests execute real subprocesses and files rather than fake cargo/mise executables.

- A real child printing distinct stdout/stderr and exiting 7 produces a log with both streams, receipt exit 7, a FAIL line/tail, and accumulated script status 1. A succeeding real child afterwards still runs and produces exit 0 without clearing the prior failure.
- Two actual initialization/stage runs create distinct directories; hash earlier retained files before/after the second run and require identical bytes. Test the current warning detector with actual warning/no-warning fixtures, a missing log and a log path made a directory (permission tests alone fail under root). Missing/nonregular input must fail rather than become grep success. These are warning-detector tests, not firmware build evidence.
- A real long-lived child interrupted with the test's process-group cleanup leaves run.status incomplete and no successful completion receipt. A setup command that genuinely exits nonzero yields incomplete/no fake stage pass. Test the actual initialization/finalization operations, not a duplicate mini-runner.
- A log path made a directory prevents child execution (prove no child marker), produces no fabricated exit receipt and aborts incomplete. A receipt target made a directory after a real successful child causes nonzero/incomplete. Test failed setup command, failed eval, failed status rename and metadata errors through actual initialization/finalization operations; no global errexit workaround.
- A real temporary Git repo exercises unstaged change, staged change, config addition/removal and failed metadata acquisition using production snapshot/finalization functions. Changed tracked/config snapshots prevent completion. Unchanged dirty snapshots remain honestly dirty; changed untracked contents with unchanged names demonstrate the documented unverified limit rather than a false detection claim.
- Existing harness tests continue, and parent runs the real canonical `scripts/check` once on the frozen integration state. Verify all 27 expected stages appear exactly once, actual failures retain their exit codes, run is complete only after all returns, and old retained run bytes stay unchanged. Do not force an artificial failure in the production workspace just to test aggregation.

Use Python unittest from the existing locked uv harness; add its invocation before check-harnesses' final exec, preserving every current suite. Original rough estimate remains 185–290 added lines, but checked error paths may exceed it; correctness takes precedence over that estimate. If scope materially grows, narrow to unique run directories, actual stage TSV/status and explicit identity limitations rather than add a provenance framework. No dependency/lock/firmware/workflow changes.

## Acceptance and review decisions

Parent accepted TSV, unique run logs/no alias, and preservation of the 27 commands/order. This revision resolves the four documented review blockers: pre-opened logs versus actual exits, real grep error handling, staged-inclusive snapshots with untracked limits, and explicitly checked setup/signal/receipt/finalization paths. Independent scoped re-review clears the four blockers; implementation can proceed after the current device observation. Then implement test-first, independently review, and run canonical checks on the frozen state. Hosted/HIL and hardware gates remain open.

The earlier arbitrary review-record utility is rejected, not an alternate implementation option. No implementation or validation is claimed by this design. Independent review: target/kata-hardening/check-stage-evidence-review.md.

## Implementation checkpoint

Actual stage receipts, unique retained logs, checked snapshots/setup/finalization
and the warning-input fix are implemented. Independent review caught dynamic
descriptor syntax that required a newer Bash than the Mac path promises; fixed
descriptor scope now preserves an inherited descriptor. Shared signal-trap
installation makes the real interruption test exercise production handling.
Thirteen focused process/Git tests pass, including child exit 143 remaining an
ordinary failure. Bash 3.2 runtime is unavailable here and remains untested.

All 27 applicable canonical stages passed in target/check-logs/run.ikibfMAS,
with each expected stage exactly once, exit-code 0, complete status, and identical
start/end HEAD/status/tracked-diff/config snapshots. Parent reaped exit 0.
Independent scoped review and parent fresh-eyes are clear. Full log:
target/kata-hardening/check-stage-evidence.log. Hosted/HIL and streaming release
acceptance remain separate open gates; this unit does not close 97bk.
