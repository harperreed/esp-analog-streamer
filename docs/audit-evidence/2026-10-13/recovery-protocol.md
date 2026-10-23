# Sonos HTTP recovery protocol evidence — 2026-10-13

Doctor Biz, current device-advertised HTTP schemas provide concrete recovery candidates, but do not yet establish ownership-safe automatic playback restart. Keep pqqq open. The selected HTTP path and no-source-steal requirement remain; TLS is outside this work.

The parent acquired the verified selected device description and five advertised SCPDs using read-only HTTP GET with same-origin validation and redirects disabled. This review independently recalculated all six SHA-256 hashes and parsed the schemas. All match the saved receipts. Acquisition sent no SOAP commands, volume writes or media changes. An initial origin-check failure occurred before network activity; correcting full-description-URL versus origin comparison resolved it.

The adjacent JSON records sanitized filenames, exact hashes, HTTP status, byte counts, acquisition times and exact relevant action arguments/directions/related state variables. Private source receipts and XML remain under `target/kata-hardening/oct13-next/recovery-scpd/`; the complete parsed inventory is `target/kata-hardening/oct13-next/recovery-actions.json`. These are device-advertised signatures, not inferred behavior or invented protocol messages.

The current AVTransport response contains 42 actions and is byte-identical to the historical retained schema. Its acquisition at 05:52:02.285150 UTC returned 52,535 bytes, SHA-256 `37b60a5577dbc75d12d11574e6cfbfec0a8442363fb1590ebd3fa147df14d84d`. The other schemas and description were acquired at 05:46:13 UTC. No intervening schema change was observed.

| Service | Observed contract | Ownership limit |
|---|---|---|
| AVTransport | Play(InstanceID, Speed); SetAVTransportURI(InstanceID, CurrentURI, CurrentURIMetaData) | Play carries no expected URI or controller identity, so this action cannot express our required ownership guard. |
| AVTransport extensions | ChangeTransportSettings(InstanceID, NewTransportSettings, CurrentAVTransportURI); ChangeCoordinator(InstanceID, CurrentCoordinator, NewCoordinator, NewTransportSettings, CurrentAVTransportURI, RestartSink) | Current-state argument names do not establish compare-and-swap semantics or stale-owner rejection. Group/source effects remain unknown. |
| Queue | AttachQueue(QueueOwnerID) returns QueueID/QueueOwnerContext; CreateQueue(QueueOwnerID, QueueOwnerContext, QueuePolicy) returns QueueID | Owner-labeled attachment exists. No playback restart action is advertised in this service. Attachment safety, no-create failure and binding to active playback remain unproved. |
| VirtualLineIn | StartTransmission(InstanceID, CoordinatorID) returns CurrentTransportSettings; Play(InstanceID, Speed) | No session token is returned and Play accepts no coordinator/owner/URI guard. This schema supplies no evidence of safer session authority. |
| ConnectionManager | GetProtocolInfo, GetCurrentConnectionIDs, GetCurrentConnectionInfo | No PrepareForConnection or other instance-allocation action is advertised. Existing instance IDs could be observed; controller isolation is unproved. |
| GroupManagement | AddMember, RemoveMember, ReportTrackBufferingResult, SetSourceAreaIds | These signatures establish membership/state machinery, not safe media restart authority. |

Queue owner/context/policy and transport-setting/URI fields are unconstrained strings. InstanceID is ui4 without advertised range restrictions. Neither schema proves that a separate controller instance exists, or that a source switch invalidates an old queue owner. Queue UpdateID mutation arguments do not establish authority to resume active playback. AttachQueue must not be sent as a presumed harmless observation.

The next primary evidence needed is the actual contract for Queue attachment and AVTransport ChangeTransportSettings: required settings encoding, whether URI arguments validate or install state, whether attachment creates when no owned queue exists, how queue identity binds to active playback, and whether an operation atomically rejects the old owner after another controller takes over. Obtain official semantics or a provenance-bearing legitimate-controller plaintext capture before preparing any extension-write experiment. Exact action names alone cannot answer these questions. No evidence here proves every plaintext Sonos recovery route impossible.

A separate bounded gate can test passive recovery without new remote command authority: close one sender media connection, retain the original valid intent and network generation, and observe whether Sonos requests the same tagged URI and receives new payload. Require timestamps, checked old-owner release, bounded replacement admission, one/two listener resource accounting, and zero recovery Play/SetURI/Stop. Rejected canceled/stale-generation requests must remain rejected. Passing would establish Sonos-initiated media reconnect for that failure case only. It would not replace the original 0.5/1/2.5/4-second actual-path interruption matrix, takeover tests, or sample-accounted continuity. A failure must remain a failure rather than trigger unguarded Play.

No recovery behavior was implemented or verified by this schema audit. The original automatic recovery and real-device acceptance gates remain unmet.

## Follow-up: ownership and Stop

The passive-close gate proposed above has since run and failed. See
[the completed acquisition](passive-reconnect.md); it is no longer a proposed
next test.

The local review at source commit `0b71fd7` also inspected
`docs-ignore/victrola-hacks.tgz`. Its `sonos-integration.md` identifies the
playback sequence as a reconstruction, not a complete native-session capture.
The saved `evidence/sonos-readonly.json` contains getInfo, getVolume and
getPlaybackStatus exchanges. It supplies no plaintext conditional-restart
transaction or settings encoding. The reconstructed session path uses WSS;
Doctor Biz's choice to defer TLS still applies.

A focused primary-source search inspected Noson, SoCo, duncan3dc/sonos,
node-sonos-ts, go-sonos and OpenPhonos for the extension names and settings
fields. It found no concrete recovery invocation or settings construction in
the inspected text. This excludes history, submodules, binaries and possible
dynamic calls; it is not an exhaustive claim about these projects.
[OpenPhonos' generated method](https://github.com/amp64/openphonos/blob/cc5ee47d4aa2dba953ab3c964295e3778d33f0a1/src/Sonos/Services/AVTransport1.cs#L911)
forwards caller-supplied strings. The
[node-sonos-ts ChangeCoordinator test](https://github.com/svrooij/node-sonos-ts/blob/8548ab5a96126b92dcef45d5bad695abb54b5e30/src/tests/services/av-transport.service.test.ts#L55)
uses empty settings against a mocked request. Neither establishes a valid
device encoding or stale-owner rejection; do not copy those empty arguments
into a hardware test. Exact scan revisions and limits are retained in
`target/kata-hardening/oct13-recovery/semantics.md`.

Inspected revisions: Noson `31e26bdcffe50a27f00c957c6c96b382558f6f4d`;
SoCo `9be1d36c295a147f501a7994541f758f2b565653`;
duncan3dc/sonos `996de7331e10f9fb64b497a32483f642ab86b90c`;
node-sonos-ts `8548ab5a96126b92dcef45d5bad695abb54b5e30`;
go-sonos `faa122ce8c5d738fa0c194b81804f5eacf90a1a6`;
OpenPhonos `cc5ee47d4aa2dba953ab3c964295e3778d33f0a1`.
The archive scan skipped files larger than 1.5 MB (2 MB for OpenPhonos).

URI equality alone cannot express every required ownership condition. Even if
an extension atomically compares the URI, that comparison cannot distinguish
an intervening owner that reuses the URI, or a manual Stop that leaves it
unchanged. This is a logical limit, not a newly observed device result. A safe
restart contract must also establish that the original authority remains valid
and that Stop revoked it. A separate unguarded Play after an ownership check
would reopen the race. Preserve the current terminal STOPPED/PAUSED handling.

The existing production controller already retains delivery through bounded
read-only recovery. Each HTTP request opens a fresh socket; creating another
Client would not add playback authority. Its missing independent gate is the
real-device control-read timeout and cancellation acceptance documented in
[the control-recovery plan](../../superpowers/plans/2026-10-11-sonos-control-recovery.md).
That gate could test retained delivery without automatic media writes, but
would not prove playback restart, AP-loss recovery or sample continuity.

Keep pqqq open. Do not repeat the completed passive-close experiment or schema
inventory as evidence of progress. Further extension work needs a documented
contract or legitimate-controller request/reply evidence, including stale
owner and Stop rejection. No firmware, device, workstation audio, mixer or
access-control changes ran during this follow-up; no new test pass is claimed.
