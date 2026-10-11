# Current shared MP3 wrapper cost — October 10, 2026

Doctor Biz, all four isolated fixtures fit their 24,000 µs block budget. Each
completed 128 measured calls without an encode failure or a call over budget.
This rejects the narrow hypothesis that today's isolated wrapper necessarily
misses the budget on these fixed inputs. It does not clear live audio or Kata.

| Repeated block | Mean µs | Maximum µs | Calls >24,000 µs |
|---|---:|---:|---:|
| Zero | 10045.078125 | 10137 | 0 |
| DC | 12870.0859375 | 13152 | 0 |
| Tones | 16896.625 | 17173 | 0 |
| Noise | 18182.125 | 18404 | 0 |

Each fresh encoder received one separate warmup, then 128 measured calls.
Warmup costs were 10,982 / 17,733 / 17,187 / 18,324 µs respectively; none
exceeded budget. Every success represented 1,152 stereo frames at 48 kHz and
produced 960 bytes at 320 kbps. The measured minimum acquisition stack headroom
was 5,540 bytes on the 8,192-byte worker stack; zero warmup reported 5,924.
All eight batch runtime intervals were valid, without counter wrap or saturation.
Exact phase sums, checksums, wall/runtime counters and artifact hashes are in
[current-mp3-cost.json](current-mp3-cost.json).

The throwaway example source-includes production `stream_codec.rs` and
`threads.rs`, runs on core 1 at priority 8, and starts no application capture,
ADC or network workload. It times only `Encoder::encode_block`: enum dispatch,
output conversion/resizing, Rust/C calls, Shine, header checks and copying.
`push`, fixture construction, checksums, logging and explicit one-tick delays
fall outside each bracket. Wall time includes preemption. Whole-batch task
runtime includes work between calls and interrupts charged to the task; it is
not pure codec compute.

The four fixtures preserve the earlier benchmark's formulas: zero; left/right
DC +12,000/−12,000; 997/1,703 Hz tones at amplitude 16,000; and signed i16
xorshift noise seeded with `0x12345678`. Each generates one 2,304-sample block
and repeats it. These are neither continuous random samples nor phase-continuous
tones. Shared allocator callbacks retain their SPIRAM/8BIT policy; boot receipts
classify the fixture PCM as external RAM and the output buffer as internal.

The frozen example ELF is
`868ed4addad62552e6bbf595b13f02148b648582dc7291b6fa5da9e9dd3da69d`.
Its linked C archive exactly matches the frozen production archive,
`af005426add1c4a071a20551e6712dc59767971f0acc6262496aadf7834ae148`.
The build receipt retains all ten successful C compile commands, object hashes,
compiler identity and generated configuration. Archive equality does not imply
identical standalone executable placement or attest the running device image.

The parent owned one ROM-reset-to-complete UART acquisition, bounded to 120
seconds and 2 MiB, with a 240-second limit per flash. The raw log contains all
four fixture-end markers and the final successful completion marker. Parser
replay matches the saved result. Completion establishes receipt acquisition,
not worker-thread teardown. The UART retains the SDK's old-I2C-driver warning;
no panic, watchdog, reset during acquisition or encode failure was accepted.

Restoration succeeded in `finally` to exact frozen unified B ELF
`dc9084526287470ce20796a1be14d19c578332224f6f757eb3bda26e9a9902da`,
with matching bootloader and partitions. B is the experimental listener-priority-8
candidate; current source retains listener priority 7. Fresh post-restoration
status confirms stopped MP3, Medium buffer, autoplay off and recognition off.
Parent postrun checks found no UART or known `aplay` source owner and confirmed
unchanged production input hashes. No full-flash readback or device image
attestation was performed. Acquisition and restoration error lists are empty.

Validation retained 27 passing canonical `scripts/check` stages, explicit
example format/build checks, 20 passing parser tests and separate implementation,
build/link and parent-acquisition reviews. Debug C output exposes three expected
cc 1.6 compiler-detection warning lines from its unsuccessful `-?` probe; all ten
actual C compiles exit zero and the ordinary canonical warning check passes.
These records are hashed in the JSON; private device status is not copied here.

Archived [example](current-mp3-cost-probe/current-mp3-cost.rs),
[parser](current-mp3-cost-probe/parse_receipt.py) and
[parser tests](current-mp3-cost-probe/test_parse_receipt.py) preserve the diagnostic.
The example's relative module paths assume its original `firmware/examples`
location; the archive is evidence, not a new build target. The parent acquisition
script stays in ignored evidence because it contains machine-specific addressing;
its hash is retained. Run parser tests from the repository root with
`uv run --locked python -m unittest discover -s docs/audit-evidence/2026-10-10/current-mp3-cost-probe -p test_parse_receipt.py`;
saved UART can be parsed without hardware.

The next measurement plan remains pending. It should distinguish input-dependent
cost from application conditions, using a controlled comparison within the same
image if that approach is chosen. These data do not locate the live slowdown or failing
radio hop, establish ADC continuity, prove MP3 more reliable than FLAC, or satisfy
capture/listener/Sonos acceptance. Keep those failed gates open before shipping.
