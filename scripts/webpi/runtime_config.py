from __future__ import annotations

import json
import os
import tempfile
from dataclasses import dataclass
from pathlib import Path, PureWindowsPath
from typing import Mapping


WEBPI_TUNNEL_ID_ENV = "WEBPI_CONTROL_PLANE_TUNNEL_ID"
WEBPI_TUNNEL_KEY_ENV = "WEBPI_CONTROL_PLANE_API_KEY"
WEBPI_TUNNEL_CLIENT_ENV = "WEBPI_TUNNEL_CLIENT_BIN"

LEGACY_OR_GLOBAL_SECRET_ENV = {
    "CONTROL_PLANE_TUNNEL_ID",
    "CONTROL_PLANE_API_KEY",
    "WEBPI_TUNNEL_CLIENT_BIN",
    "OPENAI_ADMIN_KEY",
    "OPENAI_API_KEY",
    "WEBPI_TOKEN",
    "WEBPI_SHARED_KEY",
    "WEBPI_PUBLIC_URL",
    "TUNNEL_TOKEN",
    "CLOUDFLARE_API_TOKEN",
    "CLOUDFLARE_API_KEY",
    "CF_API_TOKEN",
    "CF_API_KEY",
}

HARDENED_AUTH_ENV = {
    "WEBPI_SHARED_KEY_ENABLED": "false",
    "WEBPI_ALLOW_ANONYMOUS": "false",
    "WEBPI_OAUTH2_SHARED_KEY_BRIDGE": "false",
    "WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED": "false",
}


def _isolated_base(base_environ: Mapping[str, str]) -> dict[str, str]:
    blocked = LEGACY_OR_GLOBAL_SECRET_ENV | WEBPI_SECRET_ENV | set(HARDENED_AUTH_ENV)
    # Standalone WebPi is file-configured: neither product's ambient variables
    # may override the selected local file or inject code into child runtimes.
    child = {
        key: value for key, value in base_environ.items()
        if key.upper() not in blocked
        and not key.upper().startswith(("WEBCODEX_", "WEBPI_"))
        and key.upper() not in {"NODE_OPTIONS", "NODE_PATH", "PYTHONPATH", "PYTHONHOME"}
    }
    child.update(HARDENED_AUTH_ENV)
    return child

WEBPI_SECRET_ENV = {
    WEBPI_TUNNEL_ID_ENV,
    WEBPI_TUNNEL_KEY_ENV,
    WEBPI_TUNNEL_CLIENT_ENV,
}


class TunnelConfigError(ValueError):
    pass


@dataclass(frozen=True)
class TunnelCredentials:
    tunnel_id: str
    api_key: str
    tunnel_client_bin: str | None = None


def tunnel_config_path(state_dir: Path) -> Path:
    return state_dir / "secrets" / "openai-tunnel.json"


def _valid_tunnel_id(value: str) -> bool:
    if not value.startswith("tunnel_"):
        return False
    suffix = value.removeprefix("tunnel_")
    return len(suffix) == 32 and all(ch in "0123456789abcdef" for ch in suffix)


def _normalize_client_bin(value: object) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise TunnelConfigError("tunnel_client_bin must be a string when present")
    result = value.strip()
    if not result:
        return None
    if not (Path(result).is_absolute() or PureWindowsPath(result).is_absolute()):
        raise TunnelConfigError("tunnel_client_bin must be an absolute path")
    return result


def _validated_credentials(tunnel_id: object, api_key: object, tunnel_client_bin: object = None) -> TunnelCredentials:
    if not isinstance(tunnel_id, str) or not _valid_tunnel_id(tunnel_id.strip()):
        raise TunnelConfigError("tunnel_id must be tunnel_ followed by 32 lowercase hexadecimal characters")
    if not isinstance(api_key, str) or not api_key.strip():
        raise TunnelConfigError("api_key must be non-empty")
    return TunnelCredentials(
        tunnel_id=tunnel_id.strip(),
        api_key=api_key.strip(),
        tunnel_client_bin=_normalize_client_bin(tunnel_client_bin),
    )


def load_tunnel_credentials(
    state_dir: Path,
    environ: Mapping[str, str] | None = None,
) -> TunnelCredentials | None:
    env = os.environ if environ is None else environ
    path = tunnel_config_path(state_dir)
    if path.exists():
        if not path.is_file():
            raise TunnelConfigError(f"WebPi tunnel config is not a regular file: {path}")
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            raise TunnelConfigError("WebPi tunnel config is unreadable or invalid JSON") from error
        if not isinstance(value, dict):
            raise TunnelConfigError("WebPi tunnel config must be a JSON object")
        allowed = {"version", "tunnel_id", "api_key", "tunnel_client_bin"}
        unknown = sorted(set(value) - allowed)
        if unknown:
            raise TunnelConfigError(f"WebPi tunnel config contains unknown fields: {', '.join(unknown)}")
        if value.get("version", 1) != 1:
            raise TunnelConfigError("unsupported WebPi tunnel config version")
        return _validated_credentials(
            value.get("tunnel_id"),
            value.get("api_key"),
            value.get("tunnel_client_bin"),
        )

    tunnel_id = env.get(WEBPI_TUNNEL_ID_ENV, "").strip()
    api_key = env.get(WEBPI_TUNNEL_KEY_ENV, "").strip()
    tunnel_client_bin = env.get(WEBPI_TUNNEL_CLIENT_ENV, "").strip() or None
    if not tunnel_id and not api_key and tunnel_client_bin is None:
        return None
    if not tunnel_id or not api_key:
        raise TunnelConfigError(
            f"{WEBPI_TUNNEL_ID_ENV} and {WEBPI_TUNNEL_KEY_ENV} must be configured together"
        )
    return _validated_credentials(tunnel_id, api_key, tunnel_client_bin)


def inspect_tunnel_configuration(
    state_dir: Path,
    environ: Mapping[str, str] | None = None,
) -> dict[str, object]:
    path = tunnel_config_path(state_dir)
    try:
        credentials = load_tunnel_credentials(state_dir, environ)
    except TunnelConfigError:
        return {
            "configured": False,
            "source": "invalid-file" if path.exists() else "invalid-environment",
        }
    if credentials is None:
        return {"configured": False, "source": "none"}
    return {
        "configured": True,
        "source": "file" if path.exists() else "environment",
    }


def save_tunnel_credentials(
    state_dir: Path,
    tunnel_id: str,
    api_key: str,
    tunnel_client_bin: str | None = None,
) -> Path:
    credentials = _validated_credentials(tunnel_id, api_key, tunnel_client_bin)
    path = tunnel_config_path(state_dir)
    path.parent.mkdir(parents=True, exist_ok=True)
    payload: dict[str, object] = {
        "version": 1,
        "tunnel_id": credentials.tunnel_id,
        "api_key": credentials.api_key,
    }
    if credentials.tunnel_client_bin is not None:
        payload["tunnel_client_bin"] = credentials.tunnel_client_bin

    fd, temp_name = tempfile.mkstemp(prefix=".openai-tunnel.", suffix=".tmp", dir=path.parent)
    temp_path = Path(temp_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
            json.dump(payload, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        try:
            os.chmod(temp_path, 0o600)
        except OSError:
            pass
        os.replace(temp_path, path)
    finally:
        if temp_path.exists():
            temp_path.unlink()
    return path


def build_tunnel_child_env(
    base_environ: Mapping[str, str],
    credentials: TunnelCredentials,
) -> dict[str, str]:
    child = _isolated_base(base_environ)
    child["CONTROL_PLANE_TUNNEL_ID"] = credentials.tunnel_id
    child["CONTROL_PLANE_API_KEY"] = credentials.api_key
    if credentials.tunnel_client_bin is not None:
        child["WEBPI_TUNNEL_CLIENT_BIN"] = credentials.tunnel_client_bin
    return child


def build_non_tunnel_child_env(base_environ: Mapping[str, str]) -> dict[str, str]:
    return _isolated_base(base_environ)
