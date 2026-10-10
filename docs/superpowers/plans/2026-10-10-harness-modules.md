# Kata 7hr4: importable harness implementations

Implementation and independent review complete; all 21 canonical stages passed.
The original planning scope below came from `kata show 7hr4 --json`, scripts/check,
the four harnesses, their three AST-based suites and gotchas.md. No hardware was
used to validate this refactor. Report: target/kata-hardening/harness-report.md.

## Scope and acceptance

Keep scripts/fault-check, scripts/encoder-fault-check, scripts/stream-soak and scripts/stream-check as public shell entry points. Move each embedded Python body to one importable sibling module (`fault_check.py`, `encoder_fault_check.py`, `stream_soak.py`, `stream_check.py`). Delete each heredoc implementation in the same change that connects its wrapper. Do not add a second implementation or compatibility shim. Existing imports stream_format, decoder_integrity and failure_observation remain the shared sources of truth.

All imports must be inert: no argparse execution, output creation, signal registration, DNS, subprocesses or device calls. `main(argv=None)` owns parsing and execution; tests call `parse_args(argv)` and explicit helpers. Preserve shell validation/help where it currently rejects duplicate options or unsupported combinations; do not silently broaden fault-check's CLI. Keep its original argument list intact for health-mode explicit-option rejection. Avoid changing deadlines, summary fields, PASS/FAIL lines, exit statuses, umask or cleanup order while exposing helpers.

Success: the actual UART readers, decoder command factories/accounting/completion logic, validators and process cleanup helpers are directly imported in tests; all existing negative controls survive; real wrapper subprocess tests prove help, invalid-argument handling, caller-relative output semantics and import safety; canonical scripts/check stays green. Hardware acceptance is outside this host-only refactor and is not implied by a passing host suite.

## Bounded implementation units and ownership

1. Python environment owner: pyproject.toml, uv.lock and changed harness/test shell launchers. Formalize a uv project with locked NumPy (already needed by stream-check and decoder/format tests); use unittest already in stdlib. Explicitly select the repo project from wrappers while preserving the caller's cwd. Verify uv's actual project/cwd behavior before choosing flags; do not assume --project implies cwd changes. Do not install or replace external ffmpeg, mise, Rust or ESP tools. Keep scripts/check the canonical command. No new Python framework or packaging build backend is needed merely to import sibling modules.
2. Fault harness owner: scripts/fault-check, scripts/fault_check.py, scripts/encoder-fault-check, scripts/encoder_fault_check.py. Lift Python intact first, then give UART reads an explicit Path and give wait/match, decoder duration/completed, command construction, budget checks and stop-process helpers explicit arguments. Encoder currently runs almost everything at top level; move runtime state into main/run or a small session object rather than leaving mutable global resources. Preserve its thread stop/shutdown/join semantics and original two-second process TERM/KILL waits. Fault-check uses nested closures: expose only helpers the tests need and pass the existing record/check callbacks rather than building a generic harness framework.
3. Stream harness owner: scripts/stream-soak, scripts/stream_soak.py, scripts/stream-check, scripts/stream_check.py. Expose parser, real FFmpeg command construction, Tail, decoder_updates and final completion validation; keep the exact same functions in the live loop. Preserve soak's final drain both when exit races accounting and when all children have exited. Keep stream-check analysis/decode/check_length functions and its packet-copy policy distinct from output-duration decoder exact-sample policy. Do not merge those policies. Preserve three-second soak stop waits and failure observation's original collector identity.
4. Test migration owner (after helper contracts settle): scripts/check-harnesses, scripts/test_decoder_integrity.py, scripts/test-stream-format and new unittest modules only as needed. Replace AST extraction/evaluation with imports and real calls; delete tree/load/command AST adapters. Do not retain a legacy fragment path. Existing test commands remain callable and canonical check stages remain named as today. Add a dedicated CLI/inert-import regression module reached by scripts/check-harnesses. Parent controls shared launcher/environment edits to prevent ownership collisions.

Units 2 and 3 may proceed independently only after agreeing on parser/main, decoder command and update helper signatures. Unit 4 then migrates consumers; no need to rewrite all nested runtime state into classes. All four harness modules belong in this bounded migration because current test-stream-format and test_decoder_integrity also extract stream-check code. Migrating only the three named in check-harnesses would leave the same failure mode in the canonical suite.

## Test-first sequence

Before editing each wrapper, add imported-interface tests that fail because its module/interface does not exist. Move that module and connect its wrapper, then make its tests pass. Existing fragment tests may remain temporarily during migration, but remove their adapters before declaring 7hr4 complete. Use real temp files, real child processes, real local fixture decoders and loopback HTTP where applicable. No mock API mode or mocked end-to-end behavior.

Retain check-harnesses' absent-file/partial-LF/CR/invalid-UTF8 reader cases, all geometry field/boundary cases, ASCII-only bounded port parsing, default/custom/boundary endpoints and 1/2 listener selection. Assert built endpoint URLs and decoder argv through production factories rather than evaluating string AST nodes. Add tests for Tail unfinished/oversized/backlogged records. Budget tests grow actual temp files across the exact 128 MiB boundary (sparse files suffice for stat-based budgets); assert retained evidence and the original error.

Retain every IntegrityTests case, particularly complete sample counts in both harnesses; WAV24 low bits/channel order; exact output duration; stream-check whole-final-packet allowance without underflow; omitted/duplicate/corrupt/CRC-negative FLAC controls; corruption diagnostics; incremental partial records counted once; truncated final record/oversized log rejection; encoder marker wait covering lookup/hold/delivery; 16.532-second EOF allowance; encoder actual decoder missing-audio rejection; both soak final-flush interleavings. Tests must call the production completion branch/helper instead of constructing an AST while-loop.

Retain FormatTests' real WAV/FLAC metadata/scale and stereo-energy checks, production soak FLAC/MP3 command duration behavior and MP3 short-input rejection. Keep failure-observation tests, especially invalid grace rejected before any collector and postfailure raw-log-only accounting. Do not weaken exact sample accounting to timeline duration.

Real CLI tests execute all four unchanged public script names under a temporary cwd: help and malformed arguments exit before side effects; imports work with unrelated argv and absent *_REPO env vars; no output files appear during import. Use a fresh child interpreter with a real audit hook to reject socket/subprocess attempts during imports. Output resolution can use imported production resolve-output helpers plus CLI runs with deliberately invalid/unresolvable host only if failure is guaranteed bounded and needs no external network; prefer loopback fixture integration for actual run/summary wiring. This plan authorizes no physical UART or board call.

For lifecycle integration, spawn real new-session child processes, including a child that ignores TERM, and exercise production stop helpers; assert reaping and existing timeout behavior. Keep socket shutdown before joining workers. Test signal interruption in real CLI subprocesses where runtime can be exercised using local fixtures without serial access; otherwise report remaining hardware wiring limits explicitly.

## Contracts requiring care

- Relative --out resolves against caller cwd. Default fault-check/soak/stream-check outputs are repo-root target paths; encoder's current default target/encoder-fault-check resolves against caller cwd. Preserve that distinction.
- fault-check performs shell validation before Python parsing. Moving validation casually changes duplicate-option/error behavior; retain shell guard initially.
- encoder imports are currently unsafe because parsing, mkdir, handlers and DNS happen at top level. Simply renaming its file is insufficient.
- soak decoder_updates captures args and completion lives inside main. Extracting a pure command alone misses the race regressions; expose and invoke the actual completion helper.
- Endpoints are currently duplicated expressions across parser, DNS, state and FFmpeg paths. Centralize only where needed for callable interface tests; do not add state or a configuration framework.
- Soak's checked budget permits polling/cleanup overshoot. Preserve that documented contract rather than claiming a hard filesystem quota.
- Existing stream-check uses NumPy; new import tests must run under the repo uv environment rather than the caller's ambient Python project.

## Verification and handoff

Run each affected canonical test script after its migration; then scripts/check once all changes are integrated, reading each failed stage's full log. No hardware acceptance run is required or authorized here. Before declaring done, review the actual diff for deleted fragments, imports with side effects, cwd semantics, helper call sites, final-drain paths and changed deadlines/results. Search separately for AST consumers, embedded heredocs, legacy environment variables and stale --no-project comments in changed launchers. Record actual limits and evidence in the parent plan/Kata only after verification.

Confidence: high on scope and regression inventory; medium on exact minimal helper signatures until implementers lift the runtime closures. The main complexity is encoder mutable session state and soak closure/race exposure, not package layout. Estimated touched source is roughly 1,500 existing harness lines moved, plus 200–400 lines of focused interfaces/tests; avoid treating moved lines as a reason to broaden behavior.
