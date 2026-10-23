# ABOUTME: Builds frozen real encoder sources with one private-header-bit change.
# ABOUTME: Retains commands, source hashes, raw frames, decoder output and rejection controls.
"""Throwaway: uv run --quiet --no-project python run.py; output lives beside this file."""
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parent
REPO = next(path for path in ROOT.parents if (path / ".git").is_dir())
REVISION = "bfea1d40adc401205ac67ec76d9ad411b9f78215"
UNITS = ["bitstream", "huffman", "l3bitstream", "l3loop", "l3mdct",
         "l3subband", "layer3", "reservoir", "tables"]
COMMANDS = []


def run(args, *, output=None):
    COMMANDS.append([str(value) for value in args])
    result = subprocess.run(args, capture_output=True, check=False, timeout=60)
    if result.returncode or result.stderr:
        raise RuntimeError(f"command returned {result.returncode}: {args}\n"
                           f"{result.stderr.decode(errors='replace')}")
    if output is not None:
        output.write_bytes(result.stdout)
    return result.stdout


def digest(data):
    return hashlib.sha256(data).hexdigest()


def validate_marker(baseline, modified):
    if len(baseline) != 96 * 960 or len(modified) != len(baseline):
        raise ValueError("unexpected encoded byte count")
    for block in range(96):
        start = block * 960
        a, b = baseline[start:start + 960], modified[start:start + 960]
        if a[2] & 1 or not b[2] & 1:
            raise ValueError(f"private bit expectation failed at frame {block}")
        differences = [index for index, pair in enumerate(zip(a, b)) if pair[0] != pair[1]]
        if differences != [2] or a[2] ^ b[2] != 1:
            raise ValueError(f"non-private-bit output change at frame {block}")


def main():
    archive = run(["git", "-C", str(REPO), "archive", "--format=tar", REVISION, "mp3"])
    (ROOT / "source.tar").write_bytes(archive)
    baseline_root = ROOT / "baseline"
    modified_root = ROOT / "modified"
    if baseline_root.exists() or modified_root.exists():
        raise RuntimeError("snapshot directories already exist; retain them and use a new output directory")
    baseline_root.mkdir()
    with tarfile.open(fileobj=io.BytesIO(archive)) as snapshot:
        snapshot.extractall(baseline_root, filter="data")
    shutil.copytree(baseline_root, modified_root)
    source = modified_root / "mp3/vendor/shine/layer3.c"
    before = source.read_bytes()
    assignment = b"  config->mpeg.ext = 0;"
    if before.count(assignment) != 1:
        raise RuntimeError("expected exactly one ext initializer")
    source.write_bytes(before.replace(assignment, b"  config->mpeg.ext = 1;"))
    source_hashes = {}
    for state, tree in [("baseline", baseline_root), ("modified", modified_root)]:
        source_hashes[state] = {
            str(path.relative_to(tree)): digest(path.read_bytes())
            for path in sorted(tree.rglob("*")) if path.is_file()
        }
    compiler = shutil.which("cc")
    ffmpeg = shutil.which("ffmpeg")
    if compiler is None or ffmpeg is None:
        raise RuntimeError("cc and ffmpeg are required")
    versions = {
        "cc": run([compiler, "--version"]).decode().splitlines()[0],
        "ffmpeg": run([ffmpeg, "-version"]).decode().splitlines()[0],
    }
    streams = {}
    decoded = {}
    for state, tree in [("baseline", baseline_root), ("modified", modified_root)]:
        vendor = tree / "mp3/vendor/shine"
        binary = ROOT / f"{state}.encoder"
        run([compiler, "-std=c99", "-O3", "-fsigned-char", "-Wall", "-Wextra", "-Werror",
             "-I", str(vendor), str(ROOT / "probe.c"), str(tree / "mp3/src/bridge.c"),
             *[str(vendor / f"{unit}.c") for unit in UNITS], "-lm", "-o", str(binary)])
        stream = ROOT / f"{state}.mp3"
        streams[state] = run([str(binary)], output=stream)
        decoded[state] = run([ffmpeg, "-hide_banner", "-loglevel", "error", "-f", "mp3",
                              "-i", str(stream), "-f", "s16le", "-acodec", "pcm_s16le",
                              "pipe:1"], output=ROOT / f"{state}.s16le")
    validate_marker(streams["baseline"], streams["modified"])
    if decoded["baseline"] != decoded["modified"]:
        raise ValueError("private bit changed decoded PCM")
    if len(decoded["baseline"]) != 96 * 1152 * 2 * 2:
        raise ValueError("unexpected decoded sample count")
    if not any(decoded["baseline"]):
        raise ValueError("unexpected silent decoded fixture")
    controls = {}
    for name, a, b in [
        ("unchanged-library", streams["baseline"], streams["baseline"]),
        ("extra-payload-change", streams["baseline"],
         streams["modified"][:20] + bytes([streams["modified"][20] ^ 1]) + streams["modified"][21:]),
    ]:
        try:
            validate_marker(a, b)
        except ValueError as error:
            controls[name] = {"rejected": True, "reason": str(error)}
        else:
            raise ValueError(f"negative control accepted: {name}")
    receipt = {
        "status": "pass", "revision": REVISION, "scope": "throwaway host real C encoder and bridge",
        "target_firmware_execution": False, "archive_sha256": digest(archive),
        "driver_sha256": digest((ROOT / "probe.c").read_bytes()), "versions": versions,
        "source_hashes": source_hashes, "commands": COMMANDS, "frames": 96,
        "changed_bits_per_frame": "byte 2 bit 0 only", "negative_controls": controls,
        "decoded_sample_frames": len(decoded["baseline"]) // 4,
        "decoded_pcm_identical": True,
        "artifacts": {state: {"mp3_sha256": digest(streams[state]),
                               "pcm_sha256": digest(decoded[state])} for state in streams},
    }
    (ROOT / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({key: receipt[key] for key in ["status", "frames", "decoded_sample_frames",
                                                  "decoded_pcm_identical", "negative_controls"]}, indent=2))


if __name__ == "__main__":
    main()
