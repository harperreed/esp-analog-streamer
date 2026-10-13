# Independent recipient rebuild review

Doctor Biz, no actionable findings remain. The retained evidence supports a
local recipient source modification and full firmware relink with public
configuration. This review does not authorize distribution or deployment.

The source archive matches revision
bfea1d40adc401205ac67ec76d9ad411b9f78215, per-file hashes/modes and the initial
recipient extraction. SHA-256:
9f8ca3d84b3d369844800d938e7d596a5e36eeec766c5f2ee2b5cd561158690f.
Its 232 members comprise 230 tracked source files, a generated build guide and
the embedded manifest. Required ordinary firmware and firmware-only canonical
check inputs appear complete. The package reads committed blobs through a
positive allowlist. The original private cfg.toml, target outputs and production
images were not read for this review.

The first fresh-target build linked successfully and all five stages passed,
but run.VG4kYsui failed its final configuration boundary: native bootstrap added
seven cryptography manifests beneath the newly created .embuild Python venv.
The retained diff agrees with that explanation. This original run remains
incomplete with overall exit 1. The unchanged-source follow-up run.ZAtxchL1 and
modified run.EtYxJOgq both completed with exit 0, all five stage exits zero and
identical start/end HEAD, status, diff and config snapshots. The follow-up
baseline artifact was Cargo-fresh; the modified artifact was rebuilt. Calling
the initial cold run a passing canonical check would overstate the evidence.

Every retained-artifact hash in both receipts matches its file. The current
recipient differs from the source manifest only in mp3/vendor/shine/layer3.c,
whose retained patch changes config->mpeg.ext from 0 to 1. Public configuration
matches its retained SHA-256; lock hashes, effective sdkconfig and generated
linker script match across the pair.

The final baseline ELF is
871e3fd5321a0ef65edb35f7c003f28d120cc6a26004775e4a273287297bc574;
the modified ELF is
f06bf899de36cb9446a2619b8a0de9b49fd972fe2ed4194d0f60dc8507c82637.
Their retained disassemblies show zero stored at offset 88 at 0x400844ff in
the baseline, and one stored at offset 88 at 0x4008450e in the modified image.
Read-only ELF section-byte inspection agrees with those instruction bytes.
The target compiler offset fixture identifies mpeg.ext as offset 88.
shazam_mp3_open calls the linked shine_initialise at 0x40084480 in both images.
This establishes the reachable C change in the final target firmware, beyond
an intermediate library or differing image hash.

The identical sets of 26 prefixed Shine/bridge function symbols remain in
.iram0.text. The identical linker scripts select instructions and literals
from the ten C object members inside the MP3 rlib; named-symbol checks alone
do not exhaustively measure literal placement. Retained table symbols keep
their baseline flash/DRAM placement. I2S placement checks pass and
CONFIG_I2S_ISR_IRAM_SAFE=y in both images. The initial collector incorrectly
expected 27 prefixed functions; its retained correction compares the observed
sets and required entry points. This is static placement evidence; it does
not establish runtime allocation capabilities, encoder keep-up or hardware
continuity.

The host experiment uses the same baseline MP3 source hashes as the recipient
manifest and changes only layer3.c. Its 96 frames differ only at byte 2 bit 0;
decoded PCM is identical for 110592 sample frames, with SHA-256
2f7e977fc1490e8f2b72bbdba75edce5b67757512dea18bc8ccf2d0c9f5f33cc.
Unchanged-library and extra-payload-change controls were rejected. The parent's
independent decoder receipt agrees. This demonstrates the bounded host fixture,
not execution of either target firmware image.

External Rust/Cargo/mise caches and copied SDK/tools/dist inputs were reused;
the native Python environment and managed components were prepared locally.
The archive is neither a complete dependency/toolchain bundle nor an offline
build proof. Public-config images do not establish correspondence to an existing
private production image. Complete-firmware license compatibility, distribution
clearance, hardware and streaming acceptance remain outside this proof.

This reviewer performed read-only source/artifact inspection, hashes and ELF
byte inspection. No builds, tests, flashing, hardware, audio, Kata or Git
mutations were performed. Only this review artifact was written. The
machine-local file ~/.codex/docs/local.md was absent.
