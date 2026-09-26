from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location("prepare_windows_unified_installer", SCRIPTS / "prepare_windows_unified_installer.py")
nsis = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(nsis)

BOOTSTRAP_SPEC = importlib.util.spec_from_file_location("build_windows_unified_bootstrap", SCRIPTS / "build_windows_unified_bootstrap.py")
bootstrap = importlib.util.module_from_spec(BOOTSTRAP_SPEC)
assert BOOTSTRAP_SPEC and BOOTSTRAP_SPEC.loader
BOOTSTRAP_SPEC.loader.exec_module(bootstrap)

SOURCE = "b" * 40
CONTRACT = {"min_generation": 1, "max_generation": 1}


def pe(machine: int) -> bytes:
    data = bytearray(128)
    data[:2] = b"MZ"
    data[0x3C:0x40] = (64).to_bytes(4, "little")
    data[64:68] = b"PE\0\0"
    data[68:70] = machine.to_bytes(2, "little")
    return bytes(data)


class WindowsUnifiedNsisTests(unittest.TestCase):
    def fixture(self, root: Path) -> dict:
        bin_dir = root / "artifacts" / "bin"
        bin_dir.mkdir(parents=True)
        infos = {}
        records = {}
        sums = {}
        for index, name in enumerate(nsis.BINARIES):
            info = {
                "schema_version": 1, "binary": name, "version": "0.9.0", "git_commit": SOURCE,
                "git_dirty": False, "built_at": str(100 + index), "target": "x86_64-pc-windows-msvc",
                "architecture": "x86_64", "desktop_runtime_contract": CONTRACT, "environment_data_format": 1,
            }
            infos[name] = info
            path = bin_dir / f"{name}.exe"
            path.write_bytes(pe(0x8664))
            relative = path.relative_to(root).as_posix()
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            sums[relative] = digest
            records[name] = {
                "path": relative, "sha256": digest, "build_info": info,
                "build_info_sha256": nsis.collector.package.canonical_digest(info), "probe": "native-build-job",
            }
        payload_hash = nsis.collector.package.tree_digest(bin_dir / "webcodex-desktop.exe")
        manifest = {
            "schema_version": 1, "version": "0.9.0", "source_sha": SOURCE,
            "source_workflow_run_id": 787654321,
            "source_workflow_ref": "yyjeqhc/webcodex/.github/workflows/release-build.yml@refs/tags/v0.9.0",
            "platform": "win32-x64", "target": "x86_64-pc-windows-msvc", "architecture": "x86_64",
            "desktop_runtime_contract": CONTRACT, "artifacts": records,
            "desktop_payload": {
                "path": "artifacts/bin/webcodex-desktop.exe", "sha256": payload_hash,
                "executable": "webcodex-desktop.exe",
                "managed_files": [
                    {"path": path, "sha256": records[name]["sha256"]}
                    for path, name in sorted(nsis.MANAGED_INSTALL_FILES.items())
                ],
            },
        }
        raw = json.dumps(manifest, indent=2, sort_keys=True).encode() + b"\n"
        (root / "source-manifest.json").write_bytes(raw)
        sums["source-manifest.json"] = hashlib.sha256(raw).hexdigest()
        (root / "SHA256SUMS").write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(sums.items())), encoding="ascii")
        return infos

    def test_prepares_tauri_inner_resources_for_outer_lifecycle_bootstrap(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "candidate"
            root.mkdir()
            infos = self.fixture(root)
            with mock.patch.object(nsis.collector, "probe", side_effect=lambda path, name: infos[name]):
                result = nsis.prepare(root, Path(temp) / "stage", "win32-x64")
            output = Path(result["config"]).parent
            overlay = json.loads(Path(result["config"]).read_text())
            manifest = json.loads((root / "source-manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(overlay["bundle"]["windows"]["nsis"]["installMode"], "currentUser")
            self.assertEqual(len(overlay["bundle"]["resources"]), 3)
            self.assertTrue((output / "resources/webcodex-runtime/webcodex.exe").is_file())
            self.assertEqual(
                [item["path"] for item in manifest["desktop_payload"]["managed_files"]],
                sorted(nsis.MANAGED_INSTALL_FILES),
            )
            self.assertNotIn("installerHooks", overlay["bundle"]["windows"]["nsis"])

    def test_rejects_unmanaged_windows_desktop_payload_path(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "candidate"
            root.mkdir()
            infos = self.fixture(root)
            manifest_path = root / "source-manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["desktop_payload"]["managed_files"].append({"path": "uninstaller.exe", "sha256": "0" * 64})
            encoded = json.dumps(manifest, indent=2, sort_keys=True).encode() + b"\n"
            manifest_path.write_bytes(encoded)
            sums_path = root / "SHA256SUMS"
            sums = nsis.parse_sums(sums_path)
            sums["source-manifest.json"] = hashlib.sha256(encoded).hexdigest()
            sums_path.write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(sums.items())), encoding="ascii")
            with mock.patch.object(nsis.collector, "probe", side_effect=lambda path, name: infos[name]):
                with self.assertRaisesRegex(ValueError, "managed-file allowlist"):
                    nsis.validate_candidate(root, "win32-x64")

    def test_outer_bootstrap_extracts_candidate_in_section_before_inner_install(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "candidate"
            root.mkdir()
            infos = self.fixture(root)
            inner = Path(temp) / "inner.exe"
            inner.write_bytes(pe(0x8664))
            with mock.patch.object(nsis.collector, "probe", side_effect=lambda path, name: infos[name]):
                script = bootstrap.render(root, inner, Path(temp) / "unified.exe", "win32-x64")
            section = script.split('Section "-WebCodexUnifiedBootstrap"', 1)[1].split("SectionEnd", 1)[0]
            function_bodies = script.split("Function .onInit", 1)[1].split("FunctionEnd", 1)[0]
            self.assertIn("File /oname=source-manifest.json", section)
            self.assertIn("EnumRegKey $R0 HKLM", section)
            self.assertIn("StrCpy $WebCodexExisting 1", section)
            self.assertIn("SetRegView 64", function_bodies)
            self.assertIn("/ENVIRONMENTDIR=", function_bodies)
            self.assertIn("installer-verify", section)
            self.assertIn("installer-verify-same", section)
            self.assertIn("Goto webcodex_bootstrap_done", section)
            self.assertIn("--expected-runtime-dir", section)
            self.assertLess(section.index("installer-verify-same"), section.index("upgrade-preflight"))
            self.assertIn("upgrade-preflight", section)
            self.assertIn("upgrade-prepare", section)
            self.assertIn("/S /UPDATE /D=$WebCodexInstallDir", section)
            self.assertIn("upgrade-finish", section)
            self.assertIn("upgrade-rollback", script)
            self.assertIn(r'"$WebCodexTrustedCLI" environment upgrade-rollback', script)
            self.assertIn("Get-FileHash -Algorithm SHA256", section)
            self.assertIn("manifest-bound SHA-256 check", section)
            self.assertIn('"$WebCodexTrustedCLI" environment installer-verify-same', section)
            self.assertNotIn('ExecWait \'"$WebCodexCandidate\\artifacts\\bin\\webcodex.exe" environment', section)
            self.assertNotIn("File ", function_bodies)

    def test_rejects_pe_architecture_mismatch_before_rendering_nsis(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / "candidate"
            root.mkdir()
            infos = self.fixture(root)
            (root / "artifacts/bin/webcodex-server.exe").write_bytes(pe(0xAA64))
            with mock.patch.object(nsis.collector, "probe", side_effect=lambda path, name: infos[name]):
                with self.assertRaisesRegex(ValueError, "digest mismatch"):
                    nsis.validate_candidate(root, "win32-x64")


if __name__ == "__main__":
    unittest.main()
