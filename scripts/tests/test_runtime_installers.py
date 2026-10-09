from __future__ import annotations

import copy
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import collect_unified_installer_inputs as inputs
from scripts import package_unified_installer as packaging
from scripts import prepare_release_metadata as metadata
from scripts import runtime_installer_manifest as contract
from scripts import collect_release_bundle as collector
from scripts import verify_public_release as public
from scripts import release_plan as plan
from scripts import release_publication as publication
from scripts.tests import test_collect_release_bundle as old_bundle
from scripts.tests import test_release_plan as old_plan
from scripts.tests import test_release_publication as old_publication

VERSION = "0.8.1"
SOURCE = "a" * 40
REPO = "yyjeqhc/webcodex"
WORKFLOW = f"{REPO}/.github/workflows/release-build.yml@refs/tags/v{VERSION}"


def info(name: str, platform: str, version: str = VERSION) -> dict:
    _, target, architecture, _ = inputs.NATIVE_TARGETS[platform]
    return {"schema_version": 1, "binary": name, "version": version, "git_commit": SOURCE,
            "git_dirty": False, "built_at": "123", "target": target, "architecture": architecture,
            "desktop_runtime_contract": {"min_generation": 1, "max_generation": 1}, "environment_data_format": 1}


def source(platform: str, payloads: dict[str, bytes], version: str = VERSION, run: int = 123) -> dict:
    identity = info("webcodex", platform, version)
    records = {}
    for name, payload in payloads.items():
        build = info(name, platform, version)
        records[name] = {"path": f"artifacts/bin/{name}", "sha256": hashlib.sha256(payload).hexdigest(),
                         "build_info": build, "build_info_sha256": packaging.canonical_digest(build), "probe": "native-build-job"}
    value = {"schema_version": 2, "package_flavor": "runtime", "version": version, "source_sha": SOURCE,
             "source_workflow_run_id": run, "source_workflow_ref": f"{REPO}/.github/workflows/release-build.yml@refs/tags/v{version}",
             "platform": platform, "target": identity["target"], "architecture": identity["architecture"],
             "desktop_runtime_contract": identity["desktop_runtime_contract"], "artifacts": records}
    return value


class RuntimeInstallerTests(unittest.TestCase):
    def test_native_collector_never_needs_or_copies_desktop_and_flavor_cannot_be_inferred(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for name in packaging.RUNTIMES:
                path = root / name
                path.write_text("#!/usr/bin/env python3\nprint(" + repr(json.dumps(info(name, "linux-x64"))) + ")\n")
                path.chmod(0o755)
            args = inputs.make_parser().parse_args([
                "--package-flavor", "runtime", "--platform", "linux-x64", "--input-root", str(root),
                "--webcodex", "webcodex", "--webcodex-server", "webcodex-server", "--webcodex-runner", "webcodex-runner",
                "--version", VERSION, "--source-sha", SOURCE, "--workflow-run-id", "123", "--workflow-ref", WORKFLOW,
                "--output-dir", str(root / "candidate")])
            with mock.patch.object(inputs, "detect_platform", return_value="linux-x64"), mock.patch.dict(os.environ, {"GITHUB_RUN_ID": "123", "GITHUB_WORKFLOW_REF": WORKFLOW, "GITHUB_SHA": SOURCE}):
                result = inputs.collect(args)
            candidate = Path(result["source_manifest"]).parent
            manifest = json.loads((candidate / "source-manifest.json").read_text())
            self.assertEqual(set(manifest["artifacts"]), set(packaging.RUNTIMES))
            self.assertEqual(manifest["schema_version"], 2)
            self.assertNotIn("desktop_payload", manifest)
            # The default Full reader must reject this exact checksummed source.
            with self.assertRaises(packaging.PackageError):
                packaging.validate_manifest(candidate / "source-manifest.json", candidate / "SHA256SUMS", candidate, "linux-x64")
            accepted = packaging.validate_manifest(candidate / "source-manifest.json", candidate / "SHA256SUMS", candidate, "linux-x64", "runtime")
            for fmt in ("deb", "rpm"):
                stage = root / fmt
                stage.mkdir()
                packaging.stage_linux_payload(accepted, stage)
                self.assertFalse((stage / "usr/lib/webcodex/webcodex-desktop").exists())
                self.assertFalse((stage / "usr/share/applications").exists())
                for name in packaging.RUNTIMES:
                    self.assertEqual((stage / f"usr/lib/webcodex/webcodex-runtime/{name}").read_bytes(), (root / name).read_bytes())
                if fmt == "deb":
                    packaging.stage_deb_metadata(accepted, stage, candidate)
                    control = (stage / "DEBIAN/control").read_text()
                    self.assertIn("Package: webcodex-runtime\n", control)
                    self.assertIn("Conflicts: webcodex\n", control)
                    self.assertNotIn("gtk", control)
                    self.assertNotIn("webkit", control)
                    self.assertNotIn("Replaces:", control)
                    self.assertIn("installer-finish", (stage / "DEBIAN/postinst").read_text())
                    self.assertIn("--installer-target linux-x64-runtime-deb --json", (stage / "DEBIAN/preinst").read_text())
                    if shutil.which("dpkg-deb"):
                        output = root / "runtime.deb"
                        subprocess.run(["dpkg-deb", "--build", "--root-owner-group", str(stage), str(output)], stdin=subprocess.DEVNULL, capture_output=True, check=True)
                        listing = subprocess.run(["dpkg-deb", "--contents", str(output)], capture_output=True, check=True).stdout
                        self.assertNotIn(b"webcodex-desktop", listing)
                else:
                    spec = packaging.rpm_spec(accepted, stage)
                    self.assertIn("Name: webcodex-runtime\n", spec)
                    self.assertIn("Conflicts: webcodex\n", spec)
                    self.assertIn("/usr/share/webcodex-runtime/upgrade-candidate", spec)
                    self.assertNotIn("/usr/lib/webcodex/webcodex-desktop", spec.split("%files", 1)[1])
                    self.assertNotIn("Obsoletes:", spec)
                    self.assertIn("--installer-target linux-x64-runtime-rpm --json", spec)

    @unittest.skipUnless(
        inputs.detect_platform() in ("linux-x64", "linux-arm64")
        and all(shutil.which(tool) for tool in ("rpmbuild", "rpm", "rpm2cpio", "cpio", "objcopy", "true")),
        "native RPM packaging tools are unavailable",
    )
    def test_rpm_preserves_prebuilt_payload_and_candidate_bytes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            comment = root / "comment"
            comment.write_bytes(b"WebCodex immutable RPM candidate\0")
            binary = root / "prebuilt"
            # An already stripped ELF still has sections that RPM's default
            # brp-strip-comment-note would rewrite after our manifest was hashed.
            subprocess.run(
                ["objcopy", "--remove-section", ".comment", "--add-section", f".comment={comment}",
                 str(Path(shutil.which("true")).resolve()), str(binary)],
                stdin=subprocess.DEVNULL, capture_output=True, check=True,
            )
            payload = binary.read_bytes()
            platform = inputs.detect_platform()
            for flavor in ("runtime", "full"):
                with self.subTest(flavor=flavor):
                    case = root / flavor
                    candidate = case / "candidate"
                    candidate.mkdir(parents=True)
                    names = packaging.RUNTIMES if flavor == "runtime" else packaging.BINARIES
                    manifest = source(platform, {name: payload for name in names})
                    paths = {}
                    for name in names:
                        path = candidate / f"artifacts/bin/{name}"
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_bytes(payload)
                        path.chmod(0o755)
                        paths[name] = path
                    if flavor == "full":
                        manifest.pop("package_flavor")
                        manifest["schema_version"] = 1
                        manifest["desktop_payload"] = {
                            "path": "artifacts/bin/webcodex-desktop",
                            "executable": "webcodex-desktop",
                            "sha256": packaging.tree_digest(paths["webcodex-desktop"]),
                        }
                    encoded = (json.dumps(manifest, indent=2) + "\n").encode()
                    (candidate / "source-manifest.json").write_bytes(encoded)
                    sums = [f"{record['sha256']}  {record['path']}" for record in manifest["artifacts"].values()]
                    sums.append(f"{hashlib.sha256(encoded).hexdigest()}  source-manifest.json")
                    (candidate / "SHA256SUMS").write_text("\n".join(sums) + "\n")
                    manifest["_artifacts"] = paths
                    if flavor == "full":
                        manifest["_desktop_payload"] = paths["webcodex-desktop"]
                    stage = case / "payload"
                    stage.mkdir()
                    packaging.stage_linux_payload(manifest, stage)
                    package_name = "webcodex-runtime" if flavor == "runtime" else "webcodex"
                    share = stage / "usr/share" / package_name
                    share.mkdir(parents=True)
                    shutil.copyfile(stage / "usr/share/doc" / package_name / "unified-source-manifest.json",
                                    share / "unified-source-manifest.json")
                    packaging._copy_upgrade_candidate(candidate, share / "upgrade-candidate", manifest)
                    output = case / "installer.rpm"
                    packaging.package_rpm(manifest, stage, output, case)
                    archive = subprocess.run(
                        ["rpm2cpio", str(output)], stdin=subprocess.DEVNULL,
                        capture_output=True, check=True,
                    )
                    extracted = case / "extracted"
                    extracted.mkdir()
                    subprocess.run(
                        ["cpio", "-id", "--quiet", "--no-absolute-filenames"],
                        input=archive.stdout, cwd=extracted, capture_output=True, check=True,
                    )
                    for name in names:
                        installed = (extracted / "usr/lib/webcodex/webcodex-desktop" if name == "webcodex-desktop"
                                     else extracted / "usr/lib/webcodex/webcodex-runtime" / name)
                        retained = extracted / "usr/share" / package_name / "upgrade-candidate/artifacts/bin" / name
                        self.assertEqual(installed.read_bytes(), payload, f"rewritten installed {name}")
                        self.assertEqual(retained.read_bytes(), payload, f"rewritten retained candidate {name}")
                    scripts = subprocess.run(
                        ["rpm", "-qp", "--scripts", str(output)], stdin=subprocess.DEVNULL,
                        capture_output=True, check=True, text=True,
                    ).stdout
                    self.assertIn("environment installer-verify", scripts)
                    self.assertIn("environment installer-finish", scripts)

    def test_runtime_authorized_hooks_bind_literal_architecture_and_package_manager(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for platform in ("linux-x64", "linux-arm64"):
                candidate = root / platform
                candidate.mkdir()
                payloads = {name: name.encode() for name in packaging.RUNTIMES}
                manifest = source(platform, payloads)
                (candidate / "source-manifest.json").write_text(json.dumps(manifest))
                (candidate / "SHA256SUMS").write_text("fixture")
                for name, payload in payloads.items():
                    path = candidate / f"artifacts/bin/{name}"
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(payload)
                stage = root / f"{platform}-stage"
                stage.mkdir()
                packaging.stage_deb_metadata(manifest, stage, candidate)
                deb_hook = (stage / "DEBIAN/preinst").read_text()
                deb_post = (stage / "DEBIAN/postinst").read_text()
                rpm_spec = packaging.rpm_spec(manifest, stage)
                rpm_hook = rpm_spec.split("%pre\n", 1)[1].split("%post\n", 1)[0]
                rpm_post = rpm_spec.split("%post\n", 1)[1].split("%files\n", 1)[0]
                for fmt, pre, post in (("deb", deb_hook, deb_post), ("rpm", rpm_hook, rpm_post)):
                    for hook, count in ((pre, 2), (post, 1)):
                        commands = [line for line in hook.splitlines() if " environment installer-verify " in line or " environment installer-verify-same " in line]
                        self.assertEqual(len(commands), count)
                        for command in commands:
                            self.assertIn(f"--installer-target {platform}-runtime-{fmt} --json", command)
                        self.assertEqual(hook.count("--installer-target"), count)
                        self.assertEqual(subprocess.run(["sh", "-n"], input=hook, text=True, capture_output=True).returncode, 0)
            # Existing Full/PKG helpers emit their unchanged argument shape.
            self.assertNotIn("--installer-target", packaging.rpm_preinstall())
            self.assertNotIn("--installer-target", packaging.rpm_postinstall())
            self.assertNotIn("--installer-target", packaging._upgrade_preinstall("candidate", "transaction", "authorization", "runtime"))
            self.assertNotIn("--installer-target", packaging._upgrade_postinstall("cli", "transaction", "authorization"))
            with self.assertRaises(packaging.PackageError):
                packaging.rpm_preinstall("linux-x64-runtime-deb;false")

    def test_v2_binds_exact_legacy_view_and_runtime_sources_and_rejects_tamper(self):
        legacy = {"version": VERSION, "binaries": list(packaging.RUNTIMES), "artifacts": {}, "installers": {"linux-x64-deb": {"fixture": True}}}
        canonical = {"schema_version": 2, **legacy, "installers": copy.deepcopy(legacy["installers"])}
        for target, (platform, fmt) in contract.RUNTIME_TARGETS.items():
            filename = contract.installer_filename(VERSION, target)
            base = f"https://github.com/{REPO}/releases/download/v{VERSION}/"
            canonical["installers"][target] = {"platform": platform, "format": fmt, "flavor": "runtime", "filename": filename,
                "url": base + filename, "sha256": "a" * 64, "source_manifest_url": base + contract.source_filename(VERSION, platform), "source_manifest_sha256": "b" * 64}
        self.assertEqual(contract.legacy_projection(canonical), legacy)
        validate = lambda value, old=legacy: contract.validate(value, old, version=VERSION, repo=REPO, full_targets={"linux-x64-deb"})
        self.assertEqual(len(validate(canonical)), 4)
        for mutation in ("version", "schema_version", "source_manifest_sha256", "flavor", "filename"):
            changed = copy.deepcopy(canonical)
            if mutation in ("version", "schema_version"):
                changed[mutation] = "changed"
            else:
                changed["installers"]["linux-x64-runtime-deb"][mutation] = "changed"
            with self.assertRaises(ValueError, msg=mutation):
                validate(changed)
        old = copy.deepcopy(legacy); old["binaries"].append("webcodex-desktop")
        with self.assertRaises(ValueError):
            validate(canonical, old)

    def test_complete_32_checksum_bundle_and_frozen_full_reader_view(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            version = old_bundle.VERSION
            stem, _ = old_bundle._write_bundle(root, f"v{version}", "release", unified=True)
            build = json.loads((root / "release-build.json").read_text())
            legacy = json.loads((root / "manifest.json").read_text())
            canonical = {"schema_version": 2, **legacy, "installers": copy.deepcopy(legacy["installers"])}
            for platform in ("linux-x64", "linux-arm64"):
                payloads = {name: f"{platform}:{name}".encode() for name in packaging.RUNTIMES}
                value = source(platform, payloads, version, old_bundle.RUN_ID)
                path = root / contract.source_filename(version, platform)
                path.write_text(json.dumps(value))
                source_hash = metadata.sha256(path)
                for fmt in ("deb", "rpm"):
                    target = f"{platform}-runtime-{fmt}"
                    filename = contract.installer_filename(version, target)
                    (root / filename).write_bytes((b"!<arch>\n" if fmt == "deb" else bytes.fromhex("edabeedb")) + b"fixture")
                    digest = metadata.sha256(root / filename)
                    build["installer_artifacts"][target] = {"filename": filename, "sha256": digest, "flavor": "runtime", "source_manifest_filename": path.name, "source_manifest_sha256": source_hash}
                    base = f"https://github.com/{REPO}/releases/download/v{version}/"
                    canonical["installers"][target] = {"platform": platform, "format": fmt, "flavor": "runtime", "filename": filename,
                        "url": base + filename, "sha256": digest, "source_manifest_url": base + path.name, "source_manifest_sha256": source_hash}
            (root / "manifest-v2.json").write_text(contract.encode(canonical))
            (root / "manifest.json").write_text(contract.encode(contract.legacy_projection(canonical)))
            build["installer_manifest_v2"] = {"filename": "manifest-v2.json", "sha256": metadata.sha256(root / "manifest-v2.json")}
            runtime = {"schema_version": 1, "release_version": version, "runtime_version": version, "desktop_runtime_contract": {"min_generation": 1, "max_generation": 1}}
            (root / "webcodex-release-manifest.json").write_text(json.dumps(runtime))
            build["runtime_manifest"] = {"filename": "webcodex-release-manifest.json", "sha256": metadata.sha256(root / "webcodex-release-manifest.json")}
            (root / "release-build.json").write_text(json.dumps(build))
            names = {path.name for path in root.iterdir()} - {"SHA256SUMS", "release-build.json", "linux-x64-elf.txt", "linux-arm64-elf.txt"}
            self.assertEqual(len(names), 32)
            sums = "".join(f"{metadata.sha256(root / name)}  {name}\n" for name in sorted(names))
            (root / "SHA256SUMS").write_text(sums)
            verify = lambda: collector.verify_bundle_directory(root, repo=REPO, run_id=old_bundle.RUN_ID, expected_source_sha=SOURCE, expected_tag=f"v{version}", artifact_name=f"{stem}-bundle", require_unified_installers=True, require_runtime_installers=True)
            self.assertEqual(len(verify()["installer_artifacts"]), 12)
            parsed = public.parse_sha256sums(sums, version, runtime_manifest=True, unified_installers=True, runtime_installers=True)
            self.assertEqual(len(parsed), 32)
            # The retained old parser continues seeing exactly six three-binary
            # archives and eight Full installers, with no new fields or targets.
            public.validate_public_manifest(legacy, version)
            self.assertEqual(len(public.validate_public_installers(legacy, version)), 8)
            for extra in ("extra.sha256", "provenance.json", "supplemental.dmg"):
                with self.assertRaises(public.VerificationError):
                    public.parse_sha256sums(sums + f"{'a' * 64}  {extra}\n", version, runtime_manifest=True, unified_installers=True, runtime_installers=True)
            # Even a checksummed v2 mutation cannot fall back to the intact view.
            canonical["schema_version"] = 3
            (root / "manifest-v2.json").write_text(contract.encode(canonical))
            digest = metadata.sha256(root / "manifest-v2.json")
            build["installer_manifest_v2"]["sha256"] = digest
            (root / "release-build.json").write_text(json.dumps(build))
            (root / "SHA256SUMS").write_text("".join(f"{metadata.sha256(root / name)}  {name}\n" for name in sorted(names)))
            with self.assertRaises(collector.CollectionError):
                verify()

    def test_selection_requires_full_before_any_dispatch(self):
        with self.assertRaises(publication.PublicationError):
            publication.start_build(repo=REPO, source_sha=SOURCE, tag=f"v{VERSION}", state_file=Path("unused"), timeout=1, resolve_secs=0, include_runtime_installers=True)
        with self.assertRaises(collector.CollectionError):
            collector.verify_bundle_directory(Path("absent"), repo=REPO, run_id=123, expected_source_sha=SOURCE, expected_tag=f"v{VERSION}", artifact_name="absent", require_runtime_installers=True)

    def test_preparer_produces_v2_and_exact_legacy_bytes_with_explicit_selection(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            artifact = root / "artifacts"; artifact.mkdir()
            for platform in metadata.PLATFORMS:
                payloads = {name: f"{platform}:{name}".encode() for name in packaging.RUNTIMES}
                with tarfile.open(artifact / metadata.archive_filename(VERSION, platform), "w:gz") as archive:
                    for name, payload in payloads.items():
                        member = tarfile.TarInfo(name + (".exe" if platform.startswith("win32-") else "")); member.size = len(payload)
                        archive.addfile(member, io.BytesIO(payload))
                full = source(platform, {**payloads, "webcodex-desktop": b"desktop"})
                full.pop("package_flavor"); full["schema_version"] = 1; full["desktop_payload"] = {}
                (artifact / metadata.source_manifest_filename(VERSION, platform)).write_text(json.dumps(full))
                if platform.startswith("linux-"):
                    (artifact / contract.source_filename(VERSION, platform)).write_text(json.dumps(source(platform, payloads)))
            for platform in metadata.desktop_platforms_for_version(VERSION):
                (artifact / metadata.desktop_filename(VERSION, platform)).write_bytes(b"desktop")
            for target, (_platform, fmt) in (metadata.INSTALLER_TARGETS | contract.RUNTIME_TARGETS).items():
                name = contract.installer_filename(VERSION, target) if target in contract.RUNTIME_TARGETS else metadata.installer_filename(VERSION, target)
                (artifact / name).write_bytes({"deb": b"!<arch>\n", "rpm": bytes.fromhex("edabeedb"), "pkg": b"xar!", "exe": b"MZ"}[fmt] + b"package")
            package = root / "package.json"; package.write_text(json.dumps({"version": VERSION}))
            output = root / "output"
            args = [sys.executable, str(metadata.ROOT / "scripts/prepare_release_metadata.py"), "--version", VERSION,
                    "--source-sha", SOURCE, "--workflow-run-id", "123", "--workflow-ref", WORKFLOW,
                    "--artifact-dir", str(artifact), "--output-dir", str(output), "--package-json", str(package)]
            self.assertNotEqual(subprocess.run(args + ["--require-unified-installers"], capture_output=True).returncode, 0)
            result = subprocess.run(args + ["--require-unified-installers", "--require-runtime-installers"], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            canonical = json.loads((output / "manifest-v2.json").read_text())
            legacy = json.loads((output / "manifest.json").read_text())
            self.assertEqual(len(canonical["installers"]), 12)
            self.assertEqual(len(legacy["installers"]), 8)
            self.assertEqual((output / "manifest.json").read_bytes(), contract.encode(contract.legacy_projection(canonical)).encode())
            # The existing contract generator appends exactly one record.
            self.assertEqual(len((output / "SHA256SUMS").read_text().splitlines()), 31)
            changed = artifact / contract.source_filename(VERSION, "linux-x64")
            value = json.loads(changed.read_text()); value["artifacts"]["webcodex"]["sha256"] = "0" * 64
            changed.write_text(json.dumps(value))
            self.assertNotEqual(subprocess.run(args + ["--require-unified-installers", "--require-runtime-installers"], capture_output=True).returncode, 0)

    def test_durable_runtime_selection_is_preserved_and_old_schemas_default_false(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            state_path = root / "state.json"
            for enabled in (False, True):
                value = old_publication._state(); value["include_unified_installers"] = True; value["include_runtime_installers"] = enabled
                publication._write_state(state_path, value)
                self.assertIs(publication._load_state(state_path)["include_runtime_installers"], enabled)
            value["schema_version"] = 2; value.pop("include_runtime_installers")
            publication._write_state(state_path, value)
            self.assertFalse(publication._load_state(state_path)["include_runtime_installers"])
            value = old_plan._state(root, phase=plan.PHASE_AWAIT_TAG)
            value["require_runtime_installers"] = True
            plan._write_state(state_path, value)
            self.assertTrue(plan._load_state(state_path)["require_runtime_installers"])
            value["schema_version"] = 3; value.pop("require_runtime_installers")
            plan._write_state(state_path, value)
            loaded = plan._load_state(state_path)
            self.assertTrue(loaded["require_unified_installers"])
            self.assertFalse(loaded["require_runtime_installers"])
            loaded["require_runtime_installers"] = True
            with self.assertRaises(plan.ReleasePlanError):
                plan._require_build_selection(loaded, {"include_unified_installers": True, "include_runtime_installers": False})

    def test_runtime_native_job_reuses_archives_and_contains_no_desktop_toolchain(self):
        text = (metadata.ROOT / ".github/workflows/release-build.yml").read_text()
        job = text.split("  runtime-native:\n", 1)[1].split("  unified-native:\n", 1)[0]
        self.assertIn("--package-flavor runtime", job)
        self.assertIn("runtime-input/$ARCHIVE_STEM-$PLATFORM.tar.gz", job)
        self.assertIn("linux-arm64", job)
        for forbidden in ("cargo build", "npm ci", "libgtk", "libwebkit", "webcodex-desktop"):
            self.assertNotIn(forbidden, job)


if __name__ == "__main__":
    unittest.main()
