# Dedicated heap and UART crash diagnostics

Use the isolated `heap-coredump` example with the explicit diagnostic SDK config.
It does not start Wi-Fi or load saved NVS credentials. The normal firmware,
partition layout and SystemView configuration stay unchanged. Parent orchestration
owns image selection, flashing, deliberate failure and normal-image restoration.

Build from the normal repository layout with the pinned mise and `+esp` tools.
Archive the current normal ELF, generated config, bootloader and partition images
before changing build inputs. Never set an external `CARGO_TARGET_DIR` here:
embuild derives SDK/workspace paths from the normal `firmware/target` layout.

The fixture is [heap-coredump.rs](../firmware/examples/heap-coredump.rs); the opt-in
flags live in [sdkconfig.diagnostics.defaults](../firmware/sdkconfig.diagnostics.defaults).
From the repository root, build the normal-config baseline first:

```bash
eval "$(mise env -s bash)"
# Installed espup activation supplies LIBCLANG_PATH for target bindings.
source "$HOME/export-esp.sh"
repo="$PWD"
umask 077
mkdir -p backups/diagnostics
mkdir backups/diagnostics/build-images || exit 1
unset SDKCONFIG ESP_IDF_SDKCONFIG ESP_IDF_SDKCONFIG_DEFAULTS CARGO_TARGET_DIR
no_source="$repo/target/kata-hardening/device-diagnostics/no-source-sdkconfig"
if compgen -G "$no_source*" >/dev/null; then
  echo 'Source sdkconfig and its variants must be absent' >&2
  exit 1
fi
( cd firmware
  CARGO_PROFILE_RELEASE_DEBUG=2 \
    ESP_IDF_SDKCONFIG="$no_source" ESP_IDF_SDKCONFIG_DEFAULTS=sdkconfig.defaults \
    cargo +esp build --release --example heap-coredump --message-format=json-render-diagnostics
) > backups/diagnostics/build-images/baseline-cargo.log 2>&1 || exit 1
```

`ESP_IDF_SDKCONFIG` names a source input appended after defaults, not generated
output. That path and its profile/chip variants must remain absent. Leave
`SDKCONFIG` unset: the pinned build removes generated sdkconfig before CMake,
whereas an environment override can select persisted configuration. The pinned
esp-idf-sys defaults-list delimiter is a semicolon. `CARGO_PROFILE_RELEASE_DEBUG=2`
is a command-local Cargo setting for full debug information; installed Cargo
reference docs confirm that value includes types and variables. Keep it on both
baseline and diagnostic builds to compare the same optimized release profile.
It does not change the release optimization setting. This workstation has
`$HOME/export-esp.sh`; source the installed espup activation file on other hosts
rather than inventing a `LIBCLANG_PATH`.

Select the generated config from this exact completed Cargo JSON receipt.
Normal and full-debug diagnostic output directories can coexist; their number,
file times and directory order do not select the active config. The log remains
beside the archived ELF/config for review.

```bash
generated_out="$(uv run --quiet --project "$repo" --locked python3 - "backups/diagnostics/build-images/baseline-cargo.log" <<'PYCONFIG'
import json
import sys
from pathlib import Path
records = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()
           if line.lstrip().startswith('{')]
assert records and records[-1] == {'reason': 'build-finished', 'success': True}, 'Build must finish successfully'
outputs = [record['out_dir'] for record in records
           if record.get('reason') == 'build-script-executed'
           and record.get('package_id', '').rsplit('#', 1)[-1].split('@', 1)[0] == 'esp-idf-sys']
assert len(outputs) == 1, 'Require one SDK output in this exact build receipt'
assert (Path(outputs[0]) / 'sdkconfig').is_file(), 'Generated config must exist'
print(outputs[0])
PYCONFIG
)" || exit 1
generated_sdkconfig="$generated_out/sdkconfig"
cp firmware/target/xtensa-esp32-espidf/release/examples/heap-coredump \
  backups/diagnostics/build-images/baseline.elf
cp "$generated_sdkconfig" backups/diagnostics/build-images/baseline.sdkconfig
cp "$generated_out/build/config/sdkconfig.h" backups/diagnostics/build-images/baseline.sdkconfig.h
cp "$generated_out/build/config/sdkconfig.json" backups/diagnostics/build-images/baseline.sdkconfig.json
( cd firmware
  CARGO_PROFILE_RELEASE_DEBUG=2 ESP_IDF_SDKCONFIG="$no_source" \
    ESP_IDF_SDKCONFIG_DEFAULTS='sdkconfig.defaults;sdkconfig.diagnostics.defaults' \
    cargo +esp build --release --example heap-coredump --message-format=json-render-diagnostics
) > backups/diagnostics/build-images/diagnostic-cargo.log 2>&1 || exit 1
```

Confirm the diagnostic build script reran and select its own recorded SDK output.
Archive the diagnostic ELF and generated config before another build:

```bash
generated_out="$(uv run --quiet --project "$repo" --locked python3 - "backups/diagnostics/build-images/diagnostic-cargo.log" <<'PYCONFIG'
import json
import sys
from pathlib import Path
records = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()
           if line.lstrip().startswith('{')]
assert records and records[-1] == {'reason': 'build-finished', 'success': True}, 'Build must finish successfully'
outputs = [record['out_dir'] for record in records
           if record.get('reason') == 'build-script-executed'
           and record.get('package_id', '').rsplit('#', 1)[-1].split('@', 1)[0] == 'esp-idf-sys']
assert len(outputs) == 1, 'Require one SDK output in this exact build receipt'
assert (Path(outputs[0]) / 'sdkconfig').is_file(), 'Generated config must exist'
print(outputs[0])
PYCONFIG
)" || exit 1
generated_sdkconfig="$generated_out/sdkconfig"
cp firmware/target/xtensa-esp32-espidf/release/examples/heap-coredump \
  backups/diagnostics/build-images/diagnostic.elf
cp "$generated_sdkconfig" backups/diagnostics/build-images/diagnostic.sdkconfig
cp "$generated_out/build/config/sdkconfig.h" backups/diagnostics/build-images/diagnostic.sdkconfig.h
cp "$generated_out/build/config/sdkconfig.json" backups/diagnostics/build-images/diagnostic.sdkconfig.json
rg -n 'CONFIG_(ESP_COREDUMP_|HEAP_TRACING_|HEAP_TRACE_HASH)' "$generated_sdkconfig"
sha256sum backups/diagnostics/build-images/*.elf backups/diagnostics/build-images/*.sdkconfig firmware/partitions.csv
```

Pass the archived diagnostic ELF and generated sdkconfig to capture. Verify the
actual generated config before flashing: `CONFIG_ESP_COREDUMP_ENABLE_TO_UART=y`,
`CONFIG_ESP_COREDUMP_DATA_FORMAT_ELF=y`, `CONFIG_ESP_COREDUMP_CHECKSUM_CRC32=y`,
`CONFIG_HEAP_TRACING_STANDALONE=y`, `CONFIG_HEAP_TRACING_STACK_DEPTH=2`,
1,792-byte dump stack, no heap trace hash map, and whole-DRAM capture disabled.
The fixture owns exactly 64 internal trace records; confirm that bound in its
source and its successful no-overflow receipt. Confirm the espcoredump component
linked. Require the baseline/default image to select no core dump and no heap
tracing, with no intentional diagnostic abort. Compare the partition CSV and
built partition image hashes against the saved normal inputs; no layout change
belongs in this procedure. Generated config paths contain the active build hash;
a stale path or stale ELF is not evidence of the requested flags.

After the experiment, restore the frozen normal image. For a subsequent normal
production rebuild, clear the diagnostic inputs explicitly:

```bash
unset SDKCONFIG ESP_IDF_SDKCONFIG ESP_IDF_SDKCONFIG_DEFAULTS CARGO_TARGET_DIR CARGO_PROFILE_RELEASE_DEBUG
( cd firmware
  cargo +esp build --release
)
```

Verify the resulting default config has core dumps and heap tracing off, then
use the established `scripts/flash` flow for normal firmware. Preserve stopped
MP3/Medium settings with autoplay and recognition off; flashing an example alone
does not prove normal restoration.

`scripts/core-dump` captures or replays UART and invokes the installed official
Espressif decoder. Every invocation needs explicit paths for the matching ELF,
generated SDK config, partition CSV, SDK checkout, decoder, GDB and correct ROM
ELF. The checked local tools are esp-coredump 1.17.2 and GDB 17.1_20260402. Classic
ESP32 revision 3 uses `esp32_rev300_rom.elf`; verify the chip revision first.

Create `backups/diagnostics` before the first run. Supply a new output directory
inside it; the helper refuses to reuse an existing directory. The following
paths were verified on this workstation; other machines must
supply their existing installed paths. The ROM selection assumes the confirmed
revision-3 board. With the diagnostic images archived as above:

```bash
diagnostic_elf="$repo/backups/diagnostics/build-images/diagnostic.elf"
generated_sdkconfig="$repo/backups/diagnostics/build-images/diagnostic.sdkconfig"
pinned_sdk="$repo/firmware/.embuild/espressif/esp-idf/v5.5.5"
official_decoder="$repo/firmware/.embuild/espressif/python_env/idf5.5_py3.12_env/bin/esp-coredump"
pinned_gdb="$HOME/.espressif/tools/xtensa-esp-elf-gdb/17.1_20260402/xtensa-esp-elf-gdb/bin/xtensa-esp32-elf-gdb"
verified_rom_elf="$repo/firmware/.embuild/espressif/tools/esp-rom-elfs/20241011/esp32_rev300_rom.elf"

```

Before flashing either fixture image, preflight its exact archived ELF with
that same pinned GDB. Set `diagnostic_elf` to the baseline archive for its check,
then to the diagnostic archive for the second check. These offline commands
print only the ELF debug-section inventory and the FreeRTOS type definition;
they do not load a dump or print captured registers/stacks.

```bash
"$repo/firmware/.embuild/espressif/python_env/idf5.5_py3.12_env/bin/python" - "$diagnostic_elf" <<'PYELF'
import sys
from elftools.elf.elffile import ELFFile
with open(sys.argv[1], 'rb') as source:
    elf = ELFFile(source)
    assert elf.has_dwarf_info(strict=True), 'ELF must retain DWARF debug information'
    print([section.name for section in elf.iter_sections() if section.name.startswith('.debug')])
PYELF
"$pinned_gdb" --nx --batch -ex 'ptype TCB_t' "$diagnostic_elf"
```

Require both commands to succeed before reset/capture. The
first retained controlled crash failed decoding because its release ELF had no
DWARF sections and GDB could not resolve `TCB_t`. Stack sampling reached the
GDB task-field query, so the failure did not occur while parsing the ELF.
The installed official decoder waits the full ten-second command timeout when
those type queries return errors; repeated queries exhausted the sixty-second
outer bound. Preserve that failed receipt. Retaining full debug information and
preflighting the actual ELF addresses the cause; increasing the deadline would
leave the missing types. A rebuilt ELF needs its own matching fresh capture.

Capture after both ELF checks pass:

```bash
scripts/core-dump capture --port /dev/ttyUSB0 --reset \
  --elf "$diagnostic_elf" --config "$generated_sdkconfig" \
  --partitions firmware/partitions.csv --sdk "$pinned_sdk" \
  --decoder "$official_decoder" --gdb "$pinned_gdb" --rom "$verified_rom_elf" \
  --output backups/diagnostics/controlled-crash
```

The optional `--reset` pulses RTS for 100 ms using the capture's single owned
serial handle. Without it, capture opens with DTR/RTS false and issues no reset.
Do not run another UART owner. The physical reset and fault behavior still need
real-board verification; pseudo-terminal tests prove byte acquisition only.

Replay uses the same arguments, replacing `capture --port … --reset` with
`decode --capture backups/diagnostics/controlled-crash/uart.bin` and choosing a
new output directory. Replay does not access a device. A different real ELF must
fail official application identity validation; retaining a hash alone does not
prove ELF identity against the dump.

Hard limits are 120 seconds and 2 MiB UART, 1 MiB encoded body, 4 KiB per line,
then 60 seconds and 2 MiB combined output for decoding. Version probes have
separate five-second/16 KiB bounds. Failures keep a safe manifest and bounded
private evidence. The helper terminates its decoder process group, reaps its
child and removes official temporary decode files on failure or interruption.

New directories use mode 0700; files use 0600. Existing permissions never change.
Raw UART, ELF, config, registers, stacks and decoder errors stay beneath ignored
`backups/`; do not copy them into public reports. Console output gives status and
artifact path only. The manifest records full file hashes, SDK revision, decoder
source/distribution metadata hashes, tool versions and exact command. Successful
decoding requires the official checksum/ELF checks and complete report; the tool
never requests whole-memory printing.

Core dumps omit most PSRAM content. Use the fixture's bounded sentinel checksum
and separate internal/PSRAM heap counters as supplemental evidence. Fixed trace
capacity, stack watermark, memory deltas and OFF/ON/OFF timing distinguish
observer overhead from workload behavior. A dump or a surviving task alone is
not a performance pass.

Host verification: `uv run --locked python -m unittest discover -s scripts -p test_core_dump.py`.
Tests use real files,
child processes and pseudo terminals. The optional installed-tool replay test
rejects malformed data through the official decoder; only a retained real crash
can establish successful decoding and matching/wrong-ELF behavior.
