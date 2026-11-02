# FLAC buffer checkpoint

Doctor Biz requested a buffer after the receiver traces showed multi-second
delivery gaps. This bounded first step expands existing encoded history without
changing startup behavior. Sol agents implemented disjoint codec and firmware
changes; an independent Sol review found one fixture timeout, now fixed.

Status: source committed as `53b7d67` on `wip/kata-capture-protection`, based
on `a0ea788`. Access is restored, all 20 canonical checks pass and production
with the buffer has passed two-listener music and noise trials, 120 seconds each.
The encoder-fault test and final 30-second two-listener production check passed.
Configuration is unchanged, controller stopped, autoplay off and host playback
ended. Kata `qh4t` is closed with evidence. The earlier
sandbox failures below remain historical receipts, not the current access state.
Compaction count at this checkpoint: 3 across the continuing session.

| Profile | Slots | History | Payload bytes | Frame write deadline |
| --- | ---: | ---: | ---: | ---: |
| Recognition off | 141 | 12.032 s | 2,312,259 | 8.021 s |
| Recognition on | 24 | 2.048 s | 393,576 | 2.048 s |

The exact frame bound is 16,399 bytes, including the longest frame number and
verbatim stereo payload. The off profile adds 705 payload bytes and 188 bytes
of ESP32 length metadata versus the former 94 oversized slots: 893 bytes total,
excluding allocator bookkeeping. Socket deadlines remain independent of storage.
Listeners still start at the current head. More retained history is not proof
of a receiver playback cushion or a fix for sustained insufficient bandwidth.

Verification retained in `target/flac-buffer/`:

- Worst-case noise and maximum renumbering growth fill the exact allocation;
  independent CRC checks and Claxon verify decoded PCM. Twenty codec unit tests
  and seven codec integration tests pass, including real FFmpeg decoding.
- Three new history tests pass after failing against 94 slots. An 11.008-second
  delayed reader retains exact encoded frames across wraparound while another
  reader advances. Exact capacity and overrun behavior remain bounded. Seven
  existing ring tests pass.
- Fourteen decoder-integrity tests pass using the installed cached Python/numpy
  environment. The fault fixture holds the encoder for 17 seconds; EOF remains
  bounded at 16.532 seconds. Its deferred marker wait is 30 seconds, covering
  lookup, suspension and delivery. The actual matcher passes virtual-time
  boundary tests against a real temporary UART file. This does not test hardware.
- Historical sandbox attempt: `scripts/check` passed 14 of 20 stages, including
  formatting, Clippy,
  Loom, harness checks, firmware release build and linked I2S IRAM checks.
  Rust socket tests, TLS peer, relay and throughput stages failed because socket
  creation was denied. Stream-format and decoder-integrity entry points could not
  fetch numpy because DNS was unavailable. The direct cached decoder run above
  passed; that canonical attempt remained red. The later unrestricted run passed
  all 20 stages. No tests were disabled.
- The `encoder-faults` release example builds. Harness checks pass again after
  the review fix. Independent review reports no remaining source findings.

Kata updates applied after hardware verification:

- `qh4t`: closed after tight-bound tests, both profile builds, all 20 canonical
  stages and actual two-listener music/noise memory/stream checks passed.
- `dctv`: startup buffering remains separate. A four-second delay preference was
  asked but has no recorded answer. Before adding it, define current-generation
  history selection and test receiver burst acceptance, exact audio and reset.
- `pd6c`, `dp08`, `ee3q`: existing transport, capture and release gates remain open.
  `dp08` received the passing encoder-isolation and steady-capture evidence;
  startup faults and full recovery/release acceptance remain unresolved.
- `cn4m`: received the measured channel reversal with source and decoded tones.

Next: decide the startup-buffer/continuity policy in `dctv` separately, then
test actual receiver buffering and Sonos behavior. Do not extend this checkpoint
into an unbounded soak or treat a short pass as release acceptance. Preserve
manual Stop, the verified front-jack input routing and final stopped state.

Raw receipts and agent reports: `.superpowers/sdd/flac-buffer-bound-report.md`
and `.superpowers/sdd/flac-history-capacity-report.md`. Public evidence is in
`docs/audit-evidence/2026-10-08/flac-buffer.md`. No continuity improvement
on Sonos has been measured for this patch.

Historical access receipt: Doctor Biz authorized testing while the office was
empty. No further noise approval was needed; output routing still required
verification. That access check found no `/dev/ttyUSB0` or `/dev/snd`.
The board-state request failed before connection with `failed to open socket:
Operation not permitted`. The session restriction did not establish whether the
board was offline. No flash or playback occurred in that attempt. A fresh run passed all ten
codec/history integration tests and fourteen decoder-integrity tests, saved as
`target/flac-buffer/authorized-retest-{rust,decoder}.log`. Hardware trials required
a local execution session exposing the board, audio devices and network sockets.
That access was subsequently restored and used for the checks above. Doctor Biz
then confirmed the source cable is in the front Headphone jack, not rear Line Out.
The active test routing enables only that front analog output; rear Line Out is
muted. Both tones reach the board, but their channel order is reversed (`cn4m`).
