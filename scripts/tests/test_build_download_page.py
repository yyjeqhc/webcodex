from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from scripts.build_download_page import INSTALLER_TARGETS, build, filename, validate_manifest


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
                target: {
                    "platform": platform,
                    "format": package_format,
                    "filename": filename("1.2.3", target),
                    "url": f"https://github.com/yyjeqhc/webcodex/releases/download/v1.2.3/{filename('1.2.3', target)}",
                    "sha256": hashlib.sha256(target.encode()).hexdigest(),
                    "source_manifest_url": f"https://github.com/yyjeqhc/webcodex/releases/download/v1.2.3/webcodex-source-v1.2.3-{platform}.json",
                    "source_manifest_sha256": "a" * 64,
                }
                for target, (platform, package_format) in INSTALLER_TARGETS.items()
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
        target = next(reversed(INSTALLER_TARGETS))
        platform, package_format = INSTALLER_TARGETS[target]
        del self.manifest["installers"][target]
        with self.assertRaisesRegex(ValueError, "all eight"):
            validate_manifest(self.manifest)
        self.manifest["installers"][target] = {
            "platform": platform,
            "format": package_format,
            "filename": filename("1.2.3", target),
            "url": "https://example.invalid/setup.exe",
            "sha256": "0" * 64,
            "source_manifest_url": f"https://github.com/yyjeqhc/webcodex/releases/download/v1.2.3/webcodex-source-v1.2.3-{platform}.json",
            "source_manifest_sha256": "a" * 64,
        }
        with self.assertRaisesRegex(ValueError, "non-canonical"):
            validate_manifest(self.manifest)


if __name__ == "__main__":
    unittest.main()
