# Archived FLAC-selected WAV input attempt

Doctor Biz: acquisition completes successfully, but **input_fixture_valid=false**. The WAV child saves 421964 bytes (44-byte header plus 70320 stereo PCM frames, 1.465 s), then fails EOFError('truncated WAV body') with exit 1. Required input is 2304044 bytes / 384000 frames / 8 s. This single archived FLAC-selected attempt provides no accepted representative fixture, current-image FLAC pair or completed downstream host comparison.

Numeric peer 192.168.23.153:36500 joins 29.523357 s, departs 33.051651 s “fell a ring behind,” and closes 33.233571 s. Producer order join≤departure≤close holds; close follows departure by 181920 µs. Frozen fixed checked_listener_closes resolves this numeric peer and validates a later loss-free close-covering receipt. Three listener export receipts report dropped/full/contended/poisoned=0 and sticky=false. Archived validate_input exactly reproduces saved body failure; separate UART checker correctly rejects the failed departure. No validator is overridden.

Conservative capture bracket is 20.204715 s end to 40.246749 s begin, with ordered snapshots and all nine fault deltas 0. Whole-trial capture counters remain at startup baselines: queue_overflows=15, software_dropped_frames=480, read_errors=0, restarts=2, supervisor_restarts=0, overflow_restarts=2, settling_discarded_frames=26400, discontinuities=2, health_restart_requests=0. Startup faults remain. Bounded counter stability does not prove coherent state between snapshots or clear the actual WAV ring loss.

Unlike prior MP3-selected attempts, this boot remains ordinary FLAC epoch 0 from Startup baseline 4.317722 s. Every complete report has block_samples=4096, not 1152. Generation uses actual producer spans and cumulative frame deltas multiplied by 4096/48000; first baseline count 0 and subsequent complete endpoints supply exact anchors. Eight windows each generate 118 FLAC frames; weighted generation is 0.999743x. No Overrun boundary or positive encoder reset/raw-overrun field appears in retained complete evidence.

|Window|Producer interval(s)|FLAC frame delta|Generation ratio|Task runtime(s)|Raw overruns / cumulative resets|
|---|---|---|---|---|---|
|1|4.317722–14.403135|118|0.998406x|1.786179|0 / 0|
|2|14.403135–24.473577|118|0.999890x|1.789055|0 / 0|
|3|24.473577–34.542654|118|1.000025x|1.814213|0 / 0|
|4|34.542654–44.621655|118|0.999041x|1.801242|0 / 0|
|5|44.621655–54.693076|118|0.999793x|1.792498|0 / 0|
|6|54.693076–64.752365|118|1.000999x|1.788425|0 / 0|
|7|64.752365–74.832052|118|0.998973x|1.784401|0 / 0|
|8|74.832052–84.893099|118|1.000824x|1.782836|0 / 0|

All complete window endpoints report zero listeners, although the WAV lifetime falls inside W3; endpoints cannot prove absence of intervening activity. Observed clean FLAC encoder generation does not repair failed WAV capture, establish loss-free terminal audio or identify ring-loss cause. Encode wall and task-runtime values include scheduling/preemption effects; no codec, cache or observer cause is assigned.

Observed free stack minima are metrics 1196 / listener 1628 / encoder 5652 bytes; supervisor and capture task watermarks are absent. Nine encoder export receipts report zero observed drop/sticky loss. These observations do not certify future stack margin or final drain.

Copied ELF SHA256 `d88a65965ff456c7500df1ae05d5ac548ef4290b35524215bf3e27175378b991` matches archived eb38 ordinary features=[] retrospective identity and acquisition hashes. Full device attestation remains false. All frozen helper hashes match acquisition records; the current nonarchived stream_format dependency hash is explicit. Original raw/body/header/image/helper/build/result/arrival manifests and selection/restoration receipt hashes are preserved and verified unchanged.

Control receipts prove format selection and restoration separately. Before selection, /api/sonos HTTP 200 reports selected/active MP3, Medium, autoplay=false, Sonos stopped; /api/state HTTP 200 reports recognition=false. FLAC selection POST returns 202; subsequent HTTP 200 readbacks report selected/active FLAC, Medium, autoplay=false, Sonos stopped. After trial, restore POST returns 202; HTTP 200 readbacks report selected/active MP3, Medium, autoplay=false, recognition=false and Sonos stopped. restore-required.json records restore_required=false/restored=true/target_format=mp3 at host monotonic 1133521.3438408. These are retained state receipts, not full-image attestation or ADC fidelity evidence.

Owned ROM/reset and consecutive byte-arrival mapping cover 28792 raw bytes with no tail. Natural 90 s collector completes at 1133503.156139280 after release 1133413.140991793; reader/cleanup errors are empty. Wrapper/acquisition pass, supervision root 0/cleaned=true, source cleanup 0 and empty aplay log are distinct from input child 1/cleanup_valid=true. Parent reports session 10016 reaped 0 and source/UART absent. This offline analysis performs no hardware/process action.

Recommendation: stop the failed ADC-fixture route as planned. Preserve valid acquisition, failed representative input, observed clean FLAC encoder windows and completed MP3 restoration as separate facts. No accepted 8 s input, paired current FLAC trial or host-work completion is claimed.
