#!/usr/bin/env python3
"""Emit/verify the advisory Desktop Runtime manifest from packaged build contracts.

The archive passed by release-build is already checksum-verified, source-pinned
and owned by the release workflow. No generation constant is duplicated here:
the packaged binaries' common build-info schema supplies the advertised range.
Older collected bundles may omit this optional asset; that means compatibility
unknown to Desktop, never implicit compatibility.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import re
import subprocess
import tarfile
import tempfile
from pathlib import Path

NAME = "webcodex-release-manifest.json"
MAX_BYTES = 128 * 1024
BINARIES = ("webcodex", "webcodex-server", "webcodex-runner")
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:\+[0-9A-Za-z.-]+)?")

class ManifestError(ValueError):
    pass


def contract(value: object) -> tuple[int, int]:
    if not isinstance(value, dict):
        raise ManifestError("runtime contract is missing")
    lower, upper = value.get("min_generation"), value.get("max_generation")
    if type(lower) is not int or type(upper) is not int or not 0 < lower <= upper <= 65535:
        raise ManifestError("runtime contract range is malformed")
    return lower, upper


def validate(value: object, version: str) -> dict:
    if not isinstance(value, dict) or type(value.get("schema_version")) is not int or value.get("schema_version") != 1:
        raise ManifestError("unsupported release manifest schema")
    if value.get("release_version") != version or not VERSION.fullmatch(version):
        raise ManifestError("release manifest version mismatch")
    runtime = value.get("runtime_version")
    if not isinstance(runtime, str) or not VERSION.fullmatch(runtime):
        raise ManifestError("release manifest Runtime version is invalid")
    lower, upper = contract(value.get("desktop_runtime_contract"))
    return {"schema_version": 1, "release_version": version, "runtime_version": runtime,
            "desktop_runtime_contract": {"min_generation": lower, "max_generation": upper}}


def from_builds(builds: list[dict], version: str) -> dict:
    if len(builds) != len(BINARIES) or not VERSION.fullmatch(version):
        raise ManifestError("expected three official Runtime build identities")
    ranges = []
    for binary, info in zip(BINARIES, builds):
        if not isinstance(info, dict) or type(info.get("schema_version")) is not int or info.get("schema_version") != 1 or info.get("binary") != binary:
            raise ManifestError("packaged build identity is malformed")
        # An official release is a reproducible versioned artifact set. This is
        # release provenance, not a runtime rule requiring version equality.
        if info.get("version") != version:
            raise ManifestError("packaged release version mismatch")
        ranges.append(contract(info.get("desktop_runtime_contract")))
    lower, upper = max(pair[0] for pair in ranges), min(pair[1] for pair in ranges)
    return validate({"schema_version": 1, "release_version": version, "runtime_version": version,
                     "desktop_runtime_contract": {"min_generation": lower, "max_generation": upper}}, version)


def inspect_archive(archive: Path) -> list[dict]:
    """Run only bounded build-info commands from the verified Linux x64 artifact."""
    with tempfile.TemporaryDirectory(prefix="webcodex-release-contract-") as temp:
        root = Path(temp)
        with tarfile.open(archive, "r:gz") as bundle:
            members = bundle.getmembers()
            if len(members) != 3 or {item.name for item in members} != set(BINARIES):
                raise ManifestError("Runtime archive member set mismatch")
            for member in members:
                if not member.isfile() or not 0 < member.size <= 1024 * 1024 * 1024:
                    raise ManifestError("Runtime archive member is not a bounded regular file")
                stream = bundle.extractfile(member)
                if stream is None:
                    raise ManifestError("Runtime archive cannot be read")
                with (root / member.name).open("wb") as output:
                    while chunk := stream.read(65536):
                        output.write(chunk)
                (root / member.name).chmod(0o700)
        results = []
        env = {key: os.environ[key] for key in ("PATH", "LANG", "TMPDIR") if key in os.environ}
        for binary in BINARIES:
            with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
                try:
                    completed = subprocess.run([str(root / binary), "--build-info-json"], env=env,
                                               stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                               check=False, timeout=8)
                except (OSError, subprocess.TimeoutExpired) as exc:
                    raise ManifestError("packaged build-info probe failed") from exc
                if completed.returncode != 0 or stdout.tell() > MAX_BYTES or stderr.tell() > MAX_BYTES:
                    raise ManifestError("packaged build-info probe failed")
                stdout.seek(0)
                try:
                    results.append(json.load(stdout))
                except (UnicodeError, json.JSONDecodeError) as exc:
                    raise ManifestError("packaged build-info is not JSON") from exc
        return results


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--binary-archive", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    manifest = from_builds(inspect_archive(args.binary_archive), args.version)
    destination = args.output_dir / NAME
    if destination.exists():
        raise ManifestError("manifest output already exists")
    encoded = (json.dumps(manifest, indent=2) + "\n").encode("utf-8")
    destination.write_bytes(encoded)
    sums = args.output_dir / "SHA256SUMS"
    previous = sums.read_text(encoding="ascii")
    if any(line.endswith("  " + NAME) for line in previous.splitlines()):
        raise ManifestError("duplicate Runtime manifest checksum")
    sums.write_text(previous.rstrip("\n") + "\n" + hashlib.sha256(encoded).hexdigest() + "  " + NAME + "\n", encoding="ascii")
    print(f"generated {NAME}; generations {manifest['desktop_runtime_contract']}")


if __name__ == "__main__":
    main()
