# Control-read timeout and Stop cancellation

These bounded real-Sonos observations test the existing HTTP controller's
read-only recovery. They do not establish automatic playback restart, network
outage recovery, or source-exact audio continuity. pqqq and wmsy remain open.

## Method

At source `71f643a`, a throwaway probe waits for at least 15 seconds of the same
Playing intent. It sends the real description GET, then sleeps only the control
thread until the original observation deadline plus 50 ms. The unchanged
deadline reader and cancellation checkpoints determine the actual result.
This is a scheduling stall, not injected packet loss or an AP outage.

Diagnostic ELF:
`34e03a714a99ee013785cefd9a4ed5ebfe5949dbb7f047dbeade962249c5b7f1`.
All 27 canonical stages passed in `run.YiikcIHL`. Each acquisition used a fresh
boot, one native MP3/Medium Sonos listener, verified settings and network lease,
one authorized initial Play, and explicit checked Stop. No workstation source,
mixer, volume or access-control changes ran. The uncontrolled ADC input was quiet.

## Retention

The first acquisition failed before injection: the driver dereferenced a null
network snapshot. The firmware intentionally uses nonblocking optional snapshots.
The corrected driver records the gap and obtains fresh metadata within two
seconds, capped by the existing deadline. A disconnected or changed lease still
fails immediately. Four real localhost HTTP regressions cover this behavior.

The second acquisition reached the real `TimedOut` result, then Degraded,
Recovering and Playing. The original queue, URI, intent and lease remained.
The original listener survived the entire observation and delivered 1,647 MP3
frames / 1,581,120 bytes before explicit cleanup. Sonos position advanced from
17 seconds before the stall to 23–32 seconds afterward.

| Event | Device time, microseconds |
| --- | ---: |
| Original listener joined | 25,310,095 |
| Stall began / ended | 46,433,603 / 51,034,301 |
| Actual timeout | 51,040,410 |
| Degraded / Recovering / Playing | 51,068,637 / 52,154,564 / 52,725,578 |
| Observation ended | 61,211,144 |
| Original listener departed / closed | 61,450,680 / 61,452,166 |
| Clean close-batch export | 63,011,924 |

That acquisition also remains failed: the driver's UART drain stopped at the
close line before the following telemetry export. Its archived UART is an exact
prefix of the completed parent log. Replaying its unchanged evaluator against
the full log and original API records passes, in a separate offline receipt.
No third retention acquisition ran. The 4.600698-second stall and 1.691277-second
stall-end-to-Playing interval describe this one trial, not a recovery guarantee.

## Cancellation

The fresh cancellation boot accepted explicit Stop wholly inside the active
stall. Local delivery closed before the read returned. The real result was
`Interrupted`, followed by Stopped; no late Playing transition occurred.

| Event | Device time, microseconds |
| --- | ---: |
| Stall began | 48,336,157 |
| Stop request bracket | 49,010,736–49,378,583 |
| Original listener closed | 49,310,738 |
| Clean close-batch export | 49,790,040 |
| Stall ended | 53,293,917 |
| Interrupted / Stopped | 53,299,751 / 53,313,290 |
| Last snapshot began | 66,802,513 |

Eleven post-stall snapshots retain local stopped, remote STOPPED/OK and idle
listeners. The original acquisition remains failed because its drain incorrectly
required an export after the stall ended. The exporter emits after events or
loss changes; this close batch had already been exported during the stall.
An unrelated later export is neither necessary nor sufficient evidence of that
close. The corrected check requires a clean, valid event range covering the
actual owner's close, exported at or after that close. Malformed/lost telemetry
still fails. The same rule applies to retention and cancellation.

All 27 driver tests pass. The corrected evaluator passes both complete retained
observations in separate offline receipts. The final driver did not run another
live acquisition.

## Limits and restoration

The original failed results are immutable. Offline evaluation is reported
separately; it does not turn any acquisition exit code into a success. No further
hardware trial was used to obtain a green acquisition after these harness errors.
The final machine-readable receipt records tests, offline verdicts, full-log
capture checks, restoration and artifact hashes.

Capture counters and positive transport payload do not establish decoded sample
continuity or audible fidelity. Only one listener and one scheduling-stall case
per completed observation ran. AP access remains unavailable. The original
0.5/1/2.5/4-second interruption matrix, two-listener recovery, source takeover,
and safe automatic restart remain required. URI checks and operator Stop retain
their documented non-atomic race against other controllers.

MQTT connection failures with TLS select timeouts and HTTP receive warnings
occurred separately in the logs, including before injection. This work neither
fixes them nor establishes a causal link to the control stall. Sonos control
remained HTTP; no deferred TLS experiment resumed.

The diagnostic patch is an audit artifact, not active production code. Both
source files were restored byte for byte. All five restoration firmware stages
passed in `run.M3NzhK5v`; the rebuilt ordinary ELF exactly matches
`1d2105b115ca0423097ff742193890d8329c0e366d6f52076d6655f2c25c9eb3`.
See the JSON receipt for final device verification. Raw private UART/API evidence,
failed results, original driver revisions, tests and reviews live under
`target/kata-hardening/oct13-control-fault/`. Archived scripts require those
private fixtures and are not standalone CI tests.
The public patch uses zero context (`git apply --unidiff-zero`) and represents
the exact tested source changes. The original contextual patch remains private;
the JSON receipt records both hashes separately.
