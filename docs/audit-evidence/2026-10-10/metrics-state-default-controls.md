Doctor Biz: both matched 90 s ordinary acquisitions complete successfully, but both images incur actual encoder raw loss. Archived eb38 default generates **0.961316x** over eight complete windows; current bacdecb default generates **0.786533x**. Delivery was not exercised. This separates the image-associated rate difference from active media listeners without identifying its cause.

| Observation | Archived default | Current default |
|---|---|---|
| Exact copied ELF SHA256 | d88a65965ff456c7500df1ae05d5ac548ef4290b35524215bf3e27175378b991 | d78be640ac61d83383872e5b11a2e2e72d33bbb22cce6a64f58acf7888ed3d77 |
| Retrospective source context | eb38c65 | bacdecb |
| Features receipt | [] | [] |
| Natural requested duration / acquisition valid | 90 s / true | 90 s / true |
| Complete zero-listener windows | 8 | 8 |
| Generation ratio range | 0.953061–0.975117x | 0.783167–0.789022x |
| Weighted generation ratio | 0.961316x | 0.786533x |
| Encode wall per frame | 22.266481–22.336878 ms | 27.842751–27.973162 ms |
| Actual Overrun boundaries / final resets | 1 / 1 | 8 / 8 |
| Last Overrun event | 58.430542 s | 83.886259 s |
| Metrics / encoder free stack minimum | 1468 / 5188 bytes | 1436 / 5196 bytes |
| Supervisor free stack minimum | Unreported | 3044 bytes |

Each individual device report and paired JSON retain every complete window's producer bounds, actual cumulative baseline, frame delta, generation ratio, task runtime, encode-wall cost per frame, raw overruns/resets and capture brackets. Ordinary MP3 starts from count 0 after format switch; prior Flac counts are excluded. Complete cumulative endpoints anchor subsequent windows. Overrun retains the window baseline and never becomes a replacement anchor. No crossover phase schedule is required or invented.

The same six frozen helper archives have identical SHA256 values across both runs and match each acquisition's original helper hashes. Owned-boot selection and strict ordinary parser schemas replay from those archives without invoking crossover CLI or substituting mutable helper files. Exact images/build-identity copies match their acquisition-recorded hashes. Identity receipts are explicitly retrospective saved-image receipts, not new builds or clean-source attestations. Full device image attestation remains false. The transitive stream_format import is not archived; its checked current hash remains explicit in individual reports.

Both runs use the retained stereo-noise-420s.wav fixture, verified SHA256 `9311d9ca5dfe1a5f9269631afe22647834207f2b3239571474acc8ac15f3228d`. Routing readback outputs are identical: Headphone on, Line Out off, mono Master 0 dB/on and PCM 0 dB. Source-on after-reset and final-before-cleanup API readings both report −23.6 dBFS, active MP3/Medium, autoplay=false, recognition=false, Sonos stopped. Separate resets/playback starts do not establish identical PCM at ADC or encoder, and API level/routing does not prove signal fidelity.

No listener lifecycle record is observed and all sixteen complete windows report listeners_start/end=0. This bounds observations, not continuous absence of brief unobserved churn. Encode wall includes preemption; its per-frame increase is an observed measurement, not isolated codec CPU cost. Charged interrupts contribute to task runtime. The image-linked generation/cost association does not prove observer, cache, scheduler, codec or memory causation. No overhead tolerance or equivalence claim is established.

Complete capture counter baselines differ: queue_overflows 18 archived versus 15 current. Both retain software_dropped_frames 480, restarts 2, overflow_restarts 2, settling_discarded_frames 26400 and discontinuities 2; read_errors, supervisor_restarts and health_restart_requests remain 0. All eight complete capture reports per run are monotonic and nonoverlapping. Startup capture losses remain; their differing baseline is not a treatment delta. Stable capture counters do not clear actual encoder loss. Individual JSON reports retain missing startup and terminal brackets rather than substituting host times.

Both collectors finish naturally with raw byte counts matching arrival coverage, empty reader/cleanup errors and no unterminated tail. Wrappers pass with no acquisition/cleanup errors; supervision root return codes are 0, cleaned=true/errors=[]. Both source cleanups return 0 with empty aplay logs. Parent reports archived session 36182 and current session 80940 reaped with exit 0 and UART/aplay absent. These offline analyses perform no hardware/process action. Recorded original raw, arrival, image, build-identity, helper and result hashes remain unchanged.

Recommendation: preserve valid acquisition separately from both audio raw-loss failures and unexercised delivery. The current ordinary image remains slower under matched duration, source fixture, routing and observed level. Further root-cause work must account for the actual encode-wall-per-frame difference without treating it as proof of a cache or observer cause.
