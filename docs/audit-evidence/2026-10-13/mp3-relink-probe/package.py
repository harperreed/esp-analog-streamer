#!/usr/bin/env python3
# ABOUTME: Packages committed firmware source using a narrow, explicit allowlist.
# ABOUTME: Writes deterministic source and hash receipts without private inputs or builds.

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import tarfile


FIXED = frozenset({
    ".gitignore", ".github/workflows/check.yml", "Cargo.toml", "Cargo.lock", "LICENSE", "mise.toml",
    "pyproject.toml", "uv.lock",
    "core/Cargo.toml", "mp3/Cargo.toml", "mp3/build.rs", "tools/Cargo.toml",
    "firmware/Cargo.toml", "firmware/Cargo.lock", "firmware/build.rs",
    "firmware/.cargo/config.toml", "firmware/rust-toolchain.toml",
    "firmware/sdkconfig.defaults", "firmware/sdkconfig.diagnostics.defaults",
    "firmware/components_esp32.lock", "firmware/partitions.csv",
    "firmware/cfg.toml.example", "firmware/build_support/mp3_iram.rs",
    "firmware/bindings/diagnostics.h", "firmware/bindings/tcp-retention.h",
    "firmware/assets/sonos-roots.pem",
    "firmware/assets/sonos-roots.PROVENANCE.md",
    "scripts/check", "scripts/check-i2s-iram", "scripts/i2s_iram.py",
    "scripts/test-i2s-iram", "scripts/test_i2s_iram.py",
    "test/sonos-web.test.cjs", "test/fixtures/shazam/match.json",
    "test/fixtures/shazam/match.expected.json", "test/fixtures/shazam/miss.json",
    "docs/audit-evidence/2026-10-13/shine-review.md",
    "docs/audit-evidence/2026-10-13/shine-review.json",
})
PREFIXES = (
    "core/src/", "core/tests/", "mp3/src/", "mp3/tests/",
    "mp3/vendor/shine/", "tools/src/", "firmware/src/", "firmware/examples/",
)
BUILD = b"""# Recipient build engineering probe

This archive records committed first-party source and licenses. Its source
revision and per-file SHA-256 hashes appear in SOURCE-MANIFEST.json. External
dependency source/toolchains are not bundled. This is not distribution clearance.

Install the pinned tools declared in mise.toml and its esp-toolchain task,
and the native prerequisites documented by the original project's Linux CI.
Required external inputs include Rust crates, the esp Rust/LLVM toolchain,
ESP-IDF v5.5.5 with submodules, its compiler/Python tools, and managed components
mdns 1.8.2 and esp_websocket_client 1.8.0. Network access may be required for
fresh setup. Declare any reused caches. Cargo offline mode alone does not
prove native installers stayed offline. Do not use production target caches.

Create firmware/cfg.toml by copying firmware/cfg.toml.example. Keep its public
placeholder values for this engineering proof. Create it before the first
compile, use fresh firmware/target output, and avoid CARGO_TARGET_DIR overrides:
the config macro derives the config location from that target path. Build from
firmware/ using cargo +esp build --release --locked, with mise tools activated
and the installed esp toolchain environment available. Retain complete logs,
effective generated sdkconfig, tool identities and final ELF hashes. Do not
flash this image: the public config is only a compile/link fixture.

For canonical firmware verification, initialize a local Git repository in the
extracted source, commit the public source without bypassing hooks, then run
scripts/check --firmware with the pinned mise and uv tools available. This
check requires Git HEAD for snapshots and root .gitignore for generated outputs.
It performs no flash. Its stages are format, build, I2S placement regression,
I2S placement and warning checks. It is not full host or hardware acceptance.

To prove recipient modification and relinking, retain a baseline target build,
apply and record a controlled reachable Shine C change in the same recipient
tree, and rebuild the entire target firmware. This also tests Cargo's source
change tracking. Verify that changed C code
appears in the final ELF, not only an intermediate archive. Preserve Shine and
bridge instruction/literal IRAM placement. Archive separate source patches,
build receipts and image hashes. Do not flash the modified proof image.

Public-config results do not assert byte identity with an existing private
production image. Preserve LICENSE, Shine COPYING, notices and provenance.
Complete-firmware license compatibility remains a separate release decision.
"""


def git(repo, *args):
    return subprocess.run(
        ["git", "-C", str(repo), *args], check=True, capture_output=True
    ).stdout


def safe_name(name):
    path = PurePosixPath(name)
    if (not name or path.is_absolute() or ".." in path.parts
            or str(path) != name or "\\" in name
            or any(ord(char) < 32 or ord(char) > 126 for char in name)):
        raise ValueError(f"unsafe archive path: {name!r}")


def collect(repo, revision):
    files = {}
    tree = git(repo, "ls-tree", "-rz", "--full-tree", revision)
    for record in tree.split(b"\0"):
        if not record:
            continue
        metadata, raw_name = record.split(b"\t", 1)
        name = raw_name.decode("utf-8", errors="strict")
        if name not in FIXED and not name.startswith(PREFIXES):
            continue
        safe_name(name)
        mode, kind, _ = metadata.decode("ascii").split()
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise ValueError(f"unsupported tracked mode/type: {name} {mode} {kind}")
        if name in files:
            raise ValueError(f"duplicate source path: {name}")
        files[name] = (int(mode[-3:], 8), git(repo, "show", f"{revision}:{name}"))
    missing = FIXED - files.keys()
    if missing:
        raise ValueError(f"missing required tracked inputs: {sorted(missing)}")
    for prefix in PREFIXES:
        if not any(name.startswith(prefix) for name in files):
            raise ValueError(f"missing required source prefix: {prefix}")
    return files


def json_bytes(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__ or
        "Create deterministic committed-source recipient build artifacts.")
    parser.add_argument("output", type=Path, help="new or empty artifact directory")
    parser.add_argument("--revision", default="HEAD", help="source commit or ref (default: HEAD)")
    args = parser.parse_args()
    repo = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
    revision = git(repo, "rev-parse", "--verify", "--end-of-options",
                   f"{args.revision}^{{commit}}").decode().strip()
    files = collect(repo, revision)
    files["RECIPIENT-BUILD.md"] = (0o644, BUILD)
    manifest = {
        "source_revision": revision,
        "scope": "first-party source engineering probe; external dependencies unbundled",
        "fixed_allowlist": sorted(FIXED),
        "source_prefixes": list(PREFIXES),
        "files": [{"path": name, "mode": f"{mode:04o}", "size": len(data),
                   "sha256": hashlib.sha256(data).hexdigest()}
                  for name, (mode, data) in sorted(files.items())],
        "manifest_limit": "manifest excludes its own bytes; archive.sha256 hashes full archive",
    }
    manifest_data = json_bytes(manifest)
    files["SOURCE-MANIFEST.json"] = (0o644, manifest_data)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError(f"output directory must be empty: {output}")
    archive = output / "recipient-source.tar.gz"
    with archive.open("xb") as raw:
        with gzip.GzipFile(fileobj=raw, filename="", mode="wb", mtime=0) as zipped:
            with tarfile.open(fileobj=zipped, mode="w", format=tarfile.USTAR_FORMAT) as tar:
                for name, (mode, data) in sorted(files.items()):
                    info = tarfile.TarInfo(name)
                    info.mode, info.size = mode, len(data)
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ""
                    tar.addfile(info, io.BytesIO(data))
    (output / "source-manifest.json").write_bytes(manifest_data)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (output / "archive.sha256").write_text(f"{digest}  {archive.name}\n")
    print(json.dumps({"archive": str(archive), "sha256": digest,
                      "source_revision": revision, "members": len(files)}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"package failed: {error}") from error
