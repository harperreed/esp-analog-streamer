# Listener execution resources

Kata 7vhq remains open. The bounded device acceptance failed during native Sonos
TLS setup, before any Sonos listener joined. This is not proof that reserving
listener workers repairs real playback recovery. Open Kata count remains 18.

The implementation starts exactly two permanent 4096-byte output workers before
Sonos setup. An inline two-job mailbox transfers existing socket, slot and codec
lease ownership. Idle workers retain none of those job resources. Admission still
allows two payload owners; it creates no task per connection. Startup failure
exits both worker gates, and rejection/queued destruction releases resources
outside the mailbox lock. Peak output task count remains two; idle memory grows
by two stacks, task backing and mailbox/native synchronization storage.

The target reported 96-byte jobs and a 224-byte mailbox, excluding native backing
and allocator overhead. Boot internal free memory went from 104415 to 82467 bytes
across output mailbox, output-worker and admission-worker startup. That interval
includes concurrent activity and is not an isolated allocation measurement.
The two diagnostic workers reported lifetime minimum stack headroom of 1116 and
1052 bytes. These are observations from this short run, not a worst-case bound.

## Verification

Scoped Sol 6.1 design/code reviews and parent fresh-eyes review found no material
code blocker. New host tests execute the actual mailbox with real threads and
sockets: two-worker reuse, partial startup failure, exact owned rejection,
queued codec leases, checked close and destruction outside the lock. Host tests
do not reproduce ESP-IDF allocation or condition-variable scheduling.

Canonical run.kcrncMVU passed 26 stages and failed only firmware formatting for
module ordering in lib.rs. After that formatting-only correction, canonical
`scripts/check --firmware` passed all five stages in run.eiWqre4n. Host sources
were unchanged from the 22 passing host stages. This is not one all-green full
run. The disposable device driver's nine regression checks also pass.

## Failed real contention trial

Candidate ELF SHA-256:
`473391a8309a13b027f058a1a9a76b000c06bd31984db1d2d34278fede2c83a7`.
The normal flash build matched the archived ELF. The test attested that image
and boot `0d350978db9b4e74c115d58ee7bd7366` before and after the trial.

Two FLAC diagnostic clients received payload; a third received HTTP 503. Native
Play reserved intent 3 with two held slots. The newer diagnostic (owner 2)
checked-closed at device event_us 12427038. The older diagnostic (owner 1)
received 1753272 bytes and remained alive until local cleanup. No Sonos listener
joined, and no successful Sonos payload or recovery observation followed.

TLS returned -0x2700; the pinned IDF 5.5.5 Mbed TLS header names this
MBEDTLS_ERR_X509_CERT_VERIFY_FAILED. The log does not contain the detailed
verification reason. It does not establish an invalid certificate, a clock
problem or memory exhaustion. Board state became Error with “Sonos connection
failed.” Do not weaken certificate checks or rerun this unchanged trial.
Reconnect, startup cancellation and failed-selection scenarios were not run.

Capture counters also reported 20 queue overflows, two restarts and 480 software
dropped frames. This trial does not isolate their cause or prove clean capture.
It is not a release acceptance result.

The driver's guarded Stop failed on HTTP 404, followed by wait-stopped,
restore-MP3 and exact-configuration errors. Keep those four cleanup errors in the
failed receipt. Both diagnostic checked closes and source/UART teardown did
complete. A separate parent restoration, after observing Error and no active
player, used the board's session-scoped Stop and idle format switch. It verified
stopped MP3/Medium, autoplay/recognition off, no active player, and the same image
and boot. No volume or mixer setting changed.

## Close-or-block decision

Keep 7vhq open: actual Sonos recovery under debug contention is unmet. The next
prerequisite is a scoped diagnosis of the recorded TLS verification failure;
the worker resource correction cannot be called target-accepted until that
boundary succeeds. Do not add a broad soak or unrelated optimization to this
ticket. Reuse this failed receipt rather than repeating the experiment.

Raw logs, exact image and frozen driver live under
`target/kata-hardening/listener-resource-device/`. The companion JSON records
their hashes, canonical run identifiers, failed verdict and separate restore.
