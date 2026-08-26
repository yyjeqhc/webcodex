#!/usr/bin/env python3
"""Read-only release-day control-plane diagnostics."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

if __package__:
    from . import collect_release_bundle as collector
    from . import release_publication as publication
    from . import release_readiness as readiness
else:
    import sys

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import collect_release_bundle as collector
    import release_publication as publication
    import release_readiness as readiness


EXPECTED_PLATFORMS = (
    "linux-x64",
    "linux-arm64",
    "darwin-x64",
    "darwin-arm64",
    "win32-x64",
    "win32-arm64",
)
REQUIRED_TOOLS = ("git", "gh", "npm", "node", "python3", "bash")


class DoctorError(RuntimeError):
    pass


def _record(checks: list[dict], name: str, fn) -> None:
    try:
        detail = fn()
    except Exception as exc:  # bounded diagnostic aggregator; individual checks stay explicit
        checks.append({"name": name, "status": "failed", "detail": str(exc)})
    else:
        checks.append({"name": name, "status": "passed", "detail": detail})


def _require_tools() -> str:
    missing = [name for name in REQUIRED_TOOLS if shutil.which(name) is None]
    if missing:
        raise DoctorError(f"missing required release tools: {', '.join(missing)}")
    return ", ".join(REQUIRED_TOOLS)


def _platform_contract(root: Path) -> str:
    if tuple(collector.PLATFORMS) != EXPECTED_PLATFORMS:
        raise DoctorError(
            f"collector platform contract drift: expected={EXPECTED_PLATFORMS} actual={tuple(collector.PLATFORMS)}"
        )
    manifest_path = root / "npm/webcodex/manifest.example.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DoctorError("npm manifest.example.json is unreadable") from exc
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, dict) or tuple(artifacts) != EXPECTED_PLATFORMS:
        raise DoctorError(
            f"npm manifest platform contract drift: expected={EXPECTED_PLATFORMS} actual={tuple(artifacts) if isinstance(artifacts, dict) else None}"
        )
    return f"six-platform contract: {', '.join(EXPECTED_PLATFORMS)}"


def _workflow_contract(root: Path) -> str:
    ci = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    readiness_workflow = (root / ".github/workflows/release-readiness.yml").read_text(encoding="utf-8")
    build = (root / ".github/workflows/release-build.yml").read_text(encoding="utf-8")
    required = {
        "ci.yml": (
            ("macos-15-intel", ci),
            ("windows-11-arm", ci),
            ("ubuntu-24.04-arm", ci),
        ),
        "release-readiness.yml": (
            ("ci_run_id", readiness_workflow),
            ("linux/amd64", readiness_workflow),
            ("linux/arm64", readiness_workflow),
        ),
        "release-build.yml": tuple((platform, build) for platform in EXPECTED_PLATFORMS)
        + (("macos-15-intel", build), ("windows-11-arm", build), ("ubuntu-24.04-arm", build)),
    }
    missing = []
    for filename, tokens in required.items():
        for token, body in tokens:
            if token not in body:
                missing.append(f"{filename}:{token}")
    if missing:
        raise DoctorError(f"release workflow contract is missing: {', '.join(missing)}")
    if "packages: write" in readiness_workflow or "actions/upload-artifact" in readiness_workflow:
        raise DoctorError("release-readiness gained publication/upload authority")
    return "CI, readiness, and authoritative build workflow contracts are consistent"


def _compile_verifiers(root: Path) -> str:
    for relative in (
        "scripts/verify_public_release.py",
        "scripts/prepare_server_deployment_assets.py",
        "scripts/release_publication.py",
        "scripts/release_readiness.py",
    ):
        source = (root / relative).read_text(encoding="utf-8")
        compile(source, relative, "exec")
    return "public verifier, deployment metadata, publication, and readiness Python parse cleanly"


def _actionlint(root: Path) -> str:
    executable = shutil.which("actionlint")
    if executable is None:
        fallback = Path.home() / "go/bin/actionlint"
        executable = str(fallback) if fallback.is_file() and os.access(fallback, os.X_OK) else None
    if executable is None:
        return "optional: actionlint not installed"
    result = subprocess.run(
        [
            executable,
            str(root / ".github/workflows/ci.yml"),
            str(root / ".github/workflows/release-readiness.yml"),
            str(root / ".github/workflows/release-build.yml"),
            str(root / ".github/workflows/release-image.yml"),
        ],
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=60,
        check=False,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        raise DoctorError(f"actionlint failed: {detail[:2000]}")
    return "actionlint passed"


def run_doctor(
    *,
    repo: str,
    version: str,
    source_sha: str,
    root: Path,
    timeout: float,
) -> dict:
    source_root = root.absolute()
    source = collector.normalize_source_sha(source_sha)
    release_version = publication.normalize_version(version)
    checks: list[dict] = []

    _record(checks, "required-tools", _require_tools)
    _record(checks, "six-platform-contract", lambda: _platform_contract(source_root))
    _record(checks, "workflow-contract", lambda: _workflow_contract(source_root))
    _record(checks, "python-verifiers", lambda: _compile_verifiers(source_root))
    _record(checks, "actionlint", lambda: _actionlint(source_root))

    preflight_result: dict | None = None

    def preflight_check() -> str:
        nonlocal preflight_result
        preflight_result = publication.preflight_release(
            repo=repo,
            version=release_version,
            source_sha=source,
            root=source_root,
            timeout=timeout,
        )
        return "release source/version/namespaces and GitHub/npm publication identities passed"

    _record(checks, "publication-preflight", preflight_check)

    ci_result: dict | None = None

    def ci_check() -> str:
        nonlocal ci_result
        client = collector.GitHubClient(repo, collector.resolve_github_token(), timeout)
        ci_result = readiness._successful_main_ci_run(client, source)
        return f"exact-main CI run {ci_result['id']} attempt {ci_result['run_attempt']} is successful"

    _record(checks, "exact-main-ci", ci_check)

    def release_list_check() -> str:
        client = collector.GitHubClient(repo, collector.resolve_github_token(), timeout)
        payload = publication._github_json_array(client, "/releases?per_page=1&page=1")
        return "authenticated bounded GitHub Release listing is readable (draft-capable lookup path)"

    _record(checks, "github-release-listing", release_list_check)

    failures = [check for check in checks if check["status"] != "passed"]
    return {
        "status": "passed" if not failures else "failed",
        "repo": repo,
        "version": release_version,
        "source_sha": source,
        "checks": checks,
        "failed_checks": [check["name"] for check in failures],
        "preflight": preflight_result,
        "main_ci": (
            {"run_id": ci_result["id"], "run_attempt": ci_result["run_attempt"], "url": ci_result["html_url"]}
            if ci_result is not None
            else None
        ),
        "mutations_performed": False,
    }
