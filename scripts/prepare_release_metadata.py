#!/usr/bin/env python3
"""Validate native WebCodex release archives and generate publish metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import tarfile
from pathlib import Path

try:
    from . import runtime_installer_manifest as runtime_contract
    from . import package_unified_installer as packaging
except ImportError:
    import runtime_installer_manifest as runtime_contract
    import package_unified_installer as packaging

ROOT = Path(__file__).resolve().parent.parent
RUNTIME_PLATFORMS = ("linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64", "win32-arm64")
PLATFORMS = RUNTIME_PLATFORMS
PRIMARY_DESKTOP_PLATFORMS = ("darwin-arm64", "win32-x64", "win32-arm64")
LEGACY_DESKTOP_PLATFORMS = ("darwin-x64", *PRIMARY_DESKTOP_PLATFORMS)
SUPPLEMENTAL_DESKTOP_FIRST_VERSION = (0, 4, 3)
BINARIES = ("webcodex", "webcodex-server", "webcodex-runner")
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
INSTALLER_PLATFORMS = tuple(INSTALLER_TARGETS)


def installer_filename(version: str, target: str) -> str:
    try:
        platform, package_format = INSTALLER_TARGETS[target]
    except KeyError as exc:
        raise SystemExit(f"unsupported installer target: {target}") from exc
    return f"webcodex-unified-v{version}-{platform}.{package_format}"

def source_manifest_filename(version: str, platform: str) -> str:
    return f"webcodex-source-v{version}-{platform}.json"


def validate_source_manifest(path: Path, version: str, platform: str, source_sha: str, run_id: int, workflow_ref: str, *, package_flavor: str = "full") -> dict:
    try:
        with path.open("rb") as stream:
            raw = stream.read(packaging.MAX_BYTES + 1)
        if len(raw) > packaging.MAX_BYTES:
            raise ValueError("source manifest exceeds its size limit")
        value = runtime_contract.parse(raw)
    except (OSError, UnicodeError, ValueError) as exc:
        raise SystemExit(f"invalid source manifest: {path}") from exc
    required = {
        "schema_version", "version", "source_sha", "source_workflow_run_id",
        "source_workflow_ref", "platform", "target", "architecture",
        "desktop_runtime_contract", "artifacts", "desktop_payload",
    }
    runtime = package_flavor == "runtime"
    if runtime:
        required.remove("desktop_payload")
        required.add("package_flavor")
    if not isinstance(value, dict) or set(value) != required:
        raise SystemExit(f"source manifest has an unexpected schema: {path}")
    if type(value["schema_version"]) is not int or value["schema_version"] != (2 if runtime else 1) or value["version"] != version or value["platform"] != platform:
        raise SystemExit(f"source manifest identity mismatch: {path}")
    if (value["source_sha"], value["source_workflow_run_id"], value["source_workflow_ref"]) != (source_sha, run_id, workflow_ref):
        raise SystemExit(f"source manifest CI provenance mismatch: {path}")
    if not isinstance(value["source_sha"], str) or not __import__("re").fullmatch(r"[0-9a-f]{40,64}", value["source_sha"]):
        raise SystemExit(f"source manifest source SHA is invalid: {path}")
    if type(value["source_workflow_run_id"]) is not int or value["source_workflow_run_id"] <= 0:
        raise SystemExit(f"source manifest workflow run ID is invalid: {path}")
    if not isinstance(value["source_workflow_ref"], str) or not value["source_workflow_ref"]:
        raise SystemExit(f"source manifest workflow ref is invalid: {path}")
    if not isinstance(value["artifacts"], dict) or set(value["artifacts"]) != set(BINARIES if runtime else (*BINARIES, "webcodex-desktop")):
        raise SystemExit(f"source manifest component set is invalid: {path}")
    for name, record in value["artifacts"].items():
        if not isinstance(record, dict) or not isinstance(record.get("build_info"), dict):
            raise SystemExit(f"source manifest build identity is invalid: {path}: {name}")
        info = record["build_info"]
        if (info.get("version") != version or info.get("git_commit") != value["source_sha"]
                or info.get("git_dirty") is not False or info.get("environment_data_format") != 1):
            raise SystemExit(f"source manifest component provenance/data format mismatch: {path}: {name}")
    if runtime:
        if value.get("package_flavor") != "runtime" or platform not in ("linux-x64", "linux-arm64"):
            raise SystemExit("invalid Runtime source flavor/platform")
        _, target, architecture, _ = packaging.PLATFORMS[platform]
        if (value["target"], value["architecture"]) != (target, architecture):
            raise SystemExit("Runtime source architecture mismatch")
        for name, record in value["artifacts"].items():
            if set(record) != {"path", "sha256", "build_info", "build_info_sha256", "probe"} or record["path"] != f"artifacts/bin/{name}" or record["probe"] != "native-build-job":
                raise SystemExit("Runtime source requires exact native component attestations")
            if not isinstance(record["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", record["sha256"]) or record["build_info_sha256"] != packaging.canonical_digest(record["build_info"]):
                raise SystemExit("Runtime source artifact/build-info hash mismatch")
            try:
                packaging.validate_build_info(record["build_info"], name, value)
            except packaging.PackageError as exc:
                raise SystemExit(str(exc)) from exc
    return value


def validate_runtime_archive(source: dict, path: Path) -> None:
    """Bind the Runtime attestations to the unchanged three-component archive."""
    if normalized_members(path) != set(BINARIES):
        raise ValueError("Runtime native archive component set mismatch")
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            if member.isfile():
                stream = archive.extractfile(member)
                assert stream is not None
                digest = hashlib.sha256()
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
                if source["artifacts"][member.name.removeprefix("./")]["sha256"] != digest.hexdigest():
                    raise ValueError("Runtime source differs from retained native archive bytes")


def validate_installer(path: Path, target: str) -> None:
    platform, package_format = (INSTALLER_TARGETS | runtime_contract.RUNTIME_TARGETS)[target]
    with path.open("rb") as handle:
        magic = handle.read(8)
    expected = {
        "deb": b"!<arch>\n",
        "rpm": bytes.fromhex("edabeedb"),
        "pkg": b"xar!",
        "exe": b"MZ",
    }[package_format]
    if not magic.startswith(expected):
        raise SystemExit(f"installer has an invalid {target} container signature: {path}")

def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def archive_filename(version: str, platform: str) -> str:
    return f"webcodex-v{version}-{platform}.tar.gz"


def desktop_platforms_for_version(version: str) -> tuple[str, ...]:
    core_text = version.split("+", 1)[0].split("-", 1)[0]
    try:
        core = tuple(int(part) for part in core_text.split("."))
    except ValueError as exc:
        raise SystemExit(f"invalid release version: {version}") from exc
    if len(core) != 3:
        raise SystemExit(f"invalid release version: {version}")
    return PRIMARY_DESKTOP_PLATFORMS if core >= SUPPLEMENTAL_DESKTOP_FIRST_VERSION else LEGACY_DESKTOP_PLATFORMS


def desktop_filename(version: str, platform: str) -> str:
    if platform not in LEGACY_DESKTOP_PLATFORMS:
        raise SystemExit(f"unsupported Desktop platform: {platform}")
    suffix = "-setup.exe" if platform.startswith("win32-") else ".dmg"
    return f"webcodex-desktop-v{version}-{platform}{suffix}"


def expected_members(platform: str) -> set[str]:
    suffix = ".exe" if platform in {"win32-x64", "win32-arm64"} else ""
    return {f"{name}{suffix}" for name in BINARIES}


def normalized_members(path: Path) -> set[str]:
    members: set[str] = set()
    with tarfile.open(path, "r:gz") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile():
                raise SystemExit(f"archive contains non-file member: {path.name}: {member.name}")
            name = member.name
            while name.startswith("./"):
                name = name[2:]
            if not name or "/" in name or "\\" in name:
                raise SystemExit(f"archive contains nested or invalid path: {path.name}: {member.name}")
            if name in members:
                raise SystemExit(f"archive contains duplicate member: {path.name}: {name}")
            members.add(name)
    return members


def atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(content, encoding="utf-8", newline="\n")
    os.replace(tmp, path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--require-unified-installers", action="store_true")
    parser.add_argument("--require-runtime-installers", action="store_true")
    parser.add_argument("--artifact-dir", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--repo", default="yyjeqhc/webcodex")
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--workflow-run-id", required=True, type=int)
    parser.add_argument("--workflow-ref", required=True)
    parser.add_argument(
        "--package-json",
        type=Path,
        default=ROOT / "npm" / "webcodex" / "package.json",
    )
    args = parser.parse_args()

    if args.require_runtime_installers and not args.require_unified_installers:
        raise SystemExit("Runtime installers require explicit Full installer selection")
    version = args.version.removeprefix("v")
    if not version or any(ch.isspace() for ch in version):
        raise SystemExit("invalid release version")

    package = json.loads(args.package_json.read_text(encoding="utf-8"))
    if package.get("version") != version:
        raise SystemExit(
            f"package version mismatch: expected {version}, found {package.get('version')!r}"
        )

    artifacts: dict[str, dict[str, str]] = {}
    checksum_lines: list[str] = []

    for platform in RUNTIME_PLATFORMS:
        filename = archive_filename(version, platform)
        path = args.artifact_dir / filename
        if not path.is_file() or path.stat().st_size <= 0:
            raise SystemExit(f"missing or empty artifact: {path}")

        actual = normalized_members(path)
        expected = expected_members(platform)
        if actual != expected:
            raise SystemExit(
                f"unexpected archive contents for {platform}: "
                f"expected={sorted(expected)} actual={sorted(actual)}"
            )

        digest = sha256(path)
        checksum_lines.append(f"{digest}  {filename}")
        artifacts[platform] = {
            "url": f"https://github.com/{args.repo}/releases/download/v{version}/{filename}",
            "sha256": digest,
        }

    for platform in desktop_platforms_for_version(version):
        desktop_name = desktop_filename(version, platform)
        desktop_path = args.artifact_dir / desktop_name
        if not desktop_path.is_file() or desktop_path.stat().st_size <= 0:
            raise SystemExit(f"missing or empty Desktop distribution artifact: {desktop_path}")
        desktop_digest = sha256(desktop_path)
        checksum_lines.append(f"{desktop_digest}  {desktop_name}")

    installer_paths = {
        target: args.artifact_dir / installer_filename(version, target)
        for target in INSTALLER_TARGETS
    }
    source_paths = {
        platform: args.artifact_dir / source_manifest_filename(version, platform)
        for platform in RUNTIME_PLATFORMS
    }
    present_installers = {target for target, path in installer_paths.items() if path.exists()}
    present_sources = {platform for platform, path in source_paths.items() if path.exists()}
    expected_installer_names = {path.name for path in installer_paths.values()}
    expected_windows_evidence = {
        name for platform in RUNTIME_PLATFORMS if platform.startswith("win32-")
        for name in (f"webcodex-unified-v{version}-{platform}.exe.provenance.json", f"webcodex-unified-v{version}-{platform}.source-manifest.json", f"webcodex-unified-v{version}-{platform}.candidate-SHA256SUMS")
    }
    unexpected_installers = {
        path.name for path in args.artifact_dir.glob(f"webcodex-unified-v{version}-*")
        if path.is_file() and not path.name.endswith(".sha256") and path.name not in expected_installer_names | expected_windows_evidence
    }
    if unexpected_installers:
        raise SystemExit(f"unexpected unified installers: {', '.join(sorted(unexpected_installers))}")
    if args.require_unified_installers and present_installers != set(INSTALLER_TARGETS):
        raise SystemExit("required unified installer set is missing or incomplete")
    if present_sources and not present_installers:
        raise SystemExit("source manifests are present without unified installers")
    if present_installers and present_installers != set(INSTALLER_TARGETS):
        missing = sorted(set(INSTALLER_TARGETS) - present_installers)
        raise SystemExit(f"incomplete unified installer set; missing: {', '.join(missing)}")
    if present_installers and present_sources != set(RUNTIME_PLATFORMS):
        missing = sorted(set(RUNTIME_PLATFORMS) - present_sources)
        raise SystemExit(f"incomplete source manifest set; missing: {', '.join(missing)}")
    installers: dict[str, dict[str, str]] = {}
    source_digests: dict[str, str] = {}
    source_manifests: dict[str, dict] = {}
    if present_installers:
        for platform, source_path in source_paths.items():
            source_manifests[platform] = validate_source_manifest(source_path, version, platform, args.source_sha, args.workflow_run_id, args.workflow_ref)
            source_digest = sha256(source_path)
            source_digests[platform] = source_digest
            checksum_lines.append(f"{source_digest}  {source_path.name}")
        for target, path in installer_paths.items():
            if not path.is_file() or path.stat().st_size <= 0:
                raise SystemExit(f"missing or empty installer: {path}")
            platform, package_format = INSTALLER_TARGETS[target]
            validate_installer(path, target)
            filename = path.name
            digest = sha256(path)
            checksum_lines.append(f"{digest}  {filename}")
            source_path = source_paths[platform]
            sidecar = path.with_suffix(path.suffix + ".provenance.json")
            source_info = source_manifests[platform]["artifacts"]["webcodex"]["build_info"]
            marker_present = "windows_guarded_bootstrap_contract" in source_info
            source_marker = source_info.get("windows_guarded_bootstrap_contract")
            if marker_present and (package_format != "exe" or type(source_marker) is not int or source_marker != 1):
                raise SystemExit("invalid Windows guarded bootstrap source attestation")
            if marker_present:
                cli_record = source_manifests[platform]["artifacts"]["webcodex"]
                raw_digest = hashlib.sha256(json.dumps(cli_record["build_info"], sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
                if cli_record.get("build_info_sha256") != raw_digest:
                    raise SystemExit("Windows guarded bootstrap raw CLI attestation hash is invalid")
            if marker_present and not sidecar.is_file():
                raise SystemExit("Windows guarded source requires matching outer bootstrap provenance")
            if package_format == "exe" and sidecar.is_file():
                evidence = json.loads(sidecar.read_text(encoding="utf-8"))
                if marker_present or "guarded_handoff_version" in evidence:
                    expected_fields = {"schema_version", "guarded_handoff_version", "platform", "version", "source_sha", "workflow_run_id", "workflow_ref", "candidate_manifest_sha256", "inner_installer_sha256", "installer_sha256"}
                    if set(evidence) != expected_fields or type(evidence["guarded_handoff_version"]) is not int or evidence["guarded_handoff_version"] != 1 or source_marker != 1:
                        raise SystemExit("invalid Windows guarded handoff provenance")
                    if (evidence["schema_version"], evidence["platform"], evidence["version"], evidence["source_sha"], evidence["workflow_run_id"], evidence["workflow_ref"], evidence["installer_sha256"]) != (1, platform, version, args.source_sha, args.workflow_run_id, args.workflow_ref, digest):
                        raise SystemExit("Windows guarded handoff provenance does not bind this artifact")
                    if any(not isinstance(evidence[key], str) or not re.fullmatch(r"[0-9a-f]{64}", evidence[key]) for key in ("candidate_manifest_sha256", "inner_installer_sha256")):
                        raise SystemExit("invalid Windows guarded handoff candidate provenance")
                    candidate_manifest = args.artifact_dir / f"webcodex-unified-v{version}-{platform}.source-manifest.json"
                    if not candidate_manifest.is_file() or sha256(candidate_manifest) != evidence["candidate_manifest_sha256"] or evidence["candidate_manifest_sha256"] != source_digests[platform]:
                        raise SystemExit("Windows guarded handoff source and candidate manifest are missing or changed")
                    validate_source_manifest(candidate_manifest, version, platform, args.source_sha, args.workflow_run_id, args.workflow_ref)
            installers[target] = {
                "platform": platform,
                "format": package_format,
                "filename": filename,
                "url": f"https://github.com/{args.repo}/releases/download/v{version}/{filename}",
                "sha256": digest,
                "source_manifest_url": f"https://github.com/{args.repo}/releases/download/v{version}/{source_path.name}",
                "source_manifest_sha256": source_digests[platform],
            }

    runtime_installers = {}
    runtime_names = {runtime_contract.installer_filename(version, target) for target in runtime_contract.RUNTIME_TARGETS}
    runtime_names.update(runtime_contract.source_filename(version, platform) for platform in ("linux-x64", "linux-arm64"))
    present_runtime = {path.name for path in args.artifact_dir.glob(f"webcodex-runtime-*v{version}-*") if path.is_file() and not path.name.endswith(".sha256")}
    if not args.require_runtime_installers and present_runtime:
        raise SystemExit("Runtime artifacts require explicit Runtime installer selection")
    if args.require_runtime_installers:
        if present_runtime != runtime_names:
            raise SystemExit("Runtime installer/source set is missing, incomplete or unexpected")
        runtime_sources = {}
        for platform in ("linux-x64", "linux-arm64"):
            path = args.artifact_dir / runtime_contract.source_filename(version, platform)
            source = validate_source_manifest(path, version, platform, args.source_sha, args.workflow_run_id, args.workflow_ref, package_flavor="runtime")
            try:
                validate_runtime_archive(source, args.artifact_dir / archive_filename(version, platform))
            except ValueError as exc:
                raise SystemExit(str(exc)) from exc
            runtime_sources[platform] = sha256(path)
            checksum_lines.append(f"{runtime_sources[platform]}  {path.name}")
        for target, (platform, fmt) in runtime_contract.RUNTIME_TARGETS.items():
            path = args.artifact_dir / runtime_contract.installer_filename(version, target)
            validate_installer(path, target)
            digest = sha256(path)
            source_name = runtime_contract.source_filename(version, platform)
            checksum_lines.append(f"{digest}  {path.name}")
            runtime_installers[target] = {"platform": platform, "format": fmt, "flavor": "runtime", "filename": path.name,
                "url": f"https://github.com/{args.repo}/releases/download/v{version}/{path.name}", "sha256": digest,
                "source_manifest_url": f"https://github.com/{args.repo}/releases/download/v{version}/{source_name}",
                "source_manifest_sha256": runtime_sources[platform]}

    manifest = {
        "version": version,
        "binaries": list(BINARIES),
        "artifacts": artifacts,
    }
    if installers:
        manifest["installers"] = installers

    if runtime_installers:
        canonical = {"schema_version": 2, **manifest, "installers": {**installers, **runtime_installers}}
        canonical_path = args.output_dir / "manifest-v2.json"
        atomic_write(canonical_path, runtime_contract.encode(canonical))
        checksum_lines.append(f"{sha256(canonical_path)}  manifest-v2.json")
        manifest = runtime_contract.legacy_projection(canonical)
    manifest_path = args.output_dir / "manifest.json"
    atomic_write(manifest_path, json.dumps(manifest, indent=2) + "\n")
    if installers:
        # The same retained bytes serve npm, the download page, and Desktop.
        # Legacy runtime-only manifests remain outside the public asset set.
        checksum_lines.append(f"{sha256(manifest_path)}  manifest.json")
    if runtime_installers and len(checksum_lines) + 1 != 32:
        raise SystemExit("Runtime release requires exactly 32 primary checksum records including Runtime contract")
    atomic_write(args.output_dir / "SHA256SUMS", "\n".join(checksum_lines) + "\n")

    print(f"release metadata prepared for {version}")
    for platform, artifact in artifacts.items():
        print(f"{platform} {artifact['sha256']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
