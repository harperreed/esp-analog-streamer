# Ownership-safe Sonos recovery: pqqq

Doctor Biz, retain a bounded recovery candidate after transient loss, then prove that the same session still belongs to this controller before any media command. Never call `play_current()` from reconnect: it calls Stop and unconditionally creates a session. This is a concrete design for review, not implemented behavior or permission to run speaker tests.

Initial design scope: exact `kata show pqqq`, current firmware/core Sonos code, queue code, approved Sonos design, gotchas, archived Victrola `sonos-integration.md`, and public Sonos references. The archive was listed/read, never modified or executed. No production edits, builds, hardware requests or extra agents ran during that initial design. The linked protocol research records the later Sol 6.1 investigation.

## Existing contracts

`fail()` clears queue, socket, session and group, tells autoplay ownership was lost, and schedules control reconnect. Reconnect retries three times with 1/2/4-second delays, rediscovers the selected player's leader, validates player/household/group with `info:1/getInfo`, subscribes to group playback, and marks Stopped. This intentionally restores observation only. It cannot recover media because it discarded session and queue identity.

The existing `Commands` generation/token is revoked when Stop, selection or autoplay-disable is accepted. `run_if_current` serializes the final create/load send with cancellation acceptance. `request` correlates command ID, namespace, response name, deadline and household/scope; it dispatches events while awaiting replies. Transport operations have an existing five-second timeout. Preserve those real boundaries. Token cancellation cannot undo bytes already sent or serialize another Sonos controller's takeover.

Current ownership loss uses header-scoped `sessionError`, session state, suspended `sessionInfo`, and a changed item after `item_confirmed`. A pre-load playback snapshot can be ignored while the item is unconfirmed. Recovery must not reuse that pre-load exception: after disconnect, foreign current-item evidence is a stop condition, not a stale event to disregard without a connection/snapshot ordering proof.

## Protocol proof and its limit

The exact existing LAN commands available for observation are `playbackSession:1 / subscribe`, scoped to the retained session ID with `{}`, and `playback:1 / getPlaybackStatus`, scoped to the rediscovered group with `{}`. The latter is corroborated by the archived Victrola live read; it is not currently called by production `connect()`. An initial session subscription yields `sessionInfo` according to the [Sonos sessionInfo reference](https://docs.sonos.com/reference/sessioninfo). Subscription success alone returns an empty body and does not identify the current media owner. [Sonos subscribe reference](https://docs.sonos.com/reference/playbacksession-subscribe-sessionid).

The documented ownership operation is `joinSession`, matching the original `appId` and `appContext`; it joins an existing session. `createSession` replaces a session, and `joinOrCreateSession` may create one. Neither is a recovery query. The documentation explicitly forbids automatic recreation after eviction. [Sonos playback sessions](https://docs.sonos.com/docs/playback-sessions). Successful create/join replies use `sessionStatus`, including `sessionId` and `sessionCreated`; recovery must require the retained ID and `sessionCreated == false`. [Sonos sessionStatus](https://docs.sonos.com/reference/sessionstatus).

**Unresolved protocol proof:** no inspected repository code, retained LAN receipt or archived Victrola snippet implements or exercises `joinSession` on the local `:1` WebSocket transport. The individual joinSession endpoint reference was inaccessible during research. The pinned aiosonos playback-session source has no join implementation. Sonos's official [cloud queue workflow](https://docs.sonos.com/docs/cloud-queue-play-audio) explicitly describes joining through a secure WebSocket to a player and receiving `sessionStatus`, so join semantics are documented for a local workflow too. Its precise `playbackSession:1` command scope/body/reply and rejection of old-session writes after takeover remain unproven. See the [protocol research evidence](../../audit-evidence/2026-10-10/sonos-join-protocol-research.md). Do not invent `getSessionStatus`, `getCurrentSession` or a resume command. A same-ID `sessionInfo(suspended=false)` plus a matching group item is useful evidence, but no inspected source proves that this combination alone grants fresh write authority after reconnect.

Recommended implementation gate: first obtain reviewed protocol evidence for joining the retained session on this actual LAN transport, including failure after eviction, and for rejecting session-scoped load after a takeover races the proof. Until then implementable recovery is bounded control observation only; automatic media resume/reload remains blocked. This is an explicit P0 proof gap, not a reason to substitute a takeover-capable API.

## Minimal retained state and transitions

Keep one in-memory `RecoveryCandidate` owned by the existing worker: original command generation and captured token, selected player, household/group, session ID, original app identity/context, immutable `QueueSnapshot`, prior confirmed item, failure cause, overall deadline, attempt count and whether one recovery load has already been sent. No NVS recovery state, second worker or second media queue. Store the original app context at session creation instead of recomputing an unverified value on reconnect.

Retain the queue snapshot as the same queue object or ownership-transferred snapshot, not a second mutable source. During a short recoverable transport outage, keep the existing immutable queue publication available so an already playing Sonos can fetch it; revoke publication immediately on cancellation, ownership loss or terminal expiry. This deliberately differs from today's fail-clear behavior and needs tests for every terminal path. Queue availability grants no control authority. Original format, item ID and buffer preset remain fixed even if saved settings change for a future Play.

Only a transport failure during a positively established Playing/Buffering intent can create a candidate. A malformed/ambiguous ownership response, eviction, suspended session, foreign source, canceled token or lost create acknowledgment cannot. The first version also treats media `playbackError` as terminal; widening recoverable errors needs actual error-specific semantics. Do not turn every `fail()` into retry eligibility.

| State/event | Result |
| --- | --- |
| Playing/Buffering + eligible transport failure | Degraded; retain candidate, close failed control socket, start finite budget |
| Degraded + due attempt + current token | Recovering; resolve/validate destination, reconnect and observe |
| Recovering + same-session proof + matching current item already Playing | Playing; no load command |
| Recovering + same-session proof + matching item Buffering | Buffering; bounded wait, do not claim recovered audio |
| Recovering + same-session proof + matching item idle/error eligible for reload | One session-scoped recovery load, then await matching playback confirmation |
| Any recovery + Stop, selection, autoplay-disable, eviction, suspended session, foreign source or ambiguous proof | Revoke candidate/token, clear queue, no media send; Stopped or Error with reason |
| Any recovery + deadline/attempt/resource exhaustion | Error; clear candidate/queue and close socket; manual fresh Play required |

Public status must distinguish Degraded/Recovering control uncertainty from actual Playing and Buffering. Those are proposed state additions requiring UI handling, not existing enum variants. A successful socket reconnect or join is not media recovery. Even Playing confirms Sonos transport state, not audibility or sample continuity.

## Bounded attempt algorithm

Proposed budget: three attempts, base waits 1/2/4 seconds with bounded ±20% jitter, and one overall 30-second monotonic deadline from first failure. These are design values, not existing constants or configuration keys. Every discovery/connect/TLS/send/receive operation is capped by remaining overall time as well as its normal timeout. Simply chaining existing five-second operations could exceed 30 seconds; add a deadline parameter where needed rather than claiming the current implementation supplies this aggregate bound. Use a verified random source when implementing jitter; no unverified API is prescribed.

Maintain at most one control socket, one command in flight and one candidate. An exhausted attempt closes its socket before retry; no recursive `fail()` starts a new budget. Playback errors and cancellation are handled before the next scheduled retry. Retry backoff releases the worker loop so it continues servicing commands.

On each attempt, check the captured generation/token before resolve and each network step. Resolve the selected player, validate `getInfo`, and require the retained household and group. The smallest version permits a changed coordinator for that same group, but cancels if the selected player moved groups or group identity changed; documented sessions can move with a coordinator, but automatic group migration is outside this change.

Subscribe to group playback before session probing. Invoke the verified join-only operation once its LAN contract is established; require a correlated successful reply with the exact retained session and no newly created session. Subscribe to that session and obtain current non-suspended session info plus a fresh group playback status with the retained item ID. Scope/correlation/connection attempt must match throughout. Foreign item, null/absent item, missing snapshot, conflicting events or partial/late reply leaves ownership uncertain and cancels automatic media action. Missing item is not permission to reload.

If the retained item is still Playing, finish observation recovery without a media operation. If it is Buffering, wait only within the same budget for Playing. If the retained item is confirmed idle and all proof predicates hold, reuse the existing `loadCloudQueue` body and exact immutable item/format/preset; do not create a session or blindly issue group `play`. Send at most one automatic load per outage. A lost load reply does not authorize retransmission: return to observation to see whether the retained item is playing; terminal uncertainty stays Error.

Guard that session-scoped load with the original token using `run_if_current` at the actual final send, exactly as current create/load sends are guarded. Process queued ownership events during proof/load replies. A new foreign source between proof and send is an unavoidable distributed race: the server must reject writes to an evicted old session. Verify that property in protocol/HIL evidence before enabling recovery. If it cannot be established, keep automatic load disabled; a group-level play cannot repair the proof gap.

Local terminal ownership/cancellation events must also revoke the captured token through the existing token authority; merely clearing `self.session` leaves a stale in-flight action authorized. Do not invent a parallel cancellation flag. Reuse/refactor generation revocation with its current lock ordering. Manual Stop clears retry/candidate immediately and may suspend only the retained session through an already valid socket under the existing cleanup policy; it must never create or join another session just to stop. No volume operation belongs to recovery.

## Review and verification gates

Before implementation, review the proposed retention window, finite budget and conservative cancellation on suspended/null-item/group-change with Doctor Biz. They protect other controllers but can intentionally require manual Play after some outages. No irreversible speaker action is authorized by this document.

Tests precede code: pure transition tests cover eligibility and budget exhaustion; integration tests exercise the actual worker/protocol handling with stale IDs/scopes, partial and late replies, queued events during join/load, Stop/selection/disable at final-send boundaries, takeover and ownership uncertainty. Test response parsing as received, not mocked ownership booleans. Check queue identity/publication lifetime and resource reclamation. A same-ID subscription acknowledgment must fail the positive-proof oracle by itself. An accepted Stop must prevent all later recovery media sends for its old token.

Real Sonos acceptance remains required under approved device-test scope: 0.5/1/2.5/4-second interruptions with one/two listeners; owned-session reconnect and resumed media; takeover while disconnected and between proof/send; cancellation during backoff and in-flight operations; no source reclaim and no volume changes. Retain session/item identities, correlated requests/replies/events, command acceptance/send timestamps, queue/stream receipts, socket/heap bounds and cleanup results. Never use the production path to create a foreign test session without explicit test authorization. Track control reconnect latency separately from matching Playing and actual audio continuity.

Recommendation: approve this conservative state design only after closing the LAN join-only and evicted-session-write proof gaps. Then implement the bounded state/cancellation machinery with protocol fixtures before any deployment. Pqqq remains open until real takeover and recovery gates pass; this design is not recovery completion.
