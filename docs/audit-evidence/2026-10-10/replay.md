# Deterministic streaming replay acceptance

The changes accompanying this receipt extend base 7b05adf. Local validation and
independent production, oracle, scheduler and integration reviews passed.
The first GitHub-hosted execution remains pending; Kata 89g8 stays open for that
gate. No hardware run or firmware deployment occurred.

## Local results

- `scripts/check`: all 25 stages passed, exit 0. This includes ordinary host
  tests, Clippy, capture/listener Loom, harness tests, firmware compilation and
  I2S IRAM checks. Log: `target/kata-hardening/check-fifth-batch-reviewed.log`.
- `scripts/check-stream-replay`: nine Rust tests passed, including three tests
  imported with the real firmware modules. Fourteen listener artifacts passed
  the oracle, and all 12 damaged-audio controls failed for their intended
  diagnostic. Canonical artifacts: `target/stream-replay.uHZRMM`; independent
  pre-canonical repeat: `target/stream-replay.N21tfn`.
- `scripts/test-replay-stream`: 12 tests passed with actual FFmpeg. Fixtures
  exercise schema boundaries, exact PCM, missing/duplicated/damaged frames,
  incomplete writes, and preservation of failed decoder receipts.
- `mise exec actionlint@1.7.12 -- actionlint .github/workflows/stream-replay.yml`
  passed. The workflow installs host dependencies and uv 0.12.19, invokes both
  replay commands on every PR, and retains artifacts. No hosted run is claimed.

## What the gate establishes

The host scheduler calls production capture, codec, encoder/listener pump,
runtime, admission and write-progress operations. Integer-time fixtures check
repeatability, deadlines, close ownership, reset/reconnect, partial writes,
listener overrun and a three-second pause with production capacities. The
partial-timeout reducer preserves a named failure and saves its reduced input.

Capture accounting partitions every generated sample by actual admission,
accepted input, reset, overwrite, retained or staged state. Each fixture declares
each actor's exact source ranges, admission watermark, terminal state and pending
source. A successful decoder cannot choose its own expected audio. Real FLAC
frames are decoded against independently generated source PCM, with exact sample
counts and hashes. Incomplete socket tails never count as completed audio.

Review caught and corrected two false-pass risks in the initial harness: missing
capture ranges could evade accepted-input accounting, and delivered frames could
select their own expected source. Named omission/duplication controls now reject
both. Review also required same-graph reset/reconnect, stale-client events and
retained runtime outcomes at the actual encoder block boundary. These were test
defects; they do not prove corresponding production audio loss.

Production review preserved late successful-read behavior, strict admission
deadlines, whole-frame write deadlines, full-tick sleeps, close-before-release
ownership and existing close-log parser text. The extraction adds no recovery
policy or alternate streaming implementation.

## Remaining limits

Cooperative host steps do not suspend within real blocking locks or codec calls.
This gate does not prove ESP32 task scheduling, DMA continuity, radio/lwIP timing,
analog calibration, MP3 continuity or Sonos ownership-safe recovery. Device
timing overhead from the extraction is unmeasured. The existing P1 MP3 throughput
regression remains a release blocker; no longer soak is justified by these host
results alone. Real sockets, Loom and HIL remain separate gates.
