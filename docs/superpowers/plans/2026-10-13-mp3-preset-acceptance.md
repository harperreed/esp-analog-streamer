# Measure MP3 preset cursor admission

Doctor Biz's e8hd acceptance still lacks exact device preset offsets. Earlier
concurrent-stream overlap conflated admission skew with the selected history.
This bounded gate measures the real selection, with no workstation audio or
Sonos playback required. It does not close e8hd's broader reliability scope.

Use a throwaway passive diagnostic at the actual compressed-listener admission:
capture ring generation/head, selected cursor and device time under the same
existing lock. Emit the receipt with peer, actual format and preset after the
lock is released. Do not change selection, encoding, deadlines or media bytes.
Archive and remove the patch after acquisition.

The contract is `head - first_sent_sequence + 1 = retained_frames`, since
receipt sequences are one-based and cursors point to the next frame. Expected
MP3 targets are 42/167/334 frames, or 1.008/4.008/8.016 seconds at 48 kHz and
1,152 samples per frame. Also verify head minus selected cursor directly.

One fresh boot of the verified diagnostic image, stopped MP3/Medium,
autoplay/recognition off, idle listeners and unchanged configuration. Wait for
enough history. Acquire Low, Medium and High sequentially using the existing
debug HTTP query; do not write saved settings. Each receives at least 64 whole
960-byte MP3 frames, then closes its owned socket. Independently decode and
count actual samples. Bind each request's real local peer/port to one admission,
join, positive delivery, stream-media receipt and checked close. Require the
first delivered sequence to match the actual admission cursor plus one, same
generation, expected target, bounded slots and loss-free event export covering
the owner's close. Preserve every failed attempt; no unchanged reruns.

Require clean startup/steady capture counters, no raw encoder loss, stable
boot/image/network lease and settings, then restore exact ordinary source/ELF
and stopped/idle state. This is quiet-ADC source-history evidence only. It does
not prove playout protection, analog calibration, a network-fault recovery path,
or long-term delivery. No AP, hosted authentication, mixer/ACL or TLS changes.

Parent owns canonical checks, builds, hardware, integration and commit. Sol 6.1
workers own the small temporary source patch and a bounded private driver;
independent review precedes the device run. Reuse existing real parsers and
decoder checks, with regression tests for admission skew, cursor/first-frame
mismatch, reset, incomplete evidence and incorrect preset. Finish this gate or
record its exact failure before choosing another experiment.

## Result

Passed on the first live acquisition: 42/167/334 retained frames and exactly
73,728 decoded sample frames per 64-frame prefix. Full UART tail passes; exact
ordinary sources and ELF restored, final board and remote stopped. See
`docs/audit-evidence/2026-10-13/mp3-preset.md` and JSON for hashes and limits.
All 27 diagnostic canonical stages, five restore stages and 20 driver tests
pass. No Kata closes: e8hd broader recovery/endurance/fidelity gates remain.
This phase has one compaction; commit the evidence before fresh continuation.
