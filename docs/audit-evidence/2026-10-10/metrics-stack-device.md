# Metrics stack device comparison

Doctor Biz: marking log_capture inline(never) raised measured audio-metrics free-stack minima to 1188 bytes in both bounded repeats, from 420 bytes in baseline soak and 580 in baseline slow testing. The allocation stays 4096 bytes. This is a measured stack improvement; both repeats still fail delivery acceptance, and the earlier telemetry loss remains unresolved.

The parent reports the only firmware change is the no-inline boundary, with target metrics frame 800→208 bytes, log_capture frame 528 bytes and unchanged exporter frame 992 bytes. Frozen baseline/repeat ELF and identical PCG64 source hashes, plus complete evidence manifests, are in the paired JSON. Device attestation remains false.

| Measurement | Soak repeat | Slow repeat |
|---|---:|---:|
| Complete capture reports | 4 | 4 |
| First / last reported frames | 475200 / 2364480 | 477600 / 2700720 |
| Encoder reports / successor intervals | 4 / 3 | 6 / 5 |
| Generation ratio range | 0.99961–1.00037x | 0.99722–1.00126x |
| Export receipts / certified closes | 3 / 2 | 4 / 2 |
| Export drops / sticky loss | 0 / false | 0 / false |
| Metrics / listener / encoder minimum free stack, bytes | 1188 / 1668 / 5308 | 1188 / 1608 / 5300 |

Only LF-terminated UART records count; neither repeat has an unfinished tail. Encoder ratios cover successor report windows only and exclude the initial report window. Generation progress does not establish sample continuity.

The soak repeat again lost both listeners to ring lag after about 16.9 seconds, delivering 197/198 encoded frames (4.728/4.752 seconds of audio), about 28% of real time. Producer departures occurred at 38672640/38862004 us; checked closes at 38859519/39048793 us. The helper retains its premature-EOF failure at 189120 bytes, scenario return code 1 and wrapper acquisition error. The failure prevented its slow stage. Final retained FrameHash prefixes contain 228096/226944 samples (4.752/4.728 seconds) without detected prefix gaps, distinct from any earlier summary snapshot. Decoder labels must not be assumed to match UART peer order.

The separate slow repeat exercised timeout event 38101429 us: elapsed 8087 ms against limit 8016 ms. The slow listener's departure at 38101743 us was stopped-reading; its observed/certified close at 40571683 us reports 2469 ms. The fast listener fell a ring behind at 45773165 us and closed at 45964838 us (191 ms). All have later zero-loss receipts. The helper failed with fast listener ended before reclamation probe, so no third admission, probe decoding, or slot-reuse acceptance occurred. Its scenario return code remains 1; wrapper acquisition_errors is empty. A close proves release under the tested ownership code, but no new admission exercised that released capacity here.

Startup capture faults remain: soak/slow queue overflows 17/15, software-dropped frames 480, two overflow restarts/discontinuities, and 26400 settling-discarded frames. Nine fault/settling counters remain fixed across complete reports in each repeat; raw encoder overruns/resets are zero in the recorded windows. These counters do not clear startup or prove every DMA sample survived.

| Heap field: observed minimum bytes | Soak repeat | Slow repeat |
|---|---:|---:|
| Internal free | 67011 | 67039 |
| Internal largest | 61440 | 61440 |
| PSRAM free | 2770392 | 2770652 |
| PSRAM largest | 2359296 | 2359296 |
| Internal minimum sum | 56119 | 56315 |
| PSRAM minimum sum | 362468 | 363468 |

Largest blocks are snapshots; minimum sums combine per-region lows. They do not authorize additional RAM. Both wrappers retain no cleanup errors. Parent final state and process cleanup checks passed. Repeated MQTT TLS failures remain in UART; no watchdog, panic or allocation-failure line was observed. Neither fact proves stack safety or explains transport behavior.

Recommendation: retain the bounded stack improvement, keep full 21tp and listener acceptance open, and attribute loss with reason counters before claiming the original dropped record is fixed. This slow repeat ends before the baseline's successful third admission and therefore exercises a smaller event sequence. Zero loss here cannot clear baseline sticky dropped=1. Current evidence does not locate transport failure or distinguish full from transient contention in the baseline queue.
