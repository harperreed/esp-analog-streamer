# ABOUTME: Checks IRAM configuration and linked placement using real firmware ELF tool output.
# ABOUTME: Mutates saved metadata to prove missing flags and flash callbacks fail validation.

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CHECKER = ROOT / "scripts/check-i2s-iram"
ELF = ROOT / "firmware/target/xtensa-esp32-espidf/release/esp-shazam"


class IramTests(unittest.TestCase):
    def test_defaults_enable_driver_iram_safety(self):
        self.assertIn("CONFIG_I2S_ISR_IRAM_SAFE=y",
                      (ROOT / "firmware/sdkconfig.defaults").read_text().splitlines())

    def test_help_explains_artifact_inputs(self):
        result = subprocess.run(["bash", str(CHECKER), "--help"],
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("sdkconfig", result.stdout)
        self.assertIn("elf", result.stdout)

    def test_real_elf_rejects_missing_flag_and_flash_callback(self):
        # No tool/API mock: parse the real Xtensa linker's symbol table, then corrupt a copy.
        tools = list((ROOT / "firmware/.embuild/espressif/tools/xtensa-esp-elf").glob(
            "*/xtensa-esp-elf/bin/xtensa-esp32-elf-objdump"))
        if not tools or not ELF.exists():
            self.skipTest("placement mutation needs an existing firmware ELF and pinned objdump")
        symbols = subprocess.run([str(tools[0]), "-t", str(ELF)], check=True,
                                 capture_output=True, text=True).stdout
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "sdkconfig"
            table = Path(directory) / "symbols"
            table.write_text(symbols)
            config.write_text("CONFIG_IDF_TARGET_ESP32=y\nCONFIG_I2S_ISR_IRAM_SAFE=y\n")
            args = ["bash", str(CHECKER), "--sdkconfig", str(config),
                    "--symbol-table", str(table)]
            result = subprocess.run(args, text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            config.write_text("CONFIG_IDF_TARGET_ESP32=y\n# CONFIG_I2S_ISR_IRAM_SAFE is not set\n")
            result = subprocess.run(args, text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("CONFIG_I2S_ISR_IRAM_SAFE", result.stderr)
            config.write_text("CONFIG_IDF_TARGET_ESP32=y\nCONFIG_I2S_ISR_IRAM_SAFE=y\n")
            for name, source_section, destination_section in (
                    ("i2s_capture8received", ".iram0.text", ".flash.text"),
                    ("i2s_capture9CALLBACKS", ".dram0.data", ".ext_ram.bss"),
                    ("__atomic_fetch_add_4", ".iram0.text", ".flash.text")):
                with self.subTest(name=name):
                    table.write_text("\n".join(line.replace(source_section, destination_section)
                                              if name in line else line
                                              for line in symbols.splitlines()))
                    result = subprocess.run(args, text=True, capture_output=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(name, result.stderr)


if __name__ == "__main__":
    unittest.main()
