# Gotchas

Measure preset history using ring head and selected cursor under the same lock.
The first delivered receipt is one-based: head minus first sequence plus one
equals retained frames. Concurrent payload overlap includes admission skew and
cannot substitute for this receipt. October 13 verified MP3 42/167/334-frame
selection; this proves source history, not receiver playout protection.

Device metrics network/settings fields are deliberately optional nonblocking
snapshots. Null is unavailable evidence, not a disconnected or unchanged lease.
Record gaps and obtain a fresh snapshot within the existing test deadline;
never substitute cached ownership metadata or retry a known contradiction.

Listener loss receipts follow event batches, not a periodic clock. During
control-read cancellation, Stop can close and export the stream before the
stalled read returns. Require a clean export whose valid event range covers
the matching owner's close; demanding an export after stall-end creates a
false failure. Drain complete lines through that receipt. Keep original failed
acquisitions separate from offline evaluation of their complete retained data.

HTTP recovery cannot infer restart authority from matching URI alone. An owner
can reuse a URI, and a Stop can leave it unchanged; even atomic URI comparison
would need revocation semantics to cover those cases. These are contract limits,
not new observed Sonos behavior. The Victrola archive contains reconstructed WSS
playback and read-only receipts, not a plaintext conditional-restart transaction.
See October 13 recovery-protocol evidence before reopening that investigation.

Channel diagnostics expose the last completed measurement, not current capture
validity. Age starts when capture finishes the meter push, keeps its original
timestamp through store contention, and grows through a reset until a new
interval replaces it. RMS removes each channel's DC; peak includes DC. Digital
rail counts cover the rolling window and do not prove analog clipping. Physical
channel routing and jack-to-PCM gain still require a calibrated source.

Doctor Biz confirmed on October 12 that no AP access is available. Record the
unavailable AP acquisition explicitly; pd6c allows that investigation outcome.
It does not establish a transport root cause or complete recovery acceptance.
The workstation's default route to the board flips with interface metrics:
Wi-Fi on October 12, wired enp5s0 (src .153) again on October 13 with Wi-Fi
still up. Pin and record the receiver interface for comparisons; do not reinterpret
the earlier wired traces as wireless.

Doctor Biz approved checked Stop and hardware playback testing on October 11:
"proceed with board tests. you can play sonos, etc". Do not ask again for that
choice or routine Sonos playback tests. The check/Stop source-switch race remains
documented; this does not authorize blind automatic Play or SetURI after loss.
Use the front headphone jack source, leave mixer/volume settings alone, and
restore stopped MP3/Medium with autoplay and recognition off.

The Sonos board API can report Stopped when intent revocation occurs, before
the worker completes its checked remote Stop. Hardware gates must poll the
same speaker/URI until remote STOPPED, with a bounded deadline, rather than
asserting remote completion after the first local Stopped response. Re-discover
selected groups after UART open resets the board. See sonos-http-board evidence.

Report wmsy as the parent audit, with child progress and the central outcome
separately. Doctor Biz flagged that an open epic looked unstarted: on October
11 it had 21 closed children and 17 open children; the project-wide 18 open
count included the parent. These are historical counts, not a live dashboard.
Supporting closures do not establish automatic ownership-safe playback recovery.

A Sonos control-read timeout must not revoke healthy audio before classifying
the failure. Retain the original queue during bounded read-only recovery; this
grants no authority for Play or SetURI and cannot cross a network generation.
The production timeout-cleanup negative control must fail the retention test.
See the October 11 sonos-control-recovery plan and evidence.

The later TLS diagnostic resolves one hidden failure: RSA returned -0x4290
(RSA_PUBLIC_FAILED + MPI_ALLOC_FAILED), then X.509 mapped it to NOT_TRUSTED.
Heap reported after unwinding is not allocation-time headroom. Lowering the
general threshold from 8191 to 2047 increased observed setup free RAM but still
failed native TLS; that trial was reverted. Internal TLS allocation remains.
Doctor Biz superseded the external-TLS allocator proposal by selecting HTTP/UPnP
Sonos control. Do not resume that experiment or weaken unrelated HTTPS settings.
See tls-verification-memory evidence and the October 11 sonos-http plan.

Cargo's ESP-IDF rebuild reset the temporary SDK diagnostic edits. Apply such
throwaway patches after the IDF build, rebuild the affected native libraries,
relink the firmware, and verify the exact ELF contains the diagnostic strings
before testing. Preserve original hashes and restore both sources/libraries.
Do not call an image diagnostic merely because its build command succeeded.

The October 11 permanent-listener-worker candidate passed host/firmware checks
but failed real Sonos contention before any Sonos listener joined: native TLS
returned -0x2700 (the pinned header's certificate-verification failure). Detailed
verification flags were not recorded; neither invalid credentials nor memory
exhaustion is established. Keep that failed gate separate from the prior
per-connection thread ENOMEM. See listener-resource evidence; do not claim
target acceptance from debug-client displacement or repeat an unchanged run.
Its failed driver cleanup needed a separate scoped Stop before idle MP3 restore.

ESP32 UART core decoding needs debug types in the exact archived ELF. A stripped
release ELF made the pinned decoder wait repeatedly on failed GDB TCB_t queries.
Use command-local CARGO_PROFILE_RELEASE_DEBUG=2 for diagnostic and baseline builds,
preflight the ELF type lookup, and capture a fresh matching dump. Raising the
timeout does not repair missing types. Retain normal and diagnostic build caches;
checks must identify the configuration used by Cargo, not assume only one exists.

The October 10 matched MP3 link test restored live encoder keep-up by placing
Shine C instructions/literals in IRAM: flash control ran at 0.725x with raw loss;
IRAM kept up with zero/one/two listeners and no added capture faults. The normal
build produces the exact tested ELF. Cargo bundles C objects inside the MP3
rlib, so selecting the standalone C archive in a linker script does nothing.
Keep tables and Rust wrapper placement unchanged. This is a bounded placement
correction, not proof of a specific cache cause or long-term transport health;
joint delivery still measured about 98.9% of real time. See mp3-instruction-placement
evidence before repeating the resolved input/placement investigation.

Doctor Biz flagged prolonged work without fewer open Katas. Finish tickets
against their original acceptance criteria; do not let extra diagnostic gates
or unrelated audio failures keep completed work open. Reconcile implementation
and evidence before starting another investigation. Keep genuine failed gates
open, and never redefine scope or close tickets just to improve the count.
Doctor Biz repeated this concern during 45k9. Record a close-or-block decision
after the bounded acceptance run; do not turn each failed gate into another
unbounded investigation under the same ticket. Count closures separately from
code changes and passing checks.
Doctor Biz repeated this after the TLS fixes and CI split left 18 open. Keep
one executable closure target in progress; finish its original acceptance or
record the exact unmet gate before choosing another. A review, diagnosis, or
supporting commit does not count as a closure. Do not answer the same correction
with another broad audit or a fresh promise unaccompanied by implementation.
Do not give every agent a branch of the same hardware investigation. Keep an
independent implementation moving when its acceptance does not depend on that
failure. Report remaining acceptance gaps explicitly; another diagnostic run
is evidence, not a completed ticket.

The October 10 concurrent-admission trials both failed. One exposed a corrected
host-driver reset-handling defect; the other returned the exact MP3 HTTP 200
head then closed without payload on a replacement stream. Preserve the failed
verdicts even when most cases pass. The first trial's full UART has 12 encoder
overruns; its last periodic summary alone undercounts the tail. See the saved
concurrent-admission-device evidence before retrying or assigning a cause.

The matched admission-resource boots also failed compressed playback before
their pending-head phase. UART listener receipts say the encoded-ring generation
changed after encoder overruns; they do not identify the overrun's root cause.
Ten-second task snapshots missed the brief live listener overlap. Separate
admission-only evidence from combined-load acceptance instead of repeating a
sequence whose known audio failure prevents the intended measurement.

The October 10 listener frame-state diagnostic captured 174 Ready observations
inside payload-write brackets, including 96 at observed ages >=100 ms. Ready
includes sampler preemption; these counts cannot measure runnable wait time.
The diagnostic encoded near real time while the ordinary control lost raw
history, so comparing the images cannot isolate sampling overhead. Both failed
audio. See listener-frame-state-device evidence before changing scheduling.

The separate priority-7/8 experiment holds executable bytes and allocated
placement fixed. Its one priority-byte change reduced observed send maxima to
12–19 ms, but encoding still lost raw audio and disconnected both listeners.
Only one A/B pair ran; no production priority change landed. Reuse the saved
listener-service-priority8.patch for any later combined assessment. Historical
MP3 cost numbers used a different wrapper/build setup; measure the current
shared encoder before treating those numbers as its performance budget.

Mise 2026.9.15 accepts Rust's llvm-tools-preview install alias but can keep
reporting the toolchain missing afterward. Pin the canonical llvm-tools component
name; it selects the same bundled LLVM. Verify clean activation and exact rustc,
llvm-cov and llvm-profdata versions before trusting coverage tooling.

October 10 cold MP3 controls reproduce delivery failure on both archived and
diagnostic unified images. The diagnostic image also encodes at about 0.95x
before listeners arrive and incurs raw overruns despite unchanged capture
fault counters. Do not blame active socket probes for a zero-listener slowdown
or accept better receiver delivery while encoding loses audio. See sender-controls
under docs/audit-evidence/2026-10-10 and Kata wbz6.

ESP-IDF 5.5.5's newlib poll header defines POLLNVAL as bit 7 (128), but
libc 0.2.190 omits that constant for ESP-IDF. The generic lwIP fallback uses
a different value and is not this target's ABI. Check the platform header
and generated IDF bindings; host compilation cannot catch missing target symbols.

Doctor Biz corrected automatic FLAC restoration on October 9: this board is
actively iterating on MP3. Leave the current experimental MP3 firmware deployed
between tests. Preserve stopped playback and settings, but do not confuse
cleanup with firmware rollback. Keep the saved FLAC ELF for recovery; restore
it only when requested or needed to recover a failed board. The unified build
contains both codecs; missing saved settings default to FLAC while this board
keeps MP3 selected.

Runtime format switching must revoke the original Play token on Stop, including
the final Sonos create/load send. Stop acceptance can wait for that bounded send;
the browser's eight-second POST budget covers its five-second deadline. A browser
timeout means the outcome is unknown. Every Play retry waits for listener capacity,
even if a canceled earlier Play already committed the requested codec.

The unified-image MP3/FLAC/MP3 comparison on October 9 reproduced both MP3
listeners overrunning in both repeats, while capture and encoding stayed current.
FLAC kept its sockets for 90 seconds but delivered only about 96% of real time
during overlap. Socket survival is not a throughput pass. Preserve actual joint
and survivor-only intervals whichever listener ends first. See
docs/audit-evidence/2026-10-09/runtime-codec-matched.md.

Pinned IDF 5.5.5 lwIP with TCP/IP core locking disabled posts socket API work
to a mailbox and waits on a completion semaphore even for nonblocking sends.
Long send-call wall time does not by itself prove send-buffer pressure. Measure
accepted bytes, send-call time and WouldBlock/poll time separately before changing
priorities or Nagle. The generated build uses MSS 1440, not an assumed 1460.

Doctor Biz corrected stopping after the MP3 benchmark on October 9: the standing
instruction was to start and continue the implementation. Treat the benchmark
as a checkpoint when the follow-on prototype is already authorized; do not turn
a recommendation into another permission request or a reason to end work.

The October 9 live MP3 prototype supersedes the benchmark-only status below.
Inline 4,608-byte PCM storage caused a controlled main-task stack overflow;
construct it on the heap. A 16 KiB encoder stack starved Sonos TLS allocations;
8 KiB left 5,924 bytes free in measured runs. A busy atomic publication is Retry,
not empty audio: bounded rechecks avoid a full 10 ms sleep on each collision.
Priority 8 kept MP3 encoding current, but encoded-ring overruns still ended High
Sonos and two-listener streams. Keep MP3 opt-in and preserve these failed gates.
See docs/audit-evidence/2026-10-09/mp3-stream.md.

The staged MP3 test on October 9 showed why whole-run average throughput can
mislead: listener 1 ran near real time alone, slowed to about 71% while a second
was connected, then caught up after listener 2 overran and closed. Its full
post-join average was 99.9%. Report actual joint-connection and catch-up windows
separately, with admission skew and telemetry sampling limits. Capture/encoder
progress stayed current; the test does not locate the failing network hop.
See docs/audit-evidence/2026-10-09/mp3-staged.md.

The October 9 Shine benchmark kept up on average at 48 kHz stereo 320 kbps
with capture/TLS only, but concurrent production FLAC caused growing backlog.
The corrected exclusive run still had a 508 ms encode call. MP3 input was
synthetic; no ADC-to-MP3 transport or Sonos path was built. Keep these limits
with the 53% music-payload saving; lower bandwidth is not a proven radio fix.
See docs/audit-evidence/2026-10-09/mp3-benchmark.md and Kata e8hd.

Pinned IDF 5.5.5 implements sub-tick usleep with esp_rom_delay_us. Short Rust
sleeps can busy-wait instead of releasing the core. Use explicit vTaskDelay
in timing probes, record late deadlines separately from final backlog, and
retain original runs when correcting pacing. A zero final backlog can hide
a half-second stall; task runtime includes charged interrupts.
When adapting virtual wake times, preserve the production full sleep interval.
Subtracting wrapper overhead from a 10 ms wait can turn it into a sub-tick busy
wait. Host virtual-clock tests do not verify this ESP-IDF scheduling behavior.

Doctor Biz chose Low 1 s / Medium 4 s / High 8 s startup history, default Medium,
applied on the next Play. Actual whole-frame offsets are 1.024/4.010667/8.021333 s.
The October 8 hardware check matched live and buffered PCM exactly and verified
real Sonos queue immutability and Stop. This proves source history, not receiver
playout protection. The same test reproduced a capture restart during Sonos setup;
do not present the preset feature as a capture or network reliability fix.

Doctor Biz confirmed on October 8 that the ESP32 LINE IN cable uses the
workstation's front jack, labeled Headphone by ALSA. For board-only input use
`plughw:CARD=PCH,DEV=0`, Headphone on and rear Line Out muted; Master and PCM
at 0 dB per the source-gain preference. Rear Line Out jack detection was off,
front Headphone detection on. Muting Headphone silences the board's source.
The verified 997 Hz left / 1703 Hz right fixture arrived as 1703 Hz left /
997 Hz right; channel reversal is an open calibration finding, not a buffer fix.

When investigating unexpected sound, inspect original playback commands before
attributing it. On October 8 this session launched a 180-second Solomun DJ excerpt
through `aplay -D plughw:CARD=PCH,DEV=0` at 09:23, 09:28 and 09:45 Chicago time,
as well as later test tones. The claim that only tones were launched was wrong.
The later packet harness writes framehash files, not speaker output; the source
of music reported during that later run remains unproven. A sandbox-local `ps`
cannot exclude host audio players. Verify process visibility and physical output
routing before claiming no player exists or starting another audible test.

Check `docs-ignore/victrola-hacks.tgz` before proposing Victrola-like buffering.
Doctor Biz supplied firmware and live-test notes there. On the tested Onyx build
0.43.179.0x2aa3460, wirelessAudioDelay passes to adchls:start as adchlsLatency;
only high was live-tested, and delay durations/buffer sizes remain unproven.
The documented native Sonos route uses 320 kbps MP3 at the highest quality setting;
its separate direct FLAC endpoint is Ogg-wrapped 24-bit FLAC. Do not assume its
Sonos reliability demonstrates the same bandwidth or buffering as our FLAC path.

On this Linux workstation, invoke serial-backed harnesses with
`ESPFLASH_PORT=/dev/ttyUSB0`; `scripts/serial-log` otherwise defaults to a macOS
device path. A collector failure before stream admission is an invocation failure,
not transport evidence. Doctor Biz's report of music from workstation speakers
does not establish what signal reaches the board's LINE IN; record routing as
unverified until the ADC measurement or physical path establishes it.

Known live FLAC/WAV endpoints must select their input demuxer before FFmpeg
startup checks. Constant production FLAC can take over 12 seconds to fill the
auto-detection probe despite timely audio delivery. Keep sample, CRC and
discontinuity checks unchanged; this fix does not explain ring-overrun failures.

Doctor Biz later confirmed LINE IN is connected to LINE OUT during the October 8
capture-publication acceptance session. Treat this as the workstation connection
discussed earlier. A later decoded prefix verified driven tones, and standalone
settling acquired ten complete trials. The short network capture itself failed;
tone presence does not clear continuity or calibrate gain. Earlier quiet-input
results remain valid only for their recorded signal conditions; the cable's actual connection time is unknown.

## Atomic capture history evidence — 2026-10-08

Raw capture publication now has one unique publisher and atomic sample payloads.
Readers validate stamps, generation, retained range and era after copying; Retry
scratch is unusable. Keep the actual protocol in concurrency tests: sequence
checks around non-atomic payload would still be a Rust data race. Bounded Loom
models support the ordering argument but do not prove arbitrary target schedules.

On the pinned Xtensa build, SeqCst sample operations use direct loads/stores and
MEMW without atomic helper locks. Linked read/publish still call ROM unsigned
64-bit division/remainder; those are arithmetic, not 64-bit atomics. Objdump can
render valid Rust text as data because of Xtensa property metadata. Inspect a
separate scratch copy and verify unchanged text bytes; never flash that copy.

Delayed-reader fixture events are retrospective: acquisition records numbers,
releases buffers/ring ownership, waits for recovery, then logs with bounded yields.
Match the event timestamp, not UART arrival, to capture counters. A failed soak's
surviving decoder can keep running after its summary snapshot; final retained
framehashes establish its eventual samples. Preserve the original failed verdict.

## Keep executing authorized work — 2026-10-08

Doctor Biz corrected another premature stop at a batch boundary. A committed
phase is a checkpoint, not permission to end while authorized acceptance work
remains. Update the plan and Kata, continue the next useful experiment, and stop
only for a real blocker or completion of the requested scope.

## Capture protection measurements — 2026-10-08

Retain the original UART collector after a failed soak: reopening this board's
serial port can reset it even with DTR/RTS configured before open. The optional
stream-soak failure observation keeps recording but leaves summary counters as
explicitly aged pre-failure snapshots. Zero stale deltas cannot clear capture.

The TLS fixture reproduced three capture overflows/restarts while all TLS cycles
completed. `raw_read_max_us` measures the encoder's raw-ring copy, not I2S driver
read time. Keep that distinction when tracing long capture service intervals.
Physical ALSA RUNNING also does not prove the signal reaches LINE IN: the attempted
music run still measured only noise at the ADC. Verify the board signal too.

Long diagnostic UART dumps can starve the idle task even after acquisition ends.
Explicitly yield between bounded batches; lowering priority alone is insufficient.
Retain watchdog-contaminated runs and reject incomplete/interleaved records.

TLS diagnostics reproduced two distinct capture losses. One run added DMA queue
overflows/restarts; another kept those counters fixed but lost 4,320 staged frames
while an encoder raw-ring read held its mutex for 110,567 us, including preemption.
The latter is direct evidence for removing reader-held locking from publication;
it does not explain or fix the former. See capture-setup.md/.json in audit evidence.

Cross-core task probes have costs and limits: the global tick advances on core 0,
task lookup/state queries take the kernel lock, and current-task identity does
not identify ISR execution. Actual capture priority was 10. Long observer gaps
also occurred in clean cycles, so do not equate them with capture starvation.

Identically instrumented IRAM-off controls measured capture-task runtime at
16.63% with unconditional FIR versus 8.40% with recognition disabled correctly,
using two 45-second FLAC listeners on quiet analog input. Runtime includes charged
interrupts; the read-service delay estimate also includes work and preemption.
Both retained startup capture faults. Do not turn the CPU saving into a claim
that TLS/setup capture stalls are fixed.

Pinned IDF I2S IRAM safety needs both IRAM interrupt registration and internal
driver objects; callback placement alone is insufficient. One- and two-listener
90-second tests each survived 120 verified NVS writes without additional capture
faults. Exact decoder samples do not prove every DMA interrupt arrived. See
docs/audit-evidence/2026-10-08/capture-protection.md for scope and hashes.

Settling counters count discarded whole chunks, including interrupted budgets.
Restart counters count attempts, and supervisor/overflow causes can coincide.
The fault fixture measured 24,000 extra settling frames after recovery, but its
stream never exposes those discarded samples. It cannot justify shortening the
500 ms policy; cold codec setup and isolated I2S restart need separate physical
measurements before changing that policy.

## Linux workstation validation — 2026-10-07
Doctor Biz's correction: a failed diagnostic is a step in the investigation,
not a stopping point. Continue useful, authorized experiments and root-cause
work rather than ending each turn with a measurement and another proposal.

Remote-session changes can remove physical audio access: the analog card still
exists in /proc/asound, but its ACL may grant only the local greeter. PipeWire
then exposes Dummy Output; pw-play can silently route there even with an absent
named target. Check physical sink existence and actual playback links before
calling a run a cable/music test. A live player process alone proves nothing.

After restoring the analog control/playback ACLs, PipeWire still exposed only
Dummy Output in the remote session. Direct ALSA plughw:CARD=PCH,DEV=0 playback
ran on the real device and the two-listener120s FLAC retry passed. Verify the
PCM RUNNING state and board level. This repairs the test audio path; a single
pass with unchanged firmware does not explain the earlier network failures.

Operator scripts must not assume the agent's PATH. The first correlated-capture
runner used `rg`, which existed for the agent but not Doctor Biz's terminal.
Use standard `grep` for these simple checks and validate dependencies before
route checks or privileged work. Missing tools are not evidence of a bad route.

The workstation's `eno1` profile is link-local only; plugging into it does not
give a LAN route. `enp5s0` uses DHCP and acquired 192.168.23.153. Verified board
traffic used Ethernet before and after the two-listener comparison. It still
failed: a status request timed out at 49.424 s, with only 17.493/16.725 s decoded.
The harness then stopped the streams; the UART peer reset is cleanup, not a
natural ring-lag disconnect. This verified the Ethernet outgoing route only;
Wi-Fi remained connected on the same subnet, so the incoming path was not
isolated. A later interface-bound test raised an ARP-flux hypothesis. Do not
claim an Ethernet-only comparison until repeating with workstation Wi-Fi off.
Evidence: `docs/audit-evidence/2026-10-07/ethernet-comparison.json`.

The network-only comparison was initially misclassified: both runs actually used
b/g, including the intended normal-mode control. `esp_wifi_set_protocol` survived
source restoration and reflashing on this board. Verify protocol readback AND
the association's `phy:` log; matching an ELF hash does not restore persistent
device state. Explicitly restore bitmap7 for b/g/n before repeating the control.
See the corrected `docs/audit-evidence/2026-10-07/wired-radio-comparison.json`.

With workstation Wi-Fi off and b/g/n independently verified, the plain TCP
fixture still loses a peer at full rate. The paired production capture records
zero kernel drops, open receive windows and multi-second sequence holes; both
listeners naturally leave for ring lag. Shared pauses support checking the
ESP/AP boundary but do not locate the fault. Half-rate peers both finish, yet
one falls about seven seconds behind by host-second bins. A lower offered rate
is diagnostic evidence, not a verified streaming fix. Curated evidence:
`docs/audit-evidence/2026-10-07/wired-correlated-capture.json` and
`docs/audit-evidence/2026-10-07/wired-radio-comparison.json`.

The faster household SSID is separate from the 2.4 GHz network by design;
Doctor Biz cannot enable 2.4 GHz on it. The classic ESP32 cannot join its 5 GHz
service. A failed association is not a failed throughput comparison. Save a
private credential backup before temporary network changes; a saved firmware
ELF can restore the board but does not restore the editable build config.

Doctor Biz connected this workstation's analog output to the board's LINE IN and
supplied test music under `media-ignore/`. This supersedes the older Mac/turntable
input assumptions for this setup. USB identifies a CP2102 at `/dev/ttyUSB0`;
device access requires a local permission grant. The old board IP is not confirmed
on this network. Keep Wi-Fi credentials in the gitignored `firmware/cfg.toml`.

The pinned espflash release must select the host platform. A hardcoded macOS ARM64
asset installs successfully on Linux but cannot execute. Leaving asset selection
to mise installed a working Linux espflash 4.6.0. Use `mise run esp-toolchain` for
the pinned Xtensa toolchain; the host Rust toolchain alone cannot build firmware.

The updated image joined at `192.168.23.217`. All canonical checks passed after
workstation setup (188 Rust tests passed, two existing tests ignored). Physical
music playback produced valid 48 kHz/24-bit FLAC; a single listener decoded a full
120 s with unchanged post-startup capture fault counters. Two listeners failed:
one fell a ring behind after 47.7 s, reproducing transport trouble on direct local
Wi-Fi without the prior Mac/subnet-router path. This does not locate the loss.
The short pass is not an endurance or worst-case bandwidth guarantee.

The supplied DJ-set excerpt has identical left/right channels. Separate 440 Hz
left and 880 Hz right test tones returned reversed, with over 60 dB separation.
Doctor Biz confirmed a single 3.5 mm stereo cable, without RCA adapters.
PipeWire links preserve channel order and the codec/I2S settings match documented
defaults. The location of the reversal remains unresolved; do not blindly flip
`msb_right` to compensate. Evidence: `docs/audit-evidence/2026-10-07/linux-line-in-validation.json`.

A selective codec gain test localizes the reversal before the ADC: register
0x09 = 0x40 raised the stream's left channel by 12.74 dB, leaving right unchanged.
The left stream still contained the source's 880 Hz right tone. Codec-to-stream
channel mapping is correct; workstation analog output, cable and board input
wiring remain candidates. Normal firmware was restored and its ELF hash matches
the saved control. Do not change I2S ordering to conceal an analog routing issue.

Doctor Biz cannot select answers in the current TUI. Use plain chat for operator
questions rather than selectable question widgets. Verify captures from actual
processes and files; an answer alone is not evidence that a command ran.

The Linux two-listener packet trace confirms receiver sequence holes and pauses
even after ACKing all observed data. Valid receive windows never reached zero;
the two zero fields were cleanup resets. Local ACK generation is fast, but this
does not prove ACK arrival at ESP32. Only listener 2 naturally closed for ring
lag (27.9 s); listener 1 ended during harness cleanup. Initial capture lacked
tcpdump drop statistics, now retained by the script. Host receive coalescing
also means packet records are not on-air segments. Do not infer RF loss location
or sender silence from this one capture. Next discriminator: simultaneous AP
boundary captures; a wired workstation comparison could first remove one radio
leg. See `docs/audit-evidence/2026-10-07/linux-stream-packet-timing.json`.

## Confirm the exact board before pinout research
"ESP32-S3 AI Smart Speaker" matched a Waveshare S3 board, but the hardware is an Ai-Thinker ESP32-A1S Audio Kit (classic ESP32). Ask for the listing or module label first. Once USB works, confirm with esptool's chip-id / flash-id and an I2C probe (ES8388 answers at 0x10, AC101 at 0x1A).

## The Audio Kit has two micro-USB ports; only "UART" carries data
The other port is power only. macOS 26 ships a CP2102 driver (AppleUSBSLCOM), so the board shows up as `/dev/cu.usbserial-*` with nothing to install.

## ES8388 boots as I2S master with the ADC at -96 dB
Without `0x08 = 0x00` (slave) and `0x10 = 0x11 = 0x00` (ADC volume 0 dB), the input looks dead. The codec has no reset pin and survives ESP32 resets, so start setup with a soft reset (`0x00 = 0x86`, then `0x06`).

## GPIO12 is not a line-in detect on the Audio Kit
Drivers label GPIO12 "aux detect", but that constant came from Espressif's LyraT board. On this kit GPIO12 is SD card DAT2 and the MTDI strapping pin, which sets flash voltage at boot. Don't drive it. LINE IN feeds ES8388 LIN2/RIN2 (`0x0A = 0x50`).

## The toolchain comes from mise, and mise's RUSTUP_TOOLCHAIN beats rust-toolchain.toml
Install tools with `mise trust && mise install`, then `mise run esp-toolchain`. Don't use brew, `rustup toolchain install` or `cargo install`. mise exports `RUSTUP_TOOLCHAIN=1.93.0`, which overrides `firmware/rust-toolchain.toml`, so build firmware with `cargo +esp` (or `scripts/flash`). Scripts load the pinned tools with `eval "$(mise env -s bash)"`.

## Don't over-plan this project
Harper stopped a planning spiral on 2026-10-03 ("lost in the planning sauce"). Write only the plan text the next task needs, and execute.

## songrec-lib's library crate is named `songrec`
The package is `songrec-lib = "=0.5.4"`, but its `[lib] name = "songrec"`, so Rust code writes `songrec::SignatureGenerator`, not `songrec_lib::`. It also pulls about 274 crates (cpal, rodio, reqwest) into the dev-dependency tree. That's fine on the Mac, and the firmware never builds it.

## Sessions started from Ghostty can't read ~/Downloads
`ls`, `ffprobe` and `open()` on anything under `~/Downloads` fail with "Operation not permitted", while `stat` works and Desktop, Documents and Music read fine. It looks like macOS's per-app Files & Folders privacy setting for Ghostty (verify in System Settings > Privacy & Security). The test songs therefore come from `mdfind`:
- A: "P.Y.T. (Pretty Young Thing)"
- B: "Blame It On the Boogie"

Filter the results with `grep -E '\.m4a$'`, because mdfind also returns `test/fixtures/songs/clip.wav`, which carries the song's title tag. The plan's Tasks 4–12 already do this (SDD Ruling R7).

## Stock firmware backup
`backups/stock-flash-2026-10-03.bin` (gitignored) is the full 4 MB of Ai-Thinker's v1.1.0 firmware. Restore with
`esptool --port /dev/cu.usbserial-0001 write-flash 0 backups/stock-flash-2026-10-03.bin`.
- Size 4194304 bytes, sha256 `9bf7d88933f91eae99175f07314d28dc957097e4d27f566ff2dd7547a2eeacf6`.
- `esptool verify-flash 0 backups/stock-flash-2026-10-03.bin` matched the board's own MD5 of all 4 MB before the first flash.

## Read flash at 115200 baud; faster reads fail on this CP2102
`esptool read-flash` at 921600 or 460800 dies at once with "Invalid head of packet (0xFF)". A `--trace` run shows that commands at those rates work, but the first byte of the stub's 4 KB data frame arrives as 0x7F instead of 0xC0. At the default 115200 the 4 MB read takes about 6.5 minutes and passes its MD5 check.

## ldproxy's release zip carries no executable bit
The `ldproxy-aarch64-apple-darwin.zip` asset (esp-rs/embuild tag `ldproxy-v0.3.2`) was zipped on a FAT host and stores `rw-r--r--`. mise 2025.10.17 keeps archive file modes, so ldproxy installed but `command -v ldproxy` found nothing. `mise.toml` fixes it with a per-tool `postinstall` that runs `chmod +x "$MISE_TOOL_INSTALL_PATH/ldproxy"`. If you install the tool before that line exists, rerun `mise install --force github:esp-rs/embuild`.

## ldproxy 0.3.2's "Running ldproxy" line fails the firmware warnings check
rustc reports any linker stderr as a `linker_messages` warning, and ldproxy 0.3.2 logs that line at info level. `firmware/.cargo/config.toml` sets `LDPROXY_LOG = "warn"`, so ldproxy's real warnings and errors still show. Upstream moved the line to debug in ldproxy 0.3.5 (esp-rs/embuild commit 093676c0), but 0.3.5 ships no prebuilt binary.

## espflash 4.6.0 always warns "Monitor options were provided" without --monitor
It's an espflash bug, and it's harmless: `flash()` sets `monitor_args.elf` to the image path just before `check_monitor_args()` tests `elf.is_some()`. `scripts/flash` passes no monitor options.

## espflash rewrites the flash speed in the bootloader's header
espflash 4.6.0 rewrites byte 3 of the bootloader's image header (flash size and speed) and recomputes the bootloader's SHA-256, even when `--bootloader` names the IDF-built file. The speed comes from `--flash-freq`, or else the chip default, which is 40 MHz on the ESP32 (`src/image_format/idf.rs`, `Chip::default_flash_frequency`). The app's header gets the same value.
- esp-idf-sys builds an 80 MHz bootloader (header byte `0x2f`), but without the flag its 2nd stage would set up flash at 40 MHz while the app's sdkconfig says 80.
- `scripts/flash` passes `--flash-freq 80mhz`. Change it together with `CONFIG_ESPTOOLPY_FLASHFREQ_*` in `sdkconfig.defaults`.
- The boot log's `boot.esp32: SPI Speed : 80MHz` shows the header; `esp_psram: Speed: 80MHz` shows the app's sdkconfig.
- `firmware/.cargo/config.toml` has no cargo `runner` for the same reason: `espflash flash --monitor` would write a 40 MHz header, its own bootloader and its default partition table. Flash with `scripts/flash`.

## A 12 s fingerprint takes about 11 s on this board
On 2026-10-03 it took 12618 ms at ESP-IDF's default 160 MHz, and 10526 ms in a throwaway 240 MHz build. About half the time scales with the clock and half doesn't (PSRAM or flash fetch at 40 MHz); the firmware runs at 240 MHz (SDD Ruling R11). `search_interval_secs` stays 8, against the plan's "over 6 s" rule, because the policy starts the interval when the answer arrives. The spec's Risks section has the numbers.
- Since Task 11's fix round (SDD Ruling R19), the FFT's 8 KB tables and buffers live in PSRAM (`CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=8191`; see "Internal RAM is the budget"). With PSRAM at 40 MHz, a fingerprint then took 16.1–16.9 s, and a re-check cycle while matched about 48 s.
- Since Task 11b (SDD Ruling R20), flash and PSRAM run at 80 MHz. Over 9 asks a fingerprint took 9.75–10.35 s (mean 10.2 s), and a cycle while matched 40.8–41.4 s. The 12 s snapshot holds the ring lock about 50 ms, down from about 90 ms.
- Since Task 14b, core 0 captures 48 kHz stereo and runs the decimator, and the two cores share flash and PSRAM. Over 7 asks a fingerprint took 10.84–11.28 s (mean 11.1 s), the snapshot 58–62 ms, and a cycle while matched 42.2–42.8 s.
- Generating the test signal, `chord_and_chirp(12)`, takes about 19 s on the ESP32 at 160 MHz (libm `sinf` on large arguments). The probe doesn't time it; keep it out of anything timed.

## Long work on the main task trips the task watchdog
`main` runs on core 0, so a fingerprint there starves IDLE0. The watchdog then prints a backtrace every 5 s (no panic by default). The prints double as a rough profiler: decode the addresses with `xtensa-esp32-elf-addr2line -fC -e firmware/target/xtensa-esp32-espidf/release/esp-shazam`; the tool sits under `firmware/.embuild/espressif/tools/xtensa-esp-elf/`. Long-running work belongs on core 1, where Task 9's recognizer runs, because `sdkconfig.defaults` watches only CPU 0's idle task.

## LINE IN is wired to Harper's turntable, not the Mac
`afplay` can't reach the board. Device audio checks need Harper to play a record, or to move the 3.5 mm cable to the Mac's headphone jack for a test. On its LINE output the turntable read -22 to -28 dBFS at PGA 0 dB (a 3 s sample), so the ES8388 PGA stays at 0 dB. Harper's continuous test source reads about -43 dBFS, and idle sits about -79 dBFS, so the board's quiet threshold is -60 (`firmware/cfg.toml`, SDD Ruling R12). Device checks run against whatever plays.

## Turntable output must be on LINE (fixed), not VARIABLE
VARIABLE was turned down, and music reached the board at about -64 dBFS. That's under the -50 quiet threshold, so the board would have heard silence. The codec was never at fault: LIN2/RIN2 carried the music (the level rose about 24 dB with the PGA), and LIN1/RIN1 read only the ADC's own -86 dBFS. When levels look low, check the source's output mode and volume first. Also confirm the source is stopped before calling a capture a noise floor: a capture logged as idle at 16:54 read the same -64 dBFS and most likely held the turned-down music.

## Park the board after a device test, or it keeps asking Shazam
From Task 9 on, the firmware asks Shazam about every 41 s while music plays, for as long as the board has power. When a test ends, run `espflash board-info --port /dev/cu.usbserial-0001 --after no-reset`. The chip then stays in its serial bootloader ("Staying in bootloader"), and nothing gets written. The next reset, `scripts/serial-log` or `scripts/flash` boots the app again.
Since 2026-10-04 Harper's `firmware/cfg.toml` has `shazam_enabled = false` (Harper: "focusing on the streaming"). The board then sends nothing to Shazam (only SNTP, MQTT and mDNS traffic) and stays running, so Harper can watch the level on the page. Park it only after a test that turns the flag back on.

## ESP-IDF's I2S receive queue holds one buffer fewer than you configure
`i2s_common.c` (IDF 5.5.5) creates the receive queue with `dma_buffer_count - 1` slots. When the reader falls behind, the ISR drops the oldest buffer without logging anything.
- At 48 kHz with two 32-bit slots a frame (Task 14b), a 240-frame buffer is 5 ms and 1,920 bytes of internal RAM. `audio_in` uses 10, so it can run 45 ms late.
- The audio thread never waits for the recognizer: it `try_lock`s the 12 s ring and keeps its samples while the 12 s snapshot (about 60 ms) holds it (SDD Ruling R25).
- Core 0 still slows down: the start of each fingerprint (1–20 ms after the snapshot; it fills about 2 MB of fresh PSRAM buffers) put the thread 26–31 ms behind over 11 asks, and Wi-Fi start-up 15–25 ms at boot. Otherwise it ran at most 2.2 ms late. At 35 ms of queue the margin was 4 ms, hence 10 buffers.
- To catch a drop, count frames against `esp_timer` time: a lost buffer leaves a lasting 240-frame step. Task 14b's throwaway diagnostic does this (`target/task14b/diag.patch`, kept locally).

## On the classic ESP32, an I2S slot is as many bits as the data width
`hal/esp32/include/hal/i2s_ll.h` sets `rx_bits_mod` to the data width ("on ESP32, sample_bit should equals to data_bit"), while `i2s_std.c` computes BCLK from `slot_bit_width`. So 24-bit data in 32-bit slots would run at the wrong sample rate (read from the source, not tried). For 24-bit audio either use a 32-bit data width, as `audio_in` does (the sample arrives in the top 24 bits; shift right by 8), or 24-bit slots with MCLK at 384 x fs (`i2s_std.c` refuses 24-bit slots otherwise) and the codec's MCLK/LRCK ratio to match. On the board the low byte of each 32-bit slot reads 0: the ES8388 drives zeros after its 24th bit.

## The audio thread's decimator costs about 9% of core 0
Measured in Task 14b: about 440 µs per 240-frame (5 ms) chunk over a 5-minute run, against Task 14a's estimate of 1–2% of a core. The whole capture loop took about 0.8 ms per chunk (16%), the diagnostic's own counting included. The FIR is f32 on the FPU, but each of its 63 taps is a dependent `add.s` (Rust doesn't fuse into `madd.s`). Folding the symmetric taps or splitting the sum would cut it, if core 0 runs short.

## toml-cfg drops attributes and pastes values into Rust source
`#[toml_cfg::toml_config]` rebuilds the struct from field names and types alone. Doc comments and `#[allow]` on the struct or its fields vanish, so use `//` comments in `config.rs`. Each `cfg.toml` value is printed as TOML and parsed as a Rust expression. A string containing `"`, `\`, a newline or a control character comes out in a TOML form that isn't Rust (`'...'`, triple quotes, or TOML escapes). The build then fails, and its panic message prints the value: a password among them lands in `target/check-logs/firmware-build.log`. A float setting needs a decimal point (`-60.0`, not `-60`).
It also ignores unknown keys, so a misspelled key in `cfg.toml` silently falls back to its default.

## Boot logs show the Wi-Fi name and the password's length
ESP-IDF's Wi-Fi driver logs `wifi:connected with <SSID>` at every connect. With station OWE support on (`CONFIG_ESP_WIFI_ENABLE_WPA3_OWE_STA=y`, the IDF default), `libnet80211.a` also warns "OWE is enabled, but authmode threshold is 3 or password length is N", where N is the real length of `wifi_pass`. Redact both before pasting a log anywhere. Turning OWE off would remove the warning; that waits on Harper (plan `## Now`, Open).
Update: the password-length warning appears in Task 9's logs but in none from Tasks 10, 11 or 11b. Check a fresh boot log before acting on this.

## mDNS comes from the espressif/mdns component, pinned to 1.8.2
`esp_idf_svc::mdns` exists only when that managed component is in the build: `[[package.metadata.esp-idf-sys.extra_components]]` in `firmware/Cargo.toml`. 1.8.2 builds on IDF 5.5.5; the registry's newest on 2026-10-03 was 1.14.0. The component manager downloads it into esp-idf-sys's out dir under `firmware/target/` and writes `firmware/components_esp32.lock` (committed). No `firmware/managed_components/` appears.
esp-idf-sys reruns its build script only when a file it tracks changes, such as `sdkconfig.defaults`, never `Cargo.toml`. After a change to `package.metadata.esp-idf-sys`, an existing `firmware/target` fails with "could not find `mdns` in `esp_idf_svc`". Run `touch firmware/sdkconfig.defaults` and build again.

## On this Mac, every esp-idf-sys build-script run clones ESP-IDF again
`~/.gitconfig` rewrites `https://github.com/` to `git@github.com:` (`url.insteadOf`), so `git remote get-url origin` in `firmware/.embuild/espressif/esp-idf/v5.5.5` returns the SSH form. embuild 0.33.5 compares that with the https URL, finds no match, deletes the checkout and clones it again over SSH. The build still passes: on 2026-10-03 the whole build, clone included, took 2 minutes. Ordinary builds don't rerun the script. A change to `sdkconfig.defaults` does, and so does the `touch` a metadata change needs (previous entry).

## Lookups of esp-shazam.local take 5 s on the Mac; use `curl -4`
The board has no IPv6 address (esp-idf-svc never creates a link-local one), so it can't answer AAAA queries. The mdns component never sends NSEC to say there is none. A dual-stack lookup, curl's default, then waits 5 s for the AAAA before returning the A record. Measured 2026-10-03: 5.02 s cold and 5.01 s warm dual-stack, against 0.01 s IPv4-only. `curl --max-time 5` by name fails; `curl -4` answers in 0.3 s. A browser will likely wait the same 5 s on its first lookup (not verified).
The Mac (192.168.99.0/24, en0) reaches the board's 192.168.200.0/24 through Tailscale (`utun4`). mDNS answers still arrive on en0, so something on the network repeats mDNS between the subnets. Requests over that path took 0.3–1.4 s.

## The board's pages answer GET only
`/` and `/api/state` have GET handlers and nothing else. `curl -I` sends HEAD, which ESP-IDF's httpd answers with `405 Method Not Allowed` (text/html), so check headers on a GET: `curl -s -D - -o /dev/null <url>`.

## Internal RAM is the budget: TLS to Shazam needs one 16.7 KB block of it
mbedTLS allocates only from internal RAM (`CONFIG_MBEDTLS_INTERNAL_MEM_ALLOC`, the IDF default), and `mbedtls_ssl_setup` needs one contiguous block of about 16.7 KB for its 16 KB input buffer. Thread stacks come out of internal RAM too (ESP-IDF pthreads use `MALLOC_CAP_INTERNAL`), and malloc tries internal RAM first for anything up to `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL` bytes.
- **The big consumer is the fingerprint, not TLS.** chfft's `RFft1D::new(2048)` keeps five 8 KB tables and a 4 KB index table. SongRec's generator adds 8 KB and 4 KB, and every FFT allocates 8,200 bytes. With the old threshold of 16384, all of that sat in internal RAM for the whole fingerprint. Internal free fell to 860 bytes during one, with no TLS session open, and to 5,472 bytes during another.
- **MQTT added to it.** Task 11's first build ran a pump thread and a publisher thread with 6 KiB stacks each and took 25.2 KB. One boot's first ask then failed with `mbedtls_ssl_setup returned -0x7F00` (`MBEDTLS_ERR_SSL_ALLOC_FAILED`).
- **Fixes, 2026-10-03 (SDD Ruling R19):**
  - `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=8191` sends the 8 KB FFT buffers to PSRAM.
  - The recognizer drops the signature and its URI before the HTTPS call.
  - `mqtt_out` uses `new_cb` (no pump thread), a 4 KiB publisher and a 4 KiB esp-mqtt task. That's 15.8 KB measured, minus the 2 KiB task-stack cut. The task used 2.1 KB at most.
- **Measured over 10 asks at 40 MHz flash/PSRAM while `/api/state` was polled every 3 s.** In `MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT`, which is what mbedTLS asks for:
  - lowest free since boot: 30,844 bytes
  - largest block before every TLS session: 47,104 bytes
  - TLS failures: none
  - Before the threshold change, the same figures were 860 and 25,600 bytes. Plain `MALLOC_CAP_INTERNAL` reads about 29 KB higher, because it also counts 32-bit-only IRAM that mbedTLS can't use.
- **The cost:** with PSRAM at 40 MHz, a fingerprint took about 16.5 s instead of 10.7 s. Flash and PSRAM at 80 MHz (SDD Ruling R20) bring it to about 10.2 s, and Task 14b's 48 kHz capture to about 11.1 s (see "A 12 s fingerprint takes about 11 s"). At 80 MHz, over 9 asks with the same 3 s poller, the lowest free was 31,496 bytes, the largest block before every TLS session was 47,104, and no TLS session failed.
- **Task 14b, 2026-10-04:** 48 kHz capture with a 2 s stream ring in PSRAM, 10 I2S DMA buffers of 1,920 bytes (19,200 bytes, against 15,360 before). The lowest `INTERNAL|8BIT` free was 30,056 bytes over five asks with 8 DMA buffers, and 36,064 over two asks with 10 (a short run). The 10-buffer build's worst case is the five-ask low less the two added buffers' 3,840 bytes, about 26.2 KB, which supersedes the short run's 36,064. So the margin above the 10 KB floor with Shazam on was about 16 KB (15,976 bytes with a 10 KiB floor) before the stream server, which takes some of it even with no listener (next item). The largest block before every TLS session was 47,104, with no TLS failures. With Shazam off, the lowest was 95,044.
- **Task 15, 2026-10-04:** one stream listener costs 17-25 KB at the low (median 21 KB over 11 windows), two 41-55 KB (median 48 KB over 34 windows): a 4 KiB thread each, 3.4 KB of buffers each, lwIP's queue and the Wi-Fi driver's TX buffers. Measured as each run's median idle low minus each window's low with only listeners present, per 10 s window (`heap_caps_monitor_local_minimum_free_size_start`; the since-boot low is Wi-Fi start-up's, about 97 KB, and hides them). **The server costs internal RAM even with no listener:** idle free before any listener was 125,131-125,307 bytes in Task 15's runs, against 130,967 in Task 14b's flag-off run on the same capture settings, so about 5.7-5.8 KB (the accept thread's 4 KiB stack and TCB, the listening socket, the mDNS service, the larger socket table). After the first listener, idle free settles another 0.5-1 KB lower, once rather than per connection (run D's 13 connections cost 928 bytes, run F's one 548). **So with Shazam on, the margin above the 10 KB floor is about 9-10 KB with the server idle** (15,976 less 5.8-6.8 KB), and even one listener doesn't fit.
Measure the internal heap before adding anything that lives in internal RAM. If the margin shrinks again, letting mbedTLS use PSRAM (`CONFIG_MBEDTLS_DEFAULT_MEM_ALLOC`) removes the 16.7 KB need. ESP-IDF calls internal-only the more secure choice, though, so that is Harper's call.

## esp-mqtt holds its API lock through each connect attempt
`esp_mqtt_client_enqueue` and `_publish` wait for that lock, so a publish can block through a whole connect attempt while the broker is away: the TCP connect and the CONNACK wait take up to `network_timeout` (10 s by default) each. Publish only from `mqtt_out`'s own thread; the recognizer just sends on a channel. Callbacks from `EspMqttClient::new_cb` run on esp-mqtt's task, which needs no pump thread. `EspMqttClient::new`'s `connection.next()` does need one: without it, esp-mqtt stalls. Queued messages expire after 30 s (`OUTBOX_EXPIRED_TIMEOUT_MS`), and the publish on every connect covers longer outages.

## esp-idf-svc's MQTT error event always says ESP_FAIL
esp-idf-svc 0.53 maps every `MQTT_EVENT_ERROR` to a fixed `ESP_FAIL` (a TODO in its `mqtt/client.rs`), so `mqtt: ESP_FAIL (error code -1)` carries no cause. esp-mqtt's own `E (…) mqtt_client:` and `esp-tls:` lines just before it give the cause, for example `transport_read(): EOF` or `Connection reset by peer`.

## Reading the retained MQTT topic
- The retained message outlives the board. A parked or unplugged board leaves its last song on the broker, because there is no Last Will. After a reset, compare the payload with `/api/state`, or wait for the board's first publish (`null` right after it connects).
- A live subscriber gets the board's publishes with `retain=0`; only a new subscription shows `retain=1`. `mosquitto_sub -q 1 -F '%I retain=%r qos=%q %t %p'` shows the flags.
- The board reaches a broker on the Mac at its en0 address (192.168.99.151:1884), although the Mac reaches the board through Tailscale: the gateway routes between 192.168.99.0/24 and 192.168.200.0/24 (`ping -b en0 192.168.200.8`: 3–6 ms). Task 11's broker-restart test used that path.

## With shazam_enabled = false, `state` stays "quiet"; the page says "Shazam is off"
Only the recognizer thread sets `state` and `now`, so a board that doesn't start it reports `state: quiet` and `now: null` for good. `level_dbfs` stays live: since Task 14b the audio thread measures it every 250 ms, Shazam or not (SDD Ruling R24), so it shows whether audio arrives. The page reads `shazam_enabled` and shows "Shazam is off · <level> dBFS" in place of the state, and hides "Nothing playing.". Anything else reading `/api/state` should check `shazam_enabled` before trusting `state`.

## The flag-off image is smaller: only one path ships
`CONFIG.shazam_enabled` is a constant, so both branches of `if CONFIG.shazam_enabled` type-check and lint in every build, but the optimizer and linker drop the dead one. On 2026-10-03 espflash's "App/part. size" read 1,485,248 bytes with the recognizer and 1,258,816 without, and the flag-off build had no warnings. `scripts/check` builds whatever `cfg.toml` says, so it links only that image.

## `scripts/serial-log` execs into uv, so `pgrep -f serial-log` finds nothing
To wait for a backgrounded capture, match the python command line: `pgrep -f "usbserial-0001 <secs>"`. A wait loop on `serial-log` ends at once, and the capture is still running.

## "Flag out" means off on Harper's board
On 2026-10-03 Harper asked to "feature flag out the shazam bits" and go for the stream. The flag shipped defaulting to on, and Task 14b was briefed to test the Shazam path on the board. Harper corrected that on 2026-10-04: "i thought we had talked about putting that behind a feature flag and focusing on the streaming". When Harper flags a feature out, turn it off in `cfg.toml` and spend no device runs on it until Harper asks.

## ESP-IDF's usleep busy-waits anything under one tick
`CONFIG_FREERTOS_HZ=100`, so a tick is 10 ms. ESP-IDF's `usleep` (`newlib/src/time.c`) spins in `esp_rom_delay_us` for anything shorter, and Rust's `std::thread::sleep` calls `usleep` on ESP-IDF. A loop that waits should sleep at least 10 ms, as `stream.rs` does, or it burns its core.

## Historical Task 15 throughput settings and measurements
The following measurements used the Task 15-era 5,760-byte send buffer and dynamic TX buffers; they are not the current profile. One raw WAV listener needs 288,000 bytes/s (2.3 Mbit/s). lwIP's TCP send buffer (`CONFIG_LWIP_TCP_SND_BUF_DEFAULT`, 5,760 bytes) caps a connection at that much per round trip, so round trips must stay under 20 ms.
- From the Mac, round trips averaged 75-90 ms with Wi-Fi modem sleep on (ESP-IDF's default, `WIFI_PS_MIN_MODEM`: the AP holds the board's frames until its next beacon) and 14-34 ms with it off. The firmware turns it off since SDD Ruling R27 (`net::start_wifi`). The Mac is on Wi-Fi and reaches the board through Tailscale, so the board sees the subnet router (192.168.200.5) as the peer; `curl --interface en0` takes the routed path instead.
- On 2026-10-04 listeners sometimes fell a ring behind within 6-22 s and sometimes held for minutes, on the same image. The spec's Risks has the numbers. Test with `scripts/stream-check` by IP, and expect run-to-run variance.
- lwIP here has no per-socket send buffer. The Kconfig help mentions `TCP_SNDBUF`, but `sockets.c` leaves `SO_SNDBUF` unimplemented, so only the global default changes it.
- A full connection costs internal RAM of 3-4x its send buffer, because the Wi-Fi driver's dynamic TX buffers fill too: about 20 KB at the default 5,760 bytes, 43 KB at 11,520 and 70 KB at 23,040, at the low.

## Historical Task 15 stalled-peer reset and FIN retention
The following measurements used the Task 15-era close policy; they do not describe the current checked positive-linger path. After a zero-window probe, lwIP's `snd_nxt` is one byte past what the peer has, and an abort (`SO_LINGER` 0) resets with `snd_nxt`. A peer with a closed window drops that reset (RFC 5961). Behind Tailscale's subnet router it's worse: the router closes its NAT state on the reset, so the peer's later packets never reach the board, and a curl stopped for 15 s hung about 9 minutes after it resumed, until its own keepalive gave up. The Task 15 image closed a dropped listener with a FIN instead. That keeps the peer's unsent audio (at most 5.7 KB of internal RAM) until the FIN gets through or lwIP's 12 window probes run out, about 7.6 minutes.
- **They pile up.** A dropped listener's PCB, about 6 KB of internal RAM with its queued audio, outlives its slot by up to 7.6 minutes. It goes when the peer reads again (and gets the FIN) or closes. A player that keeps its stalled connection open while it reconnects (pause, dropped, reconnect, pause again) leaves one more each time: three such cycles within 7.6 minutes hold about 18 KB. This was the unfixed Task 15 state on 2026-10-04; it mattered most with Shazam on.

## Current transport profile and measured limits
Current source sets a 46,080-byte TCP send buffer, static TX16 and cache32,
with the prior RX/reordering bounds preserved. Listener close uses two-second
positive linger and retains the listener slot until checked close; an
unrecoverable close reboots under Doctor Biz's approved policy. Queued payload
and control PCBs remain separate resource measurements.

The real FLAC/WAV trickle collection returned both ports to LISTEN-only by log
time 214.921 s and restored the largest PSRAM block to 917,504 B. This short run
proved sampled reclamation, not a universal lifetime bound. Direct draining still
lost one connection to generation overrun with nonzero window and ACK/retransmit
stalls; the six-hour soak failed. Normal routed playback remains unverified.

The historical successful ten-minute runB used dynamic TX32, unlike static16/cache32.
IDF 5.5.5 allows dynamic TX with PSRAM preference only when IGNORE_NOTFOUND is on.
Despite its Kconfig prose, that flag does not disable allocator PSRAM preference:
lwipopts.h and esp_adapter.c gate that on TRY_ALLOCATE alone. It disables the TX
cache/by-reference path. A temporary dynamic-profile experiment must verify mapped
PSRAM; production still needs an explicit boot-memory requirement. Fresh static
tests on October 5 passed one listener for 120 s but lost one of two to ring lag
after 41.9 s. The dynamic/no-cache/copy profile also lost one of two after 71.1 s
and used more internal memory; it was rejected and static defaults restored.
One short pass cannot establish a transport fix.

An ESP-only diagnostic retried nonblocking sends after 20 ms poll slices while
preserving the absolute frame deadline. It still lost a listener to ring lag
after 85.4 s and was reverted. Do not reintroduce it as a proven low-water fix.
Large en0-bound DF ICMP probes answered, so no MTU black hole was observed in
that small sample; TCP delivery still needs packet evidence.

The separate Wi-Fi-only fixture also hit scheduled write deadlines with two
135,168 B/s peers, no audio/application workload and 181,675 B internal free
before admission. Audio load is not necessary for the stall. Native Mac TCP
counters showed out-of-order received bytes during a production reproduction;
they do not locate the loss. A relay connect-timeout failure ends downstream
HTTP without a response; distinguish that from a board handler closing it.

A native macOS default/more/default ACK-policy comparison delivered about 120 s
of valid audio in its first two runs, then lost both default peers around 84 s
with receive windows still above 64 KB and flat capture-counter deltas. The
control both passed and failed: TCP_SENDMOREACKS is not a demonstrated fix.
Public TCP_CONNECTION_INFO has no duplicate-byte field; offset 104 counts
transmitted retransmission packets. Its TX packet count excludes pure ACKs,
and RX packet counts can reflect coalesced input buffers, not wire segments.

Verify a capture by its process and file, not a dialog selection. Doctor Biz
may be away from the Mac; selecting a response does not run a Terminal command.
Owned-socket TCP statistics work without BPF administrator access, but cannot
locate where missing or reordered data disappears along the network path.

## Formatting a float to a precision takes 1 KB of stack
Rust's `{:.1}` on a float goes through `float_to_decimal_common_exact`, which puts a 1,024-byte buffer on the stack. On the 4 KiB stream threads, log integers.

## The Mac idle-sleeps through long device tests
On 2026-10-04 the Mac turned its display off at 14:27:08 and went to idle sleep at 14:27:57, then cycled between dark wakes of about 45 s and maintenance sleeps of about 5 minutes (`pmset -g log`). Processes froze in between: a 60 s stream capture stopped after 20 s of audio, its `/api/state` poller recorded nothing for 5 minutes at a time, and ffmpeg then waited 15 minutes on a connection the board had already dropped. Run long device tests under `caffeinate -is` (it holds off sleep for that one command). `scripts/stream-check` gives ffmpeg a 10 s I/O timeout (`-rw_timeout`), so a stalled stream fails the check instead of hanging it.

## A pyserial read with DTR and RTS set false still reset the board
On 2026-10-04 a read meant to leave the board running (pyserial, `dtr = False` and `rts = False` set before `open()`) logged `rst:0x1 (POWERON_RESET)` and a fresh boot, and the read itself took over 2 minutes instead of 25 s while the Mac was sleeping on and off. The mechanism isn't pinned down. Don't count on reading the serial log without resetting the board.

## Sweeping a big PSRAM buffer costs far more than reading it once
The ESP32 reads flash and PSRAM through one cache, 32 KB per core (`memory-types.rst`: "the first 64 KB block which is used for PRO and APP MMU caches"). ESP-IDF warns that data over 32 KB falls back to PSRAM speed and can "push out" cached flash, slowing the code too (`external-ram.rst:225`). On 2026-10-04 the FLAC encoder's first version took 19.4 ms per block with its 32 KB block in PSRAM against 10.5 ms in internal RAM. A plain sequential read of the same 32 KB took 1.2 ms from PSRAM and 0.7 ms from internal RAM, but the encoder swept the block four times while writing its output. It also slowed core 0: the audio thread's decimator went from 434 to 562-640 µs per chunk. Work on data while it sits in internal RAM, make one sequential pass over PSRAM (as `flac::Encoder::push` does), and time it on the board: the Mac ran the same code 120-250 times faster.

## Core 1 is shared: a slow stream thread can stall capture through the ring's lock
Stream listener threads run on core 1 at priority 7. When the first FLAC encoder (43-99 ms per 85 ms block) oversubscribed core 1 with two listeners, a listener was kept off the CPU while holding the stream ring's lock. The audio thread then waited 37.4 ms for it, and capture ran 35.6 ms late of its 45 ms queue (Task 16, run A). Keep per-listener work on core 1 well under real time; the encoder now takes about 14 ms per block.

## Reading the firmware's machine code
The esp-14 toolchain's GNU `xtensa-esp-elf-objdump` prints raw words for the Rust code, even with `XTENSA_GNU_CONFIG` pointing at `lib/xtensa_esp32.so`. To read what LLVM made of a crate, have rustc write assembly from the firmware directory: `CARGO_TARGET_DIR=<scratch> cargo +esp rustc -p esp-shazam-core --release -- --emit asm -C opt-level=3`, then look in `<scratch>/xtensa-esp32-espidf/release/deps/*.s`.

## The ESP32 has no 64-bit atomics
`std::sync::atomic::AtomicU64` doesn't exist on `xtensa-esp32-espidf`: the build fails with "cannot find `AtomicU64` in `atomic`". Use `AtomicU32`, or a mutex.

## espflash can fail with "Bad data checksum"
On 2026-10-04 one `scripts/flash` failed with `Error while running FlashDeflData command ... Bad data checksum`, a transfer corrupted on the way over the CP2102. The board then holds a partial image and boot-loops until a flash goes through. The retry worked at the same settings.

## Tell Harper the moment a feature works on the board
On 2026-10-04 `/stream.flac` played on the board at about 16:45, but the agent kept optimizing, soaking and documenting until 18:30, and Harper, who was waiting to test, heard nothing for over two hours ("you have been spiraling again?"). A background implementer should `SendMessage` the main session as soon as the feature first works on the board, and the main session tells Harper right away. Polish and measurements follow. On long runs, send Harper a one-line status every 30 minutes or so.

## Kata audit work can overlap another live session
On 2026-10-04 another session changed firmware and committed `ee68892` and `cf42331` while the Sol 6.1 agents worked on constant FLAC subframes. Keep file ownership explicit and coordinate before flashing. The board responds at `192.168.200.224`; `192.168.200.8` is the MQTT broker.

## Five audit tickets concern archived diagnostics
`hb9p`, `sznz`, `b69f`, `3db1`, and `vya4` concern the temporary module saved under `target/task15b/` and `target/task16/`. It is absent from current firmware. The audit retired these tickets as wontfix for obsolete diagnostics; it did not fix their bugs. Repair and verify any diagnostic before reusing it, and do not cite unsafe archived measurements as validation. Production capture/CPU/heap telemetry and separate hardware examples cover current tests; they do not supply linkoutput error counts, a raw throughput blast, or controlled encoder working-buffer placement comparisons. The Wi-Fi transmit-resource investigation remains open in `23mz`.

## Kata audit, 2026-10-04

The machine's global Git configuration rewrites public GitHub HTTPS URLs to SSH.
When esp-idf-sys refreshes ESP-IDF, that rewrite can make its URL check reclone the
SDK and fail SSH authentication. `GIT_CONFIG_GLOBAL=/dev/null scripts/check`
(and the same scoped variable for `scripts/flash`) uses public HTTPS without
changing global Git configuration.

A per-send `SO_SNDTIMEO` does not enforce a whole-frame deadline on macOS: one
send exceeded 700 ms against a 400 ms timeout while a peer drained in bursts.
`DeadlineWriter` now uses nonblocking sends and `poll` against one absolute
deadline, then restores the exact original flags. Callers must exclusively own
the socket and its clones during each write.

Doctor Biz corrected the audit's physical source on 2026-10-04: LINE IN is a player
streaming FIP, not the turntable from older notes. Audit playback measurements use
real FIP music. They do not establish worst-case incompressible-input throughput.

Hardware fault checks must include the fault setup interval in their capture-counter
baseline. A pre-READY baseline can hide loss during allocation; sample before START.
Later playback alone does not prove supervisor recovery: require its restart request,
the capture restart counter, and decoded audio before testing the separate reboot path.

On the 2026-10-04 overnight audit, both two-WAV and one-WAV runs through the Mac's
utun4 route hit the raw ring's 2 s write deadline. A single stream failed before
fault injection, with capture counters stable. Do not treat this route as a
reliable WAV freshness test or claim worst-case two-stream throughput from FLAC
music. Keep real loopback contract tests separate from routed playback evidence.

The fault harness originally used a 4 s ffmpeg read timeout, shorter than the
8.02 s FLAC write allowance. Use the existing stream-check's 10 s read timeout
while retaining completion/error assertions; that correction does not explain
the underlying routed stall.

On-device loopback TCP separated WAV freshness from that routed limit: both real
WAV peers received EOF at about 2.03 s during capture suspension, then both slots
admitted fresh clients. A separate real Wi-Fi disconnect/reconnect left capture
fault counters unchanged. Keep those contract tests distinct from network endurance.

Fallible pthread creation is not a complete internal-OOM guarantee: Rust allocates
thread Arc/Box metadata before the IDF call, and task stacks share the internal
reserve with DMA. PSRAM exhaustion tests prove strict frame placement, not safety
under arbitrary internal-heap exhaustion. Do not invent a reserve threshold from
one observed free-memory sample.

A full capture staging buffer must try the destination mutex before discarding
old samples. If the ring is available, flush staging and the incoming chunk in
order; the staging limit bounds contention, not delivery. A dropped-buffer count
does not establish one equally long lock hold: backlogged reads can collide often.

ESP-IDF Rust logs are synchronous and can wait indefinitely on filter/FILE/UART
locks. Default UART output also busy-waits FIFO space. High-priority health
checks must publish recovery actions independently of logs; moving only ordinary
telemetry leaves rare recovery logs capable of blocking the recovery itself.

Hardware fault fixtures must not log while a suspended task might own a logger
lock: a RAII resume guard cannot run while the observer itself is blocked. Save
event times, resume first, then report. Base task priority alone cannot make a
state-check/suspend pair atomic because the target can inherit higher priority.
The encoder fixture uses a short, pinned-IDF-verified local scheduler suspension.

Default ESP-IDF log prefixes and esp_timer_get_time marker times have different
origins. Sample esp_log_timestamp and esp_timer together before logging, then
convert clocks for comparisons; a UART marker's receipt is not exact clock sync.

Live UART files can end midway through a telemetry line while a producer writes.
Only LF-terminated byte lines are observations; decode each completed line after
framing. The overnight isolated health test falsely admitted an unfinished second
post-reset sample and failed on its absent read_errors field. Keep that failed
run, fix the reader with a real-file regression, then rerun the hardware check.

Read each decoder's stderr with its filename attached. Concatenating two files
without labels made the overnight soak's one errored decoder look like two.
Listener 2 had premature HTTP EOF, demux I/O and then invalid residual; listener 1
had empty stderr. Log byte counts can verify that attribution without guessing.

A whole-frame send deadline does not make TCP delivery atomic. The bounded raw
tee diagnosis received three CRC16-valid FLAC frames then 1,980 bytes of a fourth;
that partial tail matched the other peer's complete frame, and complete prefixes
decoded cleanly. ffmpeg reported invalid residual only when it read the truncated
tail. Check complete-frame CRCs and decode the prefix before blaming the encoder.

Pinned ESP-IDF 5.5.5 stores and reads the heap failed-allocation callback through
a plain global function pointer. One registration owner and allocator thread
safety prose do not establish safe late publication to concurrent callers.
The pressure fixture uses direct real allocation probes and actual admission
errors instead. Verify pinned source before adding a diagnostic callback.

An early post-reset level of -120 dBFS does not prove that the physical source
went silent. Keep the reading's time and compare later meter samples and decoded
RMS. The output probe began at -120 then read -86/-84.6, while decoded audio RMS
was about -82.42 dBFS and encoded frames stayed roughly 11 KB.

The pinned single-pbuf STA linkoutput path has no coalescing allocation before
Wi-Fi transmit, so its ERR_MEM result proves actual driver ESP_ERR_NO_MEM.
The output observer counts all STA ports; this does not identify a proprietary
pool or a rejected stream packet. A chained-pbuf ERR_MEM can also come from the
coalescing allocation, so keep those counters separate.

On this Mac, setting TCP_MAXSEG before connect does not clamp advertised MSS:
connection setup replaces it. TCP_NOOPT does suppress SYN options and lowered
the peer MSS in a real loopback test, but it also removes other TCP options.
Use that only as a labelled diagnostic, not as proof of a path-MTU fault.

Verify the board's actual MSS after client-option experiments. TCP_NOOPT was set
before connect in this audit, but every observed board connection still used
1440 bytes; the experiment never established smaller segments on that path.
Per-socket IP_BOUND_IF=25 on macOS can scope an upstream socket to en0 without
changing system routes. Both direct en0 stream sockets held a 60-second bound;
default-route reliability remains separate. A local relay adds buffering, so
label relayed results and do not reuse them as direct slow-reader acceptance.

The unrecoverable socket-close branch must reach restart without logging first:
the synchronous logger can wait on another task's lock. Retain a named failure
binding and the listener slot through the nonreturning restart; a wildcard can
drop a still-owned socket before recovery. Removing this log loses its diagnostic
signature. Reset observations alone do not identify the cause, and ESP-IDF's own
shutdown handlers still prevent an unconditional restart-time guarantee.

Pinned binary/source review maps the by-reference single-pbuf allocation failure
to an empty preallocated class 9 TX-cache free list; runtime does not allocate
another object on that branch. Why occupied slots fail to retire remains open.
Cache 32 → 64 → 32 repeats both had zero errors while transport still failed,
so keep cache 32 and do not infer a fix from one larger-capacity observation.
The extra 32 slots reduced sampled PSRAM free by 54,400 B, with no sampled
internal decrease; setup still permits internal fallback.


Held sockets are not proof of realtime playback. Match CRC-valid frame ends to
actual cumulative receive timestamps: the en0 control held 60 s but delivered
only 14.7627 s PCM in its final 20 s, with 128 driver ERR_MEM results. Preserve
late delivery rate and raw tails alongside socket lifetime. Its unchanged
default-route repeat lost both peers to ring lag; en0 success does not satisfy
routed acceptance or establish the transport cause.


The en0 cache64 control removed observed driver errors but both peers still
fell a ring behind near 22 s and delivered only10.752 s PCM. Earlier cache 32 held
60 s with 128 errors. Cache-error counts and playback acceptance measure different
things; neither zero errors nor a larger cache establishes a fix. Keep cache 32.


Final en0-relayed production checks also failed: two peers lost API response
within 32.159 s; one peer naturally fell a ring behind after 71.3 s and decoded
59.050667 s. Per-socket interface control is useful evidence, not a proven
solution. Keep state timeouts, natural UART close reasons and cleanup decoder
errors distinct, and do not launch a long soak from socket lifetime alone.


Read HTTP framing completion rather than waiting for TCP EOF. A final en0 state
probe received all 240 chunked-response bytes but an EOF-sensitive throwaway
collector called it a timeout. The canonical framing-aware probe then passed
in 828.055 ms with Shazam false. Preserve the collector failure separately; it
is not proof of an API blackout.

Doctor Biz confirmed audible host FLAC on Altec + 3, then requested board-only
playback tests. Keep the workstation as the physical analog source; Sonos must
fetch the ESP32 stream. Latest authorized listening volume is 15%.

For board listening, the workstation Master mixer at56/87 attenuates line out by
23.25dB. A Sonos PLAYING state and advancing position did not mean Doctor Biz
could hear the board: decoded audio measured about-43dBFS RMS. Raising Master
to72/87 (-11.25dB) raised decoded RMS to-30.77dBFS, peak-18.85dBFS, with zero
clipped samples in that check. Keep speaker volume and analog-source gain
separate in test reports. Doctor Biz confirmed the board music sounds great
with Master72/87 and Sonos15%; retain those settings for listening.

Sonos eviction events carry session identity in the header; the observed body
contains only the object type, ERROR_SESSION_EVICTED, and reason. Validate the
header against our owned session before decoding/applying ownership loss. A
foreign session created without loading media returns ERROR_COMMAND_FAILED on
suspend even with the board uninvolved. Close that unused test connection and
observe playback independently; do not mistake its cleanup failure for retake.

Doctor Biz corrected the test-source gain preference: use workstation line out
at unity gain, with both ALSA Master and PCM at 0 dB. This supersedes the earlier
Master72/87 listening setting. Control listening loudness at Sonos rather than
attenuating the analog test source. Unity gain does not certify an analog voltage.

Doctor Biz requested undoing Sonos test changes after confusing low-volume
behavior. Restored the saved pre-test group volume of 34% and disabled board
autoplay; verified current non-board playback continued unchanged. Individual
speaker volumes were not saved, so exact original speaker balance is unknown.
Do not reapply the old 5/10/15% test caps. Read live Sonos state before claiming
its volume or source; the later live group was at 49% on a non-board session.

Doctor Biz corrected the quiet-audio investigation: audit attenuation in our
chain before blaming the source or asking for its model. A 0 dB codec setting
is not proof of jack-to-PCM gain. The 2026-10-07 review matched 113,760 live
FLAC samples exactly to raw WAV after bit-depth conversion and found positive
channel correlation, but ADC mute/ALC/noise-gate readback and physical input gain
remain unverified. See docs/audit-evidence/2026-10-07/quiet-input-review.md.
Old Sonos test scripts still contain 5/15% volume writes; do not rerun those
scripts unchanged after Doctor Biz requested undoing test volume changes.

The quiet-input review squad found that diagnostic flashing would reset the
codec state under investigation. Preserve evidence first; post-reset readback
cannot clear a pre-reset fault. A known signal with measured jack voltage is
the missing absolute gain reference. The old generated host FLAC fixture uses
-24 dB attenuation and must not serve as a unity-gain comparison. See
docs/audit-evidence/2026-10-07/quiet-input-squad.md for ranked findings and tests.

FLAC validation must count decoded samples, not FFmpeg's `out_time_us`: timestamps
can span omitted blocks. A valid stream prefix can also lose its final block
without a CRC error or frame-number gap, so a symmetric duration tolerance can
hide loss. Keep exact sample counts for output-duration decoders; packet-copy
captures may include a final whole packet, but must never accept underflow.

Exported Bash functions run in a new `bash -c` without the parent's strict
options. Pass `-euo pipefail` explicitly for diagnostic stages, or an early
failure can end with a success receipt. Process-group supervisors also need
nested timeout commands in foreground mode so cleanup reaches their children.

A thread may start successfully before resetting the caller's pthread settings
fails. Keep creation and cleanup outcomes distinct: returning a retryable launch
error after success can create duplicate workers. Log cleanup failures separately
and retain startup resources only when creation actually failed.

Doctor Biz confirmed LINE IN was disconnected during the earlier October 8
quiet-input settling and TLS experiments. Physical ALSA playback does not prove
signal at the ADC. Those quiet soft-reinit trials included a transient through
about 420 ms. Later connected-input trials verified driven tones and measured
soft-reinit anomalies through 490 ms; retain the 500 ms policy. Neither set proves
physical cold-start behavior or calibrates gain.

UART arrival timestamps identify read chunks, not individual device events.
Several encoder and capture reports can share one timestamp; use original line
order to pair reports and bracket capture coverage. A format announcement may
follow the fresh encoder storage receipt, so validate the actual emitted order.

Diagnostic cleanup must tolerate a thread object whose start failed. Joining
an unstarted thread raises and can skip source termination. Establish deadlines
before starting helpers, record the real source PID, and keep cleanup failures
separate from the initiating failure.

An encoder probe's OFF phase is not automatically equivalent to a default build.
The October 10 probe lost raw history while a matched normal-image control kept
up over 320 capture-covered seconds. Keep failed sequences and image identities;
that association does not identify timing, code placement or cache as the cause.
See docs/audit-evidence/2026-10-10/encoder-default-control.md.

Asynchronous close logs need a later complete batch receipt before tests accept
them. Otherwise a lost-record warning can arrive after a false pass. Validate
the same or a newer append-only UART snapshot after the success callback; require
the sticky loss flag and unique, bounded receipt fields. Use producer event time
for delayed records, and distinguish pre-listener close from playback departure.

The soak harness starts UART and decoder children in separate sessions. Killing
only its parent's process group cannot guarantee cleanup. The bounded listener
trial uses a per-helper Linux subreaper and requires its positive cleanup receipt
before another reset. Real escaped-session child tests cover forced helper death.
Workstation Master is mono; validate declared mixer channels instead of requiring
two copies of every gain token. Syntax checks alone missed that preflight error.

Capture reporting and listener export run sequentially, but inlining capture
formatting kept an 800-byte metrics frame beneath the 992-byte exporter. An
out-of-line capture boundary reduces that caller to 208 bytes in both linked
builds. Inspect actual call frames before increasing task stacks; static savings
still need device watermarks. See the October 10 metrics-stack-isolation evidence.

The matching stack-boundary repeats measured 1188 bytes free versus 420/580 in
the original soak/slow trials, without enlarging the task. Delivery still failed.
The repeated slow test ended before its third admission, so zero export loss in
that smaller event sequence cannot clear the earlier dropped record.

The listener reason-counter trial directly measured one full-queue rejection,
with no contended or poisoned drops. Nine exported records plus one dropped
record establish at least ten attempts, not a complete census: no terminal
queue-empty receipt exists. Preserve strict loss rejection and keep historical
drops unattributed. See October 10 listener-loss-device evidence.

The ESP Rust std pthread mutex allocates its OnceBox/native mutex on first
access, including try_lock; IDF then allocates mutex/semaphore storage. Initialize
telemetry queues before spawning capture/metrics or stream workers. Linux futex
host tests cannot prove target first-use allocation behavior. Keep setup separate
from the try-lock-only producer path; see the October 10 encoder telemetry plan.

The real stream boot starts FLAC before Sonos applies saved MP3. Probe acceptance
must validate the complete inactive startup replacement, not reject every switch
or alter firmware startup to suit a synthetic fixture. Active invalidation and
raw loss still reject; preserve archived negative evidence unchanged.

Probe routing readbacks must check gain as well as jack switches. Master is mono
and must be on at 0 dB; PCM has two gain channels and no switch. Validate each
declared channel instead of counting duplicate tokens. These controls do not
prove that the ADC received the intended signal.

A failed soak summary can precede further decoder output during its observation
tail. For final decoded totals, count the retained framehash rows and preserve
the earlier summary as its own observation. In probe builds, a reset keeps
cumulative frame totals but starts a fresh timing baseline; subtract the actual
replacement-boundary count rather than the prior window's count.

Moving probe Storage formatting onto metrics measured only 428 bytes free on its
4 KiB stack in the October 10 failed diagnostic. Linked application frame sums
omit recursive Debug/logger/libc work. Static arithmetic cannot certify runtime
margin; exercise the real storage renderer before accepting that extraction.

On the pinned Xtensa build, destructuring a returned Option<Record> by value
created a second 200/296-byte stack copy in the exporter. Borrowing that returned
value removed the actual memcpy. Separating receipt formatting only helps when
the record-owning helper returns first; an extra leaf alone leaves both frames
live. Verify generated code and real watermarks rather than assuming either.

A partial UART line can cross reset and receive its timestamp in the next boot.
Probe acquisition must anchor to the first exact owned ROM reset marker, record
original byte offsets, and reject malformed first or later reset candidates.
Host arrival time alone cannot establish which boot produced a whole line.

Do not name scratch Python helpers inspect.py: imports from dataclasses/argparse
can load and execute that helper instead of the standard library. The October 10
layout helper is inspect-layout.py; original images and trial hashes were checked
after the collision and remained unchanged.

The supervisor samples metrics on the same core at higher priority. A Ready
sample includes a metrics task preempted by that supervisor; sampled states are
not time accounting. New observer firmware measured slower MP3 generation than
its archived control on October 10, but changed addresses confound runtime cost.
Use one image with a runtime gate before attributing that difference to logging
or queries; do not call a layout/cache mechanism proven from symbol shifts.

Keep event identity separate from nested result identity. The crossover wrapper
collected and cleaned up correctly but failed when both supplied a `kind` keyword.
Test the actual serialized completion event, not only its result validator. The
original failed acquisition remains failed after a later helper fix.

The October 10 same-image observer OFF/ON/OFF repeat stayed near 0.724x with
38 raw overruns. Switching the observer bundle off did not restore throughput
in that image. This narrows runtime attribution but does not prove a code-layout
or cache cause; compare exact default images before adding more instrumentation.

Firmware UART lines can contain `esp_shazam_firmware::stream: stream:` before
the peer. An arbitrary-text peer capture swallows the inner label, breaking
exact endpoint matching while departure/close pairing still appears to work.
Parse the socket-address token or the explicit `unknown peer` fallback and test
the actual logger prefix. Correct peer parsing does not clear audio loss.

The pinned embuild derives its workspace from OUT_DIR. Reconstructed firmware
must retain its normal firmware/target layout: an external target directory can
redirect SDK installation and relative configuration lookup. Activate mise in
the trusted original repo before entering a copied source tree. Isolate the
copies and caches; do not change trust settings to make a reconstruction build.

An encoded-ring generation reset invalidates every old cursor, even if retained
capacity was sufficient. The old FellBehind label also covered this case and
invalid offsets; it did not prove that a receiver exhausted history. Use explicit
read reasons and align them with producer reset events when diagnosing delivery.

A matching MP3 C archive does not establish matching live cost. The October 10
current-wrapper standalone benchmark fit all repeated synthetic blocks within
24ms, while the full application had missed that budget. Input content, heap
conditions and executable layout remain different; do not call the discrepancy
proof of contention or a cache defect. cc 1.6 debug command output also exposes
its intentionally failing compiler-family `-?` probe. Keep that expected noise
separate from the actual ten successful C compilations.

A sender-overhead matrix finished its real trials but lost final summary output
when UART contained `encoded_frames=2086esets=0`. Reject malformed counter records
before advancing count anchors, exclude the dependent window, and write failure
receipts even when analysis fails. Preserve original verdicts when recovering
from raw logs. Numeric field parsers must retain digit-bearing names such as
`send_over_10ms` without accepting an IPv4 prefix as a counter.

Wi-Fi loss can abort local TCP state without delivering FIN/RST to the remote
host. Test stale socket cleanup through checked local close, released admission
capacity and fresh streams; record remote EOF separately. Keep old host sockets
open through the fresh admissions so host cleanup cannot explain board recovery.

Doctor Biz has repeatedly flagged long runs with too few Kata closures. Keep one
primary implementation moving through its original acceptance; parallel workers
should resolve named blockers or finish independent tickets. Report the actual
open count and exact remaining gate. Supporting tools, plans and partial tests
are progress, but do not count as closures or justify expanding acceptance.
Repeating this promise is not a correction. Before starting more work, name the
ticket and the original unmet criterion the action will satisfy. If it requires
external access or a decision, surface that dependency instead of filling the
wait with more diagnostics or another review of the same backlog.

Encoder reset acknowledgement retains the last actual output receipt. A ring
reset alone publishes no audio, so recovered output can have a higher generation
and a smaller positive sequence. Validate the pair and production timestamp;
never invent a new-generation zero-sequence publication in harness fixtures.

Accepted capture and raw publication progress are sampled separately. A later
publication count can exceed the earlier accepted count without corruption.
Validate each receipt's own provenance; do not reject the whole metrics response
using a cross-read equality or ordering assumption. Backlog also needs matching
operator epochs, because recovery retains the last actual encoded receipt.

Live listener observations must not depend on the logging owner's cache refresh.
An October 11 trial kept HTTP fresh while that owner had a 54-second acquisition
gap. Copy the bounded listener registry after HTTP metadata/cache guards end,
then record served time. Cached capture/encoder families still need independent
freshness; a rendered historical graph is not a current-health assertion.

Explicitly drop parsed Sonos JSON after copying needed identity or encoding wire
bytes. Rust's last borrow does not drop the owning tree: getInfo retained about
14.5 KiB across the next TLS send. Releasing it fixed that pressure, but later
4437-byte TLS allocations still failed with a 3712-byte largest internal block.
Do not equate total free heap, native listener admission, or this partial fix
with working recovery. Keep bounded verified fixes separate from later probes.

Sonos can retain the original stopped radio URI while a new native queue is
unpublished, then expose the queue before its current track changes. Startup
observation may wait on those recorded states within a deadline, but neither
grants ownership. Require the exact published track and fresh coordinator before
cleanup. Native control passes with 2048-byte TLS output records; a real 14027-byte
Shazam request also passes, but neither trial proves concurrent-load recovery.

Do not combine an interactive serial monitor with timeout's separate process
group: terminal reads can suspend it and leave UART owned. Use the supported
noninteractive monitor and verify process exit before opening the port again.

Sonos capacity needs execution resources as well as a slot. The October 11 FLAC
contention trial reclaimed the newer diagnostic and kept the older, but TLS left
too little contiguous internal memory for a new 4 KiB listener thread. Admission
then closed safely with ENOMEM and native playback failed. Do not claim recovery
from a joined log alone: require actual payload. WAV clients in the paired trial
fell behind before reservation, so their later checked closes do not prove eviction.

Pinned ESP-IDF 5.5.5 lwIP Kconfig mentions a TCP_SNDBUF socket option that its
actual socket headers and implementation do not expose. Verify declarations and
implementation before suggesting runtime options. PCB nrtx resets and saturates;
it is not a lifetime retransmission count. The public Wi-Fi statistics dump also
lacks documented module masks and counter semantics in the inspected headers.

Doctor Biz selected HTTP/UPnP Sonos control on October 11 ("skip tls for now")
after reviewing successful workstation SOAP playback. The Sonos PSRAM TLS test
is superseded; do not keep asking for it or resume the failed TLS experiments.
Preserve standalone board controls, format/buffer settings and intent admission.
SOAP URI checks are not atomic ownership; Stop policy was asked separately and
automatic recovery must not reclaim a changed source. See the sonos-http plan.

A compressed frame's actual write budget can be shorter than its nominal socket
allowance because media age clips the deadline. Record both, with their units;
owner-lifetime write/poll maxima do not describe the terminal frame. WAV uses an
Instant origin, so do not fabricate its boot-clock deadline or move its origin
while adding diagnostics. Reject malformed timeout receipts instead of skipping
them and accepting another valid record as sufficient evidence.

With Ethernet and Wi-Fi on the same subnet, binding Ethernet's source IP does not
necessarily select Ethernet. Check the actual route and bind the socket device
as well for a wired experiment. On this host SO_BINDTODEVICE works without sudo;
raw packet capture still requires privileges. Keep endpoint/device receipts.

ALSA can say PCH is invalid or report no soundcards while /proc/asound still lists
the hardware. Check actual device opens and ACLs: the October 12 desktop greeter
held audio access while harper received EACCES. Restore the desktop session;
do not bypass output verification or redirect test audio to another sink.

Startup capture already flushes the I2S queue before its overflow baseline.
The October 12 phase trace measured eight pre-baseline overflows and six later
overflows in two recoveries, observed during Wi-Fi construction/start. Do not
erase the latter as expected codec-clock backlog. Driver-read wall time includes
preemption/waits; phase overlap does not identify a task or ISR cause. An accepted
post-restart read can still be discarded for settling before raw publication.

Capture core0/priority10 reproduces startup DMA loss during Wi-Fi setup. Moving
capture to core1 clears startup but failed the matched two-listener MP3 load.
Core0/priority24 clears both bounded gates, including ordinary feature-off
firmware. Keep startup and loaded acceptance together; a quiet boot alone is
insufficient. Use stream-soak --require-clean-startup for setup checks so stable
post-loss counters cannot silently become a passing baseline. This does not
prove recovery under all workloads or identify every preempting task.

Raw progress is an optional atomic snapshot. When raw_valid=false, numeric zero
fields are placeholders; retain the observation gap and compare generations
only where their validity flag is true. A failed snapshot does not establish a
capture reset or prove uninterrupted publication through that interval.

NVS stress under active MP3 can outlast the older FLAC timing. Require both
listeners to span every write/readback, cleanup and cooldown marker; a passing
short decoder run alone does not pass the setup gate. The October 13 90-second
run missed cleanup, while the 180-second repeat covered the whole sequence.

The pinned NVS partition take() permits automatic erase after NO_FREE_PAGES or
NEW_VERSION_FOUND. A diagnostic that promises to preserve storage must use
take_with(false) and stop on error. The ordinary production policy is unchanged.

For radio-persistence evidence, flash with espflash --after no-reset, then let
the recorded serial acquisition own the first reset. An unrecorded production
boot can restore the protocol before the observer starts and hide the stored
diagnostic value. October 13 measured b/g across reset, then explicit b/g/n
restoration before association. Preserve association channel per boot; it changed
from 11 to 6 in the paired acquisition.

A Sonos URI can repeat across board boots: the in-memory intent and network
counters restart. The October 13 passive driver therefore rejected a valid
PLAYING observation as an old URI. Preserve that failed automated gate; future
harnesses need current-boot ownership evidence, not an assumed changed URL.
The same trial closed its socket after 584 MP3 frames, then observed no
replacement admission for 103.5 seconds. Startup retries do not prove established-playback recovery;
no admitted replacement does not mean no HTTP attempts. See passive-reconnect.

Throwaway collectors still need actual serialization checks. Passing a snapshot
field named monotonic into Trial.event collided with its own monotonic field and
aborted before the first observation. The actual event-writer regression catches
this; pure URI/metrics validators and syntax checks alone did not.

Pinned Shine's Q31 analysis/MDCT products shift by 32 three times, giving
forward scale 1/8; the verified long-block inverse overlap contributes 9.
October 13 corrects coefficient initialization by 8/9, derived from transform
and FFmpeg basis checks. Do not add a second PCM gain correction. Tone gain is
now near unity, but noise distortion, clipping and finite-stream tail remain
separate codec gates. The normalized golden has independent upstream provenance.

Normalized Shine MDCT still overflowed on near-full-scale PCM. Halve its integer
table and compensate with global_gain +4; do not attenuate PCM a second time.
Exact fused-row bounds cover signed 16-bit transform inputs; arbitrary quantizer
termination and decoded overshoot are separate. Negative PCM to Q31 must use
multiplication, not signed left shift. See October 13 headroom evidence.

Raw MP3's 1,057-sample delay means equal decoded/source counts do not retain a
terminal impulse. Two zero blocks recover the tested tails, not every possible
tail. Manual Stop cancels delivery while the shared encoder continues; do not
add a drain there. Epoch reset must clear nonzero history and partial PCM.

Never move a shared experiment directory while another worker is using it.
Freeze each probe's source snapshot and give workers separate output ownership;
retain raced acquisitions as failed evidence and reacquire from known hashes.

MP3 quality comparisons need matched constraints. The retained dualmono excerpt
scores 39.19 dB SNR with corrected Shine, 58.95 with default LAME, but 37.70
with LAME stereo sharing and reservoir both disabled. This does not isolate
either setting or prove audibility. Full-band noise SNR also confounds LAME's
high-frequency removal. Float overshoot alone is not internal integer overflow.
Silent correlation windows provide no delay evidence; record null, not a lag.

toml-cfg 0.2.0 finds cfg.toml through rustc's nearest output ancestor named
target, not CARGO_MANIFEST_DIR. Recipient rebuilds need their own firmware/target
and public cfg before compilation. Reusing the production target path can
import private settings. Copy native SDK/tools independently; venv paths do not
relocate reliably. See October 13 mp3-relink evidence.

A fresh SDK Python install adds cryptography Cargo manifests under .embuild.
The canonical check inventories those inputs, so its first build can link and
pass every stage while failing final boundary stability. Retain that failure;
complete dependency setup before the acceptance run. A public-config rebuild
and modified-library relink do not prove a complete distribution-source bundle.

Discrete byte TPDF with ties-up rounding is not the continuous NumPy candidate:
ideal zero-input mean is +1/512 output LSB. Keep arithmetic/RNG and decoder
receipts tied to each candidate. Standalone preprocessing plus the unchanged
FLAC encoder repeats quantization; its timing does not predict a fused path or
loaded streaming headroom. See October 13 quant-target evidence.

Workstation tc/netem cannot reach the Sonos stream: the speaker (.205) fetches
straight from the board (.217:8000), and no AP access exists. Receiver-side
shaping proves recovery for workstation listeners only. cv5a's "Sonos resumes
only when still owned" scenario stays an explicit unmet gate, also pending
pqqq's automatic resume; never report a netem trial as Sonos-path evidence.
Listener end reasons appear only on UART (`stream: ... left after ...`), never
in /api/metrics; every recovery trial needs the serial log open.

Harness verdict fixtures for a device seam must come from real captured UART
lines, not from a plan's description of them. The first network-fault review
found three blockers the green suite hid: hand-written fixtures omitted the
`socket error kind=ConnectionReset` departures every harness-closed listener
logs, assumed the buffered preset on the plain `/stream.mp3` path (it has a
flat 4 s media-age bound), and expected a WAV frame timeout. A later review
"corrected" that to MediaAge from reading stream.rs; the board then ended
the starved WAV reader with `stopped reading` at 3.05 s. Both bounds are
live and the tighter one wins: the WAV write limit (ring capacity / 48 kHz)
fires before the 4 s age bound. Grep docs/audit-evidence for the real line,
and treat a verdict expectation derived from source alone as unverified.
