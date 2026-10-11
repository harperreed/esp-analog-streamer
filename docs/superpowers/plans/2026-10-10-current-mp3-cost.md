# Current MP3 cost implementation plan

Doctor Biz, this is a **throwaway experimental diagnostic**. Measure today's
shared MP3 wrapper on synthetic input, then restore the exact current unified
priority-8 candidate. No analog source, capture or network load is needed.

**Goal:** determine mean and maximum complete-wrapper wall time versus the
24,000 us represented by each 1,152-frame, 48 kHz stereo MP3 block.

**Architecture:** a temporary firmware example source-includes the existing
codec adapter and thread helper. A bounded runner freezes artifacts, retains
UART and parses complete per-fixture receipts. Parent alone flashes and owns
serial access. This plan authorizes no production implementation changes.

**References:** `target/kata-hardening/mp3-encoder-next.md`;
`docs/audit-evidence/2026-10-09/mp3-probe/device/mp3-cost.rs`;
`firmware/examples/flac-cost.rs`; `firmware/src/stream_codec.rs`.

## Boundaries

- Do not edit production source, manifests, allocator, bitrate, priority policy,
  telemetry or phase probes. Do not relink the historical Shine wrapper ABI.
- Example runs core 1, priority 8, 8 KiB stack, matching active production MP3.
  Initialize only IDF link patches/logging and the benchmark worker; do not call
  application startup, Wi-Fi, I2S, Sonos, MQTT or recognition initialization.
- Use `#[path = "../src/stream_codec.rs"] mod stream_codec;` and the same form
  for `threads.rs`. Existing dependencies satisfy these modules. Handle unused
  items in this example's module scope; do not change their source for linting.
- Use `stream_codec::Encoder::new(OutputFormat::Mp3)`, `push`, `frames_wanted`
  and `encode_block`. This shares production allocator callbacks verbatim:
  `heap_caps_calloc(1, size, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT)` and
  `heap_caps_free`. No production API addition or copied allocator is needed.
- Time only `Encoder::encode_block(number, &mut bytes)`, using
  `esp_timer_get_time` around it. This matches the production codec observation's
  operation boundary: enum dispatch, output resize/conversion, Rust/C calls,
  Shine, header checks and copy. It excludes `push`, PCM conversion, raw reads,
  publication and reset. Wall time includes preemption. Whole-batch task runtime
  includes setup between calls and interrupts charged to the worker; it is not
  pure Shine compute or a phase runtime measurement.
- No historical rate attribution, internal phase instrumentation, ADC acquisition
  or causal claims about instrumentation, code placement or cache behavior.

## Task 1: temporary example and receipts

- [x] Create `firmware/examples/current-mp3-cost.rs`, with ABOUTME lines and the
  shared modules above. Archive a copy under ignored
  `target/kata-hardening/current-mp3-cost/source/` before removing it later.
- [x] Allocate the fixture `Vec<i32>` for 2,304 samples and output `Vec<u8>` with
  capacity 960 on the heap before spawning; move them into the closure. Never
  materialize a 4,608-byte PCM array on the main stack. Shared `Mp3Encoder::new`
  already constructs its private i16 storage on the heap; call it in the worker.
- [x] For each of zero, DC, tones and noise, preserve the old benchmark's exact
  fixture formulas, seed `0x12345678`, amplitudes and channel order. Generate
  the 2,304 i16 values once per fixture, outside timed loops, store each as
  `i32::from(value) * 256`; shared `push` then recovers those exact i16 values.
  Reuse that block as the old isolated benchmark did; do not call this continuous
  noise or phase-continuous tones. Recreate encoder once per fixture, outside
  timing. No 192 kbps or internal-allocation matrix.
- [x] Fill via shared `push`, assert `frames_wanted() == 0`, then time one
  `encode_block` call. One warmup block followed by 128 measured attempts per
  fixture. Consume/checksum output outside each codec bracket. Check 960 bytes
  for every success. Preserve warmup as its own receipt, not in measured mean.
- [x] Explicit `sys::vTaskDelay(1)` between calls and before task-runtime
  baselines; never substitute short Rust sleeps. This is an isolated cost test,
  not a paced real-time producer. Keep per-call UART out of acquisition: retain
  totals/max/counts in memory and print phase receipts after each bounded batch.
- [x] Use existing `vTaskGetInfo(NULL, ..., 0, eRunning)` runtime-counter pattern,
  with wrapping subtraction and explicit counter-validity interval check; record
  worker `uxTaskGetStackHighWaterMark(NULL)` bytes and allocated-buffer address
  classification with `esp_ptr_external_ram`. Do not enable encoder-probe merely
  to obtain storage receipts; shared callback source establishes Shine policy.
- [x] Machine-readable lines have a fixed prefix and schema version. Each phase
  includes fixture, attempted/succeeded/failed counts, input frames represented
  by successes, successful output bytes, codec wall sum/max, calls over 24,000 us,
  failed-call wall sum/max, batch elapsed, task runtime/validity, stack watermark
  and checksum. Distinguish successful-call mean from failed calls; never divide
  by 128 if fewer succeeded. Record a fixture error and abort that fixture on
  encode error rather than reuse unspecified failed state. Remaining fixtures
  can start fresh, but overall success stays false.
- [x] Emit explicit `fixture_end` for each of the four fixtures with actual
  attempts and status, including aborted fixture status; emit final `complete`
  containing exact expected/finished fixture counts and overall status. A panic,
  reset, watchdog, missing receipt/end marker, duplicate phase or partial line is
  an incomplete/failed acquisition, never zero cost or a successful fixture.

## Task 2: build identity and host verification

- [x] Freeze current unified B before building: ELF SHA256 must be
  `dc9084526287470ce20796a1be14d19c578332224f6f757eb3bda26e9a9902da`.
  Preserve its matching bootloader, partitions and source/feature receipt in
  the ignored experiment directory. If identity differs, stop and reconcile;
  do not substitute an old FLAC or an unstated current build.
- [x] Retain current production `libshazam_mp3.a` and SHA256, source hashes for
  `mp3/build.rs`, bridge, vendored C and both included firmware modules, firmware
  lockfile and generated sdkconfig. Locate the actual archive from Cargo's
  `rustc-link-search` output rather than choosing an arbitrary cached build hash.
- [x] Build from firmware with
  `CC_ENABLE_DEBUG_OUTPUT=1 cargo +esp build -vv --release --example current-mp3-cost`,
  redirecting full stdout/stderr to an ignored build log. This mechanism exists
  in lockfile-selected installed **cc 1.6.0**: its `src/lib.rs` documents the env
  variable; `src/command_helpers.rs` emits its `rerun-if-env-changed` directive.
  Changing absent to 1 should rerun the C build; require the log to contain all
  actual Shine/bridge compile commands and their successful exits. No guessed
  CFLAGS, production build-script edit or historical archive is required.
- [x] Retain effective C argv, compiler path/version, archive and constituent
  object hashes, and final example ELF/build options. Confirm the benchmark's
  linked C archive matches the frozen production archive byte-for-byte. If it
  differs, inspect why before running; report a rebuilt current-source archive
  honestly if exact production identity cannot be retained. Do not claim exact
  production artifact cost from source equality alone. Preserve full commands
  and response-file contents if any, not only a selected flag summary.
- [x] Run canonical `scripts/check` and retain its complete receipt; it covers
  existing host MP3 encoder/decode correctness and target production checks.
  Also format and target-build the temporary example explicitly: normal firmware
  fmt/build stages do not check this example. No mock scheduler TDD or test that
  restates a constant. These are existing correctness plus actual target checks
  for a throwaway diagnostic, not a new product feature.
- [x] Review source, timing boundaries, allocation/stack handling and completed
  build receipts before the parent flashes. Canonical production IRAM checks
  concern the production image; do not apply I2S symbol expectations to a
  standalone example that intentionally omits I2S.

## Task 3: parent acquisition and cleanup

- [x] Put runner/parser adaptations and raw evidence under ignored
  `target/kata-hardening/current-mp3-cost/`. Reuse the old `run-device.py` bounded
  capture/error/finally structure, not its hardcoded filenames, old ELF or marker.
  Provide explicit input example and restoration paths, help text, structured
  failure report and bounded deadline. Keep parser separate from hardware access
  so saved logs can be checked without flashing. Exercise parser rejection of
  missing/duplicate end markers and failed encode with small local text fixtures.
- [x] Parent verifies no other UART/source owner and stopped MP3/Medium state;
  no physical audio player is started. Parent flashes the frozen example with
  the canonical 80 MHz/4 MiB/ESP-IDF bootloader/partition settings in
  `scripts/flash`, preserving logs. Avoid rebuilding over a reviewed ELF during
  flash; use its existing exact `espflash` argument form on frozen paths.
- [x] Parent records one bounded boot through complete marker. Retain raw UART,
  parser report, actual ELF hash and explicit lack of device image attestation
  where applicable. Report each fixture mean/max and >24 ms count, wall/runtime
  validity and stack watermark. Missing completion or any failure rejects the
  benchmark; do not repeat indefinitely or silently omit a case.
- [x] In `finally`, parent restores **the frozen current unified B** with its
  matching bootloader/partitions even if collection fails. Verify fresh state is
  stopped MP3/Medium, autoplay/recognition off, with no UART/source owner. Record
  restoration failure separately and leave it visible. Never restore historical
  FLAC firmware. No analog routing needs modification.
- [x] Remove only the temporary example once its source copy/hashes are archived.
  Preserve the existing one-line listener-priority experiment and all unrelated
  work. Produce one result report; commit decisions belong to the parent.

## Interpretation and handoff

If successful mean complete-wrapper wall is >=24 ms on any fixture, today's
isolated path reproduces a frame-budget failure without raw-ring or sockets.
If all cases fit with margin, that rejects this narrow isolated-cost hypothesis
under these inputs; it does not prove current live input or contention fits.
Maxima and late-call counts remain visible regardless of the mean. Whole-task
runtime helps bound attributed load, but cannot isolate codec compute.

No result clears audio, capture, listener, Sonos or Kata acceptance. The test
omits those loads on purpose and synthetic fixtures cannot establish ADC-input
continuity. The next action is review of this plan, then a small throwaway
implementation under the parent's existing authorization; hardware ownership
remains exclusively with the parent.

Design review accepts the bounded plan; receipt:
target/kata-hardening/current-mp3-cost-plan-review.md. Implementation uses a
120-second acquisition deadline, 2 MiB UART cap and 240-second bound per flash.
The parser must require one complete boot and exact fixture/phase/end records.
Acquisition and later formatting stack watermarks remain distinct; detached
thread spawn does not imply a join. Different standalone executable placement
remains a limitation even when the C archive matches production.

Parent froze the current production C archive (SHA256
af005426add1c4a071a20551e6712dc59767971f0acc6262496aadf7834ae148)
using the cached Cargo dependency fingerprint chain and actual link-search
output, alongside exact unified B restoration artifacts. Receipt:
target/kata-hardening/current-mp3-cost/production-inputs/receipt.json.
Implementation, independent reviews, canonical 27 checks, explicit example build,
linked inspection and one bounded hardware acquisition are complete. Public
reviewed evidence: `docs/audit-evidence/2026-10-10/current-mp3-cost.md` and `.json`.
All four isolated cases fit the 24 ms budget; noise mean 18.182 ms, max 18.404 ms.
This does not clear live audio acceptance. The temporary example is removed
from `firmware/examples` after an exact source copy was archived in the evidence
subdirectory. Exact unified B is restored and fresh stopped MP3/Medium settings
and sole-owner cleanup are verified. No production source policy changed.
