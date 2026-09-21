from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))

from scripts.webpi.relocate import RelocationError, copy_webpi_tree


class WebPiRelocationTests(unittest.TestCase):
    def test_copy_preserves_repo_and_runtime_but_excludes_local_state_and_caches(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            base = Path(td)
            source = base / "source"
            target = base / "target"
            for relative, text in {
                ".git/config": "git",
                "README.md": "repo",
                ".webpi-runtime/node/node.exe": "node",
                "target/dogfood/webpi.exe": "cli",
                "target/dogfood/deps/build-cache.bin": "cache",
                "target/debug/intermediate.bin": "debug-cache",
                "plugins/pi-bridge/node_modules/pkg/index.js": "dep",
                "plugins/pi-bridge/dist/plugin.js": "plugin",
                ".webpi-state/server/webpi.env": "SECRET",
                "scripts/webpi/__pycache__/x.pyc": "cache",
            }.items():
                path = source / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text, encoding="utf-8")

            summary = copy_webpi_tree(source, target)

            self.assertTrue((target / ".git" / "config").is_file())
            self.assertTrue((target / ".webpi-runtime" / "node" / "node.exe").is_file())
            self.assertTrue((target / "target" / "dogfood" / "webpi.exe").is_file())
            self.assertFalse((target / "target" / "dogfood" / "deps").exists())
            self.assertFalse((target / "target" / "debug").exists())
            self.assertTrue((target / "plugins" / "pi-bridge" / "node_modules" / "pkg" / "index.js").is_file())
            self.assertFalse((target / ".webpi-state").exists())
            self.assertFalse((target / "scripts" / "webpi" / "__pycache__").exists())
            self.assertEqual(summary["target"], str(target.resolve()))
            self.assertEqual(summary["state_copied"], False)

    def test_existing_target_is_rejected_without_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            base = Path(td)
            source = base / "source"
            target = base / "target"
            source.mkdir()
            target.mkdir()
            with self.assertRaises(RelocationError):
                copy_webpi_tree(source, target)

    def test_target_inside_source_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            source = Path(td) / "source"
            source.mkdir()
            with self.assertRaises(RelocationError):
                copy_webpi_tree(source, source / "nested")


if __name__ == "__main__":
    unittest.main()
