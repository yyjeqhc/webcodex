"""Exercise the production Tauri hook from its native cwd with an old dist tree."""
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class DesktopFrontendBuildTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("npm") and shutil.which("rustc"), "npm and rustc are required")
    def test_direct_production_cargo_build_rebuilds_dist_and_failed_build_is_fatal(self):
        with tempfile.TemporaryDirectory() as temporary:
            desktop = Path(temporary)
            native = desktop / "src-tauri"
            native.mkdir()
            (desktop / "dist").mkdir()
            (desktop / "dist/index.html").write_text("old UI")
            (desktop / "package.json").write_text(json.dumps({"scripts": {"build": "node build.cjs"}}))
            (desktop / "build.cjs").write_text("require('fs').writeFileSync('dist/index.html', 'new UI')")
            # Compile the real build entry point, stubbing only Tauri helpers.
            source = native / "build_fixture.rs"
            source.write_text('mod tauri_build { pub fn is_dev() -> bool { false } pub fn build() {} }\n'
                              + (ROOT / "apps/desktop/src-tauri/build.rs").read_text())
            program = native / ("build_fixture.exe" if os.name == "nt" else "build_fixture")
            compiled = subprocess.run(["rustc", str(source), "-o", str(program)], stdin=subprocess.DEVNULL,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
            self.assertEqual(compiled.returncode, 0)
            env = {**os.environ, "CARGO_MANIFEST_DIR": str(native)}
            result = subprocess.run([str(program)], cwd=desktop, env=env, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
            self.assertEqual(result.returncode, 0)
            self.assertEqual((desktop / "dist/index.html").read_text(), "new UI")
            (desktop / "build.cjs").write_text("process.exit(1)")
            failed = subprocess.run([str(program)], cwd=desktop, env=env, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
            self.assertNotEqual(failed.returncode, 0, "A failed UI build must stop native packaging")

    @unittest.skipUnless(shutil.which("npm"), "npm is required to exercise the build hook")
    def test_native_build_hook_replaces_stale_dist_in_desktop_not_repo_frontend(self):
        hook = json.loads((ROOT / "apps/desktop/src-tauri/tauri.conf.json").read_text())["build"]["beforeBuildCommand"]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            desktop = root / "apps/desktop"
            native = desktop / "src-tauri"
            native.mkdir(parents=True)
            (desktop / "dist").mkdir()
            (desktop / "dist/index.html").write_text("stale Desktop UI")
            (desktop / "package.json").write_text(json.dumps({"scripts": {"build": "node build.cjs"}}))
            (desktop / "build.cjs").write_text("require('fs').writeFileSync('dist/index.html', 'current Desktop UI')")
            # A decoy frontend models Tauri's workspace frontend discovery.
            frontend = root / "frontend"
            frontend.mkdir()
            (frontend / "package.json").write_text(json.dumps({"scripts": {"build": "node -e \"process.exit(42)\""}}))
            result = subprocess.run(hook["script"], cwd=native / hook["cwd"], shell=True,
                                    stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
            self.assertEqual(result.returncode, 0, "Desktop build hook did not complete")
            self.assertEqual((desktop / "dist/index.html").read_text(), "current Desktop UI")
