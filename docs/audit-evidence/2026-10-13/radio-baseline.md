# Radio baseline restoration — October 13

Diagnostic restoration, ordinary restore and final preservation comparisons
passed. Independent runtime audit also passed this restoration subcriterion.
This is a throwaway experiment, with no permanent firmware behavior change.

The first 30-second seed capture completed with collector exit 0. It read bitmap
7 before any setter, selected FLASH storage, wrote b/g, then read bitmap 3. The
second 30-second reset capture also completed with exit 0 and read bitmap 3
**before** its setter. Both captured reset reason 1 and complete success phases.
This establishes cross-reset retention rather than only same-process state.

On the observation image's first captured boot, both reads saw bitmap 3: after
EspWifi construction and immediately before the production setter. Production
then reported bitmap 7/PS0 before association and again after association on
channel 11, RSSI -42 dBm, negotiated PHY 4. The 45-second collector completed with exit 0; the state helper verified the
observation image hash and boot identity. Saved settings matched, with stopped
MP3/Medium, idle state, autoplay/recognition off and Sonos STOPPED. Receiver route used interface `wlp6s0`.
See [structured receipts](radio-baseline.json) for exact image hashes and the
raw UART/build/SDK/source/flash/audit hash manifest. Public source archives were
compared byte for byte with the tested archives.

The initial independent review blocked building because default NVS `take()` can
erase after NO_FREE_PAGES or NEW_VERSION_FOUND. The corrected temporary seed and
observation startup use verified `take_with(false)`; rereview approved these
artifacts before building. The intended mutation is only station protocol.
The seed has no credential setter. The observation patch adds reads and the
non-erasing NVS guard; real production still reapplies its existing configured
credentials unchanged. Ordinary production retains its
original NVS acquisition behavior; final saved and compiled configuration comparisons passed.

Parent reports both builds passed and the observation IRAM check passed, checking
11,002 linked symbols. The prior canonical run `YkqTJGio` passed 27 checks on unchanged
retained source; this report claims no new canonical run. Parent reports temporary
production source changes reverted and the example removed. The ordinary image restore
and its 45-second collector completed with exit 0. Final checks verified exact
ordinary image hash/boot identity, unchanged saved and compiled configuration,
stopped MP3/Medium, autoplay/recognition off, idle listeners and remote STOPPED/OK.
Ordinary UART reports 7/PS0 and association on channel 6, RSSI -47 dBm,
PHY 4. The association channel changed from observation channel 11; this was
not a same-channel comparison.

Archived throwaway sources: [seed](radio-seed.rs),
[observation patch](radio-observe.patch). Raw UART, build, flash and state receipts
remain in `target/kata-hardening/oct13-next`; they are intentionally not copied
wholesale because they can contain private network identifiers. First-boot
capture uses parent-verified espflash `--after no-reset`, followed by serial-log's
owned reset, so an unrecorded production boot cannot consume persisted bitmap 3.

Pinned API references: IDF 5.5.5 `components/esp_wifi/include/esp_wifi.h:437–449`
marks protocol persistent; `:699–736` declares protocol set/get and `:1086–1097`
defines FLASH storage. Its `docs/en/api-guides/wifi.rst:448–452,4232–4235`
describes configuration after initialization and reload across reboot.
esp-idf-svc 0.53.0 `src/nvs.rs:268–277,88–103` establishes the guarded NVS path.
Runtime initialization-only protocol calls succeeded in the captured seed.

hv4x remains open: its original current
16-bit FLAC transport controls require correlated sender/AP evidence. No AP
access is available. These receipts neither waive that criterion nor locate a
transport bottleneck. Reconnect readback was already verified October 11/13 and
was not repeated here.

Independent audit found one ROM boot/application entry per captured stage and
no fatal lines. Existing PSRAM/I2C warnings appeared in both seed boots; production
also logged OWE configuration and MQTT connection warnings, with an HTTP receive
warning in observation. These do not establish transport health. Flash logs
warned about unused monitor options and baud rate. Collector/flash exit statuses
remain parent receipts; the audit did not observe those process exits itself.
