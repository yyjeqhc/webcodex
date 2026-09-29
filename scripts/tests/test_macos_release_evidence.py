"""Execute the actual release workflow verifier against synthetic signed evidence.

No Apple credentials, signing or publication is involved. Native signing smoke
owns signature validation; this suite checks its downstream evidence consumer.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]


class MacosReleaseEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        workflow = (ROOT / ".github/workflows/release-build.yml").read_text(encoding="utf-8")
        blocks = re.findall(r"^          python3 - <<'PY'\n(.*?)^          PY$", workflow, re.M | re.S)
        matches = [block for block in blocks if "unexpected native macOS Desktop evidence schema" in block]
        if len(matches) != 1:
            raise AssertionError("expected one authoritative native macOS evidence verifier")
        cls.verifier = textwrap.dedent(matches[0])

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.candidates = self.root / "candidate-input"
        self.candidates.mkdir()
        self.env = dict(os.environ, BUILD_KIND="release", DESKTOP_DARWIN_X64_SUPPLEMENTAL="false")
        for platform in ("darwin-arm64", "darwin-x64"):
            name = f"webcodex-desktop-v0.4.4-{platform}.dmg"
            self.env[f"DESKTOP_{platform.upper().replace('-', '_')}_FILENAME"] = name
            data = f"synthetic {platform} DMG".encode()
            (self.candidates / name).write_bytes(data)
            self.write_evidence(platform, {
                "schema_version": 1,
                "platform": platform,
                "signing_mode": "developer-id",
                "notarized": True,
                "dmg_sha256": hashlib.sha256(data).hexdigest(),
                "runtime": {binary: {"runtime_input_sha256": "a" * 64, "bundled_signed_sha256": "b" * 64}
                            for binary in ("webcodex", "webcodex-server", "webcodex-runner")},
            })

    def path(self, platform: str) -> Path:
        name = self.env[f"DESKTOP_{platform.upper().replace('-', '_')}_FILENAME"]
        return self.candidates / f"{name}.evidence.json"

    def write_evidence(self, platform: str, evidence: dict) -> None:
        self.path(platform).write_text(json.dumps(evidence), encoding="utf-8")

    def update(self, platform: str = "darwin-arm64", **changes: object) -> None:
        evidence = json.loads(self.path(platform).read_text(encoding="utf-8"))
        evidence.update(changes)
        self.write_evidence(platform, evidence)

    def verify(self, expected_success: bool) -> None:
        result = subprocess.run([sys.executable, "-c", self.verifier], cwd=self.root, env=self.env,
                                stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode == 0, expected_success, result.stderr)

    def test_formal_release_accepts_developer_id_and_current_digest_fields(self) -> None:
        self.verify(True)

    def test_verification_build_accepts_only_adhoc_without_notarization(self) -> None:
        self.env["BUILD_KIND"] = "verification"
        for platform in ("darwin-arm64", "darwin-x64"):
            self.update(platform, signing_mode="adhoc", notarized=False)
        self.verify(True)
        self.env["BUILD_KIND"] = "release"
        self.verify(False)

    def test_formal_release_rejects_missing_notarization(self) -> None:
        self.update(notarized=False)
        self.verify(False)

    def test_bad_dmg_digest_is_rejected(self) -> None:
        self.update(dmg_sha256="0" * 64)
        self.verify(False)

    def test_retired_runtime_field_is_not_silently_accepted(self) -> None:
        evidence = json.loads(self.path("darwin-arm64").read_text(encoding="utf-8"))
        for item in evidence["runtime"].values():
            item["unsigned_input_sha256"] = item.pop("runtime_input_sha256")
        self.write_evidence("darwin-arm64", evidence)
        self.verify(False)

    def test_supplemental_intel_is_not_required_in_primary_bundle(self) -> None:
        self.env["DESKTOP_DARWIN_X64_SUPPLEMENTAL"] = "true"
        self.path("darwin-x64").unlink()
        self.verify(True)

    def test_unknown_build_kind_does_not_weaken_signing_gate(self) -> None:
        self.env["BUILD_KIND"] = "unknown"
        self.verify(False)


if __name__ == "__main__":
    unittest.main()
