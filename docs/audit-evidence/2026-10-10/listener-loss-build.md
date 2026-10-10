# Listener loss reason build checkpoint

The eight-slot listener queue now counts full, contended and poisoned publication
failures separately. Each failed attempt increments one wrapping reason counter,
the existing total and the sticky loss flag. Publication still tries once without
waiting or allocating; checked close and resource release retain their behavior.
The existing exporter appends the three counters to its receipt.

The original nine-field acceptance parser and aggregate/sticky loss rejection
remain unchanged. A separate strict diagnostic helper requires the three reason
fields when attribution is requested. It cannot clear acceptance failure. Because
the atomics are sampled separately, a snapshot need not have matching total and
reason sums. Missing attribution is not zero loss.

Actual queue tests cover full, held and poisoned storage, successful publication,
reason/total wrap and sticky loss. Existing stalled-exporter real-socket cleanup
tests remain. Real-file parser regressions cover LF completion and missing,
duplicate, malformed or overflowing reason fields. Red logs record absent APIs;
green host checks pass. Independent review found no blocking defect.

Canonical `scripts/check` passed all 27 stages. The separate probe build and IRAM
check passed; the latter inspected 10823 linked symbols. `shazam_mp3_receipt` is
absent from default and present in probe. The workspace run reported 423 passing
tests and two existing ignored tests; focused listener tests passed 15/15 and
probe-host listener tests 16/16. Harness module tests passed 19/19.

| Artifact | SHA256 |
| --- | --- |
| Default | `0fd529101a44fd3633ca590174afe32a431c2f64d8a4b72e46b5d5224fd8fd22` |
| Encoder probe | `b7de9f287e793b3bb393cafffa01b0b34b53715e524051f0ac15b8ba86817d7b` |

Both linked images grow the queue symbol from 2008 to 2024 bytes. DRAM data grows
16 bytes and BSS stays unchanged. Metrics and capture-report frames remain
208/528 bytes; the exporter grows from 992 to 1008 bytes. These are linked layout
measurements, not live heap or stack margins. Original ELF hashes and mtimes
remain unchanged; disassembly copies preserve executable bytes exactly.

At this checkpoint no new image has been flashed. The earlier lost record remains
unattributed and all failed trials remain intact. A bounded device run must check
the new receipts, watermarks and cleanup; zero loss in a smaller workload cannot
explain the earlier failure. Full 21tp and delivery acceptance remain open.

Local evidence: `target/kata-hardening/listener-loss-{implementation,review,layout}.md`,
the layout reproducer/JSON, `check-listener-loss.log`, and both exact images under
`listener-loss-device/` in that directory.
