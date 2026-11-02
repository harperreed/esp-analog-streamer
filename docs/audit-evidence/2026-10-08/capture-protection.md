# Capture protection

Source base: `8ba0450`, branch `wip/kata-capture-protection`. Implementation:
`493c6c6` (optional FIR/measurements) and `1d751fe` (IRAM protection). Katas: a0mf,
dege and ed4k. Machine-readable measurements are in `capture-protection.json`.
Private UART, configuration and full sample hashes remain under
`target/capture-protection/`; no credentials or speaker identifiers are published.

## Optional recognition work

The disabled profile now skips construction and execution of the recognition FIR
and its feed/ring buffers. Enabled recognition preserves sample output, FIR phase
reset, deferred ring clearing and preallocated steady-state publication. The real
host tests cover allocation counts, uneven chunks, held snapshot locks and gaps.

Two images used identical measurement code with IRAM protection disabled in both.
Only optional recognition processing differed. Each ran two FLAC decoders for
45 seconds against the same quiet analog input, approximately -72 dBFS. Each
decoder received exactly 2,160,000 stereo frames, with no omitted/duplicated
decoded frames, decoder discontinuities or additional capture faults.

| Measurement | Unconditional FIR | Optional FIR, recognition off |
|---|---:|---:|
| Capture runtime, three full middle windows | 16.6307% | 8.4046% |
| Maximum service-delay estimate after admission | 3,923 us | 2,898 us |
| Startup queue overflows | 17 | 15 |
| Startup restart attempts | 2 | 2 |
| Startup settling frames discarded | 26,400 | 26,400 |

Runtime is a share of one core: sum of task-runtime deltas divided by elapsed
window time. It includes charged interrupts and excludes an unfinished task
slice. Service delay subtracts the received chunk duration from the interval
between read completions; it includes capture work and preemption, and is not an
isolated scheduler-latency measurement. These are observed short-run maxima,
not bounds on all future workloads. Startup still lost capture continuity.

Recognition-on and recognition-off production builds both passed. The enabled
image was never flashed, no Shazam API calls ran, and the exact private build
configuration was restored. The control images and final image have separate
SHA-256 identities in the JSON evidence.

## IRAM protection and NVS

Pinned ESP-IDF v5.5.5 enables `CONFIG_I2S_ISR_IRAM_SAFE=y` for classic ESP32.
Generated sdkconfig and a compile-time assertion verify the effective value.
The canonical check now inspects the actual linked callbacks, counters, ISR,
queue, atomic and scheduler helpers. Negative controls move symbols to flash or
external RAM and remove the flag, and must fail.

The source audit covers `components/esp_driver_i2s/Kconfig`,
`i2s_private.h:46`, and `i2s_common.c:239,315,329,413,526,700,879` in that pinned
checkout. The flag adds `ESP_INTR_FLAG_IRAM` and forces controller/channel,
queue, semaphore and descriptor-pointer allocations into INTERNAL|8BIT memory.
DMA descriptors and payloads were already INTERNAL|DMA|8BIT. Registration copies
the callback table; our context is null. Callbacks use inline CAS, IRAM literals
and DRAM counters. Normal ISR queue/scheduler calls are in IRAM. Assertion/panic
paths and arbitrary future call-graph changes are outside this placement proof.

Both FIR controls and the first IRAM-enabled production image have identical
static internal sections: IRAM vectors 1,028 B, IRAM text 101,691 B, DRAM data
18,994 B and DRAM BSS 22,144 B. Enabling the flag adds no static bytes in this
build; internal allocation requirements still matter under memory pressure.
The diagnostic adds its own 8 KiB task stack and NVS work, so its free-heap
readings are not an isolated estimate of the flag's allocation cost.

The separate `nvs-capture` example runs production capture plus 120 changing
1,024-byte writes, commits and exact readbacks in its own namespace/key. It
erases that key and observes another 20 seconds. It refuses recognition-on
builds and never writes Wi-Fi or Sonos settings.

| NVS run | Decoder duration | Actual stereo frames per listener | Verified writes | Added capture faults |
|---|---:|---:|---:|---:|
| One listener | 90 s | 4,320,000 | 120 | 0 |
| Two listeners | 90 s | 4,320,000 each | 120 | 0 |

Both runs included baseline, opening, every write/readback, cleanup and recovery
markers, with no reset. The longest measured set/commit/readback operation was
58,597 us and 50,645 us respectively; this is not the cache-disabled interval.
Counters retain startup faults separately. Exact decoder accounting proves the
delivered FLAC sequence; it cannot independently detect every omitted DMA event.

## Restart investigation and limits

`settling_discarded_frames` counts actual whole chunks discarded by the existing
500 ms policy. `frames` includes those chunks. Restart counters count attempts;
both cause counters can increment for one attempt, and the initial queue flush
is separate. A second restart can interrupt a settling budget, so multiplying
restart count by 24,000 does not reconstruct actual discarded audio.

Pinned `i2s_common.c:1226/1256` resets RX/DMA/FIFO, queue and current-pointer state.
It does not run ES8388 setup. Cold startup also resets/configures the codec and
its analog path. These are distinct operations; their settling needs cannot be
inferred from each other. The current stream discards the first 500 ms, which
prevents its decoders from showing whether earlier samples were usable.

The real `capture-faults` run passed. The configured 20 ms pause spanned 34 ms
between UART markers and added no capture faults. After the 3.51 s suspension,
the old stream reached EOF within 1.922 s of the observed suspend marker. Recovery
added one restart attempt (both supervisor and overflow causes), one
discontinuity and exactly 24,000 settling-discarded frames, then decoded exactly
288,000 frames in six seconds. Permanent suspension triggered a software reboot;
both post-reboot listeners decoded exactly 384,000 frames in eight seconds.
The two-listener admission cap also passed. This tests current recovery behavior,
not Sonos playback resumption or the minimum safe settling time.

ed4k remains open for controlled physical input and pre-discard sample capture
around both cold codec setup and isolated I2S restarts. Keep the existing delay
until those measurements justify a narrower recovery policy. TLS/control setup,
reconnect capture protection, music/noise stress and long soaks remain separate
acceptance gates; NVS success does not establish the cause of TLS disruptions.

## Verification

Both Sol implementations received independent cross-review with no actionable
findings. Parent fresh-eyes review found none. `scripts/check` passed all 17
stages: 287 Rust tests passed, two existing tests ignored; formatting, strict
Clippy, real decoder negative controls, firmware build/warnings and IRAM checks
passed. Both diagnostic examples built without warnings. Raw source, ADC
fidelity, minimum analog settling and long-term reliability are not claimed.

Final production ELF SHA-256:
`3dc70b4a5c49bc5af9889ea602967b4171b5899917306d02139cb507a86f79d7`.
After restoring it, both listeners decoded exactly 2,160,000 frames in 45 seconds
with unchanged post-startup capture fault counters. Sonos remained stopped,
autoplay stayed off, and saved Sonos and private firmware configuration matched
their pre-batch values exactly. No push or merge occurred.

a0mf and dege have implemented, tested fixes but remain open for their explicit
separate TLS/setup/reconnect acceptance. ed4k remains an investigation. The next
capture gate is the setup/reconnect scenario tracked with dp08, followed by the
controlled settling measurement; do not treat these short passes as a release gate.
