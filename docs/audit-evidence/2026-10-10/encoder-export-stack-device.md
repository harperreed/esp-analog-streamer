# Encoder export stack device repeat

The eb38c65 change improves measured probe stack headroom, but delivery and
encoder throughput still fail. The probe metrics minimum free stack rises from
428 to 908 bytes after actual Storage and Window rendering. That observed
480-byte improvement matches the reduction in the linked Storage application
path; it is not a complete worst-case stack bound or a throughput explanation.

Exact default/probe image hashes and original evidence manifests are in the
paired JSON. The same noise fixture and stopped MP3/Medium settings were used.
The saved ELF hash is not runtime image attestation. Earlier failed artifacts
remain intact.

## Probe repeat

Parent canceled the partial diagnostic through its existing SIGTERM handler after
sufficient repeated failure evidence. It remains completed=false and
acquisition_complete=false. Five actual raw overruns invalidate sequences; the
observed sequences reach only OFF1 intervals 1 and 2, which are discarded. No
retained ON/OFF2 or completed acquisition exists. Actual baseline/previous-window
frame deltas yield roughly 0.903–0.914 times real time. Do not attribute the
difference from the previous roughly 0.87 run to a specific cause.

Observed encoder receipts report no queue loss. Metrics minimum is 908 bytes;
encoder minimum is 4,940. Capture counters remain stable across complete reports,
but windows 1 and 10 lack full capture brackets. Counter stability does not clear
raw-ring overrun. No listener workload ran in this probe.

The first post-reset arrival contains an interrupted prior-boot probe line joined
to ROM text. Current strict replay matches the saved rejection, including this
metadata error and the real raw-loss failures. Independent review reproduced a
false rejection by adding that exact fragment to an otherwise healthy real-file
fixture. Kata sjpm tracks acquisition selection anchored to the first owned ROM
reset marker; it must preserve original bytes and continue rejecting raw loss.
Do not reinterpret this failed run as healthy after fixing selection.

The selection fix landed as 7891c44. Its real-file positive and reset negatives,
independent review and all 27 canonical checks pass. Replaying the original
reset_released event selects byte 3,767 (line 13) and still rejects all five
raw-loss boundaries. The original result and analysis remain unchanged.

## Default slow-reader repeat

Fast delivery ends at 956,160 accepted bytes. No fast decoded artifact exists,
so this is not a decoded-audio count. The reclaimed connection decodes ten frames,
11,520 samples, or 0.240 seconds. One full encoder queue drop and missing window 6
leave diagnostic history incomplete. Six exported windows report zero raw
overruns/resets; the missing window prevents full certification. Metrics minimum
remains 1,148 bytes, listener 1,664 and encoder 5,188.

All nine capture counters stay stable across four complete reports. Windows 1
and 7 lack full bracketing. See JSON for producer generation, exact bounds and
complete-report heap minima. These separate runs do not establish a controlled
performance improvement over the previous slow-reader trial.

## Cleanup and review

All original hashes remain unchanged. Probe replay, generation accounting and
reset provenance passed independent review as evidence of a failed partial run.
Both diagnostic collections retain failed/canceled status. Source cleanup exits
successfully; default helper and UART cleanup receipts report success. Parent
reaped both sessions and checked that UART and aplay had no remaining owner.

The exact default unified image remains installed. Final saved API and read-only
routing checks confirm stopped MP3/Medium, autoplay and recognition off, front
Headphone enabled, rear Line Out muted, Master unmuted at 0 dB and PCM at 0 dB.
No mixer settings were written.

An offline analysis initially imported a scratch inspect.py instead of Python's
stdlib inspect, executing the layout helper. It was renamed byte-for-byte to
inspect-layout.py; references were updated and original images/trial hashes
verified unchanged. The failed attempt remains noted in the analysis report.

Severity: delivery and diagnostic loss still block acceptance. The stack
correction is verified on this workload; it does not authorize a long soak or
close Kata 21tp/wbz6. The next delay measurement must remain small and preserve
the observed margin.
