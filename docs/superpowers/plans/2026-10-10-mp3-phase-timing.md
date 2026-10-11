# Conditional three-phase Shine timing

Status: the uninstrumented reconstruction gate failed. Both rebuilt contexts
generated approximately 0.9995x real time, with zero encoder overruns in their
90-second observations. The saved originals' rate gap did not reproduce. Keep
this timing design unimplemented; the measurements cannot attribute that gap.
Paired evidence is retained as reconstructed-default-controls.md/.json.
The separate current-image delivery trial failed and cleaned up; evidence is in
reconstructed-listener-failure.md/.json. The wrapper stopped before its
slow-reader phase.

Doctor Biz, the input route is closed: archived FLAC-selected WAV failed after
1.465s of the requested8s despite0.999743x FLAC generation and no encoder loss.
There is no valid ADC host-work pair. This is the next bounded rate diagnosis,
not permission to claim a cache cause or an input-validity certificate.

The design passed independent review after removing partial-window flushing and
specifying the receipt ABI and clock ownership. No timing implementation exists
yet. Source provenance inspection matched both saved ELF hashes and the available
partial source hashes, but found no complete historical source/config/compiler
manifest. First prepare and measure uninstrumented reconstructions of eb38c65 and
bacdecb. Do not substitute current HEAD or implement the timing experiment before
those baselines reproduce the original rate separation.

## Smallest implementation

Instrument shine_encode_buffer_internal in mp3/vendor/shine/layer3.c around its
three consecutive calls: shine_mdct_sub, shine_iteration_loop and
shine_format_bitstream. MDCT includes subband filtering; iteration includes
quantization/Huffman allocation; formatting writes the bitstream. Do not time
every granule/bin or add algorithm counters in this experiment.

Use four adjacent monotonic readings: before MDCT, after MDCT/before iteration,
after iteration/before formatting, after formatting. Differences provide the
three wall durations. Four readings suffice; six separate bracketing readings
are unnecessary. Return durations through one optional diagnostic output pointer
passed down the private shazam_mp3_encode/encode-buffer path in diagnostic builds.
Keep public/default Shine entry points and normal output semantics intact. No
global measurement state and no fields appended to shine_global_config: config,
bitstream, PCM and output allocations remain unchanged. The bounded receipt can
live in the Rust caller's small per-call local storage.

One new diagnostic feature forwarded firmware→mp3 enables a C define through
the existing build.rs feature pattern. Default compilation omits timing calls,
receipt plumbing and export rows entirely. Reject combination with encoder-probe
and metrics-state-crossover. These are proposed changes, not existing flags.

The C timing hook calls the fixed symbol shazam_mp3_phase_now_us using the
ownership and exact ABI below. Keep IDF headers out of the host-capable C library.
The shim must not allocate, log, query task state or unwind. Do not substitute
cycles/performance registers or guessed APIs. Reject non-monotonic/overflowed
durations as invalid evidence without changing audio execution.

Rust aggregates receipts on the encoder owner into one feature-gated aggregate
in the existing Window record. The owner snapshots into encoder_telemetry::EVENTS;
export_encoder_records/render_encoder_event render a separate bounded phase row
from the metrics task while borrowing the record. No owner formatting, even once
per ten seconds. Population and boundary rules below are authoritative.

## Matched pair and provenance

Apply the identical small timing patch to copied git-archive source contexts
under separate target scratch directories for archived/current bases; no worktrees.
Record both base commits, dirty
patch hashes, full source hashes, feature set, actual compiler/toolchain versions,
build command/configuration and final ELF hashes. Preserve existing C opt-level3,
signed-char/longcalls and Rust release settings; verify rather than infer historical
compiler inputs. Saved originals remain untouched and remain the primary baseline.
The new images are diagnostic derivatives, never the exact original images.

Confirm identical instrumented C source and sizes/instruction structure apart
from explainable linkage/literal relocation differences. Record changed text
placement and persistent caller frame sizes. State allocation is preserved, but
the extra call/receipt code still changes registers, stack and layout. Matching
hooks does not cancel those effects automatically.

Use the existing naturally completed90-second no-listener collector/source
scenario, same420-second driven fixture/routing, actual mode MP3/recognitionoff,
fresh boot and unchanged ownership/cleanup/provenance rules. Keep its ordinary
window parser separate from the crossover oracle. Compare full zero-listener
producer windows with actual baselines, export-loss/fault status and capture
counter brackets. Acquisition completion is separate from audio/delivery passing.

## Verification and interpretation

Before device use, real pinned-C host tests must show unchanged deterministic
encoded output, one initialized receipt per attempted MP3 encode, encoder/reset
independence and correct owner-window aggregation. Verify feature-off builds
contain no timing-hook references and original allocation sizes stay unchanged.
Run canonical checks and linked inspection; the parent owns all live operations.
Successful monotonic calls produce valid receipts; invalid-clock receipts remain
invalid without changing encode results.

Measure a bounded empty four-read bracket on the target using the same shim and
report its distribution outside active audio acquisition. This estimates read
overhead, not code-layout overhead. Do not call it negligible until measured;
do not subtract an empty-bracket estimate as though it recovers exact pure CPU
time. Avoid a large general benchmark framework.

First acceptance question: do diagnostic derivatives retain the archived/current
generation-rate and codec-wall/frame separation? If they converge or reverse,
report that confound and stop attribution from this pair. If separation remains,
compare phase totals per measured frame, max values and existing overall codec
wall/frame. Quantify any unassigned wrapper/bridge/output-copy time; three phase
totals do not cover shazam_mp3_encode validation/memcpy or Rust output handling.

A reproducibly larger phase identifies where elapsed time grew, not why. Wall
time includes preemption, interrupts and memory stalls. Similar host work counts
are only representative input evidence. Keep charged-runtime limits, all raw
overruns, failed delivery gates and original acquisition artifacts. No priority,
allocation, cache, gain, bitrate or codec optimization belongs in this experiment.

Only this scratch plan was written; no code, tests, builds or hardware actions ran.

## Exact feature-only ABI and clock ownership

One shared C header declares:

```c
struct shazam_mp3_phase_receipt {
  uint32_t schema;       /* 1 */
  uint32_t valid;        /* 0 or 1, never C bool */
  uint64_t mdct_us;
  uint64_t iteration_us;
  uint64_t formatting_us;
};
int64_t shazam_mp3_phase_now_us(void);
int shazam_mp3_encode_timed(void *, int16_t *, unsigned char *,
                           struct shazam_mp3_phase_receipt *);
```

Rust mirrors the five fields with #[repr(C)] and u32/u64, verifies field offsets,
size32 and actual alignment on host and target. The function returns the same
int status values as current shazam_mp3_encode. A nonnull output receipt is
initialized immediately to schema1/valid0/zero durations before any fallible
work. Four nonnegative, nondecreasing int64 readings make valid1 and populate
durations after formatting, even if later bitstream/frame/header validation
fails. Invalid readings leave valid0/zero durations and do not change encoding
or return status. A null receipt selects ordinary encoding without clock reads;
it is not an encoding error. Audio buffers retain their existing contracts.

Keep shazam_mp3_encode and public shine_encode_buffer/interleaved signatures
unchanged. Under the feature only, add a private-header timed interleaved entry
that sets config buffer pointers identically and passes the receipt to the static
internal function. Existing entries pass null; the internal function shares one
algorithm body and executes hooks only for nonnull. The feature-off internal
signature/body retain no receipt argument or timing branches. Add feature-only
Rust Encoder::encode_timed while retaining Encoder::encode unchanged; firmware's
MP3 adapter uses the timed method only under this feature. No codec state fields,
algorithm changes, extra encoder allocations or parallel implementation body.

The mp3 Rust crate owns the single exported non-unwinding clock symbol for all
consumers. On target_os=espidf it calls existing esp_idf_sys::esp_timer_get_time;
on other targets it uses std::sync::OnceLock<std::time::Instant> plus elapsed
microseconds, returning-1 on conversion overflow. This makes ordinary host
workspace consumers link without a firmware/test-defined symbol. Initialization
is forced once before measured encoding on each platform; host elapsed origin
does not need to match device boot origin. It is real monotonic time, never a
synthetic-only test clock. No duplicate symbol from firmware or integration tests.

## Exact populations, owner boundaries and cost accounting

Carry the receipt and actual encode result in a feature-only owner observation
at the existing Codec observation seam, before EncoderPump handles Err and before
publication. For every MP3 attempt increment attempted; resultOk increments
succeeded, resultErr increments failed. Independently increment invalid if the
receipt.valid is0. Only succeeded+valid calls contribute phase totals/max and
valid_succeeded. Valid failed receipts contribute a separate failed count only,
not successful phase totals. Require attempted=succeeded+failed and
valid_succeeded≤succeeded. Preserve attempted codec wall total and valid-success
codec wall total separately if computing a same-population wrapper remainder;
the existing all-attempt codec total cannot be divided by valid_succeeded after
an error/invalid receipt. Use checked aggregates/sticky invalidation, no wrap.

All these counters are interval counts, unlike cumulative Window.frames/resets.
EncoderMetrics::baseline resets the aggregate alongside existing interval metrics.
Its current report snapshots then baselines even when queue publication fails;
retain that behavior and sticky queue loss rather than carrying lost windows
forward. Switched resets cumulative frames/resets and changes epoch as today.
Startup/Switched invoke the existing baseline rules. Ordinary-feature Overrun
does not baseline: retain phase counts across the actual reset inside that
ten-second window, just as codec wall total persists. Stopped does not baseline.
Retain existing boundary/failure publication order and baseline/report semantics
unchanged. An interrupted terminal/switch tail is excluded from numeric phase
attribution; existing boundary/failure evidence identifies the interruption.
Acquisition/analysis receipts mark that tail unreported and its phase counts
unavailable, never zero. A terminal encode failure remains visible through existing
failure evidence even when its local phase receipt is discarded. No partial
Window, baseline call or additional hot-path export is added. Queue capacity
remains2; loss invalidates affected numeric evidence. Do not add an independent
sequence counter: existing
epoch/window and cumulative reset/boundary events identify this default diagnostic.

Keep the aggregate one fixed bounded struct: counts above, three u64 totals/max,
matched successful codec wall total and validity flags. Measure actual
size_of Window/Payload/Record/Option<Record>/Queue before and after on host and
target; two inline queue slots enlarge with Record. Retain exact byte deltas,
linked encoder/metrics/exporter persistent frame deltas and real free-stack
watermarks for encoder and metrics on **both** derivatives. Never infer acceptable
stack margin from host sizes. Preserve borrowed record rendering and current
stack-sensitive exporter leaf separation; no additional queue/task or hot logger.

The frozen comparison identities are d88a6596 (full hash in original control
receipts), associated with eb38c65, and d78be640, associated with bacdecb. Latest
HEAD525a28e includes later refusal-code changes and is not the current-default
base for this pair. Commit association is retrospective context, not clean-source
attestation or proof that git archive reproduces an ELF. Check saved build source
hashes, dirty patch/config receipts and available frozen source before reconstructing
derivatives. If exact inputs are unavailable, label both reconstruction and all
unknowns explicitly; first reproduce each original baseline's rate behavior
without timing hooks from the reconstructed context. Do not attribute phases
unless this baseline reproduction and the later instrumented-pair separation
both hold. Reproduced rate is behavioral evidence, not full binary attestation.

Copied source contexts must receive the exact same timing patch hash. Inspect their
actual Cargo/config/build-script/SDK/toolchain environment, carry required local
configuration securely without printing secrets, and use isolated build outputs.
Record any base-dependent build difference instead of assuming matching parity.
If archived source seams differ, report and review the smallest adapter patch
before building; never silently claim identical instrumentation. The original
control and all failed input acquisitions remain immutable.
