# Encoder probe device trial

Doctor Biz, `scripts/encoder-probe-check` records one diagnostic ELF's automatic OFF1/ON/OFF2 sequence with no media listeners. It does not flash, change settings/mixer/volume, invoke Sonos Play, or roll back to FLAC. Parent must identify the exact flashed ELF and release UART and the analog source first.

```sh
scripts/encoder-probe-check 192.168.23.217 \
  --elf EXACT_FLASHED.elf --fixture APPROVED_NOISE.wav \
  --out NEW_PRIVATE_DIRECTORY --budget-seconds 360
```

The command requires stopped MP3/med, autoplay false and `/api/state`'s `shazam_enabled=false`. It reads Headphone on / Line Out off and retains Master/PCM gain readbacks without writing them. It copies/hashes the operator's ELF; this is provenance, not device attestation. The fixture must be approved 48 kHz stereo 16-bit PCM WAV, never looped. Record the generator's adjacent recipe alongside the run when available.

Source starts and settles for 12 seconds before the existing DTR false / RTS true / 100 ms / RTS false reset. UART runs before reset. The single collection deadline is source launch + 12 + budget; boot, source monitoring, state reads and automatic restarts all consume that same budget. Raw loss never extends it or triggers a host retry. Failure can mean no complete valid sequence exists within the budget.

After success or failure, while the source remains owned, the runner attempts final routing and both state GETs. One shared final allowance lasts at most 16 seconds: four routing reads capped at one second each, two GETs capped at six seconds each, further clipped to the absolute final deadline. Thus the minimum WAV length is `12 + budget + 16`: **388 seconds for a 360-second budget**. The existing 300-second file is too short; use the parent-generated approved 420-second file. Final evidence requests can fail; they preserve the original failure and make completion false. Unexpected source exit also fails. SIGINT/SIGTERM reach cleanup; if interrupted with reset asserted, cleanup first releases RTS. Source teardown has bounded terminate/kill waits, and UART has a bounded join/close.

Each fresh private directory retains original UART bytes (including partial tails), byte-offset/chunk-arrival records, events, source liveness every 200 ms, aplay output, full raw/decoded status/error receipts, ELF/hash, WAV metadata/hash and `result.json`. `errors.jsonl` is append-only; later cleanup errors never replace the initiating error. Raw arrival stamps describe reads, so analyzer uses encounter order to bracket capture coverage. All sequences, including invalidated/incomplete ones, survive analysis.

The analyzer requires one sequence's 24 consecutive paired report windows: two discarded plus six retained in each phase. The last discarded report supplies the first retained cumulative frame/reset predecessor; the first sequence delta stays unknown. It rejects missing/repeated labels, raw loss/reset, bad runtime/accounting, storage omissions, capture faults/progress gaps and observed listener joins/leaves. COMPLETE reports after the 24-window schedule remain evidence and still undergo accounting/loss checks. Storage validity checks actual emitted ranges, chunk/output reservation, PCM and live Shine pointer/size receipts independently from phase and capture validity. Addresses and allocator policy text do not prove cache behavior or physical memory classification.

Phase results use sums across six complete retained windows: generated audio divided by elapsed wall, codec wall divided by exact cumulative frame deltas, task runtime and wall totals. ON-only attempt spans remain nested in raw wait; checked remainders/residuals must match firmware. Capture coverage endpoints are saved separately. A valid slow OFF phase is a performance finding. No overhead tolerance, historical-image attribution or release acceptance is invented; repeated OFF controls and parent judgment establish those.

Host verification runs through `scripts/check-harnesses` and canonical `scripts/check`. Synthetic probe unit rows are labelled synthetic. Durable sanitized state and production UART slices under `scripts/fixtures/encoder-probe/` include source-file/ELF hashes; they prove existing metric parsing, not probe HIL. The parent must run the actual diagnostic command and review complete raw evidence before claiming device acceptance. Source liveness/read-only routing do not prove sample delivery at the ADC, and zero listener endpoints cannot exclude unlogged brief churn.
