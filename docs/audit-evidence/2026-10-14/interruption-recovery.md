# Receiver-side interruption and overload recovery (cv5a, 2026-10-14)

Two full batches of thirteen trials ran on the real board with Linux `tc`
netem/tbf on the workstation's wired receiver interface, filtered to the
board's address. Under the corrected verdict code, attempt 4 passes all
thirteen trials and attempt 3 passes eleven. Every stall of 0.5, 1, 2.5 and
4 s with one and two MP3 listeners was survived in attempt 4 with exact
decoded frames and no discontinuity; in attempt 3 the 2.5 s stalls ended
both listeners on media age about 3.5 s after the path was restored, with
checked closes and fresh admissions. This does not establish anything about
the Sonos path (the speaker fetches directly from the board and workstation
shaping cannot reach it), ownership-safe resume (pqqq), RF or AP behavior,
FLAC, or long-term delivery. Hashes, identities, per-trial numbers and the
retained original verdicts are in `interruption-recovery.json`.

## Method

`scripts/network-fault` (harness commit in the JSON) installs a four-band
`prio` root on `enp5s0` with a netem child on band 4 and a `u32` filter on
`dst 192.168.23.217/32`, plus an `ingress` qdisc redirecting
`src 192.168.23.217/32` to `ifb0` with a root netem. A stall is `loss 100%`
in both directions for the requested seconds; a deficit is `rate` on `ifb0`
only. Each batch opens the UART (which resets the board), waits for
readiness, records `/api/metrics` identity and settings, and polls metrics
and `/api/sonos` every second. Listeners are ffmpeg `-f framehash` decoders
of `/stream.mp3` counted by `decoder_integrity.FrameHash`; departures and
checked closes come from the UART listener export. Null trials (tree
installed, 0% loss) measure the steady-state age bound and largest-free
swing that later verdicts use. The board ran image `0ff2a7cc…`, MP3/Medium,
autoplay and recognition off, Sonos stopped and untouched, quiet LINE IN.

Four attempts, all retained under `target/kata-hardening/oct14-interruption*`:

1. 14:27 UTC: null-1 and null-2 passed; the first stall died on a harness
   `TypeError`; qdisc restore verified. Fixed in `8e74e5f`.
2. 15:06 UTC: stopped in 9 ms on the `sudo -n` probe before touching
   anything. The launcher `scripts/network-fault-batch` now runs `sudo -v`
   in the same shell.
3. 15:09 UTC, 674.5 s, boot `10b4dd7f…`, 76 raw artifacts.
4. 15:20 UTC, 680.5 s, boot `1e858ec1…`, 65 raw artifacts.

The original verdict code in attempts 3 and 4 failed trials the product
passed: it judged the stall survival on the post-trial idle metrics sample,
required the replacement join to follow the checked close, expected a WAV
reader to end on media age, and treated the deficit prediction as a point
estimate. Those `result.json` files are retained unchanged. `--reverdict`
re-judges each run's retained `events.jsonl` and `result.json` with the
corrected code into `reverdict.json`, and the figures below come from that.

## Null controls

Both attempts: exact frames (2,880,000 per listener over 60 s), no product
departures, capture counters flat from the baseline (`queue_overflows` 8 at
boot, a known startup fault, unchanged after). Attempt 4 measured the age
bound at 142,463 us and a 6,144 B swing in the internal largest free block;
attempt 3 measured 2,126,239 us (one steady-state transient) and no swing.

## Stalls

Held durations matched requests: attempt 4 0.549/1.068/2.549/4.049 s with
one listener and 0.548/1.048/2.552/4.050 s with two; attempt 3 within 0.1 s
of those. Metrics polls to the board timed out one to four times per stall,
so the control path was cut too.

Attempt 4: all sixteen listener-trials survived. Exact frames (1,944,000,
1,968,000, 2,040,000 and 2,112,000 per listener for 0.5, 1, 2.5 and 4 s),
zero discontinuities. Media age after removal peaked between 0.43 s
(0.5 s, one listener) and 3.17 s (2.5 s, two listeners; 3.16 s for 4 s,
two listeners) and was 45–77 ms at the last active sample.

Attempt 3: 0.5 s, 1 s and 4 s with one listener survived likewise (4 s peak
3.27 s). The 2.5 s stalls ended on `media age exceeded`: one listener at
3.5 s after removal (1,001,088 frames decoded), both listeners at 3.0 and
3.5 s (930,816 and 928,512 frames); each had a checked close 0.5–1.0 s
later and a fresh join 0.5 s after the departure. The 4 s two-listener
trial ended one listener on media age while the other survived; the
replacement, started on the departure line 2.04 s before that slot's
checked close, was refused 503 while the survivor held the other slot. That
is the two-slot contract working; the harness now starts replacements on
the close line, and that trigger has not yet run on the board. The 2.5 s
divergence between attempts is recorded, not explained; lwIP retransmit
backoff landing a retransmit near the 4 s media-age deadline is a
hypothesis only.

## Deficits

Rate at 80% of 320 kbit/s ended the listener on media age 18.54 s after
install in attempt 4 and 10.02 s in attempt 3; at 50%, 9.03 s and 9.02 s.
The naive prediction (20 s and 8 s) is an upper bound, since a TCP flow
through a rate shaper gets at most the nominal rate; the corrected verdict
requires the end no later than prediction plus tolerance and no earlier
than the physical floor. All four had a checked close and a fresh join
within 10 s of the departure. Metrics polls timed out 9–17 times per
deficit. In attempt 3 the 50% trial left the internal largest free block at
43,008 B against 47,104 B before, with a measured tolerance of 0 B: FAIL,
retained. Attempt 4 ended its batch at 45,056 B. A 2–4 KiB drop after
listener churn is not shown to recover within a run and not shown to grow.

## Delayed reader

One MP3 listener plus a WAV reader taking one byte per 0.5 s. The WAV reader
ended `stopped reading` 3.05 s after admission (its own write limit fires
before the 4 s age bound), checked close 2.0 s later; the MP3 listener was
exact at 1,920,000 frames with capture counters flat. Both attempts agree.

## Restoration

Every attempt deleted its qdiscs and `ifb0` in a `finally` and verified the
`enp5s0` tree equal to the pre-run `mq` + four `fq_codel` listing. No
settings, Sonos state, firmware or saved configuration changed; the board
reports the same image before and after. `prio bands 4` without a priomap
and default-tree restore after `del root` on an `mq` device were unverified
without root before these runs and are now confirmed.

## Limits

Workstation listeners on a wired receiver only: this is recovery against
receiver-visible loss and rate limits, not RF, AP queueing or the board's
own Wi-Fi hop. MP3 as deployed; FLAC would need a saved-setting change. No
Sonos playback, no ownership evidence, cv5a's fourth scenario unmet. The
attempt-4 pass is an offline re-judgment of complete retained data; the
original failing verdicts are kept beside it. The corrected replacement
trigger is untested on hardware. Two attempts disagree on 2.5 s stalls.
