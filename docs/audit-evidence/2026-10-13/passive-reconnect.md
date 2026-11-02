# Passive Sonos media reconnect — October 13, 2026

Doctor Biz, the established-playback probe produced a bounded negative result:
after the sender closed its socket following a complete MP3 frame through its
normal checked-close path, no replacement listener was admitted during the next
103.535651 seconds
of recorded device activity. Sonos continued reporting PLAYING, then reported
STOPPED. This does not establish what it does under packet loss, a write timeout,
an AP interruption, or a different close outcome. Keep pqqq open.

Both driver runs failed their aggregate acceptance. Their failures remain in
the adjacent receipt; the negative observation comes from separately reviewed
UART and read-only Sonos records. No recovery Play, SetURI or Stop was issued.
Each run had one authorized initial Play. The first run needed separate checked
cleanup; the second stopped naturally. No workstation source, mixer, volume,
ACL or TLS configuration changed.

## First-owner trial: failed before injection

Image `bc67008fc7eb1788d28766828e42a460ce718c10c8b88506fb9e429cb9af396d`
claimed the first owned connection. That owner naturally ended with
ConnectionReset after 1.344929 seconds and 122 complete frames / 117120 bytes.
Its checked close completed at 25094837 device microseconds; another owner
joined at 25101183 and delivered 3374 frames before explicit cleanup. This is
startup replacement evidence, not the intended established-playback test.

The driver passed a `monotonic` field into `Trial.event`, which already supplies
that field. The resulting TypeError prevented its first snapshot. Initial
cleanup correctly refused Stop without confirmed ownership. A parent cleanup
attempt also refused before any write because the saved pre-discovery groups
were empty. Fresh current configuration, coordinator, exact tagged URI and
TrackURI checks then allowed the authorized Stop; stopped/idle state was verified.
All failed receipts remain saved.

The driver now calls the sample timestamp `sample_started_monotonic`. The real
event-writer regression reproduces the failure with the original row and passes
with the corrected row. The archived source and regression preserve this fix;
source review and pure validation alone had missed the integration error.

## Conditional established-playback trial: no admitted replacement

The second throwaway image,
`15f28aacd54fd377299795b8f62b251e4714344c6d4d3649ebe5ffe29f812fa4`,
binds candidates to the first intent/network-generation/operator-epoch tuple.
It injects once, after a candidate completes at least ten seconds of payload
writes. This changes diagnostic eligibility only. It cannot turn the first
failed trial into a pass or characterize startup reliability.

| Device receipt | Value |
| --- | ---: |
| First completed payload timestamp | 31062475 us |
| Injected end timestamp | 41206014 us |
| Elapsed payload-write interval | 10143539 us |
| Complete frames / bytes | 584 / 560640 |
| Departure timestamp | 41206442 us |
| Checked close timestamp | 41478215 us |
| Last UART device-export timestamp | 145013866 us |
| Recorded interval after checked close | 103535651 us |

Read-only observations establish exact URI/TrackURI and PLAYING before injection.
Fully bracketed Sonos position advances from zero to six seconds. The six-second
observation lies between device metrics timestamps 39669733 and 40978027, both
before injection. The seven-second observation straddles injection and close;
do not count it as pre-injection evidence. Sonos later reports STOPPED, with the
first such snapshot starting at device timestamp 47659492.

Only one listener joins during this boot. Its complete-frame counts match the
injection and departure, followed by its checked close. Listener telemetry
reports zero dropped events. No replacement admission follows. One raw HEAD
request receives 405; these receipts do not prove an absence of all HTTP/TCP
attempts. There is no AP or packet-capture evidence for this experiment.

The driver records 60 snapshots and 64 metrics responses with the same image,
boot and connected network generation. Its fresh-URI check nevertheless fails:
the retained URI repeats across board resets because the intent and network
counters repeat. Thus it records 40 ownership-deadline errors and refuses its
cleanup Stop. Do not waive that requirement or treat the run as an automated
acceptance pass. Separate read-only verification finds Sonos STOPPED/OK, idle
listeners and unchanged settings. A future driver must establish current-boot
ownership without assuming URL uniqueness across resets before it can be reused
as an acceptance gate. No third acquisition was run.

## Verification and restoration

The first probe passes all 27 canonical stages in `run.uD339GKt`. The refined
probe passes all five firmware stages in `run.W81DFkwg`; its change is confined
to firmware-side diagnostic eligibility. Both linked IRAM checks pass. Eighteen
focused real-component host tests also pass, covering checked close, admission,
reconnect, cancellation and stale-generation rejection. These are build/host
results, not device recovery acceptance.

The strict startup parser accepts all 17 first-trial, 14 second-trial and three
restoration capture rows. No capture loss/restart counters increase in these
observations. Quiet ADC input and these counters do not prove audio fidelity or
sample continuity. Existing MQTT/esp-tls connection errors, SDK warnings and the
actual first-trial ConnectionReset remain in raw logs; none is suppressed or
relabelled as a Sonos TLS result.

Both temporary patches are removed; all three original source hashes match.
The rebuilt ordinary unified ELF exactly matches the previously verified
`e93f8f72692c1630d082de42b3d1ae8d11d8c106c2ef6e978e96b74a83cb5eea`
and contains no probe marker. Final boot
`4741a15f17af22e1871dfa19523aec1b` is stopped MP3/Medium, with autoplay and
recognition off, idle listeners and Sonos STOPPED/OK. Saved Sonos settings and
the private compiled configuration are unchanged. All owned collectors,
drivers and flash processes have exited.

The adjacent JSON binds these claims to archived artifact hashes under
`target/kata-hardening/oct13-passive/`. The two patches are historical diagnostic
sources, not active firmware features. The archived driver remains unsuitable
for another acceptance run until its documented reboot-identity assumption is
fixed and reviewed.

This closes no Kata: wmsy remains 23/38 children closed, 15 open. The high-severity
release gap is still ownership-safe playback recovery. Do not rely on passive
media reconnect to satisfy it. Establish a supported ownership-safe restart
contract or the agreed live-continuity policy before claiming recovery robust;
preserve manual Stop and the existing prohibition on blind automatic Play.
