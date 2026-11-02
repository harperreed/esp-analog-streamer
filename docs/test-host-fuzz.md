# Bounded host fuzz diagnostics

Doctor Biz: `scripts/fuzz-host` builds and exercises the real host encoder, HTTP reader/router, capture and encoded rings, and write progress API. It needs Linux x86_64, Git, mise, a C++ compiler, timeout with `--foreground` and `--kill-after`, tar and sha256sum. Install the scoped tools with:

```sh
mise trust fuzz/mise.toml
(cd fuzz && mise install rust@nightly-2026-02-01 cargo:cargo-fuzz@0.13.2)
scripts/test-fuzz-host
scripts/fuzz-host
```

The standalone package pins libfuzzer-sys 0.4.13 and locks dependencies. Root stable 1.93.0 and firmware `+esp` remain separate. The runner checks the exact nightly compiler and cargo-fuzz versions, requests the x86_64 GNU host and address sanitizer explicitly, and rejects inherited target/compiler/wrapper flags. Root or user Cargo configuration can still affect builds; inspect receipts when reproducing results.

Commit source before acceptance. The runner rejects changes to tracked files and checks that its required files exist in HEAD. It archives HEAD, records its hash and revision, extracts that archive, explicitly trusts its committed mise configuration and builds only from that snapshot. Untracked and ignored files are excluded. Run-specific build output, mutated corpus and artifacts stay under ignored `target/fuzz-host/run.*`. Each directory contains source, tool/host/dependency receipts, commands, seed hashes, build/replay/fuzz logs and final status. The revision is resolved once for checks, metadata and the archive. The runner rejects any symlink in the archive, so archived build inputs cannot follow a link outside the snapshot. A later checkout edit cannot change those source inputs.

## Targets and input layouts

- `flac_frames`: at most 32784 bytes. First 16 bytes control mode (byte 0 bit 0 selects raw; bit 1 selects bounded raw capacity), old and new little-endian u32 numbers (bytes 1–4 and 5–8), raw u16 length (9–10), stereo chunk width (11), raw u16 capacity (12–13), and reserved bytes. Encoded mode repeats remaining bytes into 8192 i32 samples, pushes complete stereo frames and encodes one block. Old number is masked to 31 bits. Raw mode copies remaining bytes into bounded storage. Independent bitwise CRC checks, canonical number/value checks and exact payload comparison validate successful renumbering; errors leave all storage unchanged. Raw success promises structural validity, not decodable audio. Ordinary tests decode a silent block through Claxon.
- `http_head`: at most 4098 bytes. First u16 modulo 4097 selects the head cap; remaining bytes feed the real reader and, if UTF-8, direct routing. Checks cover consumed bounds, repeatability, malformed route/method statuses, independent fixed route/query expectations, incomplete/consumed UTF-8 errors and buffer availability. LF-only lines, leading blank lines, repeated CR and unknown cache queries retain the existing permissive contract. Invalid bytes after the completed head need not reject.
- `stream_ops`: at most 2048 bytes and 128 four-byte operations. Bytes 0–3 choose write length, deadline milliseconds, raw capacity and encoded capacity. Each operation advances virtual time by byte 0 modulo 8; byte 0 modulo 4 chooses publish/reset/read/rejoin. Bytes 1–2 select legal short/zero/interrupted/would-block/fatal send or poll outcomes. Byte 3 chooses encoded push/reset/read/retained admission. Real production APIs enforce stale generations, retained history and overrun behavior. Each supplied send/poll event checks its result kind, exact accepted-byte change and immediate next action before the deadline. Accepted bytes stay bounded and monotonic; terminal errors discard write state. This is sequential API fuzzing, not a full firmware pipeline or concurrency proof.

The committed corpus manifest contains exact bytes, sizes, SHA256 and synthetic origin/license. No audio capture is implied. Seeds include frame-number boundaries, predictor/noise/rail blocks, raw capacity and malformed header cases, HTTP framing/query branches and write/ring schedules.

## Limits and findings

Default mutation budget is 60 seconds per target; `--seconds 600` selects scheduled/manual diagnostics. Every target has one worker, one million iterations, a five-second input timeout, 1024 MiB sampled RSS limit and 64 MiB single-allocation limit. Build, replay and mutation subprocesses have 600, 120 and seconds+30 wall limits; the runner body, starting before archive creation and tool activation, has a 2400-second supervisor. Argument/source/environment/dependency preflight runs before that bound. Stage timeouts use `--foreground` to remain in the supervisor’s dedicated process group; on timeout, interruption or stage failure the supervisor sends TERM to that group and KILL after two seconds, then records a nonzero status. Commands must not detach into another session or group. CI has a 45-minute job limit including installation and always attempts evidence upload. These bounds may stop a run before its nominal mutation budget. RSS sampling is not hard memory containment; no virtual-address limit is imposed on ASan.

Local supervisor tests exercised uutils timeout 0.10.0, including nested stage
cleanup and unrelated-process preservation. GNU timeout on the GitHub runner
still needs hosted execution. An exported Bash function does not inherit its
parent shell's strict options; the runner explicitly sets them in its child shell
so a failed stage cannot continue to a successful final receipt.

Keep finding artifacts and logs. Reproduce the saved input using the same archived source/toolchain and `cargo fuzz run --target x86_64-unknown-linux-gnu --sanitizer address TARGET ARTIFACT`. Bound reproduction with timeout. Consult installed `cargo fuzz tmin --help`, then run minimization under a separate 60-second outer timeout; preserve its logs and smallest output. Add a named ordinary stable regression calling the real API with the minimal bytes or operations. Demonstrate failure before the fix and success after it. Nightly replay supplements these regressions; it does not replace them.

The predecessor's moving-source/mixed-toolchain run is development evidence only. Acceptance requires the final committed archive to build, replay every committed seed and complete all three bounded runs. Earlier sanitizer instrumentation evidence is retained separately; no intentional fault target ships here.
