# ABOUTME: Validates effective ESP32 I2S configuration and IRAM/DRAM linked symbols.
# ABOUTME: Checks the pinned capture path without treating symbol placement as hardware proof.

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


def validate(config, table):
    settings = set(config.splitlines())
    for required in ("CONFIG_IDF_TARGET_ESP32=y", "CONFIG_I2S_ISR_IRAM_SAFE=y"):
        if required not in settings:
            raise ValueError(f"generated sdkconfig requires {required}")
    symbols = {}
    for line in table.splitlines():
        match = re.match(r"^([0-9a-f]+)\s+.*?\s(\.[\w.]+)\s+([0-9a-f]+)\s+(?:\.hidden\s+)?(\S+)$", line)
        if match:
            address, section, size, name = match.groups()
            symbols[name] = (section, int(address, 16), int(size, 16))

    def require(name, section, fragment=False):
        found = [(symbol, placement) for symbol, placement in symbols.items()
                 if (name in symbol if fragment else name == symbol)]
        if len(found) != 1:
            raise ValueError(f"expected one linked {name}, found {len(found)}")
        symbol, placement = found[0]
        if not placement[0].startswith(section):
            raise ValueError(f"{symbol} linked in {placement[0]}, requires {section}")
        return placement[2]

    for callback in ("i2s_capture8received", "i2s_capture8overflow"):
        require(callback, ".iram", fragment=True)
    for counter in ("i2s_capture9CALLBACKS", "i2s_capture15QUEUE_OVERFLOWS"):
        require(counter, ".dram", fragment=True)
    # Includes the classic ESP32 ISR and its queue, atomic and scheduler helpers.
    for name in ("i2s_dma_rx_callback", "__atomic_fetch_add_4",
                 "xQueueIsQueueFullFromISR", "xQueueReceiveFromISR",
                 "xQueueGenericSendFromISR", "xPortInIsrContext",
                 "xPortEnterCriticalTimeout", "vPortExitCritical",
                 "esp_cpu_compare_and_set", "vTaskSwitchContext",
                 "_frxt_setup_switch", "_frxt_int_enter", "_frxt_int_exit"):
        require(name, ".iram")
    for name in ("__atomic_s32c1i_fetch_add_4",):
        # The PSRAM workaround adds this helper; other pinned profiles can inline s32c1i.
        if name in symbols:
            require(name, ".iram")
    for name in ("s_atomic_lock", "port_uxCriticalNesting", "port_uxOldInterruptState"):
        if name in symbols:
            require(name, ".dram")
    return symbols


def config_from_cargo(messages, elf):
    """Select the SDK output named by this successful normal firmware build."""
    records = []
    for line in messages.splitlines():
        if line.lstrip().startswith('{'):
            record = json.loads(line)
            if not isinstance(record, dict):
                raise ValueError("Cargo message must be an object")
            records.append(record)
    finished = [record for record in records if record.get('reason') == 'build-finished']
    if len(finished) != 1 or finished[0].get('success') is not True or records[-1] != finished[0]:
        raise ValueError("Cargo receipt requires one final successful build-finished message")
    outputs = [record.get('out_dir') for record in records
               if record.get('reason') == 'build-script-executed'
               and record.get('package_id', '').rsplit('#', 1)[-1].split('@', 1)[0] == 'esp-idf-sys']
    if len(outputs) != 1 or not isinstance(outputs[0], str):
        raise ValueError("Cargo receipt requires exactly one esp-idf-sys output directory")
    artifacts = [record for record in records if record.get('reason') == 'compiler-artifact'
                 and record.get('target', {}).get('name') == 'esp-shazam'
                 and record.get('target', {}).get('kind') == ['bin']]
    if len(artifacts) != 1 or not isinstance(artifacts[0].get('executable'), str):
        raise ValueError("Cargo receipt requires one normal esp-shazam executable")
    if Path(artifacts[0]['executable']).resolve() != elf.resolve():
        raise ValueError("Cargo executable receipt does not match --elf")
    config = Path(outputs[0]) / 'sdkconfig'
    if not config.is_file():
        raise ValueError("Cargo-selected generated sdkconfig is missing")
    return config


def main():
    parser = argparse.ArgumentParser(description="Check generated sdkconfig and linked elf for pinned ESP32 I2S IRAM safety.")
    config_input = parser.add_mutually_exclusive_group()
    config_input.add_argument("--sdkconfig", type=Path, help="effective generated sdkconfig; auto-discovered only if unambiguous")
    config_input.add_argument("--cargo-messages", type=Path, help="successful Cargo JSON build log naming the actual SDK output and normal ELF")
    parser.add_argument("--elf", type=Path, default=Path("firmware/target/xtensa-esp32-espidf/release/esp-shazam"))
    parser.add_argument("--objdump", type=Path, help="pinned Xtensa objdump executable")
    parser.add_argument("--symbol-table", type=Path, help="saved objdump -t output for regression/audit replay")
    args = parser.parse_args()
    try:
        if args.cargo_messages is not None:
            args.sdkconfig = config_from_cargo(args.cargo_messages.read_text(), args.elf)
        if args.sdkconfig is None:
            configs = list(Path("firmware/target/xtensa-esp32-espidf/release/build").glob("esp-idf-sys-*/out/sdkconfig"))
            if len(configs) != 1:
                raise ValueError("pass --sdkconfig: expected exactly one generated configuration")
            args.sdkconfig = configs[0]
        if args.symbol_table:
            table = args.symbol_table.read_text()
        else:
            if args.objdump is None:
                executables = list(Path("firmware/.embuild/espressif/tools/xtensa-esp-elf").glob("*/xtensa-esp-elf/bin/xtensa-esp32-elf-objdump"))
                if len(executables) != 1:
                    raise ValueError("pass --objdump: expected exactly one pinned Xtensa tool")
                args.objdump = executables[0]
            table = subprocess.run([str(args.objdump), "-t", str(args.elf)],
                                   check=True, capture_output=True, text=True).stdout
        symbols = validate(args.sdkconfig.read_text(), table)
        print(f"ok I2S IRAM configuration and {len(symbols)} linked symbols checked")
        print("Placement check covers pinned normal ISR helpers; allocation capabilities and full hardware continuity require separate evidence.")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"FAIL I2S IRAM: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
