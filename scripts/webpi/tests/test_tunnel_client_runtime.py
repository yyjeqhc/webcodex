from __future__ import annotations

import hashlib
import io
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))

from scripts.webpi.tunnel_client_runtime import (
    PinnedTunnelClient,
    TunnelClientError,
    WINDOWS_AMD64,
    ensure_local_tunnel_client,
    install_from_archive_bytes,
    install_from_verified_source,
    runtime_binary_path,
    verify_local_binary,
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def make_asset(binary: bytes, archive: bytes = b"") -> PinnedTunnelClient:
    return PinnedTunnelClient(
        version="test",
        target="windows-amd64",
        archive_name="client.zip",
        archive_sha256=sha256(archive),
        binary_sha256=sha256(binary),
        member_name="tunnel-client.exe",
    )


def make_archive(member_name: str, binary: bytes) -> bytes:
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(member_name, binary)
    return buffer.getvalue()


class TunnelClientRuntimeTests(unittest.TestCase):
    def test_windows_pin_matches_current_supported_release(self) -> None:
        self.assertEqual(WINDOWS_AMD64.version, "0.0.14")
        self.assertEqual(
            WINDOWS_AMD64.archive_sha256,
            "784ab8da7b5a88f0109f1fd8aaf0a1c86067430b896dddf307ef7e3cc49fa1a5",
        )
        self.assertEqual(
            WINDOWS_AMD64.binary_sha256,
            "fcc85a69ec0ad82518e4f8964f60c45e31787957782a0fc9c1b0c44e82d61b9b",
        )

    def test_runtime_path_is_webpi_local(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            asset = make_asset(b"x")
            self.assertEqual(
                runtime_binary_path(root, asset),
                root
                / ".webpi-runtime"
                / "tunnel-client"
                / "test"
                / "windows-amd64"
                / "tunnel-client.exe",
            )

    def test_verified_existing_source_is_copied_into_webpi_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            binary = b"verified-client"
            asset = make_asset(binary)
            source = root / "source.exe"
            source.write_bytes(binary)

            installed = install_from_verified_source(source, root, asset)

            self.assertEqual(installed, runtime_binary_path(root, asset))
            self.assertEqual(installed.read_bytes(), binary)
            self.assertTrue(verify_local_binary(installed, asset))

    def test_wrong_source_hash_is_rejected_without_destination(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            asset = make_asset(b"expected")
            source = root / "source.exe"
            source.write_bytes(b"wrong")
            with self.assertRaises(TunnelClientError):
                install_from_verified_source(source, root, asset)
            self.assertFalse(runtime_binary_path(root, asset).exists())

    def test_verified_archive_extracts_only_pinned_member(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            binary = b"verified-client"
            archive = make_archive("tunnel-client.exe", binary)
            asset = make_asset(binary, archive)

            installed = install_from_archive_bytes(archive, root, asset)

            self.assertEqual(installed.read_bytes(), binary)
            self.assertTrue(verify_local_binary(installed, asset))

    def test_bad_archive_or_binary_hash_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            binary = b"verified-client"
            archive = make_archive("tunnel-client.exe", binary)
            wrong_archive_asset = make_asset(binary, b"different")
            with self.assertRaises(TunnelClientError):
                install_from_archive_bytes(archive, root, wrong_archive_asset)

            corrupt_binary = b"other-client"
            corrupt_archive = make_archive("tunnel-client.exe", corrupt_binary)
            asset = PinnedTunnelClient(
                version="test",
                target="windows-amd64",
                archive_name="client.zip",
                archive_sha256=sha256(corrupt_archive),
                binary_sha256=sha256(binary),
                member_name="tunnel-client.exe",
            )
            with self.assertRaises(TunnelClientError):
                install_from_archive_bytes(corrupt_archive, root, asset)

    def test_ensure_prefers_valid_webpi_local_binary_without_download(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            binary = b"verified-client"
            asset = make_asset(binary)
            destination = runtime_binary_path(root, asset)
            destination.parent.mkdir(parents=True)
            destination.write_bytes(binary)

            def forbidden_opener(_url: str):
                raise AssertionError("download must not run")

            self.assertEqual(
                ensure_local_tunnel_client(root, asset=asset, opener=forbidden_opener),
                destination,
            )


if __name__ == "__main__":
    unittest.main()
