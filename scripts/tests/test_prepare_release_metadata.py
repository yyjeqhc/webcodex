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
            records = {name: {"build_info": {"version": "0.3.0", "source_sha": source_sha, "environment_data_format": 1}} for name in ("webcodex", "webcodex-server", "webcodex-runner", "webcodex-desktop")}
            value = {"schema_version": 1, "version": "0.3.0", "source_sha": source_sha, "source_workflow_run_id": 123, "source_workflow_ref": "repo/.github/workflows/release-build.yml@refs/tags/v0.3.0", "platform": platform, "target": "fixture", "architecture": platform.split("-")[1], "desktop_runtime_contract": {}, "artifacts": records, "desktop_payload": {}}
            (self.artifacts / metadata.source_manifest_filename("0.3.0", platform)).write_text(json.dumps(value), encoding="utf-8")
        self.package = self.root / "package.json"
        self.package.write_text(json.dumps({"version": "0.3.0"}), encoding="utf-8")
        self.output = self.root / "output"

    def tearDown(self):
        self.temp.cleanup()

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
        for platform in metadata.PLATFORMS:
            name = metadata.installer_filename("0.3.0", platform)
            signature = b"!<arch>\n" if platform.startswith("linux-") else b"xar!" if platform.startswith("darwin-") else b"MZ"
            (self.artifacts / name).write_bytes(signature + b"fixture")
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(set(manifest["installers"]), set(metadata.PLATFORMS))
        sums = (self.output / "SHA256SUMS").read_text()
        self.assertTrue(all(metadata.installer_filename("0.3.0", platform) in sums for platform in metadata.PLATFORMS))
        self.assertTrue(all(manifest["installers"][platform]["source_manifest_sha256"] for platform in metadata.PLATFORMS))

    def test_rejects_partial_unified_installer_set(self):
        path = self.artifacts / metadata.installer_filename("0.3.0", metadata.PLATFORMS[0])
        path.write_bytes(b"!<arch>\nfixture")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("incomplete unified installer set", result.stderr)

    def test_rejects_source_manifest_from_different_workflow_run(self):
        for platform in metadata.PLATFORMS:
            (self.artifacts / metadata.installer_filename("0.3.0", platform)).write_bytes(
                (b"!<arch>\n" if platform.startswith("linux-") else b"xar!" if platform.startswith("darwin-") else b"MZ") + b"fixture"
            )
        path = self.artifacts / metadata.source_manifest_filename("0.3.0", metadata.PLATFORMS[0])
        value = json.loads(path.read_text())
        value["source_workflow_run_id"] += 1
        path.write_text(json.dumps(value), encoding="utf-8")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("CI provenance mismatch", result.stderr)


if __name__ == "__main__":
    unittest.main()
