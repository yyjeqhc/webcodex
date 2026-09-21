from __future__ import annotations

import hashlib
import io
import os
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable


RELEASE_BASE = "https://github.com/openai/tunnel-client/releases/download/v0.0.14"
MAX_DOWNLOAD_BYTES = 64 * 1024 * 1024
MAX_BINARY_BYTES = 64 * 1024 * 1024


class TunnelClientError(RuntimeError):
    pass


@dataclass(frozen=True)
class PinnedTunnelClient:
    version: str
    target: str
    archive_name: str
    archive_sha256: str
    binary_sha256: str
    member_name: str


WINDOWS_AMD64 = PinnedTunnelClient(
    version="0.0.14",
    target="windows-amd64",
    archive_name="tunnel-client-v0.0.14-windows-amd64.zip",
    archive_sha256="784ab8da7b5a88f0109f1fd8aaf0a1c86067430b896dddf307ef7e3cc49fa1a5",
    binary_sha256="fcc85a69ec0ad82518e4f8964f60c45e31787957782a0fc9c1b0c44e82d61b9b",
    member_name="tunnel-client.exe",
)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def runtime_binary_path(runtime_root: Path, asset: PinnedTunnelClient = WINDOWS_AMD64) -> Path:
    return (
        runtime_root
        / ".webpi-runtime"
        / "tunnel-client"
        / asset.version
        / asset.target
        / asset.member_name
    )


def verify_local_binary(path: Path, asset: PinnedTunnelClient = WINDOWS_AMD64) -> bool:
    try:
        return (
            path.is_file()
            and path.stat().st_size <= MAX_BINARY_BYTES
            and sha256_file(path) == asset.binary_sha256
        )
    except OSError:
        return False


def _atomic_write_binary(destination: Path, data: bytes) -> Path:
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, temp_name = tempfile.mkstemp(prefix=".tunnel-client.", suffix=".tmp", dir=destination.parent)
    temp_path = Path(temp_name)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_path, destination)
    finally:
        if temp_path.exists():
            temp_path.unlink()
    return destination


def install_from_verified_source(
    source: Path,
    runtime_root: Path,
    asset: PinnedTunnelClient = WINDOWS_AMD64,
) -> Path:
    if not verify_local_binary(source, asset):
        raise TunnelClientError("candidate tunnel-client failed pinned SHA-256 verification")
    destination = runtime_binary_path(runtime_root, asset)
    if source.resolve() == destination.resolve(strict=False):
        return destination
    return _atomic_write_binary(destination, source.read_bytes())


def install_from_archive_bytes(
    archive_bytes: bytes,
    runtime_root: Path,
    asset: PinnedTunnelClient = WINDOWS_AMD64,
) -> Path:
    if len(archive_bytes) > MAX_DOWNLOAD_BYTES:
        raise TunnelClientError("tunnel-client archive exceeds the maximum allowed size")
    if sha256_bytes(archive_bytes) != asset.archive_sha256:
        raise TunnelClientError("tunnel-client archive failed pinned SHA-256 verification")
    try:
        with zipfile.ZipFile(io.BytesIO(archive_bytes), "r") as archive:
            infos = [info for info in archive.infolist() if info.filename == asset.member_name]
            if len(infos) != 1:
                raise TunnelClientError("tunnel-client archive does not contain exactly one pinned member")
            info = infos[0]
            if info.file_size > MAX_BINARY_BYTES:
                raise TunnelClientError("tunnel-client binary exceeds the maximum allowed size")
            binary = archive.read(info)
    except zipfile.BadZipFile as error:
        raise TunnelClientError("tunnel-client archive is not a valid zip file") from error
    if sha256_bytes(binary) != asset.binary_sha256:
        raise TunnelClientError("tunnel-client binary failed pinned SHA-256 verification")
    destination = runtime_binary_path(runtime_root, asset)
    return _atomic_write_binary(destination, binary)


def _read_bounded_response(response: object) -> bytes:
    read = getattr(response, "read", None)
    if not callable(read):
        raise TunnelClientError("tunnel-client download response is unreadable")
    data = read(MAX_DOWNLOAD_BYTES + 1)
    if len(data) > MAX_DOWNLOAD_BYTES:
        raise TunnelClientError("tunnel-client download exceeds the maximum allowed size")
    return data


def download_and_install(
    runtime_root: Path,
    *,
    asset: PinnedTunnelClient = WINDOWS_AMD64,
    opener: Callable[[str], object] = urllib.request.urlopen,
) -> Path:
    url = f"{RELEASE_BASE}/{asset.archive_name}"
    try:
        response = opener(url)
        enter = getattr(response, "__enter__", None)
        exit_ = getattr(response, "__exit__", None)
        if callable(enter) and callable(exit_):
            with response as opened:
                data = _read_bounded_response(opened)
        else:
            data = _read_bounded_response(response)
    except TunnelClientError:
        raise
    except Exception as error:
        raise TunnelClientError("failed to download pinned OpenAI tunnel-client") from error
    return install_from_archive_bytes(data, runtime_root, asset)


def ensure_local_tunnel_client(
    runtime_root: Path,
    *,
    source_candidates: Iterable[Path] = (),
    asset: PinnedTunnelClient = WINDOWS_AMD64,
    opener: Callable[[str], object] = urllib.request.urlopen,
) -> Path:
    destination = runtime_binary_path(runtime_root, asset)
    if verify_local_binary(destination, asset):
        return destination
    if destination.exists():
        try:
            destination.unlink()
        except OSError as error:
            raise TunnelClientError("invalid WebPi-local tunnel-client cannot be replaced") from error
    for candidate in source_candidates:
        if verify_local_binary(candidate, asset):
            return install_from_verified_source(candidate, runtime_root, asset)
    return download_and_install(runtime_root, asset=asset, opener=opener)
