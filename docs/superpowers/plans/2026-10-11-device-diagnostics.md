# Bounded heap and UART crash diagnostics (cxbw)

Status: complete against original Kata acceptance. Base e5cef5b. Parent owns
canonical checks, builds, flashing, capture, restoration and commits. Sol 6.1
workers own scoped implementation and independent reviews. Previous goal turn
closed ewvq; 23 katas remain. This task is not an audio-reliability release gate.

## Contract

Use pinned ESP-IDF 5.5.5/classic ESP32 standalone heap tracing and UART ELF core
dumps. Keep default firmware configuration and partition layout unchanged. The
diagnostic overlay is explicit and its generated config must be verified before
flashing. Use generated pinned headers rather than a hand-written trace ABI.

One standalone example runs without application startup, Wi-Fi or saved NVS
credentials. Compare the same fixture built without diagnostic hooks against
the diagnostic build; do not compare unlike production and isolated workloads.
Within the diagnostic image use OFF/ON/OFF bounded allocation loops, log outside
timed windows, and report time, task runtime, stack watermark and separate
internal/PSRAM free/largest/minimum values. Keep 64 trace records internally;
validate complete trace results for known live/freed blocks and reject overflow.
Use leak-mode tracing for repeated allocate/free timing so freed records do not
fill the fixed history. Report a bounded PSRAM sentinel checksum and explain
that external memory is mostly absent from the core dump.

The diagnostic fixture ends in a distinctive, deliberate IDF abort with a
resolvable fixture frame. The baseline fixture reports completion without a
dump. UART acquisition and official decoder execution have fixed byte/time
bounds, own only their resources, preserve failure receipts and keep raw dumps,
ELFs, temporary decode files and register/stack reports in a new private ignored
directory. Console output contains status/count/hash/path only. No whole-memory
printing. Never change permissions of existing paths.

Use installed esp-coredump 1.17.2, pinned-toolchain GDB 17.1_20260402 and the
verified ESP32 revision-3 ROM ELF. Preserve exact ELF/config/partition/tool hashes
and SDK revision. Official checksum/ELF validation remains authoritative; prove
matching-ELF decoding and rejection of a different real ELF. Synthetic framing
tests do not prove decoding. Restore the frozen normal image and verify stopped
MP3/Medium, autoplay/recognition off, and no UART/source owner.

SystemView stays off: use it only for a named task/ISR timing question with a
verified compatible transport. No incidental flash partition change.

## Units and verification

1. Device fixture/overlay/generated header metadata. Owner
   diagnostic_device_design_61. Target build and actual board checks belong to
   parent; worker supplies exact config path/delimiter and source references.
2. Importable bounded private UART capture/decode tool and focused host tests.
   Owner diagnostic_host_design_61. Reuse locked pyserial and official decoder;
   no custom crash decoder or extra package installation.
3. Parent integrates tests into scripts/check, reviews the scoped diff, builds
   baseline and diagnostic configurations, verifies effective flags and linked
   memory, runs the dedicated-device experiment, independently reviews receipts,
   restores normal firmware/config, commits and closes only if original criteria
   have evidence.

## Decisions

The original Kata is the acceptance authority; there is no separate architecture
spec. Existing examples and Python harness patterns make this a bounded addition.
Doctor Biz already authorized diagnostic hardening and dedicated-board tests.
Disjoint firmware and host files can be implemented independently; parent retains
integration and hardware ownership. A mismatch costs a scoped integration fix,
not concurrent writes to the same file. No push or merge is authorized here.

Normal restore ELF: target/kata-hardening/network-generations/images/production.elf,
SHA-256 e7404b41273f0b12fd55315dde025f81fd9b3d8a19771e49d234ef4ec7550ff7.
Normal bootloader and partition hashes remain in that image directory's manifest.

## October 11 checkpoint

Doctor Biz again flagged too much work without closures. The independent Sol
scope audit and parent review closed dp08 at 48520cf using its existing original
acceptance evidence. Open count is now 22. DMA failures remain 93hm; no failed
trial was reclassified. Do not add full streaming acceptance to this diagnostic
ticket either.

Device/host implementation and independent source review are complete. Host
tests passed (13); canonical scripts/check passed all 27 stages in
target/check-logs/run.o3YKR34X. The normal generated configuration again selects
no core dumps and no heap tracing. Baseline and diagnostic device allocation
fixtures passed their assertions; private images and baseline UART are under
backups/diagnostics/cxbw-images. The actual crash capture is
backups/diagnostics/cxbw-crash-1.

That capture FAILED official decoding with decode_timeout. Preserve the failure.
The Sol host diagnosis found no DWARF in diagnostic.elf; pinned GDB cannot resolve
TCB_t, and the official decoder waits ten seconds per failed type query. Bounded
stack sampling proves decoding reached GDB, not an ELF parsing hang. Private
diagnosis receipts: cxbw-host-stack-1 and cxbw-host-types-1 beneath the same private
parent. Next: build the optimized diagnostic with debug information retained,
verify TCB_t in the exact ELF before flashing, then capture/decode a fresh matching
dump. Do not simply raise the timeout. Wrong-ELF rejection, static section cost,
sanitized evidence and final review/commit remain required before cxbw closure.

Normal e7404b41 firmware and saved normal bootloader were restored successfully;
HTTP confirms stopped, active MP3, Medium, autoplay false. No UART owner or aplay
process remains. Source audio and Sonos playback were not started by this fixture.
Restore receipt: target/kata-hardening/device-diagnostics/restore-normal.log.

## Final acceptance

The optimized full-debug diagnostic ELF 8896d7e3 passed DWARF and GDB TCB_t
preflight. Real crash cxbw-crash-2 and saved replay cxbw-replay-1 decoded through
the official tool with the named intentional abort and reason. A different real
ELF failed official SHA-256 identity validation in cxbw-wrong-elf-1. The original
timeout remains failed. Matching full-debug baseline f2edac3d passed its device
assertions without a dump. Static DRAM overhead is 6,560 bytes, IRAM 1,068 bytes;
full runtime, heap and stack measurements and their limits are recorded in
docs/audit-evidence/2026-10-11/device-diagnostics.md and .json.

The first final canonical run failed because two retained SDK build directories
exposed the I2S checker's single-config assumption. A tested fix binds that check
to the successful Cargo build receipt and matching normal executable. Both
caches remain. All 27 stages subsequently passed; the exact run is in the
evidence JSON. Independent source and acceptance reviews found no blockers.

Normal e7404b41 firmware and its saved bootloader are restored. Final HTTP and
UART evidence verifies MP3/Medium/stopped, autoplay and recognition off. No
UART or source-playback owner remains. No flash layout or speaker settings
changed. Raw data stays private; only sanitized evidence is versioned.

Next functional ticket is 6vct: authoritative pipeline timestamps, source-age
policy accounting for intentional buffering, enforcement and encoder progress
checks. Existing dependencies are ready; do not wait for pqqq's unresolved Sonos
ownership-write contract to start that separate bounded work. Retire this phase
after its commit and Kata closure; the overall goal remains active.
