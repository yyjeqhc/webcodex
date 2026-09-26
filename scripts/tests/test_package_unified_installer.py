from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "package_unified_installer.py"
SPEC = importlib.util.spec_from_file_location("package_unified_installer", SCRIPT)
pkg = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(pkg)

SOURCE = "a" * 40
CONTRACT = {"min_generation": 1, "max_generation": 1}

class UnifiedInstallerTests(unittest.TestCase):
    def test_preinstall_rejects_tampered_candidate_cli_before_execution(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package_scripts = root / "pkg-scripts"
            candidate = package_scripts / "upgrade-candidate"
            cli = candidate / "artifacts/bin/webcodex"
            cli.parent.mkdir(parents=True)
            marker = root / "executed"
            cli.write_text(f"#!/bin/sh\nprintf executed > '{marker}'\n", encoding="utf-8")
            cli.chmod(0o755)
            (candidate / "source-manifest.json").write_text("{}\n", encoding="utf-8")
            (candidate / "SHA256SUMS").write_text("digest  source-manifest.json\n", encoding="ascii")
            preinstall = package_scripts / "preinstall"
            preinstall.write_text(pkg._upgrade_preinstall(
                "upgrade-candidate", str(root / "transaction"), str(root / "authorization.json"),
                str(root / "runtime"), "existing_install=0", candidate_cli_sha256="0" * 64,
                hash_checker="/usr/bin/sha256sum",
            ), encoding="utf-8")
            preinstall.chmod(0o755)
            result = subprocess.run([str(preinstall)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(marker.exists())

    def test_authorized_preinstall_retains_candidate_for_failed_owner_finish(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package_scripts = root / "pkg-scripts"
            candidate = package_scripts / "upgrade-candidate"
            (candidate / "artifacts/bin").mkdir(parents=True)
            (candidate / "artifacts/bin/webcodex").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            (candidate / "artifacts/bin/webcodex").chmod(0o755)
            (candidate / "source-manifest.json").write_text("{}\n", encoding="utf-8")
            (candidate / "SHA256SUMS").write_text("digest  source-manifest.json\n", encoding="ascii")
            authorization = root / "authorization.json"
            authorization.write_text("receipt", encoding="utf-8")
            recovery = root / "recovery"
            script = package_scripts / "preinstall"
            script.write_text(
                pkg._upgrade_preinstall(
                    "upgrade-candidate", str(root / "transaction"), str(authorization),
                    str(root / "runtime"), "existing_install=1", str(recovery),
                ),
                encoding="utf-8",
            )
            script.chmod(0o755)
            subprocess.run([str(script)], check=True, capture_output=True)
            self.assertEqual(
                (recovery / "candidate/source-manifest.json").read_bytes(),
                (candidate / "source-manifest.json").read_bytes(),
            )
            self.assertEqual(recovery.stat().st_mode & 0o777, 0o700)

    def test_same_package_preinstall_and_postinstall_are_readonly_and_retryable(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package_scripts = root / "pkg-scripts"
            candidate = package_scripts / "upgrade-candidate"
            (candidate / "artifacts/bin").mkdir(parents=True)
            cli = candidate / "artifacts/bin/webcodex"
            cli.write_text(
                "#!/bin/sh\n"
                "[ \"$1 $2\" = 'environment installer-verify-same' ] || exit 41\n"
                "[ \"${MOCK_VERIFY_RESULT:-0}\" = 0 ] || exit \"$MOCK_VERIFY_RESULT\"\n",
                encoding="utf-8",
            )
            cli.chmod(0o755)
            (candidate / "source-manifest.json").write_text("{}\n", encoding="utf-8")
            (candidate / "SHA256SUMS").write_text("digest  source-manifest.json\n", encoding="ascii")
            authorization = root / "authorization.json"
            recovery = root / "recovery"
            marker = root / "same-package.pending"
            runtime = root / "runtime"
            runtime.mkdir()
            preinstall = package_scripts / "preinstall"
            preinstall.write_text(pkg._upgrade_preinstall(
                "upgrade-candidate", str(root / "transaction"), str(authorization), str(runtime),
                "existing_install=1", str(recovery), str(marker),
            ), encoding="utf-8")
            preinstall.chmod(0o755)
            subprocess.run([str(preinstall)], check=True, capture_output=True)
            self.assertTrue(marker.is_file())
            self.assertTrue((recovery / "candidate/source-manifest.json").is_file())

            postinstall = root / "postinstall"
            postinstall.write_text(pkg._upgrade_postinstall(
                str(cli), str(root / "transaction"), str(authorization), str(recovery), str(runtime), str(marker),
            ), encoding="utf-8")
            postinstall.chmod(0o755)
            failed = subprocess.run([str(postinstall)], env=dict(os.environ, MOCK_VERIFY_RESULT="17"), capture_output=True)
            self.assertEqual(failed.returncode, 1)
            self.assertTrue(marker.exists())
            self.assertTrue((recovery / "candidate/source-manifest.json").exists())
            completed = subprocess.run([str(postinstall)], capture_output=True)
            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            self.assertFalse(marker.exists())
            self.assertFalse((recovery / "candidate").exists())

    def test_authorized_postinstall_finishes_owner_transaction_and_keeps_failed_receipt(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            authorization = root / "authorization.json"
            authorization.write_text("receipt", encoding="utf-8")
            mock_cli = root / "webcodex"
            mock_cli.write_text(
                "#!/bin/sh\n"
                "[ \"$1 $2\" = 'environment installer-finish' ] || exit 41\n"
                "[ \"${MOCK_FINISH_RESULT:-0}\" = 0 ] || exit \"$MOCK_FINISH_RESULT\"\n"
                "rm -f \"$MOCK_AUTHORIZATION_FILE\"\n",
                encoding="utf-8",
            )
            mock_cli.chmod(0o755)
            script = root / "postinstall"
            script.write_text(
                pkg._upgrade_postinstall(str(mock_cli), str(root / "transaction"), str(authorization)),
                encoding="utf-8",
            )
            script.chmod(0o755)
            env = dict(os.environ, MOCK_AUTHORIZATION_FILE=str(authorization))
            failed = subprocess.run([str(script)], env=dict(env, MOCK_FINISH_RESULT="17"), capture_output=True)
            self.assertEqual(failed.returncode, 1)
            self.assertTrue(authorization.exists(), "failed owner completion must preserve recovery receipt")
            self.assertIn(b"receipt is retained", failed.stderr)
            completed = subprocess.run([str(script)], env=env, capture_output=True)
            self.assertEqual(completed.returncode, 0, completed.stderr.decode())
            self.assertFalse(authorization.exists(), "the Core finish command commits and clears its receipt")

    def make_fixture(self, root: Path, platform_name: str = "linux-x64") -> tuple[Path, Path, dict]:
        os_name, target, architecture, _ = pkg.PLATFORMS[platform_name]
        (root / "artifacts" / "bin").mkdir(parents=True)
        app = root / ("WebCodexDesktop.app" if os_name == "darwin" else "desktop")
        if os_name == "darwin":
            binary_dir = app / "Contents/MacOS"
            resources = app / "Contents/Resources/webcodex-runtime"
            binary_dir.mkdir(parents=True)
            resources.mkdir(parents=True)
            desktop_path = binary_dir / "webcodex-desktop"
            desktop_exec = "Contents/MacOS/webcodex-desktop"
        else:
            app = root / "artifacts" / "bin" / "webcodex-desktop"
            desktop_path = app
            desktop_exec = app.name
        artifacts = {}
        for index, name in enumerate(pkg.BINARIES):
            info = {
                "schema_version": 1, "binary": name, "version": "0.8.1",
                "git_commit": SOURCE, "git_dirty": False, "built_at": str(100 + index),
                "target": target, "architecture": architecture,
                "desktop_runtime_contract": CONTRACT, "environment_data_format": 1,
            }
            path = desktop_path if name == "webcodex-desktop" else root / "artifacts" / "bin" / name
            if name == "webcodex-desktop" or name in pkg.RUNTIMES:
                path.write_text("#!/usr/bin/env python3\nimport json\nprint(" + repr(json.dumps(info)) + ")\n", encoding="utf-8")
                path.chmod(0o755)
            if os_name == "darwin" and name in pkg.RUNTIMES:
                shutil.copy2(path, resources / name)
            artifacts[name] = {
                "path": path.relative_to(root).as_posix(),
                "sha256": pkg.sha256_file(path),
                "build_info": info,
                "build_info_sha256": pkg.canonical_digest(info),
                "probe": "local-executable" if platform_name == "linux-x64" and name in pkg.RUNTIMES else "native-build-job",
            }
        manifest = {
            "schema_version": 1, "version": "0.8.1", "source_sha": SOURCE,
            "source_workflow_run_id": 123456, "source_workflow_ref": "yyjeqhc/webcodex/.github/workflows/release-build.yml@refs/tags/v0.8.1",
            "platform": platform_name, "target": target, "architecture": architecture,
            "desktop_runtime_contract": CONTRACT, "artifacts": artifacts,
            "desktop_payload": {
                "path": app.relative_to(root).as_posix(),
                "sha256": pkg.tree_digest(app),
                "executable": desktop_exec,
            },
        }
        manifest_path = root / "source-manifest.json"
        encoded = (json.dumps(manifest, separators=(",", ":")) + "\n").encode()
        manifest_path.write_bytes(encoded)
        sums = root / "SHA256SUMS"
        sum_lines = [f"{item['sha256']}  {item['path']}" for item in artifacts.values()]
        sum_lines.append(f"{hashlib.sha256(encoded).hexdigest()}  source-manifest.json")
        sums.write_text("\n".join(sorted(sum_lines)) + "\n", encoding="ascii")
        return manifest_path, sums, manifest

    def test_builds_testfixture_deb_and_keeps_cli_on_path_without_start_scripts(self):
        if not shutil.which("dpkg-deb"):
            self.skipTest("dpkg-deb is unavailable")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            manifest, sums, _ = self.make_fixture(root)
            output = root / "out" / "webcodex.deb"
            args = pkg.make_parser().parse_args([
                "--platform", "linux-x64", "--input-root", str(root),
                "--manifest", str(manifest), "--checksums", str(sums), "--output", str(output),
            ])
            result = pkg.package(args)
            self.assertEqual(result["package"], "deb")
            contents = subprocess.run(["dpkg-deb", "--contents", str(output)], check=True, capture_output=True, text=True).stdout
            self.assertIn("usr/bin/webcodex", contents)
            self.assertIn("usr/bin/webcodex-server", contents)
            self.assertIn("usr/bin/webcodex-runner", contents)
            extracted = root / "extracted"
            subprocess.run(["dpkg-deb", "--extract", str(output), str(extracted)], check=True)
            self.assertEqual(os.readlink(extracted / "usr/bin/webcodex"), "../lib/webcodex/webcodex-runtime/webcodex")
            self.assertEqual(os.readlink(extracted / "usr/bin/webcodex-server"), "../lib/webcodex/webcodex-runtime/webcodex-server")
            self.assertIn("usr/lib/webcodex/webcodex-desktop", contents)
            self.assertIn("usr/lib/webcodex/webcodex-runtime/webcodex-runner", contents)
            control = subprocess.run(["dpkg-deb", "--field", str(output)], check=True, capture_output=True, text=True).stdout
            self.assertIn("libwebkit2gtk-4.1-0", control)
            self.assertFalse(any(line.startswith("postinst") for line in contents.splitlines()))
            control_dir = root / "control"
            subprocess.run(["dpkg-deb", "--control", str(output), str(control_dir)], check=True)
            preinst = (control_dir / "preinst").read_text(encoding="utf-8")
            subprocess.run(["sh", "-n", str(control_dir / "preinst")], check=True)
            self.assertIn("dpkg-query -W", preinst)
            self.assertIn("--environment-dir '/var/lib/webcodex-installer/transaction'", preinst)
            self.assertIn("/var/lib/webcodex-installer/authorization.json", preinst)
            self.assertIn("installer-verify", preinst)
            self.assertIn("installer-verify-same", preinst)
            self.assertIn("systemd", preinst)
            self.assertIn('candidate="$script_dir/upgrade-candidate"', preinst)
            postinst = (control_dir / "postinst").read_text(encoding="utf-8")
            subprocess.run(["sh", "-n", str(control_dir / "postinst")], check=True)
            self.assertIn("upgrade-finish", postinst)
            self.assertIn("environment installer-finish", postinst)
            self.assertIn("/var/lib/webcodex-installer/same-package.pending", postinst)
            self.assertIn("/var/lib/webcodex-installer/recovery/candidate", postinst)
            self.assertIn("recovery='/var/lib/webcodex-installer/recovery'", preinst)
            self.assertIn("--environment-dir '/var/lib/webcodex-installer/transaction'", postinst)
            self.assertIn("authorization receipt is retained", postinst)
            self.assertNotIn("upgrade-rollback", postinst)
            self.assertTrue((control_dir / "upgrade-candidate/source-manifest.json").is_file())
            self.assertTrue((control_dir / "upgrade-candidate/artifacts/bin/webcodex-server").is_file())

    def test_rejects_unknown_environment_data_format(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            manifest_path, sums_path, manifest = self.make_fixture(root)
            manifest["artifacts"]["webcodex"]["build_info"]["environment_data_format"] = 2
            record = manifest["artifacts"]["webcodex"]
            record["build_info_sha256"] = pkg.canonical_digest(record["build_info"])
            binary = root / record["path"]
            binary.write_text(
                "#!/usr/bin/env python3\nimport json\nprint(" + repr(json.dumps(record["build_info"])) + ")\n",
                encoding="utf-8",
            )
            binary.chmod(0o755)
            record["sha256"] = pkg.sha256_file(binary)
            encoded = json.dumps(manifest, separators=(",", ":")).encode() + b"\n"
            manifest_path.write_bytes(encoded)
            entries = pkg.parse_sha256sums(sums_path)
            entries[record["path"]] = record["sha256"]
            entries["source-manifest.json"] = hashlib.sha256(encoded).hexdigest()
            sums_path.write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(entries.items())), encoding="ascii")
            with self.assertRaisesRegex(pkg.PackageError, "environment data format"):
                pkg.validate_manifest(manifest_path, sums_path, root, "linux-x64")

    def test_rejects_artifact_sha_mismatch_before_package_output(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            manifest, sums, _ = self.make_fixture(root)
            (root / "artifacts/bin/webcodex-server").write_text("changed\n", encoding="utf-8")
            parsed = pkg.make_parser().parse_args([
                "--platform", "linux-x64", "--input-root", str(root),
                "--manifest", str(manifest), "--checksums", str(sums), "--output", str(root / "out.deb"),
                "--dry-run",
            ])
            with self.assertRaisesRegex(pkg.PackageError, "SHA-256 mismatch"):
                pkg.package(parsed)

    def test_accepts_native_build_job_sidecars_for_cross_architecture(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            manifest, sums, _ = self.make_fixture(root, "linux-arm64")
            parsed = pkg.validate_manifest(manifest, sums, root, "linux-arm64")
            self.assertEqual(parsed["target"], "aarch64-unknown-linux-gnu")

    def test_mac_dry_run_stages_app_runtime_and_path_wrappers(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            manifest, sums, metadata = self.make_fixture(root, "darwin-arm64")
            parsed = pkg.validate_manifest(manifest, sums, root, "darwin-arm64")
            stage = root / "stage"
            scripts = root / "pkg-scripts"
            stage.mkdir()
            pkg.stage_macos(parsed, stage, scripts, root)
            subprocess.run(["sh", "-n", str(scripts / "preinstall")], check=True)
            subprocess.run(["sh", "-n", str(scripts / "postinstall")], check=True)
            self.assertTrue((stage / "Applications/WebCodex Desktop.app/Contents/Resources/webcodex-runtime/webcodex-server").is_file())
            self.assertEqual(os.readlink(stage / "usr/local/bin/webcodex"), "/Library/Application Support/WebCodex/runtime/webcodex")
            self.assertIn("upgrade-preflight", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("upgrade-prepare", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("--environment-dir '/Library/Application Support/WebCodex/installer-transaction'", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("pkgutil --pkg-info", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("installer-verify", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("installer-verify-same", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("/Library/Application Support/WebCodexInstaller/same-package.pending", (scripts / "postinstall").read_text(encoding="utf-8"))
            self.assertIn("/Library/Application Support/WebCodexInstaller/authorization.json", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("LaunchDaemons", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertIn("upgrade-finish", (scripts / "postinstall").read_text(encoding="utf-8"))
            self.assertIn("environment installer-finish", (scripts / "postinstall").read_text(encoding="utf-8"))
            self.assertIn("authorization receipt is retained", (scripts / "postinstall").read_text(encoding="utf-8"))
            self.assertIn("recovery='/Library/Application Support/WebCodexInstaller/recovery'", (scripts / "preinstall").read_text(encoding="utf-8"))
            self.assertNotIn("upgrade-rollback", (scripts / "postinstall").read_text(encoding="utf-8"))
            self.assertFalse((scripts / "upgrade-candidate/source-manifest.json").is_symlink())

    def test_mac_pkg_dry_run_is_reviewable_on_non_macos_host(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            manifest, sums, _ = self.make_fixture(root, "darwin-arm64")
            args = pkg.make_parser().parse_args([
                "--platform", "darwin-arm64", "--input-root", str(root),
                "--manifest", str(manifest), "--checksums", str(sums),
                "--output", str(root / "WebCodex.pkg"), "--dry-run",
            ])
            result = pkg.package(args)
            self.assertEqual(result["package"], "pkg")
            self.assertIn("Applications/WebCodex Desktop.app/Contents/MacOS/webcodex-desktop", result["entries"])
            self.assertIn("usr/local/bin/webcodex", result["entries"])
            self.assertNotIn("postinstall", result["entries"])
            self.assertIn("upgrade-preflight", result["preinstall"])
            self.assertIn("upgrade-prepare", result["preinstall"])
            self.assertIn("installer-verify", result["preinstall"])
            self.assertFalse((root / "WebCodex.pkg").exists())

if __name__ == "__main__":
    unittest.main()
