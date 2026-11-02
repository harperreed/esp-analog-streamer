# Retained throwaway qm2a probe

These are experimental sources and receipts, not shipped firmware features.
No private configuration, ordinary ELF or source audio is included. Raw local
acquisition files remain under target/kata-hardening/oct13-quant-target.

To reproduce host work at source c7eab74, copy the contents of implementation/
to a fresh target/kata-hardening/<probe>/ directory. This depth matters to the
relative core/src/flac.rs import. Activate the pinned mise tools and run
`bash target/kata-hardening/<probe>/check-host`. The generator recreates all
synthetic fixtures; FFmpeg verifies samples from an independent arithmetic oracle.
Do not run the archived check-host in place: its relative paths expect that
scratch layout. implementation/README.md documents the full contract.

Copy only quant-cost.rs and quant-cost/contract.rs temporarily to the matching
firmware/examples paths to build the example using the pinned ESP toolchain:
`cargo +esp build --release --example quant-cost --message-format=json-render-diagnostics`
from firmware/. This build needs the established native SDK environment.
The parent removed both temporary files after the acquisition.

Hardware runner sources document this acquisition, not a portable unattended
flash command. run-board.py asserts the saved ordinary ELF hash and depends on
private local preflight receipts, a saved bootloader/partition table, the selected
Sonos identity and physical serial device. Re-establish those prerequisites for
any new acquisition; do not substitute empty records or bypass the assertions.
Its first SIGTERM enters cleanup and restoration; further SIGTERM is ignored
during restoration. SIGKILL, power loss and host failure cannot run cleanup.

For retained-data verification, decompress diagnostic-uart.log.gz into scratch.
Run `uv run --locked python <archive>/measure.py <uart> <archive>/implementation/host-output/ffmpeg-results.json <scratch-result>`.
Run the same measure.py with `--parser-tests <host-results>` for its synthetic
parser controls. Those rows are tests, not device evidence.

measurement.json contains all 180 actual rows, summaries and two actual-log
negative controls. parser-tests.json contains 45 synthetic negative controls.
process-tests.json records actual subprocess cleanup tests, with no UART access.
restoration.json reports the compared private pre/post state without their raw
network/player settings. Source and receipt SHA-256 hashes are in manifest.json.

Canonical checks: all 27 stages in run.v8h8Edp2; all 5 firmware stages after source
removal in run.ldXR4onj. The original and final rebuilt ordinary ELF both hash
0ff2a7ccb677fb8045f6faa1d040b930f8adaaf38913946c1847e1f3eb14faf2.
The initial target compile error and initial host red test remain retained.
The standalone UART warnings about 4 MB PSRAM mapping and deprecated I2C also occur
in the restored ordinary boot. The login shell also reports a pre-existing
missing /opt/homebrew/bin/brew; build/acquisition command statuses still passed.

Kernel red/green logs use gzip to retain their exact blank-line endings without
introducing whitespace errors in Git. Decompress to inspect the original bytes.
