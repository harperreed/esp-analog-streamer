# PR 1 merge preparation

Doctor Biz authorized fixing and merging PR 1. The branch starts at 603df2d;
existing untracked Kata exports, notes and the device-metrics plan stay local.
The audit has 34 closed children and 14 open; its recovery goal remains open.

Reproduced the public-config example build failure. Keep the NVS diagnostic's
recognition-off requirement at runtime before any side effects, so ordinary
recognition-enabled builds can compile every example. The stale fuzz oracle
rejects HEAD despite production support; a retained seed and literal route
regressions cover it. The old full seeded fuzz run passes locally, so the
expired hosted failure cannot be attributed conclusively to that oracle bug.

Independent control review found queued Play can outlive completed Disable.
Recheck the existing admission policy at worker dispatch; tests exercise the
real queue and snapshot change. Streaming review found no merge blocker in
the critical paths it examined. Final canonical and hosted gates still decide
whether this candidate can merge. Known MP3/Sonos, RAM, transport-gap and
long-soak work stays open. The off-format settings UI also needs a follow-up:
it leaves controls enabled although the intentional API gate refuses them.

I mishandled a temporary config swap: after changing directories, its relative
restore path failed and the cleanup deleted the backup. I told Doctor Biz and
recovered the saved office config, checking credentials against the existing
production ELF. No firmware was flashed. Cleanup now uses an absolute path,
compares the restored file, and removes its backup only after success. This
was avoidable; safe cleanup must not depend on the current directory.

The first published candidate passed all 28 local canonical stages, with 820
workspace tests passing and two existing ignored tests. Hosted CI then failed
all seven jobs before checking code: espup 0.18.0's tag exists, but its release
and binary downloads return 404. Empty-cache installation reproduced the same
failure. Pinning the available 0.19.0 installer passes a fresh download and
matches the official asset digest. Its command accepts the existing ESP32,
standard-library and explicit 1.98.1.0 toolchain options; no firmware compiler
or SDK pin changes. Fresh espflash/ldproxy installs also pass. Mise's existing
optional usage-tool release-cutoff warning remains visible in those probes.
CodeRabbit skipped the PR because it exceeds its file limit; that green status
is not a code review. The independent reviews are the review evidence.
