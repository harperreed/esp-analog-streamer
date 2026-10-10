This preserves the throwaway matched MP3/FLAC observer used for the runtime-codec
experiment. It is not a production test script. Only repository-root discovery
and the import of the adjacent archived MP3 TCP helper differ from the private
observer. That TCP helper matches the original helper byte for byte.

Run from the repository root on Linux with Python/uv, FFmpeg, ffprobe, a C compiler
named `cc`, and Linux TCP headers exposing the requested `TCP_INFO` fields. The
compiler derives field offsets from the installed headers. The board and its
input source must already be ready; collect UART independently if needed.

Replace `HOST` with the board's numeric IPv4 address. Manually select the active
codec before running and make `--format` match it. The observer changes no device
settings, firmware, playback, volume, UART, or source audio. The current iteration
stays selected as MP3 between experiments.

```sh
uv run --quiet --no-project python -B \
  docs/audit-evidence/2026-10-09/runtime-codec-probe/staged-probe.py HOST \
  --format mp3 --seconds 90 --join-after 30 \
  --out target/runtime-codec-observation
```

`--out` must name a new directory. The default stream port is 8000. Use `--help`
for the bounded duration and admission options. The output retains raw bytes,
whole prefixes, exact decoder accounting, receive arrivals, Linux TCP telemetry,
connection outcomes, and phase rates. Private fixture/audio files are not archived.

`passed` means two bounded captures without natural EOF/errors and valid decoded
whole prefixes. It does not mean real-time sustained delivery. Compare the actual
`both_connected` interval separately from `planned_postjoin_including_recovery`;
the latter includes catch-up. The observer emits `after_second_end` when the
second receiver ends, but no `after_first_eof` phase. The analyst derived that
additional phase offline from saved EOF timestamps, arrival records, and complete
frame ends. It is not an observer-generated result.

Receive timestamps mark chunk completion; they do not locate each byte's arrival.
Receiver `TCP_INFO` cannot locate a failing network hop or reveal sender-side
retransmission state. Whole-prefix checks exclude final incomplete bytes; an
invalid final FLAC CRC cannot distinguish truncation from corruption. Frame and
decoder continuity do not prove speaker playback or sample-exact analog continuity.

Offline archive checks passed: help, Python compilation, Linux TCP field-layout
compilation, four real saved-codec fixture tests, and one phase-boundary test.
The fixture checks cover MP3 partial tails and large framehash output, plus FLAC
truncation and missing interior frames. No network or hardware test ran for this
archive check.
