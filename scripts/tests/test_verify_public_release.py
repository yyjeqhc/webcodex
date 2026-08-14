from __future__ import annotations

import io
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
