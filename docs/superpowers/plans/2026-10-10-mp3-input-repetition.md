# MP3 Input Repetition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure whether fresh deterministic full-scale noise exceeds the isolated 24 ms MP3 block budget while repeated-block controls fit.

**Architecture:** Adapt the accepted disposable current-cost example and saved-log parser into a separate experiment directory. One boot runs repeated-A, fresh and repeated-B, with identical per-block generation/write work and the unchanged shared production encoder. Parent exclusively owns build/device acquisition and exact-B restoration; independent plan review precedes implementation.

**Tech Stack:** Existing Rust ESP-IDF example, pinned Shine C archive, Python standard-library saved-log parser/tests, existing bounded device runner.

**Spec:** `target/kata-hardening/current-mp3-next.md`, refined by this plan's equal-generation control.

## Global Constraints

- This is throwaway diagnostic work; no production source, manifest, allocator, bitrate, priority, scheduling or recovery policy changes. No application mock/synthetic mode.
- Core 1, priority 8, 8192-byte stack; 48000 Hz stereo, 1152 frames/2304 samples, 320 kbps, 960 output bytes per successful call.
- Reuse `#[path = "../src/stream_codec.rs"]` and `threads.rs`; allocator callbacks remain verbatim. C archive SHA256 must equal `af005426add1c4a071a20551e6712dc59767971f0acc6262496aadf7834ae148`.
- Read-only restoration inputs remain `target/kata-hardening/current-mp3-cost/production-inputs/restore-B/`; ELF SHA256 `dc9084526287470ce20796a1be14d19c578332224f6f757eb3bda26e9a9902da`. Never overwrite accepted evidence or rebuild restoration from priority-7 source.
- Exactly three conditions, each new encoder, one warmup and 128 measured calls. Regenerate all 2304 samples before EVERY call in EVERY condition, outside codec timing, with identical PRNG steps/buffer writes.
- No Wi-Fi, application startup, capture, ADC acquisition, physical source playback or receiver. No internal phase probes, amplitude matrix or historical rate-gap reproduction.
- One acquisition, existing 120-second capture bound, 2 MiB UART cap and 240-second bound per flash. No automatic reruns.

## Review Focus

- Resetting fresh state between warmup and measured would repeat its first block: independent expected input checks must cover continuation.
- Fresh-only regeneration would change cache/work conditions: shared generator executes for both repeated controls too.
- Failed or incomplete output must never contribute to successful-call means or become a zero-cost result.
- Old-prefix or out-of-order receipts must never satisfy the new three-condition contract.
- Failed acquisition must still restore frozen B and report restoration failure separately.

---

### Task 1: equal-work disposable benchmark and input identity

**Files:** Create `firmware/examples/mp3-input-repetition.rs`; archive under `target/kata-hardening/mp3-input-repetition/source/`. Read the accepted `target/kata-hardening/current-mp3-cost/source/current-mp3-cost.rs`; do not modify it.

**Interfaces:** Keep shared `Encoder::new(OutputFormat::Mp3)`, `push`, `frames_wanted`, `encode_block`. Example-local `fill_noise(pcm: &mut [i32], state: &mut u32)` advances xorshift 13/17/5 once per sample, stores `i32::from(state as i16) * 256`, and requires length 2304. Seed is `0x12345678u32`.

- [ ] Implement the three-condition loop using one generator and timing path. Before each repeated-A/B block, reset state to seed; fresh initializes seed once before warmup and carries state through all 129 calls. Seed-reset choice stays outside the codec bracket. Fresh encoder for each condition; heap PCM/output initialized before spawn; encoder allocated in worker.
- [ ] Regenerate PCM then push, assert zero wanted frames, time only `encode_block(number, output)`, check 960 bytes, and retain accepted per-call accounting/error abort behavior. Explicit `vTaskDelay(1)` between calls. No per-call UART or large stack arrays.
- [ ] Add deterministic input identity outside timing: FNV-1a u32, initial 2166136261 and multiplier 16777619, wrapping arithmetic over each generated signed i16's two little-endian bytes in sample order. Start a separate checksum accumulator for warmup and measured, without resetting fresh PRNG at that boundary. Compute checksum from generated i32 PCM recovered by `>>8`; input remains unchanged by shared push. Record each subphase's final PRNG state and input checksum alongside existing output checksum and wall/runtime/stack fields.
- [ ] Emit prefix `mp3-input-repetition:` and schema `v=1`; exactly three ordered condition names `repeated_a`, `fresh`, `repeated_b`. Each condition retains separate warmup/measured records and one end marker: six subphase records total, not six conditions. Boot/final markers require expected=finished=3. Include acquisition failure markers; no ambiguous silent abort.
- [ ] Archive source hashes and explicitly format/build this example. Keep fixture generation/checksum cost outside codec brackets; whole-batch runtime includes both and must not be called pure codec compute.

### Task 2: independent expected inputs and strict saved-log acceptance

**Files:** Create `target/kata-hardening/mp3-input-repetition/parse_receipt.py` and `test_parse_receipt.py`, adapted from accepted counterparts. Create the new bounded runner in the same directory by adapting the existing current-cost runner, keeping hardware access out of parser/tests.

**Interfaces:** `parse(raw: str) -> dict` validates one complete boot and returns phase records with successful means. CLI retains positional `uart` path. Independent Python reference generates expected checksums/final PRNG states using explicit 32-bit masking after each xorshift operation and the same signed-16 conversion; it never invokes Rust generator or hardware.

- [ ] First write failing saved-log tests for the new prefix, exact condition/subphase order/counts and independently computed input checksums/states. Pin fresh warmup-to-measured continuation, repeated-A/B identical input receipts, and rejection when fresh resets after warmup, or a sample's byte order/value differs. These exercise actual receipt validation rather than a mock encoder.
- [ ] Run `uv run --locked python -m unittest discover -s target/kata-hardening/mp3-input-repetition -p 'test_parse_receipt.py'`; confirm relevant RED before implementation, then adapt parser and confirm GREEN. Use the existing project and lockfile; introduce no Python project/dependency.
- [ ] Retain accepted malformed/incomplete/reset/runtime/count/output-size checks. Add old-prefix, duplicate/reordered/missing condition, wrong seed-state/checksum, failed encode and partial-line rejection. An independently computed expected checksum must match actual target receipts, not merely another generated test fixture.
- [ ] Adapt runner paths/prefix/parser and its hard-coded completion discriminator: expected=4/finished=4 must become expected=3/finished=3. Bind acquisition to the new reviewed example ELF SHA256, determined after its build; never reuse the old example hash. Preserve the fixed B restoration hash above, complete flash/collection/finally restoration and separate cleanup errors. Inspect actual existing CLI before documenting invocation; invent no flags. Add descriptive help and ensure parse-only tests cannot open UART. Retain bounded failure receipts without retrying.

### Task 3: identity checks, independent review and parent acquisition

**Files:** New `target/kata-hardening/mp3-input-repetition/` build/review/device receipts only; optional final tracked report `docs/audit-evidence/2026-10-10/mp3-input-repetition.md` belongs to parent after acquisition.

- [ ] Freeze/copy required accepted restoration files into new experiment ownership without modifying originals; verify exact ELF hash and source/feature provenance. Use current accepted C build identity mechanism; require exact archive hash above and retain actual link-search/compile/build receipts. Shared Rust source equality alone is insufficient.
- [ ] Parent runs canonical `scripts/check` and explicit example release build/format, retaining logs. Independent implementation/build review checks equal generator work, continuation/input hashes, complete-wrapper bracket, heap/stack handling, parser failure tests, archive equality and immutable restoration artifacts before flashing.
- [ ] Parent verifies stopped MP3/Medium, autoplay/recognition off, no UART/source owner. Flash frozen reviewed example with the actual canonical `scripts/flash` ESP32/4 MiB/80 MHz bootloader/partition argument form, without rebuilding during flash. Collect one complete bounded boot. No source player is needed or launched.
- [ ] In finally, restore exact frozen B with its matching bootloader/partitions, even after failure. Verify stopped MP3/Medium, autoplay/recognition off and no remaining UART/source owner. Preserve manual Stop and owned recovery/settings rules. Record absence of image attestation and restoration errors honestly.
- [ ] Archive temporary source/hashes and remove only the temporary example. Do not retain diagnostic source as production source or commit frozen private configuration. Parent may archive the exact example/parser/tests as evidence, as in cc31c48, and owns reporting/commit decisions.

### Task 4: finite interpretation and branch exit

- [ ] Report each condition's measured successful mean/max/>24 ms count, warmup separately, actual counts/bytes, input/output checksums, runtime validity and stack margin. Acquisition validity is independent of fitting the budget.
- [ ] If fresh mean >=24000 us and both repeated means <24000 us, report an observed fresh-sequence budget failure between passing repeated controls in this isolated image, with the deterministic input sequence retained as a reproducer. One fresh phase does not establish measured repeatability or unique temporal-input causation; order/time effects remain possible. Equal generation/write work addresses the fresh-only regeneration confound, but does not locate a codec defect or establish ADC equivalence. Return the retained input for a concrete corrective proposal, without changing codec math, adding runs or launching more measurements automatically.
- [ ] If all measured means <24000 us, retain late calls/maxima and end isolated-input exploration. This rejects only this fresh-noise hypothesis; it does not clear live encoding.
- [ ] If either repeated control fails its mean budget, report inconclusive against the accepted repeated baseline; do not label fresh input causal. Missing/failed records reject acquisition. No automatic reflash, scaling sweep, phase tracing or full-app substitution follows any outcome.
- [ ] Parent selects already-grounded independent Kata work when no corrective proposal follows. Existing 8s9m admission ownership/startup decisions already exist; do not repeat their table. All audio/capture/recovery/listener/hosted/HIL/endurance gates remain open until actual acceptance.

Self-review: equal-work controls, warmup continuity, independent input identity, exact artifact restoration and finite stop rules are assigned above. No build, source implementation or hardware operation has run as part of writing this plan. Parent independently reviews this plan before implementation; the established parent/worker execution method remains in force.

Independent plan review accepted with reporting correction: `target/kata-hardening/mp3-input-repetition-plan-review.md`. This revision incorporates that correction and the runner/locked-environment/evidence-archival clarifications; no implementation has run.
