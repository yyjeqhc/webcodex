#!/usr/bin/env python3
"""Validate stable WebCodex Cargo workspace boundaries."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any


REQUIRED_PACKAGES = frozenset(
    {
        "webcodex",
        "webcodex-admin",
        "webcodex-runner-config",
        "webcodex-cli",
        "webcodex-core",
        "webcodex-runner",
        "webcodex-workspace",
    }
)

FORBIDDEN_DIRECT_DEPENDENCIES = {
    "webcodex-core": frozenset({"salvo", "rusqlite", "quinn"}),
    "webcodex-runner": frozenset({"salvo", "rusqlite"}),
    "webcodex-cli": frozenset(
        {
            "salvo",
            "rusqlite",
            "quinn",
            "webcodex-runner",
            "webcodex-workspace",
        }
    ),
}

EXCLUDED_DIRECTORIES = frozenset(
    {
        ".git",
        ".claude",
        "docs",
        "target",
        "generated",
        "gen",
        "dist",
        "node_modules",
        "coverage",
    }
)

PARENT_SOURCE_PATH = re.compile(r'#\s*\[\s*path\s*=\s*"\.\./')


def check_metadata(metadata: dict[str, Any]) -> list[str]:
    """Return workspace membership and direct-dependency violations."""
    violations: list[str] = []
    packages_by_id = {
        package["id"]: package
        for package in metadata.get("packages", [])
        if isinstance(package, dict) and "id" in package
    }
    workspace_packages = []
    for member_id in metadata.get("workspace_members", []):
        package = packages_by_id.get(member_id)
        if package is None:
            violations.append(
                f"workspace member {member_id!r} is absent from cargo metadata packages"
            )
            continue
        workspace_packages.append(package)

    packages_by_name = {
        package.get("name"): package
        for package in workspace_packages
        if isinstance(package.get("name"), str)
    }
    missing = sorted(REQUIRED_PACKAGES.difference(packages_by_name))
    if missing:
        violations.append(
            "workspace is missing required package(s): " + ", ".join(missing)
        )

    for package_name, forbidden in FORBIDDEN_DIRECT_DEPENDENCIES.items():
        package = packages_by_name.get(package_name)
        if package is None:
            continue
        direct_dependencies = {
            dependency.get("name")
            for dependency in package.get("dependencies", [])
            if isinstance(dependency, dict)
            and isinstance(dependency.get("name"), str)
        }
        illegal = sorted(forbidden.intersection(direct_dependencies))
        if illegal:
            violations.append(
                f"package {package_name} directly depends on forbidden package(s): "
                + ", ".join(illegal)
            )

    return violations


def check_parent_source_paths(root: Path) -> list[str]:
    """Return Rust source locations that share source across a parent directory."""
    violations: list[str] = []
    for current, directory_names, file_names in os.walk(root):
        directory_names[:] = sorted(
            name for name in directory_names if name not in EXCLUDED_DIRECTORIES
        )
        current_path = Path(current)
        for file_name in sorted(file_names):
            if not file_name.endswith(".rs"):
                continue
            path = current_path / file_name
            try:
                lines = path.read_text(encoding="utf-8").splitlines()
            except (OSError, UnicodeError) as error:
                violations.append(f"could not inspect Rust source {path}: {error}")
                continue
            relative_path = path.relative_to(root).as_posix()
            for line_number, line in enumerate(lines, start=1):
                if PARENT_SOURCE_PATH.search(line):
                    violations.append(
                        f"{relative_path}:{line_number}: cross-parent #[path] source sharing"
                    )
    return violations


def evaluate(root: Path, metadata: dict[str, Any]) -> list[str]:
    root = root.resolve()
    return check_metadata(metadata) + check_parent_source_paths(root)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--metadata", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        metadata = json.loads(args.metadata.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        print(
            f"[workspace-boundary][FAIL] could not read cargo metadata: {error}",
            file=sys.stderr,
        )
        return 2

    violations = evaluate(args.root, metadata)
    if violations:
        for violation in violations:
            print(f"[workspace-boundary][FAIL] {violation}", file=sys.stderr)
        print(
            f"[workspace-boundary][FAIL] {len(violations)} violation(s)",
            file=sys.stderr,
        )
        return 1

    print("[workspace-boundary][ok] all 8 required workspace packages are present")
    print(
        "[workspace-boundary][ok] core, runner, and CLI direct dependencies "
        "respect their boundaries"
    )
    print("[workspace-boundary][ok] no cross-parent #[path] source sharing")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
