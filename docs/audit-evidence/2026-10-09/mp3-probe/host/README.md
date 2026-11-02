# Host feasibility probe reproduction

These are archived throwaway sources. Run them from the original scratch layout, not inside this archive: the FLAC Cargo dependency points four directories up to repository `core`, and Python scripts locate private fixture/hardware inputs relative to `target/mp3-benchmark/host`.

From the repository root, copy this directory's contents to `target/mp3-benchmark/host`. Obtain upstream Shine at `target/mp3-benchmark/source/shine`, checking out commit `ab5e3526b64af1a2eaa43aa6f441a7312e013519`. The upstream checkout is not bundled. Supply the private original fixture at `target/tls-capture/music-180s.wav` (SHA256 `578595f0a1594aada1c3583256649fce05098cd2126e140beabf9029578841aa`); no audio is archived. Device decode also requires the original private `target/mp3-benchmark/hardware/probe-uart.log`, whose hash is in `../../mp3-host.json` relative to this README's directory. Missing private inputs are a reproduction prerequisite, not permission to substitute data.

```sh
cc -O2 -fsigned-char -Wall -I target/mp3-benchmark/source/shine/src/lib target/mp3-benchmark/host/probe.c target/mp3-benchmark/source/shine/src/lib/*.c -lm -o target/mp3-benchmark/host/probe
uv run --no-project --with numpy==2.5.3 python target/mp3-benchmark/host/run.py
uv run --no-project --with numpy==2.5.3 python target/mp3-benchmark/host/joins.py
uv run --no-project --with numpy==2.5.3 python target/mp3-benchmark/host/device-decode.py
CARGO_TARGET_DIR=target/mp3-benchmark/host/flac-target cargo build --release --manifest-path target/mp3-benchmark/host/flac-probe/Cargo.toml
uv run --no-project --with numpy==2.5.3 python target/mp3-benchmark/host/flac-compare.py
```

Recorded tool versions: GCC15.2.0, FFmpeg8.0.1-3ubuntu2, numpy2.5.3. The original standalone Cargo lockfile is not archived; this comparison uses repository core's actual encoder with its current dependencies. Check core/src/flac.rs against the source hash in `mp3-host.json` before claiming reproduction of the recorded measurement. Scripts write audio artifacts only under scratch `target`; do not publish them. No script plays audio or accesses serial hardware.

`mp3-host.json` contains curated results and caveats. Counts come from decoded raw PCM bytes, never timestamps. Host timings cannot establish ESP32 feasibility. Frame-boundary joins require decoder warmup despite zero reservoir references; finite streams need delay/tail handling. The FLAC comparison uses exactly479232samples/channel (9.984s), with its42byte stream header excluded from steady payload savings. MP3 fidelity is lossy and amplitude gain remains unresolved.
