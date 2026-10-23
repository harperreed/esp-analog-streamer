# cn4m channel diagnostics: bounded implementation design

Design baseline: `28575fcacf97f6a114bdb0c7229ddaada1edf99a` on
`wip/kata-capture-protection`. The implementation follows this contract;
the outcome below records measured limits, including a missed CPU target.

Extend the existing `StereoLevelMeter` and its existing nonblocking publication
into `SongStore`. Publish the channel detail through `/api/state`; its handler
already owns this store and the browser already polls it. Leave `/api/metrics`
as the existing operational/identity evidence source. A second cache or DSP task
would add synchronization without improving this feature.

## Measurement contract

The meter consumes the existing sign-extended signed 24-bit L/R samples after
settling. Channel labels describe current software order; they do not certify
physical jack routing. No sample copies, gain, channel swap, or new audio source.

Keep four completed 250 ms buckets and the filling bucket. Each channel bucket
adds an absolute sample peak and `u32` digital-rail count to the existing count,
signed sum, and signed-square sum. Use a stereo-only bucket type so the unrelated
mono `LevelMeter` does not grow. Integer sums remain within the existing
131,071-frame constructor bound. Calculate floats only at completed intervals.

Return one fixed-size `StereoReading` from `push`, replacing its scalar return;
migrate its existing callers/tests together, without a second metering API.
Proposed fields are `level_dbfs: f32`, `window_frames: u32`, and
`channels: [ChannelReading; 2]`. `ChannelReading` contains
`rms_dbfs: f32`, `peak_normalized: f32`, `digital_rail_samples: u32`, and
`dc_offset_normalized: f32`. Derive Copy/Clone/Debug/PartialEq as needed.

| Field | Meaning |
| --- | --- |
| `rms_dbfs` | DC-removed RMS: `10 log10(max(E[x²] - E[x]², 0) / 8388608²)`, using each channel's own mean. Preserve the current sub-one-sample-unit variance silence rule and -120 dBFS floor. This is AC RMS, not total RMS including DC. |
| `peak_normalized` | `max(abs(x)) / 8388608`, before DC removal, range 0 through 1. Negative rail reaches 1; positive rail reaches 8388607/8388608. Compute absolute value safely from the widened sample. |
| `digital_rail_samples` | Number of samples equal to -8388608 or +8388607 in this channel's current measurement window. A rolling-window count, not a lifetime counter or proof of analog clipping. |
| `dc_offset_normalized` | Signed mean sample value divided by 8388608. Range -1 through 8388607/8388608; retains fractional sample means. |
| `window_frames` | Actual completed frames represented per channel: 12,000/24,000/36,000/48,000 during 48 kHz startup, then 48,000. Partial filling buckets and half frames are excluded. |
| `level_dbfs` | Existing combined AC energy: average the two channel variances, then convert to dBFS. Do not average channel dB values. Preserve the existing rounded policy/page value and opposite-polarity behavior. |

Peaks and rail counts aggregate only the four completed buckets, so expired
peaks/counts disappear together with expired energy. Preserve arbitrary chunk
boundaries, half-frame retention, reset semantics, and the last completed reading
when one push crosses several intervals. A full-scale sine's RMS is about
-3.01 dBFS; its sample peak approaches 1. These are distinct quantities.

## Store, age, and reset semantics

Replace the store's separate scalar/time measurement storage with one optional
fixed-size stamped reading. Its single combined value is rounded to 0.1 dB on
publication, just as today. `level_dbfs()` and the existing top-level JSON field
continue returning that combined value, with the existing silence startup
fallback. `level_sample()` keeps its existing return type and original
measurement Instant; Sonos keeps its existing two-second freshness policy.
Recognition continues reading the same combined value.

Keep the existing reset behavior: reset the local meter, including its partial
interval/half frame, and clear `pending_level`. The next reading contains only
post-reset samples. Retain the store's last completed measurement with its
original timestamp; the API explicitly presents it as historical. Do not add
an atomic epoch, a reset-notification queue, or another scalar fallback store.
Immediate cross-thread invalidation would be necessary for a claim of current
capture continuity; this API makes no such claim. This is the selected reset
contract for the diagnostic view. Tests must cover reset and stale readings
without changing playback policy.

`set_level` becomes one internal `set_measurement(reading, measured_at)`
operation; migrate its source callers. Do not keep a scalar setter alongside it
that could create channel-less measurements or fabricate freshness. Keep one
newest pending stamped reading, replacing it on each completed interval; try
the existing store mutex without blocking. Retried publication preserves the
original Instant. Never timestamp a retry or a reset as a new measurement.
Read errors and settling naturally age the last store reading. This feature
does not change recognition, Sonos freshness, or capture recovery policy.

Add `last_measurement: null | { age_ms, window_frames, channels: { left, right } }`
to the existing JSON state. It is null before the first reading; after a reset
the old historical reading remains available until replaced. Age is elapsed
monotonic time since capture finished processing the push that returned the
completed interval, evaluated at serialization. It is not time since HTTP or
store publication and is not the ADC sample timestamp. The measurement excludes
partial trailing samples even though that push's processing includes them.
Reject a future Instant as unavailable instead of saturating it to a fabricated
zero age. Never export a `fresh`, `current`, or continuity-valid flag. Consumers
can distinguish recent and aged history, but age alone cannot prove continuity.
Sonos keeps its existing two-second threshold on the original Instant.

Keep existing fields/types and browser rendering unchanged.
All JSON allocation and formatting remain in the HTTP/store owner, outside the
capture loop. No new endpoint, listener, task, codec behavior, or settings write.

## Verification and acceptance boundary

Use TDD for meter/store changes. Extend current tests with separate channel
amplitudes, either silent channel, opposite polarity, unequal DC plus AC,
constant DC at either rail, both signed extrema, odd chunks, multiple intervals
in one push, startup denominators, and bucket peak/rail expiration. Check exact
rail counts and normalized DC/peak, with bounded floating tolerances for RMS.

Add a real production-component integration test: decode I2S slot bytes through
`samples_from_i2s_slots`, feed the actual meter, publish through the actual store,
and parse its serialized JSON. Exercise delayed publication with an old Instant,
meter reset while the store mutex is held, the initial 250 ms after reset,
and equal readings with different receipt times. Review the unchanged firmware
branch that discards pending publication; a test-owned Option cannot verify it.
Check combined `level_sample()` and JSON agree, no stale reading becomes fresh,
and reset/expiry counts use actual production types. After reset, the historical
snapshot retains its age and values until the first new completed interval;
neither reset nor retry refreshes it. No mocked meter/store.
Run the canonical `scripts/check`, including its existing firmware checks.

Estimate 120-180 production lines plus 180-260 test lines. Expected bucket RAM
growth is 80 bytes (ten stereo channel buckets, eight added bytes each), plus
fixed readings and stamps, with no additional heap allocation. Measure actual
target `size_of`/linked stack use; use <= 512 additional resident bytes and zero
per-push allocations as proposed bounds. ABI padding and target Instant
layout must be measured rather than copied from host sizes.

Parent-owned board check: retain baseline/candidate build and boot identities,
take matched quiet-input windows with the same stopped MP3/Medium state and
recognition/autoplay off, and poll `/api/state` and existing metrics without
changing desktop audio, ACLs, mixers, source, speaker levels, or codec settings.
Use existing meter/store work maxima, capture-task runtime, heap and stack
watermarks and fault counters. Suggested bounds are <= one percentage point
added capture-core CPU, <= 512 bytes added resident RAM, no new allocation in
capture, and no added capture faults. A 5 ms DMA block is the existing service
interval; any measured meter/store duration approaching it fails the bounded
work gate. Maxima include preemption, and paired firmware placement differs:
report these attribution limits, actual values and baseline noise. If CPU delta
cannot be resolved, record it as unmeasured; do not replace it with a host speed
claim. A quiet run checks plumbing and overhead, not loaded reliability.

cn4m stays open after software/quiet acceptance. Physical jack voltage,
input-to-PCM gain, analog headroom/distortion/noise and channel-routing location
still need Doctor Biz's measurement help and a known driven signal. Existing
tone evidence shows end-to-end reversal without locating it. Preserve that
finding; no compensating gain/swap or fidelity claim from WAV/FLAC agreement.

## Implementation outcome

The API and nonblocking publication are implemented. Fourteen real-component
diagnostic tests cover the contract, including the largest permitted sample rate.
The original behavioral negative control failed all thirteen initial tests.
All 27 canonical stages pass in `run.qBh9KzSn`; independent source review found
no actionable issues. The final ordinary ELF exactly matches the first measured
candidate, `1d2105b115ca0423097ff742193890d8329c0e366d6f52076d6655f2c25c9eb3`.

Target compilation bounds growth of the measured meter/store/pending objects
plus one transient reading at 190 bytes. That does not measure total call-stack
depth. Thirty live API
snapshots pass with ages 5–240 ms and unchanged settings. Quiet capture remained
fault-free, but CPU increased from 9.883722% to 11.067786% of one core. The initial
one-point target was missed. Narrowing the stereo count to u32 made observed CPU
worse at 11.901087%; that change is removed, and both measurements are retained.

Ruling: retain the smaller, original implementation for iteration, with its
measured cost explicit. The one-point target was this design's proposal; cn4m
requires measured overhead and capture isolation, without that numeric limit.
This is not a passing result for the proposed CPU target. The cost of the
decision is less scheduling headroom, and the remaining release load/soak gates
must still assess it. Do not infer instruction cost from a counter's width.

Physical calibration remains unavailable while Doctor Biz cannot access the
workstation. Keep cn4m open. See the channel-diagnostics audit evidence for the
bounded streaming check and final board state.
