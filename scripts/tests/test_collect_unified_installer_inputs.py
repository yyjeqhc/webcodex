from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SCRIPT = SCRIPTS / "collect_unified_installer_inputs.py"
SPEC = importlib.util.spec_from_file_location("collect_unified_installer_inputs", SCRIPT)
collector = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(collector)

SOURCE = "b" * 40
CONTRACT = {"min_generation": 1, "max_generation": 1}
CI_CONTEXT = {
    "GITHUB_RUN_ID": "787654321",
    "GITHUB_WORKFLOW_REF": "yyjeqhc/webcodex/.github/workflows/release-build.yml@refs/tags/v0.9.0",
    "GITHUB_SHA": SOURCE,
}

class NativeManifestCollectorTests(unittest.TestCase):
    def setUp(self):
        # Fixture provenance must not inherit the CI job running this test.
        self.enterContext(mock.patch.dict(os.environ, CI_CONTEXT))
        # These executable-script fixtures model a Linux native build job on
        # every test host; individual Windows cases override the target.
        self.enterContext(mock.patch.object(collector, "detect_platform", return_value="linux-x64"))

    def fixture(self, root: Path, *, dirty: bool = False, target: str = "x86_64-unknown-linux-gnu", architecture: str = "x86_64", suffix: str = "") -> dict[str, Path]:
        paths = {}
        (root / "bin").mkdir()
        for index, name in enumerate(collector.BINARIES):
            info = {
                "schema_version": 1, "binary": name, "version": "0.9.0",
                "git_commit": SOURCE, "git_dirty": dirty, "built_at": str(500 + index),
                "target": target, "architecture": architecture,
                "desktop_runtime_contract": CONTRACT, "environment_data_format": 1,
            }
            path = root / "bin" / f"{name}{suffix}"
            path.write_text("#!/usr/bin/env python3\nimport sys\nif sys.argv[1:] != ['--build-info-json']: raise SystemExit(2)\nprint(" + repr(json.dumps(info)) + ")\n", encoding="utf-8")
            path.chmod(0o755)
            paths[name] = path
        return paths

    def args(self, root: Path, paths: dict[str, Path], out: Path, *, platform: str = "linux-x64", desktop_executable: str = "webcodex-desktop"):
        relative = {name: path.relative_to(root).as_posix() for name, path in paths.items()}
        return collector.make_parser().parse_args([
            "--platform", platform, "--input-root", str(root),
            "--webcodex", relative["webcodex"],
            "--webcodex-server", relative["webcodex-server"],
            "--webcodex-runner", relative["webcodex-runner"],
            "--webcodex-desktop", relative["webcodex-desktop"],
            "--desktop-payload", relative["webcodex-desktop"],
            "--desktop-executable", desktop_executable,
            "--version", "0.9.0", "--source-sha", SOURCE,
            "--workflow-run-id", CI_CONTEXT["GITHUB_RUN_ID"],
            "--workflow-ref", CI_CONTEXT["GITHUB_WORKFLOW_REF"],
            "--output-dir", str(out),
        ])

    def test_collects_all_four_native_identities_and_feeds_package_validator(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = self.fixture(root)
            output = root / "installer-input"
            result = collector.collect(self.args(root, paths, output))
            manifest_path = Path(result["source_manifest"])
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(result["artifacts"], 4)
            self.assertEqual(manifest["source_workflow_run_id"], 787654321)
            self.assertTrue(manifest["source_workflow_ref"].endswith("refs/tags/v0.9.0"))
            self.assertEqual({item["probe"] for item in manifest["artifacts"].values()}, {"native-build-job"})
            self.assertEqual(len({item["build_info"]["built_at"] for item in manifest["artifacts"].values()}), 4)
            validated = collector.package.validate_manifest(
                manifest_path, output / "SHA256SUMS", output, "linux-x64"
            )
            self.assertEqual(validated["source_sha"], SOURCE)

    def test_local_preview_collector_never_invents_a_ci_run(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = self.fixture(root, target="x86_64-pc-windows-msvc", architecture="x86_64", suffix=".exe")
            infos = {}
            for name in collector.BINARIES:
                infos[name] = {
                    "schema_version": 1,
                    "binary": name,
                    "version": "0.9.0",
                    "git_commit": SOURCE,
                    "git_dirty": False,
                    "built_at": "123",
                    "target": "x86_64-pc-windows-msvc",
                    "architecture": "x86_64",
                    "desktop_runtime_contract": CONTRACT,
                    "environment_data_format": 1,
                }
            infos["webcodex"]["windows_guarded_bootstrap_contract"] = 1
            args = self.args(root, paths, root / "candidate", platform="win32-x64", desktop_executable="webcodex-desktop.exe")
            args.local_preview = True
            args.workflow_run_id = None
            args.workflow_ref = None
            with mock.patch.object(collector, "detect_platform", return_value="win32-x64"), mock.patch.object(collector, "probe", side_effect=lambda path, name: infos[name]), mock.patch.dict(os.environ, {}, clear=True):
                result = collector.collect(args)
            manifest = json.loads(Path(result["source_manifest"]).read_text(encoding="utf-8"))
            self.assertIs(manifest["local_preview"], True)
            self.assertEqual(manifest["source_workflow_run_id"], 0)
            self.assertEqual(manifest["source_workflow_ref"], "webcodex/local-preview")
            self.assertEqual({item["probe"] for item in manifest["artifacts"].values()}, {"local-preview-host"})
            self.assertFalse(any("refs/tags" in str(v) for v in manifest.values() if isinstance(v, str)))

    def test_unsigned_preview_accepts_only_its_exact_ci_workflow_and_source(self):
        # Pull-request GITHUB_SHA is a merge SHA; checkout and binary build
        # identity are bound to EXPECTED_SHA (the actual PR head) instead.
        context = {
            "GITHUB_ACTIONS": "true",
            "GITHUB_REPOSITORY": "yyjeqhc/webcodex",
            "GITHUB_RUN_ID": "42559605539",
            "GITHUB_WORKFLOW_REF": "yyjeqhc/webcodex/.github/workflows/windows-unified-preview.yml@refs/pull/1405/merge",
            "GITHUB_EVENT_NAME": "pull_request",
            "GITHUB_REF": "refs/pull/1405/merge",
            "GITHUB_HEAD_REF": "fix/1394-windows-v043-upgrade",
            "GITHUB_BASE_REF": "main",
            "GITHUB_SHA": "a" * 40,
            "EXPECTED_SHA": SOURCE,
        }
        with mock.patch.dict(os.environ, context, clear=True):
            collector.validate_local_preview_context(SOURCE)
            with self.subTest("head workflow ref"):
                os.environ["GITHUB_WORKFLOW_REF"] = (
                    "yyjeqhc/webcodex/.github/workflows/windows-unified-preview.yml"
                    "@refs/heads/fix/1394-windows-v043-upgrade"
                )
                collector.validate_local_preview_context(SOURCE)
        with self.subTest("workflow_dispatch on main"), mock.patch.dict(os.environ, {
            **context, "GITHUB_EVENT_NAME": "workflow_dispatch",
            "GITHUB_REF": "refs/heads/main", "GITHUB_SHA": SOURCE,
            "GITHUB_WORKFLOW_REF": "yyjeqhc/webcodex/.github/workflows/windows-unified-preview.yml@refs/heads/main",
        }, clear=True):
            collector.validate_local_preview_context(SOURCE)
        bad = {
            "GITHUB_ACTIONS": "false",
            "GITHUB_REPOSITORY": "other/repository",
            "GITHUB_RUN_ID": "not-a-number",
            "GITHUB_WORKFLOW_REF": CI_CONTEXT["GITHUB_WORKFLOW_REF"],
            "GITHUB_EVENT_NAME": "release",
            "GITHUB_REF": "refs/tags/v0.9.0",
            "GITHUB_HEAD_REF": "unknown-feature",
            "GITHUB_BASE_REF": "other-base",
            "GITHUB_SHA": "not-a-sha",
            "EXPECTED_SHA": "a" * 40,
        }
        for key, replacement in bad.items():
            with self.subTest(key=key), mock.patch.dict(os.environ, {**context, key: replacement}, clear=True):
                with self.assertRaises(collector.CollectionError):
                    collector.validate_local_preview_context(SOURCE)
        for unbound in ("GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_WORKFLOW_REF"):
            reduced = context.copy()
            del reduced[unbound]
            with self.subTest(missing=unbound), mock.patch.dict(os.environ, reduced, clear=True):
                with self.assertRaises(collector.CollectionError):
                    collector.validate_local_preview_context(SOURCE)
        with self.subTest("local desktop build"), mock.patch.dict(os.environ, {}, clear=True):
            collector.validate_local_preview_context(SOURCE)
        with self.subTest("forge local context"), mock.patch.dict(os.environ, {
            "GITHUB_ACTIONS": "true"
        }, clear=True):
            with self.assertRaises(collector.CollectionError):
                collector.validate_local_preview_context(SOURCE)

    def test_rejects_mismatched_ci_context_before_probing_or_publishing(self):
        for variable in CI_CONTEXT:
            with self.subTest(variable=variable), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                paths = self.fixture(root)
                output = root / "installer-input"
                with mock.patch.dict(os.environ, {variable: "mismatched-context"}), mock.patch.object(collector, "probe") as probe:
                    with self.assertRaisesRegex(collector.CollectionError, "does not match GitHub Actions context"):
                        collector.collect(self.args(root, paths, output))
                probe.assert_not_called()
                self.assertFalse(output.exists())

    def test_rejects_dirty_build_from_actual_binary_probe(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = self.fixture(root, dirty=True)
            with self.assertRaisesRegex(collector.CollectionError, "version/source/clean"):
                collector.collect(self.args(root, paths, root / "installer-input"))

    def test_native_collector_refuses_foreign_target(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = self.fixture(root)
            args = self.args(root, paths, root / "installer-input")
            args.platform = "linux-arm64"
            with self.assertRaisesRegex(collector.CollectionError, "exact native platform"):
                collector.collect(args)

    def test_windows_collection_preserves_exe_paths_for_core_preflight(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = self.fixture(root, target="aarch64-pc-windows-msvc", architecture="aarch64", suffix=".exe")
            output = root / "installer-input"
            args = self.args(root, paths, output, platform="win32-arm64", desktop_executable="webcodex-desktop.exe")
            with mock.patch.object(collector, "detect_platform", return_value="win32-arm64"):
                result = collector.collect(args)
            manifest = json.loads(Path(result["source_manifest"]).read_text(encoding="utf-8"))
            self.assertEqual(manifest["platform"], "win32-arm64")
            self.assertEqual(manifest["artifacts"]["webcodex"]["path"], "artifacts/bin/webcodex.exe")
            self.assertTrue((output / "artifacts/bin/webcodex-desktop.exe").is_file())
            managed = manifest["desktop_payload"]["managed_files"]
            self.assertEqual([item["path"] for item in managed], sorted(collector.WINDOWS_MANAGED_INSTALL_FILES))
            self.assertEqual(
                {item["path"]: item["sha256"] for item in managed},
                {
                    path: manifest["artifacts"][name]["sha256"]
                    for path, name in collector.WINDOWS_MANAGED_INSTALL_FILES.items()
                },
            )

    def test_windows_desktop_executable_records_staged_name_not_source_name(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = self.fixture(root, target="x86_64-pc-windows-msvc", architecture="x86_64", suffix=".exe")
            source = paths["webcodex-desktop"]
            # Desktop and CLI are separate build outputs. On case-insensitive
            # macOS/Windows volumes WebCodex.exe aliases bin/webcodex.exe.
            desktop_dir = root / "desktop-output"
            desktop_dir.mkdir()
            renamed = desktop_dir / "WebCodex.exe"
            source.rename(renamed)
            paths["webcodex-desktop"] = renamed
            output = root / "installer-input"
            args = self.args(root, paths, output, platform="win32-x64", desktop_executable="WebCodex.exe")
            with mock.patch.object(collector, "detect_platform", return_value="win32-x64"):
                result = collector.collect(args)
            manifest = json.loads(Path(result["source_manifest"]).read_text(encoding="utf-8"))
            self.assertEqual(manifest["desktop_payload"]["path"], "artifacts/bin/webcodex-desktop.exe")
            self.assertEqual(manifest["desktop_payload"]["executable"], "webcodex-desktop.exe")

if __name__ == "__main__":
    unittest.main()
