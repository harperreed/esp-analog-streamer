# Staged MP3 delivery measurement

These experiment helpers measure the same receiver before and after a second
joins. They are not firmware features or production services. Run from a Linux
host with FFmpeg, a C compiler and the already-flashed MP3 board:

```sh
uv run --no-project python docs/audit-evidence/2026-10-09/mp3-staged-probe/staged-tcp-probe.py HOST --out target/mp3-staged
```

The first socket drains immediately. A second connects after 30 seconds;
measurement ends 90 seconds after the first HTTP response. The first drain and
100 ms TCP_INFO sampler continue during the second handshake. Saved whole-frame
prefixes decode after collection, so FFmpeg does not compete with receiving.
Actual connect and HTTP-admission events bound the transition; admission is not
the time the firmware selected its ring cursor. Phase rates use complete coded
frame deltas, not independently decoded or sample-exact input continuity.

The helper loads the adjacent `mp3-stream-probe/tcp-stall-probe.py` and its
acceptance helper. Output is private by default and bounded to 8 MiB of audio
per receiver and 4 MiB of TCP telemetry. A passing capture verdict means no
EOF/errors and valid decoded prefixes; inspect phase delivery rates separately.

For sender correlation, the private trial runner collected UART bytes alongside
JSONL chunk records containing `host_monotonic`, `offset` and `bytes`. UART
records use the host arrival time of their last chunk. A 200 ms serial read
buffer, transmission and scheduling delay make this approximate alignment,
not synchronized device clocks. Keep windows crossing phase boundaries separate.

The runner used the saved experimental and production ELF files, a driven LINE
IN fixture, and `finally` cleanup to stop the source, restore production, and
verify the original stopped Sonos configuration. It is machine-specific and is
not included here. Reproduction requires authorization for hardware/audio use,
verified routing, exclusive UART ownership and a known restore image.

Before the board trial, the staged probe received actual Shine output from a
paced local TCP server. Both normal delivery and an early-EOF negative control
preserved real decode results; the negative control failed its capture verdict.
This validates the observer, not ESP32 or Wi-Fi behavior.

After the runner has written its final restoration receipt, extract the numeric
summary with the archived analyzer:

```sh
uv run --no-project python docs/audit-evidence/2026-10-09/mp3-staged-probe/analyze-staged.py --self-test
uv run --no-project python docs/audit-evidence/2026-10-09/mp3-staged-probe/analyze-staged.py --trial PRIVATE_TRIAL_DIRECTORY --output target/mp3-staged-summary.json
```

The analyzer separates the actual joint interval and the survivor's catch-up
interval. It strips private addresses and paths, retains source hashes, and
reports whether production and configuration restoration were established.
