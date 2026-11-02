# Close reaper on hardware (ttms, 7fe1; 2026-10-19)

The close reaper (`f5f0bb6`, image `2ed701de…`) releases a listener's
slot at hand-off, before the socket's checked close, and the board was
made to show it: a reader stopped in place (`freeze-N` in
`scripts/network-fault`, SIGSTOP on one decoder) is departed by the board
at the 24 s live deadline, its close then lingers about 2.3 s, and a
replacement started at the departure line joined inside that window in
every run that reached it: 1.06 to 1.93 s after `released_us` and 0.46 to
1.37 s before the Closed line (freeze-1 and freeze-2 in two runs, plus a
third freeze-2 re-verdicted after a harness pairing fix). The same trial
on the previous image `613e7621…` gets a 503 for the replacement and a
Closed line without `released_us`, so the verdict discriminates. Null
trials, the five-player MP3 zoo and the reaper's stack headroom
(2,272 to 2,280 B free of 4,096) are unchanged or within bounds. What
this run also found, on both images, is a separate problem: a frozen
peer degrades the surviving listener and the board's HTTP server (ticket
b814). Rows and hashes are in `close-reaper.json`.

## Acceptance (plan #1) and what was measured

| Acceptance | Measured |
| --- | --- |
| hand-off, bounded queue, failure path, event ordering (host tests) | `core/tests/close_reaper.rs`, the loom listener model, `scripts/check` 27 stages (`f5f0bb6`, `ea39416`) |
| replacement admitted before the departed socket's close line | 4 passing freeze trials: release→join 1,058,796 to 1,925,376 µs, join→close 458,859 to 1,367,576 µs; departure→release 172 to 514 µs |
| `released_us` < Joined < Closed | holds in every passing trial by the verdict's rule |
| `reaper_stack_free_bytes` read | 2,272 to 2,280 B on every Closed line |
| client zoo unchanged | ffmpeg, mpv, mpg123, sox, curl on MP3: no reconnects, still receiving after a 4 s stall, per-player skips identical to 2026-10-18 (0, 1, 1, 1, 1); FLAC rows are 404s because the board's active format is MP3, as on 2026-10-18 |

## Runs

- `oct19-reaper-capture` null-1 pass (19:02 UTC); `oct19-reaper-freeze`
  freeze-1 and freeze-2 pass (19:09); `oct19-reaper-freeze-2`, `-3`, `-4`
  freeze-2 at the 75 s trial length: two missed the departure (59 s and
  more than 58 s after the freeze) and one paired the decoders backwards,
  passing on re-verdict; `oct19-reaper-freeze-5` freeze-1 and freeze-2
  pass at 120 s (20:4x); `oct19-reaper-null2` null-2 pass (21:4x);
  `oct19-prereaper-freeze-a`, `-b` on `613e7621…`: 503 and no release
  stamp, as expected; `oct19-zoo-1` MP3 pass.
- Every run attested through `/api/metrics` (`build_elf_sha256`,
  `boot_token` in the JSON); the pre-reaper image was rebuilt
  bit-identical from `3fb6667` for the A/B and the reaper image reflashed
  from HEAD afterwards.

## Harness lessons

- A peer-reset close takes 0 ms and a `tc` stall's departures close
  before the path returns, so neither can show early admission; only a
  reader that stops reading gives a lingering close.
- The receiver's kernel absorbs 29 to 35 s of stream after the stop, so
  the deadline fires 53 to 66 s after the freeze; the trial is 120 s.
- Two decoders join within 40 ms in either order; the frozen decoder is
  paired with the peer that stopped reading, the rest by join order, and
  a retained run is re-paired on `--reverdict`.

## Limits

- Six passing freeze trials across runs is a small sample for the timing
  spread; the window is about 2.3 s wide and the join landed 0.46 s from
  its end once.
- b814 is open: survivor skips in 3 of 5 reaper runs and 0 of 2
  pre-reaper runs, `/api/metrics` failures on both images with a frozen
  peer present, clean null-2 on the reaper image right after.
- MP3 only, quiet LINE IN, one board, office network.
