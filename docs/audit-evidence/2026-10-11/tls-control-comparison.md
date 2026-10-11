# Native Sonos control comparison

Both the saved pre-decorrelation firmware and current candidate failed native
Sonos control on October 11. Each reported `Sonos send failed` and a TLS
allocation failure for 4,437 bytes. The new codec is not required to reproduce
this failure. This does not exclude its added 1,344-byte encoder payload from
affecting memory margin, timing or failure frequency. Neither run clears 93hm
or pqqq, nor certifies capture, playback recovery or a streaming release.

## Procedure and results

Parent flashed each archived ELF with the same pinned bootloader/partitions,
then ran the reviewed throwaway `target/kata-hardening/tls-allocation/control-run.py`.
Its SHA-256 is `84078e91fda9236cb1ebeca04c2e45f208d59ba411411a8275d3853ae5a6322a`.
Each run used the same 180-second stereo noise fixture, front analog output,
FLAC/Medium and one native Play request. The driver stopped at the first
observed Playing/Error state; the firmware can retry within that interval.
No debug listener was added. Source, settings and UART cleanup were checked.

| Image | First pre-connect free/largest internal bytes | First connected free/largest | Recorded 4,437-byte failures | Result |
| --- | --- | --- | --- | --- |
| Baseline | 45,423 / 36,864 | 19,055 / 10,240 | 2 | FAILED |
| Candidate | 44,079 / 34,816 | 17,663 / 9,216 | 1 | FAILED |

These are sequential observations, not a statistical comparison. Heap
snapshots precede the failed writes and do not identify the competing
allocation. Different failure counts do not establish an improvement.

An initial candidate invocation failed before any recorded device operation:
HTTP was not ready after flashing (`ConnectionRefusedError`). Its failed
receipt remains under `candidate/`. After a successful read-only readiness
check, the actual candidate control trial ran under `candidate-ready/`.

## Provenance

Private artifacts remain under `target/kata-hardening/tls-allocation/`;
they include private Sonos metadata and must not be published wholesale.
Image hashes identify the operator-selected files, not device attestation.

| Artifact | SHA-256 |
| --- | --- |
| Baseline ELF | `8fb4228ff4abff83cc39b5a63eea2a301865f83947f3ec6181989cdd857ac81a` |
| Candidate ELF | `32a09ef74ea266905b59048a67705f5a428f196c3656045efe2ca1c6cb4aa0be` |
| Fixture | `027accd2d3a6b52bb7a80587c7e567ce85509aa02e7cff0f0ae7a447216959b6` |
| baseline/result.json | `f34b6a1eeff774e47938434015e640f4ab1feb7a3d3b1db195a1f14b7518771f` |
| baseline/uart.log | `281a1ac3c850d5c9b0e7c9de8051be1a10997da044cf4640bd4710d6bd398ab8` |
| candidate/result.json (preflight failure) | `0276f55639df08a9cd73bc3d36fdf092c435fe4737049a9dc899f742f4b77772` |
| candidate-ready/result.json | `15bfca2927c94d7f6063c33936d21b3c93067ed3e536e9f9b1b76860474ece03` |
| candidate-ready/uart.log | `1b8fce59387f8136ca1b1d48e002bc583b06c3db042c483fd4d7b73e95aa9aa4` |

Both control trials restored MP3/Medium/stopped, autoplay and recognition off,
and joined the source/UART helpers without cleanup errors. Mixer and volume
were not changed. Cleanup does not promise restoration of a prior Sonos media
URI; ownership checks are observations, not atomic exclusion of other controllers.
The current candidate remains installed. Production source did not change;
the previous 27-stage canonical pass is not a fresh run or a hardware pass.

Close-or-block decision: retain these failures in the existing capture/recovery
work, leave the 19 remaining Katas open, and proceed with independent amqe
implementation. Further TLS work must target a specific missing observation
or tested fix; this comparison does not justify another general soak.
