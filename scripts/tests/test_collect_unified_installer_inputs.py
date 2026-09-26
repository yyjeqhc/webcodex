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

class NativeManifestCollectorTests(unittest.TestCase):
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
            "--workflow-run-id", "787654321",
            "--workflow-ref", "yyjeqhc/webcodex/.github/workflows/release-build.yml@refs/tags/v0.9.0",
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
            renamed = source.with_name("WebCodex.exe")
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
