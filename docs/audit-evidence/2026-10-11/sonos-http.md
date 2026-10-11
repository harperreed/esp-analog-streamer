# Sonos HTTP control candidate

Doctor Biz selected HTTP/UPnP control on October 11, superseding the proposed
Sonos TLS allocator experiment. This candidate uses the existing standalone
controller and settings with peer-bound HTTP AVTransport commands. It preserves
MP3/FLAC, buffer presets, command cancellation and two-slot intent admission.
The production Sonos controller no longer opens a secure WebSocket. Other
features' HTTPS settings are unchanged; the TLS fixture remains diagnostic-only.

## Evidence and remaining gates

The first integrated candidate passed all 27 canonical stages in
`target/check-logs/run.maDMyTMw`. Independent review then identified topology
namespace/hierarchy validation, canceled-Play cleanup evidence and harness
ownership/deadline defects. Regression tests reproduce those cases; reviewed
corrections are in the current candidate. The subsequent canonical run
`run.wVMBHa6B` passed 26 stages and failed only Clippy on a constant-value
assertion in the new cancellation test. That assertion was removed; the real
capacity, token and state checks remain. Final workspace Clippy with warnings
denied, formatting and all 40 adapter tests pass. Receipts are
`clippy-final.log` and `adapter-final.log` under the raw-evidence directory.
This is a corrected stage plus the canonical run, not a claim that the failing
invocation returned success. The 14 protocol tests, public deadline test and
10 harness tests pass; independent scoped reviews accept the corrections.

Both default and `sonos-fixture` diagnostic example builds pass. Checking the
preserved fixture exposed a stale duplicate-module example; it now calls the
actual library, and its Wi-Fi hold loop uses the current lease API. The first
failed build is retained separately from the corrected build logs. Neither
diagnostic was executed. The local shell also reports a pre-existing unavailable
Homebrew startup path; final Rust checks report no compiler warnings.

A throwaway read-only host probe calls the actual Rust `sonos_upnp::Client`.
It reconstructs the selected coordinator from retained SSDP evidence, validates
its current device description and topology, then reads transport and media URI.
Both the initial and topology-corrected clients succeeded against the real
coordinator, observing Stopped. Neither probe issued a media or volume write.
Raw receipts are `target/kata-hardening/sonos-http/read-probe-run.log` and
`read-probe-reviewed.log`. The initial probe manifest path error is retained in
`read-probe.log`; it occurred before any network request.

The prepared `scripts/sonos-http-check` exercises board-controlled FLAC/Medium,
deferred MP3/High settings and subsequent MP3/High playback, then restores
MP3/Medium. It binds running image/boot and UART delivery to the observed intent,
uses only the front headphone jack for its source, and never writes volume.
No candidate has been flashed and this hardware gate has not run.

## Product decision and limitations

Stop policy awaits Doctor Biz's answer. The proposed checked Stop validates the
current coordinator and exact stream URI before sending SOAP Stop. Another
controller can change playback between that observation and command. The other
option is to revoke only this stream, allowing already buffered audio to drain.
The candidate must not be deployed until that choice is resolved.

HTTP observations do not grant session-scoped authority. There is no automatic
SetURI/Play on recovery or source change; ownership-safe automatic recovery stays
open. Target heap/stack behavior, both-codec playback, contention and sample
continuity are unverified for this candidate. No Kata closure is claimed.
