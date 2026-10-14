# Validate receiver-side interruption and overload recovery (cv5a)

## Now
- Step: Tasks 1–3 complete (2026-10-14): review fixes B1–B3, S1–S8 applied,
  36 harness tests, `scripts/check --host` run.ABE6Lao4 passed 22 stages;
  harness committed, no device action yet
- Next: Task 4 device run, in order null-*, stall-*, deficit-*, delayed-reader
- Open: sudo must be cached by Doctor Biz (`! sudo -v`) before Task 4
- Approved: "effectively wmsy" (2026-10-13, choosing the epic); "cv5a via
  receiver-side netem (Recommended)" (2026-10-13, target question);
  "Yes, go (Recommended)" to "Start Tasks 1–3 now" (2026-10-13)
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
  listener in this plan has a flat 4 s bound. The WAV debug path uses the
  same 4 s bound and checks it before mapping a write timeout
  (`stream.rs:729,757`), so a starved WAV reader ends `MediaAge`, not
  `StoppedReading` as the October 4 trickle evidence recorded. It fires
  before the socket deadline and before ring overwrite (~12 s).
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
   recovery, or a `MediaAge` departure with checked close, slot freed and a
   fresh admission within 10 s of the close. Anything else fails.
3. `deficit-{0.8,0.5}-1`: rate at 80% and 50% of 320 kbit/s on ingress for
   up to 60 s or until departure. Expect a `MediaAge` end near the predicted
   time, bounded heap, then rate removed and a fresh admission within 10 s.
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
