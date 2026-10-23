# Sonos join protocol evidence — 2026-10-10

Doctor Biz, official documentation supports joining in a local-player workflow, but does not settle the exact LAN message or rejection of stale writes. Pqqq remains gated. This research read repository code, the recovery plan, gotchas, archived integration notes and public Sonos documentation. No device requests, hardware actions, builds, tests or archive changes ran.

## Primary evidence

- [Play audio (Cloud queue)](https://docs.sonos.com/docs/cloud-queue-play-audio), workflow steps 1–8, describes SSDP discovery, a secure WebSocket to a player, `joinSession` in `playbackSession`, and a `sessionStatus` response. This corrects the recovery plan's earlier cloud-only characterization. It supplies no exact `playbackSession:1` join header, target scope or body schema.
- [Playback sessions](https://docs.sonos.com/docs/playback-sessions) says joining requires matching `appId` and `appContext`; creating replaces an existing session. It requires clients to relinquish control after eviction and forbids automatic recreation. Client policy does not prove server rejection of a raced write.
- [sessionStatus](https://docs.sonos.com/reference/sessionstatus) defines `sessionId` and `sessionCreated`; false identifies an existing session. This defines response meaning, not the exact LAN response envelope.
- [sessionError](https://docs.sonos.com/reference/playbacksession-sessionerror) documents `ERROR_SESSION_JOIN_FAILED` for identity mismatch or no current session, and eviction notification to session subscribers. It does not specify the failure reply to a stale session-scoped load.
- [loadCloudQueue](https://docs.sonos.com/reference/playbacksession-loadcloudqueue-sessionid) requires an open session and targets a session ID. Its displayed endpoint uses cloud HTTP. Neither this precondition nor [Control](https://docs.sonos.com/docs/control)'s general target validation establishes atomic rejection after takeover.

The playback sessions guide's own join reference link returned 404 at `https://docs.sonos.com/reference-link/playbacksession-joinsession`. The direct unqualified reference also returned 404; the group-ID reference and its Markdown variant were inaccessible through the browser tool. These access results establish no endpoint or scope. The [official documentation index](https://docs.sonos.com/llms.txt), retrieved over public HTTPS, omitted both join commands while listing create, load, subscribe and session objects.

## Repository limits and next proof

Current firmware creates with group scope, then subscribes and loads with session scope. Core code supplies framing and reply correlation but no join implementation. The archive's `sonos-integration.md` documents create, subscribe and load, with no join or stale-write receipt. Parser eviction fixtures prove parsing, not server behavior. The user-owned archive was read without extraction to disk, modification or execution.

First obtain an official LAN schema or a provenance-bearing application-message capture of a trusted controller's legitimate join. TLS packet capture alone cannot reveal the message body. Require exact namespace/version, target scope, required body, success envelope and failure envelope; do not guess and probe a production group.

Then, under separate device-test authorization, use one isolated group and two controlled identities. Verify reconnect joins original session A with the same ID and `sessionCreated == false`; mismatched identity and absent session must fail without creation. After a successful A join, have controller B take over. Hold A's prepared session-scoped load behind a send barrier until B's successful takeover receipt and current-source observation, then release exactly one stale A load. Require a correlated failure and unchanged B session/item/source. Repeat with A disconnected during takeover. Record raw messages, model/software version and both timelines. Any source change or uncertain outcome fails the gate; restore only through the approved test cleanup.

This experiment has not run. No exact join request is proposed here, and automatic recovery media writes remain blocked pending both proofs.
