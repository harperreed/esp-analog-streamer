# ABOUTME: Verifies throwaway FLAC streams with the real FFmpeg decoder and retained PCM.
# ABOUTME: Checks exact sample bytes, stream lengths, checksums and decoder error output.

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import struct


def fnv(data):
    result = 0x811C9DC5
    for byte in data:
        result = ((result ^ byte) * 0x01000193) & 0xFFFFFFFF
    return result


def reference_pcm(source, policy, blocks):
    # Independent scalar implementation: Python floor division replaces Rust shifts.
    states = [0x12345678, 0x9ABCDEF1]

    def byte(channel):
        state = states[channel]
        state ^= (state * 8192) & 0xFFFFFFFF
        state ^= state // 131072
        state ^= (state * 32) & 0xFFFFFFFF
        states[channel] = state
        return state // 16777216

    pcm = bytearray()
    for _ in range(blocks):
        for index, sample in enumerate(source):
            numerator = sample
            if policy == "tpdf":
                numerator += byte(index % 2) - byte(index % 2)
            if policy != "trunc":
                numerator += 128
            quantized = max(-32768, min(32767, numerator // 256))
            pcm.extend(struct.pack("<h", quantized))
    return bytes(pcm), states


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="host driver's output directory")
    args = parser.parse_args()
    results = []
    for line in (args.directory / "receipts.jsonl").read_text().splitlines():
        receipt = json.loads(line)
        stem = f"{receipt['kind']}-{receipt['policy']}-{receipt['phase']}"
        source = args.directory / f"{stem}.flac"
        want = (args.directory / f"{stem}.s16le").read_bytes()
        source_bytes = (args.directory / f"{receipt['kind']}.i32le").read_bytes()
        source_samples = [value[0] for value in struct.iter_unpack("<i", source_bytes)]
        reference, states = reference_pcm(source_samples, receipt["policy"], receipt["blocks"])
        assert reference == want, f"independent quantizer oracle: {stem}"
        assert fnv(source_bytes) == receipt["source_hash"], f"source checksum: {stem}"
        assert states == [receipt["rng_left"], receipt["rng_right"]], f"RNG states: {stem}"
        decoded = subprocess.run(
            ["ffmpeg", "-nostdin", "-v", "error", "-xerror", "-f", "flac", "-i", str(source),
             "-f", "s16le", "-acodec", "pcm_s16le", "pipe:1"],
            check=True, capture_output=True,
        )
        assert not decoded.stderr, (stem, decoded.stderr.decode())
        assert decoded.stdout == want, f"sample mismatch: {stem}"
        assert len(want) == receipt["samples"] * 2, f"sample count: {stem}"
        assert fnv(want) == receipt["pcm_hash"], f"PCM checksum: {stem}"
        results.append({**receipt, "independent_quantizer_exact": True, "sample_exact": True, "decoded_bytes": len(want),
                        "flac_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                        "pcm_sha256": hashlib.sha256(want).hexdigest()})
    assert len(results) == 30
    (args.directory / "ffmpeg-results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"PASS: {len(results)} independent quantizer oracles and real FFmpeg decodes, exact samples and empty error output")


if __name__ == "__main__":
    main()
