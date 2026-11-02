# Representative WAV input attempts

Doctor Biz: both natural 90 s acquisitions pass, but **both representative WAV inputs fail**. Each connection ends “fell a ring behind” with a truncated body. Neither retained partial body is an 8 s fixture, an input-equivalence result or a host throughput benchmark. Ordinary MP3 encoder loss remains a separate failure.

| Evidence | Current bacdecb default | Archived eb38 default |
|---|---|---|
| Acquisition / input_fixture_valid | true / false | true / false |
| WAV child / cleanup_valid | exit 1 / true | exit 1 / true |
| Saved body bytes, including 44-byte header | 13004 | 79244 |
| Complete stereo frames / audio duration | 2160 / 0.045 s | 13200 / 0.275 s |
| Required body / frames / duration | 2304044 bytes / 384000 / 8 s | Same |
| Owner peer | 192.168.23.153:60800 | 192.168.23.153:50910 |
| Producer join → departure → close | 30.200379 → 32.363015 → 32.605374 s | 29.843230 → 32.440315 → 32.442036 s |
| Departure reason | Fell a ring behind | Fell a ring behind |
| Close duration from departure | 242359 µs | 1721 µs |
| Capture bracket: preceding end → following begin | 20.411435 → 34.921305 s | 20.396408 → 32.442619 s |
| Nine capture-counter deltas in bracket | All 0 | All 0 |
| Complete ordinary encoder windows | 8 | 8 |
| Weighted MP3 generation | 0.785716x | 0.959784x |
| Actual encoder Overrun boundaries | 8 | 1 |
| Metrics / listener / encoder / supervisor free stack | 1132 / 1672 / 5196 / 3108 bytes | 1196 / 1736 / 5188 / unreported |

Both WAV children report EOFError('truncated WAV body'). Frozen validate_input replay matches each saved failure. Numeric producer order join≤departure≤close holds in both runs. Two complete listener receipts per run report dropped/full/contended/poisoned=0 and sticky=false. A later receipt covers each close event: current close 32605374 µs / export 36839499 µs; archived close 32442036 µs / export 34236967 µs. This certifies the observed close-export evidence, without repairing ring loss or partial body length and without claiming a terminal drain.

Both capture brackets have ordered snapshots and all nine fault deltas 0. Whole-trial sampled startup baselines are queue_overflows 19 current versus 15 archived; both retain software_dropped_frames 480, restarts 2, overflow_restarts 2, settling_discarded_frames 26400 and discontinuities 2. Read errors, supervisor restarts and health restart requests remain 0. These bounded stable samples neither prove coherent state between snapshots nor clear observed listener loss or separate encoder overruns. Baseline differences are not treatment deltas.

Each ordinary encoder report uses actual producer spans and cumulative MP3 frame differences. Live count resets to 0 on format switch, excluding old Flac frames; subsequent complete endpoints anchor deltas. Overrun retains the ordinary reporting baseline. Current ratios range 0.779660–0.787709x, archived 0.953269–0.973088x. Full per-window timing, frame counts, encode-wall-per-frame costs, task runtimes, capture brackets and raw faults remain in companion JSON. The intentional WAV listener is a different workload from prior no-listener observations. No cache, observer or codec cause is established.

Only one of nine frozen helper files differs across these runs: encoder_fault_check.py. Production fix b15c222 corrects numeric peer extraction when the logger prefix contains `::stream: stream:`. Current frozen checked_listener_closes extracts “stream: 192.168.23.153:60800,” so separate validate_uart rejects close matching. Archived fixed parser resolves peer 192.168.23.153:50910 and then correctly rejects the failed ringbehind departure. Prior current verdicts are unchanged. Independent numeric lifecycle and close-covering receipts are reported separately, without bypassing either frozen validator or treating a corrected parser as a successful body capture. All other frozen helper hashes match; JSON preserves both checker hashes and the explicit nonarchived stream_format dependency limit.

Exact copied ELF identities are current `d78be640ac61d83383872e5b11a2e2e72d33bbb22cce6a64f58acf7888ed3d77` and archived `d88a65965ff456c7500df1ae05d5ac548ef4290b35524215bf3e27175378b991`. Their retrospective ordinary features=[] receipts and saved image/build copies match acquisition hashes; full device attestation remains false. Same source fixture, routing and source-on API level −23.6 dBFS do not establish identical PCM at ADC or encoder. Selected state remains active MP3/Medium, autoplay=false, recognition=false and Sonos stopped. Both connect to 192.168.23.217:8000 with owned ROM/reset boot evidence; exact local peers and raw arrival maps remain preserved.

Both collectors complete 90 s naturally with root exit 0, complete raw/arrival coverage and clean supervision; source cleanup returns 0 with empty aplay logs. Input child exits 1 with cleanup_valid=true in each run. Parent reports wrappers reaped with exit 0 and source/UART absent. Offline analysis performs no hardware/process action. Original body/header/raw/arrival/helper/image/build/result hashes verify unchanged; companion JSON links each frozen report and raw manifest.

Recommendation: retain acquisition success separately from failed input fixtures and actual encoder loss. No validated representative pair exists for the downstream host comparison. Do not analyze these partial bodies as accepted 8 s input or use them to assign a cause.

The current UART retains a 154-byte unterminated tail; the archived run has no
partial tail. Raw-byte coverage includes that tail but does not make it a complete
capture report. Complete-window and checked-close evidence excludes it.
