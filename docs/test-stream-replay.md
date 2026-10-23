# Deterministic stream replay

Run `scripts/check-stream-replay` for the complete host gate. It schedules the
actual capture ring, encoder pump, compressed listener, runtime, admission guards
and write-progress operation, then decodes the emitted FLAC with FFmpeg. Fixtures
declare each listener's source ranges, terminal state and retained close state.
Capture accounting assigns every published source sample to accepted input,
explicit admission/reset/overrun loss, or retained input. Negative controls must
fail for their intended reason.

Versioned workloads live in `core/tests/fixtures/stream-replay/`; each records its
seed and ordered virtual-time events. The suite checks repeatability and reduces
the partial-write timeout workload while preserving that named failure. Artifacts
and decoder receipts remain in a fresh `target/stream-replay.*` directory per run.
The canonical `scripts/check` runs this gate and its oracle tests. The separate
`stream-replay.yml` workflow runs both on every PR and retains artifacts; its first
hosted execution remains pending publication of this branch. It pins the same
locally exercised [uv 0.12.19 release](https://github.com/astral-sh/uv/releases/tag/0.12.19).

Virtual scheduling covers absent capture, listener pauses, reset during partial
assembly/copy, backpressure, disconnects, slot reuse and runtime cancellation.
Steps preserve production blocking locks; the scheduler never holds an external
guard across a step. This does not measure ESP32 task scheduling, DMA, radio or
lwIP timing, analog fidelity, MP3 continuity, or Sonos ownership recovery. Real
socket tests, Loom and hardware gates remain necessary.

## Artifact oracle

Run `scripts/test-replay-stream` for ordinary schema checks and real FFmpeg
positive/negative fixtures. These fixtures use FFmpeg as the encoder; they test
the validator, not the Rust production encoder. Replay integration must emit
artifacts from the actual Rust encoder and shared listener pipeline.

Validate one listener with:

```
uv run --locked python scripts/replay_stream.py RUN/listener.json
```

The schema version is 1. All file paths are relative to the JSON directory and
must stay inside it, including after resolving symlinks. Required fields:

| Field | Meaning |
| --- | --- |
| `schema_version` | Integer `1` |
| `transport` | Exact accepted bytes, including HTTP |
| `http_header` | Exact expected response header, ending CRLF CRLF |
| `codec_header` | Exact expected FLAC metadata header, beginning `fLaC` |
| `expected_pcm` | Independent source reference, stereo interleaved little endian s32 |
| `complete_audio_bytes` | Codec header plus payload bytes whose shared listener writes completed |
| `expected_frames` | Exact stereo sample frames in the complete audio prefix |
| `terminal_reason` | Nonempty terminal receipt reason |
| `phase` | `header` for incomplete initial response write, otherwise `payload` |
| `incomplete_tail` | Exact accepted bytes of the incomplete write, possibly empty |
| `pending_write` | Required for header phase or nonempty tail; full pending write |

For header phase, pending_write must equal HTTP plus codec headers, transport
must equal its strict accepted prefix, and complete_audio_bytes/expected_frames
must both be zero. This includes no accepted bytes and a stop inside FLAC
metadata. An incomplete write requires terminal_reason `write_timeout`,
`write_error`, or `peer_closed`. A payload tail must be a strict prefix of the
pending payload. The producer derives complete_audio_bytes from shared listener
successful write events; it never guesses a boundary from decoder diagnostics.

For payload phase, transport must equal HTTP header plus the declared complete
audio prefix plus tail. The prefix must start with the exact codec header. A
zero-frame result permits only that header as the complete prefix. Header-only
and incomplete-header results report `zero_pcm`; they never establish decoded
audio success. The scheduler must separately verify terminal ownership/events.

Produce expected_pcm independently from retained capture source ranges: apply
the production FLAC16 clamp/shift quantization and normalize each signed 16-bit
sample by shifting left 16 into s32. Preserve stereo order. Never derive the
reference from decoded FLAC. This checks digital replay, not analog accuracy.

Real decoding forces `-f flac`, CRC/bitstream/buffer/explode checks, and level
prefixed warnings. The existing decoder log gate rejects errors and frame-number
mismatch warnings. FrameHash checks 48 kHz stereo s32 format and contiguous sample
timestamps. Exact sample counts catch a missing final whole frame; fixed
4096-sample-frame SHA256 comparisons catch content or channel changes regardless
of decoder packet boundaries. No duration tolerance or decoder-error trimming
is allowed.

Inputs are capped at 16 MiB each, manifests at 64 KiB, reference frames at one
million, decoder outputs at 32 MiB, and each decoder at 30 seconds. Output size
is checked while the process runs; failed jobs are killed and reaped. Evidence
is retained in `<manifest-stem>.oracle-<unique>/` beside the manifest, including the
complete audio prefix, raw PCM, framehash, and decoder log. Each invocation gets its own directory; repeated validation preserves prior evidence.
The 32 MiB threshold is sampled every 20 ms, so a running process can briefly exceed
it before termination. This is a failure threshold, not a filesystem quota.
FFmpeg diagnostics detect CRC and numbering faults it reports; the scheduler
must independently assert the first listener frame number and complete write events.
