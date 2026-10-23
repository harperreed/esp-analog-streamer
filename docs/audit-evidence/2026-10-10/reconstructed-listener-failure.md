# Reconstructed current listener failure

Doctor Biz, the requested two-listener delivery failed, with actual encoder raw loss. The wrapper records acquisition_valid=false after an unclassified FFmpeg integrity error. Only the soak ran; the requested slow-listener scenario never ran. Cleanup completed with owned child return code 1, no cleanup errors, source return code 0 and empty aplay log. This is not a completed planned “both” acceptance run.

Final retained framehashes contain 2.064 s / 86 packets and 2.256 s / 94 packets. The earlier failure summary contains 2.064 s and 2.184 s / 91 packets; its second decoder snapshot predates further retained output. Both observations remain unchanged. stream_soak.py updates decoder jobs in order; the first integrity exception exits that loop. Its failure tail retains raw logs without updating decoder accounting, and finalization stops processes and closes accounting without another final read. The summary explicitly labels decoder accounting a pre-failure snapshot. The final file contains three additional committed packets after its last accounting read; their exact flush time is unrecorded. FFmpeg reports premature HTTP endings at 82560 and 90240 bytes, followed by demux I/O errors. No delivery or audio pass follows from valid decoding of those short prefixes.

| Complete window | Actual frame delta | Generation | Encode wall/frame | Listener endpoints |
|---|---:|---:|---:|---|
| 1 | 387 | 0.927110x | 23.346835 ms | 0→0 |
| 2 | 386 | 0.924607x | 23.517319 ms | 0→2 |
| 3 | 378 | 0.906973x | 23.589889 ms | 2→0 |
| 4 | 386 | 0.925918x | 23.319119 ms | 0→0 |

The same reconstructed image 626456a35e462d049a4f5bdacbff1421f04ae0223332eed9d0bdbacef32a6621 previously generated 0.999629x over eight complete no-listener windows, with weighted encode wall 21.442324 ms/frame. Here the first complete window already generates 0.927110x before either admission. This does not establish that listeners caused the slowdown. The mixed windows do not isolate joint-listener throughput. Cumulative MP3 anchors are 0, 387, 773, 1151 and 1537; Overrun retains the third window’s 773 baseline. No terminal partial window is invented.

Admissions occurred at 22.529997 s (peer :38108) and 22.941597 s (:38112). Raw Overrun at 30.646195 s records frames=1001, resets=1. Departures report FellBehind at 30.647615 s (:38112) and 30.650205 s (:38108), 1.420 and 4.010 ms later. In this reconstruction’s stream_pipeline.rs, raw Overrun resets the encoded ring before emitting Reset. flac_ring.rs increments generation on reset and returns the same Overrun for generation mismatch, capacity lag or invalid offset. Thus these closely following departures support reset invalidation; the label does not prove encoded capacity exhaustion. Raw loss itself remains real, and its cause remains unresolved.

Peer :38112 sent 86 frames / 82560 bytes; :38108 sent 94 / 90240. Their send-call wall totals were 5.995454 and 6.629276 s over payload elapsed 7.497493 and 8.116934 s. Both record zero short sends, WouldBlock, interruptions and send errors, with 29/32 calls over 10 ms. Accepted bytes and call wall time do not establish wire delivery or isolate buffer pressure. Decoder identity follows matching byte totals; the acquisition did not directly bind each FFmpeg process to its client socket port.

All retained telemetry exports report dropped=0 and loss_occurred=false. Four capture snapshots retain queue_overflows=14, software_dropped_frames=480, restarts/overflow_restarts/discontinuities=2 and settling_discarded_frames=26400; remaining fault fields are zero. Observed values remain stable through capture intervals after departures. The summary’s capture snapshot is aged 9.039 s at failure and 24.055 s at summary; its zero deltas cannot clear the failure interval. Exact complete-window capture brackets, including missing first-before/final-after brackets, remain in JSON.

Source receipt uses plughw:CARD=PCH,DEV=0 and the same stereo-noise-420s.wav hash 9311d9ca5dfe1a5f9269631afe22647834207f2b3239571474acc8ac15f3228d. Routing readbacks retain Headphone on, Line Out off, Master mono on/0 dB and both PCM gains 0 dB. Observed driven API level is −23.6 dBFS; MP3/Medium, recognition off, autoplay off and Sonos stopped persist. These receipts do not certify identical ADC samples or fidelity.

JSON retains raw artifact hashes, image/helper/fixture provenance, inspected reconstructed source hashes, the frozen ordinary parser hash and unarchived stream_format.py dependency hash. Full device image attestation remains false. Every retained acquisition artifact was rechecked unchanged. No tests, build, hardware action or production edit ran during analysis. Independent review verified the retained evidence and these limits.
