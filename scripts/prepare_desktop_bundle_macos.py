#!/usr/bin/env python3
"""Stage exact native WebCodex runtime bytes for one macOS Tauri Desktop bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform as host_platform
import re
import shutil
import stat
import subprocess
import sys
from pathlib import Path


BINARIES = ("webcodex", "webcodex-server", "webcodex-runner")
PLATFORM_ARCH = {
    "darwin-x64": ("x86_64", "x86_64"),
    "darwin-arm64": ("arm64", "arm64"),
}
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
SOURCE_RE = re.compile(r"^[0-9a-fA-F]{40}$")


class StageError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_line(argv: list[str]) -> str:
    try:
        result = subprocess.run(
            argv,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=15,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise StageError(f"could not execute staging probe: {Path(argv[0]).name}") from exc
    lines = result.stdout.splitlines()
    if result.returncode != 0 or not lines:
        raise StageError(f"staging probe failed: {Path(argv[0]).name}")
    return lines[0].strip()


def stage(args: argparse.Namespace) -> dict:
    if sys.platform != "darwin":
        raise StageError("macOS Desktop staging must run on a native macOS host")
    if not VERSION_RE.fullmatch(args.version):
        raise StageError(f"invalid Desktop bundle version: {args.version!r}")
    if not SOURCE_RE.fullmatch(args.source_sha):
        raise StageError("source SHA must be one exact 40-hex Git commit")
    if args.built_at <= 0:
        raise StageError("built_at must be a positive Unix timestamp")
    if args.platform not in PLATFORM_ARCH:
        raise StageError(f"unsupported macOS Desktop platform: {args.platform!r}")
    if args.signing_mode not in {"adhoc", "developer-id"}:
        raise StageError(f"unsupported macOS signing mode: {args.signing_mode!r}")

    expected_host, expected_binary_arch = PLATFORM_ARCH[args.platform]
    actual_host = host_platform.machine()
    if actual_host != expected_host:
        raise StageError(
            f"native host architecture mismatch for {args.platform}: expected={expected_host} actual={actual_host}"
        )

    bin_dir = args.bin_dir.resolve(strict=True)
    output_dir = args.output_dir.absolute()
    if not bin_dir.is_dir():
        raise StageError(f"runtime binary directory is not a directory: {bin_dir}")
    if output_dir.exists() or output_dir.is_symlink():
        raise StageError(f"Desktop bundle output already exists: {output_dir}")

    runtime_dir = output_dir / "resources" / "webcodex-runtime"
    runtime_dir.mkdir(parents=True)
    short_source = args.source_sha[:12].lower()
    resources: dict[str, str] = {}
    files: dict[str, dict[str, object]] = {}
    try:
        for name in BINARIES:
            source = bin_dir / name
            try:
                source_stat = source.lstat()
            except OSError as exc:
                raise StageError(f"missing Desktop runtime binary: {source}") from exc
            if not stat.S_ISREG(source_stat.st_mode) or source.is_symlink():
                raise StageError(f"Desktop runtime binary must be a regular non-symlink file: {source}")
            if source_stat.st_size <= 0:
                raise StageError(f"Desktop runtime binary is empty: {source}")

            actual_version = run_line([str(source), "--version"])
            expected_version = (
                f"{name} {args.version} (commit {short_source}, dirty=false, built_at={args.built_at})"
            )
            if actual_version != expected_version:
                raise StageError(
                    f"unexpected {name} identity: {actual_version!r} (expected {expected_version!r})"
                )
            actual_arch = run_line(["/usr/bin/lipo", "-archs", str(source)])
            if actual_arch != expected_binary_arch:
                raise StageError(
                    f"unexpected Mach-O architecture for {name}: expected={expected_binary_arch} actual={actual_arch}"
                )

            source_digest = sha256_file(source)
            destination = runtime_dir / name
            shutil.copy2(source, destination, follow_symlinks=False)
            destination_digest = sha256_file(destination)
            destination_stat = destination.lstat()
            if (
                not stat.S_ISREG(destination_stat.st_mode)
                or destination.is_symlink()
                or destination_stat.st_size != source_stat.st_size
                or destination_digest != source_digest
            ):
                raise StageError(f"staged Desktop runtime byte verification failed for {name}")
            destination.chmod(destination_stat.st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
            resources[str(destination.resolve())] = f"webcodex-runtime/{name}"
            files[name] = {
                "filename": name,
                "size": destination_stat.st_size,
                "source_sha256": source_digest,
                "staged_unsigned_sha256": destination_digest,
            }

        macos: dict[str, object] = {}
        if args.signing_mode == "adhoc":
            macos["signingIdentity"] = "-"
        overlay = {
            "version": args.version,
            "bundle": {
                "active": True,
                "targets": ["dmg"],
                "resources": resources,
                "macOS": macos,
            },
        }
        overlay_path = output_dir / "tauri.bundle.conf.json"
        overlay_path.write_text(json.dumps(overlay, indent=2) + "\n", encoding="utf-8")

        metadata = {
            "schema_version": 2,
            "platform": args.platform,
            "version": args.version,
            "source_sha": args.source_sha.lower(),
            "built_at": args.built_at,
            "signing_mode": args.signing_mode,
            "resource_dir": "resources/webcodex-runtime",
            "provenance": "same_unsigned_runtime_input_before_platform_signing",
            "files": files,
        }
        metadata_path = output_dir / "desktop-bundle.json"
        metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
        return {
            "config": str(overlay_path),
            "metadata": str(metadata_path),
            "runtime_dir": str(runtime_dir),
        }
    except Exception:
        shutil.rmtree(output_dir, ignore_errors=True)
        raise


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--bin-dir", type=Path, required=True)
    result.add_argument("--version", required=True)
    result.add_argument("--source-sha", required=True)
    result.add_argument("--built-at", type=int, required=True)
    result.add_argument("--platform", choices=tuple(PLATFORM_ARCH), required=True)
    result.add_argument("--signing-mode", choices=("adhoc", "developer-id"), required=True)
    result.add_argument("--output-dir", type=Path, required=True)
    return result


def main() -> int:
    try:
        result = stage(parser().parse_args())
    except (StageError, OSError) as exc:
        print(f"desktop macOS staging failed: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
