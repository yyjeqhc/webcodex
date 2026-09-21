"""Explicit migration of ONE WebPi-owned env file; never imports WebCodex state."""
from __future__ import annotations
import ctypes
import os
from pathlib import Path
import re
import tempfile
from uuid import uuid4

HARDENED = {"WEBPI_SHARED_KEY_ENABLED": "false", "WEBPI_ALLOW_ANONYMOUS": "false", "WEBPI_OAUTH2_SHARED_KEY_BRIDGE": "false", "WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED": "false"}
MAX_ENV_BYTES = 128 * 1024


def migrate_text(text: str) -> str:
    """Translate keys, never values; conflicting aliases and duplicates fail closed."""
    lines: list[str] = []
    values: dict[str, str] = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            lines.append(raw)
            continue
        line = line.removeprefix("export ").strip()
        if "=" not in line:
            raise ValueError("malformed WebPi env entry")
        key, value = line.split("=", 1)
        key, value = key.strip(), value.strip()
        if not re.fullmatch(r"[A-Z][A-Z0-9_]*", key) or "\x00" in value:
            raise ValueError("invalid WebPi env key/value")
        new_key = "WEBPI_" + key[len("WEBCODEX_"):] if key.startswith("WEBCODEX_") else key
        if new_key in values:
            raise ValueError("duplicate or conflicting WebPi configuration aliases")
        values[new_key] = value
        lines.append(new_key + "=" + HARDENED.get(new_key, value))
    if not values.get("WEBPI_TOKEN", "").strip().strip("\"'").strip():
        raise ValueError("migration requires an existing non-empty WebPi bootstrap credential")
    for key, value in HARDENED.items():
        if key not in values: lines.append(key + "=" + value)
    return "\n".join(lines) + "\n"


def _copy_private_permissions(source: Path, target: Path) -> None:
    if os.name != "nt":
        os.chmod(target, 0o600)
        return
    # Copy the original DACL BEFORE secret bytes are written; Windows chmod is
    # not a substitute for ACLs. Fail closed if security metadata cannot be copied.
    from ctypes import wintypes
    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    get_security = advapi.GetFileSecurityW
    get_security.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD)]
    get_security.restype = wintypes.BOOL
    set_security = advapi.SetFileSecurityW
    set_security.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.c_void_p]
    set_security.restype = wintypes.BOOL
    size = wintypes.DWORD()
    get_security(str(source), 4, None, 0, ctypes.byref(size))
    if not 0 < size.value < 65536: raise OSError("cannot obtain source configuration ACL")
    descriptor = ctypes.create_string_buffer(size.value)
    if not get_security(str(source), 4, descriptor, size.value, ctypes.byref(size)):
        raise OSError("cannot read source configuration ACL")
    if not set_security(str(target), 4 | 0x80000000, descriptor):
        raise OSError("cannot preserve configuration ACL")


def _validate_file(path: Path, root: Path) -> None:
    root = root.resolve(strict=True)
    lexical = path.absolute()
    if ".." in lexical.parts or not path.resolve(strict=True).is_relative_to(root):
        raise ValueError("configuration must stay physically inside the selected WebPi root")
    if not lexical.is_relative_to(root): raise ValueError("configuration must be inside the selected WebPi root")
    current = root
    for part in lexical.relative_to(root).parts:
        current = current / part
        if current.is_symlink() or getattr(current, "is_junction", lambda: False)():
            raise ValueError("configuration cannot use symlinks or junctions")
    info = path.stat()
    if not path.is_file() or info.st_nlink != 1 or info.st_size > MAX_ENV_BYTES:
        raise ValueError("configuration must be a bounded regular single-link file")


def _read_bounded(path: Path) -> bytes:
    with path.open("rb") as stream:
        data = stream.read(MAX_ENV_BYTES + 1)
    if len(data) > MAX_ENV_BYTES:
        raise ValueError("configuration exceeds the migration size bound")
    return data


def migrate_env_file(path: Path, root: Path, *, apply: bool = False) -> dict[str, object]:
    _validate_file(path, root)
    original = _read_bounded(path)
    migrated = migrate_text(original.decode("utf-8")).encode("utf-8")
    report: dict[str, object] = {"changed": original != migrated, "applied": False, "credentials_rotated": False}
    if original == migrated or not apply: return report
    backup = path.with_name(path.name + ".pre-webpi-namespace-" + uuid4().hex + ".bak")
    fd, temporary = tempfile.mkstemp(prefix=".webpi-migrate-", dir=path.parent)
    temp = Path(temporary)
    try:
        with os.fdopen(fd, "wb") as stream:
            _copy_private_permissions(path, temp)
            stream.write(migrated)
            stream.flush()
            os.fsync(stream.fileno())
        with backup.open("xb") as stream:
            _copy_private_permissions(path, backup)
            stream.write(original)
            stream.flush()
            os.fsync(stream.fileno())
        _validate_file(path, root)
        if _read_bounded(path) != original:
            raise RuntimeError("configuration changed during migration; reconcile before retrying")
        os.replace(temp, path)
    finally:
        temp.unlink(missing_ok=True)
    report.update({"applied": True, "backup": str(backup)})
    return report
