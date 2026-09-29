"""Credential-free subprocess tests of the macOS CI signing setup script.

All platform/signing commands are fake executables in a disposable directory;
these tests never import a certificate or change the user's real keychains.
"""
from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
FAKE_COMMAND = r'''#!/usr/bin/env python3
import base64
import json
import os
from pathlib import Path
import stat
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
if name == "uname":
    print("Darwin")
    raise SystemExit(0)
root = Path(os.environ["RUNNER_TEMP"])
record = {"command": name, "operation": args[0]}
if name == "openssl" and args[0] == "base64":
    record["certificate_mode"] = stat.S_IMODE((root / "webcodex-developer-id.p12").stat().st_mode)
with (root / "calls.jsonl").open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record) + "\n")
if name == "openssl":
    if args[0] == "rand":
        print("a" * 64)
    elif args[0] == "base64":
        sys.stdout.buffer.write(base64.b64decode(sys.stdin.buffer.read(), validate=True))
    else:
        raise SystemExit(2)
elif name == "security":
    if args[0] == "create-keychain":
        Path(args[-1]).write_bytes(b"test-keychain")
    elif args[0] == "delete-keychain":
        Path(args[-1]).unlink(missing_ok=True)
    elif args[0] == "import" and os.environ.get("FAIL_SIGNING_STAGE") == "import":
        raise SystemExit(4)
    elif args[0] == "find-identity":
        if os.environ.get("FAIL_SIGNING_STAGE") != "identity":
            print('  1) ' + 'b' * 40 + ' "' + os.environ["APPLE_SIGNING_IDENTITY"] + '"')
else:
    raise SystemExit(2)
'''


@unittest.skipIf(os.name == "nt", "Bash executable fixtures require POSIX")
class MacosSigningSetupTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("uname", "security", "openssl"):
            path = self.bin / name
            path.write_text(FAKE_COMMAND, encoding="utf-8")
            path.chmod(0o755)
        self.certificate = self.root / "webcodex-developer-id.p12"
        self.keychain = self.root / "webcodex-developer-id.keychain-db"
        self.env = dict(os.environ, PATH=f"{self.bin}{os.pathsep}{os.environ['PATH']}",
                        RUNNER_TEMP=str(self.root), GITHUB_ENV=str(self.root / "github-env"),
                        APPLE_CERTIFICATE=base64.b64encode(b"fixture-certificate-private").decode(),
                        APPLE_CERTIFICATE_PASSWORD="fixture-password-private",
                        APPLE_SIGNING_IDENTITY="Developer ID Application: Test Fixture (TESTTEAM)")
        self.env.pop("FAIL_SIGNING_STAGE", None)

    def run_setup(self, success: bool) -> None:
        result = subprocess.run(["bash", str(ROOT / "scripts/macos_ci_developer_id_setup.sh")],
                                env=self.env, stdin=subprocess.DEVNULL, capture_output=True,
                                text=True, timeout=10)
        self.assertEqual(result.returncode == 0, success, result.stderr)
        for secret in (self.env["APPLE_CERTIFICATE"], self.env["APPLE_CERTIFICATE_PASSWORD"],
                       "fixture-certificate-private"):
            self.assertNotIn(secret, result.stdout + result.stderr)

    def calls(self) -> list[dict]:
        path = self.root / "calls.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_success_keeps_only_the_private_keychain_for_following_signing_steps(self) -> None:
        self.run_setup(True)
        self.assertFalse(self.certificate.exists())
        self.assertTrue(self.keychain.is_file())
        self.assertIn("WEBCODEX_SIGNING_KEYCHAIN=", (self.root / "github-env").read_text())
        decode = next(call for call in self.calls() if call["operation"] == "base64")
        self.assertEqual(decode["certificate_mode"], 0o600)

    def test_import_and_identity_failures_remove_owned_credential_material(self) -> None:
        for stage in ("import", "identity"):
            with self.subTest(stage=stage):
                self.env["FAIL_SIGNING_STAGE"] = stage
                self.run_setup(False)
                self.assertFalse(self.certificate.exists())
                self.assertFalse(self.keychain.exists())
                self.assertFalse((self.root / "github-env").exists())
                self.assertTrue(any(call["operation"] == "delete-keychain" for call in self.calls()))

    def test_existing_scratch_is_preserved_without_running_signing_commands(self) -> None:
        for path in (self.certificate, self.keychain):
            with self.subTest(name=path.name):
                path.write_bytes(b"preexisting")
                self.run_setup(False)
                self.assertEqual(path.read_bytes(), b"preexisting")
                self.assertEqual(self.calls(), [])
                path.unlink()

    def test_dangling_scratch_symlinks_are_not_followed_or_removed(self) -> None:
        for path in (self.certificate, self.keychain):
            with self.subTest(name=path.name):
                target = self.root / "not-created"
                path.symlink_to(target)
                self.run_setup(False)
                self.assertTrue(path.is_symlink())
                self.assertFalse(target.exists())
                self.assertEqual(self.calls(), [])
                path.unlink()


if __name__ == "__main__":
    unittest.main()
