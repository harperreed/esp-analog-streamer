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
