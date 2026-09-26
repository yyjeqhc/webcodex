#!/usr/bin/env python3
"""Render the repository's static download page from a validated release manifest."""

from __future__ import annotations

import argparse
import json
import re
import shutil
from pathlib import Path

PLATFORMS = ("linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64", "win32-arm64")
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def filename(version: str, platform: str) -> str:
    suffix = ".deb" if platform.startswith("linux-") else ".pkg" if platform.startswith("darwin-") else ".exe"
    return f"webcodex-unified-v{version}-{platform}{suffix}"


def validate_manifest(value: object) -> dict:
    if not isinstance(value, dict):
        raise ValueError("release manifest must be a JSON object")
    version = value.get("version")
    if not isinstance(version, str) or not VERSION_RE.fullmatch(version):
        raise ValueError("release manifest has an invalid version")
    installers = value.get("installers")
    if not isinstance(installers, dict) or set(installers) != set(PLATFORMS):
        raise ValueError("release manifest must contain all six installers")
    for platform in PLATFORMS:
        item = installers[platform]
        if not isinstance(item, dict):
            raise ValueError(f"invalid installer entry for {platform}")
        name = filename(version, platform)
        url = f"https://github.com/yyjeqhc/webcodex/releases/download/v{version}/{name}"
        if item.get("filename") != name or item.get("url") != url:
            raise ValueError(f"non-canonical installer filename or URL for {platform}")
        if not isinstance(item.get("sha256"), str) or not SHA256_RE.fullmatch(item["sha256"]):
            raise ValueError(f"invalid installer SHA-256 for {platform}")
    return value


def build(manifest_path: Path, output_dir: Path, static_dir: Path) -> None:
    manifest = validate_manifest(json.loads(manifest_path.read_text(encoding="utf-8")))
    if output_dir.exists():
        raise ValueError(f"output directory already exists: {output_dir}")
    output_dir.mkdir(parents=True)
    for name in ("index.html", "styles.css", "app.js"):
        source = static_dir / name
        if not source.is_file():
            raise ValueError(f"missing static download page asset: {source}")
        shutil.copyfile(source, output_dir / name)
    shutil.copyfile(manifest_path, output_dir / "manifest.json")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--static-dir", type=Path, default=Path(__file__).resolve().parent.parent / "download")
    args = parser.parse_args()
    try:
        build(args.manifest, args.output_dir, args.static_dir)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc
    print(f"static download page built for {json.loads(args.manifest.read_text(encoding='utf-8'))['version']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
