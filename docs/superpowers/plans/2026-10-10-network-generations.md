# Network lifecycle generations (ewvq)

Status: implementation, independent source review, all 27 canonical stages
(run.p3FPXV1J) and same-IP real-board acceptance pass. Normal updated production
is restored with MP3/Medium stopped, autoplay/recognition off and no UART/source
owner. Evidence commit and Kata closure are next. This continues Doctor Biz’s authorized
Kata hardening. Parent owns canonical checks, commits and hardware; Sol 6.1 workers
own the disjoint implementation areas below.

## Policy and scope

Shared implementation contract for ewvq:
core::network_lifecycle exports clonable Network (one Arc<Mutex<State>>), Copy Eq Snapshot { pub generation:u32, pub address:Option<Ipv4Addr> }, Copy Eq Lease { pub generation:u32, pub address:Ipv4Addr }.
Network::new(), snapshot()->Snapshot, lease()->Option<Lease>, is_current(Lease)->bool, invalidate()->Snapshot, observe(expected:Snapshot, address:Option<Ipv4Addr>)->Snapshot. observe rejects stale expected snapshot and invalid LAN addresses; unchanged observations don't advance generation. invalidate always advances even offline so pending observations die. Exhaustion permanently unavailable; no wrap. Locks cover fixed state copies/updates only. All lease use is within one boot.
Wifi callbacks invalidate on association/disconnect/stop. Main supervisor obtains expected snapshot BEFORE querying driver association/netif/address, then observe(expected, current_address). Boot publishes same before stream startup. Existing 1s poll covers IP changes; payloads of delayed IP events are not used to install addresses. No additional observer thread or event queue. Network shared to stream and Sonos from lib.rs; capture never receives it.
Sonos stamps queued Play/Discover and socket work; stale commands/results cannot gain ownership. Stop/local configuration survives network loss. Queue publication tagged; queue readers hide stale data immediately. Discovery binds lease address; no second board_ip source. Reconnect observation-only; preserve existing quiet-then-active autoplay policy after OwnershipLost. In-flight platform calls may finish within existing bounds but results are rejected; no lifecycle lock across I/O.
Sonos QueueSnapshot gains network_generation:u32 constructor argument; media URL includes network_generation query alongside buffer. stream_http::request_network_generation(&str)->Result<Option<u32>,io::Error> parses generation independently; route_for permits one well-formed generation key alongside existing strict buffer query. Epochless direct/debug URLs keep current contract. Stream checks advertised generation against accepted lease before admission; stale tagged URL rejected (ServiceUnavailable).
Stream GenerationListener uses shared mutex with Option<TcpListener>+Lease; each short turn checks current lease, drops obsolete listener/backlog before binding replacement nonblocking wildcard:8000; no listener offline. Bind retries bounded with existing100ms delay. Same two permanent workers. Accept returns original listener lease and rechecks it. TcpListener bind/SO_REUSEADDR pinned support verified.
Carry Network+Lease through admission/listeners and recheck before expensive admission/HTTP200 and media frames. Keep existing head/write/close absolute deadlines, no added 100ms retirement guarantee. Stale loops exit as distinct NetworkChanged reason via existing checked close/slot release. No codec Runtime retirement and no capture changes. Existing in-flight IO bounds may delay retirement; measure actual cleanup in device test.
Tests production helpers using actual loopback sockets plus pure transitions; no mocked E2E. Capture independent. Device tests retain two active sockets through actual forced disconnect, check generation/address, old close and slot release, fresh two decoders, no reboot/capture fault increments. Pending admission and backlog are covered by real host sockets. Changed IP/late snapshots/no-IP covered deterministic host if real DHCP change unavailable, explicitly scope evidence.

## Work and verification

1. Network state and driver integration: core lifecycle module/tests, net.rs, lib.rs.
2. Sonos command/socket/publication integration: sonos.rs, core sonos_queue.rs and their tests.
3. Stream admission/listener integration: stream_admission.rs, stream.rs, stream_http.rs, departure reason and related real-socket tests.
4. Parent integrates, reviews, runs scripts/check, then tests actual reconnects on the board. Preserve failed evidence; close only against original Kata acceptance.

Do not add automatic playback recovery, a new metrics service, a codec reset or an
unrelated TCP policy change. Current production restore image is bd8392cd041355bfd7a7fbd9b9eded48d741cf4e70600a2595293a4bec755490.
The front headphone jack feeds LINE IN. Hardware remains parent-owned.

## Acceptance checkpoint

Device 3 passes: old generation 2 invalidated, new generation 5 published at the
same IP, old listeners closed in 0–1 ms, stale URL rejected with 503 and two new
streams each decoded 480,384 stereo sample frames. Capture advanced with all nine
fault deltas zero and no unexpected reset. Old host connections remained open
through fresh admissions. The first two harness failures remain failed: remote
EOF was an invalid proxy for local cleanup; address publication preceded listener
readiness. Fourteen helper regressions pass after fixing those assumptions.
See ../../audit-evidence/2026-10-11/network-generations.md and its JSON.

The reviewed normal image is e7404b41273f0b12fd55315dde025f81fd9b3d8a19771e49d234ef4ec7550ff7;
the tested injection image is 2948c3be97c69b4caa91880191efa04051b03391e7eb194d7b101f28eb6bc0f9.
Changed IP is host-tested; no real DHCP reassignment or Sonos playback recovery
claim. Production restoration is confirmed; this meets ewvq's original scope.
