from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts import workspace_boundary_check as boundary


def package_policy(
    layer: str,
    *,
    normal: tuple[str, ...] = (),
    dev: tuple[str, ...] = (),
    build: tuple[str, ...] = (),
    test_support: tuple[str, ...] = (),
) -> dict:
    return {
        "layer": layer,
        "role": f"fixture role for {layer}",
        "normal": sorted(normal),
        "dev": sorted(dev),
        "build": sorted(build),
        "test_support": sorted(test_support),
    }


def fixture_policy(
    packages: dict[str, dict],
    *,
    providers: tuple[str, ...] = (),
    forbidden_external: dict[str, list[str]] | None = None,
) -> boundary.BoundaryPolicy:
    return boundary.parse_policy(
        {
            "version": 1,
            "layers": {"leaf": 0, "application": 1},
            "root_test_support": {
                "feature": "root-test-support",
                "providers": sorted(providers),
            },
            "forbidden_external_dependencies": forbidden_external or {},
            "packages": packages,
        }
    )


def fixture_metadata(
    package_dependencies: dict[str, list[tuple[str, str, tuple[str, ...]]]],
    *,
    package_features: dict[str, tuple[str, ...]] | None = None,
) -> dict:
    package_features = package_features or {}
    packages = []
    members = []
    for name in sorted(package_dependencies):
        package_id = f"path+file:///fixture#{name}@0.4.0"
        dependencies = []
        for dependency_name, kind, features in package_dependencies[name]:
            dependencies.append(
                {
                    "name": dependency_name,
                    "kind": None if kind == "normal" else kind,
                    "features": list(features),
                }
            )
        packages.append(
            {
                "id": package_id,
                "name": name,
                "dependencies": dependencies,
                "features": {feature: [] for feature in package_features.get(name, ())},
            }
        )
        members.append(package_id)
    return {"packages": packages, "workspace_members": members}


def valid_policy_and_metadata() -> tuple[boundary.BoundaryPolicy, dict]:
    policy = fixture_policy(
        {
            "webcodex-app": package_policy(
                "application", normal=("webcodex-core",)
            ),
            "webcodex-core": package_policy("leaf"),
        }
    )
    metadata = fixture_metadata(
        {
            "webcodex-app": [("webcodex-core", "normal", ())],
            "webcodex-core": [],
        }
    )
    return policy, metadata


class MetadataBoundaryTests(unittest.TestCase):
    def test_valid_complete_workspace_passes(self) -> None:
        policy, metadata = valid_policy_and_metadata()
        self.assertEqual(boundary.check_metadata(metadata, policy), [])

    def test_unknown_new_workspace_member_is_reported(self) -> None:
        policy, metadata = valid_policy_and_metadata()
        new_id = "path+file:///fixture#webcodex-new@0.4.0"
        metadata["packages"].append(
            {"id": new_id, "name": "webcodex-new", "dependencies": [], "features": {}}
        )
        metadata["workspace_members"].append(new_id)
        self.assertEqual(
            boundary.check_metadata(metadata, policy),
            ["workspace package(s) missing from policy: webcodex-new"],
        )

    def test_policy_member_absent_from_workspace_is_reported(self) -> None:
        policy = fixture_policy(
            {
                "webcodex-app": package_policy("application"),
                "webcodex-ghost": package_policy("leaf"),
            }
        )
        metadata = fixture_metadata({"webcodex-app": []})
        self.assertEqual(
            boundary.check_metadata(metadata, policy),
            ["policy package(s) absent from workspace: webcodex-ghost"],
        )

    def test_forbidden_direct_workspace_dependency_is_reported(self) -> None:
        policy = fixture_policy(
            {
                "webcodex-app": package_policy(
                    "application", normal=("webcodex-core",)
                ),
                "webcodex-core": package_policy("leaf"),
                "webcodex-helper": package_policy("leaf"),
            }
        )
        metadata = fixture_metadata(
            {
                "webcodex-app": [
                    ("webcodex-core", "normal", ()),
                    ("webcodex-helper", "normal", ()),
                ],
                "webcodex-core": [],
                "webcodex-helper": [],
            }
        )
        self.assertEqual(
            boundary.check_metadata(metadata, policy),
            [
                "package webcodex-app has unlisted normal workspace dependency(ies): "
                "webcodex-helper"
            ],
        )

    def test_reverse_layer_dependency_is_reported_even_when_allowlisted(self) -> None:
        policy = fixture_policy(
            {
                "webcodex-app": package_policy("application"),
                "webcodex-core": package_policy("leaf", normal=("webcodex-app",)),
            }
        )
        metadata = fixture_metadata(
            {
                "webcodex-app": [],
                "webcodex-core": [("webcodex-app", "normal", ())],
            }
        )
        self.assertEqual(
            boundary.check_metadata(metadata, policy),
            [
                "reverse layer dependency: webcodex-core (leaf:0) normal-depends on "
                "webcodex-app (application:1)"
            ],
        )

    def test_allowed_dependency_passes(self) -> None:
        policy, metadata = valid_policy_and_metadata()
        self.assertEqual(boundary.check_metadata(metadata, policy), [])

    def test_explicit_dev_only_exception_can_cross_layers(self) -> None:
        policy = fixture_policy(
            {
                "webcodex-app": package_policy("application"),
                "webcodex-core": package_policy("leaf", dev=("webcodex-app",)),
            }
        )
        metadata = fixture_metadata(
            {
                "webcodex-app": [],
                "webcodex-core": [("webcodex-app", "dev", ())],
            }
        )
        self.assertEqual(boundary.check_metadata(metadata, policy), [])

    def test_root_test_support_is_allowed_only_as_explicit_dev_ownership(self) -> None:
        valid_policy = fixture_policy(
            {
                "webcodex-app": package_policy("application"),
                "webcodex-core": package_policy(
                    "leaf",
                    dev=("webcodex-app",),
                    test_support=("webcodex-app",),
                ),
            },
            providers=("webcodex-app",),
        )
        valid_metadata = fixture_metadata(
            {
                "webcodex-app": [],
                "webcodex-core": [
                    ("webcodex-app", "dev", ("root-test-support",))
                ],
            },
            package_features={"webcodex-app": ("root-test-support",)},
        )
        self.assertEqual(boundary.check_metadata(valid_metadata, valid_policy), [])

        production_policy = fixture_policy(
            {
                "webcodex-app": package_policy("application"),
                "webcodex-core": package_policy("leaf", normal=("webcodex-app",)),
            },
            providers=("webcodex-app",),
        )
        production_metadata = fixture_metadata(
            {
                "webcodex-app": [],
                "webcodex-core": [
                    ("webcodex-app", "normal", ("root-test-support",))
                ],
            },
            package_features={"webcodex-app": ("root-test-support",)},
        )
        violations = boundary.check_metadata(production_metadata, production_policy)
        self.assertIn(
            "package webcodex-core enables root-test-support on non-dev workspace "
            "dependency webcodex-app",
            violations,
        )
        self.assertIn(
            "reverse layer dependency: webcodex-core (leaf:0) normal-depends on "
            "webcodex-app (application:1)",
            violations,
        )

    def test_forbidden_external_dependency_guardrail_is_preserved(self) -> None:
        policy = fixture_policy(
            {"webcodex-core": package_policy("leaf")},
            forbidden_external={"webcodex-core": ["salvo"]},
        )
        metadata = fixture_metadata({"webcodex-core": [("salvo", "normal", ())]})
        self.assertEqual(
            boundary.check_metadata(metadata, policy),
            [
                "package webcodex-core directly depends on forbidden external "
                "package(s): salvo"
            ],
        )

    def test_diagnostics_are_deterministic(self) -> None:
        policy = fixture_policy(
            {
                "webcodex-app": package_policy(
                    "application", normal=("webcodex-core",)
                ),
                "webcodex-core": package_policy("leaf"),
                "webcodex-policy-only": package_policy("leaf"),
            }
        )
        metadata = fixture_metadata(
            {"webcodex-app": [], "webcodex-core": [], "webcodex-new": []}
        )
        first = boundary.check_metadata(metadata, policy)
        second = boundary.check_metadata(metadata, policy)
        self.assertEqual(first, second)
        self.assertEqual(
            first,
            [
                "workspace package(s) missing from policy: webcodex-new",
                "policy package(s) absent from workspace: webcodex-policy-only",
                "package webcodex-app policy lists absent normal workspace "
                "dependency(ies): webcodex-core",
            ],
        )


class PolicyParsingTests(unittest.TestCase):
    def test_duplicate_toml_key_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "workspace-boundaries.toml"
            path.write_text("version = 1\nversion = 1\n", encoding="utf-8")
            with self.assertRaisesRegex(boundary.PolicyError, "could not read"):
                boundary.load_policy(path)

    def test_invalid_unsorted_allowlist_is_rejected(self) -> None:
        with self.assertRaisesRegex(boundary.PolicyError, "must be sorted"):
            fixture_policy(
                {
                    "webcodex-app": {
                        "layer": "application",
                        "role": "fixture",
                        "normal": ["webcodex-z", "webcodex-a"],
                        "dev": [],
                        "build": [],
                        "test_support": [],
                    },
                    "webcodex-a": package_policy("leaf"),
                    "webcodex-z": package_policy("leaf"),
                }
            )

    def test_unknown_policy_dependency_is_rejected(self) -> None:
        with self.assertRaisesRegex(boundary.PolicyError, "unknown package"):
            fixture_policy(
                {
                    "webcodex-core": package_policy(
                        "leaf", normal=("webcodex-typo",)
                    )
                }
            )


class ParentSourcePathTests(unittest.TestCase):
    def test_both_cross_parent_path_spellings_are_reported_in_rust_sources(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "src").mkdir()
            (root / "tests").mkdir()
            (root / "src" / "lib.rs").write_text(
                '#[path = "../shared.rs"]\nmod shared;\n', encoding="utf-8"
            )
            (root / "tests" / "integration.rs").write_text(
                '#[path="../support.rs"]\nmod support;\n', encoding="utf-8"
            )
            violations = boundary.check_parent_source_paths(root)
            self.assertEqual(len(violations), 2)
            self.assertTrue(any("src/lib.rs:1" in item for item in violations))
            self.assertTrue(
                any("tests/integration.rs:1" in item for item in violations)
            )

    def test_generated_and_non_rust_files_are_excluded(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for excluded in (
                ".claude",
                "target",
                "generated",
                "dist",
                "node_modules",
            ):
                directory = root / excluded
                directory.mkdir()
                (directory / "fixture.rs").write_text(
                    '#[path = "../shared.rs"]\n', encoding="utf-8"
                )
            (root / "docs").mkdir()
            (root / "docs" / "historical.rs").write_text(
                '#[path = "../historical.rs"]\n', encoding="utf-8"
            )
            (root / "history.md").write_text(
                '#[path = "../historical.rs"]\n', encoding="utf-8"
            )
            self.assertEqual(boundary.check_parent_source_paths(root), [])


if __name__ == "__main__":
    unittest.main()
