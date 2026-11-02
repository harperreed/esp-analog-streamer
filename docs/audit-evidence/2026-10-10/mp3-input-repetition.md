# MP3 input repetition — October 10, 2026

Doctor Biz, fresh deterministic noise and both repeated-block controls fit the
isolated 24,000 µs MP3 block budget. End this isolated input branch. This result
does not clear live audio, ADC, radio, receiver or recovery acceptance.

| Condition, in acquisition order | Measured calls | Mean µs | Maximum µs | Calls >24,000 µs |
|---|---:|---:|---:|---:|
| Repeated A | 128 | 18546.359375 | 18768 | 0 |
| Fresh | 128 | 18518.0 | 18768 | 0 |
| Repeated B | 128 | 18546.859375 | 18768 | 0 |

Three fresh encoders each received one separate warmup, then 128 measured calls.
Warmups were 19,180 / 18,625 / 18,637 µs; all succeeded within budget. Every
success represented 1,152 stereo frames at 48 kHz and produced 960 bytes at
320 kbps. No encode failures occurred. Minimum acquisition stack headroom was
5,636 bytes from an 8,192-byte worker stack; the first warmup reported 5,668.
All six runtime intervals were valid, without wrap or saturation. Exact phase
sums, runtime/wall counters, input identities and hashes are in
[mp3-input-repetition.json](mp3-input-repetition.json).

All conditions regenerate 2,304 full-scale signed-i16 samples before every call,
using xorshift 13/17/5 with seed `0x12345678`, then store each sample shifted
by eight bits for the shared encoder. Repeated controls reset the seed for every
block; fresh carries state through warmup and all 128 measured blocks. Generation,
input checksum and push stay outside the shared `Encoder::encode_block` timer.
They remain inside the batch interval. Task runtime counts scheduled work,
delay-call overhead and charged interrupts, excluding time asleep; batch wall
time includes sleep. Neither measures pure codec compute.

The independent Python input oracle matches all six FNV-1a checksums and final
PRNG states, including fresh continuation across 129 blocks. Repeated measured
input checksums are both 2,246,642,629; fresh is 111,076,412. Repeated final states
are 3,670,521,071; fresh ends at 638,221,377. Both repeated measured output
checksums are 3,566,123,461, while fresh is 3,336,232,499. These 32-bit checksums
support input/control identity; they are not exact decoded-audio proofs.

The example source-includes unchanged production codec/thread modules, uses
core 1, priority 8 and an 8 KiB stack, and starts no application capture, ADC,
network or receiver workload. The frozen example ELF SHA256 is
`774da7fbcde9d632349c8dcbe47ccf10099035da6caebb095cdc60457b079dbc`.
Its linked C archive exactly matches the frozen production archive,
`af005426add1c4a071a20551e6712dc59767971f0acc6262496aadf7834ae148`.
This establishes archive identity, not identical standalone executable placement
or attestation of the running device image.

One parent-owned acquisition completed all three condition-end markers and the
final 3/3 completion marker within the 120-second/2 MiB UART bounds. Each flash
had a 240-second limit. Raw parser replay equals the saved receipt; acquisition
and restoration error lists are empty. Completion proves receipt acquisition,
not worker teardown. Parent process exit was zero.

Restoration succeeded to exact frozen unified B ELF
`dc9084526287470ce20796a1be14d19c578332224f6f757eb3bda26e9a9902da`
with its matching bootloader and partitions. B retains experimental listener
priority 8; current source retains listener priority 7. Fresh status confirms
stopped MP3, Medium buffer, autoplay off and recognition off. Parent independently
verified source/artifact hashes and no UART or known `aplay` source owner.
No full-flash readback or device image attestation was performed. Private device
status and machine addressing stay out of this public evidence.

Validation retained 27 passing canonical stages in `run.LZpbFw91`, explicit
example formatting/build success, 29 passing parser tests, and independent
implementation and build/link reviews. All ten actual C compiles exit zero.
Debug output retains three expected cc compiler-detection warning lines from
its `-?` probe; the ordinary canonical warnings check passes. UART retains the
SDK old-I2C-driver warning. No panic, watchdog or acquisition reset was accepted.

Exact archived [example](mp3-input-repetition-probe/mp3-input-repetition.rs),
[parser](mp3-input-repetition-probe/parse_receipt.py) and
[tests](mp3-input-repetition-probe/test_parse_receipt.py) preserve reproducibility.
The example's relative modules assume its original `firmware/examples` location;
it is evidence, not a new build target. The machine-specific parent runner is
hashed, not copied. From the repository root, run:

```sh
uv run --locked python -m unittest discover -s docs/audit-evidence/2026-10-10/mp3-input-repetition-probe -p test_parse_receipt.py
```

One fresh phase in one boot establishes neither repeatability nor causation.
Fresh input did not reproduce the isolated budget failure under these conditions;
it does not explain the live slowdown or failing radio hop. No further automatic
input matrix or probe follows this result. The selected next work is the accepted
concurrent-admission plan for Kata 8s9m, pending implementation. Existing live
audio and recovery failures remain open.
