# Live-stream robustness for any consumer (2026-10-16 to 2026-10-18)

The board no longer hangs up on a slow reader. Under live-radio semantics
(commits `aae0089`, `98e54a0`, `3fb6667`; image `613e7621…` built from
`3fb6667`), a compressed listener that falls behind skips to the newest
complete frame and keeps its socket; termination is for dead sockets,
lease changes, capture resets and Stop. On the real board with
receiver-side `tc` stalls, every stall from 0.5 s to 15 s with one and two
MP3 listeners kept its socket with exact decoded frames and zero
departures (19 of 19 trials in batch 2 under the corrected verdict), 30 s
stalls ended with one checked `stopped reading` per listener past the
24 s live budget and a fresh admission, both sustained deficits kept
their sockets skipping, and five real players — ffmpeg, mpv, mpg123, sox,
curl — each played through a 4 s stall with zero reconnects. Skips are
audible gaps the length of the consumer's own stall; this is continuity,
not lossless delivery. This does not cover FLAC (the board is deployed
on MP3), the board's own Wi-Fi hop, or the Sonos path. Hashes, per-trial
rows and the retained original verdicts are in
`live-stream-robustness.json`.

## What changed in the product

- Skip at `Overwritten`, at a pre-write stale frame, and after a late
  complete write, at frame boundaries under the ring lock
  (`FlacRing::catch_up`, `core/src/flac_ring.rs`;
  `firmware/src/stream_pipeline.rs`). FLAC numbering continues per
  listener; MP3 frames are independent by construction.
- Live write deadline is twice the ring depth (24.0 s MP3, 24.06 s FLAC,
  4.1 s FLAC with recognition): batch 1 showed an 8 s blackhole ending a
  listener with a 12,048,858 µs poll at the one-ring-depth deadline,
  because the board's TCP retransmit backoff adds several seconds after
  the path returns. Exact readers (`?exact=1`, WAV) keep the 8 s limit.
- Counters `skips`, `frames_skipped`, `last_skip_us` on the UART departure
  line, in `/api/metrics` listener slots and in
  `last_listener_departure.stats`; the metrics body cap is 4608 on both
  sides. Timeout receipts label the budget the listener really had.
- Response head: `Cache-Control: no-cache, no-store`, `Accept-Ranges:
  none`, `icy-name`, `icy-description`; `HEAD` answered with the head and
  no body before any slot, lease or close policy is touched.

## Batch 1 (`oct16-interruption-1`, image `217cfbfa…`, one-ring-depth deadline)

17 trials, 1037 s, qdisc restored. Original verdict 15/17. Every stall
through 8 s with one listener survived with exact frames; both 15 s
stalls ended on the 12 s deadline, correct for that image's contract;
`stall-8-2` lost one of two listeners on the 12.05 s poll described
above — the product observation that set the 24 s budget, and the one
genuine failure of the batch. `deficit-0.5-1` was a verdict defect (board
skipped 7 times, 1261 frames; the rate limit starved all 96 metrics
polls). Re-judged under the final verdict code, which expects 15 s stalls
to survive, the batch reads 14/17: the two 15 s departures are failures
of the old deadline, not of the board's behavior under it, and the
deficit becomes a pass.

## Batch 2 (`oct18-interruption-2`, image `613e7621…`, 24 s deadline)

19 trials, 1184 s, qdisc restored, boot `0eb82152…`. Original verdict
18/19; re-judged under the final verdict code: 19/19, `result.json`
unchanged. Stalls held: 0.546/0.565, 1.076/1.049, 2.593/2.544,
4.041/4.079, 8.091/8.044, 15.053/15.054 s (one/two listeners); every
listener completed with exact frames (1,944,000 to 2,640,000 per
listener) and zero discontinuities; board skips per listener 0 (≤4 s),
1 (8 s and 15 s), 386 to 1763 frames skipped. `stall-30-{1,2}`: one
checked `stopped reading` per listener at the 24 s budget, fresh
admission within 10 s. `deficit-0.8-1`: listener kept its socket, 3 board
skips (502 frames), decoder exact; the original FAIL was the harness
mis-filing the board's late `ConnectionReset` for our own close and
expecting the decoder to outlive the injection although `ffmpeg -t`
counts audio seconds. `deficit-0.5-1`: kept its socket, 7 skips (1237
frames), metrics starved (60 timeouts), judged on the attributed
counters. Delayed WAV reader: ended on its own deadline, MP3 listener
exact. Internal largest free block within the null tolerance throughout.

## Client zoo (`oct18-zoo-1`, same image)

`scripts/client-zoo`, MP3 only, 30 s per player, 10 s steady then a 4 s
receiver stall, headless null outputs. ffmpeg, mpv, mpg123, sox and curl:
one board join each, zero reconnects, still receiving at the end; the
board skipped once for each except ffmpeg, whose buffer absorbed the
stall. 193.6 s, qdisc restored.

## Verdict code

Batch 1 ran with verdicts that read the post-trial idle sample; batch 2
with verdicts that mis-filed late resets and expected decoders to outlive
deficits. Both `result.json` files are kept unchanged; `--reverdict
--policy live` under `19d7608` produced the `reverdict.json` beside each
(16/17 and 19/19). The three October exact-policy batches re-judge
unchanged under the same code.

## Limits

MP3 as deployed; FLAC's skip path is covered by host tests with real
decoders, not by hardware. Receiver-side shaping on a wired host: this is
recovery from receiver-visible loss and rate limits, not RF, AP queueing
or the board's own Wi-Fi hop (ek5b). Deficit steadiness is unjudged where
the rate limit starved the metrics path. A live listener whose supply is
persistently more than 4 s stale drops frame after frame and never
terminates; that is the contract and reads as a skip storm in metrics.
Early slot release during close linger is not yet built.
