# Representative WAV input while FLAC is selected

Doctor Biz, this route is feasible without Sonos Play or target instrumentation. The smallest adaptation is one expected-format parameter on the existing scratch observation harness. Parent owns temporary format selection, restoration, both exact images and all hardware actions. This document is a read-only design result; no tests, builds, live HTTP, settings, playback or hardware actions ran.

## Implementation checkpoint

The shared validator extension and restricted scratch flag now pass independent
review and fresh-eyes. Focused checks passed: 21 module, 37 probe, 15 WAV and 18
crossover tests. Parent ran all 27 canonical stages with complete receipts and
equal boundary snapshots at target/check-logs/run.OAqOjceh. An initial worker
invocation used bare Python and lacked NumPy; the same module tests passed in the
canonical locked uv environment without dependency changes. No live format
change or FLAC input measurement has run at this checkpoint.

## Actual activation semantics

`core/src/sonos_web.rs` defines the real JSON command `{"action":"format","format":"flac"}`. `OutputFormat` serializes lowercase. `firmware/src/web.rs` accepts it through POST /api/sonos and returns 202 when queued; this acknowledges submission, not active codec completion.

`firmware/src/sonos.rs`, Command::SetFormat, first saves the new config and, when the controller state is Stopped, calls stream::select_format(format,false,token). That is an active encoder-switch request while stopped. In stream.rs the non-forced path calls Runtime::switch with a three-second switch deadline. Runtime::commit rechecks outstanding compressed listeners and can return Ok(false) without switching. SetFormat does not inspect that bool, so neither saved config nor 202 proves activation. GET /api/sonos obtains active_format from the actual stream runtime in Handle::snapshot; poll this independently from config.output_format.

No endpoint-specific prepare command is needed or exposed by the inspected handlers. A WAV request reads the independent raw ring and does not activate the selected compressed codec. Sonos Play would force encoder selection, wait for capacity, resolve/connect the chosen group and load playback; it introduces an unnecessary external playback side effect. Do not use Play for this acquisition. Stop cleans only the board's owned Sonos session and changes controller state; it is not a codec switch. Require the initial state already stopped rather than using Stop to interrupt unrelated playback.

The source inspected is current source, not a claim that its text attests the archived linked image. Each actual image must independently demonstrate stopped state, saved FLAC and active FLAC before acquisition and after the owned reset. A timeout/mismatch fails the attempt; do not rescue it with Play.

## Important saved-config correction

The preceding after-wav-failure-next.md says not to change saved configuration. The actual supported format API writes the Sonos NVS config before attempting the runtime switch; there is no inspected transient-format API. Thus this experiment requires a temporary persisted format change and explicit restoration. Parent's subsequent authorization for format/config testing and restoration supplies that scope. Keep the saved before/after receipts and report the temporary write plainly. Do not describe the selection as unsaved or invent an endpoint to make it so.

## Minimal harness change proposed

Add `--expected-format {mp3,flac}`, default mp3. Permit flac only with `--scenario observe --observe-seconds 90 --capture-wav-input`. Extend the shared scripts/encoder_probe_check.py::require_state(sonos, app, expected_format='mp3') as the single validation source. Reject expected_format outside mp3/flac; preserve every existing condition: stopped, active_format and config.output_format equal the expected format, buffer_preset med, autoplay false, and shazam_enabled false. The scratch harness passes its validated argument at the existing snapshot seam. No duplicate local validator. All existing two-argument callers keep exact default MP3 behavior. Record expected_format in acquisition and input workload metadata.

Keep the entire saved config stable within each harness run using the current before/final comparison; parent selects FLAC before entering the harness and restores MP3 after it exits. The harness never owns settings mutations or automatic format rollback. A failed capture should not silently select another codec. Before changes, freeze the current helper/parser/test bytes and hash them; future acquisitions continue archiving exact active helpers.

The existing WAV byte/header checks and producer-stamped capture/listener checks remain strict and unchanged. The ordinary encoder metric parser requires numeric block_samples but does not impose MP3's 1152 count, so it can retain FLAC telemetry without pretending to apply MP3 throughput formulas. The input validator independently retains encoder cursor loss; that alone does not establish WAV loss. Do not relax the 8-second target or failed-departure/capture-counter gates. All such runs remain workload=wav_input_capture and explicitly ineligible for the crossover oracle.

Estimated implementation delta: about 15–25 helper lines plus the small shared-validator extension, targeted tests and metadata. Parent will authorize tracked-file implementation after the currently running frozen canonical check and commit. No second acquisition harness, new transport mode, generalized mode framework, firmware edit or new analyzer oracle.

## Parent-owned sequence for each exact image

1. Retain the previous failed prefixes and receipts unchanged. Verify exact image/build/fixture identities. Current default is d78be640ac61d83383872e5b11a2e2e72d33bbb22cce6a64f58acf7888ed3d77; archived default is d88a65965ff456c7500df1ae05d5ac548ef4290b35524215bf3e27175378b991. Verify positive cleanup from every previous owned helper before reset/flash. No hardware identity is inferred from a host ELF alone.
2. Read and retain /api/sonos and /api/state. Require stopped MP3/Medium, autoplay false, recognition false and the expected saved configuration. Preserve routing and source fixture conventions. Reject unexpected state instead of changing unrelated settings.
3. Parent submits POST /api/sonos with Content-Type application/json and body {"action":"format","format":"flac"}. Preserve response and bounded polling receipts. Require config.output_format=flac AND active_format=flac AND state=stopped, with Medium/autoplay/recognition and all unrelated config unchanged. A 202 is not success. Do not issue Play. If selection cannot activate without additional workload, reject this route. Any attempted FLAC POST, including timeout or a lost response with uncertain delivery, triggers parent step 6 restoration even if activation, acquisition startup or capture fails. Saving may precede failure; restoration is not conditional on receiving 202.
4. Run the same existing 90-second observe/WAV harness with --expected-format flac, the same 420-second driven noise fixture and routing, and a fresh output directory. Persist the FLAC treatment in the manifest. The owned reset reloads saved FLAC; require the existing post-reset state snapshot to confirm active FLAC before the scheduled WAV request. No runtime switch occurs during capture. Keep the source lead-in 12 seconds, WAV launch at or after reset-release+30seconds, eight-second exact capture, 20-second whole deadline, 90-second UART tail and final receipts identical on both images. No compressed listener or recognition workload is opened.
5. Require real exact bytes, source/routing/state/provenance receipts, positive child cleanup, matching WAV departure/checked close/later loss-free export, and conservative monotonic producer capture brackets. Missing evidence or known capture loss leaves input_fixture_valid false; no padding, stitching, shortening or synthetic replacement.
6. After the harness has stopped/reaped its WAV helper, UART collector and source and retained its final FLAC receipts, parent explicitly restores with {"action":"format","format":"mp3"}. Poll until actual active_format and saved output_format are MP3, state stopped, Medium/autoplay/recognition and unrelated config match the saved baseline. Merely restoring saved config is insufficient. If restoration fails, retain the actual state, flag cleanup failure and stop further trials; do not claim restored or start a Sonos playback recovery. This restoration is explicit session cleanup, not a hidden FLAC fallback.
7. Repeat under the other exact image using the same convention. Only two complete strict captures authorize the planned host distribution/work-count comparison. If either FLAC capture fails, stop this input-capture route as after-wav-failure-next.md requires; no further transport variants.

## Required host verification before implementation handoff

- Actual argument parser accepts FLAC only for the exact 90-second WAV mode and defaults to MP3; other scenarios retain existing semantics.
- Real saved JSON snapshot fixtures require both selected and active FLAC, stopped state, Medium/autoplay/recognition and stable unrelated config. Reject selected FLAC with active MP3 and the reverse, playing state, or changed config. Default MP3 fixtures preserve the existing conditions.
- Existing real fragmented socket, exact literal/zero samples, partial failure, timeout, listener/capture chronology and export-loss tests remain passing; add a complete strict FLAC-format ordinary telemetry fixture if needed to prove no accidental MP3 block-size assumption.
- Existing explicit WAV workload crossover rejection remains. No host test claims actual target mode activation or source/UART concurrency. Parent verifies those from actual snapshots/ROM-framed telemetry and hardware cleanup receipts.

## Interpretation and limits

A successful capture supplies representative ADC samples under the explicitly different FLAC workload. It is not the exact MP3 consumed cursor or a retest of no-listener MP3 rate. FLAC changes compressed encoder CPU/allocation/priority competition; a clean FLAC acquisition cannot exclude input/capture effects appearing only under MP3. Analog noise remains stochastic: compare distributions and host Shine work mixes, not byte equality across captures. Preserve source-on levels, routing and aplay stderr; source liveness is not an underrun-free or physical-voltage certificate. Host replay does not recreate target scheduling, PSRAM/cache behavior or reset history. Existing failed MP3/WAV/delivery outcomes remain failed.


## Pre-implementation freeze

Doctor Biz, archived exact current relevant files under metrics-state/before-flac-input/ before any implementation. SHA256:

- encoder_probe_check.py: 20f9722db3f545bb00e82561899f47fa8ae8ae2c26ba90cf3a484247750330d6
- test_harness_modules.py: 0a0fb1996884ba90259cbba7d2b8bca841a2f3d78cdef78e9a79c0ee621f6470
- listener-device-trial.py: 0506b66b9e5f6072778df9258b52254b6b8a07201520d911e2a3422ab2d99524
- test_wav_input.py: d9ff36b33d0aa3898ddceeeae475231d76c6cdaaf4d566811594142437a2fbe2
- test_crossover.py: c7abd352f76fdc77757459bf285d097be8c85442043ccfdd2dbed7b485e8e731

Only this plan and archive copies changed for readiness. No tracked files were edited and no tests, builds, live HTTP or hardware actions ran. Shared-validator implementation remains pending parent kickoff after the frozen check.
