# Close the cv5a follow-ups (gcq8, 7fe1, ek5b)

## Now
- Step: gcq8 run done (`oct14-churn-1`, 30 cycles, 372.7 s): heap
  acceptance met (47,104 B before, cycle 1 and cycle 30; one-step dip at
  cycles 21–22, slope −10.9 B/cycle). Three listeners cut by the board in
  cycles 6 and 13 with no tree installed: ek5b evidence, commented there.
  Review found B1 (cut listener with exact frames judged clean) and
  S1–S7; fixes plus `--rejudge` and a heap/listeners verdict split are
  with the implementer. `--pcap` launcher change for ek5b with a second
  agent. Evidence `.md` drafted.
- Step: gcq8 closing. Harness and `--pcap` launcher committed as
  `cef749c` after `scripts/check --host` run.8w5SmF89 (22 stages);
  `oct14-churn-1` re-judged under it: heap_passed true, listeners_clean
  false (3 observations), all discontinuities 0. Evidence pair
  `docs/audit-evidence/2026-10-14/listener-churn.{md,json}`.
- Next: 7fe1 then ek5b in Doctor Biz's tty (`scripts/network-fault-batch
  -- --trials null-1,null-2,stall-4-2`, then five of
  `scripts/network-fault-batch --pcap -- --trials null-1,null-2`), one
  board user at a time; the agent evaluates each result.json.
- Follow-up noted, not ticketed: start/poll-decoder helpers exist three
  times (stream_soak inline, network_fault.Driver, listener_churn); lift
  into stream_soak when one of them next changes.
- 2026-10-15: hv4x closed on verified acceptance; dctv kept open with the
  agent's recommendation (bounded skip for live Sonos); branch pushed and
  draft PR #1 opened for m31k/97bk. First hosted `Canonical checks` run
  (38764870099, `push` event) failed on infrastructure only: rustfmt and
  clippy components absent for the pinned toolchain, `uv` not a mise tool
  so every Python stage got `uv: not found`, and the firmware job's final
  boundary check tripped on seven ESP-IDF bootstrap `Cargo.toml` files
  under `firmware/.embuild`. Stages that ran passed (test, loom, sonos-web,
  coverage-host, firmware-build, firmware-warnings). No `pull_request`
  event ran any workflow, so host-coverage, host-fuzz and stream-replay
  never started; cause under investigation. Fixes delegated; m31k stays
  open until the hosted coverage job runs and its reports are inspected.
- 2026-10-15, later: `5f46cef` (rustfmt/clippy/uv pins, `.embuild`
  prune) and `e8ee49a` (ripgrep, conda ffmpeg 8.0.1, workflow activation)
  pushed; `pull_request` runs now fire on synchronize. At `e8ee49a`:
  canonical host+firmware, coverage and replay GREEN; m31k CLOSED on the
  inspected hosted coverage report. Host fuzz FAILED on a stale harness:
  seed `ramp` encodes left/side since Oct 11 and `fuzz/src/lib.rs`
  asserts independent channels; reproduced locally; fix delegated (harness
  only). Open count 13.
- Open: none
- Approved: "let's add katas and then start nuking em" (2026-10-14);
  "you should verify and close. why would i know?" (2026-10-15, closures
  are the agent's verification); "Yes, push and open the draft PR"
  (2026-10-15, first push of wip/kata-capture-protection for m31k)
- Compactions: 0

## Scope

Three bounded tickets filed 2026-10-14 from cv5a's evidence
(`docs/audit-evidence/2026-10-14/interruption-recovery.md`). Each has its
acceptance in the ticket body; this doc only fixes the order and the
ownership. Keep one closure target in progress; a failed acquisition is
retained and reported, not retried blindly (gotchas).

1. **gcq8** — internal largest-free drop after listener churn. Thirty
   two-listener MP3 cycles with `/api/metrics` sampled between cycles.
   Needs no root, no tty, no physical change: runs from the agent's shell.
   `scripts/listener-churn`, tests, `check-harnesses` registration.
2. **7fe1** — survivor-holds-a-slot replacement. One
   `scripts/network-fault-batch -- --trials null-1,null-2,stall-4-2` from
   Doctor Biz's tty under `84a0abc` or later; at most one more batch if
   the trial does not produce exactly one departure.
3. **ek5b** — quiet-network sender stall. Five null-only batches with a
   receiver pcap each. `network-fault-batch` grows a `--pcap` option that
   runs `scripts/capture-stream-packets` for the batch so this is one
   command per batch; stop at five regardless.

Parent owns board runs, canonical checks, evidence and commits; subagents
implement and review. No buffer, priority, Nagle, Wi-Fi, heap or allocator
change under any of the three.
