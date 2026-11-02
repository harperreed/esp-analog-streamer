# Reconstructed default controls

Doctor Biz, the original uninstrumented rate separation did not reproduce. Both reconstructed images generate near real time, with similar encode wall cost. The prerequisite for phase timing attribution fails; do not implement the phase experiment from this pair.

| Measure | Reconstructed archived | Reconstructed current |
|---|---|---|
| Weighted generation ratio | 0.999482345x | 0.999629159x |
| Window ratio range | 0.996741–1.000818x | 0.996916–1.001442x |
| Weighted encode wall/frame | 21.762754 ms | 21.442324 ms |
| Actual Overrun boundaries / final resets | 0 / 0 | 0 / 0 |

Frozen originals remain archived 0.961315744x and current 0.786533079x, with weighted encode wall/frame 22.302876 and 27.889090 ms. Both reconstructed images differ from their frozen behavior; two near-equal new images cannot establish historical reproduction.

Each new acquisition is valid and naturally completes the requested 90 seconds, with eight complete zero-listener windows, no listener lifecycle records, zero export drops/loss, empty unterminated tails, positive collector cleanup and source cleanup return code 0 with empty aplay logs. Actual cumulative MP3 anchors and frozen archived parser helpers determine frame deltas; Overrun never replaces a baseline. Missing startup/terminal capture brackets remain missing. Delivery was not exercised.

Capture faults remain startup observations: both retain software_dropped_frames=480, restarts=2, overflow_restarts=2, settling_discarded_frames=26400 and discontinuities=2. Queue overflows and other exact counters are retained in paired JSON. Stable observed counters and absent encoder overruns do not certify audio fidelity or end-to-end reliability.

Archived image is `fc1ec48175736d2063de9a786589cfefcc0de931265d47ca1b863eca7be71b55`; current is `626456a35e462d049a4f5bdacbff1421f04ae0223332eed9d0bdbacef32a6621`. Both ordinary build receipts declare features=[]. Shared frozen helpers match across new acquisitions: True. Individual analyses bind image and receipt hashes, own the ROM reset, retain routing/status and fixture receipts, and verify all acquisition files unchanged. Full device image attestation remains false; the analysis also records the unarchived transitive `scripts/stream_format.py` dependency hash. Every original artifact listed by the original public comparison also still matches its recorded hash; public MD/JSON hashes are retained in paired JSON.

These are clean-source-context reconstructions with present tools/configuration, not recovered historical binaries. Paired build inspection establishes source/tool/SDK/config evidence and explicitly limits absent actual MP3 compiler-command evidence. Do not infer a cache, observer, scheduling or codec cause from the convergence.

Recommendation: retain the failed reproduction gate and investigate behavior in this new generation context before phase instrumentation. Any listener workload is a separate acceptance question.
