from __future__ import annotations

import json
import os
import re
import sys
import tempfile
import tomllib
from pathlib import Path

try:
    from .config_migration import _copy_private_permissions
except ImportError:
    from config_migration import _copy_private_permissions

PROVIDER_ID = "pi-bridge"
MARKER_BEGIN = "# WEBPI-PI-BRIDGE-BEGIN"
MARKER_END = "# WEBPI-PI-BRIDGE-END"
PI_BRIDGE_TIMEOUT_SECS = 120
_TIMEOUT_LINE = re.compile(rb"(?m)^timeout_secs[ \t]*=[ \t]*\d+[ \t]*\r?$")


def toml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def backup_path(config: Path) -> Path:
    return config.with_name(config.name + ".webpi-backup")


def atomic_write(config: Path, data: bytes) -> None:
    fd, tmp_name = tempfile.mkstemp(prefix=config.name + ".webpi-", suffix=".tmp", dir=config.parent)
    try:
        with os.fdopen(fd, "wb") as handle:
            _copy_private_permissions(config, Path(tmp_name))
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp_name, config)
    except BaseException:
        try:
            os.unlink(tmp_name)
        except FileNotFoundError:
            pass
        raise


def backup_private_config(config: Path, backup: Path) -> None:
    with backup.open("xb") as stream:
        _copy_private_permissions(config, backup)
        stream.write(config.read_bytes())
        stream.flush()
        os.fsync(stream.fileno())


def current_provider_ids(raw: bytes) -> set[str]:
    parsed = tomllib.loads(raw.decode("utf-8"))
    plugins = parsed.get("plugins", {})
    providers = plugins.get("providers", []) if isinstance(plugins, dict) else []
    result: set[str] = set()
    if isinstance(providers, list):
        for provider in providers:
            if isinstance(provider, dict) and isinstance(provider.get("id"), str):
                result.add(provider["id"])
    return result


def install(config: Path, node: str, plugin: str, cwd: str) -> None:
    raw = config.read_bytes()
    if PROVIDER_ID in current_provider_ids(raw):
        print(json.dumps({"status": "already-present", "provider": PROVIDER_ID}))
        return
    if MARKER_BEGIN.encode() in raw or MARKER_END.encode() in raw:
        raise RuntimeError("partial WebPi provider marker already exists")

    backup = backup_path(config)
    if backup.exists():
        raise RuntimeError("WebPi config backup already exists; rollback/finalize it before retrying")
    backup_private_config(config, backup)

    block = (
        "\n"
        + MARKER_BEGIN
        + "\n[[plugins.providers]]\n"
        + f"id = {toml_string(PROVIDER_ID)}\n"
        + f"name = {toml_string('WebPi Pi Bridge')}\n"
        + f"command = {toml_string(node)}\n"
        + f"args = [{toml_string(plugin)}]\n"
        + f"cwd = {toml_string(cwd)}\n"
        + f"timeout_secs = {PI_BRIDGE_TIMEOUT_SECS}\n"
        + MARKER_END
        + "\n"
    ).encode("utf-8")
    atomic_write(config, raw + block)
    print(json.dumps({"status": "installed-candidate", "provider": PROVIDER_ID, "backup": True}))


def upgrade_timeout(config: Path) -> None:
    raw = config.read_bytes()
    if PROVIDER_ID not in current_provider_ids(raw):
        raise RuntimeError("WebPi Pi Bridge provider is not installed")
    begin = raw.find(MARKER_BEGIN.encode())
    end = raw.find(MARKER_END.encode())
    if begin < 0 or end < 0 or end < begin:
        raise RuntimeError("WebPi-managed Pi Bridge markers are missing or incomplete")
    block_end = end + len(MARKER_END.encode())
    block = raw[begin:block_end]
    matches = list(_TIMEOUT_LINE.finditer(block))
    if len(matches) != 1:
        raise RuntimeError("WebPi-managed Pi Bridge block must contain exactly one timeout_secs line")
    desired = f"timeout_secs = {PI_BRIDGE_TIMEOUT_SECS}".encode()
    match = matches[0]
    candidate_block = block[:match.start()] + desired + block[match.end():]
    candidate = raw[:begin] + candidate_block + raw[block_end:]
    if candidate == raw:
        print(json.dumps({"status": "already-current", "provider": PROVIDER_ID, "timeout_secs": PI_BRIDGE_TIMEOUT_SECS}))
        return
    backup = backup_path(config)
    if backup.exists():
        raise RuntimeError("WebPi config backup already exists; rollback/finalize it before retrying")
    backup_private_config(config, backup)
    atomic_write(config, candidate)
    print(json.dumps({"status": "updated-candidate", "provider": PROVIDER_ID, "timeout_secs": PI_BRIDGE_TIMEOUT_SECS, "backup": True}))

def uninstall(config: Path) -> None:
    raw = config.read_bytes()
    begin = raw.find(MARKER_BEGIN.encode())
    end = raw.find(MARKER_END.encode())
    if begin < 0 and end < 0:
        print(json.dumps({"status": "not-present", "provider": PROVIDER_ID}))
        return
    if begin < 0 or end < 0 or end < begin:
        raise RuntimeError("WebPi provider markers are incomplete; refusing automatic removal")
    end += len(MARKER_END.encode())
    if end < len(raw) and raw[end:end + 2] == b"\r\n":
        end += 2
    elif end < len(raw) and raw[end:end + 1] == b"\n":
        end += 1
    start = begin
    if start > 0 and raw[start - 2:start] == b"\r\n":
        start -= 2
    elif start > 0 and raw[start - 1:start] == b"\n":
        start -= 1
    candidate = raw[:start] + raw[end:]
    if PROVIDER_ID in current_provider_ids(candidate):
        raise RuntimeError("pi-bridge still appears in parsed providers after marker removal")
    backup = backup_path(config)
    if backup.exists():
        raise RuntimeError("WebPi config backup already exists; rollback/finalize it before retrying")
    backup_private_config(config, backup)
    atomic_write(config, candidate)
    print(json.dumps({"status": "uninstalled-candidate", "provider": PROVIDER_ID, "backup": True}))


def rollback(config: Path) -> None:
    backup = backup_path(config)
    if not backup.exists():
        print(json.dumps({"status": "no-backup"}))
        return
    atomic_write(config, backup.read_bytes())
    backup.unlink()
    print(json.dumps({"status": "rolled-back", "provider": PROVIDER_ID}))


def finalize(config: Path) -> None:
    backup = backup_path(config)
    if backup.exists():
        backup.unlink()
    print(json.dumps({"status": "finalized", "provider": PROVIDER_ID}))


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit("usage: install_runner_provider.py install|upgrade-timeout|uninstall|rollback|finalize CONFIG [NODE PLUGIN CWD]")
    action = sys.argv[1]
    config = Path(sys.argv[2]).resolve()
    if action == "install":
        if len(sys.argv) != 6:
            raise SystemExit("install requires CONFIG NODE PLUGIN CWD")
        install(config, sys.argv[3], sys.argv[4], sys.argv[5])
    elif action == "upgrade-timeout":
        upgrade_timeout(config)
    elif action == "uninstall":
        uninstall(config)
    elif action == "rollback":
        rollback(config)
    elif action == "finalize":
        finalize(config)
    else:
        raise SystemExit("unknown action")


if __name__ == "__main__":
    main()
