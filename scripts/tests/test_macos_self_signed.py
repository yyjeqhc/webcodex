"""Credential-free tests of pinned signing and designated-requirement checks."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipIf(os.name == "nt", "POSIX shell fixtures")
class SelfSignedTests(unittest.TestCase):
    def test_persistent_requirement_and_fail_closed_fences(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            security = root / "security"
            security.write_text('#!/bin/sh\nprintf \'  1) ' + 'B' * 40 + ' "Fixture"\\n\'\n')
            security.chmod(0o755)
            codesign = root / "codesign"
            codesign.write_text('''#!/bin/sh
if [ "$1" = -d ]; then
  printf '%s\\n' "$FIXTURE_REQUIREMENT"
fi
''')
            codesign.chmod(0o755)
            requirement = 'designated => identifier "dev.webcodex.runner" and certificate root = H"' + 'b' * 40 + '"'
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                       WEBCODEX_MACOS_CERTIFICATE_SHA1="b" * 40)
            for actual, pin, success in (
                (requirement, "b" * 40, True),
                (requirement + ' and cdhash H"abc"', "b" * 40, False),
                (requirement.replace("dev.webcodex.runner", "foreign"), "b" * 40, False),
                (requirement.replace("b" * 40, "a" * 40), "b" * 40, False),
                (requirement, "c" * 40, False),
            ):
                with self.subTest(actual=actual, pin=pin):
                    env.update(FIXTURE_REQUIREMENT=actual, WEBCODEX_MACOS_CERTIFICATE_SHA1=pin)
                    result = subprocess.run(["bash", str(ROOT / "scripts/macos_sign_self_signed.sh"),
                                             str(root / "binary"), "dev.webcodex.runner", "Fixture"],
                                            env=env, stdin=subprocess.DEVNULL, capture_output=True, timeout=10)
                    self.assertEqual(result.returncode == 0, success, result.stderr)
