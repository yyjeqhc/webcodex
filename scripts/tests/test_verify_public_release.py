from __future__ import annotations

import io
import hashlib
import json
import struct
import tarfile
import tempfile
import unittest
from pathlib import Path

from scripts import verify_public_release as verifier


def synthetic_pe(machine: int) -> bytes:
    data = bytearray(128)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 64)
    data[64:68] = b"PE\0\0"
    struct.pack_into("<H", data, 68, machine)
    return bytes(data)


def synthetic_elf(machine: int, glibc: str = "2.17", needed: tuple[str, ...] = ("libc.so.6",)) -> bytes:
    data = bytearray(1024)
    data[:4] = b"\x7fELF"
    data[4] = 2
    data[5] = 1
    data[6] = 1
    struct.pack_into("<H", data, 16, 2)
    struct.pack_into("<H", data, 18, machine)
    struct.pack_into("<I", data, 20, 1)
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<H", data, 52, 64)
    struct.pack_into("<H", data, 54, 56)
    struct.pack_into("<H", data, 56, 2)

    dynstr = bytearray(b"\0")
    needed_offsets = []
    for library in needed:
        needed_offsets.append(len(dynstr))
        dynstr.extend(library.encode("ascii") + b"\0")
    dynstr.extend(f"GLIBC_{glibc}".encode("ascii") + b"\0")
    strtab_offset = 0x240
    strtab_vaddr = 0x400000 + strtab_offset
    data[strtab_offset : strtab_offset + len(dynstr)] = dynstr

    dynamic_entries = [(5, strtab_vaddr), (10, len(dynstr))]
    dynamic_entries.extend((1, offset) for offset in needed_offsets)
    dynamic_entries.append((0, 0))
    dynamic_offset = 0x180
    dynamic_size = len(dynamic_entries) * 16
    for index, entry in enumerate(dynamic_entries):
        struct.pack_into("<qQ", data, dynamic_offset + index * 16, *entry)

    load = struct.pack("<IIQQQQQQ", 1, 5, 0, 0x400000, 0, len(data), len(data), 0x1000)
    dynamic = struct.pack(
        "<IIQQQQQQ",
        2,
        4,
        dynamic_offset,
        0x400000 + dynamic_offset,
        0,
        dynamic_size,
        dynamic_size,
        8,
    )
    data[64 : 64 + 56] = load
    data[120 : 120 + 56] = dynamic
    return bytes(data)


class ManifestTests(unittest.TestCase):
    def test_manifest_requires_exact_release_platforms(self) -> None:
        version = "0.3.8"
        manifest = {
            "version": version,
            "binaries": list(verifier.BINARIES),
            "artifacts": {
                platform: {
                    "url": verifier.expected_artifact_url(version, platform),
                    "sha256": "a" * 64,
                }
                for platform in verifier.PLATFORMS
            },
        }
        validated = verifier.validate_public_manifest(manifest, version)
        self.assertEqual(set(validated), set(verifier.PLATFORMS))
        del manifest["artifacts"]["win32-arm64"]
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_public_manifest(manifest, version)

    def test_existing_server_image_release_record_preserves_immutable_deployment_source(self) -> None:
        version = "0.3.8"
        digest = "sha256:" + "c" * 64
        bootstrap = (
            "#!/bin/sh\n"
            f"image: ${{WEBCODEX_SERVER_IMAGE:-{verifier.SERVER_IMAGE}@{digest}}}\n"
            f"compose_target={verifier.SERVER_MATERIALIZED_COMPOSE}\n"
        ).encode()
        metadata = {
            "schema_version": 1,
            "image": verifier.SERVER_IMAGE,
            "tag": f"v{version}",
            "version": version,
            "image_tag": f"v{version}",
            "deployment_assets": {
                verifier.SERVER_BOOTSTRAP_ASSET: hashlib.sha256(bootstrap).hexdigest(),
            },
            "deployment_source_sha": "2" * 40,
            "source_sha": "b" * 40,
            "created_at": "2026-08-20T15:13:57+08:00",
            "digest": digest,
            "platforms": {
                "linux/amd64": "sha256:" + "d" * 64,
                "linux/arm64": "sha256:" + "e" * 64,
            },
        }
        expected_base = {key: metadata[key] for key in verifier.SERVER_IMAGE_BASE_METADATA_KEYS}
        validated = verifier.validate_server_image_release_record(
            metadata,
            version,
            bootstrap,
            expected_base=expected_base,
        )
        self.assertEqual(validated["deployment_source_sha"], "2" * 40)

        drifted = dict(expected_base)
        drifted["digest"] = "sha256:" + "f" * 64
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_server_image_release_record(metadata, version, bootstrap, expected_base=drifted)
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_server_image_release_record(
                metadata,
                version,
                bootstrap + b"# drift\n",
                expected_base=expected_base,
            )

    def test_server_image_metadata_requires_release_identity_and_two_platforms(self) -> None:
        version = "0.3.8"
        metadata = {
            "schema_version": 1,
            "image": verifier.SERVER_IMAGE,
            "tag": f"v{version}",
            "version": version,
            "image_tag": f"v{version}",
            "deployment_assets": {
                verifier.SERVER_BOOTSTRAP_ASSET: "f" * 64,
            },
            "deployment_source_sha": "2" * 40,
            "source_sha": "b" * 40,
            "created_at": "2026-08-25T05:00:00+00:00",
            "digest": "sha256:" + "c" * 64,
            "platforms": {
                "linux/amd64": "sha256:" + "d" * 64,
                "linux/arm64": "sha256:" + "e" * 64,
            },
        }
        validated = verifier.validate_server_image_metadata(metadata, version)
        self.assertEqual(validated["source_sha"], "b" * 40)
        self.assertEqual(validated["digest"], "sha256:" + "c" * 64)
        del metadata["platforms"]["linux/arm64"]
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_server_image_metadata(metadata, version)

    def test_github_assets_allow_image_metadata_without_breaking_historical_releases(self) -> None:
        version = "0.3.8"
        names = [verifier.canonical_archive_name(version, platform) for platform in verifier.PLATFORMS]
        names.append("SHA256SUMS")
        release = {
            "tag_name": f"v{version}",
            "draft": False,
            "prerelease": False,
            "assets": [
                {"name": name, "state": "uploaded", "browser_download_url": f"https://example.invalid/{name}"}
                for name in names
            ],
        }
        self.assertEqual(set(verifier.validate_github_assets(release, version)), set(names))
        for name in (verifier.SERVER_IMAGE_METADATA, *verifier.SERVER_DEPLOYMENT_ASSETS):
            release["assets"].append(
                {
                    "name": name,
                    "state": "uploaded",
                    "browser_download_url": f"https://example.invalid/{name}",
                }
            )
        validated = verifier.validate_github_assets(release, version)
        self.assertTrue({verifier.SERVER_IMAGE_METADATA, *verifier.SERVER_DEPLOYMENT_ASSETS}.issubset(validated))
        release["assets"].append(
            {"name": "unexpected.bin", "state": "uploaded", "browser_download_url": "https://example.invalid/x"}
        )
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_github_assets(release, version)

    def test_github_assets_reject_partial_server_deployment_set(self) -> None:
        version = "0.3.8"
        names = [verifier.canonical_archive_name(version, platform) for platform in verifier.PLATFORMS]
        names.extend(["SHA256SUMS", verifier.SERVER_IMAGE_METADATA])
        release = {
            "tag_name": f"v{version}",
            "draft": False,
            "prerelease": False,
            "assets": [
                {"name": name, "state": "uploaded", "browser_download_url": f"https://example.invalid/{name}"}
                for name in names
            ],
        }
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_github_assets(release, version)

    def test_sha256sums_requires_exact_unique_set(self) -> None:
        version = "0.3.8"
        text = "\n".join(
            f"{'a' * 64}  {verifier.canonical_archive_name(version, platform)}"
            for platform in verifier.PLATFORMS
        ) + "\n"
        self.assertEqual(len(verifier.parse_sha256sums(text, version)), 5)
        with self.assertRaises(verifier.VerificationError):
            verifier.parse_sha256sums(text + text.splitlines()[0] + "\n", version)


class BinaryInspectionTests(unittest.TestCase):
    def _write(self, root: Path, name: str, content: bytes) -> Path:
        path = root / name
        path.write_bytes(content)
        return path

    def test_foreign_architectures_are_checked_without_execution(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            verifier.inspect_binary_architecture(
                self._write(root, "linux-x64", synthetic_elf(62)), "linux-x64"
            )
            verifier.inspect_binary_architecture(
                self._write(root, "linux-arm64", synthetic_elf(183)), "linux-arm64"
            )
            macho = b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x0100000C) + b"\0" * 16
            verifier.inspect_binary_architecture(
                self._write(root, "darwin-arm64", macho), "darwin-arm64"
            )
            verifier.inspect_binary_architecture(
                self._write(root, "win32-x64.exe", synthetic_pe(0x8664)), "win32-x64"
            )
            verifier.inspect_binary_architecture(
                self._write(root, "win32-arm64.exe", synthetic_pe(0xAA64)), "win32-arm64"
            )

    def test_linux_abi_enforces_glibc_and_needed_allowlist(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            good = self._write(root, "good", synthetic_elf(62))
            max_glibc, needed = verifier.inspect_linux_abi(good)
            self.assertEqual(max_glibc, "2.17")
            self.assertEqual(needed, ("libc.so.6",))
            too_new = self._write(root, "too-new", synthetic_elf(62, glibc="2.18"))
            with self.assertRaises(verifier.VerificationError):
                verifier.inspect_linux_abi(too_new)
            unexpected = self._write(
                root, "unexpected", synthetic_elf(62, needed=("libsecret-host.so",))
            )
            with self.assertRaises(verifier.VerificationError):
                verifier.inspect_linux_abi(unexpected)

    def test_archive_requires_only_canonical_root_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive_path = root / "windows.tar.gz"
            payload = synthetic_pe(0x8664)
            with tarfile.open(archive_path, "w:gz") as archive:
                for name in verifier.expected_binary_names("win32-x64"):
                    info = tarfile.TarInfo(name)
                    info.size = len(payload)
                    archive.addfile(info, io.BytesIO(payload))
            result = verifier.inspect_archive(archive_path, "win32-x64", root / "extract")
            self.assertEqual(set(result["members"]), verifier.expected_binary_names("win32-x64"))


class NpmTarballTests(unittest.TestCase):
    def test_reads_staged_manifest_from_npm_tarball(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            path = root / "package.tgz"
            values = {
                "package/package.json": {"name": verifier.PACKAGE, "version": "0.3.8"},
                "package/manifest.json": {"version": "0.3.8"},
            }
            with tarfile.open(path, "w:gz") as archive:
                for name, value in values.items():
                    raw = json.dumps(value).encode("utf-8")
                    info = tarfile.TarInfo(name)
                    info.size = len(raw)
                    archive.addfile(info, io.BytesIO(raw))
            package, manifest = verifier.read_npm_package_jsons(path)
            self.assertEqual(package["name"], verifier.PACKAGE)
            self.assertEqual(manifest["version"], "0.3.8")


if __name__ == "__main__":
    unittest.main()
