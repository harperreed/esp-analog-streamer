# Sender accounting overhead — October 10

One matched ON/OFF pair kept delivery near real time through MP3 → FLAC → MP3. Removing sender accounting lowered measured listener CPU per sent frame in every row, while encoder rates and delivery stayed similar. This supports bounded overhead assessment; it does not establish a pure CPU cost or explain historical radio failures.

Each image used one normal RTS boot, the same 420-second 48 kHz stereo 16-bit fixture played once, and direct listeners: 30 seconds solo followed by about 60 seconds together. No Sonos Play ran. Playback stayed stopped, Medium buffering and autoplay false, recognition disabled. Both boots reported protocol `0x07`, b/g/n and power save off. Image and fixture hashes, exact arithmetic inputs, per-listener reports and capture counters are in [sender-overhead.json](sender-overhead.json).

Delivery ratios are decoded audio seconds per host second. Dual columns follow listener 1 / listener 2. Encoder columns give whole anchored window generation ratio and mean encode wall microseconds per coded frame.

| Mode / trial | Solo delivery | Dual delivery | Encoder solo ratio / µs | Encoder dual ratio / µs |
|---|---:|---:|---:|---:|
| ON MP3 1 | 0.996797 | 0.999383 / 0.998983 | 1.000106 / 19280.4 | 0.999954 / 19632.4 |
| OFF MP3 1 | 0.998396 | 0.998429 / 0.998829 | 1.000146 / 19111.8 | 1.000036 / 19632.3 |
| ON FLAC | 0.998397 | 1.000003 / 1.000003 | 1.000372 / 6835.0 | 0.999969 / 7015.5 |
| OFF FLAC | 0.998396 | 1.000580 / 0.999157 | 1.000470 / 6634.8 | 0.999818 / 6993.4 |
| ON MP3 2 | 0.999197 | 0.999711 / 0.999310 | 0.999563 / 19215.4 | 1.000092 / 19641.1 |
| OFF MP3 2 | 0.998396 | 0.998427 / 0.998026 | 0.999834 / 19110.5 | 1.000165 / 19586.9 |

Original listener CPU and frame-write clocks remain enabled in both builds. Values below follow listener 1 / listener 2; frame-write totals and maxima are milliseconds. CPU includes charged interrupts and excludes unfinished scheduling slices. Wall time includes waiting and preemption.

| Mode / trial | CPU µs / sent frame | Frame-write total ms | Frame-write max ms |
|---|---:|---:|---:|
| ON MP3 1 | 667.0 / 609.4 | 29078.339 / 26923.125 | 172.628 / 193.573 |
| OFF MP3 1 | 585.8 / 532.9 | 33695.999 / 28107.150 | 192.312 / 173.432 |
| ON FLAC | 4761.7 / 4747.8 | 7130.653 / 5188.658 | 19.747 / 17.192 |
| OFF FLAC | 4598.6 / 4638.7 | 7257.355 / 5144.171 | 18.015 / 22.738 |
| ON MP3 2 | 653.2 / 606.2 | 29552.418 / 27281.701 | 199.778 / 178.686 |
| OFF MP3 2 | 576.6 / 515.8 | 29503.408 / 26412.881 | 174.200 / 194.080 |

All six staged trials retained their original passing outcomes. All twelve saved whole-prefix sample counts independently match their expected totals; the JSON lists each count. No listener had natural EOF, receiver error or decoder error. OFF FLAC excludes final incomplete tails; whether those tails reflect truncation or corruption remains unresolved. This is prefix validation, not a claim about excluded bytes.

The ON collector itself failed finalization: one UART record contained `encoded_frames=2086esets=0`. Its original completion and pass remain false. Retained UART bytes, arrival offsets, events, three trial results and cleanup receipts allowed separate offline recovery. The malformed producer window and the following window lacking its exact baseline were excluded without guessing a count. The final MP3 dual encoder estimate therefore covers three whole windows rather than five. The OFF collector completed and passed normally. Analysis exceptions now preserve final results, and an actual-line regression covers this defect.

Capture brackets cover every trial, with zero deltas across all nine capture fault counters, no accepted encoder raw-read overruns or resets, and no recorded boundary or telemetry loss. Startup faults remain separate: ON/OFF initially had 18/16 queue overflows, 480 dropped frames each, two overflow restarts each, 26,400 settling-discarded frames each and two discontinuities each. These trials do not clear startup capture behavior.

The control removes the entire sender diagnostics bundle: per-call clocks, counters, readiness storage and diagnostics-only poll-flag inspection. Send/poll operations and deadline semantics remain. OFF send/poll numbers are unavailable, never zero-cost measurements. Linked code layout changes confound runtime attribution even though MP3 instruction IRAM placement remains unchanged. One sequential pair cannot establish causation, long-term stability or Sonos performance. The ON sender reports retain long send calls with no WouldBlock/poll activity; terminal send errors follow planned receiver shutdown and are distinct from natural EOF.

The unchanged production source passed the canonical normal check: 27 checks in `run.WKEIQEGi`. Control build/rebuild and seven host matrix tests passed. The ordinary production ELF was restored successfully, with saved/active MP3, Medium buffering, stopped playback and autoplay/recognition off. UART and source processes were absent afterward; the ordinary rebuild also matches the frozen production ELF exactly. This is operator/flash-tool evidence, not a full-flash device attestation.

The [control patch](sender-overhead-control.patch) records the complete temporary writer change. Production source was restored byte-for-byte before the normal rebuild. The measured comparison satisfies d2cb’s original device-overhead requirement; radio root cause, startup capture faults, Sonos recovery and long soaks remain separate open work.
