# Finish the Kata hardening backlog

Doctor Biz's active goal on October 10: finish all Kata work, then assess.
Base c134069 on wip/kata-capture-protection. Prior phase compactions: 25;
October 13 setup/reconnect acceptance phase compactions: 1.
Latest checkpoint: MP3 transform normalization fixed, checked and deployed.
Original-criteria review closed 93hm; 23 of 38 children remain closed.
The wbz6 MP3 instruction-placement correction now passes matched hardware and
all 27 canonical stages; its ordinary ELF equals the tested candidate exactly.
Prior failed device audio trials remain failed.
Use GPT-6.1 Sol subagents with clean contexts. Parent owns review, integration,
canonical checks, commits and every hardware action. Continue through batches;
a phase checkpoint is not a reason to abandon the active goal.

## October 13 MP3 normalization checkpoint

Independent actual-stage replay and original FFmpeg basis transforms establish
the cause: forward MAx/8 needs MAx/9. Coefficient initialization now includes
the derived 8/9 factor, without per-frame work. The real-wrapper decoder test
failed at gain 1.124832 before correction and passes afterward. Retained tone
gains are 0.999858/0.999851. An independently normalized upstream build matches
production on all three synthetic fixtures; its golden provenance is recorded.

All 27 canonical stages pass in `run.uO3pGenC`. Ordinary image `bb44c0f` remains
deployed; two 60-second board listeners each decoded 2,880,000 sample frames,
with no capture-counter increases across nine clean-startup/steady windows.
Quiet ADC is not physical calibration, source-continuity proof or 7vhq's gate.
Final boot `48fe3a2552f07276fb079868c2127575` is stopped MP3/Medium,
recognition/autoplay off, listeners idle, and remote Sonos STOPPED/OK.
The first final-state collector failed on empty discovery; the corrected
read-only collector revalidated the preflight UDN. Keep the failed receipt.

See October 13 mp3-normalization evidence. Noise still has gain about 1.026
and SNR 12.59 dB; resolve codec fidelity and finite-stream tail requirements
before calling MP3 production-ready. Do not repeat the normalization diagnosis,
add a second gain correction, or adopt the rejected direct-quantizer variant.
Next independent offline work is bounded tail/headroom characterization against
the existing acceptance criteria. Recovery, long soak and distribution gates
remain. No Kata closes: 23/38 closed, 15 open. No audio/access/AP/auth changes.
This phase has one compaction and a completed implementation checkpoint;
commit and resume from this evidence with the goal active.

## October 13 MP3 gain checkpoint

Offline controls reproduce +1.022 dB on synthetic tones in the current bridge
and exact upstream encoder. Fixed/float FFmpeg agree; the FLAC control is
sample-exact at unity. An instrumented baseline and one direct-quantizer
hypothesis variant keep stored spectral gain near unity while decoded tone
gain remains near 1.125. This rules out that quantizer explanation without proving
a normalization defect. Noise differs and no fitted correction is justified.
All eight decodes have the expected actual counts and artifact hashes; Sol
review independently recomputed the metrics. Production and board untouched.
See October 13 mp3-gain evidence and archived throwaway sources.

Next gain work must derive analysis/synthesis normalization independently;
do not repeat decoder controls or add 8/9 compensation based on the scalar fit.
7vhq still needs its recorded physical-source prerequisite; quiet input cannot
replace that failed contention gate. No Kata closes: 23/38 closed, 15 open.
The active goal remains; this bounded phase has no compactions.

## October 13 Shine review checkpoint

The pinned encoder integration/license review is complete. All 23 upstream
manifest hashes match the retained exact Git object; only six documented files
contain local code patches. Those files lacked dated modification notices;
factual comments now identify the October 9 changes without changing license or
code. See October 13 shine-review evidence for verification and release limits.
The existing root GPL metadata does not relicense Shine. A corresponding-source
and relinking package still needs recipient verification before distribution.
No hardware, playback, settings or authentication change was needed. No Kata
closes: 23/38 children closed, 15 open. Physical/AP/hosted and recovery constraints
remain. This scoped phase has no compactions; keep the goal active.

## October 13 MP3 preset acceptance checkpoint

The missing e8hd cursor-admission gate passes: Low/Medium/High selected
42/167/334 frames (1.008/4.008/8.016 seconds), with exact first-delivered
sequence agreement and 73,728 independently decoded sample frames per prefix.
Three sequential debug listeners, quiet ADC, no Sonos Play or settings writes.
Full 150-second UART has no added capture faults or raw encoder overruns.
This does not prove receiver latency, long-term delivery or analog fidelity.

Temporary probe removed; all 27 diagnostic and five restore canonical stages
pass, plus 20 host gate tests. Ordinary ELF is byte-identical to 1d2105b; final
boot e003702e4a4d677ef79228b8353515c9 is stopped MP3/Medium, autoplay/recognition
off, idle listeners and remote STOPPED/OK. See October 13 mp3-preset evidence.
No Kata closes; wmsy remains 23/38 closed and 15 open. e8hd's recovery, long
soak, fidelity/gain/tail and license gates remain. Physical-source/AP/hosted
access constraints are unchanged. Phase compactions: one; goal stays active.

## October 13 control-timeout acceptance checkpoint

Real Sonos observations reached a 4.600698-second control-thread stall, natural
TimedOut/Degraded/Recovering/Playing with the original queue and listener, and
a separate Stop accepted during a 4.957760-second stall. That read returned
Interrupted and did not restore playback. Complete retained observations pass
offline acceptance; all three live acquisition results remain failed for harness
errors. Null nonblocking metadata, incomplete UART drain and a wrong post-stall
export condition are corrected with actual-fixture regressions; 27 tests pass.
Do not repeat these hardware runs merely to replace failed acquisition receipts.

Diagnostic source passed all 27 canonical stages. Temporary source is removed;
all five firmware restoration stages pass and ordinary rebuilt ELF `1d2105b`
equals the saved image. Final boot `a217c3442545de892c0ce8d7b061428e` is stopped
MP3/Medium, autoplay/recognition off, listeners idle and remote STOPPED/OK.
No workstation audio/ACL/mixer changes or AP access were needed; all owned
processes ended. Full UART capture counters pass, without sample/fidelity claims.
See October 13 control-timeout.md/.json and archived diagnostic sources.

No Kata closes: pqqq's ownership-safe automatic restart and original interruption/
takeover matrix remain unmet. wmsy remains 23/38 closed, 15 open. This phase adds
device evidence, not a new recovery implementation. Next recovery work needs a
supported restart/revocation contract; do not re-enable TLS or repeat protocol
inventory. Physical source, AP and hosted-access limits remain unchanged.
Goal remains active. This phase has one compaction and is ready for a fresh
continuation after the local commit.

## October 13 recovery contract follow-up

Two clean Sol 6.1 reviews checked local Victrola evidence and primary controller
implementations. No inspected evidence establishes a plaintext restart operation
that rejects stale ownership. The Victrola archive supplies reconstructed WSS
playback and three read-only command exchanges, not the required transaction.
Even proven URI comparison would need Stop/owner revocation semantics to cover
manual Stop or ownership reuse of the same URI. These are protocol requirements,
not newly observed device behavior or proof that every HTTP extension fails.

The existing read-only controller recovery already ships. Its real-device
control-read timeout/cancellation gate remains an independent next test; no
new recovery implementation is justified by these reads. Do not rerun schema
inventory or the failed passive-close experiment. Extension work requires a
documented contract or legitimate-controller transaction with rejection evidence.
See the follow-up in October 13 recovery-protocol.md.

Doctor Biz cannot access the workstation. Do not ask again or change audio ACLs,
mixer controls or sinks. Physical calibration/settling, AP-path injection and
hosted access gates remain as recorded. This follow-up changed documentation
only; no hardware, source audio, firmware, build or test run occurred. pqqq and
wmsy remain open, with 23/38 children closed and 15 open. Goal remains active;
this is the first no-implementation-progress turn after channel diagnostics.

## October 13 channel diagnostics checkpoint

cn4m now has the requested per-channel meter/API implementation. One stamped
reading exposes AC RMS, peak, rolling rail counts, DC and age; existing combined
level and nonblocking publication remain. Fourteen real-component diagnostics
tests and all 27 canonical stages pass in `run.qBh9KzSn`. Independent review and
parent fresh-eyes review found no code defects. Growth of measured meter/store/
pending objects plus one transient reading is bounded at 190 bytes, excluding
unmeasured full call-stack depth.

Measured capture CPU rose from 9.883722% to 11.067786% of one core, missing the
design's proposed one-point target. Narrowing the count worsened it to 11.901087%
and was reverted. Retain both outcomes; this is measured overhead, not a passing
CPU target. Thirty real API snapshots and a two-listener MP3 canary pass. Each
decoder produced 2,880,000 frames; capture startup and steady counters pass.
No known-source fidelity or long-soak claim follows from quiet ADC input.

Final ELF `1d2105b115ca0423097ff742193890d8329c0e366d6f52076d6655f2c25c9eb3`
is the ordinary rebuilt candidate. Boot `7ad6a71e4450b1307826935ce157ad56` is
stopped MP3/Medium, recognition/autoplay off, listeners idle and Sonos STOPPED/OK.
All owned hardware processes ended; no workstation audio/mixer/ACL changes ran.
See the channel-diagnostics plan and October 13 audit receipt for exact evidence.

No Kata closes: cn4m still requires physical voltage/routing/fidelity calibration,
which Doctor Biz cannot perform remotely. Count remains 23/38 closed, 15 open.
Goal remains active. Continue from the original remaining criteria; do not repeat
these bounded probes or revive workstation-access questions. This phase has one
compaction and is ready for a fresh-context continuation after its commit.

## October 13 passive media close checkpoint

Both aggregate trials failed; no Kata closed (23/38 closed, 15 open). The first
fixture selected a startup connection that naturally reset before injection;
a driver timestamp-key collision prevented snapshots. Its actual event-writer
regression now passes. A separately scoped conditional fixture reached ten
seconds of complete-frame delivery, then closed its socket after sending
584 frames / 560640 bytes.
Sonos continued reporting PLAYING, then STOPPED; no replacement was admitted for
103.535651 seconds of recorded device activity. One HEAD received 405; this is
not proof of no requests or a result for AP loss/write timeouts. Pre-injection
PLAYING/TrackURI and position 0→6 have complete device-time brackets.

The second driver also failed: URI tags repeated across reboot, so its fresh-URI
assumption prevented ownership confirmation and cleanup. Keep all 40 errors and
the refused Stop. Separate read-only verification proves stopped/idle state.
Fix that harness assumption with current-boot ownership evidence before reuse;
do not waive identity or rerun unchanged. Both diagnostic patches are removed.

First probe passed all 27 canonical stages in run.uD339GKt; the refined probe
passed all five firmware stages in run.W81DFkwg. Strict capture rows 17/14/3 pass, without a fidelity/continuity claim.
See October 13 passive-reconnect.md/.json and archived sources. Ordinary rebuilt
ELF again matches e93f8f72692c1630d082de42b3d1ae8d11d8c106c2ef6e978e96b74a83cb5eea.
Final boot 4741a15f17af22e1871dfa19523aec1b is stopped MP3/Medium, autoplay/recognition
off, Sonos STOPPED/OK, idle listeners, unchanged settings; all owned processes ended.

The supported restart/ownership contract remains pqqq's next prerequisite;
passive reconnect cannot be assumed after this close case. The pending dctv
live skip-versus-terminate choice is separate. Known-source audio/AP/hosted-auth
gates remain unchanged. Goal stays active; preserve failed gates and do not
repeat completed radio/capture tests. This phase has zero compactions.

## October 13 radio and recovery protocol checkpoint

The missing hv4x diagnostic-to-production transition is now measured: seed
bitmap 3 survives a real reset; the observation image reads 3 after initialization
and immediately before the existing setter, then reports 7/PS0 and associates.
Its channel was 11; the restored ordinary boot associates on channel 6. Preserve
both observations. No reconnect rerun or buffer change was needed. hv4x remains
open because its original correlated sender/AP criterion is still unavailable.

Both throwaway images compiled without warnings; the observation image passed
the linked I2S IRAM check. Review caught NVS automatic-erase behavior before any
probe ran; diagnostic initialization now fails without erasing on NVS errors.
Both temporary production edits and the example were removed. The rebuilt
ordinary ELF exactly matches the prior verified e93f8f72692c1630d082de42b3d1ae8d11d8c106c2ef6e978e96b74a83cb5eea.
No retained firmware or harness behavior changed; canonical run.YkqTJGio remains
the check receipt for that source, not a newly run suite. See October 13
radio-baseline.md/.json and archived diagnostic sources for the actual gates.

Fresh final boot 74e77947d416e3544fe599b07175b5a7 is stopped MP3/Medium, with
autoplay/recognition off, idle listeners and remote Sonos STOPPED/OK. Saved Sonos
settings and the private compiled configuration file are unchanged. No source
audio, media command, volume write or mixer change ran in this phase.

Read-only acquisition of the current Sonos description and five service schemas
adds exact protocol evidence for pqqq. Queue AttachQueue has owner-labelled
arguments; AVTransport ChangeTransportSettings accepts a current URI. Neither
schema proves conditional ownership or safe automatic restart. Standard Play
has no expected-source input; VirtualLineIn does not add one. See October 13
recovery-protocol.md/.json. Investigate those exact semantics before any write;
do not declare all plaintext recovery impossible or call an extension by guess.
Passive same-intent media GET replacement remains a separate testable case.

dctv needs the explicit live-continuity policy. Doctor Biz was asked whether live
Sonos should skip forward after backlog or terminate; the answer is pending.
Debug/archive semantics, partial-frame termination and capture-generation safety
must remain explicit. No skip implementation was started. Existing failed 7vhq
contention evidence remains failed; audio access is still required by that gate.
No additional Kata closed: 23 children closed, 15 remain. This phase has zero
compactions; continue from these receipts without repeating the radio experiment.

## October 13 capture acceptance checkpoint

93hm is closed after independent review against its original criteria and Doctor
Biz's explicit HTTP choice. The demonstrated capture fix remains `2c9fec6`.
Three HTTP client lifetimes pass during dual 120-second MP3 decoding; 120 real
NVS write/readback operations, cleanup and cooldown pass during dual 180-second
decoding. The earlier 90-second NVS run still fails full-operation coverage.
The public network fixture disconnects Wi-Fi, production reconnects, and both
fresh listeners decode exactly 480000 frames. All retained capture rows pass
the strict startup parser; optional raw snapshots remain explicit gaps.

The network harness now waits against a reset-relative injection deadline and
accepts MP3 for network tests. All 31 focused harness tests and all 27 canonical
stages pass in `run.YkqTJGio`. Diagnostic linked IRAM checks pass. See
`docs/audit-evidence/2026-10-13/capture-setup-recovery.md` and its JSON receipt for
image identities, exact counts, failed trials and limits. The reviewed throwaway
HTTP source is archived there and removed from the firmware examples directory.

Ordinary unified image
`e93f8f72692c1630d082de42b3d1ae8d11d8c106c2ef6e978e96b74a83cb5eea`
is restored, boot `275e2c9f3a025a614d0653f2e16269c0`. Fresh read-only checks confirm
stopped MP3/Medium, autoplay/recognition off, idle listeners and Sonos STOPPED/OK.
No source audio or Play ran. Owned acquisition/decoder/flash processes are reaped.

wmsy now has 23 closed children and 15 remaining; the parent remains open. Next
priorities are pqqq ownership-safe playback recovery and 7vhq listener priority,
using their original acceptance criteria and preserving earlier failed trials.
Capture acceptance does not close either. Known-signal fidelity/settling still
requires local audio access; do not bypass the desktop ACL. AP access remains
unavailable and hosted authentication remains unresolved; no push is authorized.
This phase is saved. Resume from this checkpoint with fresh Sol 6.1 workers,
without repeating passed capture gates or treating the full goal as complete.

## October 12 continuation checkpoint

Doctor Biz resumed the full goal. The prior status-only turn made no progress.
Current HEAD at restart was `7a0d320`; both-codec HTTP playback and checked Stop
passed, while debug coexistence failed. See the October 11 sonos-http plan for
those actual board results; it supersedes the older unflashed/Stop-pending notes.

Doctor Biz now confirms no AP access is available. Independent original-criteria
review accepts pd6c's investigation with its precise missing-evidence statement.
Its acceptance receipt is updated; closing this investigation does not close
coexistence, recovery or release gates. Current routing also changed to workstation
Wi-Fi; future receiver comparisons must explicitly use the recorded interface.

pd6c is closed with reviewed receipt commit `9578eec`: wmsy has 22 closed children
and 16 remaining. The parent plus remaining children total 17 issues.

Timeout-receipt correction is committed as `261fd12`. The terminal write used an
age-clipped deadline, but its log printed the nominal socket limit. Enforced
deadlines and buffering policy are unchanged. Independent review and all 27
canonical stages pass in run.83iaZywQ. See the October 12 timeout-budget receipt.
No new hardware verdict or further closure follows from this diagnostic fix.

The qm2a synthetic quantization pilot and independent exact repeat are retained
in the October 12 quantization-host evidence. Keep its analog, audibility,
target-cost and policy-review criteria open; production conversion is unchanged.

GitHub access still fails after Doctor Biz's retry: gh auth status times out;
the desktop GCR SSH agent lists keys but refuses signing. Local unlock is pending.
No authentication settings changed or publication occurred. The frozen proposal
at 7a0d320 excludes later commits and still needs remote privacy/ref verification
and explicit publication approval under the existing no-push constraint.

Next transport acquisition should reuse tcp-retention's existing bounded PCB
observations, not invent new native hooks. The current contention driver needs
explicit wired source binding before a comparable run: the workstation default
now uses Wi-Fi. Its 1 Hz latest-sample export cannot prove exact listener wake
timing. Preserve those limits and prior failed coexistence/recovery verdicts.
This phase is saved; resume from these exact gates without repeating its checks.

The subsequent wired diagnostic acquisition stopped at prerequisites twice.
Trial 01 lost Ethernet-bound board TCP reachability after UART reset; it later
recovered. Trial 02 reached fresh Sonos STOPPED, then ALSA denied access: PCH exists
but its device ACL grants the greeter, not harper. Local desktop sign-in was asked.
No Play/debug/audio source ran, and neither trial measures coexistence. Do not
repeat until audio access is restored. See October 12 wired-contention-preconditions.

Current unified ordinary firmware is restored and verified stopped MP3/Medium,
autoplay/recognition off, Sonos STOPPED, idle listeners. Image
`36e14f18791c1011d4215f5e91e2ee5f7d560bd2318d3bbcdb53c6afaf061964`, boot
`20630fb862ad5b966e3ec15a77010ca6`. No source/flash/UART test process remains.
The scratch driver now binds both device and source; source IP alone routes over
Wi-Fi here. Startup capture losses remain visible; they are not cleared by the
precondition receipts. No new closure: 22 children closed, 16 remain.

Doctor Biz cannot access the workstation to restore audio-device access. Keep
audio-dependent acquisitions blocked; do not bypass its ACLs. Quiet startup work
continued under 93hm. The reviewed opt-in startup-capture-trace feature passed
all 27 canonical stages in run.TvABvkgv, separate feature compilation, exact IRAM
and internal-storage checks, then one quiet 32-second device run. All 24 records
arrived; fault observations fall within EspWifi::new and wifi.start. Eight
overflows precede the actual post-flush baseline, six follow it. This locates
phases without establishing the scheduling/interrupt cause. See October 12
startup-capture-phases.md/.json. Source/ELF/runtime reviews are retained in
target/kata-hardening/oct12-startup/; no production recovery policy changed.

The ordinary unified firmware is again installed, image 94fc9e630a06362586d01ff09e40dd11e6eeeb79289a4ec97f5aecefcf56647f,
boot b124e0e109fe62c64d2dc0344dcbf5da, verified stopped MP3/Medium with idle
listeners, autoplay/recognition off and remote Sonos STOPPED/OK. The next bounded
diagnostic proposed by source review changes only capture affinity for a quiet
matched comparison; it has not been implemented or run. Keep 93hm and all
continuity/recovery gates open. No new closure: 22 children closed, 16 remain.

### Capture priority correction and strict startup gate

The matched A1/B/A2/B2 quiet comparison is complete. Core 0/priority 10 controls
reproduce two startup recoveries; core 1/priority 10 clears startup but fails the
two-listener MP3 load. Keep core 0 and raise capture priority to 24: diagnostic C
and the ordinary feature-off image both pass 120 seconds with two listeners,
5760000 decoded sample frames each and no observed startup/steady loss. Eight
initial queue overflows and 24000 intentional settling frames remain accounted.
The new opt-in --require-clean-startup gate rejects the actual A rows before
baseline acceptance and passes the actual C rows; ordinary hardware passes it.

All 27 canonical stages pass in run.4Ey5mnzb; independent source/linked/harness
reviews and final fresh-eyes review have no blockers. See October 12
capture-priority.md/.json for failed alternatives, precise receipt ages,
runtime/heap limits and the scoped scheduling cause. Ordinary image
e93f8f72692c1630d082de42b3d1ae8d11d8c106c2ef6e978e96b74a83cb5eea is installed,
boot 6da14d38badb3e5d09f3745cbb76b5f2, stopped MP3/Medium, recognition/autoplay
off, idle listeners, remote Sonos STOPPED/OK. No source audio or Play ran; owned
hardware/decoder processes are reaped. The desktop ACL remains untouched.

Keep 93hm open: repeat exact-sample active HTTP setup/reconnect/configuration
acceptance with capture counters spanning setup. Sonos TLS work stays superseded
by Doctor Biz's HTTP choice. Known-signal/analog checks still need workstation
audio access; AP controls and hosted authentication remain separate constraints.
No closure is claimed: 22 children closed, 16 remain. This landed phase needs a
fresh root session; session replacement is unavailable here, so this explicit
checkpoint and clean-context Sol 6.1 workers preserve the next step.

## October 11 closure and work-selection checkpoint

Doctor Biz subsequently selected HTTP/UPnP Sonos control ("skip tls for now").
The active implementation is `2026-10-11-sonos-http.md`; this supersedes the
pending external-TLS allocator experiment below. The first integrated HTTP
candidate passes all 27 canonical stages in run.maDMyTMw, and a read-only host
probe using its actual Rust client verifies the live coordinator and reads its
transport/URI. Review fixes are accepted. Run.wVMBHa6B passed 26 stages; its
Clippy-only test assertion failure is corrected, with final Clippy and all 40
adapter tests passing. Exact scope and remaining gates are recorded in the
sonos-http evidence. No candidate has been flashed, Stop policy is pending,
and neither hardware acceptance nor a Kata closure is claimed.

Current state supersedes the historical checkpoints below: amqe is closed in
23310f6. Open count is 18, down from 19. Its live listener observations, rendered
dashboard and HTTP acceptance are recorded in
`docs/audit-evidence/2026-10-11/live-listener-metrics.md` and `.json`.
Do not repeat that acceptance or reopen it for the separate transport failures.

Doctor Biz again flagged elapsed work without enough closures. Report actual
closures separately from partial implementation and diagnostic evidence. Use
each ticket's original acceptance; no new general soak or review gate is added.

The allocation-time observation is now complete. A retained getInfo JSON tree
kept about 14.5 KiB of internal RAM live across the next TLS write. Three explicit
JSON drops release unused values before later writes. The diagnostic candidate
reached a native FLAC request, but a later 4437-byte TLS allocation still failed
with 11019 total free bytes and only a 3712-byte largest block. All three trial
verdicts remain failed; this fix closes no Kata. See
`docs/audit-evidence/2026-10-11/sonos-json-lifetimes.md`.

Doctor Biz again flagged the unchanged count. Finish and commit the six-line
lifetime correction separately; do not keep it attached to an expanding probe.
It is committed as 8b2c878 and the temporary allocator instrumentation is removed.
The subsequent 2048-byte TLS transmit-record candidate passes native Playing for
15 seconds, owned Stop and restoration, plus a real 14027-byte Shazam HTTPS upload
and parsed Miss. Receive size, internal allocation and trust settings are unchanged.
See `docs/audit-evidence/2026-10-11/tls-transmit-records.md`. Two failed startup
observation attempts remain failed; their queue-publication assumptions were
corrected against recorded device responses. The diagnostic example is removed.

Native playback alone does not close pqqq: protocol ownership proof and recovery
acceptance still remain. Do not repeat the completed baseline, allocation or
HTTPS compatibility observations without a relevant change. The next recovery
action must establish the actual session-rejoin contract and stale-session
behavior; do not guess commands or blindly call play_current on reconnect.

Independent work selection found no immediate software-only closure among e8hd,
ed4k, 97bk and z5fs. ed4k needs a verified physical cold-start method and measured
comparison; hosted CI needs publication; dedicated HIL needs established lab
access. Treat these as exact unmet requirements, not reasons to create more
planning tickets or launch another general soak.

The pd6c pinned counter/API inventory and per-listener measurement mapping are
complete in `docs/audit-evidence/2026-10-11/network-diagnosis.md`. Independent
review accepts the existing sampled maximum lag, mean accepted throughput and
elapsed send/poll measurements with their stated limits. Remaining: resolve AP
capture access, then collect correlated evidence where available or document
the specific unavailable acquisition. Doctor Biz's access answer is pending;
do not turn that into a claim of impossible access. Open count remains 18.

The existing coverage/replay/fuzz workflows passed focused permission and
actionlint review. No hosted workflows are registered yet. m31k needs publication
and actual hosted JSON/HTML/provenance inspection, not another local pilot.
Publication approval must cover the accumulated branch: the comparison against
cached origin/main spans 530 files, not just the three workflow files. The origin
repository is private; fuzz artifacts include all committed files in source.tar,
including historical tracked journals. The focused workflow review did not audit
the entire accumulated diff. No push or draft PR was performed.

97bk still needs fast host/firmware CI cadence and dedicated HIL/release evidence.
The three current workflows do not implement its whole release contract. Continue
that implementation within the existing provider and trust boundaries; do not
call publishing the coverage workflow completion of the release gate.

The active completion target is now m31k: finish its first hosted coverage run
and inspect the reports. Prepare the accumulated branch for publication under
the existing no-push constraint, then obtain the required decision. Do not repeat
local coverage or start additional release-gate implementation while describing
either as progress on m31k's remaining hosted criterion. An October 11 remote-ref
read confirms main is still 8bd024837ef94784072122d931655ce3ef3ebf96 and the
wip/kata-capture-protection branch is absent from origin.

While that publication decision is pending, 97bk's canonical host/firmware split
is implemented. `scripts/check --host` passes 22 stages in run.iyk42Anc;
`--firmware` passes five in run.XVdCY5UR. Default run.bZ2lnZDo passes all 27;
its stage order exactly equals the two scoped runs joined. Receipts explicitly
record scope; 17 process tests and independent design/code reviews pass. The
new hosted workflow uses separate read-only hosted jobs and placeholder firmware
configuration. Hosted execution and dedicated HIL remain unverified, so 97bk
stays open. No push, device flash, audio or network change occurred. The pending
publication request names frozen 4b607fa, not later local commits.

### October 11 work-in-progress limit

TLS checkpoint after 22cfe35: a two-site diagnostic reproduced the combined
RSA_PUBLIC_FAILED and MPI_ALLOC_FAILED error (-0x4290), which X.509 converted
to NOT_TRUSTED/-0x2700.
Do not chase clock validity or replace certificate roots: effective date checks
are disabled, and the host verified the current chain against our roots. See
`docs/audit-evidence/2026-10-11/tls-verification-memory.md` and `.json`.

One general-allocation trial (8191 -> 2047) passed all 27 canonical stages in
run.0VkX36b9 and increased before-connect internal free memory by 3632 bytes
relative to the diagnostic trial, but still failed TLS before any Sonos join.
It was rejected and reverted; no further threshold guesses. Its exact inner
failure was not instrumented. The scratch driver's narrow pre-session cleanup
fix passed twelve tests and actual cleanup in both failed runs.

Original general allocation/internal TLS policy is restored, with all five
firmware stages passing in run.R4rluLpM. Normal image
d448497eb0892bc25287c95a4cc0d7a5ab6a1ca42baba57b6627cefc4a75d540 is attested,
stopped MP3/Medium with autoplay/recognition off and no active player or test
audio/UART process. No diagnostic patch remains. No production code change is
retained from the unsuccessful threshold trial. Open count: 18; closures: 0.

Pending explicit choice: Doctor Biz was asked to approve one supported external
TLS allocation test. The reviewed exact patch is in the Pending decision section
of `docs/audit-evidence/2026-10-11/tls-verification-memory.md`. On this ESP32 that
would put TLS keys/plaintext in unencrypted external RAM, affecting all Mbed TLS
users. Certificate checks would remain enabled. This needs the project's
security-change approval; none has arrived. Do not infer approval from elapsed
time, the overall goal, or unattended audio permission. No external-TLS image
has been built/flashed. Prior publication/AP-access questions remain pending.
Retire this diagnostic phase with its evidence saved; resume at that explicit
memory-policy choice rather than repeating either failed experiment.

The following worker-resource checkpoint is historical.

CAP-2 checkpoint: the two permanent output workers and owned mailbox pass scoped
review and host verification. Full canonical run.kcrncMVU passed 26 stages with
only lib.rs module-order formatting failing; after that formatting correction,
firmware run.eiWqre4n passed all five. The new candidate
473391a8309a13b027f058a1a9a76b000c06bd31984db1d2d34278fede2c83a7 is installed.
Its one bounded FLAC contention trial FAILED at TLS -0x2700 before any Sonos
listener joined. Newer diagnostic checked-close and older survival passed;
real recovery did not. No reconnect/cancel/failed-start cases or further soak ran.
See `docs/audit-evidence/2026-10-11/listener-resource.md` and `.json`.

Close-or-block decision: 7vhq remains open on the exact TLS/recovery gate; do not
keep expanding its implementation or rerun unchanged. A Sol 6.1 read-only check
of all other original ticket bodies found no independent closure supported by
existing evidence. Open count is still 18, with zero closures in this phase.
Doctor Biz's complaint is valid: code changes/checks are supporting work, not
finished tickets. Retire this bounded phase after committing its code/evidence.
The next action must address the recorded TLS verification failure, whose detailed
cause is not known; no certificate-policy change is authorized by this result.

Four driver cleanup errors remain in the failed receipt. Separate parent cleanup
verified stopped MP3/Medium, autoplay/recognition off and no active player on the
same trial boot, with source/UART processes stopped. No push/merge occurred.
The following CAP-1 checkpoint is historical, preceding CAP-2 above.

Doctor Biz again corrected long work without reducing the count. It remains 18.
The executable closure target is 7vhq: bounded Sonos listener reservation. Its
independent policy review is complete; implementation, focused tests, canonical
checks and the original real contention acceptance remain. Use one implementer
and one scoped reviewer. Do not attach general transport diagnosis to this
ticket or rerun the completed native baseline. After its bounded acceptance,
close only if the original gates pass; otherwise record the exact failed gate.
m31k awaits the existing publication decision, and pd6c awaits AP access facts;
neither pending question justifies another local pilot or broad review.

7vhq implementation now passes scoped source review and all 27 canonical stages
in run.Mtobk1qk. Both actual contention trials remain FAILED; see
`docs/audit-evidence/2026-10-11/sonos-capacity.md` and `.json`. WAV diagnostics
both fell behind before reservation. The distinct FLAC workload kept the older
diagnostic, reclaimed the newer and admitted the correct Sonos intent with two
held slots, but listener thread creation failed with ENOMEM after TLS setup.
This is the exact remaining acceptance blocker. Do not repeat either unchanged
trial or add a general soak. Reconnect/cancel/start-failure device cases were
not reached. The next change must account for the listener's execution resources
before native setup; admission count alone does not ensure stack availability.

The failed FLAC cleanup retained three errors; a separate same-image reboot
restored stopped MP3/Medium, autoplay/recognition off and no active player.
Candidate ad2d30b4610a8e8bb05088856cbb30801757364dec9659fa4c12442117756147
remains installed. No source/UART process remains. Open count stays 18. Commit
this bounded implementation/evidence phase without claiming 7vhq complete; no
push/merge. Resume only at the specific listener-memory requirement.

### Historical metrics checkpoint before 23310f6

Metrics checkpoint: the bounded endpoint, collector and separate dashboard now
pass all 27 canonical stages and a live two-boot / two-listener OFF/ON/OFF test.
A real departure value reached Prometheus and Grafana's backend; source staleness
was observed. See `docs/audit-evidence/2026-10-11/device-metrics.md` for exact limits.
amqe remains open: live individual-listener coverage, rendered panels and HTTP
runtime stack/saturation acceptance remain. The last completed receipt does not
replace live per-listener data. Open count stays 19; no closure claimed here.

Retire this metrics implementation phase after its evidence commit. Resume in a
fresh context at the named listener-observation gap; do not rerun the already
passed host, boot-identity or off/on checks without a relevant code change.
Keep 768-byte cache/snapshot and 4096-byte wire bounds. Existing admission has no
stable slot ID; diagnostic frame-state slots are not reusable production slots.
The test image remains installed, MP3/Medium stopped, autoplay/recognition off,
with temporary services, UART and source owners removed. No push or PR occurred.

45k9 is closed against its original codec acceptance (9cb1259, 3fefbbb).
The live backlog is 19 open, down from 20 before that closure. Doctor Biz
again flagged too much investigation without completed tickets.

The next independent implementation is amqe, device metrics and its separate
dashboard. This supersedes the historical dashboard hold below. Its existing
cache/endpoint plan is only a first unit: completing that unit must not close
amqe without collector, identity/freshness, matched overhead and live controlled
event evidence. No additional design-only ticket can close from current notes:
the original recovery, listener priority and quantization criteria require
real device acceptance. m31k still needs hosted CI execution; publication
approval remains pending, and repeating local coverage cannot satisfy it.

Limit native TLS work at this checkpoint to the saved baseline/current control
comparison. Retain failed trials under 93hm/pqqq; do not reopen the closed
codec ticket or launch another general transport campaign. A failing comparison
is diagnostic evidence, not closure or a release pass. Follow-up must name a
specific missing criterion and a bounded implementation or experiment.

Keep one independent implementation moving alongside the sole-owner hardware
work. Report closures, implemented work and blocked acceptance separately.

## Sender diagnostics closure checkpoint

The original d2cb overhead comparison is complete: matched normal/control boots
ran MP3 → FLAC → MP3, each 30 seconds solo then about 60 seconds dual. All six
staged trials passed; twelve saved prefixes have independently matching decoded
sample totals and no natural EOF. Capture fault deltas were zero during trials;
startup faults remain. Removing the whole sender probe lowered measured listener
CPU per frame, while delivery remained near real time. One sequential image pair
does not isolate runtime accounting from layout or radio variation.

The normal collector failed summary finalization on a malformed UART record.
Its failure remains recorded; separate offline recovery excludes that producer
window and its dependent baseline. The control collector passed. Seven host
regressions and independent reviews pass. Normal production source is unchanged,
its rebuild equals the tested bd8392 image, canonical run.WKEIQEGi remains valid,
and the image is restored with MP3/Medium stopped and autoplay/recognition off.
UART/source owners are absent. See sender-overhead.md/.json and the exact control
patch under docs/audit-evidence/2026-10-10. Close d2cb with the evidence commit;
do not require resolution of every transport failure for this diagnostic ticket.

## Network lifecycle closure checkpoint

The ewvq implementation and original acceptance checks pass: one shared network
generation invalidates old commands, queue URLs, admission backlogs and streams.
Capture remains independent and reconnect does not reclaim Sonos playback.
Canonical run.p3FPXV1J has 27 passing stages; source and harness reviews pass.
The same-IP device repeat rejects the old URL, closes both old listeners, and
decodes two new streams with 480,384 stereo sample frames each. Capture advances
with all nine fault deltas zero. Prior two harness failures remain failed and
explained in docs/audit-evidence/2026-10-11/network-generations.md/.json.
The normal ewvq production image is e7404b41273f0b12fd55315dde025f81fd9b3d8a19771e49d234ef4ec7550ff7.
Its restoration is confirmed: saved/active MP3, Medium buffering, stopped,
autoplay/recognition off, no UART or source owner. Commit evidence, then close
ewvq against its original criteria; no general endurance gate is added.

Next whole-ticket candidate within the current no-push scope is cxbw: pinned
UART crash and bounded heap diagnostics, with an exact-ELF decode, measured
overhead, protected private artifacts and normal firmware restoration. No flash
partition change or mandatory SystemView installation. m31k instead needs its
already-written separate CI workflow published and run; that remains blocked on
the no-push constraint. Do not reopen resolved codec studies to fill idle time.

## Completion rules

Doctor Biz corrected prolonged work without a decreasing open count. The
[closure review](../../audit-evidence/2026-10-10/kata-closure-review.md) supersedes
later-added closure gates in the historical checkpoints below. Admission,
logging isolation and replay are closed with evidence. Do not repeat admission
resource experiments merely to close 8s9m. Dashboard implementation remains
on hold. The MP3 placement experiment was selected to satisfy wbz6's original
bounded-correction criterion and is complete below. Pick each next bounded
action from an open ticket's missing original criterion.

The bounded wbz6 comparison is complete. Its flash control lost raw audio at
0.725x; the IRAM candidate kept up under zero, one and two listeners with no raw
loss or added active capture faults. Both receivers remained connected through
59.824 seconds of overlap with exact saved-prefix sample counts. The durable
build change passes canonical run.WKEIQEGi and produces the identical tested
ELF bd8392cd041355bfd7a7fbd9b9eded48d741cf4e70600a2595293a4bec755490.
The current installed image remains stopped MP3/Medium, recognition/autoplay off.
No source or UART owner remains. Do not reopen the input-repetition or placement
experiment merely because transport/recovery work remains. The d2cb comparison
is now complete in the checkpoint above; this earlier candidate’s joint delivery
of about 98.9% remains historical evidence and does not clear transport gates.
Full evidence: `docs/audit-evidence/2026-10-10/mp3-instruction-placement.md`.

A Kata closes only when its actual acceptance criteria have evidence. Keep
hardware acceptance and implementation separate. Do not close an epic just
because code compiles, substitute a short run for a long soak, or claim a
physical calibration without measured input. Record concrete external blockers
while continuing all independent work. Read gotchas before hardware actions.

Preserve .kata.toml and docs-ignore. No push/merge, volume changes or automatic
FLAC firmware rollback. The final test state is stopped, MP3/Medium, autoplay off.
Capture input uses the confirmed front headphone jack; rear Line Out muted.
Hardware testing is authorized for the empty office weekend. Never run multiple
owners of UART/source/flash at once. Keep original failed trials.

## First batch

1. d2cb: bounded sender-call/writable-wait counters using the existing writer.
   Specification: 2026-10-09-sender-timing.md. One worker owns stream_http.rs
   and firmware stream.rs. Preserve behavior; test real sockets, then measure
   the board with the same matched workload and decide the next fix from data.
2. 5yf3: independent reference-model property tests in a new integration-test
   file, covering variable raw/encoded capacities and listener lifecycles.
   Existing dependencies are pinned; no production/manifests edited by this lane.
3. Reconcile all open issues against current code/evidence, then order remaining
   host hardening, capture/recovery and HIL work by dependencies. Read-only lane.

Ruling: these lanes run in parallel because only one edits existing production
code; the other adds independent tests and the audit is read-only. This follows
the conflict-avoidance intent of the implementation skill, plus Doctor Biz's
standing request to parallelize independent work. Parent commits reviewed changes.
Ruling: task plans and Kata acceptance are the working specification where no
separate spec exists. Routine hardening proceeds under the existing authorization;
new consequential architectural choices get a concrete design before code.

## Backlog at start

| Kata | Priority | Work | State |
| --- | --- | --- | --- |
| e8hd | P1 | Validate the experimental buffered MP3 pipeline on ESP32 | Reconciling |
| d2cb | P0 | Separate send-call delay from socket backpressure on the ESP32 | Committed 0d9d014; device comparison running |
| pd6c | P0 | Instrument and locate intermittent TCP delivery stalls | Reconciling |
| z5fs | P1 | Automate dedicated hardware-in-the-loop streaming validation | Reconciling |
| dp08 | P1 | Isolate capture publication from delayed raw-ring readers | Reconciling |
| dctv | P1 | Define bounded live-stream continuity and resynchronization policy | Reconciling |
| cn4m | P2 | Add channel diagnostics and calibrate jack-to-PCM fidelity | Reconciling |
| ed4k | P1 | Measure and narrow capture restart settling after short gaps | Reconciling |
| 93hm | P1 | Diagnose startup and TLS-related DMA capture service gaps | Reconciling |
| wmsy | P0 | Improve streaming recovery and validate independent failure handling | Reconciling |
| hv4x | P1 | Apply and verify a reproducible Wi-Fi protocol baseline | Reconciling |
| cxbw | P1 | Add pinned ESP-IDF heap, UART core-dump and selective timing diagnostics | Reconciling |
| 7hr4 | P1 | Test importable Python harness implementations instead of AST fragments | Closed, b68318b |
| ee3q | P1 | Validate 24-hour two-listener continuity and controlled recovery | Reconciling |
| 97bk | P1 | Make host, firmware and HIL evidence explicit streaming release gates | Reconciling |
| 89g8 | P0 | Build deterministic host replay of streaming failures | Full local replay gate reviewed and passed; first hosted PR run pending |
| amqe | P0 | Add a separate ESP32 streaming dashboard and bounded metrics export | Reconciling |
| n4jb | P0 | Add reproducible network fault injection on the actual board traffic path | Reconciling |
| 1wny | P1 | Use cargo-mutants to check whether reliability tests detect wrong behavior | Closed, runner 823c0cc and pilot evidence 7b05adf |
| m31k | P1 | Measure host recovery-path coverage with cargo-llvm-cov | Two local pilots passed; CI written/reviewed, hosted run pending |
| t4ae | P1 | Add bounded host fuzzing for FLAC, HTTP and stream transitions | Closed with f57aa3e/ba5b842; hosted execution remains in 97bk |
| jq3a | P0 | Model listener admission and shutdown interleavings with Loom | Closed, b68318b |
| 5yf3 | P0 | Add proptest state-machine tests for rings and listener lifecycles | Closed, 0d9d014 |
| 21tp | P2 | Move encoder and listener logs off progress and cleanup paths | Reconciling |
| qm2a | P2 | Measure and define FLAC16 quantization policy | Reconciling |
| 45k9 | P2 | Benchmark reversible stereo decorrelation for FLAC | Reconciling |
| bf85 | P1 | Run a six-hour reconnect and playback recovery soak | Reconciling |
| cv5a | P1 | Validate interruption recovery, ownership and sustained overload | Reconciling |
| 6vct | P2 | Track pipeline progress and enforce an explicit media-age budget | Reconciling |
| pqqq | P0 | Design ownership-safe Sonos playback recovery | Concrete design; LAN join and eviction proof pending |
| 7vhq | P2 | Define bounded Sonos listener reservation or priority | Reconciling |
| 6hjs | P2 | Validate poll error and hangup handling in deadline writes | Closed, b68318b |
| 8s9m | P2 | Prevent slow request headers from serializing stream admission | Reconciling |
| ewvq | P2 | Track Wi-Fi and IP generations across stream and control lifecycles | Reconciling |
| 37k3 | P2 | Defer Shazam requests until capture has fresh usable samples | Closed, 0d9d014 |
| wbz6 | P1 | Resolve diagnostic-image MP3 encoder throughput regression | Repeated without listeners; root cause unresolved |

## Verification

First batch: sender diagnostics, property tests and recognition admission (37k3)
passed independent task and combined review. All 20 canonical stages passed on
the second run; the first exposed an ESP-IDF libc POLLNVAL omission, corrected
against the pinned platform header and generated bindings. Targeted red/green
recognition tests cover reset, configured timing and rate limits. Property tests
passed 512 cases per property and a separate 4,096-case run with seed 20261010.
Sender hardware overhead and matched measurements remain pending. Logs are at
target/kata-hardening/check-first-batch{,-fixed}.log. The backlog audit is retained at
target/kata-hardening/backlog-audit.md; no issue closes from static review alone.
Read-only planning for importable harnesses (7hr4) and listener Loom (jq3a)
continues while the parent validates this batch.

Second batch: importable harness modules, actual listener Loom models and real
poll departure tests passed task and combined review. The harness timing fixture
finding was corrected and re-reviewed. All 21 canonical stages passed, including
the target firmware build; log target/kata-hardening/check-second-batch.log.
The build locked optional host Loom dependencies in firmware/Cargo.lock; a target
cargo tree confirms they do not enter the default ESP-IDF dependency graph.

Device evidence: diagnostic three-trial run 174257, archived-image cold MP3
control 175027 and exact diagnostic cold repeat 175503 all completed cleanup.
Both images fail MP3 delivery, while diagnostic zero-listener encoding slows
to about 0.95x and loses raw history. Full receipts are in October 10 sender
analysis/control evidence. wbz6 blocks d2cb overhead acceptance; neither closes.
The diagnostic image is deployed, stopped MP3/Medium with autoplay off.

Third batch: the shared write-progress operation preserves real socket behavior
and supports deterministic actual PCM/FLAC/ring replay. Seven replay tests and
31 writer tests passed, followed by all 22 canonical stages. Review corrected
one test's reuse after a terminal error. Full production orchestration and shared
FFmpeg validation still remain for 89g8; see 2026-10-10-deterministic-replay.md.
The isolated host coverage wrapper passed review and its lightweight tests;
review added explicit host targets against inherited cross-compilation settings.
Mise's llvm-tools-preview alias caused a new missing-tool warning despite an
installed component. The canonical llvm-tools name resolves it without changing
Rust 1.93.0 or LLVM 21.1.8; repeated activation is clean. The real coverage pilot
passed 380 tests with two documented existing ignores, producing JSON and HTML.
An inherited Xtensa target did not change its explicit host test/report target.
A clean committed repeat and the separate CI job remain acceptance work.

Paired no-listener controls old-1/diag-1 each contain 60 seconds of complete
encoder windows. Archived image: 1.00010x, no raw loss; diagnostic: 0.96376x,
one raw overrun/reset. Both have zero interval capture fault deltas, from nonzero
startup baselines. Sanitized receipts are encoder-cold.md/.json in October 10
evidence. This confirms the association, not a code/cache cause. Hardware is
idle on the exact earlier diagnostic ELF, stopped MP3/Medium, autoplay off.

Sonos recovery design is recorded in 2026-10-10-sonos-recovery.md. Before enabling
automatic media actions, prove the local join-only wire contract and rejection
of writes to evicted sessions. No resume API or ownership guarantee is invented.

Third batch committed 9b7f363. Clean committed coverage repeat passed with fresh
profiles and the same 380 passing tests; receipt committed as 5d43b1f. Fourth
batch adds the separate GitHub-hosted coverage workflow and bounded Linux
mutation runner. Both passed independent review; all 23 canonical stages passed
without the earlier activation warning. Workflow action pins/inputs and mise
bootstrap PATH were verified, and actionlint passed. No push or hosted job ran.
The mutation tool's real unmodified baseline passed eight tests and generated
five candidates. The full mutation run must follow a clean reviewed commit;
inventory alone is not evidence that assertions catch mutations.

The full replay orchestration brief passed two design reviews after correcting
four contracts: blocking locks remain; virtual replay assumes uncontended steps;
deadline/retirement observation boundaries remain exact; pending runtime results
are consumed once; delayed close retains socket, Slot and Lease until completion.
Implement that extraction only after the current mutation input is snapshotted.
A separate diagnostic brief will define encoder timing and allocation receipts
without changing priorities, codec quality or buffers during attribution.

Each code task supplies targeted red/green evidence and independent review.
Run scripts/check on a frozen integration state. Hardware comparisons keep
source, radio policy, hashes, baseline counters and actual overlap intervals.
Keep task reports in the plan SDD workspace and public sanitized evidence in docs.

## Fifth-batch checkpoint

Fourth batch is committed as 823c0cc. All 23 canonical checks and independent
reviews passed before that commit. The mutation pilot completed from an immutable
archive of that revision: five attempted, four caught, one unviable, no missed
mutations or timeouts. Independent receipt review confirmed intended assertions
and the separate compiler failure. This establishes only the capture-health
pilot scope; see host-mutations.md/.json in October 10 evidence.
The approved full replay contract now lives in 2026-10-10-deterministic-replay.md;
one Sol 6.1 worker owns the coupled production extraction and characterization.
A separate Sol 6.1 lane prepares a bounded fuzzing design without installing or
changing tools. Parent owns integration, reviews, canonical checks and hardware.

The third-compaction checkpoint is saved here rather than relying on conversation
state. This environment cannot start a replacement root session; fresh-context
workers continue from these durable plans under the active goal. No new hardware
run has started; the earlier diagnostic ELF remains stopped MP3/Medium.

Full replay extraction now uses the actual encoder/listener/runtime and wait
operations. Independent review approved clock boundaries, full-tick sleep,
close-before-release ownership, and the unchanged departure-log parser contract.
The artifact oracle passes real FFmpeg controls and preserves failed receipts.
The first 25-stage canonical check passed. Scheduler review then found two P1
false-pass risks: accepted-input accounting could omit capture ranges, and
listener expectations could follow whichever publications arrived. Its fix wave
adds exhaustive source categories, exact per-actor fixture expectations, and
named omission/duplication controls. Disconnect/stale-ID, same-graph reset and
partial-block runtime cases were also corrected. The final targeted run passes
nine Rust tests, 14 oracle positives and 12 intended negative rejections at
target/stream-replay.N21tfn. Scoped rereview approved all five fixes; the fresh
canonical run passed all 25 stages, including firmware compilation, with its own
replay artifacts at target/stream-replay.uHZRMM. See replay.md in October 10
audit evidence; no firmware was flashed.

The separate replay PR workflow passes actionlint and retains per-run evidence.
It has not run on GitHub; no push is authorized. Fuzz review likewise found a
timeout-cleanup defect and weak route/write assertions. A Sol 6.1 fix worker is
finishing those checks after two earlier worker turns hit unspecified automatic
content-risk filters. Saved partial work is not accepted evidence. The scoped
February nightly leaves stable and ESP toolchain pins unchanged; final fuzz
acceptance requires a reviewed committed-source archive.

Ruling: continue with clean-context workers after this fourth compaction because
the environment offers no replacement-root-session action. Preserve checkpoints
and reports rather than imply a fresh session started. The next encoder probe
brief has been adapted to the shared pump in target/kata-hardening/
encoder-probe-adapted.md; it remains design-only, with no source or hardware work.

Replay is committed as 32c2300 and its Kata comment points to the durable receipt.
The fuzz fix reviews approved independent HTTP/write assertions, nested timeout
cleanup and strict child-shell failure propagation. The latter caught another
false-success path: exported functions do not inherit shell strict options.
Real ordinary-process tests now reject that case and preserve evidence. Corpus
attributes preserve exact binary inputs; seed bytes remain unchanged. Scoped
Clippy, formatting and actionlint pass, as do all 27 canonical stages after adding
lightweight supervisor/guard and stable helper tests. Log:
target/kata-hardening/check-fuzz-batch.log. Full ASan acceptance must use the next
clean committed snapshot; the earlier mixed-input run remains development-only.
The adapted encoder diagnostic plan passed review after explicit per-phase
settling, reset/replacement invalidation and diagnostic-only report ordering.
Implementation waits until the fuzz input archive is immutable.

Fuzz implementation is committed as f57aa3e. The actual ASan pilot passed from
its immutable archive: 20,788 FLAC cases, one million HTTP cases and one million
stream-operation cases, status 0. Independent receipt review verified tool,
source and lock hashes, all 40 original seeds, all three instrumented builds,
and byte-exact preservation of all 489 archived files. No finding artifacts were
produced. Durable receipts: October 10 host-fuzz.md/.json. Hosted workflow and
upload execution remain explicitly pending in release-gate Kata 97bk.

The next worker implements the reviewed default-off encoder diagnostic in
2026-10-10-encoder-boundary-probe.md. Fuzz inputs are already frozen, so this does
not alter acceptance. The first phase requires 80 seconds; earlier MP3 raw loss
may prevent a full valid sequence. Retain invalid phase/window receipts and bound
the device experiment instead of restarting indefinitely or tuning away faults.
A separate evidence search read all 16 files in docs-ignore/victrola-hacks.tgz
without modification; it did not establish a LAN join-only command or stale-write
rejection. The observed failed suspend targeted a replacement empty session,
not the evicted board session. Automatic recovery writes remain gated by pqqq.

The default-off encoder probe passed independent source review, feature/default
host suites, all 27 canonical stages, target feature compilation and feature IRAM
verification. The query symbol is absent from the saved default ELF and present
in the saved diagnostic ELF. Build receipts are in October 10
encoder-probe-build.md. No flash has occurred. A separate worker is implementing
the bounded collector: settle input before reset, require enough source duration,
preserve all UART/state/failure receipts, and derive phase totals from consecutive
complete windows. A 300-second source is too short for its proposed 360-second
collection allowance plus setup/final receipts; do not silently loop it.

Probe implementation is committed as 5460cac. The fifth-compaction checkpoint
continues with clean-context Sol 6.1 workers; no replacement root-session action
exists. Collector review found missing final receipts on timeout, incomplete
storage validation, same-chunk UART ordering ambiguity and unrecognized WAV
listener churn. Correct those before hardware. The separate offline noise
generator passed four actual-WAV tests. Its explicit 420-second run produced
20,160,000 stereo frames; SHA256
9311d9ca5dfe1a5f9269631afe22647834207f2b3239571474acc8ac15f3228d.
This is a new PCG64 recipe, not the historical noise fixture. Hardware preflight
still shows stopped MP3/Medium, autoplay and recognition off, front Headphone
enabled and rear Line Out muted. No probe flash or source playback has occurred.

A read-only 8s9m design is saved at target/kata-hardening/admission-design.md.
It proposes two persistent header workers without increasing the two-listener
cap. Extra stack headroom and pre-admission close ownership need resolution;
there is no implemented admission change or measured latency result yet.

Collector review fixes are complete, including actual startup log order and
post-completion validation. Final cleanup also handles unstarted helper threads
and receipt-writing failure without skipping real source termination. Tests
reproduced those failures with ordinary child processes. Independent review
reports no unresolved source findings; all 22 collector, four noise and 11
existing harness module tests pass alongside the existing parser checks. The
first and final canonical runs both passed all 27 stages; the latter includes
the cleanup fixes (target/kata-hardening/check-probe-collector-final.log).
Before claiming device acceptance, retain actual probe UART/arrival/state data
and add a real replay regression. The synthetic host records do not prove that
integration. Parent alone will flash the saved probe ELF and run one bounded
trial, preserving failure if a full sequence cannot survive.

Collector/noise work is committed as 26d21c1. The actual 360-second probe trial
failed with raw overruns at device 166116 and 346626 ms; neither sequence reached
the retained OFF2 span. Its original timeout, source cleanup and unchanged
stopped MP3/Medium state are preserved. Six retained OFF1/ON windows generated
0.993720885/0.980450643x in sequence 1 and 0.989639746/0.985457331x in sequence 2.
Zero later capture fault deltas do not clear those encoder losses. Durable trial
receipts and selected real UART replay are independently reviewed. Review found
a provenance-length test gap; zip(strict=True) rejects the shortened mapping
that previously passed. The restored real fixture passes.

The saved default unified ELF then ran the identical source and collector for
one fixed budget. Its missing probe feature necessarily produces the original
timeout and completed=false; assess ordinary encoder/capture counters separately,
without converting that acquisition negative control into a probe pass. The
run has ended and source/UART are released. Independent full-window analysis
finds 32 capture-covered windows, 320.287614 seconds and 13,345 MP3 frames:
0.999976228x generation, no raw overruns or resets and zero capture fault deltas.
The final encoder window lacks a later capture bracket and stays excluded.
The feature OFF path cannot substitute for the normal-image baseline; causation
and listener delivery remain unresolved. All 27 canonical stages passed, including
the real rejected-trial replay and firmware build; log is
target/kata-hardening/check-probe-evidence.log. Independent control review verified
artifact hashes, retained/rolling totals, capture brackets and the expected
timeout with no findings. Hardware retains MP3/Medium, stopped,
autoplay and recognition off. Do not close wbz6 or d2cb from this control alone.

Next bounded unit: 21tp listener telemetry must not delay checked close or slot
release. Keep encoder/probe export out of this first slice while the OFF-path
audit proceeds. Capture fixed-size records without waiting; the existing metrics
task formats them after removing them from bounded storage. Explicit overflow
must invalidate diagnostic acceptance. No new task, playback policy, socket
ownership policy, per-frame allocation or firmware fallback is authorized by
this unit. Existing audit authorization covers this routine implementation;
the broader recovery and admission architecture decisions remain separate.

Evidence checkpoint is 0ef0e3e. Listener-only 21tp implementation is uncommitted;
its durable brief is 2026-10-10-listener-telemetry.md. The first full canonical
run passed all 27 stages (check-listener-telemetry.log). Independent task review
still requests three acceptance fixes: validating a different UART snapshot
from the success predicate, malformed/missing receipt fields, and remaining
raw-close waits without batch completion. Fix round 1 goes to the original
Sol 6.1 implementer. No new firmware has been flashed. Hardware remains on the
saved normal control image, stopped MP3/Medium with autoplay off.

Fix round 1 addressed all three findings with real-file and production-helper
regressions. Independent scoped review reports no new finding. The final
canonical run passed all 27 stages (check-listener-telemetry-final.log); final
harness coverage includes 25 probe, four noise and 18 module tests. Both target
builds pass, feature IRAM checks pass and the allocation-query symbol remains
feature-only. The target queue symbol occupies 2008 bytes; observed DRAM data
plus BSS grows by 2008 bytes against the saved normal control. This is not a
live heap or stack margin measurement. See listener-telemetry-build.md evidence.
Next: parent-owned bounded MP3 soak/slow-reader validation, retaining separate
scenario failures and measured cleanup/stack/heap results. No hardware run or
full 21tp completion is claimed at this implementation checkpoint.

Listener implementation and acceptance fixes are committed as 3aa5b5f. Parent
flashed the checked default unified ELF; flash completed and postflash GETs
verify stopped MP3/Medium with autoplay and recognition off. Before playback,
parent found the throwaway trial wrapper could leave UART/FFmpeg descendants
behind because the soak helper starts separate sessions. Correct and test owned
descendant cleanup before running either scenario. A clean-context Sol 6.1
worker is independently designing the remaining encoder telemetry slice.

Trial 1 stopped in preflight before audio/reset: duplicate event timestamp and
mono Master validation bugs in the throwaway wrapper. Both have real-receipt
regressions; original evidence remains intact. Trial 2 failed MP3 delivery: both
listeners fell behind, while two checked closes had valid later zero-loss batch
receipts. Its final UART line is incomplete and cannot extend capture coverage.
The separate slow-only trial 3 reached a write timeout and decoded ten frames
on a later connection, but the fast stream failed and export reported one lost
record. Neither run clears delivery or full 21tp. All helper/source cleanup
receipts passed; hardware remains stopped MP3/Medium, autoplay/recognition off.

Measured metrics stack minimum is 420 bytes out of 4096. Linked inspection finds
an 800-byte persistent metrics frame (capture formatting is inlined) beneath the
992-byte listener exporter frame. Ruling: test only an out-of-line boundary on
existing log_capture before adding encoder telemetry; preserve task sizes,
priorities, output and sampling order. Actual target call/frame comparison and a
matched hardware repeat are the meaningful checks, not a host test asserting an
attribute's presence. No overflow or transport cause is established by this
stack measurement. The encoder telemetry design remains gated on that result.

Failed-device evidence is independently reviewed and saved as
docs/audit-evidence/2026-10-10/listener-telemetry-device.md and paired JSON.
The reviewer verified all 30 original hashes, complete-line capture coverage,
actual decoded samples and strict rejection of trial 3's sticky export loss.
The one-line stack experiment builds in both configurations: metrics now reserves
208 bytes, separate capture reporting 528, and listener export 992. The nested
caller/export reservation drops from 1792 to 1200 bytes, while caller/capture
drops from 800 to 736. These are linked frame sizes, not measured runtime savings;
canonical verification, independent review and a matched device repeat remain.

Device evidence checkpoint is cf03fd8. The stack-only change passed all 27
canonical stages (check-metrics-stack.log), separate probe IRAM checks and
independent source/linked-call review. Default artifact SHA matches the canonical
output. Build receipt: docs/audit-evidence/2026-10-10/metrics-stack-isolation.md.
Next: commit the reviewed boundary, flash its exact default unified ELF, and
repeat bounded two-listener and slow-only measurements with the same source.
Do not close 21tp or add encoder output based on linked sizes alone.

Stack boundary committed as 3b3afdb and flashed. Both bounded repeats measured
1188 bytes minimum free metrics stack, versus 420/580 in prior soak/slow trials.
Both still failed delivery; the slow repeat failed before third admission, so
its zero-loss record set is smaller and cannot clear the original loss. Strict
complete-line analysis and all 27 evidence hashes passed independent review.
See metrics-stack-device.md and JSON in the October 10 evidence directory.
Source/UART cleanup passed; stopped MP3/Medium, autoplay/recognition off remains.

Next unit: distinguish full, contended and poisoned listener publication drops
without changing capacity or scheduling. Binding brief:
2026-10-10-listener-loss-reasons.md. The existing nine-field acceptance contract
and loss rejection remain; attribution adds a strict three-counter reader.
Implementation is in progress with a clean-context Sol 6.1 worker; parent still
owns independent review, target builds, canonical checks and hardware. Encoder
export and broader recovery/admission architecture remain separate.

Stack repeat evidence and the reason-counter brief are committed as 644c9cb.
The four-file loss-reason implementation passed independent review, all 27
canonical stages, the probe build/IRAM check and feature allocation-query gating.
Both target queue symbols grow 16 bytes to 2024; exporter frame grows 16 bytes to
1008. Metrics/capture frames remain 208/528. Build checkpoint:
docs/audit-evidence/2026-10-10/listener-loss-build.md. Next commit the reviewed
diagnostic, then parent runs a bounded slow-reader measurement on its exact
default unified image. Preserve the 3b3afdb images and original failed trials.

Loss reasons committed as bb56186 and its exact default image tested. The slow
trial reproduced one full-queue rejection (contended/poisoned zero), with failed
fast delivery and a successful ten-frame third-connection probe. Strict acceptance
correctly remains failed. Minimum metrics stack free is 1156 bytes; cleanup and
final stopped MP3/Medium state passed. Sanitized evidence is under independent
review before its checkpoint commit.

Ruling: keep listener capacity eight for the next unit. At least ten
publication attempts are evidenced, but final pending count is unknown and
enlarging storage does not explain slow consumption.
Proceed with already-scoped encoder logging isolation and measure again, keeping
the failed gate explicit. Binding unit: 2026-10-10-encoder-telemetry.md. The small
fixed encoder queue, producer identities and strict capture bounds preserve
acceptance under delayed export; no scheduling, codec, stack or buffer changes.
The prior audit authorization covers this extraction; no renewed permission gate.

Independent reason-trial review verified 13 original hashes and caught the exact
attempt-count overclaim; corrected to at least ten with pending count unknown.
Evidence saved as listener-loss-device.md/.json under October 10 audit evidence.
No delivery or complete 21tp pass is claimed. Encoder extraction is now underway
with a clean Sol 6.1 worker; capture-failure reporting design is a read-only lane.

Encoder source extraction and capture-failure handoff are under independent
review. Parent wired the capture handoff only after explicit audio_in/lib release.
Its five actual-module tests and scoped integration review pass; target stack and
atomic lowering remain unmeasured. Binding capture follow-up plan is
2026-10-10-capture-failure-telemetry.md.

Parent target-source inspection found that ESP Rust std Mutex::try_lock can
allocate on first access: the pthread backend initializes OnceBox/pthread storage,
and pinned IDF mutex initialization allocates a semaphore. Both telemetry queues
now initialize on main before spawn_capture starts metrics or capture threads.
Real initialization-preservation tests cover state/counters; independent source
review verifies backend and call ordering. Linux host tests cannot prove this
ESP allocation behavior or target runtime cost. No queue capacity changed.

Full-host lifecycle regression exposed an incorrect eight-turn test bound: the
fixture publishes 16-frame raw blocks, requiring 72 reads for 1152-frame MP3 output.
Production behavior is unchanged; preserve the failed host logs and derive a
bounded test allowance from fixture dimensions before final verification.

Encoder/capture telemetry integration is frozen and independently reviewed. All
27 canonical stages pass; the final parser refinement also passed a fresh
check-harnesses run. Host default443/probe465 tests pass, with two existing
ignored tests in each. Both target builds and IRAM checks pass. Target encoder
queue storage is 440/632 bytes, capture failure handoff16; listener stays2024.
Metrics frame224 and nested application paths1232/default,1360/probe remain
static measurements, not runtime headroom. Fresh-eyes review found no new code
defect and corrected the capture plan's no-retry claim to match target CAS loops.
Build receipt: docs/audit-evidence/2026-10-10/encoder-telemetry-build.md. Next:
commit, then bounded exact-image default and probe trials, preserving failures.
No queue, stack, codec, priority or buffering policy change accompanied this unit.

Encoder/capture extraction is committed as 05cf802. Its exact default image
flashed successfully. Bounded two-listener trial1 reached premature MP3 EOF;
retain the failure and complete cleanup before the separate slow-reader trial.
The probe preflight also omitted Master/PCM gain checks. A Sol6.1 follow-up now
validates each declared channel plus Master unmuted, with actual archived
readback fixtures, runtime negative controls and passing check-harnesses.
No mixer writes or firmware changes accompany that harness correction.

Independent local97bk unit: README now maps release gates to existing commands,
exact sample/progress limits, diagnostic completeness and distinct five/nine
capture-counter schemas. Missing/canceled/infrastructure failures cannot pass.
Coverage evidence now distinguishes historical unimplemented CI from the later
checked-in workflow; hosted execution remains unverified. Independent source/link
review and parent fresh-eyes pass. This is a review contract, not an implemented
CI aggregator or dedicated HIL runner; 97bk/m31k remain open for those gates.

Device acceptance for 05cf802 failed. Independent reviews verified all original
hashes and producer-time accounting. Default two-listener trial decoded 7.368 /
9.072 seconds, with one full encoder queue drop and missing window 4. Slow-only
trial reclaimed a slot and decoded ten probe frames, but fast delivery failed;
three encoder queue drops hide windows 5/6, and window 7 proves raw overrun/reset.
Observed metrics minima are 1164 / 1148 bytes, not safe worst-case allowances.
Receipts: encoder-telemetry-default-device.md/.json in October 10 audit evidence.

Probe image ran no media listeners. Five raw overruns repeatedly invalidated its
first OFF1 interval; baseline-backed output was roughly 0.87x real time. Storage
and Window paths ran, with metrics minimum free stack 428 bytes. Parent ended
the partial acquisition through its SIGTERM cleanup handler after sufficient
failure evidence; original canceled result and raw bytes remain. Independent
strict replay and provenance review agree. Receipt: encoder-telemetry-probe-device
.md/.json. No completed OFF/ON/OFF, performance pass or overflow proof is claimed.

Parent restored the exact default unified image, verified stopped MP3/Medium,
autoplay and recognition off, and checked routing with no mixer writes. Source
and UART cleanup passed before every reset. Kata 21tp/wbz6 comments retain all
failed gates. Next investigate measured exporter stack cost and delay using
pinned logger/task source; do not enlarge queues/stacks or tune priority blindly.
The new release contract and CI-status clarification are committed as 5eb5786.

The bounded encoder-export stack fix passed all 27 canonical stages, both host
configurations, both target builds/IRAM, independent source and linked reviews,
and parent fresh-eyes. Actual duplicate 200/296-byte Record copy is removed;
probe Storage application nesting drops 1360 to 880 bytes. Record lifetime ends
before receipt formatting and Boundary returns before Storage. All output and
queue/task policies remain unchanged. Build receipt: encoder-export-stack-build
.md. Parent now commits and repeats exact-image hardware; no runtime margin or
throughput acceptance from these static reductions.

The eb38c65 device repeat exercises probe Storage/Window and measures 908 bytes
minimum free metrics stack, versus 428 previously. Five actual raw overruns still
invalidate the canceled partial probe; no completed OFF/ON/OFF is claimed.
Default slow-reader delivery also fails, with one full encoder queue drop and
missing window 6. Its metrics minimum remains 1148 bytes. Independent evidence
reviews pass; see encoder-export-stack-device.md/.json. Default unified firmware
remains installed, stopped MP3/Medium, autoplay and recognition off. Cleanup and
read-only routing pass; original failed artifacts remain intact.

Kata sjpm fixes a separate acquisition bug: an interrupted prior-boot UART line
can complete after reset release. Live and final analysis now start at the first
exact owned ROM reset marker and retain byte-offset provenance. Any malformed
first reset candidate or later reset rejects. Independent review caught an
initial skip-forward false pass for prefixed reset text; runtime negatives now
cover it. The healthy crossed-reset fixture passes and actual saved device bytes
still reject all five raw-loss boundaries. No decoder or probe grammar was relaxed.
Committed as 7891c44 after independent review, parent fresh-eyes and all 27
canonical checks. Replay uses the original reset_released event and unchanged
trial bytes; it is integration evidence, not a fresh healthy hardware run.

Next: independently verify the minimum metrics-delay design against pinned
FreeRTOS critical sections and permanent-task lifetime before implementation.
Only low-rate sampled Ready/Blocked counts are proposed; priorities, stacks,
queues and stream policy remain unchanged. A missing final diagnostic receipt
must remain incomplete evidence. Kata 21tp/wbz6 remain open.

## Metrics-state hardware checkpoint

The bounded sampled-state observer is committed as 55d5416 after all 27
canonical stages, both target builds/IRAM, independent source and linked reviews,
and parent fresh-eyes. Three immutable-image acquisitions retain their failures.
New default encoding measured 0.780–0.789x across five complete windows, each
with raw loss; the exact preceding eb38c65 control measured 0.994–1.006x across
five windows with no raw loss. Both fast listeners fell behind before reclamation.
This comparison establishes an association, not an observer or cache root cause.

The new probe ran without listeners and retained four raw overruns. Parent ended
the partial run through its owned cleanup handler; thirteen complete windows
measured 0.928–0.941x. It did not complete its probe sequence. Sampled Ready
counts do not prove uninterrupted execution; supervisor preemption itself can
leave metrics Ready. Query maxima of 8/12 microseconds are observed values,
not bounds. Late or missing diagnostic dumps restrict which intervals they cover.

Reviewed receipts are in docs/audit-evidence/2026-10-10/metrics-state-device.md
and .json. All acquisitions completed cleanup; original failed artifacts remain
under target/kata-hardening/metrics-state. The exact preceding unified default image eb38c65 is installed
because the new default regressed encoding. Fresh API/routing checks confirm
stopped MP3/Medium, autoplay and recognition off, front Headphone on, rear Line
Out off, unchanged zero-dB gains, and no source/UART processes. This preserves
both output formats; it does not revert to FLAC playback policy.

Next: independently review the small same-image observer OFF/ON/OFF experiment.
Metrics owns its transitions and logging; supervisor never formats transition
logs. Preserve existing capture/encoder work and hold code placement fixed within
the run. All losses remain failures and missing phase evidence stays incomplete.
Kata 21tp/wbz6 remain open. Root-session replacement is still unavailable; this
durable checkpoint and fresh Sol 6.1 contexts preserve continuity without claiming
a fresh root session.


The same-image crossover now passes source/harness reviews, linked inspection,
both target builds/IRAM and all 27 canonical checks on the final source. Four
schedule tests and fifteen scratch harness tests pass, including negative controls
for boundary-only loss and replaced provenance. Linked review moved transition
formatting out of the persistent metrics frame: 432→304 bytes, compared with
256 in the exact rebuilt default. Supervisor remains 272; runtime margin remains
unverified. The frozen diagnostic SHA is 8ef23d4fae8ecd16013e3eace538b373e461f11e266a64ac9756d457a142922c.
See metrics-state-crossover-build.md and the matching crossover implementation plan.
Parent will run bounded raw-byte observation after this reviewed checkpoint.

The exact crossover image completed two 300-second acquisitions. Trial1 retained
all raw bytes and cleanup but its wrapper raised a duplicate event-key TypeError;
it remains a failed acquisition. A real-file red/green regression fixed that
scratch logging bug. Trial2 completed naturally with clean ownership receipts,
strict provenance and all phases. Six complete windows per phase measure about
0.724x in OFF1, ON and OFF2, with 38 raw overruns. The observer runtime bundle
alone does not explain this image’s deficit; layout and input/platform causes
remain unlocated. Delivery was not exercised. Independent evidence review is
underway; no Kata closes.

Next use the same owned acquisition with a bounded 90-second duration for the
archived eb38c65 default and current bacdecb default. Real default feature receipts
remain separate from the strict 300-second crossover oracle. These are diagnostic
comparisons, not new audio acceptance gates. No extra firmware instrumentation
until that comparison establishes whether the image separation repeats.

The independently reviewed 97bk canonical-stage evidence plan now has a clean
Sol 6.1 implementation worker. Its scripts/tests are independent of the frozen
firmware and scratch acquisition. Heavy checks wait until hardware measurement
is idle. Root replacement remains unavailable; durable checkpoints and fresh
worker contexts continue under the recorded exception.

The 97bk local evidence unit passes independent review and all 27 canonical
stages with complete per-stage receipts and stable boundary snapshots. Real
negative controls cover command failures, evidence failures and interruption.
Mac Bash 3.2 runtime remains unavailable; the reviewed code uses its supported
descriptor syntax. The hosted and hardware gates stay open.

The matched 90-second default observations both completed cleanly. Archived
eb38c65 generates 0.961316x with one raw overrun; current bacdecb generates
0.786533x with eight. Exact images, fixture, helper hashes and routing receipts
remain preserved. Independent paired-evidence review is underway. Current
unified default remains installed, stopped MP3/Medium, autoplay/recognition off;
source and UART ownership cleaned up. Next compare representative real WAV
input through a throwaway host Shine work-count experiment before changing
target instrumentation. This cannot reproduce target cursor/reset history.

Kata 2qez fixes actual listener peer extraction from the doubled firmware logger
prefix. The retained-line regression failed before the two-expression fix;
independent review and all 27 canonical stages pass. Actual stage receipts are
target/check-logs/run.CNK4yPju, with complete status and equal boundary snapshots.
The real WAV input trial still fails: current default delivered only 2,160 stereo
frames (45 ms) before its listener fell behind. Acquisition and cleanup completed;
this is not a valid eight-second host benchmark fixture. The corrected parser
does not change that verdict. Archived default is now flashed, with the matched
WAV trial next. Preserve the checker revision difference in that comparison.

The host Shine work-count experiment is reviewed and ready for valid WAV inputs;
no real capture comparison has run. Priority starvation remains a hypothesis:
the encoder's periodic delay does not guarantee listener service, and current
evidence lacks per-chunk wakeup timing. Do not change priorities on that basis.
Root replacement remains unavailable; this checkpoint and fresh Sol 6.1 workers
continue under the recorded session exception.

The 8s9m refusal-write prerequisite is verified: both paths now use one response
deadline with the unchanged two-second allowance. Real-socket legacy RED and
GREEN, independent review and all 27 canonical checks pass (run.jurAgEJG).
This does not implement concurrent admission or prove target scheduling bounds;
8s9m remains open. Parent will next extend the shared acquisition state validator
for explicit FLAC input collection, with temporary saved format restoration and
no Sonos Play. The earlier failed WAV inputs remain unusable as eight-second
fixtures; no downstream host work comparison has run.

Both MP3-selected WAV attempts now have independent evidence review and durable
reports in metrics-state-wav-input.md/.json. Archived also truncated (275 ms),
so neither is a valid eight-second fixture. Weighted encoder rates remain
0.785716x current versus 0.959784x archived, with eight versus one raw overruns.
Both owned acquisitions completed naturally and cleaned up; the board remains
archived unified default, stopped MP3/Medium. This closes no audio acceptance gate.

The explicit expected-format acquisition extension passes focused tests,
independent review, fresh-eyes and all 27 canonical stages (run.OAqOjceh).
Shared require_state remains the source of truth; FLAC is limited to the strict
90-second WAV acquisition. Next: parent selects saved and active FLAC while
stopped on the archived image, collects the input, then explicitly restores MP3
even after uncertain command delivery. If input fails, stop this collection route.

The archived FLAC-selected attempt completed its acquisition but its WAV input
failed after 1.465 seconds. Independent review verifies 0.999743x FLAC generation
and zero observed encoder overrun/reset, separately from the listener's ring loss.
Reports: metrics-state-flac-wav-input.md/.json. Parent reaped session 10016,
verified no source/UART owner, and restored saved and active MP3/Medium with the
complete original config. Sonos remains stopped. This ADC-fixture route is closed;
do not run a current-image FLAC counterpart or accept partial bodies for host work.

Next diagnosis: four monotonic readings around the three existing Shine phases,
under a diagnostic feature with unchanged normal allocation and audio contracts.
Keep existing queued metrics export/reporting cadence and interrupted tails as
incomplete evidence. Source provenance review must distinguish the frozen eb38c65
and bacdecb contexts from current HEAD; rate separation must survive instrumentation
before interpreting phase differences. No cause, delivery pass or broader Kata
completion follows from these measurements.

The reviewed phase plan is now 2026-10-10-mp3-phase-timing.md. Exact source
attestation is missing for both saved images despite matching partial hashes.
A Sol 6.1 worker is preparing copied git-archive reconstructions with secure local
config copies and isolated outputs; no worktrees or original-image mutations.
Parent must first build and measure these uninstrumented baselines. Timing code
remains unimplemented until their rate gap reproduces. Hardware is idle, stopped
MP3/Medium on the archived unified default, with no source/UART owner.

Both reconstructed baselines now built and passed their linked IRAM checks.
The completed 90-second observations measured 0.999482x archived and 0.999629x
current, with zero encoder overruns. This fails the historical rate-gap
reproduction gate: phase timing remains unimplemented. Independent review and
artifact checks support reconstructed-default-controls.md/.json; startup capture
faults, false device attestation and the unarchived parser dependency stay explicit.

The separate current reconstruction delivery trial failed; final decoder files
contain only 2.064 and 2.256 seconds. Its wrapper stopped before the slow-reader
phase and cleaned up. Reviewed evidence is in reconstructed-listener-failure.md
and .json; the earlier summary's 2.184-second snapshot remains unchanged.
Current reconstructed unified image
626456a3 remains installed, stopped MP3/Medium, with no source/UART owner.
Kata ce69 tracks a confirmed reporting bug: producer generation resets and actual
listener history loss both report FellBehind. A Sol 6.1 worker is implementing the
small diagnostic correction; stream termination and recovery policy stay intact.
Plan 418609e passed independent review and a fresh Sol 6.1 worker is implementing
its real ring/pipeline/telemetry/parser regressions before the production change.

ce69 implementation now passes independent code review, all 27 canonical stages
(run.OwbY8PAR, complete/equal boundary snapshots), and the final 22 harness tests
after a test-only log-fixture correction. Actual image fd8e7471 emits explicit
generation-change departures for both peers, followed by close/release records
with zero export loss. Device evidence passed independent review and is retained
as listener-end-reasons-device.md/.json. Audio/acquisition still fail; slow-reader
acceptance remains unrun. Source/UART cleanup and stopped MP3/Medium are verified.
Next diagnosis must distinguish blocked send time from runnable scheduling delay;
the failed historical rate-gap reproduction is not reopened by this reporting fix.

ce69 is closed with commit e6a9b58 and scoped device evidence. The exact recorded
22-test closure command also passes in closure-test-command.log. A fresh Sol 6.1
worker is checking pinned scheduler observation seams before proposing the next
bounded measurement. No new tracing or priority change is implemented. The
landed phase is checkpointed here; root replacement remains unavailable, so work
continues with fresh worker contexts under the recorded session exception.

The next bounded transport measurement is ON-only frame-write task-state
sampling, reviewed in 2026-10-10-listener-frame-state.md. Existing supervisor
samples two fixed listener identities; no scheduler hooks or runtime crossover.
Counts are observations, not time accounting. The first independent review
requires coherent immutable publication and actual-toolchain lifetime proof;
implementation and device acceptance remain pending. Sonos documentation now
confirms a local WebSocket join workflow, but exact join messages and stale
session write rejection remain unproven; pqqq stays open. Root replacement
remains unavailable; this checkpoint and clean Sol 6.1 workers retain continuity.

The frame-state diagnostic is committed as 23bd686 with 20 targeted tests,
all 27 canonical stages (run.cP1QYu5Y), a feature-enabled target build and
independent source/linked inspection. Ordinary and diagnostic hardware trials
both fail audio: final decoded prefixes are 2.640/2.736 and 3.624/3.144 seconds.
The diagnostic freezes one complete, invariant-valid receipt with 174 Ready
observations, 96 at observed write ages >=100 ms. Both owners remain live at
expiry; counts are qualitative observations, not wait-time fractions.
Ordinary production is slower and loses raw history; diagnostic production
stays near real time but both listeners lose encoded history. The image change
therefore cannot isolate observer cost or prove a scheduler cause.

Source/build/artifact hashes and single-boot provenance were rechecked; both
trials cleaned up and fresh state is stopped MP3/Medium, autoplay/recognition
off. Diagnostic image 5ddee0ff remains installed. The next bounded proposal
tests listener-only priority 7 versus 8, preserving encoder priority and all
audio gates. Scoped design review passes; production policy is unchanged. Evidence lives
in listener-frame-state-device.md/.json under docs/audit-evidence/2026-10-10.

The listener-only priority experiment passed canonical27 stages (run.PKNzkEkS)
and independent source/linked review. Exact A/B executable bytes and allocated
placement match; only priority data and source-line metadata differ. B1 cuts
send maxima to 19.415/11.896 ms from 441.001/436.093 ms, but both streams still
fail after encoder raw loss. Final decoded totals are 4.944/4.752 seconds.
Pre-admission encoder rate remains about 0.867x. The approved early-stop rule
ends this series without A2/B2/A3 or a production priority change.

The source override is removed; its patch and reviewed evidence are retained
as listener-service-priority8.patch and listener-service-experiment.md/.json.
Candidate dc908452 remains deployed and stopped MP3/Medium; source/UART cleanup
and fresh settings are verified. A clean Sol 6.1 worker is planning a bounded
benchmark of the current shared encoder wrapper and allocator. The older
benchmark's wrapper and build flags differ, so its results cannot certify the
current 24 ms frame budget. No historical rate attribution or ADC fixture
collection is reopened.

The current shared-wrapper MP3 cost benchmark is complete and independently
reviewed. All 27 canonical stages pass (run.GyzqljgW), the explicit example
build/linked inspection passes, and all 20 parser tests pass. The example links
the exact production C archive af005426. One complete bounded device acquisition
measures repeated-block means 10.045/12.870/16.897/18.182 ms for zero/DC/tones/noise;
all 512 measured calls fit 24 ms, with minimum 5,540 bytes acquisition stack free.
This establishes isolated fixture headroom, not live input or application
headroom. Live audio, throughput and radio root-cause gates remain open.

The temporary example is archived as evidence and removed from the build tree.
Exact candidate dc908452 is restored, fresh stopped MP3/Medium settings are
verified, and no UART/aplay source owner remains. Public evidence lives in
current-mp3-cost.md/.json under the October 10 audit directory. At that checkpoint a clean Sol 6.1 worker assessed the next bounded measurement;
the completion below supersedes that historical pending state.

## MP3 input repetition complete — October 10, 2026

The approved cbe4b0a diagnostic ran one complete parent-owned boot. Repeated-A,
fresh and repeated-B each completed 128 measured calls after separate warmups;
means were 18.546359375 / 18.518 / 18.546859375 ms, maxima all 18.768 ms, with
zero late calls or encode failures. Independent expected input identities match,
including fresh state through warmup plus 128 measured blocks. Minimum
acquisition stack free was 5,636 bytes. One fresh phase cannot prove
repeatability or causation.

All 27 canonical stages pass (`run.LZpbFw91`), explicit example format/build and
independent implementation/build reviews pass, and 29 parser tests pass. Exact
production C archive af005426 is retained. Raw receipts and parent postrun proof
confirm acquisition/restoration success, empty errors, unchanged source/artifact
hashes, stopped MP3/Medium with autoplay/recognition off, and clear UART/known
aplay guards. Exact frozen B dc908452 is restored; source remains listener
priority 7 while B retains experimental priority 8. No image attestation or
full-flash readback is claimed. Expected debug cc detection and SDK I2C warnings
remain visible in evidence.

Public mp3-input-repetition.md/.json and exact example/parser/tests are archived
under the October 10 audit directory. Independent evidence review accepted
without findings
(`target/kata-hardening/mp3-input-repetition/evidence-review.md`). Parent
verified all three public copies and the temporary example byte-for-byte, then
removed only that temporary build-tree example; cleanup is complete. All means
fit, so isolated-input exploration ends; failure-only branches were not taken
and no additional matrix/probe follows automatically. Live audio, ADC, radio
root cause, capture/recovery, hosted/HIL and endurance acceptance remain open.
The overall Kata goal remains active.

Next selected work is independent concurrent admission 8s9m, accepted plan
6e39a66, Task 1 in progress with a fresh Sol 6.1 worker. This records work in
progress, not an implemented admission fix or target acceptance. Compaction
count is 18; the parent retains the established landed-phase/root-replacement
checkpoint obligation.

## Concurrent admission implementation checkpoint

Task 1 extraction landed as 3dfbc27 with the slow-first real-socket RED/GREEN.
Task 2 now launches exactly two persistent gated workers, preserves admission
and close ownership, and exits waiting workers if startup fails. Independent
reviews pass without findings. All 39 targeted tests and all 27 canonical
stages pass (`run.QSVJJTto`), including the actual firmware build and IRAM audit.
No device was flashed for this change yet. Task 3 will check real requests and
measure the additional worker; audio continuity and the Kata 8s9m acceptance
remain open. The board remains stopped MP3/Medium on the existing candidate.

## Concurrent admission device and resource checkpoint

Both actual request trials failed; reviewed public evidence records the partial
successes and preserves the failures. Trial 1 has 12 encoder overruns; trial 2
has a replacement stream that ended after its HTTP 200 header. Keep 8s9m open.
The board now runs the default two-worker image 01d2607f71524fc7b980fc37cc3516aa1f0e987f5ea0352bd2f3dca7bf113f04,
with saved/active MP3, Medium, stopped playback and recognition/autoplay off.

The default-off permanent-worker resource diagnostic passed all 27 canonical
stages in run.uBzY2Yyr, an explicit feature build, source review and linked-code
review. At that checkpoint its two-worker image was unflashed; the matched
idle/stream measurements follow below. TLS overlap and failure injection
remain separate outstanding gates. No blind request-suite rerun follows the
header-only failure. Root-session replacement is unavailable; this checkpoint
and fresh Sol 6.1 workers preserve continuity without claiming completion.

Matched one-/two-worker resource runs are now complete and both failed audio.
Each retained 14 samples and a 150-second UART boot; neither reached the
pending-head workload. Idle free internal RAM differed by about 4.9 KB; sampled
task counts were 19/20, and cleanup retained no owned sockets. Historical stack
watermarks and query timing are partial observations, not full resource safety.
The default two-worker image 01d2607f71524fc7b980fc37cc3516aa1f0e987f5ea0352bd2f3dca7bf113f04
was restored, settings verified, and all process/UART owners reaped. Preserve
both failed verdicts and review the evidence before selecting the next bounded
diagnosis; do not automatically repeat either workload.

Next bounded host work: collect pending-head resource evidence in a separate
phase before compressed streams, since known generation resets prevented the
existing sequence from reaching that phase. Keep combined-load and aggregate
failure verdicts intact; a pending-only pass cannot clear audio or TLS gates.
Test phase receipts and cleanup with real socket behavior before any new device
run. The live-versus-isolated MP3 cost discrepancy still needs a discriminating
cause; neither these resource results nor the completed input-repetition check
justifies changing priorities, codec settings or buffers.

The disposable pending-head runner revision now passes 15 host tests and an
independent scoped review. A real subprocess SIGTERM regression first showed
the combined stream phase starting after interruption; a dedicated interruption
exception now preserves the phase receipt and reaches outer cleanup instead.
The prior runner/test bytes were archived before editing. The revision has not
run on hardware, and none of the saved failed acquisitions used it. Current
runner SHA256: 0869333d5a5c8deadc9ea559f2a5358e42f631f236b15153c8d1f188288c2a00.

Independent dashboard investigation found no pipeline counters or freshness in
the existing state API. A network snapshot proposal is retained at
`docs/superpowers/specs/2026-10-10-device-metrics-design.md`; it copies existing
numeric observations from the low-priority metrics owner, outside source locks,
and keeps missing boot identity/live stage measures explicitly unavailable.
Doctor Biz was asked the network-versus-USB transport preference asynchronously;
network is recommended. This is design evidence, not implementation or service
deployment. No new endpoint, host service or Grafana instance is running.
