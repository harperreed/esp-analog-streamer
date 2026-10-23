# MP3 prototype evidence helpers

Run from the repository root. These helpers preserve the experiment rather than
provide a production interface. No private UART, audio fixture, queue identity or
network address is archived here.

`record-evidence.py --self-test` tests sanitization and failure status handling.
`record-evidence.py` reads existing private `target/mp3-stream/hardware` receipts
and writes `docs/audit-evidence/2026-10-09/mp3-stream.json`; it performs no hardware
access. Use `--root REPOSITORY` when invoking it from another directory. Missing
results stay `not_run` or `incomplete`. Rerun after trials complete.

`acceptance.py --help` and `network-acceptance.py --help` describe their actual
capture/network trial arguments. They require the physical board, driven input,
FFmpeg and parent-owned UART collection. Network injection must be authorized.
The archived network helper loads the adjacent acceptance helper.

`MP3_TEST_HOST=DEVICE_HOST python3 sonos-acceptance.py OUTPUT_DIRECTORY` runs the
original Sonos acceptance helper with an explicit host instead of the private
experiment address. It changes real playback and buffer settings, then attempts
restoration. Run only with explicit permission for the actual receiver.

The private runners used execution names `run-acceptance.py`, `run-network.py`
and per-variant `run-*.py` under `target/mp3-stream`; they are not archived because
they include machine routing and network assumptions. Parent narrative and JSON
retain the trial outcomes and ELF hashes. Numeric records include their private
UART SHA256 and boot-relative milliseconds, not an invented boot identifier.

`python3 slow-acceptance.py DEVICE_HOST OUTPUT_DIRECTORY` checks slow-reader slot
reclamation while a real fast listener drains and decodes. `python3
tcp-stall-probe.py DEVICE_HOST --out OUTPUT_DIRECTORY --seconds 90` records two
continuously drained TCP connections and derives Linux TCP_INFO field offsets
from installed headers with the local C compiler. Both require explicit board
trial authorization. Host addresses are arguments; neither helper embeds one.

The TCP result's capture verdict covers EOF/errors and decoded whole prefixes.
It does not establish real-time delivery. The sanitized summary retains duration,
audio delivered per wall second, receive gaps and EOF beside prefix validity.
TCP telemetry is summarized with numeric bounds and a hash of its private source.

The default FLAC regression's private runner, `run-flac-regression.py`, is not
archived: it assumes the workstation ALSA device, physical input routing,
prepared ELF snapshots and saved board configuration. Its completed machine
receipt and nested canonical soak result are summarized by the extractor as
`flac-regression`, including source/snapshot hashes without private source paths.
A missing receipt does not establish a pass.
