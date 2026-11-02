# Capture priority during Wi-Fi startup

Keeping capture on core 0 and raising its priority from 10 to 24 removes the
reproduced startup losses in the bounded diagnostic trials. Moving capture to
core 1 also clears startup, but fails the two-listener load. The production
candidate therefore keeps core 0. Kata 93hm remains open for the remaining
setup, reconnect and configuration-change acceptance.

## Controlled comparisons

All variants use the same opt-in phase trace, buffers, recovery policy, stack
allocation and saved MP3/Medium settings. A and B differ only in capture core;
A and C differ only in capture priority. Entry observations verify actual core,
affinity and priority. No source audio, Sonos Play or mixer change ran. The
workstation's audio-device permissions still prevent driving the connected input.

| Quiet boot | Capture core / priority | Total overflows | Restarts | Software-discarded frames | First interval maximum service-delay estimate, us |
| --- | --- | ---: | ---: | ---: | ---: |
| A1 | 0 / 10 | 15 | 2 | 480 | 67527 |
| A2 | 0 / 10 | 16 | 2 | 480 | 75004 |
| B | 1 / 10 | 8 | 0 | 0 | 6816 |
| B2 | 1 / 10 | 8 | 0 | 0 | 7253 |
| C | 0 / 24 | 8 | 0 | 0 | 123 |

Every boot has eight queue overflows before the existing post-flush baseline.
Both A controls have later fault observations during Wi-Fi construction/start.
The associated historical reads last 50.954/72.445 ms and 50.908/79.921 ms.
These are wall times, including waits and preemption. B/B2/C have no later fault
records; fresh API capture receipts around 40 seconds confirm unchanged counters.
Incomplete B/B2 UART tails remain in the raw logs and are excluded from parsing.
The no-fault observer's completion flag stays zero while waiting for two faults;
that is expected. No trace records were dropped.

Each load run resets the board and requests two 120-second MP3 streams. The
receiver counts decoded sample frames and retains packet hashes. These streams
use quiet analog input and the workstation's Wi-Fi route, not an AP-controlled
network or a known audio fixture.

| Image | Load verdict | Decoded sample frames per listener | Startup restarts / software discards |
| --- | --- | --- | --- |
| A, core 0 priority 10 | Pass | 5760000 / 5760000 | 2 / 480 |
| B, core 1 priority 10 | Fail | 140544 / 130176 at failure | 0 / 0 |
| C, core 0 priority 24 | Pass | 5760000 / 5760000 | 0 / 0 |

A passes only the old steady-state gate: its startup losses remain failures.
B ends prematurely after 117120 received bytes; both listeners report excessive
media age near device time 28.34 seconds. Its summary's capture observation is
already 7.773 seconds old at failure. Separate raw UART samples through 41.372
seconds show capture still progressing with zero restarts or software discards;
an encoder window spanning departure reports zero raw overruns or resets.
Those later observations do not turn B's delivery failure into a pass or locate
the network bottleneck. A/C each have zero counted decoder omissions, duplicates
or discontinuities and fresh final capture observations.

## Correction and regression gate

The pinned configuration permits priorities 0–24. Wi-Fi runs at priority 23 on
core 0; capture now runs at 24 and normally blocks waiting for its next DMA
block. The controlled priority change supports insufficient capture scheduling
priority as a cause of these startup service gaps. It does not identify Wi-Fi
as the sole preempting task or prevent all flash-cache and scheduler stalls.
Catch-up capture can delay other work, so loaded verification is required.

`scripts/stream-soak --require-clean-startup` rejects nonzero, missing, duplicate
or malformed loss/recovery counters before establishing its baseline or starting
decoders. It allows the expected initial queue backlog and intentional settling
discard. The ordinary steady-state mode remains available for transport-only
experiments. Tests use sanitized actual A/C rows and malformed counter variants;
replay rejects all three retained A2 rows and accepts all three C rows.

All 27 canonical checks pass in `target/check-logs/run.4Ey5mnzb`, including
firmware compilation and the linked I2S IRAM check. Independent source, linked
diagnostic and harness reviews found no blocking issue.

The ordinary feature-off image also passes the strict startup and 120-second
two-listener gate: each listener decodes 5760000 sample frames in 5000 packets,
with zero omissions, duplicates or discontinuities. All 15 capture samples have
zero loss/recovery counters; the eight initial queue overflows and 24000
intentional settling frames remain visible. The final capture sample is 0.234
seconds old. Conservative loaded internal/PSRAM minimum sums are 46615/2634696
bytes. The complete decoder/UART receipt is retained, not just progress timestamps.

Reproduce this gate with an idle board and recognition disabled:

```sh
ESPFLASH_PORT=/dev/ttyUSB0 scripts/stream-soak BOARD_IP \
  --seconds 120 --listeners 2 --path /stream.mp3 \
  --require-clean-startup --failure-observe-seconds 15
```

This command resets the board and decodes to hashes without speaker output.
Final device identity and fresh coordinator queries verify ordinary image
`e93f8f72692c1630d082de42b3d1ae8d11d8c106c2ef6e978e96b74a83cb5eea`,
boot `6da14d38badb3e5d09f3745cbb76b5f2`, stopped MP3/Medium, recognition/autoplay
off, idle listeners and Sonos STOPPED/OK. The ordinary ELF has no trace symbols;
its source and archived image match the checked and flashed build. Owned flash,
UART and decoder processes have exited.

## Resource observations and limits

Diagnostic A/C capture charged runtime during load spans 10.94–11.10% /
10.81–10.94%; encoder charged runtime spans 77.25–77.65% / 77.47–78.10%.
These are observations, not an isolated priority-cost benchmark: A stops its
read-time observer after two faults while C continues observing. Charged runtime
includes charged interrupts. Loaded conservative internal minimum sums are
43323 / 42547 bytes, and PSRAM minimum sums are 2709332 / 2648048 bytes.
Region minima and largest-block snapshots do not prove allocation-time headroom
or a leak. The capture task still has an 8192-byte stack; no runtime capture
stack watermark is exposed.

Existing MQTT connection errors remain visible across the variants. No panic
or watchdog signature was found. Quiet input and decoded counts do not prove
analog fidelity, exact capture of every DMA interrupt, or long-run recovery.
TLS control work remains superseded by Doctor Biz's HTTP choice. The remaining
93hm acceptance must cover the active setup/reconnect/configuration paths.

Raw UART, ELF/build receipts, hashes, failed summaries and independent reviews
remain under `target/kata-hardening/oct12-affinity/`. The accompanying JSON
records sanitized verdicts and hashes without publishing raw network details.
