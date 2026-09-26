#!/usr/bin/env python3
"""Collect native CLI/Server/Runner/Desktop artifacts into a verified installer input.

This collector executes every component's --build-info-json command on the
native build host. Cross-platform packages must consume one collector output
from the matching native CI job; this tool has no foreign-binary fallback.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import stat
import sys
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any

import package_unified_installer as package

BINARIES = package.BINARIES
RUNTIMES = package.RUNTIMES
WINDOWS_MANAGED_INSTALL_FILES = {
    "WebCodex.exe": "webcodex-desktop",
    "webcodex-runtime/webcodex.exe": "webcodex",
    "webcodex-runtime/webcodex-server.exe": "webcodex-server",
    "webcodex-runtime/webcodex-runner.exe": "webcodex-runner",
}
MAX_SOURCE_SHA = re.compile(r"^[0-9a-f]{40}$")
NATIVE_TARGETS = {
    **package.PLATFORMS,
    "win32-x64": ("windows", "x86_64-pc-windows-msvc", "x86_64", "x64"),
    "win32-arm64": ("windows", "aarch64-pc-windows-msvc", "aarch64", "arm64"),
}

class CollectionError(ValueError):
    pass

def safe_relative(value: str) -> PurePosixPath:
    if not value or "\\" in value:
        raise CollectionError("artifact paths must be non-empty relative paths")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ("", ".", "..") for part in path.parts):
        raise CollectionError("artifact path escapes the native input root")
    return path

def resolve_input(root: Path, value: str, label: str) -> Path:
    relative = safe_relative(value)
    candidate = root.joinpath(*relative.parts)
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(root)
    except (OSError, ValueError) as exc:
        raise CollectionError(f"{label} is missing or outside the native input root") from exc
    return resolved

def detect_platform() -> str | None:
    machine = platform.machine()
    if platform.system() == "Linux":
        return {"x86_64": "linux-x64", "aarch64": "linux-arm64"}.get(machine)
    if platform.system() == "Darwin":
        return {"x86_64": "darwin-x64", "arm64": "darwin-arm64"}.get(machine)
    if platform.system() == "Windows":
        native_arch = platform.machine().upper()
        if native_arch in {"X86_64", "AMD64"}:
            return "win32-x64"
        if native_arch == "ARM64":
            return "win32-arm64"
    return None

def probe(path: Path, name: str) -> dict[str, Any]:
    try:
        info = package.run_build_info(path, name)
    except package.PackageError as exc:
        raise CollectionError(str(exc)) from exc
    return info

def validate_identity(info: dict[str, Any], name: str, *, version: str, source_sha: str,
                     target: str, architecture: str, contract: dict[str, int]) -> None:
    if info.get("schema_version") != 1 or info.get("binary") != name:
        raise CollectionError(f"{name} build-info schema or binary name mismatch")
    if info.get("version") != version or info.get("git_commit") != source_sha or info.get("git_dirty") is not False:
        raise CollectionError(f"{name} version/source/clean identity mismatch")
    if info.get("target") != target or info.get("architecture") != architecture:
        raise CollectionError(f"{name} native target/architecture mismatch")
    if info.get("desktop_runtime_contract") != contract:
        raise CollectionError(f"{name} Desktop Runtime contract mismatch")
    if info.get("environment_data_format") != 1:
        raise CollectionError(f"{name} environment data format is missing or unsupported")
    built_at = info.get("built_at")
    if not isinstance(built_at, str) or not built_at.isascii() or not built_at.isdigit():
        raise CollectionError(f"{name} built_at is missing or invalid")

def copy_file(source: Path, target: Path) -> None:
    if source.is_symlink() or not source.is_file() or not os.access(source, os.X_OK):
        raise CollectionError(f"native artifact must be a regular executable: {source.name}")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target, follow_symlinks=False)
    target.chmod(0o755)
    if package.sha256_file(source) != package.sha256_file(target):
        raise CollectionError(f"copied native artifact digest mismatch: {source.name}")

def collect(args: argparse.Namespace) -> dict[str, Any]:
    if args.platform not in NATIVE_TARGETS:
        raise CollectionError(f"unsupported native platform: {args.platform}")
    if detect_platform() != args.platform:
        raise CollectionError("manifest collection must run on the exact native platform")
    if not MAX_SOURCE_SHA.fullmatch(args.source_sha):
        raise CollectionError("source SHA must be one exact lower-case 40-hex commit")
    if type(args.workflow_run_id) is not int or args.workflow_run_id <= 0:
        raise CollectionError("workflow run ID must be a positive CI run number")
    if not args.workflow_ref or len(args.workflow_ref) > 256 or any(ch.isspace() for ch in args.workflow_ref):
        raise CollectionError("workflow ref is invalid")
    ci_run = os.environ.get("GITHUB_RUN_ID")
    ci_ref = os.environ.get("GITHUB_WORKFLOW_REF")
    ci_sha = os.environ.get("GITHUB_SHA")
    if ci_run is not None and ci_run != str(args.workflow_run_id):
        raise CollectionError("workflow run ID does not match GitHub Actions context")
    if ci_ref is not None and ci_ref != args.workflow_ref:
        raise CollectionError("workflow ref does not match GitHub Actions context")
    if ci_sha is not None and ci_sha.lower() != args.source_sha:
        raise CollectionError("source SHA does not match GitHub Actions context")
    os_name, target, architecture, _package_arch = NATIVE_TARGETS[args.platform]
    root = args.input_root.resolve(strict=True)
    if not root.is_dir():
        raise CollectionError("native input root must be a directory")
    output = args.output_dir.absolute()
    if output.exists() or output.is_symlink():
        raise CollectionError("manifest output directory already exists")
    paths = {
        name: resolve_input(root, getattr(args, name.replace("-", "_")), name)
        for name in BINARIES
    }
    desktop_payload = resolve_input(root, args.desktop_payload, "Desktop payload")
    executable_rel = safe_relative(args.desktop_executable)
    if os_name == "darwin":
        if desktop_payload.suffix != ".app" or executable_rel.parts[:2] != ("Contents", "MacOS"):
            raise CollectionError("macOS Desktop payload must be an .app with Contents/MacOS executable")
        desktop_executable = desktop_payload.joinpath(*executable_rel.parts)
    else:
        if desktop_payload.is_dir() or executable_rel.as_posix() != desktop_payload.name:
            raise CollectionError("non-macOS Desktop payload must be its executable file")
        desktop_executable = desktop_payload
    if paths["webcodex-desktop"] != desktop_executable.resolve(strict=True):
        raise CollectionError("Desktop executable path does not match the supplied Desktop binary")
    infos = {name: probe(paths[name], name) for name in BINARIES}
    contracts = [info.get("desktop_runtime_contract") for info in infos.values()]
    contract = contracts[0]
    if not isinstance(contract, dict) or set(contract) != {"min_generation", "max_generation"}:
        raise CollectionError("Desktop Runtime contract is malformed")
    for name, info in infos.items():
        validate_identity(info, name, version=args.version, source_sha=args.source_sha,
            target=target, architecture=architecture, contract=contract)
    if not args.version or infos["webcodex"]["version"] != args.version:
        raise CollectionError("requested version does not match native build metadata")
    if not package.VERSION_RE.fullmatch(args.version):
        raise CollectionError("requested version is invalid")
    source_tree_digest = package.tree_digest(desktop_payload)

    output.mkdir(parents=True, mode=0o700)
    try:
        staged = output / "artifacts"
        binary_dir = staged / "bin"
        artifact_paths: dict[str, Path] = {}
        for name in BINARIES:
            if name == "webcodex-desktop" and os_name == "darwin":
                continue
            destination = binary_dir / f"{name}.exe" if os_name == "windows" else binary_dir / name
            copy_file(paths[name], destination)
            artifact_paths[name] = destination
        if os_name == "darwin":
            staged_app = staged / "desktop" / "WebCodexDesktop.app"
            staged_app.parent.mkdir(parents=True)
            shutil.copytree(desktop_payload, staged_app, symlinks=True)
            if package.tree_digest(staged_app) != source_tree_digest:
                raise CollectionError("staged macOS Desktop tree digest mismatch")
            staged_exe = staged_app.joinpath(*executable_rel.parts)
            if package.sha256_file(staged_exe) != package.sha256_file(paths["webcodex-desktop"]):
                raise CollectionError("staged macOS Desktop executable digest mismatch")
            artifact_paths["webcodex-desktop"] = staged_exe
            payload_rel = staged_app.relative_to(output).as_posix()
        else:
            artifact_paths["webcodex-desktop"] = binary_dir / ("webcodex-desktop.exe" if os_name == "windows" else "webcodex-desktop")
            copy_file(paths["webcodex-desktop"], artifact_paths["webcodex-desktop"])
            payload_rel = artifact_paths["webcodex-desktop"].relative_to(output).as_posix()
        artifacts = {}
        checksum_lines = []
        for name in BINARIES:
            path = artifact_paths[name]
            relative = path.relative_to(output).as_posix()
            digest = package.sha256_file(path)
            info = infos[name]
            artifacts[name] = {
                "path": relative,
                "sha256": digest,
                "build_info": info,
                "build_info_sha256": package.canonical_digest(info),
                "probe": "native-build-job",
            }
            checksum_lines.append(f"{digest}  {relative}")
        desktop_payload_record = {
            "path": payload_rel,
            "sha256": package.tree_digest(output.joinpath(*PurePosixPath(payload_rel).parts)),
            # The manifest describes the staged install payload. The source
            # executable may have a different name (for example WebCodex.exe
            # before it is staged as webcodex-desktop.exe).
            "executable": executable_rel.as_posix() if os_name == "darwin" else Path(payload_rel).name,
        }
        if os_name == "windows":
            desktop_payload_record["managed_files"] = [
                {"path": relative, "sha256": package.sha256_file(artifact_paths[name])}
                for relative, name in sorted(WINDOWS_MANAGED_INSTALL_FILES.items())
            ]
        manifest = {
            "schema_version": 1,
            "version": args.version,
            "source_sha": args.source_sha,
            "source_workflow_run_id": args.workflow_run_id,
            "source_workflow_ref": args.workflow_ref,
            "platform": args.platform,
            "target": target,
            "architecture": architecture,
            "desktop_runtime_contract": contract,
            "artifacts": artifacts,
            "desktop_payload": desktop_payload_record,
        }
        manifest_path = output / "source-manifest.json"
        encoded = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
        manifest_path.write_bytes(encoded)
        checksum_lines.append(f"{hashlib.sha256(encoded).hexdigest()}  source-manifest.json")
        (output / "SHA256SUMS").write_text("\n".join(sorted(checksum_lines)) + "\n", encoding="ascii")
        return {
            "platform": args.platform,
            "source_sha": args.source_sha,
            "source_workflow_run_id": args.workflow_run_id,
            "source_workflow_ref": args.workflow_ref,
            "source_manifest": str(manifest_path),
            "sha256sums": str(output / "SHA256SUMS"),
            "artifacts": len(artifacts),
        }
    except Exception:
        shutil.rmtree(output, ignore_errors=True)
        raise

def make_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=tuple(NATIVE_TARGETS), required=True)
    parser.add_argument("--input-root", type=Path, required=True)
    parser.add_argument("--webcodex", required=True, help="relative path to CLI executable")
    parser.add_argument("--webcodex-server", required=True)
    parser.add_argument("--webcodex-runner", required=True)
    parser.add_argument("--webcodex-desktop", required=True, help="relative path to Desktop executable")
    parser.add_argument("--desktop-payload", required=True, help="relative path to Desktop file or .app tree")
    parser.add_argument("--desktop-executable", required=True, help="path relative to Desktop payload")
    parser.add_argument("--version", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--workflow-run-id", required=True, type=int)
    parser.add_argument("--workflow-ref", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser

def main() -> int:
    try:
        result = collect(make_parser().parse_args())
    except (CollectionError, package.PackageError, OSError, ValueError) as exc:
        print(f"unified manifest collection failed: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
