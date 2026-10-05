#!/usr/bin/env python3
"""Build a separate, immutable Tunnel archive. Never publish or modify npm bundles."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import subprocess
import tarfile
import tempfile
from pathlib import Path

TARGETS = {
    "linux-x64": "x86_64-unknown-linux-gnu",
    "linux-arm64": "aarch64-unknown-linux-gnu",
    "darwin-x64": "x86_64-apple-darwin",
    "darwin-arm64": "aarch64-apple-darwin",
    "win32-x64": "x86_64-pc-windows-msvc",
    "win32-arm64": "aarch64-pc-windows-msvc",
}
MAX_BINARY_BYTES = 256 * 1024 * 1024
ROOT = Path(__file__).resolve().parents[1]
def binary_bytes(binary: Path) -> bytes:
    size = binary.stat().st_size
    if not binary.is_file() or size <= 0 or size > MAX_BINARY_BYTES:
        raise ValueError("invalid or oversized binary")
    with binary.open("rb") as handle:
        content = handle.read(MAX_BINARY_BYTES + 1)
    if len(content) != size:
        raise ValueError("binary changed during packaging")
    return content


def read_build_info(binary: Path) -> dict:
    # Probe a private copy of the bytes being packaged, not a path another
    # compilation may replace between execution and archive creation.
    content = binary_bytes(binary)
    with tempfile.TemporaryDirectory(prefix="tunnel-build-info-") as tmp, tempfile.TemporaryFile() as output:
        snapshot = Path(tmp) / ("webcodex-tunnel.exe" if os.name == "nt" else "webcodex-tunnel")
        snapshot.write_bytes(content)
        snapshot.chmod(0o700)
        result = subprocess.run([str(snapshot), "--build-info-json"], stdin=subprocess.DEVNULL,
                                stdout=output, stderr=subprocess.DEVNULL, timeout=10, check=False)
        output.seek(0)
        raw = output.read(8193)
    if result.returncode or len(raw) > 8192:
        raise ValueError("binary build-info failed or exceeded its bound")
    try:
        value = json.loads(raw)
    except (ValueError, UnicodeDecodeError) as exc:
        raise ValueError("invalid binary build-info") from exc
    if not isinstance(value, dict):
        raise ValueError("invalid binary build-info object")
    value["_observed_sha256"] = hashlib.sha256(content).hexdigest()
    return value


def package(binary: Path, out: Path, platform: str, info: dict, *, source: str | None, development: bool) -> Path:
    if info.get("schema_version") != 1 or info.get("binary") != "webcodex-tunnel":
        raise ValueError("unsupported Tunnel build-info contract")
    version = info.get("version", "")
    if not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError("invalid Tunnel package version")
    if platform not in TARGETS or info.get("target") != TARGETS[platform]:
        raise ValueError("binary target does not match requested archive platform")
    if not development and (not source or not re.fullmatch(r"[0-9a-f]{40}", source) or info.get("source_commit") != source):
        raise ValueError("packaging requires the exact compiled source commit; development output needs --allow-development-build")
    content = binary_bytes(binary)
    digest = hashlib.sha256(content).hexdigest()
    if info.get("_observed_sha256") != digest:
        raise ValueError("binary changed after its build-info probe")
    binary_name = "webcodex-tunnel.exe" if platform.startswith("win32-") else "webcodex-tunnel"
    suffix = "development" if development else source[:12]
    out.mkdir(parents=True, exist_ok=True)
    archive = out / f"webcodex-tunnel-v{version}-{platform}-{suffix}.tar.gz"
    if archive.exists():
        raise ValueError("archive already exists; existing artifacts are never overwritten")
    manifest = {key: info.get(key) for key in ("schema_version", "binary", "version", "target", "source_commit")}
    manifest.update(development=development, sha256=digest)
    fd, temporary = tempfile.mkstemp(prefix=".tunnel-package-", dir=out)
    os.close(fd)
    try:
        with tarfile.open(temporary, "w:gz") as target:
            for name, data, mode in [
                (binary_name, content, 0o755),
                ("README.md", (ROOT / "crates/webcodex-openai-tunnel/README.md").read_bytes(), 0o644),
                ("tunnel-build.json", (json.dumps(manifest, indent=2) + "\n").encode(), 0o644),
            ]:
                entry = tarfile.TarInfo(name)
                entry.size = len(data)
                entry.mode = mode
                target.addfile(entry, io.BytesIO(data))
        with open(temporary, "rb") as handle:
            os.fsync(handle.fileno())
        # Same-directory hard link publishes a completed file without overwriting
        # a concurrent packager's artifact, on both NTFS and Unix filesystems.
        os.link(temporary, archive)
    finally:
        Path(temporary).unlink(missing_ok=True)
    return archive


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--platform", choices=TARGETS, required=True)
    parser.add_argument("--expected-source-sha")
    parser.add_argument("--allow-development-build", action="store_true")
    args = parser.parse_args()
    try:
        info = read_build_info(args.binary)
        archive = package(args.binary, args.out_dir, args.platform, info,
                          source=args.expected_source_sha, development=args.allow_development_build)
        print(json.dumps({"archive": str(archive), "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                          "development": args.allow_development_build}))
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        parser.exit(1, f"Tunnel packaging failed: {exc}\n")


if __name__ == "__main__":
    main()
