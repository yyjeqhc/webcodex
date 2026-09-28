#!/usr/bin/env python3
"""Render the repository's static download page from a validated release manifest."""

from __future__ import annotations

import argparse
import json
import re
import shutil
from pathlib import Path

RUNTIME_PLATFORMS = ("linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64", "win32-arm64")
PLATFORMS = RUNTIME_PLATFORMS
INSTALLER_TARGETS = {
    "linux-x64-deb": ("linux-x64", "deb"),
    "linux-x64-rpm": ("linux-x64", "rpm"),
    "linux-arm64-deb": ("linux-arm64", "deb"),
    "linux-arm64-rpm": ("linux-arm64", "rpm"),
    "darwin-x64-pkg": ("darwin-x64", "pkg"),
    "darwin-arm64-pkg": ("darwin-arm64", "pkg"),
    "win32-x64-exe": ("win32-x64", "exe"),
    "win32-arm64-exe": ("win32-arm64", "exe"),
}
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def filename(version: str, target: str) -> str:
    try:
        platform, package_format = INSTALLER_TARGETS[target]
    except KeyError as exc:
        raise ValueError(f"unsupported installer target: {target}") from exc
    return f"webcodex-unified-v{version}-{platform}.{package_format}"


def validate_manifest(value: object) -> dict:
    if not isinstance(value, dict):
        raise ValueError("release manifest must be a JSON object")
    version = value.get("version")
    if not isinstance(version, str) or not VERSION_RE.fullmatch(version):
        raise ValueError("release manifest has an invalid version")
    installers = value.get("installers")
    if not isinstance(installers, dict) or set(installers) != set(INSTALLER_TARGETS):
        raise ValueError("release manifest must contain all eight installer targets")
    source_identity: dict[str, tuple[str, str]] = {}
    for target, (platform, package_format) in INSTALLER_TARGETS.items():
        item = installers[target]
        if not isinstance(item, dict) or set(item) != {
            "platform", "format", "filename", "url", "sha256",
            "source_manifest_url", "source_manifest_sha256",
        }:
            raise ValueError(f"invalid installer entry for {target}")
        name = filename(version, target)
        url = f"https://github.com/yyjeqhc/webcodex/releases/download/v{version}/{name}"
        if (item.get("platform"), item.get("format")) != (platform, package_format):
            raise ValueError(f"non-canonical installer identity for {target}")
        if item.get("filename") != name or item.get("url") != url:
            raise ValueError(f"non-canonical installer filename or URL for {target}")
        if not isinstance(item.get("sha256"), str) or not SHA256_RE.fullmatch(item["sha256"]):
            raise ValueError(f"invalid installer SHA-256 for {target}")
        source_name = f"webcodex-source-v{version}-{platform}.json"
        source_url = f"https://github.com/yyjeqhc/webcodex/releases/download/v{version}/{source_name}"
        source_digest = item.get("source_manifest_sha256")
        if item.get("source_manifest_url") != source_url:
            raise ValueError(f"non-canonical source manifest URL for {target}")
        if not isinstance(source_digest, str) or not SHA256_RE.fullmatch(source_digest):
            raise ValueError(f"invalid source manifest SHA-256 for {target}")
        identity = (source_url, source_digest)
        if source_identity.setdefault(platform, identity) != identity:
            raise ValueError(f"installer targets disagree on source provenance for {platform}")
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
