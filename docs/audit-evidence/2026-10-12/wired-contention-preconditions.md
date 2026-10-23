# Wired contention acquisition stopped at prerequisites

Two October 12 attempts failed before Play, debug streams or host audio. Neither
measures coexistence, and neither changes the earlier failed contention verdict.
Keep 7vhq open. No Kata closed from this acquisition.

The existing tcp-retention example compiled without warnings and was flashed as
`01bb762ee722f12075f52d3d243ad4d9e1a762090eafe4674d708d8b5114cb90`.
It observes bounded TCP queues and ACK progress in the lwIP task. Its roughly
one-second exports do not establish exact callback or listener-wake timestamps.

The workstation now routes board traffic through Wi-Fi. Even selecting Ethernet's
source IP with a socket bind still selects the Wi-Fi route. The throwaway driver
therefore binds both `enp5s0` and `192.168.23.153`, verifies the device binding,
and records actual endpoints for every HTTP and debug connection. Four real
loopback/text tests passed; independent review also checked proxy bypass and
redirect refusal. Main and cleanup budgets, ownership checks and volume settings
were preserved. The new sampler prerequisite precedes any debug acquisition.

| Attempt | Result |
| --- | --- |
| trial-01 | Pre-UART Ethernet reads succeeded, but TCP connects failed after UART reset. The 20-second preflight expired. No post-reset baseline or workload was reached. |
| trial-02 | After three successful Ethernet reads on the post-reset boot, the retry reached fresh coordinator identity and read-only Sonos STOPPED/OK. Mixer inspection then failed before audio started. |

The first UART shows association and IP acquisition, an HTTP listener and later
valid sampler reports. That does not prove workstation reachability. Wi-Fi board
HTTP and Ethernet Sonos HTTP worked while Ethernet board HTTP timed out; the
latter later recovered. Its cause remains unknown. No timeout budget was extended.

The second failure is an access problem, not a missing card. `/proc/asound/cards`
lists PCH, but actual opens of its control and playback devices returned EACCES.
Their named ACL grants the desktop greeter access, not harper; seat0 is the greeter,
and harper is not in the audio group. Local desktop sign-in was requested before
another audio trial. No permissions, mixer settings or volume were changed.

Both receipts retain `passed=false`, empty phases and no cleanup errors. Trial 01
only finalized cleanup without a post-reset baseline. Trial 02 restored and checked
the already selected stopped MP3/Medium configuration. It issued discovery and
same-value format/buffer settings, but no Play or Stop. Neither created aplay.log.

UART still records 15/14 queue overflows, 480 software-dropped frames, two overflow
restarts and two discontinuities across the respective startup windows. Empty
`uart_faults` only means its narrow fault-pattern scan found no match; it does not
mean capture had no losses. No contention capture-delta claim is made.

## Restored state and next action

The ordinary current unified firmware, including the timeout-receipt fix, is
installed as `36e14f18791c1011d4215f5e91e2ee5f7d560bd2318d3bbcdb53c6afaf061964`,
boot `20630fb862ad5b966e3ec15a77010ca6`. Independent bound-Ethernet reads and fresh
Sonos identity/transport checks confirm board stopped, Sonos STOPPED, idle
listeners, active/saved MP3, Medium, autoplay/recognition off and gain 0. No owned
aplay, UART, flash or contention process remains.

No production source changed in this phase. The current source's full check is
run.83iaZywQ; both diagnostic and ordinary target builds succeeded here. Raw
private receipts, scripts and images remain in
`target/kata-hardening/oct12-contention/`; companion JSON retains failure and
restoration evidence with artifact hashes. Sol 6.1 independently reviewed both
the driver and the actual failure classification.

After desktop audio access returns, verify its controls and the intended front
jack before resuming the single acquisition. Do not substitute another output,
quiet capture, a longer preflight deadline or unbound Wi-Fi traffic to make the
gate pass. No AP access is available; sender evidence cannot locate the failing
radio hop by itself.
