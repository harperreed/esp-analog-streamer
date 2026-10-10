# Metrics observer crossover build

The diagnostic feature holds one firmware image fixed while metrics advances
through settling, OFF1, ON, OFF2 and finished. The planned deadlines are 0, 20,
100, 180 and 260 seconds since boot. Actual transition and ready timestamps
remain part of the evidence; skipped or missing phases invalidate inference.
This tests execution of the observer bundle without changing linked addresses
between phases. It does not isolate individual observer costs or prove a cache
mechanism. Existing delivery and raw-loss gates remain failed.

Supervisor health handling, task priorities and stacks, capture, codecs, rings,
and existing exports stay unchanged. Metrics owns transition logging. OFF skips
phase stores, state queries and publication, diagnostic rendering and the new
supervisor stack scan. Both due timers advance while OFF. Normal builds retain
the prior observer behavior. Combining metrics-state-crossover with encoder-probe
fails in the shared library, preserving ordinary encoder baseline semantics.

Four real numeric schedule tests failed before implementation and now pass.
The host workspace passed 452 tests with two existing ignores; Clippy and
formatting passed. Independent source review verified the exact record contract,
complete observer guards and same-core ordering. The first variant passed all
27 canonical stages, target compilation and IRAM validation. Its deliberate
incompatible-feature build failed at the intended library compile_error.

Linked review found that inline transition formatting enlarged the metrics
frame from 256 to 432 bytes. The final variant puts both transition messages
in a 128-byte out-of-line function that returns before existing exports.
Metrics now uses 304 bytes; supervisor remains 272. The extra 48 persistent
bytes and logger/interrupt nesting still need device measurement. Partial frame
sums do not prove stack safety. Original first-variant images remain intact.

Final diagnostic ELF SHA-256:
`8ef23d4fae8ecd16013e3eace538b373e461f11e266a64ac9756d457a142922c`.
Build command: `cargo +esp build --release --features metrics-state-crossover`.
The target build and IRAM check pass (10829 linked symbols checked). Saved build
receipt binds source hashes and features to this image; it is not a device
attestation. Inspection preserves original hash, size, mtime and both flash/IRAM
text sections. Linked branches independently confirm the intended OFF guards.

The throwaway acquisition tools add a timed observation to the existing owned
source/cleanup harness. They preserve raw UART, arrival offsets and reset release,
require genuine elapsed completion, and archive exact helper copies. Offline
analysis uses producer endpoints and exact count anchors; excluded prior windows
can anchor the next window. A canceled run cannot pass acquisition, and loss
cannot become an audio pass. No listeners are intentionally opened; unobserved
transient absence is not proven and delivery remains unexercised.

Independent harness review found two defects: loss in final/replacement boundary
counters could disappear, and matching saved files were not bound to acquisition
hashes. Both now have failing-before/passing-after regressions and clear scoped
re-review. Fifteen scratch host tests pass, including real escaped-child cleanup.
Host tests do not replace the forthcoming real UART/source run.

Final canonical check: all 27 stages pass on the revised source; log
`target/kata-hardening/check-metrics-crossover-v2.log`. Hardware crossover has
not yet run.
Evidence: target/kata-hardening/metrics-state/crossover-build-v2 and the source,
harness and linked reviews in the crossover SDD ledger/metrics-state directory.
