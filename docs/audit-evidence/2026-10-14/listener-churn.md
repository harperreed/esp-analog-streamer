# Internal largest-free block across listener churn (gcq8, 2026-10-14)

Thirty cycles of two MP3 listeners for 10 s each, with `/api/metrics`
sampled after both slots returned to idle, show no downward drift in the
internal largest free block: 47,104 B before the run, 47,104 B after cycle
1, 47,104 B after cycle 30, with a single one-step dip to 45,056 B at
cycles 21–22 that recovered by cycle 23. Least-squares slope −10.9 B per
cycle. gcq8's acceptance (last within one 2 KiB step of cycle 1, no
downward trend over the first five versus last five cycles) is met. The run
also had the board cut three of sixty listener-runs early with nothing
injected; that is reported here and on ek5b, and it does not invalidate the
heap samples, which were taken after the slots went idle. This does not
establish anything about PSRAM behavior under other workloads, about longer
churn, or about the cause of the cuts. Hashes and per-cycle rows are in
`listener-churn.json`.

## Method

`scripts/listener-churn 192.168.23.217 --cycles 30 --seconds 10` (harness
commit in the JSON) from the agent's shell: no root, no UART, no settings
change. Each cycle starts two ffmpeg `-f framehash` decoders of
`/stream.mp3`, counts decoded frames with `decoder_integrity.FrameHash`
(expected 480,000 per listener), waits up to 15 s for both `listeners[]`
slots in `/api/metrics` to report `idle` (this stands in for UART checked
closes, which need the serial port), then takes one metrics sample. The
baseline sample gated `autoplay` off and `active_format` MP3; no qdisc or
other injection existed on the receiver. Image `0ff2a7cc…`, boot
`586c4386…` before and after, 372.7 s wall, output
`target/kata-hardening/oct14-churn-1`, retained.

## Heap

Per-cycle `internal_largest` (B): 47,104 for cycles 1–20, 45,056 for 21–22,
47,104 for 23–30. `internal_free` moved within 47,375–47,931 throughout.
PSRAM largest unchanged. Slots returned to idle in 0.1–1.3 s after the
decoders closed. First move from the cycle-1 value: cycle 21. Baseline and
final boot tokens equal, so one boot covers the whole series.

The 2 KiB step seen in the interruption batches (47,104 → 45,056 at the
end of attempt 4; 43,008 after attempt 3's deficit trial) appears here as a
transient that recovers within two cycles, not as accumulation. Thirty
cycles is the ticket's bound; nothing here speaks to hours of churn.

## Cut listeners

Cycle 6: both listeners ended at 210,816 and 213,120 of 480,000 frames
(about 4.4 s). Cycle 13: one listener ended at 321,408 (about 6.7 s). ffmpeg
reported "Stream ends prematurely" with exit 251, meaning the board closed
the socket. No UART was open, so the departure reason is not recorded; the
shape matches attempt 5's null-control cuts, which the UART there named
`media age exceeded` with a 3.98 s sender poll wait. This run had no tc tree
installed, which rules the tree out as the cause. Incidence here: 3 of 60
listener-runs over 373 s. The other 57 listener-runs decoded exactly
480,000 frames with no discontinuity. These cuts are ek5b's subject.

## Verdict code

The original run used a verdict that judged cycles by frame count alone and
folded any listener failure into one FAIL. The committed harness separates
`heap_passed` (the ticket's acceptance) from `listeners_clean`, lists cut
listeners as observations, and recomputes each listener's discontinuities
from the retained framehash files; `--rejudge` re-judges the retained run
without modifying it. The original `result.json` is kept beside
`rejudge.json`.

## Limits

One run of thirty cycles on one image, quiet LINE IN, wired receiver. Idle
slots in `/api/metrics` substitute for UART checked closes. No cause for the
cuts; no allocator or heap change was made or proposed. PSRAM only observed
as unchanged.
