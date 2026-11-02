# Preserve delivery during Sonos control outages

Doctor Biz asked to dig into wmsy/pqqq after distinguishing completed supporting
work from unfinished playback recovery. This bounded correction removes a
specific failure coupling: a transient read-only status error currently revokes
an otherwise valid audio listener. It implements the retained-delivery portion
of the recovery design using the selected HTTP transport.

## Contract

- Retain the sole existing queue, URI, intent, cancellation token, format, buffer,
  network lease and coordinator after eligible observation I/O failures.
- Only a successfully started playback intent qualifies. Failed manual Play,
  ambiguous media-write acknowledgments, malformed responses and identity
  mismatches remain terminal. Do not broaden the manual Stop policy.
- Reuse the existing HTTP client metadata: each request already opens and closes
  its own socket. Verify the same device and coordinator on each retry; do not
  rediscover or migrate peers, replace the stream URL or reserve another intent.
- Recovery makes only description, topology, media URI and transport reads.
  No automatic Play, SetURI or Stop. Check URI mismatch before querying transport.
- Three observation attempts, with 1/2/4-second waits and bounded +/-20% jitter,
  share a fixed 30-second deadline from the first failed observation. Each full
  observation has one absolute deadline capped at five seconds and remaining
  recovery time. Failure cannot restart this budget.
- Report Degraded during backoff and Recovering during reads. Matching Playing
  ends recovery; matching Transitioning keeps the same finite budget. Stopped or
  Paused ends local delivery. Do not report socket success as recovered playback.
- Accepted Stop, selection or autoplay-disable revokes the original token and
  cancels recovery. Recheck token, lease and queue after every I/O boundary so
  late successful replies cannot restore canceled state. Keep sole URI evidence
  for the existing explicit Stop path; do not create another cancellation flag.
- Network-generation changes, invalid/expired reservation, foreign URI, identity
  failure, invalid response or exhaustion clear local delivery. Preserve existing
  terminal autoplay policy. Do not allow a transient control error to trigger a
  new autoplay intent.
- Preserve the two-listener ceiling, existing duplicate-listener checked-close
  behavior, pending reservation deadline and next-Play settings semantics.

## Implementation and checks

1. Add an ordered production HTTP observation routine and test it through real
   loopback HTTP requests using retained XML fixtures. Assert read-only methods,
   early foreign-URI return, malformed/truncated responses and shared deadline.
2. Add the bounded recovery state used by the real worker. Host tests exercise
   production state, actual Queue/Capacity/Commands, cancellation, deadline and
   attempt exhaustion, jitter bounds, unchanged queue identity and settings,
   terminal replies, network invalidation and retained listener admission.
3. Review the integrated diff with an independent Sol 6.1 agent. Run canonical
   scripts/check, which includes host checks and firmware compilation. Preserve
   failing regression evidence and final exact-source verification logs.
4. Record evidence in the existing pqqq/wmsy tickets. No closure from this slice.
   Real Sonos acceptance remains required. Doctor Biz subsequently authorized
   checked Stop and board playback. Image 87848e3 passed the bounded both-codec
   playback gate, but coexistence failed. October 13 now records a real
   control-thread deadline stall and Stop during that stall. Complete retained
   observations pass offline retention/cancellation checks; all three live
   acquisitions retain harness failures. This does not test an AP outage or
   automatic playback restart. See October 13 control-timeout evidence, plus
   the sonos-http plan and sonos-http-board evidence.

Parent owns integration, full checks, hardware and commits. Sol 6.1 workers own
disjoint implementation files. Existing untracked files are outside this work.
No push, hardware write or publication belongs to these implementation steps.

## Limits

This preserves existing delivery and allows receiver-initiated retries of the
same valid URL; it cannot force Sonos to retry. Full Wi-Fi reconnection changes
the lease and remains terminal. Automatic media resume and strict ownership-safe
writes remain unresolved because HTTP AVTransport has no conditional ownership
parameter. Unit/loopback tests do not establish real Sonos recovery or decoded
sample continuity. Keep pqqq's original interruption/takeover acceptance intact.
