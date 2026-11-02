# Retained delivery during Sonos control recovery

The HTTP worker now preserves its existing audio delivery when an eligible
read-only status request fails. Previously that failure called `fail()`, cleared
the queue and revoked the listener even if audio transport was still healthy.
This is a bounded implementation step for pqqq, not full playback recovery.

The same URI, queue, intent, format, buffer, coordinator and network lease remain
in use. Three read-only retries use 1/2/4-second waits with +/-20% jitter and a
fixed 30-second budget from the first failed observation. Every observation
shares one socket-I/O deadline capped at five seconds and remaining recovery
time. Status reports Degraded during backoff and Recovering during observation.
Fresh matching Playing ends recovery. Transitioning consumes the same budget.

Stop, selection, autoplay-disable, source change, invalid identity/response,
truncated response, invalid reservation, network-generation change and budget
exhaustion cannot restore stale delivery. Autoplay sample advancement pauses
during recovery. Recovery sends no Play, SetURI or Stop commands. The existing
explicit Stop implementation and its pending product decision are unchanged.

## Verification

- Canonical `scripts/check`: all 27 stages passed, exit 0, complete evidence in
  `target/check-logs/run.r4q1tnkA`. This includes host tests, lint, concurrency
  models, web/harness checks, firmware compilation, IRAM and firmware warnings.
- The earlier `run.9j4Jw3qU` failed Clippy on a test initializer and firmware
  warnings on obsolete helpers. Both causes were fixed; the failed run remains.
- The protocol suite has 22 passing tests, including eight new real-loopback
  observation cases. All 50 adapter tests pass. Production result handling runs
  with real Queue, Capacity and Commands in the host tests; it is the same helper
  called by the firmware worker.
- A negative control temporarily restored destructive timeout cleanup in that
  production helper. The retention test failed. The mutation was removed and
  tests passed. Initial missing-API compile failures are recorded separately;
  they are not described as runtime bug reproduction.
- Obsolete helper-only tests were replaced with production-result tests proving
  foreign-source cleanup and rejection of a later matching Playing reply.
- A throwaway probe called the new production observation routine against the
  actual Sonos. Identity validation succeeded, all eight validation boundaries
  ran, and transport reported Stopped. The probe made no media or volume writes.
- Independent Sol 6.1 implementation review and scoped fix review found no
  remaining blocking findings. Parent final source review found no additional
  issue. Raw reports and logs are under
  `target/kata-hardening/sonos-control-recovery/`.

The accompanying JSON records exact source hashes, built ELF hash and canonical
stage results. No candidate was flashed and no audio source was started.

## Remaining acceptance

P1 release gap: real Sonos playback, receiver-led retry, interruption/takeover
tests and decoded continuity remain unverified. Do those before shipping this
candidate. Host tests do not run the ESP-IDF worker loop, though the target build
compiles it. A retained queue permits Sonos to retry; it cannot force a retry or
restart a player that has stopped. Full Wi-Fi reconnection invalidates the old
network generation and remains terminal.

Pqqq and wmsy stay open. Automatic media resume has no verified ownership-safe
HTTP write mechanism. The prepared board gate also awaits the separate explicit
Stop-policy choice: checked SOAP Stop has a source-switch race; local cutoff lets
buffered audio drain. No policy was selected by this implementation.
