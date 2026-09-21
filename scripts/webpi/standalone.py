from __future__ import annotations

import json
import getpass
import os
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import replace
from pathlib import Path
from typing import Mapping

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from runtime_config import (
    build_non_tunnel_child_env,
    build_tunnel_child_env,
    inspect_tunnel_configuration,
    load_tunnel_credentials,
    save_tunnel_credentials,
)
from tunnel_client_runtime import WINDOWS_AMD64, runtime_binary_path, verify_local_binary
from security_smoke import require_authenticated_origin
from config_migration import migrate_env_file, _copy_private_permissions

ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / ".webpi-state"
SERVER_DIR = STATE / "server"
SERVER_ENV = SERVER_DIR / "webpi.env"
SERVER_DATA = SERVER_DIR / "data"
CONNECTIONS = STATE / "connections"
MANIFEST = STATE / "manifest.json"
ACTION_TOKEN_FILE = STATE / "webpi-action-token"
ACTION_TOKEN_NAME = "webpi-action"
PORT = 56542
SERVER_URL = f"http://127.0.0.1:{PORT}"
CLIENT_ID = "webpi-local"
USERNAME = "webpi"
DISPLAY_NAME = "WebPi Local"


class _NoopProcessContainment:
    def assign(self, process: subprocess.Popen[object]) -> None:
        del process

    def close(self) -> None:
        return


class _WindowsKillOnCloseJob:
    """Own child processes in a Windows Job that dies with the supervisor."""

    def __init__(self) -> None:
        import ctypes
        from ctypes import wintypes

        class JOBOBJECT_BASIC_LIMIT_INFORMATION(ctypes.Structure):
            _fields_ = [
                ("PerProcessUserTimeLimit", ctypes.c_longlong),
                ("PerJobUserTimeLimit", ctypes.c_longlong),
                ("LimitFlags", wintypes.DWORD),
                ("MinimumWorkingSetSize", ctypes.c_size_t),
                ("MaximumWorkingSetSize", ctypes.c_size_t),
                ("ActiveProcessLimit", wintypes.DWORD),
                ("Affinity", ctypes.c_size_t),
                ("PriorityClass", wintypes.DWORD),
                ("SchedulingClass", wintypes.DWORD),
            ]

        class IO_COUNTERS(ctypes.Structure):
            _fields_ = [
                ("ReadOperationCount", ctypes.c_ulonglong),
                ("WriteOperationCount", ctypes.c_ulonglong),
                ("OtherOperationCount", ctypes.c_ulonglong),
                ("ReadTransferCount", ctypes.c_ulonglong),
                ("WriteTransferCount", ctypes.c_ulonglong),
                ("OtherTransferCount", ctypes.c_ulonglong),
            ]

        class JOBOBJECT_EXTENDED_LIMIT_INFORMATION(ctypes.Structure):
            _fields_ = [
                ("BasicLimitInformation", JOBOBJECT_BASIC_LIMIT_INFORMATION),
                ("IoInfo", IO_COUNTERS),
                ("ProcessMemoryLimit", ctypes.c_size_t),
                ("JobMemoryLimit", ctypes.c_size_t),
                ("PeakProcessMemoryUsed", ctypes.c_size_t),
                ("PeakJobMemoryUsed", ctypes.c_size_t),
            ]

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        create_job = kernel32.CreateJobObjectW
        create_job.argtypes = [wintypes.LPVOID, wintypes.LPCWSTR]
        create_job.restype = wintypes.HANDLE
        set_information = kernel32.SetInformationJobObject
        set_information.argtypes = [wintypes.HANDLE, ctypes.c_int, wintypes.LPVOID, wintypes.DWORD]
        set_information.restype = wintypes.BOOL

        handle = create_job(None, None)
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION()
        info.BasicLimitInformation.LimitFlags = 0x00002000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if not set_information(handle, 9, ctypes.byref(info), ctypes.sizeof(info)):
            error = ctypes.WinError(ctypes.get_last_error())
            kernel32.CloseHandle(handle)
            raise error
        self._ctypes = ctypes
        self._wintypes = wintypes
        self._kernel32 = kernel32
        self._handle = handle

    def assign(self, process: subprocess.Popen[object]) -> None:
        if not self._handle:
            raise RuntimeError("Windows process containment job is already closed")
        process_access = 0x0001 | 0x0100  # PROCESS_TERMINATE | PROCESS_SET_QUOTA
        open_process = self._kernel32.OpenProcess
        open_process.argtypes = [self._wintypes.DWORD, self._wintypes.BOOL, self._wintypes.DWORD]
        open_process.restype = self._wintypes.HANDLE
        assign_process = self._kernel32.AssignProcessToJobObject
        assign_process.argtypes = [self._wintypes.HANDLE, self._wintypes.HANDLE]
        assign_process.restype = self._wintypes.BOOL
        process_handle = open_process(process_access, False, process.pid)
        if not process_handle:
            raise self._ctypes.WinError(self._ctypes.get_last_error())
        try:
            if not assign_process(self._handle, process_handle):
                raise self._ctypes.WinError(self._ctypes.get_last_error())
        finally:
            self._kernel32.CloseHandle(process_handle)

    def close(self) -> None:
        if self._handle:
            self._kernel32.CloseHandle(self._handle)
            self._handle = None


def _create_process_containment() -> _NoopProcessContainment | _WindowsKillOnCloseJob:
    if os.name == "nt":
        return _WindowsKillOnCloseJob()
    return _NoopProcessContainment()

CLI = ROOT / "target" / "dogfood" / "webpi.exe"
RUNNER_BIN = ROOT / "target" / "dogfood" / "webpi-runner.exe"
NODE = ROOT / ".webpi-runtime" / "node" / "node.exe"
PI_BRIDGE = ROOT / "plugins" / "pi-bridge" / "dist" / "plugin.js"
PI_ADMIN = ROOT / "plugins" / "pi-bridge" / "dist" / "pi-admin.js"
PI_AGENT_DIR = STATE / "pi-agent"
INSTALL_PROVIDER = ROOT / "scripts" / "webpi" / "install_runner_provider.py"


def json_print(value: object) -> None:
    print(json.dumps(value, ensure_ascii=False, indent=2))


def require_file(path: Path, purpose: str) -> None:
    if not path.is_file():
        raise RuntimeError(f"{purpose} is missing: {path}")


def load_manifest() -> dict[str, object] | None:
    if not MANIFEST.is_file():
        return None
    value = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError("WebPi manifest is malformed")
    return value


def server_is_online() -> bool:
    try:
        with urllib.request.urlopen(f"{SERVER_URL}/openapi.json", timeout=1.5) as response:
            return response.status == 200
    except (OSError, urllib.error.URLError):
        return False


def live_unknown_bearer_status() -> int | None:
    request = urllib.request.Request(
        f"{SERVER_URL}/api/actions/runtime_status",
        data=b'{"compact":true}',
        headers={
            "Authorization": "Bearer webpi-invalid-status-probe",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=1.5) as response:
            return response.status
    except urllib.error.HTTPError as error:
        with error:
            return error.code
    except (OSError, urllib.error.URLError):
        return None



def normalize_public_url(value: str) -> str:
    if any(ord(character) < 32 or ord(character) == 127 for character in value):
        raise ValueError("WebPi public URL contains control characters")
    raw = value.strip()
    parsed = urllib.parse.urlsplit(raw)
    if parsed.scheme.lower() != "https":
        raise ValueError("WebPi public URL must use https")
    if not parsed.netloc or parsed.hostname is None:
        raise ValueError("WebPi public URL must include a hostname")
    if parsed.username is not None or parsed.password is not None:
        raise ValueError("WebPi public URL must not contain user info")
    if parsed.path not in ("", "/") or parsed.query or parsed.fragment:
        raise ValueError("WebPi public URL must be an HTTPS origin without path, query, or fragment")
    try:
        _ = parsed.port
    except ValueError as error:
        raise ValueError("WebPi public URL port is invalid") from error
    return urllib.parse.urlunsplit(("https", parsed.netloc, "", "", ""))


def _env_values(path: Path) -> dict[str, str]:
    if not path.is_file():
        return {}
    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"').strip("'")
    return values


def _replace_env_values(path: Path, updates: Mapping[str, str]) -> None:
    if not path.is_file():
        raise RuntimeError(f"WebPi server env is missing: {path}")
    if path.is_symlink():
        raise RuntimeError("WebPi server env must not be a symlink")
    original = path.read_bytes()
    source = original.decode("utf-8").splitlines()
    if any(not key or any(character in key + value for character in "\r\n\x00") for key, value in updates.items()):
        raise ValueError("invalid environment update")
    output: list[str] = []
    pending = dict(updates)
    seen: set[str] = set()
    for raw_line in source:
        stripped = raw_line.strip()
        if stripped and not stripped.startswith("#") and "=" in stripped:
            key = stripped.split("=", 1)[0].strip()
            if key in pending:
                if key not in seen:
                    output.append(f"{key}={pending[key]}")
                    seen.add(key)
                continue
        output.append(raw_line)
    for key, value in pending.items():
        if key not in seen:
            output.append(f"{key}={value}")
    fd, temporary = tempfile.mkstemp(prefix=".webpi-env-", suffix=".tmp", dir=path.parent)
    temp = Path(temporary)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
            _copy_private_permissions(path, temp)
            stream.write("\n".join(output) + "\n")
            stream.flush()
            os.fsync(stream.fileno())
        if path.read_bytes() != original:
            raise RuntimeError("WebPi server env changed concurrently; retry after reviewing it")
        os.replace(temp, path)
    finally:
        if temp.exists():
            temp.unlink()


def harden_server_env(path: Path = SERVER_ENV) -> None:
    if any(name.startswith("WEBCODEX_") for name in _env_values(path)):
        raise RuntimeError("Legacy WebPi-owned config requires: webpi.cmd migrate-config (after stopping the old instance)")
    if not _env_values(path).get("WEBPI_TOKEN", "").strip():
        raise RuntimeError("WebPi requires its own non-empty bootstrap credential; refusing auth-disabled startup")
    _replace_env_values(
        path,
        {
            "WEBPI_SHARED_KEY_ENABLED": "false",
            "WEBPI_ALLOW_ANONYMOUS": "false",
            "WEBPI_OAUTH2_SHARED_KEY_BRIDGE": "false",
            "WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED": "false",
            "WEBPI_PUBLIC_ACTIONS_ONLY": "true",
            "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_ENABLED": "true",
            "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_MAX": "120",
            "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_WINDOW_SECS": "60",
            "WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_PENALTY_SECS": "60",
        },
    )


def configured_public_url(path: Path = SERVER_ENV) -> str | None:
    raw = _env_values(path).get("WEBPI_PUBLIC_URL", "").strip()
    return normalize_public_url(raw) if raw else None


def configure_public_url(public_url: str) -> None:
    ensure_server_state()
    normalized = normalize_public_url(public_url)
    _replace_env_values(SERVER_ENV, {"WEBPI_PUBLIC_URL": normalized})
    manifest = load_manifest()
    if manifest is not None:
        manifest["public_url"] = normalized
        write_manifest(manifest)
    json_print(
        {
            "status": "public-url-configured",
            "public_url": normalized,
            "openapi_url": f"{normalized}/openapi.json",
            "mcp_url": f"{normalized}/mcp",
            "action_api_base": f"{normalized}/api/actions",
            "restart_required": server_is_online(),
            "shared_key_enabled": False,
            "anonymous_enabled": False,
            "cloudflare_credentials_managed_by_webpi": False,
        }
    )

def safe_status() -> dict[str, object]:
    manifest = load_manifest()
    runner_path = Path(str(manifest.get("runner_config", ""))) if manifest else None
    tunnel = inspect_tunnel_configuration(STATE)
    env = _env_values(SERVER_ENV)
    public_url: str | None = None
    public_url_state = "none"
    try:
        public_url = configured_public_url()
        if public_url is not None:
            public_url_state = "configured"
    except (OSError, UnicodeError, ValueError):
        public_url_state = "invalid"
    server_online = server_is_online()
    live_bearer_status = live_unknown_bearer_status() if server_online else None
    shared_key_enabled = env.get("WEBPI_SHARED_KEY_ENABLED", "").strip().lower() == "true"
    anonymous_enabled = env.get("WEBPI_ALLOW_ANONYMOUS", "").strip().lower() == "true"
    auth_config_hardened = bool(env.get("WEBPI_TOKEN", "").strip()) and all(
        env.get(name, "").strip().lower() == "false"
        for name in ("WEBPI_SHARED_KEY_ENABLED", "WEBPI_ALLOW_ANONYMOUS", "WEBPI_OAUTH2_SHARED_KEY_BRIDGE", "WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED")
    )
    live_auth_hardened = None if live_bearer_status is None else live_bearer_status == 401
    public_actions_only = env.get("WEBPI_PUBLIC_ACTIONS_ONLY", "").strip().lower() == "true"
    invalid_auth_rate_limit_enabled = env.get("WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_ENABLED", "").strip().lower() == "true"
    invalid_auth_rate_limit_max = env.get("WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_MAX", "").strip()
    invalid_auth_rate_limit_window_secs = env.get("WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_WINDOW_SECS", "").strip()
    invalid_auth_rate_limit_penalty_secs = env.get("WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_PENALTY_SECS", "").strip()
    public_surface_hardened = (
        public_actions_only
        and invalid_auth_rate_limit_enabled
        and invalid_auth_rate_limit_max == "120"
        and invalid_auth_rate_limit_window_secs == "60"
        and invalid_auth_rate_limit_penalty_secs == "60"
    )
    return {
        "server_url": SERVER_URL,
        "server_online": server_online,
        "server_env_present": SERVER_ENV.is_file(),
        "config_migration_required": any(name.startswith("WEBCODEX_") for name in env),
        "native_binaries_present": CLI.is_file() and RUNNER_BIN.is_file() and CLI.with_name("webpi-server.exe").is_file(),
        "manifest_present": MANIFEST.is_file(),
        "runner_config_present": bool(runner_path and runner_path.is_file()),
        "pi_bridge_present": PI_BRIDGE.is_file(),
        "action_token_present": ACTION_TOKEN_FILE.is_file(),
        "public_url": public_url,
        "public_url_state": public_url_state,
        "openapi_url": None if public_url is None else f"{public_url}/openapi.json",
        "mcp_url": None if public_url is None else f"{public_url}/mcp",
        "shared_key_enabled": shared_key_enabled,
        "anonymous_enabled": anonymous_enabled,
        "auth_config_hardened": auth_config_hardened,
        "live_unknown_bearer_status": live_bearer_status,
        "live_auth_hardened": live_auth_hardened,
        "auth_hardened": auth_config_hardened and server_online and live_auth_hardened is True,
        "restart_required_for_auth": bool(server_online and auth_config_hardened and live_auth_hardened is not True),
        "public_actions_only": public_actions_only,
        "public_mcp_exposed": False,
        "public_tools_call_exposed": False,
        "public_invalid_auth_rate_limit_enabled": invalid_auth_rate_limit_enabled,
        "public_invalid_auth_rate_limit_max": invalid_auth_rate_limit_max,
        "public_invalid_auth_rate_limit_window_secs": invalid_auth_rate_limit_window_secs,
        "public_invalid_auth_rate_limit_penalty_secs": invalid_auth_rate_limit_penalty_secs,
        "public_surface_hardened": public_surface_hardened,
        "tunnel_configured": tunnel["configured"],
        "tunnel_config_source": tunnel["source"],
        "client_id": CLIENT_ID,
        "state_dir": str(STATE),
    }



def doctor_status() -> dict[str, object]:
    static = safe_status()
    failed: list[str] = []
    warnings: list[str] = []

    for name in (
        "server_online",
        "native_binaries_present",
        "runner_config_present",
        "pi_bridge_present",
        "action_token_present",
        "auth_hardened",
        "public_surface_hardened",
    ):
        if static.get(name) is not True:
            failed.append(name)

    runtime_output: dict[str, object] = {}
    try:
        runtime_wrapper = local_action_post(
            "runtime_status",
            {"client_id": CLIENT_ID, "compact": True},
        )
        if runtime_wrapper.get("success") is not True or not isinstance(runtime_wrapper.get("output"), dict):
            failed.append("runtime_status")
        else:
            runtime_output = runtime_wrapper["output"]
    except RuntimeError:
        failed.append("runtime_status")

    service = runtime_output.get("service")
    if runtime_output and service != "webpi":
        failed.append("runtime_service_identity")

    focus = runtime_output.get("focus")
    focus_dict = focus if isinstance(focus, dict) else {}
    runner_ok = (
        focus_dict.get("client_id") == CLIENT_ID
        and focus_dict.get("status") == "online"
        and focus_dict.get("connected") is True
    )
    if runtime_output and not runner_ok:
        failed.append("runner_online")
    if runtime_output and focus_dict.get("compatibility_status") != "compatible":
        failed.append("runner_compatibility")

    alignment = focus_dict.get("source_alignment")
    alignment_dict = alignment if isinstance(alignment, dict) else {}
    alignment_status = alignment_dict.get("status")
    if runtime_output and alignment_status not in (None, "current", "aligned"):
        warnings.append("source_alignment")

    pi_bridge_output: dict[str, object] = {}
    try:
        plugin_wrapper = local_action_post(
            "plugin_tool",
            {"action": "list", "runner": CLIENT_ID, "plugin": "pi-bridge"},
        )
        if plugin_wrapper.get("success") is not True or not isinstance(plugin_wrapper.get("output"), dict):
            failed.append("pi_bridge_probe")
        else:
            pi_bridge_output = plugin_wrapper["output"]
    except RuntimeError:
        failed.append("pi_bridge_probe")

    if pi_bridge_output and (
        pi_bridge_output.get("plugin") != "pi-bridge"
        or pi_bridge_output.get("runner") != CLIENT_ID
        or pi_bridge_output.get("status") != "ready"
    ):
        failed.append("pi_bridge_ready")

    # Keep the report intentionally narrow: no raw Action payloads, bearer material,
    # provider arguments, environment values, or filesystem paths are projected.
    job_concurrency = focus_dict.get("job_concurrency")
    runner = {
        "client_id": focus_dict.get("client_id"),
        "status": focus_dict.get("status"),
        "connected": focus_dict.get("connected"),
        "compatibility_status": focus_dict.get("compatibility_status"),
        "project_count": focus_dict.get("project_count"),
        "active_jobs": focus_dict.get("active_jobs"),
        "job_concurrency": job_concurrency if isinstance(job_concurrency, dict) else {},
        "source_alignment": alignment_dict,
    }
    pi_bridge = {
        "name": pi_bridge_output.get("name"),
        "status": pi_bridge_output.get("status"),
        "tool_count": pi_bridge_output.get("toolCount"),
    }
    status = "fail" if failed else ("warn" if warnings else "pass")
    return {
        "status": status,
        "healthy": not failed,
        "failed_checks": failed,
        "warning_checks": warnings,
        "service": service,
        "version": runtime_output.get("version"),
        "public_url": static.get("public_url"),
        "server_online": static.get("server_online"),
        "auth_hardened": static.get("auth_hardened"),
        "public_surface_hardened": static.get("public_surface_hardened"),
        "runner": runner,
        "pi_bridge": pi_bridge,
    }

def _merge_windows_persistent_environment(
    source: Mapping[str, str],
    machine_path: str | None,
    user_path: str | None,
    overrides: Mapping[str, str] | None = None,
) -> dict[str, str]:
    result = dict(source)
    path_parts = [value for value in (machine_path, user_path) if value]
    if path_parts:
        result["PATH"] = ";".join(path_parts)
    if overrides is not None:
        for key, value in overrides.items():
            if value:
                result[key] = value
    return result


def _windows_persistent_environment(source: Mapping[str, str]) -> dict[str, str]:
    if os.name != "nt":
        return dict(source)
    import winreg

    def read(root: int, path: str, name: str) -> str | None:
        try:
            with winreg.OpenKey(root, path) as key:
                value = winreg.QueryValueEx(key, name)[0]
        except OSError:
            return None
        if not isinstance(value, str) or not value:
            return None
        return winreg.ExpandEnvironmentStrings(value)

    machine_key = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"
    user_key = r"Environment"
    machine_path = read(winreg.HKEY_LOCAL_MACHINE, machine_key, "Path")
    user_path = read(winreg.HKEY_CURRENT_USER, user_key, "Path")
    overrides: dict[str, str] = {}
    for name in ("JAVA_HOME", "MAVEN_HOME", "GRADLE_HOME"):
        value = read(winreg.HKEY_CURRENT_USER, user_key, name)
        if value is None:
            value = read(winreg.HKEY_LOCAL_MACHINE, machine_key, name)
        if value is not None:
            overrides[name] = value
    return _merge_windows_persistent_environment(source, machine_path, user_path, overrides)

def isolated_child_environment(
    role: str,
    state_dir: Path = STATE,
    environ: Mapping[str, str] | None = None,
    *,
    runtime_root: Path = ROOT,
) -> dict[str, str]:
    source = os.environ if environ is None else environ
    if environ is None:
        source = _windows_persistent_environment(source)
    if role == "tunnel":
        credentials = load_tunnel_credentials(state_dir, source)
        if credentials is None:
            raise RuntimeError(
                "WebPi OpenAI Tunnel credentials are not configured; use the WebPi secret file "
                "or WEBPI_CONTROL_PLANE_TUNNEL_ID + WEBPI_CONTROL_PLANE_API_KEY"
            )
        if credentials.tunnel_client_bin is not None:
            configured_client = Path(credentials.tunnel_client_bin)
            if not configured_client.is_file():
                raise RuntimeError(
                    f"WebPi configured tunnel-client does not exist: {configured_client}"
                )
            client = configured_client
        else:
            client = runtime_binary_path(runtime_root, WINDOWS_AMD64)
            if not verify_local_binary(client, WINDOWS_AMD64):
                raise RuntimeError(
                    "WebPi-local pinned tunnel-client is missing or invalid; bootstrap it under "
                    ".webpi-runtime before starting the OpenAI Tunnel"
                )
        return build_tunnel_child_env(
            source,
            replace(credentials, tunnel_client_bin=str(client)),
        )
    if role in {"server", "runner", "admin"}:
        return build_non_tunnel_child_env(source)
    raise RuntimeError(f"unknown WebPi child role: {role}")


def tunnel_process_spec(
    state_dir: Path = STATE,
    environ: Mapping[str, str] | None = None,
    *,
    cli: Path = CLI,
    server_env: Path = SERVER_ENV,
) -> dict[str, object]:
    return {
        "argv": [
            str(cli),
            "server",
            "tunnel",
            "--provider",
            "openai",
            "--env-file",
            str(server_env),
            "--json",
            "--stop-on-stdin-eof",
        ],
        "env": isolated_child_environment("tunnel", state_dir, environ),
    }


def run_capture(argv: list[str], *, stdin: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        argv,
        cwd=ROOT,
        input=stdin,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=True,
        timeout=60,
        env=isolated_child_environment("admin"),
        shell=False,
    )


def bootstrap_key() -> str:
    values: dict[str, str] = {}
    for raw_line in SERVER_ENV.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"').strip("'")
    result = values.get("WEBPI_TOKEN") or values.get("WEBPI_SHARED_KEY")
    if not result:
        raise RuntimeError("WebPi server env has no bootstrap/admin key")
    return result


def admin_post(path: str, payload: dict[str, object]) -> dict[str, object]:
    request = urllib.request.Request(
        f"{SERVER_URL}{path}",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {bootstrap_key()}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        value = json.loads(response.read().decode("utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError(f"WebPi admin endpoint returned a malformed payload: {path}")
    return value


def local_action_post(tool: str, payload: dict[str, object]) -> dict[str, object]:
    if not ACTION_TOKEN_FILE.is_file():
        raise RuntimeError("WebPi action token is missing; run enrollment before local doctor probes")
    token = ACTION_TOKEN_FILE.read_text(encoding="utf-8").strip()
    if not token.startswith("wc_pat_"):
        raise RuntimeError("WebPi action token file is malformed")
    request = urllib.request.Request(
        f"{SERVER_URL}/api/actions/{tool}",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            value = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        with error:
            raise RuntimeError(f"WebPi local action '{tool}' returned HTTP {error.code}") from error
    except (OSError, urllib.error.URLError, UnicodeError, json.JSONDecodeError) as error:
        raise RuntimeError(f"WebPi local action '{tool}' failed") from error
    if not isinstance(value, dict):
        raise RuntimeError(f"WebPi local action '{tool}' returned a malformed payload")
    return value


def write_manifest(value: dict[str, object]) -> None:
    STATE.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def action_token_scopes(baseline: list[str]) -> list[str]:
    return list(
        dict.fromkeys(
            [
                *baseline,
                "plugin:inspect",
                "plugin:invoke",
                "computer:read",
                "computer:display_read",
                "browser:read",
            ]
        )
    )


def current_action_token_record(
    tokens: list[object], token: str, desired_scopes: list[str]
) -> dict[str, object] | None:
    prefix = token[:16]
    desired = set(desired_scopes)
    for item in tokens:
        if not isinstance(item, dict):
            continue
        if item.get("name") != ACTION_TOKEN_NAME or item.get("revoked_at") is not None:
            continue
        if item.get("token_prefix") != prefix:
            continue
        scopes = item.get("scopes")
        if not isinstance(scopes, list) or not all(isinstance(scope, str) for scope in scopes):
            return None
        if set(scopes) != desired:
            return None
        return item
    return None


def stale_action_token_ids(tokens: list[object], token: str) -> list[str]:
    prefix = token[:16]
    stale: list[str] = []
    for item in tokens:
        if not isinstance(item, dict):
            continue
        if item.get("name") != ACTION_TOKEN_NAME or item.get("revoked_at") is not None:
            continue
        if item.get("token_prefix") == prefix:
            continue
        token_id = item.get("id")
        if isinstance(token_id, str) and token_id:
            stale.append(token_id)
    return stale


def provision_action_token(manifest: dict[str, object]) -> None:
    token_list = admin_post("/api/tokens/list", {"username": USERNAME})
    tokens = token_list.get("tokens")
    if not isinstance(tokens, list):
        raise RuntimeError("WebPi token inventory is malformed")

    baseline: list[str] | None = None
    for item in tokens:
        if not isinstance(item, dict):
            continue
        if item.get("name") == "chatgpt-action" and item.get("revoked_at") is None:
            scopes = item.get("scopes")
            if isinstance(scopes, list) and all(isinstance(scope, str) for scope in scopes):
                baseline = list(scopes)
                break
    if baseline is None:
        raise RuntimeError("WebPi could not find the pairing-created chatgpt-action scope baseline")

    scopes = action_token_scopes(baseline)
    existing_token = None
    stale_ids: list[str] = []
    if ACTION_TOKEN_FILE.is_file():
        existing_token = ACTION_TOKEN_FILE.read_text(encoding="utf-8").strip()
        if existing_token.startswith("wc_pat_"):
            current = current_action_token_record(tokens, existing_token, scopes)
            if current is not None:
                stale_ids = stale_action_token_ids(tokens, existing_token)
                manifest["action_token_file"] = str(ACTION_TOKEN_FILE)
                manifest["action_token_scopes"] = scopes
                if stale_ids:
                    manifest["action_token_rotation_pending_revoke"] = stale_ids
                else:
                    manifest.pop("action_token_rotation_pending_revoke", None)
                write_manifest(manifest)
                return
            for item in tokens:
                if not isinstance(item, dict):
                    continue
                if item.get("name") != ACTION_TOKEN_NAME or item.get("revoked_at") is not None:
                    continue
                token_id = item.get("id")
                if isinstance(token_id, str) and token_id:
                    stale_ids.append(token_id)

    created = admin_post(
        "/api/tokens/create",
        {"username": USERNAME, "name": ACTION_TOKEN_NAME, "scopes": scopes},
    )
    token = created.get("token")
    if not isinstance(token, str) or not token.startswith("wc_pat_"):
        raise RuntimeError("WebPi action token creation returned no PAT")

    STATE.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".webpi-action-", dir=STATE)
    temp = Path(temporary)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as stream:
            _copy_private_permissions(SERVER_ENV, temp)
            stream.write(token + "\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp, ACTION_TOKEN_FILE)
    finally:
        temp.unlink(missing_ok=True)
    manifest["action_token_file"] = str(ACTION_TOKEN_FILE)
    manifest["action_token_scopes"] = scopes
    if stale_ids:
        manifest["action_token_rotation_pending_revoke"] = stale_ids
    else:
        manifest.pop("action_token_rotation_pending_revoke", None)
    write_manifest(manifest)


def ensure_server_state() -> None:
    if not SERVER_ENV.is_file():
        require_file(CLI, "WebPi CLI")
        SERVER_DIR.mkdir(parents=True, exist_ok=True)
        result = run_capture(
            [
                str(CLI),
                "server",
                "init",
                "--listen",
                f"127.0.0.1:{PORT}",
                "--data-dir",
                str(SERVER_DATA),
                "--env-file",
                str(SERVER_ENV),
                "--json",
            ]
        )
        value = json.loads(result.stdout)
        if value.get("listen") != f"127.0.0.1:{PORT}":
            raise RuntimeError("WebPi server init returned an unexpected listen address")
    harden_server_env()


def enroll() -> None:
    ensure_server_state()
    require_file(CLI, "WebPi CLI")
    require_file(NODE, "WebPi portable Node")
    require_file(PI_BRIDGE, "compiled WebPi Pi bridge")
    if not server_is_online():
        raise RuntimeError(
            f"WebPi Server is not reachable at {SERVER_URL}. Start it first with: "
            f"{sys.executable} {Path(__file__).name} server"
        )

    existing = load_manifest()
    if existing is not None:
        runner_config = Path(str(existing.get("runner_config", "")))
        if runner_config.is_file():
            provision_action_token(existing)
            json_print({"status": "already-enrolled", **safe_status(), "action_token_ready": True})
            return
        raise RuntimeError("WebPi manifest exists but its runner_config is missing; refusing to guess")

    CONNECTIONS.mkdir(parents=True, exist_ok=True)
    pairing = run_capture(
        [
            str(CLI),
            "pairing",
            "create",
            "--server-url",
            SERVER_URL,
            "--env-file",
            str(SERVER_ENV),
            "--username",
            USERNAME,
            "--client-id",
            CLIENT_ID,
            "--display-name",
            DISPLAY_NAME,
            "--ttl-secs",
            "600",
            "--json",
        ]
    )
    pairing_value = json.loads(pairing.stdout)
    code = pairing_value.get("pairing_code")
    if not isinstance(code, str) or not code.startswith("wc_pair_"):
        raise RuntimeError("WebPi pairing did not return a valid one-time code")

    login = run_capture(
        [
            str(CLI),
            "login",
            SERVER_URL,
            "--code-stdin",
            "--device",
            CLIENT_ID,
            "--allowed-root",
            str(ROOT),
            "--project",
            str(ROOT),
            "--transport",
            "websocket",
            "--dir",
            str(CONNECTIONS),
            "--json",
        ],
        stdin=code + "\n",
    )
    login_value = json.loads(login.stdout)
    runner_config_raw = login_value.get("runner_config")
    user_token_file_raw = login_value.get("user_token_file")
    registry_raw = login_value.get("project_registry_dir")
    if not all(isinstance(value, str) and value for value in [runner_config_raw, user_token_file_raw, registry_raw]):
        raise RuntimeError("WebPi login did not return the expected safe path metadata")

    runner_config = Path(runner_config_raw)
    require_file(runner_config, "WebPi Runner config")
    run_capture(
        [
            sys.executable,
            str(INSTALL_PROVIDER),
            "install",
            str(runner_config),
            str(NODE),
            str(PI_BRIDGE),
            str(ROOT),
        ]
    )

    manifest = {
        "version": 1,
        "server_url": SERVER_URL,
        "client_id": CLIENT_ID,
        "username": USERNAME,
        "runner_config": str(runner_config),
        "user_token_file": str(user_token_file_raw),
        "project_registry_dir": str(registry_raw),
        "project_root": str(ROOT),
        "pi_bridge": str(PI_BRIDGE),
        "node": str(NODE),
    }
    write_manifest(manifest)
    provision_action_token(manifest)
    json_print(
        {
            "status": "enrolled",
            "server_url": SERVER_URL,
            "client_id": CLIENT_ID,
            "runner_config": str(runner_config),
            "project_root": str(ROOT),
            "pi_bridge": str(PI_BRIDGE),
            "action_token_ready": True,
        }
    )


def runner_config() -> Path:
    manifest = load_manifest()
    if manifest is None:
        raise RuntimeError("WebPi is not enrolled; run standalone.py enroll")
    value = manifest.get("runner_config")
    if not isinstance(value, str) or not value:
        raise RuntimeError("WebPi manifest has no runner_config")
    result = Path(value)
    require_file(result, "WebPi Runner config")
    return result


def configure_tunnel_interactive() -> None:
    tunnel_id = input("WebPi OpenAI Tunnel ID: ").strip()
    api_key = getpass.getpass("WebPi Restricted Tunnel API key: ").strip()
    tunnel_client_bin = input("Optional absolute tunnel-client path (blank = managed/PATH): ").strip() or None
    path = save_tunnel_credentials(STATE, tunnel_id, api_key, tunnel_client_bin)
    json_print(
        {
            "status": "saved",
            "path": str(path),
            "tunnel_configured": True,
            "source": "file",
        }
    )


def run_pi_admin(arguments: list[str]) -> int:
    require_file(NODE, "WebPi portable Node")
    require_file(PI_ADMIN, "WebPi Pi admin")
    if not arguments:
        raise RuntimeError(
            "usage: webpi.cmd pi <trust-status|trust-set|package-list|package-install|package-remove|package-update|extension-candidates|extension-approvals|extension-approve|extension-revoke>"
        )
    env = isolated_child_environment("admin")
    env["WEBPI_PI_AGENT_DIR"] = str(PI_AGENT_DIR)
    return subprocess.call(
        [str(NODE), str(PI_ADMIN), *arguments],
        cwd=ROOT,
        env=env,
        shell=False,
    )


def foreground_server() -> int:
    ensure_server_state()
    require_file(CLI, "WebPi CLI")
    return subprocess.call([str(CLI), "server", "run", "--env-file", str(SERVER_ENV)], cwd=ROOT, env=isolated_child_environment("server"), shell=False)


def foreground_runner() -> int:
    require_file(RUNNER_BIN, "WebPi Runner")
    return subprocess.call([str(RUNNER_BIN), "--config", str(runner_config())], cwd=ROOT, env=isolated_child_environment("runner"), shell=False)


def foreground_tunnel() -> int:
    ensure_server_state()
    require_file(CLI, "WebPi CLI")
    if not server_is_online():
        raise RuntimeError(f"WebPi Server is not reachable at {SERVER_URL}")
    require_authenticated_origin(SERVER_URL)
    spec = tunnel_process_spec()
    return subprocess.call(spec["argv"], cwd=ROOT, env=spec["env"], shell=False)


def wait_for_server(process: subprocess.Popen[object], timeout: float = 15.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"WebPi Server exited early with code {process.returncode}")
        if server_is_online():
            require_authenticated_origin(SERVER_URL)
            return
        time.sleep(0.2)
    raise RuntimeError("WebPi Server did not become ready before the startup deadline")


def terminate_group(process: subprocess.Popen[object]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        try:
            process.send_signal(signal.CTRL_BREAK_EVENT)
            process.wait(timeout=5)
            return
        except (OSError, subprocess.TimeoutExpired):
            subprocess.run(
                [r"C:\Windows\System32\taskkill.exe", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
    else:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()


def run_both() -> int:
    ensure_server_state()
    config = runner_config()
    require_file(CLI, "WebPi CLI")
    require_file(RUNNER_BIN, "WebPi Runner")
    if server_is_online():
        raise RuntimeError(
            f"{SERVER_URL} is already serving OpenAPI. Stop the existing WebPi Server "
            "or run only the Runner with standalone.py runner."
        )

    flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    containment = _create_process_containment()
    server: subprocess.Popen[object] | None = None
    runner: subprocess.Popen[object] | None = None
    try:
        server = subprocess.Popen(
            [str(CLI), "server", "run", "--env-file", str(SERVER_ENV)],
            cwd=ROOT,
            creationflags=flags,
            shell=False,
            env=isolated_child_environment("server"),
        )
        containment.assign(server)
        wait_for_server(server)
        runner = subprocess.Popen(
            [str(RUNNER_BIN), "--config", str(config)],
            cwd=ROOT,
            creationflags=flags,
            shell=False,
            env=isolated_child_environment("runner"),
        )
        containment.assign(runner)
        json_print(
            {
                "status": "running",
                "server_url": SERVER_URL,
                "server_pid": server.pid,
                "runner_pid": runner.pid,
                "client_id": CLIENT_ID,
                "state_dir": str(STATE),
            }
        )
        while True:
            server_code = server.poll()
            runner_code = runner.poll()
            if server_code is not None or runner_code is not None:
                return server_code if server_code is not None else int(runner_code or 0)
            time.sleep(0.5)
    except KeyboardInterrupt:
        return 0
    finally:
        if runner is not None:
            terminate_group(runner)
        if server is not None:
            terminate_group(server)
        containment.close()


def run_web() -> int:
    ensure_server_state()
    config = runner_config()
    require_file(CLI, "WebPi CLI")
    require_file(RUNNER_BIN, "WebPi Runner")
    tunnel_spec = tunnel_process_spec()
    if server_is_online():
        raise RuntimeError(
            f"{SERVER_URL} is already serving OpenAPI. Stop the existing WebPi Server before run-web."
        )

    flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    containment = _create_process_containment()
    server: subprocess.Popen[object] | None = None
    runner: subprocess.Popen[object] | None = None
    tunnel: subprocess.Popen[object] | None = None
    try:
        server = subprocess.Popen(
            [str(CLI), "server", "run", "--env-file", str(SERVER_ENV)],
            cwd=ROOT,
            creationflags=flags,
            env=isolated_child_environment("server"),
            shell=False,
        )
        containment.assign(server)
        wait_for_server(server)
        runner = subprocess.Popen(
            [str(RUNNER_BIN), "--config", str(config)],
            cwd=ROOT,
            creationflags=flags,
            env=isolated_child_environment("runner"),
            shell=False,
        )
        containment.assign(runner)
        tunnel = subprocess.Popen(
            tunnel_spec["argv"],
            cwd=ROOT,
            creationflags=flags,
            env=tunnel_spec["env"],
            stdin=subprocess.PIPE,
            shell=False,
        )
        containment.assign(tunnel)
        json_print(
            {
                "status": "running-web",
                "server_url": SERVER_URL,
                "server_pid": server.pid,
                "runner_pid": runner.pid,
                "tunnel_pid": tunnel.pid,
                "client_id": CLIENT_ID,
                "state_dir": str(STATE),
            }
        )
        while True:
            for process in (server, runner, tunnel):
                code = process.poll()
                if code is not None:
                    return int(code)
            time.sleep(0.5)
    except KeyboardInterrupt:
        return 0
    finally:
        if tunnel is not None and tunnel.poll() is None:
            if tunnel.stdin is not None:
                try:
                    tunnel.stdin.close()
                    tunnel.wait(timeout=5)
                except (OSError, subprocess.TimeoutExpired):
                    pass
            terminate_group(tunnel)
        if runner is not None:
            terminate_group(runner)
        if server is not None:
            terminate_group(server)
        containment.close()


def main() -> int:
    action = sys.argv[1] if len(sys.argv) > 1 else "status"
    if action == "init":
        ensure_server_state()
        json_print({"status": "initialized", **safe_status()})
        return 0
    if action == "status":
        json_print(safe_status())
        return 0
    if action == "doctor":
        report = doctor_status()
        json_print(report)
        return 1 if report["status"] == "fail" else 0
    if action == "migrate-config":
        if sys.argv[2:] not in ([], ["--check"]):
            raise SystemExit("usage: webpi.cmd migrate-config [--check]")
        apply = sys.argv[2:] != ["--check"]
        if apply and server_is_online():
            raise RuntimeError("Stop the existing WebPi instance before migrating its configuration")
        json_print(migrate_env_file(SERVER_ENV, ROOT, apply=apply))
        return 0
    if action == "tunnel-config":
        configure_tunnel_interactive()
        return 0
    if action in {"cloudflare-config", "public-url-config"}:
        if len(sys.argv) != 3:
            raise SystemExit(f"usage: standalone.py {action} https://webpi.example.com")
        configure_public_url(sys.argv[2])
        return 0
    if action == "verify":
        from security_smoke import main as verify_main
        return verify_main(sys.argv[2:])
    if action == "pi":
        return run_pi_admin(sys.argv[2:])
    if action == "enroll":
        enroll()
        return 0
    if action == "server":
        return foreground_server()
    if action == "runner":
        return foreground_runner()
    if action == "tunnel":
        return foreground_tunnel()
    if action == "run":
        return run_both()
    if action == "run-web":
        return run_web()
    raise SystemExit("usage: standalone.py [init|status|doctor|verify|migrate-config|cloudflare-config|public-url-config|tunnel-config|pi|enroll|server|runner|tunnel|run|run-web]")


if __name__ == "__main__":
    raise SystemExit(main())
