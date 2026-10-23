# Capture restart failure handoff

Doctor Biz's authorized Kata 21tp follow-up. The design was prepared read-only; implementation starts with the isolated numeric handoff and real-module host tests. Parent integrates the two capture branches and metrics consumer after the encoder worker freezes audio_in.rs. No hardware or target checks are claimed by this plan.

## Decision

Use a dedicated, typed, fixed-size counter handoff, not an event queue. Two categories suffice: `InitialFlush` and `RecoveryRestart`. Each owns an `AtomicU32` cumulative wrapping failure count and `AtomicI32` sampled ESP error code. Expected payload is 16 bytes; measure actual target layout and alignment before claiming that size. Initialize counts and codes to zero with a const constructor. Zero count means no failure has been recorded, rather than error code zero meaning failure.

The capture owner records `error.code()` only on the existing two failed `rx.restart()` branches. Existing `firmware/src/i2s_capture.rs::read` already compares `error.code()` with `sys::ESP_ERR_TIMEOUT`, so numeric extraction is an existing project API. Store the numeric code, then increment the matching count using relaxed operations. No heap allocation, formatting, logger, mutex, channel, clock query or callback occurs in recording. The linked Xtensa increment uses a compare-and-swap retry loop; this is not a fixed instruction bound or no-retry guarantee. There is one capture writer; metrics only reads. No timestamp or new owner work is needed.

Counts and codes are independent atomic observations, explicitly NOT coherent tuples. An export can contain an earlier count with a later error code, or vice versa. Name the field `sampled_error_code`, not `error_code_at_count`. Relaxed ordering is sufficient because no non-atomic payload or resource ownership is published. This avoids a sequence protocol and its retries. If coherent event association becomes a requirement, this design is insufficient; that would be a new diagnostic requirement, not a reason to silently imply stronger semantics.

Counts preserve cumulative failure totals modulo 2^32. Intermediate error codes, number of failures per code, individual timestamps, cross-category order and individual warning lines are lost deliberately. No finite event backlog exists and exporter delay cannot fill anything. Counter wrap can hide exactly 2^32 failures between observations. A nonzero count proves failure; equal samples do not establish no failure under arbitrary delays/wrap. There is no dropped-event counter because there are no queued events. Do not call these aggregate reports complete event history.

## Existing behavior to preserve

`audio_in.rs::capture` performs initial restart before sampling the overflow baseline, publishing the capture task and setting STARTED. Failure currently logs and falls through. Recovery currently updates supervisor/overflow attempt counters, resets timing, invalidates publication, records discard/staging/gap, resets recognition and meter, clears pending level, restores the 500 ms settling budget and increments RESTARTS before calling restart. Failure again falls through to `rx.read`; existing read failures increment READ_ERRORS, reset timing and sleep 100 ms. Preserve all statements and their order; replace only the warning side effects.

`CaptureRx::restart` disables only when running. Disable failure returns with running unchanged; successful disable clears running, and enable failure leaves it false for a later attempt. Successful enable sets true. Do not edit this driver, add immediate retries, treat a failed restart as success, or change read-error sleep/recovery policy.

## Exact proposed implementation footprint

- Add `firmware/src/capture_failure_telemetry.rs`: two typed categories, the four-atomic handoff, small numeric snapshot, and production observation/export-decision methods. Follow the two ABOUTME comment convention.
- `firmware/src/lib.rs`: declare that module.
- `firmware/src/audio_in.rs`: one static handoff; replace the two warning calls using numeric result observation; add metrics-owned previous-count state and bounded aggregate export on the existing ten-second deadline. Coordinate this file with the encoder telemetry worker; do not edit concurrently.
- Add `core/tests/capture_failure_telemetry.rs`, importing the actual production module using the established `#[path = "../../firmware/src/..."]` convention from `capture_timing.rs` and `listener_telemetry.rs` tests.
- No changes to i2s_capture.rs, listener/encoder queues, capture timing/health, parsers, stack sizes or task priorities are required by this unit. Search diagnostic consumers before implementation; if any requires the old error text, explicitly update its contract rather than retaining an owner logger.

At the existing `now >= next_log` boundary, export at most one aggregate warning per changed category (two total), after `log_capture` returns, before queued listener/encoder exports. Export even when epoch/STARTED is absent so an initial flush failure remains observable. State advancement occurs on selecting the aggregate warning, not on UART delivery acknowledgment; the logger offers no such acknowledgment. Preserve existing capture line fields. Suggested separate warning fields: category, failures_total, failures_delta modulo 2^32, sampled_error_code, export_us, and `aggregate=true count_code_coherent=false`. Existing error phrases can remain readable prefixes, but this changes per-event text to delayed numeric summaries. Metrics lateness means the ten-second deadline is a scheduling limit, not a delivery guarantee. No new 100 ms polling warning loop.

## Production-seam TDD

First write tests against missing production APIs and observe failure. Expose a tiny production `observe_restart_result(kind, Result<(), i32>)` method; both actual restart branches pass `rx.restart().map_err(|error| error.code())` directly to it. This result is currently used only for warning/fallthrough, so consuming the numeric result changes no control flow. Tests call this exact method, not a simulated CaptureRx or cloned capture loop.

Test successful initial/recovery results leave all counters unchanged; each failed result changes only its category and retains signed full-width i32 codes; repeated distinct errors accumulate while only the sampled code survives. Test actual metrics-owned selection returns no initial warnings, at most two numeric summaries, unchanged counts suppress repeats, and later failures yield the correct wrapping delta. Test wrap via a private module unit test seed, not a public reset API. Do not assert count/code coherence under concurrency.

For stalled-export coverage, park a real consumer thread with a barrier after it copies a snapshot, record a bounded sequence through the actual result method, then release and assert cumulative totals and final codes. This tests the actual handoff while consumer progress stops; no mocked logger/driver and no time-based throughput claim. Test the real export decision state directly to establish coalescing and repeat suppression. The actual ten-second metrics placement and unchanged recovery branch order require diff/source review plus target inspection; host pure-function tests cannot prove I2S recovery. Parent-owned hardware acceptance remains necessary.

## Verification and risks

Run canonical `scripts/check` and existing default/probe feature host and target checks required by the encoder plan after integration. This design did not run checks. Review all capture-owner logger paths after replacement; these two calls are the scoped known remaining warnings, not proof no other owner path can panic or log.

Inspect linked default and encoder-probe builds: handoff symbol placement/size and static DRAM/BSS delta, record method atomics and helper calls, metrics persistent frame, log_capture frame, each separate failure/listener/encoder rendering path, and sequential return before exporter calls. `AtomicU32` target evidence exists in i2s_capture.rs comments and scripts/check-i2s-iram; it is not proof the new `AtomicI32` operations or linked path are free of unexpected helpers. Inspect actual generated target code. Preserve log_capture's inline(never) boundary and use a separate small rendering helper if inspection shows stacking with encoder rendering. Do not increase stacks automatically.

The encoder plan records metrics stack size 4096 and a measured minimum free 1156 in a failed delivery trial, with prior static frames metrics208/log_capture528/listener exporter about1008 pending artifact confirmation. These are evidence and constraints, not an available-budget guarantee. Host sizeof or arithmetic on old frames is not target evidence. Parent live acceptance must collect actual metrics/capture/encoder/listener stack headroom and internal/PSRAM free/largest/minima together with capture/reset/raw-loss and telemetry-loss gates, under matched image/settings/source controls.

Severity: diagnostic detail/order loss is intentional and must be stated in reports. Unknown target stack/atomic cost and untested live recovery are shipping blockers until measured. This design does not fix capture restart faults, listener delivery, or diagnostic perturbation; those remain open independent gates.

## Execution ownership

Task 1 worker owns only the new capture_failure_telemetry.rs and its new core
integration test. It must not edit audio_in.rs or lib.rs while the encoder worker
owns them. Parent reviews the module and performs or delegates the small wiring
step after encoder extraction, then checks the actual combined target. Record
host red/green results separately from unimplemented firmware wiring.

## Reviewed integration checkpoint

Parent wired both actual restart results and the metrics consumer. Five actual-module tests pass. Both target variants show a 16-byte BSS handoff, direct word stores/loads and an inlined CAS increment; no allocator or logger call occurs inside the numeric updates. Existing restart order and fallthrough remain unchanged. Canonical checks and independent source review pass. Runtime driver-failure injection and stack acceptance remain open; see the encoder-telemetry-build receipt for exact images and limits.
