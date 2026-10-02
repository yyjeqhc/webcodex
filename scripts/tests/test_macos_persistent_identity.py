"""Opt-in native signing/DMG regression. Reuses an existing key; installs nothing."""
import os
import sys
from pathlib import Path
import plistlib
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
IDENTITY = os.environ.get("WEBCODEX_TEST_SIGNING_IDENTITY")


@unittest.skipUnless(IDENTITY and sys.platform == "darwin", "explicit existing macOS signing identity required")
class PersistentIdentityTests(unittest.TestCase):
    def run_cmd(self, *args, env=None):
        try:
            return subprocess.run(args, env=env, stdin=subprocess.DEVNULL, capture_output=True,
                                  text=True, check=True, timeout=90).stdout
        except subprocess.CalledProcessError as exc:
            print(exc.stderr, file=sys.stderr)
            raise

    def test_two_builds_and_final_dmg_keep_certificate_identity(self):
        identities = self.run_cmd("security", "find-identity", "-v", "-p", "codesigning")
        hashes = re.findall(r'([A-Fa-f0-9]{40}) "' + re.escape(IDENTITY) + '"', identities)
        self.assertEqual(len(hashes), 1)
        env = dict(os.environ, WEBCODEX_MACOS_CERTIFICATE_SHA1=hashes[0])
        requirements = []
        cdhashes = []
        (ROOT / "target/hotfix-validation").mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="signing-", dir=ROOT / "target/hotfix-validation") as tmp:
            root = Path(tmp)
            for build in (1, 2):
                payload = root / str(build)
                app = payload / "WebCodex Desktop.app"
                macos = app / "Contents/MacOS"
                runtime = app / "Contents/Resources/webcodex-runtime"
                macos.mkdir(parents=True)
                runtime.mkdir(parents=True)
                info = dict(CFBundleIdentifier="dev.webcodex.desktop", CFBundleExecutable="WebCodex",
                            CFBundlePackageType="APPL", NSScreenCaptureUsageDescription="Fixture capture",
                            NSAccessibilityUsageDescription="Fixture accessibility")
                (app / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
                info["CFBundleIdentifier"] = "dev.webcodex.runner"
                runner_plist = root / "runner.plist"
                runner_plist.write_bytes(plistlib.dumps(info))
                source = root / "fixture.c"
                source.write_text(f"int main(void) {{ return {build}; }}\n")
                runner = runtime / "webcodex-runner"
                self.run_cmd("clang", str(source), "-Wl,-sectcreate,__TEXT,__info_plist," + str(runner_plist), "-o", str(runner))
                self.run_cmd("clang", str(source), "-o", str(macos / "WebCodex"))
                # Model Tauri's nested ad-hoc re-signing, then finalize actual DMG bytes.
                self.run_cmd("codesign", "--force", "--deep", "--sign", "-", str(app))
                dmg = root / f"build-{build}.dmg"
                self.run_cmd("hdiutil", "create", "-quiet", "-srcfolder", str(payload), str(dmg))
                self.run_cmd("bash", str(ROOT / "scripts/macos_finalize_dmg.sh"), str(dmg), "self-signed", IDENTITY, env=env)
                mount = root / "mount"
                mount.mkdir(exist_ok=True)
                self.run_cmd("hdiutil", "attach", str(dmg), "-readonly", "-nobrowse", "-mountpoint", str(mount), "-quiet")
                try:
                    final = mount / app.name
                    self.run_cmd("bash", str(ROOT / "scripts/verify_macos_desktop_identity.sh"), str(final), "self-signed", env=env)
                    row = []
                    hashes_row = []
                    for code in (final, final / "Contents/Resources/webcodex-runtime/webcodex-runner"):
                        result = subprocess.run(["codesign", "-d", "-r-", "--verbose=4", str(code)], capture_output=True, text=True, check=True)
                        output = result.stdout + result.stderr
                        row.append(next(line for line in output.splitlines() if line.startswith("designated =>")))
                        hashes_row.append(re.search(r"^CDHash=(.+)$", output, re.M).group(1))
                        self.assertNotIn("cdhash", row[-1].lower())
                    requirements.append(row)
                    cdhashes.append(hashes_row)
                    wrong = dict(env, WEBCODEX_MACOS_CERTIFICATE_SHA1="0" * 40)
                    with self.assertRaises(subprocess.CalledProcessError):
                        self.run_cmd("bash", str(ROOT / "scripts/verify_macos_desktop_identity.sh"), str(final), "self-signed", env=wrong)
                finally:
                    self.run_cmd("hdiutil", "detach", str(mount), "-quiet")
            self.assertEqual(requirements[0], requirements[1])
            for old, new in zip(cdhashes[0], cdhashes[1]):
                self.assertNotEqual(old, new)
