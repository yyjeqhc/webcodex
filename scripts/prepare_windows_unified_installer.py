#!/usr/bin/env python3
"""Validate a Windows native collector bundle and prepare Tauri NSIS resources/hooks."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
from pathlib import Path, PurePosixPath

import collect_unified_installer_inputs as collector

BINARIES = collector.BINARIES
RUNTIMES = collector.RUNTIMES
TARGETS = {
    "win32-x64": ("x86_64-pc-windows-msvc", "x86_64", 0x8664),
    "win32-arm64": ("aarch64-pc-windows-msvc", "aarch64", 0xAA64),
}
MANAGED_INSTALL_FILES = collector.WINDOWS_MANAGED_INSTALL_FILES
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
SOURCE_RE = re.compile(r"^[0-9a-f]{40}$")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_path(root: Path, value: object) -> tuple[Path, PurePosixPath]:
    if not isinstance(value, str) or not value or "\\" in value:
        raise ValueError("invalid candidate artifact path")
    relative = PurePosixPath(value)
    if relative.is_absolute() or any(part in ("", ".", "..") for part in relative.parts):
        raise ValueError("candidate artifact path escapes its bundle")
    path = root.joinpath(*relative.parts)
    resolved = path.resolve(strict=True)
    resolved.relative_to(root.resolve(strict=True))
    if path.is_symlink() or not resolved.is_file():
        raise ValueError("candidate component must be a regular file")
    return resolved, relative


def parse_sums(path: Path) -> dict[str, str]:
    entries: dict[str, str] = {}
    for line in path.read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([^\s]+)", line)
        if not match or match.group(2) in entries:
            raise ValueError("candidate SHA256SUMS is malformed or duplicated")
        entries[match.group(2)] = match.group(1)
    return entries


def pe_machine(path: Path) -> int:
    data = path.read_bytes()
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError(f"candidate is not a PE executable: {path.name}")
    offset = int.from_bytes(data[0x3C:0x40], "little")
    if offset < 0 or offset + 6 > len(data) or data[offset:offset + 4] != b"PE\0\0":
        raise ValueError(f"candidate has an invalid PE header: {path.name}")
    return int.from_bytes(data[offset + 4:offset + 6], "little")


def validate_candidate(candidate_dir: Path, platform: str) -> tuple[dict, dict[str, Path]]:
    if platform not in TARGETS:
        raise ValueError("unsupported Windows target")
    root = candidate_dir.resolve(strict=True)
    manifest_path = root / "source-manifest.json"
    raw = manifest_path.read_bytes()
    manifest = json.loads(raw)
    required = {
        "schema_version", "version", "source_sha", "source_workflow_run_id", "source_workflow_ref",
        "platform", "target", "architecture", "desktop_runtime_contract", "artifacts", "desktop_payload",
    }
    target, arch, machine = TARGETS[platform]
    if not isinstance(manifest, dict) or set(manifest) != required or manifest.get("schema_version") != 1:
        raise ValueError("unsupported Windows source manifest schema")
    if (manifest.get("platform"), manifest.get("target"), manifest.get("architecture")) != (platform, target, arch):
        raise ValueError("Windows source manifest platform, target, or architecture mismatch")
    if not isinstance(manifest.get("source_sha"), str) or not SOURCE_RE.fullmatch(manifest["source_sha"]):
        raise ValueError("Windows source manifest source SHA is invalid")
    if type(manifest.get("source_workflow_run_id")) is not int or manifest["source_workflow_run_id"] <= 0:
        raise ValueError("Windows source manifest CI run id is invalid")
    workflow_ref = manifest.get("source_workflow_ref")
    if not isinstance(workflow_ref, str) or ".github/workflows/release-build.yml@refs/tags/" not in workflow_ref:
        raise ValueError("Windows source manifest workflow ref is not the release-build tag job")
    for variable, recorded in (
        ("GITHUB_RUN_ID", str(manifest["source_workflow_run_id"])),
        ("GITHUB_WORKFLOW_REF", workflow_ref),
        ("GITHUB_SHA", manifest["source_sha"]),
    ):
        actual = os.environ.get(variable)
        if actual is not None and actual.lower() != recorded.lower():
            raise ValueError(f"Windows source manifest {variable} does not match GitHub Actions context")
    contract = manifest.get("desktop_runtime_contract")
    if not isinstance(contract, dict) or set(contract) != {"min_generation", "max_generation"}:
        raise ValueError("Windows Desktop Runtime contract is malformed")
    sums = parse_sums(root / "SHA256SUMS")
    if sums.get("source-manifest.json") != hashlib.sha256(raw).hexdigest():
        raise ValueError("Windows source manifest does not match SHA256SUMS")
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, dict) or set(artifacts) != set(BINARIES):
        raise ValueError("Windows source manifest must bind exactly four components")
    resolved: dict[str, Path] = {}
    expected_sums = {"source-manifest.json"}
    expected_architecture = arch
    for name in BINARIES:
        item = artifacts[name]
        if not isinstance(item, dict) or set(item) != {"path", "sha256", "build_info", "build_info_sha256", "probe"}:
            raise ValueError(f"malformed Windows artifact record for {name}")
        expected_path = f"artifacts/bin/{name}.exe"
        if item["path"] != expected_path:
            raise ValueError(f"Windows component path is not canonical for {name}")
        path, relative = safe_path(root, item["path"])
        if not isinstance(item["sha256"], str) or not SHA256_RE.fullmatch(item["sha256"]) or sha256(path) != item["sha256"]:
            raise ValueError(f"Windows component digest mismatch for {name}")
        if sums.get(relative.as_posix()) != item["sha256"]:
            raise ValueError(f"Windows component digest is not bound by SHA256SUMS: {name}")
        if item.get("probe") != "native-build-job":
            raise ValueError(f"Windows component was not collected by its native build job: {name}")
        info = item.get("build_info")
        if not isinstance(info, dict) or collector.package.canonical_digest(info) != item.get("build_info_sha256"):
            raise ValueError(f"Windows component build-info sidecar mismatch: {name}")
        collector.validate_identity(
            info, name, version=manifest["version"], source_sha=manifest["source_sha"],
            target=target, architecture=expected_architecture, contract=contract,
        )
        if pe_machine(path) != machine:
            raise ValueError(f"Windows PE machine mismatch for {name}")
        # Verify the source components again on their native Windows release host.
        actual = collector.probe(path, name)
        if actual != info:
            raise ValueError(f"Windows native build-info probe differs from its bound sidecar: {name}")
        resolved[name] = path
        expected_sums.add(relative.as_posix())
    if set(sums) != expected_sums:
        raise ValueError("Windows SHA256SUMS has an unexpected component set")
    if set(manifest["desktop_payload"]) != {"path", "sha256", "executable", "managed_files"}:
        raise ValueError("Windows Desktop payload record is malformed")
    if manifest["desktop_payload"]["path"] != "artifacts/bin/webcodex-desktop.exe":
        raise ValueError("Windows Desktop payload path is not canonical")
    if manifest["desktop_payload"]["sha256"] != collector.package.tree_digest(resolved["webcodex-desktop"]):
        raise ValueError("Windows Desktop payload hash mismatch")
    managed_files = manifest["desktop_payload"]["managed_files"]
    expected_managed_files = [
        {"path": path, "sha256": manifest["artifacts"][name]["sha256"]}
        for path, name in sorted(MANAGED_INSTALL_FILES.items())
    ]
    if managed_files != expected_managed_files:
        raise ValueError("Windows Desktop managed-file allowlist or component digest mismatch")
    return manifest, resolved


def prepare(candidate_dir: Path, output_dir: Path, platform_name: str) -> dict:
    manifest, artifacts = validate_candidate(candidate_dir, platform_name)
    if output_dir.exists():
        raise ValueError(f"output already exists: {output_dir}")
    output_dir.mkdir(parents=True)
    try:
        runtime = output_dir / "resources" / "webcodex-runtime"
        runtime.mkdir(parents=True)
        resource_map = {}
        for name in RUNTIMES:
            destination = runtime / f"{name}.exe"
            shutil.copyfile(artifacts[name], destination)
            if sha256(destination) != manifest["artifacts"][name]["sha256"]:
                raise ValueError(f"staged WebCodex runtime digest mismatch: {name}")
            resource_map[str(destination.resolve())] = f"webcodex-runtime/{name}.exe"
        config = {
            "version": manifest["version"],
            "bundle": {
                "active": True,
                "targets": ["nsis"],
                "resources": resource_map,
                "windows": {"nsis": {"installMode": "currentUser"}},
            },
        }
        config_path = output_dir / "tauri.unified-installer.conf.json"
        config_path.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
        return {"platform": platform_name, "config": str(config_path)}
    except Exception:
        shutil.rmtree(output_dir, ignore_errors=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-dir", type=Path, required=True)
    parser.add_argument("--platform", choices=tuple(TARGETS), required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = prepare(args.candidate_dir, args.output_dir, args.platform)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        raise SystemExit(str(exc)) from exc
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
