# MP3 instruction placement

A matched instruction-placement experiment restores encoder keep-up on the
tested workload. The control repeatedly loses raw audio with no listener; the
IRAM candidate has no raw overruns through zero, one and two listeners. This
supports a bounded placement correction. It does not isolate a particular cache
mechanism, clear long-soak/recovery gates or establish analog sample identity.

| Measurement | Flash control | IRAM candidate |
| --- | ---: | ---: |
| Zero-listener complete producer windows | 6 | 6 |
| Zero-listener audio seconds per wall second | 0.725451 | 1.000038 |
| Zero-listener mean encode wall time | 29.787 ms | 18.963 ms |
| One-listener complete producer windows | 0 | 2 |
| One-listener audio seconds per wall second | unavailable | 0.999936 |
| Two-listener complete producer windows | 0 | 5 |
| Two-listener audio seconds per wall second | unavailable | 0.999972 |
| Full UART Overrun boundaries | 27 | 0 |
| Actual receiver overlap | 0 s | 59.824 s |
| Added capture faults across bracketed active phases | 0 | 0 |
| Acquisition / recorded acceptance | complete / failed | complete / passed |

Both use the same 420-second noise fixture through the confirmed front jack,
MP3/Medium, stopped Sonos, recognition/autoplay off. No mixer or Sonos volume
settings changed. Each run has its own owned RTS reset, 75-second zero-listener
phase, then a 90-second receiver phase with the second client admitted after
30 seconds. RTS reset is not a physical power cycle. All source, receiver and
UART owners were released; neither acquisition reports cleanup errors.

The candidate's saved prefixes independently decode to 4,286,592 and 2,839,680
sample frames, exactly matching their complete MP3 frames. Neither peer reports
premature EOF. The control decodes only 27,648 and 32,256 sample frames, with
early EOF and no simultaneous receiver overlap. Exact prefix counts cannot
prove sample-exact ADC input or rule out every physical source discontinuity.

Candidate joint receiver delivery is approximately 0.9893/0.9889 audio seconds
per wall second. Keep that measured limit visible despite the bounded harness
pass; a healthy encoder does not prove indefinite transport throughput. Existing
send/poll diagnostics remain present. Their post-client-close tail includes
socket errors, distinct from premature stream failure during the workload.

Nine capture fault deltas are zero in both bracketed active phases. Startup
capture faults remain nonzero and are retained in the companion JSON. The
candidate has no raw Overrun boundaries or reported export loss. Complete
producer windows use device timestamps and exact epoch/count anchors; missing
windows cannot be used to fabricate per-window throughput.

## Controlled build change

Both images come from one live linker invocation's identical objects. The C
archive is byte-identical to the production archive. Only a copied linker-script
selector and output/map paths differ. The candidate moves the ten bundled C
members' instructions/literals into IRAM; Rust wrapper objects and codec tables
keep their placement. No codec setting, C compilation flag, task priority,
runtime buffer or allocator policy changes.

All 66 retained C instruction/literal rows move. All 44 C data/rodata rows keep
their addresses, sizes and bytes. Link relaxation and global code addresses
change, so this is not an isolated cache measurement. IRAM free space changes
from 28,352 to 14,460 bytes; linker fit assertions and both I2S IRAM checks pass.

The default image is `8bc907c549ecd0ba4e3db4b2e812961f81b3c34df77f2840c37b51daa372fba3`;
the candidate is `bd8392cd041355bfd7a7fbd9b9eded48d741cf4e70600a2595293a4bec755490`.
Saved images, host flash receipts and settings checks are provenance, not
independent device attestation. The first helper invocation failed before linking
because it omitted two-token library paths. The corrected helper also selects C
members inside Cargo's rlib rather than an unlinked standalone archive.

## Production follow-through

The normal build now derives its linker script through pinned embuild APIs,
inserting C-member selectors into a Cargo-local copy of ESP-IDF's script. It
preserves the original, rejects missing/ambiguous inputs and excludes Rust
wrapper/data sections. Seven focused tests pass after an initial four-test red
receipt. Independent source review found no blocker.

All 27 canonical stages pass in `target/check-logs/run.WKEIQEGi`, including target
compilation, warnings and I2S IRAM placement. Its ordinary production ELF is
byte-for-byte identical to the tested IRAM image above. No second flash or
duplicate hardware run is necessary to verify that ordinary build's bytes.
The same image remains installed with stopped MP3/Medium, recognition/autoplay
off; no test source or UART owner remains. This closes the scoped wbz6 encoder
regression, not the streaming release, sender-overhead or long-duration gates.
