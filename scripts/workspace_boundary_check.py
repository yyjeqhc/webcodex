#!/usr/bin/env python3
"""Validate the checked-in WebCodex Cargo workspace dependency policy."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


POLICY_FILENAME = "workspace-boundaries.toml"
POLICY_VERSION = 1
DEPENDENCY_KINDS = ("normal", "dev", "build")
PACKAGE_POLICY_KEYS = frozenset(
    {"layer", "role", "normal", "dev", "build", "test_support"}
)
TOP_LEVEL_POLICY_KEYS = frozenset(
    {
        "version",
        "layers",
        "root_test_support",
        "forbidden_external_dependencies",
        "packages",
    }
)

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


class PolicyError(ValueError):
    """Raised when the checked-in boundary policy is malformed or ambiguous."""


@dataclass(frozen=True)
class PackagePolicy:
    layer: str
    role: str
    normal: frozenset[str]
    dev: frozenset[str]
    build: frozenset[str]
    test_support: frozenset[str]

    def dependencies_for(self, kind: str) -> frozenset[str]:
        return getattr(self, kind)


@dataclass(frozen=True)
class BoundaryPolicy:
    layers: dict[str, int]
    root_test_support_feature: str
    root_test_support_providers: frozenset[str]
    forbidden_external_dependencies: dict[str, frozenset[str]]
    packages: dict[str, PackagePolicy]


def _require_table(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise PolicyError(f"{label} must be a TOML table")
    return value


def _require_sorted_unique_strings(value: Any, label: str) -> tuple[str, ...]:
    if not isinstance(value, list) or any(
        not isinstance(item, str) or not item for item in value
    ):
        raise PolicyError(f"{label} must be an array of non-empty strings")
    if len(value) != len(set(value)):
        raise PolicyError(f"{label} contains duplicate values")
    if value != sorted(value):
        raise PolicyError(f"{label} must be sorted")
    return tuple(value)


def parse_policy(raw: dict[str, Any]) -> BoundaryPolicy:
    """Validate and normalize one decoded workspace-boundary policy."""
    unknown_top_level = sorted(set(raw).difference(TOP_LEVEL_POLICY_KEYS))
    missing_top_level = sorted(TOP_LEVEL_POLICY_KEYS.difference(raw))
    if unknown_top_level:
        raise PolicyError(
            "unknown top-level policy key(s): " + ", ".join(unknown_top_level)
        )
    if missing_top_level:
        raise PolicyError(
            "missing top-level policy key(s): " + ", ".join(missing_top_level)
        )
    if raw.get("version") != POLICY_VERSION:
        raise PolicyError(
            f"policy version must be {POLICY_VERSION}, got {raw.get('version')!r}"
        )

    raw_layers = _require_table(raw["layers"], "layers")
    if not raw_layers:
        raise PolicyError("layers must not be empty")
    layers: dict[str, int] = {}
    for name in sorted(raw_layers):
        rank = raw_layers[name]
        if (
            not isinstance(name, str)
            or not name
            or isinstance(rank, bool)
            or not isinstance(rank, int)
            or rank < 0
        ):
            raise PolicyError(
                "layers must map non-empty names to non-negative integer ranks"
            )
        layers[name] = rank
    ranks = sorted(layers.values())
    if ranks != list(range(len(layers))):
        raise PolicyError("layer ranks must be unique and contiguous from 0")

    raw_root_support = _require_table(
        raw["root_test_support"], "root_test_support"
    )
    if set(raw_root_support) != {"feature", "providers"}:
        raise PolicyError("root_test_support must contain exactly feature and providers")
    feature = raw_root_support["feature"]
    if not isinstance(feature, str) or not feature:
        raise PolicyError("root_test_support.feature must be a non-empty string")
    providers = frozenset(
        _require_sorted_unique_strings(
            raw_root_support["providers"], "root_test_support.providers"
        )
    )

    raw_packages = _require_table(raw["packages"], "packages")
    if not raw_packages:
        raise PolicyError("packages must not be empty")
    packages: dict[str, PackagePolicy] = {}
    for package_name in sorted(raw_packages):
        if not isinstance(package_name, str) or not package_name:
            raise PolicyError("package names must be non-empty strings")
        entry = _require_table(raw_packages[package_name], f"packages.{package_name}")
        unknown_keys = sorted(set(entry).difference(PACKAGE_POLICY_KEYS))
        missing_keys = sorted(PACKAGE_POLICY_KEYS.difference(entry))
        if unknown_keys:
            raise PolicyError(
                f"packages.{package_name} has unknown key(s): "
                + ", ".join(unknown_keys)
            )
        if missing_keys:
            raise PolicyError(
                f"packages.{package_name} is missing key(s): "
                + ", ".join(missing_keys)
            )
        layer = entry["layer"]
        role = entry["role"]
        if not isinstance(layer, str) or layer not in layers:
            raise PolicyError(
                f"packages.{package_name}.layer must name a declared layer"
            )
        if not isinstance(role, str) or not role.strip():
            raise PolicyError(
                f"packages.{package_name}.role must be a non-empty string"
            )
        normal = frozenset(
            _require_sorted_unique_strings(
                entry["normal"], f"packages.{package_name}.normal"
            )
        )
        dev = frozenset(
            _require_sorted_unique_strings(entry["dev"], f"packages.{package_name}.dev")
        )
        build = frozenset(
            _require_sorted_unique_strings(
                entry["build"], f"packages.{package_name}.build"
            )
        )
        test_support = frozenset(
            _require_sorted_unique_strings(
                entry["test_support"], f"packages.{package_name}.test_support"
            )
        )
        if not test_support.issubset(dev):
            invalid = sorted(test_support.difference(dev))
            raise PolicyError(
                f"packages.{package_name}.test_support must be a subset of dev: "
                + ", ".join(invalid)
            )
        packages[package_name] = PackagePolicy(
            layer=layer,
            role=role,
            normal=normal,
            dev=dev,
            build=build,
            test_support=test_support,
        )

    package_names = frozenset(packages)
    unknown_providers = sorted(providers.difference(package_names))
    if unknown_providers:
        raise PolicyError(
            "root_test_support.providers references unknown package(s): "
            + ", ".join(unknown_providers)
        )
    for package_name, package_policy in packages.items():
        for kind in DEPENDENCY_KINDS:
            dependencies = package_policy.dependencies_for(kind)
            unknown = sorted(dependencies.difference(package_names))
            if unknown:
                raise PolicyError(
                    f"packages.{package_name}.{kind} references unknown package(s): "
                    + ", ".join(unknown)
                )
            if package_name in dependencies:
                raise PolicyError(
                    f"packages.{package_name}.{kind} must not depend on itself"
                )
        unsupported = sorted(package_policy.test_support.difference(providers))
        if unsupported:
            raise PolicyError(
                f"packages.{package_name}.test_support references package(s) that do "
                "not provide root test support: "
                + ", ".join(unsupported)
            )

    raw_forbidden = _require_table(
        raw["forbidden_external_dependencies"],
        "forbidden_external_dependencies",
    )
    forbidden_external: dict[str, frozenset[str]] = {}
    for package_name in sorted(raw_forbidden):
        if package_name not in packages:
            raise PolicyError(
                "forbidden_external_dependencies references unknown package "
                f"{package_name!r}"
            )
        dependencies = frozenset(
            _require_sorted_unique_strings(
                raw_forbidden[package_name],
                f"forbidden_external_dependencies.{package_name}",
            )
        )
        workspace_names = sorted(dependencies.intersection(package_names))
        if workspace_names:
            raise PolicyError(
                f"forbidden_external_dependencies.{package_name} must contain only "
                "non-workspace packages: "
                + ", ".join(workspace_names)
            )
        forbidden_external[package_name] = dependencies

    return BoundaryPolicy(
        layers=layers,
        root_test_support_feature=feature,
        root_test_support_providers=providers,
        forbidden_external_dependencies=forbidden_external,
        packages=packages,
    )


def load_policy(path: Path) -> BoundaryPolicy:
    """Read and validate one TOML policy file."""
    try:
        raw = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise PolicyError(f"could not read boundary policy {path}: {error}") from error
    return parse_policy(raw)


def _dependency_kind(dependency: dict[str, Any]) -> str:
    kind = dependency.get("kind")
    return "normal" if kind is None else str(kind)


def check_metadata(metadata: dict[str, Any], policy: BoundaryPolicy) -> list[str]:
    """Return deterministic workspace DAG and policy violations."""
    violations: list[str] = []
    packages_by_id = {
        package["id"]: package
        for package in metadata.get("packages", [])
        if isinstance(package, dict) and isinstance(package.get("id"), str)
    }
    workspace_packages: list[dict[str, Any]] = []
    for member_id in metadata.get("workspace_members", []):
        package = packages_by_id.get(member_id)
        if package is None:
            violations.append(
                f"workspace member {member_id!r} is absent from cargo metadata packages"
            )
            continue
        workspace_packages.append(package)

    packages_by_name: dict[str, dict[str, Any]] = {}
    duplicate_names: set[str] = set()
    for package in workspace_packages:
        name = package.get("name")
        if not isinstance(name, str):
            violations.append(
                f"workspace package {package.get('id')!r} has no valid package name"
            )
            continue
        if name in packages_by_name:
            duplicate_names.add(name)
        packages_by_name[name] = package
    if duplicate_names:
        violations.append(
            "workspace contains duplicate package name(s): "
            + ", ".join(sorted(duplicate_names))
        )

    workspace_names = frozenset(packages_by_name)
    policy_names = frozenset(policy.packages)
    missing_policy = sorted(workspace_names.difference(policy_names))
    unknown_policy = sorted(policy_names.difference(workspace_names))
    if missing_policy:
        violations.append(
            "workspace package(s) missing from policy: " + ", ".join(missing_policy)
        )
    if unknown_policy:
        violations.append(
            "policy package(s) absent from workspace: " + ", ".join(unknown_policy)
        )

    feature = policy.root_test_support_feature
    actual_providers = {
        name
        for name, package in packages_by_name.items()
        if isinstance(package.get("features"), dict)
        and feature in package.get("features", {})
    }
    unregistered_providers = sorted(
        actual_providers.difference(policy.root_test_support_providers)
    )
    stale_providers = sorted(
        policy.root_test_support_providers.difference(actual_providers)
    )
    if unregistered_providers:
        violations.append(
            f"workspace package(s) expose {feature} but are not policy providers: "
            + ", ".join(unregistered_providers)
        )
    if stale_providers:
        violations.append(
            f"policy {feature} provider(s) do not expose that feature: "
            + ", ".join(stale_providers)
        )

    for package_name in sorted(workspace_names.intersection(policy_names)):
        package = packages_by_name[package_name]
        package_policy = policy.packages[package_name]
        actual_by_kind = {kind: set() for kind in DEPENDENCY_KINDS}
        actual_test_support: set[str] = set()
        all_direct_dependency_names: set[str] = set()

        dependencies = package.get("dependencies", [])
        if not isinstance(dependencies, list):
            violations.append(f"package {package_name} has invalid dependency metadata")
            continue
        for dependency in dependencies:
            if not isinstance(dependency, dict):
                violations.append(
                    f"package {package_name} has invalid dependency metadata entry"
                )
                continue
            dependency_name = dependency.get("name")
            if not isinstance(dependency_name, str):
                violations.append(
                    f"package {package_name} has dependency without a valid name"
                )
                continue
            all_direct_dependency_names.add(dependency_name)
            if dependency_name not in workspace_names:
                continue
            kind = _dependency_kind(dependency)
            if kind not in actual_by_kind:
                violations.append(
                    f"package {package_name} has unsupported workspace dependency kind "
                    f"{kind!r} for {dependency_name}"
                )
                continue
            actual_by_kind[kind].add(dependency_name)
            dependency_features = dependency.get("features", [])
            if not isinstance(dependency_features, list):
                violations.append(
                    f"package {package_name} dependency {dependency_name} has invalid "
                    "feature metadata"
                )
                continue
            if feature in dependency_features:
                if kind != "dev":
                    violations.append(
                        f"package {package_name} enables {feature} on non-dev workspace "
                        f"dependency {dependency_name}"
                    )
                else:
                    actual_test_support.add(dependency_name)
                if dependency_name not in policy.root_test_support_providers:
                    violations.append(
                        f"package {package_name} enables {feature} on non-provider "
                        f"{dependency_name}"
                    )

        for kind in DEPENDENCY_KINDS:
            expected = package_policy.dependencies_for(kind)
            actual = actual_by_kind[kind]
            unlisted = sorted(actual.difference(expected))
            stale = sorted(expected.difference(actual))
            if unlisted:
                violations.append(
                    f"package {package_name} has unlisted {kind} workspace "
                    "dependency(ies): "
                    + ", ".join(unlisted)
                )
            if stale:
                violations.append(
                    f"package {package_name} policy lists absent {kind} workspace "
                    "dependency(ies): "
                    + ", ".join(stale)
                )

        unlisted_test_support = sorted(
            actual_test_support.difference(package_policy.test_support)
        )
        stale_test_support = sorted(
            package_policy.test_support.difference(actual_test_support)
        )
        if unlisted_test_support:
            violations.append(
                f"package {package_name} has unlisted {feature} dev dependency(ies): "
                + ", ".join(unlisted_test_support)
            )
        if stale_test_support:
            violations.append(
                f"package {package_name} policy lists absent {feature} dev "
                "dependency(ies): "
                + ", ".join(stale_test_support)
            )

        source_rank = policy.layers[package_policy.layer]
        for kind in ("normal", "build"):
            for dependency_name in sorted(actual_by_kind[kind]):
                target_policy = policy.packages.get(dependency_name)
                if target_policy is None:
                    continue
                target_rank = policy.layers[target_policy.layer]
                if target_rank > source_rank:
                    violations.append(
                        "reverse layer dependency: "
                        f"{package_name} ({package_policy.layer}:{source_rank}) "
                        f"{kind}-depends on {dependency_name} "
                        f"({target_policy.layer}:{target_rank})"
                    )

        forbidden_external = policy.forbidden_external_dependencies.get(
            package_name, frozenset()
        )
        forbidden_present = sorted(
            forbidden_external.intersection(all_direct_dependency_names)
        )
        if forbidden_present:
            violations.append(
                f"package {package_name} directly depends on forbidden external "
                "package(s): "
                + ", ".join(forbidden_present)
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


def evaluate(
    root: Path, metadata: dict[str, Any], policy: BoundaryPolicy
) -> list[str]:
    root = root.resolve()
    return check_metadata(metadata, policy) + check_parent_source_paths(root)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--metadata", required=True, type=Path)
    parser.add_argument("--policy", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    policy_path = args.policy or args.root / POLICY_FILENAME
    try:
        policy = load_policy(policy_path)
    except PolicyError as error:
        print(f"[workspace-boundary][FAIL] {error}", file=sys.stderr)
        return 2
    try:
        metadata = json.loads(args.metadata.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        print(
            f"[workspace-boundary][FAIL] could not read cargo metadata: {error}",
            file=sys.stderr,
        )
        return 2

    violations = evaluate(args.root, metadata, policy)
    if violations:
        for violation in violations:
            print(f"[workspace-boundary][FAIL] {violation}", file=sys.stderr)
        print(
            f"[workspace-boundary][FAIL] {len(violations)} violation(s)",
            file=sys.stderr,
        )
        return 1

    print(
        f"[workspace-boundary][ok] all {len(policy.packages)} workspace packages "
        "match the checked-in dependency policy"
    )
    print(
        "[workspace-boundary][ok] production workspace dependencies respect layer "
        "direction; dev/test exceptions are explicit"
    )
    print(
        f"[workspace-boundary][ok] {policy.root_test_support_feature} is confined "
        "to declared dev/test ownership"
    )
    print("[workspace-boundary][ok] no cross-parent #[path] source sharing")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
