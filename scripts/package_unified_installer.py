#!/usr/bin/env python3
"""Build a unified WebCodex .deb or macOS .pkg from a verified source manifest.

The manifest records the SHA-256 and --build-info-json result for CLI, Server,
Runner, and Desktop. SHA256SUMS binds the manifest to the build artifacts.
This script only stages files and builds package bytes; it never installs
packages or starts services.
"""
from __future__ import annotations
import argparse, hashlib, json, os, platform, re, shutil, stat, subprocess, sys, tempfile
from pathlib import Path, PurePosixPath
from typing import Any

BINARIES = ("webcodex", "webcodex-server", "webcodex-runner", "webcodex-desktop")
RUNTIMES = BINARIES[:3]
VERSION_RE = re.compile(r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
SOURCE_RE = re.compile(r"^[0-9a-f]{40}$")
PLATFORMS = {
    "linux-x64": ("linux", "x86_64-unknown-linux-gnu", "x86_64", "amd64"),
    "linux-arm64": ("linux", "aarch64-unknown-linux-gnu", "aarch64", "arm64"),
    "darwin-x64": ("darwin", "x86_64-apple-darwin", "x86_64", "x86_64"),
    "darwin-arm64": ("darwin", "aarch64-apple-darwin", "aarch64", "arm64"),
}
MAX_BYTES = 1024 * 1024

class PackageError(ValueError):
    pass

def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def canonical_digest(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return hashlib.sha256(encoded).hexdigest()

def tree_digest(path: Path) -> str:
    if path.is_symlink():
        raise PackageError("Desktop payload must not be a symbolic link")
    digest = hashlib.sha256()
    if path.is_file():
        paths = [path]
        base = path.parent
    elif path.is_dir():
        paths = sorted(path.rglob("*"), key=lambda p: p.relative_to(path).as_posix())
        base = path
    else:
        raise PackageError("Desktop payload is missing")
    for child in paths:
        rel = (child.name if child == path else child.relative_to(base).as_posix()).encode()
        if child.is_symlink():
            target = os.readlink(child)
            if os.path.isabs(target):
                raise PackageError("Desktop payload tree contains an absolute symbolic link")
            try:
                child.resolve(strict=True).relative_to(path.resolve())
            except (OSError, ValueError) as exc:
                raise PackageError("Desktop payload tree contains an escaping or broken symbolic link") from exc
            digest.update(b"L\0" + rel + b"\0" + target.encode("utf-8") + b"\0")
        elif child.is_dir():
            digest.update(b"D\0" + rel + b"\0")
        elif child.is_file():
            digest.update(b"F\0" + rel + b"\0")
            digest.update(f"{stat.S_IMODE(child.stat().st_mode):04o}".encode() + b"\0")
            with child.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
        else:
            raise PackageError("Desktop payload contains a special file")
    return digest.hexdigest()

def safe_relative(value: object, label: str) -> PurePosixPath:
    if not isinstance(value, str) or not value or "\\" in value:
        raise PackageError(f"invalid relative path for {label}")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in ("", ".", "..") for part in path.parts):
        raise PackageError(f"invalid relative path for {label}")
    return path

def parse_sha256sums(path: Path) -> dict[str, str]:
    try:
        raw = path.read_bytes()
        if len(raw) > MAX_BYTES:
            raise PackageError("source SHA256SUMS exceeds its size limit")
        lines = raw.decode("ascii").splitlines()
    except (OSError, UnicodeDecodeError) as exc:
        raise PackageError("source SHA256SUMS is unavailable or invalid") from exc
    result = {}
    for line in lines:
        if not line:
            continue
        match = re.fullmatch(r"([0-9a-f]{64})  ([^\s]+)", line)
        if not match or match.group(2) in result:
            raise PackageError("source SHA256SUMS has an invalid or duplicate entry")
        result[match.group(2)] = match.group(1)
    return result

def run_build_info(path: Path, name: str) -> dict[str, Any]:
    try:
        completed = subprocess.run([str(path), "--build-info-json"], stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False, timeout=8,
            env={key: os.environ[key] for key in ("PATH", "LANG", "TMPDIR") if key in os.environ})
    except (OSError, subprocess.SubprocessError) as exc:
        raise PackageError(f"{name} build-info probe failed") from exc
    if completed.returncode != 0 or len(completed.stdout) > 128 * 1024:
        raise PackageError(f"{name} build-info probe failed")
    try:
        value = json.loads(completed.stdout)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PackageError(f"{name} build-info probe did not return JSON") from exc
    if not isinstance(value, dict):
        raise PackageError(f"{name} build-info is not an object")
    return value

def validate_build_info(value: object, name: str, manifest: dict[str, Any]) -> dict:
    if not isinstance(value, dict) or value.get("schema_version") != 1 or value.get("binary") != name:
        raise PackageError(f"{name} build-info schema or binary name mismatch")
    if value.get("version") != manifest["version"]:
        raise PackageError(f"{name} version mismatch")
    if value.get("git_commit") != manifest["source_sha"] or value.get("git_dirty") is not False:
        raise PackageError(f"{name} source provenance mismatch or dirty build")
    if value.get("target") != manifest["target"] or value.get("architecture") != manifest["architecture"]:
        raise PackageError(f"{name} target architecture mismatch")
    built_at = value.get("built_at")
    if not isinstance(built_at, str) or not built_at.isascii() or not built_at.isdigit():
        raise PackageError(f"{name} built_at identity is missing or invalid")
    contract = manifest["desktop_runtime_contract"]
    if value.get("desktop_runtime_contract") != contract:
        raise PackageError(f"{name} Desktop Runtime contract mismatch")
    if value.get("environment_data_format") != 1:
        raise PackageError(f"{name} environment data format is missing or unsupported")
    return value

def validate_manifest(manifest_path: Path, sums_path: Path, input_root: Path, platform_name: str) -> dict[str, Any]:
    if platform_name not in PLATFORMS:
        raise PackageError(f"unsupported platform: {platform_name}")
    try:
        raw = manifest_path.read_bytes()
    except OSError as exc:
        raise PackageError("source manifest is unavailable") from exc
    if len(raw) > MAX_BYTES:
        raise PackageError("source manifest exceeds its size limit")
    sums = parse_sha256sums(sums_path)
    if sums.get(manifest_path.name) != hashlib.sha256(raw).hexdigest():
        raise PackageError("source manifest SHA-256 does not match SHA256SUMS")
    try:
        manifest = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PackageError("source manifest is not valid JSON") from exc
    required = {"schema_version", "version", "source_sha", "platform", "target", "architecture",
        "source_workflow_run_id", "source_workflow_ref", "desktop_runtime_contract", "artifacts", "desktop_payload"}
    if not isinstance(manifest, dict) or set(manifest) != required or manifest.get("schema_version") != 1:
        raise PackageError("unsupported unified source manifest schema")
    os_name, target, architecture, _ = PLATFORMS[platform_name]
    if (manifest.get("platform"), manifest.get("target"), manifest.get("architecture")) != (platform_name, target, architecture):
        raise PackageError("source manifest platform/target mismatch")
    if not isinstance(manifest.get("version"), str) or not VERSION_RE.fullmatch(manifest["version"]):
        raise PackageError("source manifest version is invalid")
    if not isinstance(manifest.get("source_sha"), str) or not SOURCE_RE.fullmatch(manifest["source_sha"]):
        raise PackageError("source manifest commit is invalid")
    run_id = manifest.get("source_workflow_run_id")
    workflow_ref = manifest.get("source_workflow_ref")
    if type(run_id) is not int or run_id <= 0 or not isinstance(workflow_ref, str) or not workflow_ref or len(workflow_ref) > 256 or any(ch.isspace() for ch in workflow_ref):
        raise PackageError("source manifest workflow provenance is invalid")
    contract = manifest.get("desktop_runtime_contract")
    if (not isinstance(contract, dict) or set(contract) != {"min_generation", "max_generation"}
        or type(contract.get("min_generation")) is not int or type(contract.get("max_generation")) is not int
        or not 0 < contract["min_generation"] <= contract["max_generation"] <= 65535):
        raise PackageError("source manifest Desktop Runtime contract is invalid")
    root = input_root.resolve(strict=True)
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, dict) or set(artifacts) != set(BINARIES):
        raise PackageError("source manifest must describe CLI, Server, Runner, and Desktop")
    host_platform = None
    if platform.system() == "Linux":
        host_platform = {"x86_64": "linux-x64", "aarch64": "linux-arm64"}.get(platform.machine())
    elif platform.system() == "Darwin":
        host_platform = {"x86_64": "darwin-x64", "arm64": "darwin-arm64"}.get(platform.machine())
    host = platform_name == host_platform
    resolved_artifacts = {}
    for name in BINARIES:
        item = artifacts[name]
        fields = {"path", "sha256", "build_info", "build_info_sha256", "probe"}
        if not isinstance(item, dict) or set(item) != fields:
            raise PackageError(f"{name} manifest entry is malformed")
        relative = safe_relative(item["path"], name)
        path = root.joinpath(*relative.parts)
        try:
            resolved = path.resolve(strict=True)
            resolved.relative_to(root)
        except (OSError, ValueError) as exc:
            raise PackageError(f"{name} input path is missing or escapes the input root") from exc
        if path.is_symlink() or not resolved.is_file() or not os.access(resolved, os.X_OK):
            raise PackageError(f"{name} must be a regular executable file")
        if not isinstance(item["sha256"], str) or not SHA256_RE.fullmatch(item["sha256"]) or sha256_file(resolved) != item["sha256"]:
            raise PackageError(f"{name} SHA-256 mismatch")
        if sums.get(relative.as_posix()) != item["sha256"]:
            raise PackageError(f"{name} SHA-256 is not bound by SHA256SUMS")
        info = item["build_info"]
        if not isinstance(item["build_info_sha256"], str) or canonical_digest(info) != item["build_info_sha256"]:
            raise PackageError(f"{name} build-info sidecar SHA-256 mismatch")
        if item["probe"] not in ("native-build-job", "local-executable"):
            raise PackageError(f"{name} build-info probe provenance is invalid")
        if name in RUNTIMES and host:
            if run_build_info(resolved, name) != info:
                raise PackageError(f"{name} measured build-info differs from its bound sidecar")
        elif item["probe"] != "native-build-job":
            raise PackageError(f"{name} cross-target build-info must come from its native build job")
        validate_build_info(info, name, manifest)
        resolved_artifacts[name] = resolved
    desktop = manifest["desktop_payload"]
    if not isinstance(desktop, dict) or set(desktop) != {"path", "sha256", "executable"}:
        raise PackageError("Desktop payload manifest entry is malformed")
    relative = safe_relative(desktop["path"], "Desktop payload")
    payload = root.joinpath(*relative.parts)
    try:
        resolved_payload = payload.resolve(strict=True)
        resolved_payload.relative_to(root)
    except (OSError, ValueError) as exc:
        raise PackageError("Desktop payload path is missing or escapes the input root") from exc
    if tree_digest(payload) != desktop["sha256"]:
        raise PackageError("Desktop payload tree SHA-256 mismatch")
    desktop_bin = resolved_artifacts["webcodex-desktop"]
    executable = safe_relative(desktop["executable"], "Desktop executable")
    if os_name == "darwin":
        if payload.suffix != ".app" or executable.parts[:2] != ("Contents", "MacOS"):
            raise PackageError("macOS Desktop payload must be an .app bundle")
        embedded_desktop = payload.joinpath(*executable.parts).resolve()
        if desktop_bin != embedded_desktop or sha256_file(embedded_desktop) != artifacts["webcodex-desktop"]["sha256"]:
            raise PackageError("macOS Desktop executable does not match its verified build identity")
        resources = payload / "Contents/Resources/webcodex-runtime"
        for name in RUNTIMES:
            embedded = resources / name
            if not embedded.is_file() or embedded.is_symlink() or sha256_file(embedded) != artifacts[name]["sha256"]:
                raise PackageError(f"macOS Desktop bundle runtime does not match {name}")
    else:
        if payload.is_dir() or executable.as_posix() != payload.name or desktop_bin != payload.resolve():
            raise PackageError("Linux Desktop payload must be its verified executable file")
        resources = None
    manifest["_input_root"] = root
    manifest["_artifacts"] = resolved_artifacts
    manifest["_desktop_payload"] = resolved_payload
    manifest["_desktop_resources"] = resources
    return manifest

def _copy_exec(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination, follow_symlinks=False)
    destination.chmod(0o755)

def public_manifest(manifest: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in manifest.items() if not key.startswith("_")}

def _copy_upgrade_candidate(input_root: Path, destination: Path, manifest: dict[str, Any]) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for name in ("source-manifest.json", "SHA256SUMS"):
        shutil.copyfile(input_root / name, destination / name)
    paths = {item["path"] for item in manifest["artifacts"].values()}
    paths.add(manifest["desktop_payload"]["path"])
    roots = sorted(paths, key=lambda value: (value.count("/"), value))
    paths = {value for value in roots if not any(value.startswith(parent + "/") for parent in roots if parent != value)}
    for value in paths:
        relative = safe_relative(value, "upgrade candidate")
        source = input_root.joinpath(*relative.parts)
        target = destination.joinpath(*relative.parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(source, target, symlinks=True)
        else:
            shutil.copy2(source, target, follow_symlinks=False)


def _upgrade_preinstall(candidate_rel: str, transaction_dir: str, authorization_file: str, runtime_dir: str, ownership_checks: str = "", recovery_dir: str = "", same_marker_file: str = "", candidate_cli_sha256: str = "", hash_checker: str = "", installed_cli: str = "") -> str:
    return f"""#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
candidate="$script_dir/{candidate_rel}"
candidate_cli="$candidate/artifacts/bin/webcodex"
{f'''if ! printf '%s  %s\\n' '{candidate_cli_sha256}' "$candidate_cli" | {hash_checker} -c - >/dev/null 2>&1; then
  echo "WebCodex candidate CLI failed its manifest-bound SHA-256 check." >&2
  exit 1
fi''' if candidate_cli_sha256 else ''}
cli="$candidate_cli"
if [ -x '{installed_cli}' ]; then cli='{installed_cli}'; fi
{ownership_checks}
if [ -f '{authorization_file}' ]; then
  "$cli" environment installer-verify --candidate-dir "$candidate" --expected-runtime-dir '{runtime_dir}' --json
  if [ -n '{recovery_dir}' ]; then
    recovery='{recovery_dir}'
    mkdir -p "$recovery"
    chmod 0700 "$recovery"
    if [ -d "$recovery/candidate" ]; then
      cmp "$candidate/source-manifest.json" "$recovery/candidate/source-manifest.json"
      cmp "$candidate/SHA256SUMS" "$recovery/candidate/SHA256SUMS"
    else
      cp -R "$candidate" "$recovery/candidate"
      chmod -R go-rwx "$recovery/candidate"
    fi
  fi
elif [ "$existing_install" -eq 1 ]; then
  if ! "$cli" environment installer-verify-same --candidate-dir "$candidate" --expected-runtime-dir '{runtime_dir}' --json; then
    echo "WebCodex upgrade needs a prepared owner receipt. Prepare from the original user account, then authorize that receipt before retrying this package." >&2
    exit 1
  fi
  if [ -n '{recovery_dir}' ]; then
    recovery='{recovery_dir}'
    mkdir -p "$recovery"
    chmod 0700 "$recovery"
    if [ -d "$recovery/candidate" ]; then
      cmp "$candidate/source-manifest.json" "$recovery/candidate/source-manifest.json"
      cmp "$candidate/SHA256SUMS" "$recovery/candidate/SHA256SUMS"
    else
      cp -R "$candidate" "$recovery/candidate"
      chmod -R go-rwx "$recovery/candidate"
    fi
    marker='{same_marker_file}'
    marker_tmp="$marker.tmp.$$"
    printf '%s\\n' same-package > "$marker_tmp"
    chmod 0600 "$marker_tmp"
    mv -f "$marker_tmp" "$marker"
  fi
else
  "$cli" environment upgrade-preflight --candidate-dir "$candidate" --environment-dir '{transaction_dir}' --json
  "$cli" environment upgrade-prepare --candidate-dir "$candidate" --environment-dir '{transaction_dir}' --json
fi
"""


def _upgrade_postinstall(installed_cli: str, transaction_dir: str, authorization_file: str, recovery_dir: str = "", runtime_dir: str = "", same_marker_file: str = "") -> str:
    return f"""#!/bin/sh
set -eu
if [ -f '{authorization_file}' ]; then
  if ! '{installed_cli}' environment installer-finish --json; then
    echo "WebCodex owner-context upgrade completion failed; the authorization receipt is retained for recovery." >&2
    exit 1
  fi
  if [ -n '{recovery_dir}' ]; then rm -rf '{recovery_dir}/candidate'; fi
  if [ -n '{same_marker_file}' ]; then rm -f '{same_marker_file}'; fi
  exit 0
fi
if [ -f '{same_marker_file}' ]; then
  if ! '{installed_cli}' environment installer-verify-same --candidate-dir '{recovery_dir}/candidate' --expected-runtime-dir '{runtime_dir}' --json; then
    echo "WebCodex installed files changed after idempotency preflight; the verification marker and candidate are retained." >&2
    exit 1
  fi
  rm -f '{same_marker_file}'
  rm -rf '{recovery_dir}/candidate'
  exit 0
fi
if ! '{installed_cli}' environment upgrade-finish --environment-dir '{transaction_dir}' --json; then
  echo "WebCodex install completion could not be verified; keep the package contents and use the documented manual recovery steps." >&2
  exit 1
fi
"""


def stage_linux(manifest: dict[str, Any], stage: Path, input_root: Path) -> None:
    artifacts = manifest["artifacts"]
    resolved = manifest["_artifacts"]
    _copy_exec(manifest["_desktop_payload"], stage / "usr/lib/webcodex/webcodex-desktop")
    for name in RUNTIMES:
        _copy_exec(resolved[name], stage / f"usr/lib/webcodex/webcodex-runtime/{name}")
    (stage / "usr/bin").mkdir(parents=True, exist_ok=True)
    for name in RUNTIMES:
        (stage / "usr/bin" / name).symlink_to(f"../lib/webcodex/webcodex-runtime/{name}")
    apps = stage / "usr/share/applications"
    apps.mkdir(parents=True)
    (apps / "webcodex.desktop").write_text("[Desktop Entry]\nType=Application\nName=WebCodex Desktop\n"
        "Exec=/usr/lib/webcodex/webcodex-desktop\nTerminal=false\nCategories=Development;\n", encoding="utf-8")
    doc = stage / "usr/share/doc/webcodex"
    doc.mkdir(parents=True)
    (doc / "unified-source-manifest.json").write_text(json.dumps(public_manifest(manifest), indent=2) + "\n", encoding="utf-8")
    control = stage / "DEBIAN"
    control.mkdir()
    arch = PLATFORMS[manifest["platform"]][3]
    (control / "control").write_text(
        f"Package: webcodex\nVersion: {manifest['version']}\nArchitecture: {arch}\nMaintainer: WebCodex\n"
        "Depends: libc6, libgcc-s1, libssl3, libgtk-3-0, libwebkit2gtk-4.1-0, libayatana-appindicator3-1, librsvg2-2\n"
        "Description: WebCodex Desktop and unified Server/Runner runtime\n"
        " Includes Desktop, CLI, Server and Runner. Installation does not start services.\n", encoding="utf-8")
    _copy_upgrade_candidate(input_root, control / "upgrade-candidate", manifest)
    preinst = control / "preinst"
    preinst.write_text(_upgrade_preinstall("upgrade-candidate", "/var/lib/webcodex-installer/transaction", "/var/lib/webcodex-installer/authorization.json", "/usr/lib/webcodex/webcodex-runtime", 'existing_install=0\nif [ "$(dpkg-query -W -f=\'${db:Status-Status}\' webcodex 2>/dev/null || true)" = installed ]; then existing_install=1; fi\nfor pattern in /usr/lib/webcodex/webcodex-desktop /usr/lib/webcodex/webcodex-runtime/webcodex /usr/lib/webcodex/webcodex-runtime/webcodex-server /usr/lib/webcodex/webcodex-runtime/webcodex-runner /usr/bin/webcodex /usr/bin/webcodex-server /usr/bin/webcodex-runner /usr/share/applications/webcodex.desktop /etc/systemd/system/webcodex* /lib/systemd/system/webcodex* /usr/lib/systemd/system/webcodex*; do\n  if [ -e "$pattern" ] || [ -L "$pattern" ]; then existing_install=1; fi\ndone', "/var/lib/webcodex-installer/recovery", "/var/lib/webcodex-installer/same-package.pending", artifacts["webcodex"]["sha256"], "/usr/bin/sha256sum", "/usr/lib/webcodex/webcodex-runtime/webcodex"), encoding="utf-8")
    preinst.chmod(0o755)
    postinst = control / "postinst"
    postinst.write_text(_upgrade_postinstall("/usr/lib/webcodex/webcodex-runtime/webcodex", "/var/lib/webcodex-installer/transaction", "/var/lib/webcodex-installer/authorization.json", "/var/lib/webcodex-installer/recovery", "/usr/lib/webcodex/webcodex-runtime", "/var/lib/webcodex-installer/same-package.pending"), encoding="utf-8")
    postinst.chmod(0o755)

def stage_macos(manifest: dict[str, Any], stage: Path, scripts_dir: Path, input_root: Path) -> None:
    artifacts = manifest["artifacts"]
    app = manifest["_desktop_payload"]
    shutil.copytree(app, stage / "Applications" / "WebCodex Desktop.app", symlinks=True)
    version_dir = stage / "Library/Application Support/WebCodex/runtime"
    for name in RUNTIMES:
        _copy_exec(manifest["_artifacts"][name], version_dir / name)
    bindir = stage / "usr/local/bin"
    bindir.mkdir(parents=True, exist_ok=True)
    for name in RUNTIMES:
        (bindir / name).symlink_to(f"/Library/Application Support/WebCodex/runtime/{name}")
    scripts_dir.mkdir()
    _copy_upgrade_candidate(input_root, scripts_dir / "upgrade-candidate", manifest)
    preinstall = scripts_dir / "preinstall"
    preinstall.write_text(_upgrade_preinstall("upgrade-candidate", "/Library/Application Support/WebCodex/installer-transaction", "/Library/Application Support/WebCodexInstaller/authorization.json", "/Library/Application Support/WebCodex/runtime", 'existing_install=0\nreceipt=\'dev.webcodex.unified-installer\'\nif /usr/sbin/pkgutil --pkg-info "$receipt" >/dev/null 2>&1; then existing_install=1; fi\nfor path in \'/Applications/WebCodex Desktop.app\' \'/Library/Application Support/WebCodex/runtime\' /usr/local/bin/webcodex /usr/local/bin/webcodex-server /usr/local/bin/webcodex-runner /Library/LaunchDaemons/org.webcodex.*.plist; do\n  if [ -e "$path" ] || [ -L "$path" ]; then existing_install=1; fi\ndone', "/Library/Application Support/WebCodexInstaller/recovery", "/Library/Application Support/WebCodexInstaller/same-package.pending", artifacts["webcodex"]["sha256"], "/usr/bin/shasum -a 256", "/Library/Application Support/WebCodex/runtime/webcodex"), encoding="utf-8")
    preinstall.chmod(0o755)
    postinstall = scripts_dir / "postinstall"
    postinstall.write_text(_upgrade_postinstall("/Library/Application Support/WebCodex/runtime/webcodex", "/Library/Application Support/WebCodex/installer-transaction", "/Library/Application Support/WebCodexInstaller/authorization.json", "/Library/Application Support/WebCodexInstaller/recovery", "/Library/Application Support/WebCodex/runtime", "/Library/Application Support/WebCodexInstaller/same-package.pending"), encoding="utf-8")
    postinstall.chmod(0o755)

def package(args: argparse.Namespace) -> dict[str, Any]:
    manifest = validate_manifest(args.manifest, args.checksums, args.input_root, args.platform)
    os_name = PLATFORMS[args.platform][0]
    expected_suffix = ".deb" if os_name == "linux" else ".pkg"
    if args.output.suffix.lower() != expected_suffix:
        raise PackageError(f"{args.platform} installer output must end in {expected_suffix}")
    if os_name == "linux" and platform.system() != "Linux":
        raise PackageError(".deb creation requires a Linux host")
    if os_name == "darwin" and platform.system() != "Darwin" and not args.dry_run:
        raise PackageError(".pkg creation requires a native macOS host")
    output = args.output.absolute()
    if output.exists() or output.is_symlink():
        raise PackageError("installer output already exists")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="webcodex-unified-installer-", dir=output.parent) as temp:
        temp_path = Path(temp)
        stage = temp_path / "root"
        stage.mkdir()
        if os_name == "linux":
            stage_linux(manifest, stage, args.input_root.resolve(strict=True))
            if args.dry_run:
                entries = sorted(path.relative_to(stage).as_posix() for path in stage.rglob("*"))
                return {"platform": args.platform, "output": str(output), "entries": entries, "package": "deb"}
            tool = shutil.which("dpkg-deb")
            if not tool:
                raise PackageError("dpkg-deb is required to create a .deb")
            result = subprocess.run([tool, "--build", "--root-owner-group", str(stage), str(output)],
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=120)
            if result.returncode:
                output.unlink(missing_ok=True)
                raise PackageError("dpkg-deb failed to build the installer")
        else:
            scripts_dir = temp_path / "pkg-scripts"
            stage_macos(manifest, stage, scripts_dir, args.input_root.resolve(strict=True))
            if args.dry_run:
                entries = sorted(path.relative_to(stage).as_posix() for path in stage.rglob("*"))
                return {"platform": args.platform, "output": str(output), "entries": entries,
                    "package": "pkg", "preinstall": (scripts_dir / "preinstall").read_text(encoding="utf-8")}
            tool = shutil.which("pkgbuild")
            if not tool:
                raise PackageError("pkgbuild is required to create a .pkg")
            result = subprocess.run([tool, "--root", str(stage), "--identifier", "dev.webcodex.unified-installer",
                "--version", manifest["version"], "--install-location", "/", "--scripts", str(scripts_dir), str(output)],
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=180)
            if result.returncode:
                output.unlink(missing_ok=True)
                raise PackageError("pkgbuild failed to build the installer")
    if not output.is_file() or output.stat().st_size == 0:
        output.unlink(missing_ok=True)
        raise PackageError("installer output was not created")
    return {"platform": args.platform, "output": str(output), "sha256": sha256_file(output),
        "package": "deb" if os_name == "linux" else "pkg"}

def make_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=tuple(PLATFORMS), required=True)
    parser.add_argument("--input-root", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--checksums", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true", help="validate inputs and render the install tree")
    return parser

def main() -> int:
    try:
        print(json.dumps(package(make_parser().parse_args()), sort_keys=True))
    except (PackageError, OSError, subprocess.SubprocessError) as exc:
        print(f"unified installer packaging failed: {exc}", file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
