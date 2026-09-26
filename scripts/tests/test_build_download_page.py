from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from scripts.build_download_page import PLATFORMS, build, filename, validate_manifest


class BuildDownloadPageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.manifest_path = self.root / "manifest.json"
        self.static = self.root / "static"
        self.static.mkdir()
        for name in ("index.html", "styles.css", "app.js"):
            (self.static / name).write_text(name, encoding="utf-8")
        self.manifest = {
            "version": "1.2.3",
            "installers": {
                platform: {
                    "filename": filename("1.2.3", platform),
                    "url": f"https://github.com/yyjeqhc/webcodex/releases/download/v1.2.3/{filename('1.2.3', platform)}",
                    "sha256": hashlib.sha256(platform.encode()).hexdigest(),
                }
                for platform in PLATFORMS
            },
        }
        self.manifest_path.write_text(json.dumps(self.manifest), encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def test_build_copies_valid_manifest_and_page_assets(self):
        output = self.root / "out"
        build(self.manifest_path, output, self.static)
        self.assertEqual(json.loads((output / "manifest.json").read_text()), self.manifest)
        self.assertTrue(all((output / name).is_file() for name in ("index.html", "styles.css", "app.js")))

    def test_rejects_incomplete_or_external_installer_manifest(self):
        del self.manifest["installers"][PLATFORMS[-1]]
        with self.assertRaisesRegex(ValueError, "all six"):
            validate_manifest(self.manifest)
        self.manifest["installers"][PLATFORMS[-1]] = {
            "filename": filename("1.2.3", PLATFORMS[-1]),
            "url": "https://example.invalid/setup.exe",
            "sha256": "0" * 64,
        }
        with self.assertRaisesRegex(ValueError, "non-canonical"):
            validate_manifest(self.manifest)


if __name__ == "__main__":
    unittest.main()
