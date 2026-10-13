#!/usr/bin/env python3
# ABOUTME: Checks recipient archive identity, member safety, modes and extracted bytes.
# ABOUTME: Records a fresh extraction receipt without building or reading private files.

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile

import package


def main():
    parser = argparse.ArgumentParser(description="Verify a source archive by fresh extraction.")
    parser.add_argument("artifacts", type=Path)
    parser.add_argument("extraction", type=Path, help="new extraction directory")
    args = parser.parse_args()
    archive = args.artifacts / "recipient-source.tar.gz"
    manifest_bytes = (args.artifacts / "source-manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    expected = {entry["path"]: entry for entry in manifest["files"]}
    assert len(expected) == len(manifest["files"]), "duplicate manifest paths"
    assert manifest["fixed_allowlist"] == sorted(package.FIXED), "allowlist differs"
    assert manifest["source_prefixes"] == list(package.PREFIXES), "prefixes differ"
    repo = Path(package.git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
    source_files = package.collect(repo, manifest["source_revision"])
    assert set(expected) == set(source_files) | {"RECIPIENT-BUILD.md"}, "unexpected source set"
    assert "firmware/cfg.toml" not in expected, "private config included"
    forbidden_parts = {".git", "target", ".embuild", "journals", "backups", "media-ignore"}
    assert not any(set(Path(name).parts) & forbidden_parts for name in expected)
    args.extraction.mkdir(parents=True, exist_ok=False)
    with tarfile.open(archive, "r:gz") as tar:
        members = tar.getmembers()
        assert len(members) == len(expected) + 1, "wrong archive member count"
        assert [member.name for member in members] == sorted(set(expected) | {"SOURCE-MANIFEST.json"})
        for member in members:
            package.safe_name(member.name)
            assert member.isfile(), f"non-file member: {member.name}"
            assert (member.uid, member.gid, member.mtime, member.uname, member.gname) == (0, 0, 0, "", "")
            data = tar.extractfile(member).read()
            if member.name == "SOURCE-MANIFEST.json":
                assert data == manifest_bytes, "embedded manifest differs"
                assert member.mode == 0o644
            else:
                entry = expected[member.name]
                assert len(data) == entry["size"] == member.size
                assert hashlib.sha256(data).hexdigest() == entry["sha256"], member.name
                assert member.mode == int(entry["mode"], 8), member.name
                if member.name in source_files:
                    assert (member.mode, data) == source_files[member.name], member.name
                else:
                    assert data == package.BUILD, "generated build instructions differ"
            destination = args.extraction / member.name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            destination.chmod(member.mode)
            assert destination.read_bytes() == data, member.name
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    assert (args.artifacts / "archive.sha256").read_text() == f"{digest}  {archive.name}\n"
    # Confirm traversal/special-name guards reject unsafe names before extraction.
    for name in ("../cfg.toml", "/cfg.toml", "firmware/../cfg.toml", "firmware//src/main.rs", "a\\b", "a\nb"):
        try:
            package.safe_name(name)
        except ValueError:
            pass
        else:
            raise AssertionError(f"unsafe name accepted: {name!r}")
    receipt = {"status": "passed", "archive_sha256": digest,
               "source_revision": manifest["source_revision"], "members": len(members),
               "source_files": len(source_files), "extraction": str(args.extraction.resolve()),
               "checks": ["exact allowlist/member set", "committed bytes and modes",
                          "embedded/sidecar manifest agreement", "fresh extraction hashes",
                          "deterministic metadata", "unsafe path negative controls"]}
    (args.artifacts / "extraction-validation.json").write_bytes(package.json_bytes(receipt))
    print(json.dumps(receipt, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) as error:
        raise SystemExit(f"verification failed: {error}") from error
