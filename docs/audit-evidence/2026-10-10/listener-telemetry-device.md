# Listener-only telemetry device evidence

Doctor Biz: both bounded device trials failed their overall gates. Keep full 21tp open. Immutable ELF/source SHA256 values and per-file evidence hashes are in the paired JSON; device attestation remains false.

Trial 2 admitted two listeners; both fell a ring behind after about 15 seconds. Producer departures were 37.540017/37.922106 s and checked closes 37.542583/37.923621 s. Three receipts report zero drops and certify both closes. Delivery reached only 22.2%/20.7% of real time. The retained ffmpeg premature-EOF failure stopped the wrapper before slow testing; timeout and reuse are unmeasured in this trial.

Strict parsing includes four complete capture reports through 1999680 frames; the unfinished 938-byte final UART fragment is excluded. Final decoded prefixes contain 150912/165888 samples (3.144/3.456 s), distinct from the stale summary's 149760 samples for the first listener. Encoder progress in three successor intervals spans 0.99831–1.00197x. This establishes measured generation progress, not end-to-end continuity.

Trial 3 exercised an actual frame-write timeout: event 37.348091 s, elapsed 8064 ms against limit 8016 ms. The slow listener departed stopped-reading at 37.348408 s and an observed close line records event 39.663543 s / 2314 ms close duration. A later third admission at 50.473837 s and ten decoded probe frames (11520 samples / 0.24 s) support usable admission after the earlier departure. The helper records slot_reclaimed=true but overall failure from the fast listener's premature EOF at 574080 bytes. The fast listener's observed ring-behind departure occurred at 52.372430 s.

Trial 3 receipts have sticky loss: dropped=1, loss_occurred=true. Production acceptance therefore rejects all strict close certification for that log; observed lines and probe success cannot clear loss or certify the full cleanup sequence. No evidence identifies the drop's publication cause. Five complete capture reports and six encoder reports remain available; five successor encoder intervals span 0.99634–1.00341x, without reported raw overruns/resets.

Both trials begin with 15 capture queue overflows, 480 software-dropped frames, two overflow restarts/discontinuities and 26400 settling-discarded frames. All nine fault/settling counters remain unchanged across each trial's complete reports, including available failure tail. This does not clear startup or establish sample continuity.

Minimum observed stack headroom is 420 bytes for audio-metrics in trial 2 and 580 bytes in trial 3, from a 4096-byte allocation; listener minima are 1676/1664 bytes and encoder 5300/5308 bytes. Recommend resolving the metrics margin and measured exporter loss before adding encoder-export work or declaring this slice accepted. No crash is not a safety proof.

Observed heap minima across trial 2/trial 3: internal free 67067/61927, internal largest 61440/55296, PSRAM free 2770408/2761764, PSRAM largest 2359296/2359296, internal per-region minimum sums 56143/56615, PSRAM minimum sums 356556/363584 bytes. Largest blocks are snapshots; minimum sums combine per-region lows and do not authorize additional RAM.

Both wrapper records retain failed verdicts and no cleanup errors. Trial 3 has no acquisition error and slow return code 1. Repeated MQTT TLS connection failures appear in UART; they do not establish the cause of listener failures. No watchdog, panic or allocation-failure line was observed. All original evidence and trial 1's preflight failure remain untouched.
