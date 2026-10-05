import hashlib
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from scripts.package_tunnel_artifact import package, TARGETS


class TunnelPackagingTests(unittest.TestCase):
    def test_platform_archive_contains_only_standalone_files(self):
        for platform, target in TARGETS.items():
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                binary = root / "binary"
                binary.write_bytes(b"synthetic executable fixture")
                sha = "a" * 40
                info = dict(schema_version=1, binary="webcodex-tunnel", version="0.1.0", target=target, source_commit=sha, _observed_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
                path = package(binary, root / "out", platform, info, source=sha, development=False)
                with tarfile.open(path) as archive:
                    name = "webcodex-tunnel.exe" if platform.startswith("win32") else "webcodex-tunnel"
                    self.assertEqual(archive.getnames(), [name, "README.md", "tunnel-build.json"])
                    self.assertEqual(archive.extractfile(name).read(), binary.read_bytes())
                    manifest = json.load(archive.extractfile("tunnel-build.json"))
                    self.assertFalse(manifest["development"])
                    self.assertEqual(manifest["sha256"], hashlib.sha256(binary.read_bytes()).hexdigest())
                before = path.read_bytes()
                with self.assertRaises(ValueError):
                    package(binary, path.parent, platform, info, source=sha, development=False)
                self.assertEqual(path.read_bytes(), before)

    def test_rebuilt_binary_cannot_borrow_previous_probe_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary = root / "binary"
            binary.write_bytes(b"new build")
            info = dict(schema_version=1, binary="webcodex-tunnel", version="0.1.0", target=TARGETS["linux-x64"],
                        source_commit=None, _observed_sha256=hashlib.sha256(b"old build").hexdigest())
            with self.assertRaisesRegex(ValueError, "changed after"):
                package(binary, root / "out", "linux-x64", info, source=None, development=True)
            self.assertFalse((root / "out").exists())

    def test_wrong_target_or_unproven_source_is_not_a_release_artifact(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary = root / "binary"
            binary.write_bytes(b"fixture")
            info = dict(schema_version=1, binary="webcodex-tunnel", version="0.1.0", target=TARGETS["linux-x64"], source_commit=None, _observed_sha256=hashlib.sha256(binary.read_bytes()).hexdigest())
            for platform, source in [("linux-arm64", "a" * 40), ("linux-x64", None), ("linux-x64", "a" * 40)]:
                with self.assertRaises(ValueError):
                    package(binary, root / "out", platform, info, source=source, development=False)
            self.assertFalse((root / "out").exists())
            path = package(binary, root / "out", "linux-x64", info, source=None, development=True)
            self.assertIn("development", path.name)
            with tarfile.open(path) as archive:
                self.assertTrue(json.load(archive.extractfile("tunnel-build.json"))["development"])


if __name__ == "__main__":
    unittest.main()
