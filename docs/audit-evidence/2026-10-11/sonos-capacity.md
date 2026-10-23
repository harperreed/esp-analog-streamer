# Sonos capacity reservation: implemented, device acceptance incomplete

Kata 7vhq remains open. The registry and session attribution pass host checks,
but neither actual contention trial passes. The remaining blocker is concrete:
after native TLS setup, the device cannot allocate a replacement listener thread
while the surviving diagnostic runs. Do not repeat a general soak to close this.

The change keeps exactly two payload owners, including revoked owners until
checked socket close. Pending Play reserves one place for 60 seconds; first
matching admission makes it active until teardown. It displaces only the newer
diagnostic. Reconnect can replace its own prior owner; stale cleanup cannot
clear a newer reservation. Identity requires the published numeric intent,
coordinator IP, current network/command generation and exact format/buffer route.
Codec or User-Agent alone grants no priority. Cancellation, failure and ownership
loss clear protection without prematurely releasing a held socket.

Canonical `scripts/check` run.Mtobk1qk passes all 27 stages, including the target
build and IRAM audit. Start/end tracked diff and configuration snapshots match.
Independent Sol 6.1 source review passes. Parent fresh-eyes review found and
fixed tagged HEAD being converted from 405 to 503; its regression first failed,
then all 34 HTTP tests passed. UART confirms actual Sonos HEAD receives 405;
the shared response regression verifies its Allow: GET header.
Real host sockets and Loom cover checked reclamation, cancellation, failed
startup and reconnect; these do not replace target acceptance.

## Actual trials

Both used candidate ELF
`ad2d30b4610a8e8bb05088856cbb30801757364dec9659fa4c12442117756147`,
the same approved front-jack noise fixture, and unchanged mixer/volume controls.
Both original verdicts remain **FAILED** in [the receipt](sonos-capacity.json).

1. **WAV diagnostics:** both clients fell behind before reservation installation.
   Checked closes occurred afterward, but that is not evidence of displacement.
   Sonos subsequently joined with the correct intent, reached Playing and sent
   49 FLAC frames / 443,531 bytes. No diagnostic survived; the required 15-second
   contention observation did not run. Scoped Stop and exact restoration passed.
2. **FLAC diagnostics:** this distinct, lower-bandwidth workload uses the existing
   encoded history. The newer diagnostic was revoked and closed at device
   timestamp 12,513,099 us. The older continued receiving, totaling 2,855,756
   bytes before local cleanup, with no remote EOF. Sonos owner 4 joined with
   intent 3 and two held slots at 16,755,344 us, after reclamation. Creating its
   thread failed with `Not enough space (os error 12)`; checked close followed at
   16,758,101 us. Native playback reported ERROR_PLAYBACK_FAILED. No Sonos payload
   delivery or successful recovery is established by this trial.

The FLAC cleanup refused Stop when observed media changed. It did not bypass
that ownership guard. Saved format became MP3, but active format stayed FLAC and
the restoration assertions failed. The parent then explicitly rebooted the
same candidate and separately verified stopped MP3/Medium, autoplay/recognition
off, no active player and no owned aplay/espflash process. This repair does not
erase the original three cleanup errors. Reconnect, cancellation and failed-start
device scenarios remain unrun; no automatic retry series followed either failure.

## Memory and limits

Actual target sizes: Slot 16, Owner 24, Intent 64, Registry 120, mutex-wrapped
Registry 128, Arc allocation 136 bytes. The initialized native mutex requests
another 104 bytes: Rust pthread wrapper 4, IDF wrapper 8, FreeRTOS queue 92,
confirmed against pinned source, target object types and linked allocations.
Registry, native backing and two Slot handles request 272 bytes versus the prior
20, an increase of 252. This is a defined subset, not the whole feature cost:
queue identity/Arc fields, larger telemetry records and allocator overhead also
consume memory. The three native backing allocations consume at least 128 heap
bytes under the pinned allocator, before other alignment/slack effects. No extra
audio buffers or tasks were added.

The next implementation must address execution-resource availability before
native setup. Admission count alone cannot guarantee a 4 KiB listener stack can
be allocated afterward. Reducing stack size without evidence or blindly raising
network buffers would not resolve this requirement. Audio continuity, audibility,
radio cause and long-duration reliability are not certified by these trials.

Local raw receipts, exact ELF, frozen acquisition drivers and hashes remain under
`target/kata-hardening/sonos-capacity-device/`; the committed JSON retains both
verdicts, relevant raw capacity lines, identities, cleanup errors and source hashes.
