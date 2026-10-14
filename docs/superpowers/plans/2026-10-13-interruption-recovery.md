# Validate receiver-side interruption and overload recovery (cv5a)

## Now
- Step: Task 4 attempt 1 (2026-10-14 14:27 UTC, `target/kata-hardening/
  oct14-interruption`, retained): null-1 and null-2 PASSED (exact frames,
  counters flat, tolerances age 122,471 us / largest 0 B); stall-0.5-1 died
  on a harness TypeError in `Driver.apply`; qdisc restore verified, ifb0
  removed, no board or settings change. Fixed with a real Driver test;
  `prio bands 4` and mq-root restore are now confirmed with root.
- Step: verdict fixes, close-line replacement trigger and `--reverdict`
  implemented and independently reviewed twice (53 tests); attempt 4
  re-judges 13/13, attempt 3 11/13; `scripts/check --host` running
- Next: commit, re-run `--reverdict` on attempts 3 and 4 so the recorded
  harness hashes are the committed code, write the evidence pair (Task 5),
  then ask Doctor Biz whether a fifth run under the final harness is wanted
- Open: Doctor Biz runs the Task 4 batch from their own tty ("You run it in
  your tty", 2026-10-14) because `sudo -n` cannot see a per-tty timestamp
  from the agent's shell; the agent evaluates `result.json` afterwards
- Approved: "effectively wmsy" (2026-10-13, choosing the epic); "cv5a via
  receiver-side netem (Recommended)" (2026-10-13, target question);
  "Yes, go (Recommended)" to "Start Tasks 1–3 now" (2026-10-13);
  "Yes, sudo is cached" to "Run Task 4 (the device batch) now?" (2026-10-14)
- Compactions: 0

## Scope

Kata cv5a asks for repeatable validation of interruption recovery, ownership
and sustained overload. Its `blocked-by` list is stale: 4dss, r0ve, dp08 and
pd6c are closed. Only pqqq (ownership-safe resume) is open.

No AP access exists (gotchas, October 12). The workstation is the receiver,
so Linux `tc` on the workstation's own interface can inject loss and rate
limits on the path that carries board traffic to this host. That covers three
of cv5a's four scenarios. It cannot cover the fourth: the Sonos speaker
(192.168.23.205) fetches its stream straight from the board, so workstation
shaping never touches it, and no automatic resume exists to test (pqqq).
"Sonos resumes only when it still owns playback" is recorded as the explicit
unmet gate; this plan does not close cv5a without it, but it moves the three
executable scenarios from proposed to measured, which is bf85's precondition.

No prior trial has injected network loss or throttling. Earlier faults were
board-side `esp_wifi_disconnect()` (network-generations), a control-thread
sleep (control-timeout) and application-level trickle reads (stream-churn).

## What the firmware promises (read, not measured)

- Per-frame absolute write deadline: MP3 8016 ms, FLAC 8021 ms with
  recognition off (`core/src/stream_buffer.rs:71-77`).
- Media age bound: 4 s from `oldest_accepted_us` (`core/src/media_age.rs:16-35`).
  The preset's seconds are added only on the Sonos-tagged buffered route;
  a plain `/stream.mp3` request is `Route::Stream` with `preset = None`
  (`core/src/stream_http.rs:637`, `firmware/src/stream.rs:427`), so every
  listener in this plan has a flat 4 s bound. The WAV debug path has the
  same 4 s age bound plus its own per-frame write limit of ring capacity /
  48 kHz (`stream.rs:636-643`); the tighter one wins, and on the board the
  write limit fired first: a starved WAV reader ended `stopped reading` at
  3.05 s (attempt 4), as the October 4 trickle evidence recorded. For MP3
  the age bound fires before the 8 s socket deadline and ring overwrite.
- A listener the harness closes itself logs as a departure too:
  `left after X s: socket error kind=ConnectionReset`. Verdicts must separate
  those from product departures by timing against the decoder's own end.
- Freshness: 2 s `FRESH_AUDIO_TIMEOUT` → `CaptureStalled`; about supply, not
  the peer (`core/src/stream_pipeline.rs:19`).
- End reasons: `MediaAge`, `StoppedReading`, `FellBehind`, `NetworkChanged`,
  `CaptureStalled`, `EncodedReset`, `Failed(io)`, … (`stream_pipeline.rs:358`).
  They appear only on UART: `stream: {peer} left after X s: {end}`
  (`firmware/src/stream.rs:1434`). `/api/metrics` carries slot state,
  `media_age_us`, heap, capture counters and `last_listener_departure.stats`,
  but no reason.
- There is no in-stream resynchronization. Every end is terminate, checked
  close (slot held until close confirmed), then client reconnect.
- Autoplay can issue a real Play after 10 s below −60 dBFS then 500 ms above
  −50 dBFS (`firmware/src/sonos.rs:1733-1742`). It is off on this board; the
  harness asserts `settings.autoplay == false` before every trial.

Expected outcomes, to be confirmed or overturned by the trials:
0.5 s and 1 s stalls survive with `media_age_us` raised by about the stall
and returning to baseline; 4 s stalls end `MediaAge` at roughly 3.7–3.9 s
into the stall plus preset seconds; 2.5 s is borderline and is recorded
either way. A sustained rate below the encoded bitrate ends `MediaAge` once
age reaches the bound; the time to that end follows from the deficit and is
checked against the formula with a tolerance, not asserted exactly.

## Injection design

- Interface: wired `enp5s0`, source 192.168.23.153 (`ip route get
  192.168.23.217` on 2026-10-13). gotchas records Wi-Fi as the default on
  October 12; today Ethernet has the lower metric. The harness pins the
  interface with `SO_BINDTODEVICE` (pattern: `scripts/wifi-throughput-check`)
  and records `ip route get` output. Wired receiver removes workstation Wi-Fi
  as a confounder; the board's own Wi-Fi hop stays real and unshaped.
- Egress (ACKs and requests to the board): `enp5s0` root `prio`, a `u32`
  filter on `dst 192.168.23.217/32` to a netem band. `enp5s0` currently has an
  `mq` root with four `fq_codel` children; the harness records the full
  `tc qdisc show dev enp5s0` text before touching it, deletes its own root on
  exit (which restores the default qdisc) and fails loudly if the text after
  restore differs from the recording.
- Ingress (media from the board): `ifb0` + `ingress` qdisc + `mirred egress
  redirect`, netem on `ifb0` filtered by `src 192.168.23.217/32`.
  Modules `sch_netem`, `sch_tbf`, `ifb` are present on kernel 7.0.0-38;
  tc is iproute2-6.19.0.
- Interruption: `loss 100%` on both directions for the requested seconds,
  timed by a monotonic clock, with the actual install/remove timestamps
  recorded; a 0.5 s request that took 0.62 s to apply is reported as 0.62 s.
- Deficit: netem `rate` on `ifb0` only (media direction) at a stated fraction
  of the measured encoded bitrate. MP3 is 320 kbit/s fixed at 960 bytes per
  1152-sample frame. The ACK direction is left alone in the first batch; a
  deliberate ACK-side trial is a separate named trial, per n4jb.
- Control: every profile also runs once with the full qdisc tree installed
  and `loss 0%`, so the tree itself is shown not to change outcomes.
- Seed: netem `seed` is recorded when this tc accepts it (verify at
  implementation).
- Privilege: non-interactive sudo is unavailable here. The harness refuses to
  run as root, requires a user-started `sudo -v`, invokes `sudo -n tc …` so a
  lapsed timestamp fails instead of hanging, offers `--dry-run` that prints
  every tc argv and creates no files, and restores the qdisc tree in a
  `finally` that also runs on SIGINT/SIGTERM. Pattern:
  `scripts/capture-stream-packets`.

## Trials

All trials: board at 192.168.23.217, MP3/Medium as deployed, Shazam and
autoplay off, Sonos stopped and untouched, LINE IN driven by continuous
workstation audio through the front headphone jack (gotchas; mixer untouched).
Each trial fetches `/api/metrics` before and after (boot token, ELF hash,
settings, network generation, capture counters, heap) and polls it and
`/api/sonos` every second during the run: that is the "control traffic".
Capture baselines are taken before any injection so setup loss cannot hide.

1. `null-1`, `null-2`: tree installed, 0% loss, one and two MP3 listeners,
   60 s. Expect exact decoded frames, no departures, counters flat.
2. `stall-{0.5,1,2.5,4}-{1,2}`: one or two MP3 listeners, 20 s steady, the
   stall, then 20 s. Verdict per listener: continuous decode with age
   recovery, or a `MediaAge` departure with checked close and a fresh
   admission, one per departure, within 10 s of the departure (the
   replacement starts on the close line, since the slot is held until
   then). Anything else fails.
3. `deficit-{0.8,0.5}-1`: rate at 80% and 50% of 320 kbit/s on ingress for
   up to 60 s or until departure. Expect a `MediaAge` end no later than the
   predicted time plus tolerance and no earlier than the physical floor
   (age grows at most 1 s/s), bounded heap, then rate removed and a fresh
   admission within 10 s of the departure.
   Internal and PSRAM largest-free must return to within a tolerance of the
   pre-trial value; the tolerance is set from `null-*`, not invented.
4. `delayed-reader-1`: one MP3 decoder plus one WAV reader trickling 1 byte
   per 0.5 s (stream-churn's read pattern, in-process). Expect the MP3
   listener unaffected (continuity, counters flat) and the WAV reader to end
   on its own deadline with checked close.

Each decoder is ffmpeg `-f framehash` through `decoder_integrity.FrameHash`,
so sample counts are real, and `-err_detect crccheck -xerror` with the log
scan rejects frame-number and CRC faults. Departures are paired with their
closes by `encoder_fault_check.checked_listener_closes` against the UART
listener export. Failed trials stay in the result with `passed:false`; no
unchanged reruns.

## Task 4 findings (2026-10-14, attempts 3 and 4, both retained)

Attempt 2 (`-2`) stopped in 9 ms on the sudo probe; attempt 1 died on the
`Driver.apply` TypeError after two passing null trials. Attempts 3 and 4
(`oct14-interruption-3`, `-4`) each ran all 13 trials in ~675 s with the
qdisc tree restored and verified. Image `0ff2a7cc…` throughout.

- `prio bands 4` is accepted without a priomap; `del root` on the `mq`
  device restores the default tree byte-for-byte. Both root-only unknowns
  are now settled.
- Null trials: exact frames, no departures, counters flat, both attempts.
  Attempt 4 measured the age bound at 142,463 us; attempt 3 at 2,126,239 us
  (one steady-state transient), which made its deficit tolerance 10.6 s.
- Stalls, attempt 4: all eight survived with exact frames and zero
  discontinuities; `stall_actual_s` 0.549/1.068/2.549/4.049 (one listener),
  0.548/1.048/2.552/4.050 (two). Age after removal peaked 0.43–3.17 s and
  was 45–77 ms by the last active sample. The verdict read the post-trial
  idle sample (`null`) and failed every one: verdict bug.
- Stalls, attempt 3: 2.5 s stalls ended `MediaAge` on both listeners about
  3.5 s after removal, with checked closes and fresh admissions; 4 s stalls
  survived. Hypothesis to record, not a finding: lwIP RTO backoff lands a
  retransmit just past the 4 s media-age deadline for a 2.5 s blackout.
- Fresh admission: the fresh listener joins before the departed listener's
  checked close (second slot free; close lingers ~1.5 s). `deficit-0.8-1`
  passed by 5 ms of ordering. Measure from the departure: verdict bug.
- Deficit: `MediaAge` at 18.5 s (attempt 4) and 10.0 s (attempt 3) against a
  20 s prediction at 0.8×; 9.0 s both attempts against 8.0 s at 0.5×. A TCP
  flow through a rate shaper gets at most the nominal rate, so the formula
  is an upper bound on time-to-departure. Contract loosened to "no later
  than predicted + tolerance"; stated here and in the evidence.
- Delayed reader: WAV ended `stopped reading` at 3.05 s (its own write
  limit fires before the 4 s age bound), checked close 2 s later, MP3
  listener exact at 1,920,000 frames. Review finding S2 was wrong; either
  bounded end is a clean outcome. Verdict accepts both and records which.
- Metrics polls timed out 1–4 times per stall and 9–17 times per deficit:
  the control path was hit too. Expected; recorded, not a failure.
- Re-verdict with the corrected code (harness 331ea17 + fixes): attempt 4
  13/13 PASS; attempt 3 11/13. Attempt 3 `stall-4-2`: one `MediaAge`
  departure, the survivor held the other slot, and the replacement started
  on the departure line 2.04 s before the checked close, so the board
  refused it. Correct two-slot behavior; the trigger is changed to the
  close line or decoder EOF (reversing review item S8). Attempt 3
  `deficit-0.5-1`: internal largest free block 47,104 → 43,008 B after the
  trial against a measured null tolerance of 0 B; attempt 4 ended its batch
  at 45,056 B. A 2–4 KiB drop after listener churn, not shown to recover
  within a run and not shown to grow further. Recorded FAIL; not toleranced
  away; candidate follow-up, not this ticket.

## Tasks

1. `scripts/network_fault.py`, `scripts/network-fault`,
   `scripts/test_network_fault.py`: qdisc planning as pure functions
   returning tc argv; parser for `tc qdisc show` restore verification (tested
   on this machine's real output, unprivileged); stall timing accounting;
   verdict evaluation from recorded events; root refusal; dry-run creates no
   files; `--help`. Register in `scripts/check-harnesses` and the import-safety
   lists in `scripts/test_harness_modules.py:420,434`. TDD, RED then GREEN.
   Template: `stream_soak.py`; closures reused from `fault_check.py`.
2. Trial driver inside the same module: listeners (`decoder_command`,
   `update_decoder`, `stop`), UART via `scripts/serial-log` and `Tail`,
   metrics via `device_metrics.fetch`, result written through
   `result.pending.json` then atomic replace, bounded overall deadline and
   cleanup budget (`sonos_http_check.CleanupDeadline`).
3. Independent review of Tasks 1–2 by a clean-context subagent, then
   `scripts/check --host`.
4. Device run, in order: `null-*`, `stall-*`, `deficit-*`, `delayed-reader`.
   Doctor Biz runs `! sudo -v` first. Stop the batch on the first infrastructure
   failure (qdisc restore mismatch, sudo lapse, board reboot) and diagnose
   before continuing; product failures are recorded and the batch continues.
5. Evidence: `docs/audit-evidence/2026-10-13/interruption-recovery.md` and
   `.json` in the house shape (image/boot identity, interface/route/qdisc
   text, tc and kernel versions, trials keyed by name, limits, raw hashes
   under `target/kata-hardening/oct13-interruption/`). Kata comment on cv5a
   with the measured outcomes and the exact unmet gate.
6. Full `scripts/check`, commit on `wip/kata-capture-protection` (the
   kata-hardening working branch; no push).

Ownership: parent owns hardware, canonical checks, integration and commits.
Implementation and review go to clean-context subagents. The kata-hardening
plan names "GPT-6.1 Sol" subagents; no such agent type exists in this
session, so general-purpose Claude subagents are used and this is recorded.

## Limits

This proves recovery against receiver-visible loss and rate limits on a wired
host, not RF interference, AP queueing or the ESP32 Wi-Fi driver. MP3 only,
as deployed; FLAC would need a saved-setting change. No Sonos playback, so
nothing here speaks to ownership-safe resume (pqqq) or cv5a's fourth
scenario. Workstation `tc` cannot reach the Sonos → board path at all.
