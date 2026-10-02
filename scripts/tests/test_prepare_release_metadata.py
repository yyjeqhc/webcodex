from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from scripts import prepare_release_metadata as metadata


class PrepareReleaseMetadataInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.artifacts = self.root / "artifacts"
        self.artifacts.mkdir()
        for platform in metadata.PLATFORMS:
            import tarfile
            archive_path = self.artifacts / metadata.archive_filename("0.3.0", platform)
            with tarfile.open(archive_path, "w:gz") as archive:
                for name in metadata.expected_members(platform):
                    info = tarfile.TarInfo(name)
                    payload = b"fixture"
                    info.size = len(payload)
                    archive.addfile(info, __import__("io").BytesIO(payload))
        for platform in metadata.desktop_platforms_for_version("0.3.0"):
            (self.artifacts / metadata.desktop_filename("0.3.0", platform)).write_bytes(b"desktop fixture")
        for platform in metadata.PLATFORMS:
            source_sha = "a" * 40
            records = {
                name: {"build_info": {
                    "schema_version": 1, "binary": name, "version": "0.3.0",
                    "git_commit": source_sha, "git_dirty": False, "built_at": "123",
                    "target": "fixture", "architecture": platform.split("-")[1],
                    "desktop_runtime_contract": {}, "environment_data_format": 1,
                }}
                for name in ("webcodex", "webcodex-server", "webcodex-runner", "webcodex-desktop")
            }
            value = {"schema_version": 1, "version": "0.3.0", "source_sha": source_sha, "source_workflow_run_id": 123, "source_workflow_ref": "repo/.github/workflows/release-build.yml@refs/tags/v0.3.0", "platform": platform, "target": "fixture", "architecture": platform.split("-")[1], "desktop_runtime_contract": {}, "artifacts": records, "desktop_payload": {}}
            (self.artifacts / metadata.source_manifest_filename("0.3.0", platform)).write_text(json.dumps(value), encoding="utf-8")
        self.package = self.root / "package.json"
        self.package.write_text(json.dumps({"version": "0.3.0"}), encoding="utf-8")
        self.output = self.root / "output"

    def tearDown(self):
        self.temp.cleanup()

    def write_installers(self):
        for target, (_platform, package_format) in metadata.INSTALLER_TARGETS.items():
            signature = {
                "deb": b"!<arch>\n",
                "rpm": bytes.fromhex("edabeedb"),
                "pkg": b"xar!",
                "exe": b"MZ",
            }[package_format]
            (self.artifacts / metadata.installer_filename("0.3.0", target)).write_bytes(signature + b"fixture")

    def prepare(self):
        import subprocess, sys
        return subprocess.run([
            sys.executable, str(metadata.ROOT / "scripts" / "prepare_release_metadata.py"),
            "--version", "0.3.0", "--artifact-dir", str(self.artifacts),
            "--output-dir", str(self.output), "--package-json", str(self.package),
            "--source-sha", "a" * 40, "--workflow-run-id", "123",
            "--workflow-ref", "repo/.github/workflows/release-build.yml@refs/tags/v0.3.0",
        ], capture_output=True, text=True)

    def test_generates_canonical_installer_manifest_and_checksums(self):
        self.write_installers()
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(set(manifest["installers"]), set(metadata.INSTALLER_TARGETS))
        sums = (self.output / "SHA256SUMS").read_text()
        self.assertIn(f"{metadata.sha256(self.output / 'manifest.json')}  manifest.json\n", sums)
        self.assertTrue(all(metadata.installer_filename("0.3.0", target) in sums for target in metadata.INSTALLER_TARGETS))
        self.assertTrue(all(manifest["installers"][target]["source_manifest_sha256"] for target in metadata.INSTALLER_TARGETS))
        self.assertEqual(manifest["installers"]["linux-x64-deb"]["source_manifest_sha256"], manifest["installers"]["linux-x64-rpm"]["source_manifest_sha256"])
        self.assertEqual(len(manifest["installers"]), 8)
        self.assertEqual(len(manifest["artifacts"]), 6)

    def test_generates_core_metadata_without_unified_installers(self):
        for platform in metadata.PLATFORMS:
            (self.artifacts / metadata.source_manifest_filename("0.3.0", platform)).unlink()
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertNotIn("installers", manifest)
        self.assertEqual(len(manifest["artifacts"]), 6)
        sums = (self.output / "SHA256SUMS").read_text()
        self.assertNotIn("webcodex-unified-", sums)
        self.assertNotIn("webcodex-source-", sums)

    def test_rejects_partial_unified_installer_set(self):
        path = self.artifacts / metadata.installer_filename("0.3.0", next(iter(metadata.INSTALLER_TARGETS)))
        path.write_bytes(b"!<arch>\nfixture")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("incomplete unified installer set", result.stderr)

    def test_rejects_source_manifest_from_different_workflow_run(self):
        self.write_installers()
        path = self.artifacts / metadata.source_manifest_filename("0.3.0", metadata.PLATFORMS[0])
        value = json.loads(path.read_text())
        value["source_workflow_run_id"] += 1
        path.write_text(json.dumps(value), encoding="utf-8")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("CI provenance mismatch", result.stderr)

    def test_rejects_component_build_info_from_different_source_or_dirty_build(self):
        self.write_installers()
        path = self.artifacts / metadata.source_manifest_filename("0.3.0", metadata.PLATFORMS[0])
        value = json.loads(path.read_text())
        info = value["artifacts"]["webcodex"]["build_info"]
        info["git_commit"] = "b" * 40
        path.write_text(json.dumps(value), encoding="utf-8")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("component provenance/data format mismatch", result.stderr)

        info["git_commit"] = "a" * 40
        info["git_dirty"] = True
        path.write_text(json.dumps(value), encoding="utf-8")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("component provenance/data format mismatch", result.stderr)


if __name__ == "__main__":
    unittest.main()
