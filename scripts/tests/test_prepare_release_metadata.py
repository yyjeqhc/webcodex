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

    def prepare(self, *extra):
        import subprocess, sys
        return subprocess.run([
            sys.executable, str(metadata.ROOT / "scripts" / "prepare_release_metadata.py"),
            "--version", "0.3.0", "--artifact-dir", str(self.artifacts),
            "--output-dir", str(self.output), "--package-json", str(self.package),
            "--source-sha", "a" * 40, "--workflow-run-id", "123",
            "--workflow-ref", "repo/.github/workflows/release-build.yml@refs/tags/v0.3.0",
            *extra,
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

    def test_guarded_raw_cli_contract_requires_same_source_outer_provenance_and_preserves_old_entries(self):
        self.write_installers()
        platform = "win32-x64"
        installer = self.artifacts / metadata.installer_filename("0.3.0", "win32-x64-exe")
        candidate = self.artifacts / "webcodex-unified-v0.3.0-win32-x64.source-manifest.json"
        source_path = self.artifacts / metadata.source_manifest_filename("0.3.0", platform)
        source = json.loads(source_path.read_text())
        record = source["artifacts"]["webcodex"]
        record["build_info"]["windows_guarded_bootstrap_contract"] = 1
        record["build_info_sha256"] = __import__("hashlib").sha256(json.dumps(record["build_info"], sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        source_path.write_text(json.dumps(source))
        candidate.write_bytes(source_path.read_bytes())
        evidence = {"schema_version": 1, "guarded_handoff_version": 1, "platform": platform, "version": "0.3.0", "source_sha": "a" * 40,
            "workflow_run_id": 123, "workflow_ref": "repo/.github/workflows/release-build.yml@refs/tags/v0.3.0",
            "candidate_manifest_sha256": metadata.sha256(candidate), "inner_installer_sha256": "b" * 64, "installer_sha256": metadata.sha256(installer)}
        sidecar = installer.with_suffix(installer.suffix + ".provenance.json")
        sidecar.write_text(json.dumps(evidence))
        result = self.prepare(); self.assertEqual(result.returncode, 0, result.stderr)
        entries = json.loads((self.output / "manifest.json").read_text())["installers"]
        for entry in entries.values():
            self.assertEqual(set(entry), {"platform", "format", "filename", "url", "sha256", "source_manifest_url", "source_manifest_sha256"})
        self.assertEqual(entries["win32-x64-exe"]["source_manifest_sha256"], metadata.sha256(source_path))
        sidecar.unlink()
        self.assertNotEqual(self.prepare().returncode, 0, "capability cannot outlive its outer provenance")
        for field in ("installer_sha256", "candidate_manifest_sha256", "source_sha", "platform", "guarded_handoff_version"):
            changed = dict(evidence); changed[field] = 2 if field == "guarded_handoff_version" else "mismatch"
            sidecar.write_text(json.dumps(changed))
            self.assertNotEqual(self.prepare().returncode, 0, field)

    def test_strict_requirement_rejects_total_absence_but_legacy_remains_valid(self):
        for platform in metadata.PLATFORMS:
            (self.artifacts / metadata.source_manifest_filename("0.3.0", platform)).unlink()
        result = self.prepare("--require-unified-installers")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("required unified installer set", result.stderr)
        self.assertEqual(self.prepare().returncode, 0)

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

    def test_workflow_metadata_requires_installers_only_for_the_explicit_input(self):
        import os, subprocess, textwrap
        workflow = (metadata.ROOT / ".github/workflows/release-build.yml").read_text()
        options = workflow.split("            metadata_options=()", 1)[1].split(
            "            python3 scripts/desktop_runtime_manifest.py", 1)[0]
        script = "metadata_options=()\n" + textwrap.dedent(options).strip()
        script = script.replace("python3 scripts/prepare_release_metadata.py", 'python3 "$METADATA_SCRIPT"')
        script = script.replace("--artifact-dir candidate-input", '--artifact-dir "$ARTIFACT_DIR"')
        script = script.replace("--output-dir release-bundle", '--output-dir "$OUTPUT_DIR" --package-json "$PACKAGE_JSON"')
        for platform in metadata.PLATFORMS:
            (self.artifacts / metadata.source_manifest_filename("0.3.0", platform)).unlink()
        env = {**os.environ, "METADATA_SCRIPT": str(metadata.ROOT / "scripts/prepare_release_metadata.py"),
               "ARTIFACT_DIR": str(self.artifacts), "OUTPUT_DIR": str(self.output),
               "PACKAGE_JSON": str(self.package), "VERSION": "0.3.0", "SOURCE_SHA": "a" * 40,
               "GITHUB_RUN_ID": "123", "GITHUB_WORKFLOW_REF": "repo/.github/workflows/release-build.yml@refs/tags/v0.3.0"}
        for include in (False, True):
            with self.subTest(include=include):
                result = subprocess.run(["bash", "-euc", script], cwd=self.root,
                                        env={**env, "INCLUDE_UNIFIED_INSTALLERS": str(include).lower()},
                                        input="", capture_output=True, text=True, timeout=30)
                if include:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("required unified installer set", result.stderr)
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertNotIn("installers", json.loads((self.output / "manifest.json").read_text()))

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
