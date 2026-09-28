"""Verify the public Desktop source is the existing retained unified manifest."""
from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import collect_release_bundle as collector
from scripts import release_publication as publication
from scripts import verify_public_release as verifier
from scripts.tests.test_collect_release_bundle import _write_bundle, VERSION, SOURCE_SHA, RUN_ID


class UpdaterPublicationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        _write_bundle(self.root, f"v{VERSION}", "release", unified=True)
        self.raw = (self.root / "manifest.json").read_bytes()
        self.manifest = json.loads(self.raw)
        self.sums = verifier.parse_sha256sums(
            (self.root / "SHA256SUMS").read_text(), VERSION, unified_installers=True,
        )
        self.assets = {
            name: {
                "name": name, "state": "uploaded", "size": (self.root / name).stat().st_size,
                "browser_download_url": f"https://github.com/{verifier.REPO}/releases/download/v{VERSION}/{name}",
                "digest": f"sha256:{collector.sha256_file(self.root / name)}",
            }
            for name in [*self.sums, "SHA256SUMS"]
        }
        self.release = {
            "tag_name": f"v{VERSION}", "draft": False, "prerelease": False,
            "assets": list(self.assets.values()),
        }

    def verify(self, raw=None):
        with mock.patch.object(verifier, "fetch_bytes", return_value=self.raw if raw is None else raw) as fetch:
            verifier.verify_public_installer_manifest(self.assets, self.sums, self.manifest, VERSION, 5)
        fetch.assert_called_once_with(self.assets["manifest.json"]["browser_download_url"], 256 * 1024, 5)

    def test_public_manifest_is_checksummed_and_matches_npm(self):
        self.assertIn("manifest.json", verifier.validate_github_assets(self.release, VERSION))
        self.verify()

    def test_missing_public_manifest_or_checksum_fails(self):
        for missing in ("asset", "checksum"):
            with self.subTest(missing=missing):
                target = self.assets if missing == "asset" else self.sums
                value = target.pop("manifest.json")
                with self.assertRaises(verifier.VerificationError):
                    self.verify()
                target["manifest.json"] = value
        self.release["assets"] = [a for a in self.release["assets"] if a["name"] != "manifest.json"]
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_github_assets(self.release, VERSION)

    def test_corrupt_bytes_or_npm_disagreement_fails(self):
        with self.assertRaises(verifier.VerificationError):
            self.verify(self.raw + b" ")
        self.manifest["installers"]["darwin-arm64-pkg"]["sha256"] = "0" * 64
        with self.assertRaises(verifier.VerificationError):
            self.verify()

    def test_noncanonical_manifest_url_fails(self):
        self.assets["manifest.json"]["browser_download_url"] = "https://example.invalid/manifest.json"
        with self.assertRaises(verifier.VerificationError):
            self.verify()

    def test_duplicate_or_malformed_assets_fail(self):
        self.release["assets"].append(dict(self.assets["manifest.json"]))
        with self.assertRaisesRegex(verifier.VerificationError, "duplicate"):
            verifier.validate_github_assets(self.release, VERSION)
        self.release["assets"][-1] = {"name": 42}
        with self.assertRaisesRegex(verifier.VerificationError, "malformed"):
            verifier.validate_github_assets(self.release, VERSION)

    def test_partial_eight_target_set_fails(self):
        removed = verifier.canonical_installer_name(VERSION, "linux-arm64-rpm")
        self.release["assets"] = [a for a in self.release["assets"] if a["name"] != removed]
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_github_assets(self.release, VERSION)
        del self.manifest["installers"]["linux-arm64-rpm"]
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_public_installers(self.manifest, VERSION)

    def test_duplicate_json_fields_fail_even_with_matching_hashes(self):
        raw = b'{"version":"' + VERSION.encode() + b'","version":"' + VERSION.encode() + b'"}'
        self.sums["manifest.json"] = hashlib.sha256(raw).hexdigest()
        self.assets["manifest.json"].update(size=len(raw), digest="sha256:" + self.sums["manifest.json"])
        with self.assertRaisesRegex(verifier.VerificationError, "duplicate"):
            self.verify(raw)

    def test_draft_verification_requires_manifest_digest(self):
        summary = {
            "build_kind": "release", "tag": f"v{VERSION}", "version": VERSION,
            "source_sha": SOURCE_SHA, "workflow_run_id": RUN_ID,
            "archive_stem": f"webcodex-v{VERSION}",
            "desktop_artifacts": {
                p: {"filename": verifier.canonical_desktop_name(VERSION, p)}
                for p in collector.primary_desktop_platforms_for_version(VERSION)
            },
            "installer_artifacts": {
                target: {
                    "filename": verifier.canonical_installer_name(VERSION, target),
                    "source_manifest_filename": verifier.canonical_source_manifest_name(
                        VERSION, collector.INSTALLER_TARGETS[target][0]
                    ),
                }
                for target in collector.INSTALLER_TARGETS
            },
        }
        draft = dict(self.release, id=123, draft=True, html_url=f"https://github.com/{verifier.REPO}/releases/tag/v{VERSION}")
        with mock.patch.object(publication, "verify_bundle", return_value=summary), \
             mock.patch.object(collector, "resolve_github_token", return_value="fixture-only"), \
             mock.patch.object(publication, "_find_authenticated_release_by_tag", return_value=draft):
            result = publication.verify_draft_assets(repo=verifier.REPO, bundle_dir=self.root, timeout=5)
            self.assertIn("manifest.json", result["assets"])
            self.assets["manifest.json"]["digest"] = "sha256:" + "0" * 64
            with self.assertRaises(publication.PublicationError):
                publication.verify_draft_assets(repo=verifier.REPO, bundle_dir=self.root, timeout=5)


if __name__ == "__main__":
    unittest.main()
