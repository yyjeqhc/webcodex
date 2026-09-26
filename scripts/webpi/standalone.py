from __future__ import annotations

import json
import getpass
import hashlib
import os
import signal
import socket
import sqlite3
import shutil
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, replace
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
from security_smoke import request_json as security_request_json, require_authenticated_origin
from config_migration import migrate_env_file, _copy_private_permissions
from supervisor_protocol import (
    REQUEST_FILE as SUPERVISOR_REQUEST_FILE,
    REQUEST_LOCK_FILE as SUPERVISOR_REQUEST_LOCK_FILE,
    DeployRequest as SupervisorDeployRequest,
    RollbackRequest as SupervisorRollbackRequest,
    SupervisorProtocolError,
    atomic_write_json as supervisor_atomic_write_json,
    load_or_create_token as supervisor_load_or_create_token,
    read_supervisor_request,
    result_envelope as supervisor_result_envelope,
    result_path as supervisor_result_path,
)

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
SUPERVISOR_DIR = STATE / "supervisor"
SUPERVISOR_TOKEN_ENV = "WEBPI_SUPERVISOR_TOKEN"
SUPERVISOR_CONTROL_DIR_ENV = "WEBPI_SUPERVISOR_CONTROL_DIR"
SUPERVISOR_CLIENT_ID_ENV = "WEBPI_SUPERVISOR_CLIENT_ID"
SUPERVISOR_CANDIDATE_ROOT_ENV = "WEBPI_SUPERVISOR_CANDIDATE_ROOT"
SUPERVISOR_STAGING_ROOT_ENV = "WEBPI_SUPERVISOR_STAGING_ROOT"
SUPERVISOR_BACKUP_ROOT_ENV = "WEBPI_SUPERVISOR_BACKUP_ROOT"
SUPERVISOR_INSTALLED_RUNTIME_ENV = "WEBPI_SUPERVISOR_INSTALLED_RUNTIME"
SERVICE_CANDIDATE_ROOT = ROOT / "target" / "service-candidates"
DEPLOYMENT_BACKUP_ROOT = STATE / "deployment-backups"
DEPLOYMENT_STAGING_ROOT = STATE / "deployment-staging"
INSTALLED_RUNTIME_DIR = ROOT / "target" / "dogfood"
DEPLOY_ARTIFACT_NAMES = ("webpi.exe", "webpi-server.exe", "webpi-runner.exe")
ROLLBACK_MANIFEST_VERSION = 2
PI_BRIDGE_PACKAGE = ROOT / "plugins" / "pi-bridge" / "package.json"
_BUILD_VERSION_RE = re.compile(
    r"^(?P<name>\S+)\s+(?P<version>\S+)\s+\(commit\s+(?P<commit>[0-9a-fA-F]{7,40}),\s*dirty=(?P<dirty>true|false),\s*built_at=(?P<built_at>\d+)\)$"
)


class _NoopProcessContainment:
    def assign(self, process: subprocess.Popen[object]) -> None:
        del process

    def close(self) -> None:
        return

    def popen(self, args: object, **kwargs: object) -> subprocess.Popen[object]:
        return subprocess.Popen(args, **kwargs)


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

    def _resume_suspended_process(self, process: subprocess.Popen[object]) -> None:
        """Resume every thread in a process created with CREATE_SUSPENDED."""
        class THREADENTRY32(self._ctypes.Structure):
            _fields_ = [
                ("dwSize", self._wintypes.DWORD),
                ("cntUsage", self._wintypes.DWORD),
                ("th32ThreadID", self._wintypes.DWORD),
                ("th32OwnerProcessID", self._wintypes.DWORD),
                ("tpBasePri", self._wintypes.LONG),
                ("tpDeltaPri", self._wintypes.LONG),
                ("dwFlags", self._wintypes.DWORD),
            ]

        create_snapshot = self._kernel32.CreateToolhelp32Snapshot
        create_snapshot.argtypes = [self._wintypes.DWORD, self._wintypes.DWORD]
        create_snapshot.restype = self._wintypes.HANDLE
        thread_first = self._kernel32.Thread32First
        thread_first.argtypes = [self._wintypes.HANDLE, self._ctypes.POINTER(THREADENTRY32)]
        thread_first.restype = self._wintypes.BOOL
        thread_next = self._kernel32.Thread32Next
        thread_next.argtypes = [self._wintypes.HANDLE, self._ctypes.POINTER(THREADENTRY32)]
        thread_next.restype = self._wintypes.BOOL
        open_thread = self._kernel32.OpenThread
        open_thread.argtypes = [self._wintypes.DWORD, self._wintypes.BOOL, self._wintypes.DWORD]
        open_thread.restype = self._wintypes.HANDLE
        resume_thread = self._kernel32.ResumeThread
        resume_thread.argtypes = [self._wintypes.HANDLE]
        resume_thread.restype = self._wintypes.DWORD

        snapshot = create_snapshot(0x00000004, 0)  # TH32CS_SNAPTHREAD
        invalid_handle = self._ctypes.c_void_p(-1).value
        if not snapshot or snapshot == invalid_handle:
            raise self._ctypes.WinError(self._ctypes.get_last_error())
        resumed = 0
        try:
            entry = THREADENTRY32()
            entry.dwSize = self._ctypes.sizeof(entry)
            ok = thread_first(snapshot, self._ctypes.byref(entry))
            while ok:
                if entry.th32OwnerProcessID == process.pid:
                    thread = open_thread(0x0002, False, entry.th32ThreadID)  # THREAD_SUSPEND_RESUME
                    if not thread:
                        raise self._ctypes.WinError(self._ctypes.get_last_error())
                    try:
                        if resume_thread(thread) == 0xFFFFFFFF:
                            raise self._ctypes.WinError(self._ctypes.get_last_error())
                        resumed += 1
                    finally:
                        self._kernel32.CloseHandle(thread)
                ok = thread_next(snapshot, self._ctypes.byref(entry))
        finally:
            self._kernel32.CloseHandle(snapshot)
        if resumed == 0:
            raise RuntimeError(f"Could not find a suspended thread for child process {process.pid}")

    def popen(self, args: object, **kwargs: object) -> subprocess.Popen[object]:
        """Create a child suspended, attach it to the Job, then let it execute."""
        creationflags = int(kwargs.pop("creationflags", 0)) | 0x00000004  # CREATE_SUSPENDED
        process = subprocess.Popen(args, creationflags=creationflags, **kwargs)
        try:
            self.assign(process)
            self._resume_suspended_process(process)
        except BaseException:
            try:
                process.kill()
                process.wait(timeout=5)
            except (OSError, subprocess.TimeoutExpired):
                pass
            raise
        return process

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


def _prepare_supervisor() -> tuple[Path, str]:
    control_dir = SUPERVISOR_DIR
    control_dir.mkdir(parents=True, exist_ok=True)
    try:
        control_dir.chmod(0o700)
    except OSError:
        pass
    # An interrupted in-flight request is never replayed automatically by a new
    # parent. The private supervisor key is persisted, however, so an already
    # written signed result remains verifiable after parent recovery.
    for stale_name in (SUPERVISOR_REQUEST_FILE, SUPERVISOR_REQUEST_LOCK_FILE):
        try:
            (control_dir / stale_name).unlink()
        except FileNotFoundError:
            pass
    return control_dir, supervisor_load_or_create_token(control_dir)


def _server_environment(control_dir: Path | None = None, token: str | None = None) -> dict[str, str]:
    env = isolated_child_environment("server")
    if control_dir is not None and token is not None:
        env[SUPERVISOR_CONTROL_DIR_ENV] = str(control_dir)
        env[SUPERVISOR_TOKEN_ENV] = token
        env[SUPERVISOR_CLIENT_ID_ENV] = CLIENT_ID
        env[SUPERVISOR_CANDIDATE_ROOT_ENV] = str(SERVICE_CANDIDATE_ROOT)
        env[SUPERVISOR_STAGING_ROOT_ENV] = str(DEPLOYMENT_STAGING_ROOT)
        env[SUPERVISOR_BACKUP_ROOT_ENV] = str(DEPLOYMENT_BACKUP_ROOT)
        env[SUPERVISOR_INSTALLED_RUNTIME_ENV] = str(INSTALLED_RUNTIME_DIR)
    return env


def _supervisor_request_due(request: object) -> float:
    requested_at_ms = int(getattr(request, "requested_at_ms"))
    execute_after_ms = int(getattr(request, "execute_after_ms"))
    return (requested_at_ms + execute_after_ms) / 1000.0


def _wait_until_supervisor_request_due(request: object) -> None:
    remaining = _supervisor_request_due(request) - time.time()
    if remaining > 0:
        time.sleep(min(remaining, 10.0))


def _consume_supervisor_request(control_dir: Path, token: str):
    try:
        request = read_supervisor_request(control_dir, token)
    except (OSError, ValueError, SupervisorProtocolError) as exc:
        # Invalid/tampered requests must not wedge the supervisor loop. Delete the
        # closed-vocabulary request file and keep the runtime online.
        for rejected_name in (SUPERVISOR_REQUEST_FILE, SUPERVISOR_REQUEST_LOCK_FILE):
            try:
                (control_dir / rejected_name).unlink()
            except OSError:
                pass
        print(f"WebPi supervisor rejected request: {type(exc).__name__}", file=sys.stderr)
        return None
    if request is None:
        return None
    try:
        (control_dir / SUPERVISOR_REQUEST_FILE).unlink()
    except OSError as exc:
        raise RuntimeError("WebPi supervisor could not consume the request") from exc
    return request


def _consume_restart_request(control_dir: Path, token: str):
    """Compatibility wrapper for older tests/callers; deploy-aware loops use the generic reader."""
    request = _consume_supervisor_request(control_dir, token)
    if request is not None and isinstance(request, SupervisorDeployRequest):
        raise RuntimeError("WebPi supervisor request is deploy, not restart")
    return request


def _write_supervisor_result(
    control_dir: Path,
    token: str,
    request: object,
    *,
    status: str,
    error_code: str | None = None,
    backup_id: str | None = None,
) -> None:
    path = supervisor_result_path(control_dir, str(getattr(request, "receipt_id")))
    supervisor_atomic_write_json(
        path,
        supervisor_result_envelope(
            request,
            token,
            status=status,
            error_code=error_code,
            backup_id=backup_id,
        ),
    )
    try:
        (control_dir / SUPERVISOR_REQUEST_LOCK_FILE).unlink()
    except FileNotFoundError:
        pass
    except OSError as exc:
        raise RuntimeError("WebPi supervisor wrote a result but could not release the request lock") from exc

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


def _bootstrap_key_from_env(path: Path) -> str:
    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"').strip("'")
    result = values.get("WEBPI_TOKEN") or values.get("WEBPI_SHARED_KEY")
    if not result:
        raise RuntimeError("WebPi server env has no bootstrap/admin key")
    return result


def bootstrap_key() -> str:
    return _bootstrap_key_from_env(SERVER_ENV)


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


def local_action_post(
    tool: str,
    payload: dict[str, object],
    *,
    timeout: float = 10.0,
) -> dict[str, object]:
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
        with urllib.request.urlopen(request, timeout=timeout) as response:
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


@dataclass(frozen=True)
class _PreparedDeployment:
    candidate_id: str
    staged_dir: Path
    backup_id: str
    backup_dir: Path


def _path_is_linklike(path: Path) -> bool:
    if path.is_symlink():
        return True
    is_junction = getattr(path, "is_junction", None)
    return bool(is_junction()) if callable(is_junction) else False


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _file_identity(path: Path, purpose: str) -> dict[str, object]:
    if _path_is_linklike(path) or not path.is_file():
        raise RuntimeError(f"{purpose} must be a regular non-linked file")
    stat = path.stat()
    return {"sha256": _sha256_file(path), "size_bytes": stat.st_size}


def _binary_build_identity(path: Path) -> dict[str, object]:
    try:
        completed = subprocess.run(
            [str(path), "--version"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=5,
            check=True,
            shell=False,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise RuntimeError("installed WebPi runtime build identity is unavailable") from exc
    line = completed.stdout.strip().splitlines()
    if len(line) != 1:
        raise RuntimeError("installed WebPi runtime build identity is invalid")
    match = _BUILD_VERSION_RE.fullmatch(line[0])
    if match is None:
        raise RuntimeError("installed WebPi runtime build identity is invalid")
    return {
        "binary": path.name,
        "product": match.group("name"),
        "version": match.group("version"),
        "git_commit": match.group("commit").lower(),
        "git_dirty": match.group("dirty") == "true",
        "built_at": int(match.group("built_at")),
    }


def _sqlite_schema_identity(path: Path) -> dict[str, object]:
    if _path_is_linklike(path) or not path.is_file():
        raise RuntimeError("WebPi database must be a regular non-linked file")
    try:
        uri = path.resolve(strict=True).as_uri() + "?mode=ro"
        connection = sqlite3.connect(uri, uri=True, timeout=2.0)
        try:
            user_version = int(connection.execute("PRAGMA user_version").fetchone()[0])
            schema_version = int(connection.execute("PRAGMA schema_version").fetchone()[0])
            rows = connection.execute(
                "SELECT type, name, tbl_name, sql FROM sqlite_schema "
                "WHERE sql IS NOT NULL ORDER BY type, name, tbl_name"
            ).fetchall()
        finally:
            connection.close()
    except sqlite3.Error as exc:
        raise RuntimeError("WebPi database schema identity is unavailable") from exc
    encoded = json.dumps(rows, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    return {
        "user_version": user_version,
        "schema_version": schema_version,
        "schema_sha256": hashlib.sha256(encoded).hexdigest(),
    }


def _pi_bridge_identity(
    entrypoint: Path = PI_BRIDGE,
    package_manifest: Path = PI_BRIDGE_PACKAGE,
) -> dict[str, object]:
    try:
        package = json.loads(package_manifest.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise RuntimeError("WebPi Pi bridge package identity is unavailable") from exc
    if not isinstance(package, dict):
        raise RuntimeError("WebPi Pi bridge package identity is invalid")
    name = package.get("name")
    version = package.get("version")
    dependencies = package.get("dependencies")
    if not isinstance(name, str) or not name or not isinstance(version, str) or not version:
        raise RuntimeError("WebPi Pi bridge package identity is invalid")
    dependency_version = None
    if isinstance(dependencies, dict):
        value = dependencies.get("@earendil-works/pi-coding-agent")
        dependency_version = value if isinstance(value, str) else None
    return {
        "provider_id": "pi-bridge",
        "package_name": name,
        "package_version": version,
        "pi_coding_agent_version": dependency_version,
        "package_manifest": _file_identity(package_manifest, "WebPi Pi bridge package manifest"),
        "entrypoint": _file_identity(entrypoint, "WebPi Pi bridge entrypoint"),
    }


def _capture_rollback_consistency_identity(
    *,
    enrollment_manifest: Path = MANIFEST,
    server_env: Path = SERVER_ENV,
    runner_config_path: Path | None = None,
    database_path: Path | None = None,
    pi_bridge: Path = PI_BRIDGE,
    pi_package: Path = PI_BRIDGE_PACKAGE,
) -> dict[str, object]:
    try:
        enrollment = json.loads(enrollment_manifest.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise RuntimeError("WebPi enrollment manifest identity is unavailable") from exc
    if not isinstance(enrollment, dict) or not isinstance(enrollment.get("version"), int):
        raise RuntimeError("WebPi enrollment manifest identity is invalid")
    if runner_config_path is None:
        raw = enrollment.get("runner_config")
        if not isinstance(raw, str) or not raw:
            raise RuntimeError("WebPi enrollment manifest has no Runner config identity")
        runner_config_path = Path(raw)
    if database_path is None:
        database_path = SERVER_DATA / "webcodex.db"
    return {
        "configuration": {
            "enrollment_manifest": {
                "schema_version": int(enrollment["version"]),
                **_file_identity(enrollment_manifest, "WebPi enrollment manifest"),
            },
            "server_env": _file_identity(server_env, "WebPi server environment"),
            "runner_config": _file_identity(runner_config_path, "WebPi Runner config"),
        },
        "database_schema": _sqlite_schema_identity(database_path),
        "plugin": _pi_bridge_identity(pi_bridge, pi_package),
    }


def _validate_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(ch in "0123456789abcdef" for ch in value)
    )


def _validate_file_identity_value(value: object) -> bool:
    return (
        isinstance(value, dict)
        and set(value) == {"sha256", "size_bytes"}
        and _validate_sha256(value.get("sha256"))
        and isinstance(value.get("size_bytes"), int)
        and not isinstance(value.get("size_bytes"), bool)
        and int(value["size_bytes"]) > 0
    )


def _validate_rollback_consistency_identity(value: object) -> bool:
    if not isinstance(value, dict) or set(value) != {"configuration", "database_schema", "plugin"}:
        return False
    configuration = value.get("configuration")
    database = value.get("database_schema")
    plugin = value.get("plugin")
    if not isinstance(configuration, dict) or set(configuration) != {
        "enrollment_manifest", "server_env", "runner_config"
    }:
        return False
    enrollment = configuration.get("enrollment_manifest")
    if (
        not isinstance(enrollment, dict)
        or set(enrollment) != {"schema_version", "sha256", "size_bytes"}
        or not isinstance(enrollment.get("schema_version"), int)
        or isinstance(enrollment.get("schema_version"), bool)
        or not _validate_sha256(enrollment.get("sha256"))
        or not isinstance(enrollment.get("size_bytes"), int)
        or isinstance(enrollment.get("size_bytes"), bool)
        or int(enrollment["size_bytes"]) <= 0
    ):
        return False
    if not _validate_file_identity_value(configuration.get("server_env")):
        return False
    if not _validate_file_identity_value(configuration.get("runner_config")):
        return False
    if (
        not isinstance(database, dict)
        or set(database) != {"user_version", "schema_version", "schema_sha256"}
        or not isinstance(database.get("user_version"), int)
        or isinstance(database.get("user_version"), bool)
        or not isinstance(database.get("schema_version"), int)
        or isinstance(database.get("schema_version"), bool)
        or not _validate_sha256(database.get("schema_sha256"))
    ):
        return False
    if not isinstance(plugin, dict) or set(plugin) != {
        "provider_id", "package_name", "package_version", "pi_coding_agent_version",
        "package_manifest", "entrypoint"
    }:
        return False
    if plugin.get("provider_id") != "pi-bridge":
        return False
    if not isinstance(plugin.get("package_name"), str) or not plugin.get("package_name"):
        return False
    if not isinstance(plugin.get("package_version"), str) or not plugin.get("package_version"):
        return False
    dependency = plugin.get("pi_coding_agent_version")
    if dependency is not None and not isinstance(dependency, str):
        return False
    return _validate_file_identity_value(plugin.get("package_manifest")) and _validate_file_identity_value(plugin.get("entrypoint"))


def _assert_bounded_child(root: Path, child: Path) -> Path:
    root_resolved = root.resolve(strict=True)
    child_resolved = child.resolve(strict=True)
    try:
        child_resolved.relative_to(root_resolved)
    except ValueError as exc:
        raise RuntimeError("WebPi deployment path escaped its fixed root") from exc
    return child_resolved


def _create_bounded_direct_child_dir(root: Path, name: str) -> Path:
    """Atomically create one new direct child under an existing fixed root.

    `_assert_bounded_child` intentionally requires an already-existing child so
    existing deployment paths can be canonicalized strictly. New backup
    directories need the opposite ordering: validate the existing root and the
    single basename, create atomically with `exist_ok=False`, then apply the same
    strict canonical boundary check before any payload is written.
    """
    if (
        not name
        or name in {".", ".."}
        or Path(name).name != name
        or any(ch not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-" for ch in name)
    ):
        raise RuntimeError("WebPi deployment child name is invalid")
    if _path_is_linklike(root) or not root.is_dir():
        raise RuntimeError("WebPi deployment root must be an existing non-linked directory")
    root_resolved = root.resolve(strict=True)
    child = root_resolved / name
    if child.exists() or _path_is_linklike(child):
        raise RuntimeError("WebPi deployment child already exists")
    child.mkdir(parents=False, exist_ok=False)
    try:
        if _path_is_linklike(child):
            raise RuntimeError("WebPi deployment child must not be a link or junction")
        bounded = _assert_bounded_child(root_resolved, child)
        if bounded.parent != root_resolved:
            raise RuntimeError("WebPi deployment child is not a direct child of its fixed root")
        return bounded
    except Exception:
        try:
            if child.is_dir() and not _path_is_linklike(child):
                child.rmdir()
        except OSError:
            pass
        raise


def _verify_deploy_artifact(path: Path, *, expected_sha256: str, expected_size: int) -> None:
    if _path_is_linklike(path) or not path.is_file():
        raise RuntimeError("WebPi deployment artifact must be a regular non-linked file")
    stat = path.stat()
    if stat.st_size != expected_size:
        raise RuntimeError("WebPi deployment artifact size mismatch")
    if _sha256_file(path) != expected_sha256:
        raise RuntimeError("WebPi deployment artifact hash mismatch")


def _artifact_expectations(request: SupervisorDeployRequest) -> dict[str, tuple[str, int]]:
    return {
        artifact.name: (artifact.sha256, artifact.size_bytes)
        for artifact in request.artifacts
    }


def _capture_runtime_builds(installed_dir: Path) -> dict[str, dict[str, object]]:
    builds: dict[str, dict[str, object]] = {}
    for name in DEPLOY_ARTIFACT_NAMES:
        builds[name] = _binary_build_identity(installed_dir / name)
    comparable = {
        (
            value["version"],
            value["git_commit"],
            value["git_dirty"],
            value["built_at"],
        )
        for value in builds.values()
    }
    if len(comparable) != 1:
        raise RuntimeError("installed WebPi runtime binaries have mixed build identities")
    return builds


def _valid_build_identity(value: object, expected_name: str) -> bool:
    return (
        isinstance(value, dict)
        and set(value) == {"binary", "product", "version", "git_commit", "git_dirty", "built_at"}
        and value.get("binary") == expected_name
        and isinstance(value.get("product"), str)
        and bool(value.get("product"))
        and isinstance(value.get("version"), str)
        and bool(value.get("version"))
        and isinstance(value.get("git_commit"), str)
        and 7 <= len(str(value["git_commit"])) <= 40
        and all(ch in "0123456789abcdef" for ch in str(value["git_commit"]))
        and isinstance(value.get("git_dirty"), bool)
        and isinstance(value.get("built_at"), int)
        and not isinstance(value.get("built_at"), bool)
        and int(value["built_at"]) >= 0
    )


def _validate_rollback_manifest_shape(manifest: object, *, backup_id: str | None = None) -> dict[str, object]:
    if not isinstance(manifest, dict):
        raise RuntimeError("WebPi rollback backup manifest is invalid")
    version = manifest.get("version")
    if version != ROLLBACK_MANIFEST_VERSION:
        if version == 1:
            raise RuntimeError("legacy WebPi rollback manifest is not eligible for complete rollback")
        raise RuntimeError("unsupported WebPi rollback manifest version")
    required = {
        "version",
        "receipt_id",
        "receipt_revision",
        "backup_id",
        "artifacts",
        "consistency",
        "restorable_components",
        "consistency_fences",
    }
    if set(manifest) != required:
        raise RuntimeError("WebPi rollback backup manifest fields are invalid")
    if backup_id is not None and manifest.get("backup_id") != backup_id:
        raise RuntimeError("WebPi rollback backup identity is invalid")
    if (
        not isinstance(manifest.get("receipt_id"), str)
        or not str(manifest["receipt_id"]).startswith("wc_deploy_")
        or not isinstance(manifest.get("receipt_revision"), int)
        or isinstance(manifest.get("receipt_revision"), bool)
        or int(manifest["receipt_revision"]) < 1
        or not isinstance(manifest.get("backup_id"), str)
    ):
        raise RuntimeError("WebPi rollback backup identity is invalid")
    if manifest.get("restorable_components") != ["runtime_binaries"]:
        raise RuntimeError("WebPi rollback restorable component contract is invalid")
    if manifest.get("consistency_fences") != ["configuration", "database_schema", "plugin"]:
        raise RuntimeError("WebPi rollback consistency fence contract is invalid")
    if not _validate_rollback_consistency_identity(manifest.get("consistency")):
        raise RuntimeError("WebPi rollback consistency identity is invalid")
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list) or len(artifacts) != len(DEPLOY_ARTIFACT_NAMES):
        raise RuntimeError("WebPi rollback backup artifact set is invalid")
    by_name = {item.get("name"): item for item in artifacts if isinstance(item, dict)}
    if set(by_name) != set(DEPLOY_ARTIFACT_NAMES):
        raise RuntimeError("WebPi rollback backup artifact names are invalid")
    for name in DEPLOY_ARTIFACT_NAMES:
        item = by_name[name]
        if set(item) != {"name", "sha256", "size_bytes", "build"}:
            raise RuntimeError("WebPi rollback backup artifact metadata is invalid")
        if (
            not _validate_sha256(item.get("sha256"))
            or not isinstance(item.get("size_bytes"), int)
            or isinstance(item.get("size_bytes"), bool)
            or int(item["size_bytes"]) <= 0
            or not _valid_build_identity(item.get("build"), name)
        ):
            raise RuntimeError("WebPi rollback backup artifact metadata is invalid")
    return manifest


def _verify_current_rollback_consistency(manifest: Mapping[str, object]) -> None:
    expected = manifest.get("consistency")
    if not _validate_rollback_consistency_identity(expected):
        raise RuntimeError("WebPi rollback consistency identity is invalid")
    current = _capture_rollback_consistency_identity()
    if current != expected:
        raise RuntimeError("WebPi rollback consistency fence mismatch")


def _backup_manifest(
    request: SupervisorDeployRequest | SupervisorRollbackRequest,
    backup_id: str,
    installed_dir: Path,
    *,
    consistency_identity: Mapping[str, object] | None = None,
    runtime_builds: Mapping[str, Mapping[str, object]] | None = None,
) -> dict[str, object]:
    consistency = (
        dict(consistency_identity)
        if consistency_identity is not None
        else _capture_rollback_consistency_identity()
    )
    if not _validate_rollback_consistency_identity(consistency):
        raise RuntimeError("WebPi rollback consistency identity is invalid")
    builds = (
        {name: dict(value) for name, value in runtime_builds.items()}
        if runtime_builds is not None
        else _capture_runtime_builds(installed_dir)
    )
    if set(builds) != set(DEPLOY_ARTIFACT_NAMES):
        raise RuntimeError("WebPi rollback runtime build identity set is invalid")
    artifacts: list[dict[str, object]] = []
    for name in DEPLOY_ARTIFACT_NAMES:
        path = installed_dir / name
        if _path_is_linklike(path) or not path.is_file():
            raise RuntimeError("installed WebPi runtime artifact is not a regular file")
        build = builds[name]
        if not _valid_build_identity(build, name):
            raise RuntimeError("WebPi rollback runtime build identity is invalid")
        artifacts.append(
            {
                "name": name,
                "sha256": _sha256_file(path),
                "size_bytes": path.stat().st_size,
                "build": build,
            }
        )
    manifest = {
        "version": ROLLBACK_MANIFEST_VERSION,
        "receipt_id": request.receipt_id,
        "receipt_revision": request.receipt_revision,
        "backup_id": backup_id,
        "artifacts": artifacts,
        "consistency": consistency,
        "restorable_components": ["runtime_binaries"],
        "consistency_fences": ["configuration", "database_schema", "plugin"],
    }
    return _validate_rollback_manifest_shape(manifest, backup_id=backup_id)


def _verify_existing_backup(
    request: SupervisorDeployRequest | SupervisorRollbackRequest,
    backup_id: str,
    backup_dir: Path,
) -> None:
    manifest_path = backup_dir / "manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise RuntimeError("existing WebPi deployment backup manifest is invalid") from exc
    manifest = _validate_rollback_manifest_shape(manifest, backup_id=backup_id)
    if (
        manifest.get("receipt_id") != request.receipt_id
        or manifest.get("receipt_revision") != request.receipt_revision
    ):
        raise RuntimeError("existing WebPi deployment backup identity mismatch")
    by_name = {
        item["name"]: item
        for item in manifest["artifacts"]
        if isinstance(item, dict)
    }
    for name in DEPLOY_ARTIFACT_NAMES:
        item = by_name[name]
        _verify_deploy_artifact(
            backup_dir / name,
            expected_sha256=str(item["sha256"]),
            expected_size=int(item["size_bytes"]),
        )
    _verify_current_rollback_consistency(manifest)


def _load_verified_backup_by_id(
    backup_id: str,
    *,
    backup_root: Path = DEPLOYMENT_BACKUP_ROOT,
) -> tuple[Path, dict[str, object]]:
    if not backup_id or len(backup_id) > 128 or not backup_id[0].isalnum() or any(
        not (ch.isalnum() or ch in "._-") for ch in backup_id
    ):
        raise RuntimeError("WebPi rollback backup id is invalid")
    root = backup_root.resolve(strict=True)
    unresolved = backup_root / backup_id
    if _path_is_linklike(unresolved):
        raise RuntimeError("WebPi rollback backup must not be a link or junction")
    backup_dir = _assert_bounded_child(root, unresolved)
    if not backup_dir.is_dir():
        raise RuntimeError("WebPi rollback backup does not exist")
    manifest_path = backup_dir / "manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise RuntimeError("WebPi rollback backup manifest is invalid") from exc
    manifest = _validate_rollback_manifest_shape(manifest, backup_id=backup_id)
    by_name = {
        item["name"]: item
        for item in manifest["artifacts"]
        if isinstance(item, dict)
    }
    for name in DEPLOY_ARTIFACT_NAMES:
        item = by_name[name]
        _verify_deploy_artifact(
            backup_dir / name,
            expected_sha256=str(item["sha256"]),
            expected_size=int(item["size_bytes"]),
        )
    _verify_current_rollback_consistency(manifest)
    return backup_dir, manifest


def _create_receipt_runtime_backup(
    request: SupervisorDeployRequest | SupervisorRollbackRequest,
    *,
    backup_root: Path = DEPLOYMENT_BACKUP_ROOT,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
) -> tuple[str, Path]:
    suffix = request.receipt_id.removeprefix("wc_deploy_")
    backup_id = f"backup-{suffix}-r{request.receipt_revision}"
    backup_root.mkdir(parents=True, exist_ok=True)
    backup_dir = backup_root / backup_id
    if backup_dir.exists():
        _verify_existing_backup(request, backup_id, backup_dir)
        return backup_id, backup_dir
    backup_dir.mkdir(parents=False, exist_ok=False)
    try:
        manifest = _backup_manifest(request, backup_id, installed_dir)
        for name in DEPLOY_ARTIFACT_NAMES:
            shutil.copy2(installed_dir / name, backup_dir / name)
        supervisor_atomic_write_json(backup_dir / "manifest.json", manifest)
        _verify_existing_backup(request, backup_id, backup_dir)
    except Exception:
        shutil.rmtree(backup_dir, ignore_errors=True)
        raise
    return backup_id, backup_dir


def _restore_verified_backup_dir(
    backup_dir: Path,
    manifest: Mapping[str, object],
    *,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
) -> None:
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list):
        raise RuntimeError("WebPi rollback backup manifest artifacts are invalid")
    by_name = {item.get("name"): item for item in artifacts if isinstance(item, dict)}
    if set(by_name) != set(DEPLOY_ARTIFACT_NAMES):
        raise RuntimeError("WebPi rollback backup artifact names are invalid")
    for name in DEPLOY_ARTIFACT_NAMES:
        item = by_name[name]
        source = backup_dir / name
        _atomic_replace_runtime_file(source, installed_dir / name)
        _verify_deploy_artifact(
            installed_dir / name,
            expected_sha256=str(item["sha256"]).lower(),
            expected_size=int(item["size_bytes"]),
        )


def _prepare_deployment_snapshots(
    request: SupervisorDeployRequest,
    *,
    candidate_root: Path = SERVICE_CANDIDATE_ROOT,
    staging_root: Path = DEPLOYMENT_STAGING_ROOT,
    backup_root: Path = DEPLOYMENT_BACKUP_ROOT,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
) -> _PreparedDeployment:
    candidate_root = candidate_root.resolve(strict=True)
    candidate_component = candidate_root / request.candidate_id
    candidate_dir_unresolved = candidate_component / "dogfood"
    if _path_is_linklike(candidate_component) or _path_is_linklike(candidate_dir_unresolved):
        raise RuntimeError("WebPi deployment candidate must not traverse a link or junction")
    candidate_dir = _assert_bounded_child(candidate_root, candidate_dir_unresolved)
    expectations = _artifact_expectations(request)
    if set(expectations) != set(DEPLOY_ARTIFACT_NAMES):
        raise RuntimeError("WebPi deployment candidate artifact set is invalid")

    suffix = request.receipt_id.removeprefix("wc_deploy_")
    stage_id = f"stage-{suffix}-r{request.receipt_revision}"
    backup_id = f"backup-{suffix}-r{request.receipt_revision}"
    staging_root.mkdir(parents=True, exist_ok=True)
    backup_root.mkdir(parents=True, exist_ok=True)
    staged_dir = staging_root / stage_id
    if staged_dir.exists():
        shutil.rmtree(staged_dir)
    staged_dir.mkdir(parents=False, exist_ok=False)

    try:
        for name in DEPLOY_ARTIFACT_NAMES:
            source_unresolved = candidate_dir_unresolved / name
            if _path_is_linklike(source_unresolved):
                raise RuntimeError("WebPi deployment artifact must not be a link or junction")
            source = _assert_bounded_child(candidate_dir, source_unresolved)
            expected_sha256, expected_size = expectations[name]
            _verify_deploy_artifact(
                source,
                expected_sha256=expected_sha256,
                expected_size=expected_size,
            )
            destination = staged_dir / name
            shutil.copy2(source, destination)
            _verify_deploy_artifact(
                destination,
                expected_sha256=expected_sha256,
                expected_size=expected_size,
            )
    except Exception:
        shutil.rmtree(staged_dir, ignore_errors=True)
        raise

    backup_dir = backup_root / backup_id
    if backup_dir.exists():
        _verify_existing_backup(request, backup_id, backup_dir)
    else:
        backup_dir.mkdir(parents=False, exist_ok=False)
        try:
            manifest = _backup_manifest(request, backup_id, installed_dir)
            for name in DEPLOY_ARTIFACT_NAMES:
                shutil.copy2(installed_dir / name, backup_dir / name)
            supervisor_atomic_write_json(backup_dir / "manifest.json", manifest)
            _verify_existing_backup(request, backup_id, backup_dir)
        except Exception:
            shutil.rmtree(backup_dir, ignore_errors=True)
            raise

    return _PreparedDeployment(
        candidate_id=request.candidate_id,
        staged_dir=staged_dir,
        backup_id=backup_id,
        backup_dir=backup_dir,
    )


def _atomic_replace_runtime_file(source: Path, destination: Path) -> None:
    temp = destination.with_name(f".{destination.name}.webpi-deploy-{os.getpid()}.tmp")
    try:
        try:
            temp.unlink()
        except FileNotFoundError:
            pass
        shutil.copy2(source, temp)
        os.replace(temp, destination)
    finally:
        try:
            temp.unlink()
        except FileNotFoundError:
            pass


def _install_prepared_deployment(
    request: SupervisorDeployRequest,
    prepared: _PreparedDeployment,
    *,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
) -> None:
    expectations = _artifact_expectations(request)
    for name in DEPLOY_ARTIFACT_NAMES:
        expected_sha256, expected_size = expectations[name]
        source = prepared.staged_dir / name
        _verify_deploy_artifact(
            source,
            expected_sha256=expected_sha256,
            expected_size=expected_size,
        )
        _atomic_replace_runtime_file(source, installed_dir / name)
        _verify_deploy_artifact(
            installed_dir / name,
            expected_sha256=expected_sha256,
            expected_size=expected_size,
        )


def _restore_deployment_backup(
    request: SupervisorDeployRequest,
    prepared: _PreparedDeployment,
    *,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
) -> None:
    _verify_existing_backup(request, prepared.backup_id, prepared.backup_dir)
    manifest = json.loads((prepared.backup_dir / "manifest.json").read_text(encoding="utf-8"))
    by_name = {item["name"]: item for item in manifest["artifacts"]}
    for name in DEPLOY_ARTIFACT_NAMES:
        item = by_name[name]
        source = prepared.backup_dir / name
        _atomic_replace_runtime_file(source, installed_dir / name)
        _verify_deploy_artifact(
            installed_dir / name,
            expected_sha256=item["sha256"],
            expected_size=item["size_bytes"],
        )


def _spawn_supervised_server(
    containment: object,
    flags: int,
    control_dir: Path,
    token: str,
) -> subprocess.Popen[object]:
    process = containment.popen(
        [str(CLI), "server", "run", "--env-file", str(SERVER_ENV)],
        cwd=ROOT,
        creationflags=flags,
        shell=False,
        env=_server_environment(control_dir, token),
    )
    wait_for_server(process)
    return process


def _spawn_supervised_runner(
    containment: object,
    flags: int,
    config: Path,
) -> subprocess.Popen[object]:
    return containment.popen(
        [str(RUNNER_BIN), "--config", str(config)],
        cwd=ROOT,
        creationflags=flags,
        shell=False,
        env=isolated_child_environment("runner"),
    )


def _stop_tunnel(process: subprocess.Popen[object] | None) -> None:
    if process is None or process.poll() is not None:
        return
    if process.stdin is not None:
        try:
            process.stdin.close()
            process.wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            pass
    terminate_group(process)


def _start_supervised_children(
    containment: object,
    flags: int,
    config: Path,
    control_dir: Path,
    token: str,
    tunnel_spec: dict[str, object] | None = None,
) -> tuple[subprocess.Popen[object], subprocess.Popen[object], subprocess.Popen[object] | None]:
    server: subprocess.Popen[object] | None = None
    runner: subprocess.Popen[object] | None = None
    tunnel: subprocess.Popen[object] | None = None
    try:
        server = _spawn_supervised_server(containment, flags, control_dir, token)
        runner = _spawn_supervised_runner(containment, flags, config)
        if tunnel_spec is not None:
            tunnel = containment.popen(
                tunnel_spec["argv"],
                cwd=ROOT,
                creationflags=flags,
                env=tunnel_spec["env"],
                stdin=subprocess.PIPE,
                shell=False,
            )
        return server, runner, tunnel
    except Exception:
        _stop_tunnel(tunnel)
        if runner is not None:
            terminate_group(runner)
        if server is not None:
            terminate_group(server)
        raise


def _runner_cutover_readiness(
    output: object,
    expected_client_id: str,
) -> tuple[bool, str]:
    if not isinstance(output, dict) or output.get("service") != "webpi":
        return False, "runtime_status_invalid"
    focus = output.get("focus")
    focus = focus if isinstance(focus, dict) else {}
    if focus.get("client_id") != expected_client_id:
        return False, "runner_client_mismatch"
    if focus.get("connected") is not True or focus.get("status") != "online":
        return False, "runner_offline"
    if focus.get("compatibility_status") != "compatible":
        return False, "runner_incompatible"

    alignment = focus.get("source_alignment")
    alignment = alignment if isinstance(alignment, dict) else {}
    if alignment.get("status") in {"aligned", "current"}:
        return True, "aligned"

    reason_code = alignment.get("reason_code")
    if reason_code != "dirty_build_prevents_exact_source_alignment":
        return False, f"source_mismatch:{reason_code or 'unknown'}"

    runner_build = focus.get("build")
    runner_build = runner_build if isinstance(runner_build, dict) else {}
    server = output.get("server")
    server = server if isinstance(server, dict) else {}
    server_build = server.get("build")
    server_build = server_build if isinstance(server_build, dict) else {}

    runner_version = runner_build.get("version")
    server_version = server.get("version")
    runner_commit = runner_build.get("git_commit")
    server_commit = server_build.get("git_commit")
    runner_dirty = runner_build.get("git_dirty")
    server_dirty = server_build.get("git_dirty")
    if (
        isinstance(runner_version, str)
        and runner_version
        and runner_version == server_version
        and isinstance(runner_commit, str)
        and runner_commit
        and runner_commit == server_commit
        and runner_dirty is True
        and server_dirty is True
    ):
        return True, "dirty_same_build"
    return False, "dirty_build_identity_mismatch"


def _wait_for_post_cutover_health(
    server: subprocess.Popen[object],
    runner: subprocess.Popen[object],
    tunnel: subprocess.Popen[object] | None = None,
    *,
    timeout_seconds: float = 20.0,
) -> None:
    """Require Server + Runner readiness before marking cutover successful.

    The deadline is absolute: partial progress never extends it. `run-web`
    additionally requires the managed tunnel process to stay alive through the gate.
    """
    deadline = time.monotonic() + timeout_seconds
    last_reason = "runtime_status_unavailable"
    while True:
        if server.poll() is not None:
            raise RuntimeError("WebPi Server exited during post-cutover health gate")
        if runner.poll() is not None:
            raise RuntimeError("WebPi Runner exited during post-cutover health gate")
        if tunnel is not None and tunnel.poll() is not None:
            raise RuntimeError("WebPi tunnel exited during post-cutover health gate")
        try:
            wrapper = local_action_post(
                "runtime_status",
                {"client_id": CLIENT_ID, "compact": True},
                timeout=1.5,
            )
            output = wrapper.get("output") if wrapper.get("success") is True else None
            ready, reason = _runner_cutover_readiness(output, CLIENT_ID)
            if ready:
                return
            last_reason = reason
        except RuntimeError:
            last_reason = "runtime_status_unavailable"
        if time.monotonic() >= deadline:
            raise RuntimeError(f"post-cutover health gate timed out: {last_reason}")
        time.sleep(0.25)


def _allocate_shadow_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def _validate_shadow_runtime_status(origin: str, token: str, timeout: float = 1.5) -> None:
    status, payload, error = security_request_json(
        origin,
        "/api/actions/runtime_status",
        {"compact": True},
        f"Bearer {token}",
        timeout,
    )
    if status != 200 or error is not None or not isinstance(payload, dict):
        raise RuntimeError("shadow WebPi runtime_status did not authenticate")
    output = payload.get("output") if payload.get("success") is True else None
    if not isinstance(output, dict) or output.get("service") != "webpi" or output.get("auth_enabled") is not True:
        raise RuntimeError("shadow WebPi runtime_status payload is incompatible")


def _check_candidate_runner_offline(candidate_runner: Path, config: Path) -> dict[str, object]:
    try:
        completed = subprocess.run(
            [str(candidate_runner), "--config", str(config), "--check-config"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=True,
            timeout=10,
            env=isolated_child_environment("runner"),
            shell=False,
        )
        summary = json.loads(completed.stdout)
    except (OSError, subprocess.SubprocessError, ValueError) as exc:
        raise RuntimeError("candidate WebPi Runner offline config check failed") from exc
    if (
        not isinstance(summary, dict)
        or summary.get("status") != "valid"
        or summary.get("client_id") != CLIENT_ID
        or not isinstance(summary.get("max_concurrent_jobs"), int)
        or isinstance(summary.get("max_concurrent_jobs"), bool)
    ):
        raise RuntimeError("candidate WebPi Runner offline config check returned an invalid summary")
    allowed = {
        "status",
        "client_id",
        "transport",
        "project_registry_configured",
        "max_concurrent_jobs",
        "mcp_provider_count",
        "plugin_provider_count",
    }
    if set(summary) != allowed:
        raise RuntimeError("candidate WebPi Runner offline config check returned unsupported fields")
    return summary


def _shadow_candidate_preflight(
    containment: object,
    flags: int,
    staged_dir: Path,
    config: Path,
    *,
    attempts: int = 3,
    timeout_seconds: float = 15.0,
) -> dict[str, object]:
    if attempts < 1 or attempts > 5:
        raise ValueError("shadow candidate attempts must be between 1 and 5")
    candidate_cli = staged_dir / "webpi.exe"
    candidate_runner = staged_dir / "webpi-runner.exe"
    for path, label in ((candidate_cli, "candidate CLI"), (candidate_runner, "candidate Runner")):
        if _path_is_linklike(path) or not path.is_file():
            raise RuntimeError(f"{label} is not a regular staged artifact")

    runner_summary = _check_candidate_runner_offline(candidate_runner, config)
    last_error: Exception | None = None
    for attempt in range(1, attempts + 1):
        shadow_root = staged_dir / f".shadow-preflight-{attempt}"
        shadow_env = shadow_root / "server.env"
        shadow_data = shadow_root / "data"
        process: subprocess.Popen[object] | None = None
        try:
            shutil.rmtree(shadow_root, ignore_errors=True)
            shadow_root.mkdir(parents=True, exist_ok=False)
            port = _allocate_shadow_loopback_port()
            origin = f"http://127.0.0.1:{port}"
            subprocess.run(
                [
                    str(candidate_cli),
                    "server",
                    "init",
                    "--listen",
                    f"127.0.0.1:{port}",
                    "--data-dir",
                    str(shadow_data),
                    "--env-file",
                    str(shadow_env),
                    "--json",
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                check=True,
                timeout=15,
                env=isolated_child_environment("admin"),
                shell=False,
            )
            token = _bootstrap_key_from_env(shadow_env)
            process = containment.popen(
                [str(candidate_cli), "server", "run", "--env-file", str(shadow_env)],
                cwd=ROOT,
                creationflags=flags,
                shell=False,
                env=isolated_child_environment("server"),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + timeout_seconds
            last_readiness_error: Exception | None = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError("shadow WebPi Server exited before readiness")
                status, schema, error = security_request_json(
                    origin,
                    "/openapi.json",
                    None,
                    None,
                    0.75,
                )
                if (
                    status == 200
                    and error is None
                    and isinstance(schema, dict)
                    and isinstance(schema.get("info"), dict)
                    and schema["info"].get("title") == "WebPi GPT Actions"
                ):
                    try:
                        require_authenticated_origin(origin, timeout=1.0)
                        _validate_shadow_runtime_status(origin, token, timeout=1.5)
                    except Exception as exc:
                        last_readiness_error = exc
                        time.sleep(0.15)
                        continue
                    if process.poll() is not None:
                        raise RuntimeError("shadow WebPi Server exited during readiness verification")
                    return {
                        "runner_config_valid": True,
                        "runner_transport": runner_summary.get("transport"),
                        "server_shadow_healthy": True,
                        "server_auth_verified": True,
                        "loopback_only": True,
                        "attempts": attempt,
                    }
                time.sleep(0.15)
            if last_readiness_error is not None:
                raise RuntimeError("shadow WebPi Server did not reach authenticated readiness before deadline") from last_readiness_error
            raise RuntimeError("shadow WebPi Server did not become ready before deadline")
        except Exception as exc:
            last_error = exc
        finally:
            if process is not None:
                terminate_group(process)
            shutil.rmtree(shadow_root, ignore_errors=True)
    raise RuntimeError("candidate WebPi shadow preflight failed") from last_error


def _execute_deploy_request(
    request: SupervisorDeployRequest,
    *,
    containment: object,
    flags: int,
    config: Path,
    control_dir: Path,
    token: str,
    server: subprocess.Popen[object],
    runner: subprocess.Popen[object],
    tunnel: subprocess.Popen[object] | None = None,
    tunnel_spec: dict[str, object] | None = None,
    candidate_root: Path = SERVICE_CANDIDATE_ROOT,
    staging_root: Path = DEPLOYMENT_STAGING_ROOT,
    backup_root: Path = DEPLOYMENT_BACKUP_ROOT,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
) -> tuple[subprocess.Popen[object], subprocess.Popen[object], subprocess.Popen[object] | None]:
    try:
        prepared = _prepare_deployment_snapshots(
            request,
            candidate_root=candidate_root,
            staging_root=staging_root,
            backup_root=backup_root,
            installed_dir=installed_dir,
        )
    except Exception:
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="failed",
            error_code="candidate_validation_failed",
        )
        return server, runner, tunnel

    try:
        _shadow_candidate_preflight(
            containment,
            flags,
            prepared.staged_dir,
            config,
        )
    except Exception:
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="failed",
            error_code="candidate_shadow_preflight_failed",
            backup_id=prepared.backup_id,
        )
        shutil.rmtree(prepared.staged_dir, ignore_errors=True)
        return server, runner, tunnel

    _wait_until_supervisor_request_due(request)
    try:
        prepared_manifest = json.loads(
            (prepared.backup_dir / "manifest.json").read_text(encoding="utf-8")
        )
        _validate_rollback_manifest_shape(
            prepared_manifest,
            backup_id=prepared.backup_id,
        )
        _verify_current_rollback_consistency(prepared_manifest)
    except Exception:
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="failed",
            error_code="consistency_fence_changed",
            backup_id=prepared.backup_id,
        )
        shutil.rmtree(prepared.staged_dir, ignore_errors=True)
        return server, runner, tunnel
    _stop_tunnel(tunnel)
    terminate_group(runner)
    terminate_group(server)
    try:
        _install_prepared_deployment(request, prepared, installed_dir=installed_dir)
        new_server, new_runner, new_tunnel = _start_supervised_children(
            containment,
            flags,
            config,
            control_dir,
            token,
            tunnel_spec,
        )
        _wait_for_post_cutover_health(new_server, new_runner, new_tunnel)
    except Exception:
        try:
            _restore_deployment_backup(request, prepared, installed_dir=installed_dir)
            new_server, new_runner, new_tunnel = _start_supervised_children(
                containment,
                flags,
                config,
                control_dir,
                token,
                tunnel_spec,
            )
            _wait_for_post_cutover_health(new_server, new_runner, new_tunnel)
        except Exception:
            _write_supervisor_result(
                control_dir,
                token,
                request,
                status="outcome_unknown",
                error_code="deploy_rollback_failed",
                backup_id=prepared.backup_id,
            )
            raise
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="rolled_back",
            error_code="candidate_start_failed",
            backup_id=prepared.backup_id,
        )
        shutil.rmtree(prepared.staged_dir, ignore_errors=True)
        return new_server, new_runner, new_tunnel

    _write_supervisor_result(
        control_dir,
        token,
        request,
        status="succeeded",
        backup_id=prepared.backup_id,
    )
    shutil.rmtree(prepared.staged_dir, ignore_errors=True)
    return new_server, new_runner, new_tunnel


def _execute_rollback_request(
    request: SupervisorRollbackRequest,
    *,
    containment: object,
    flags: int,
    config: Path,
    control_dir: Path,
    token: str,
    server: subprocess.Popen[object],
    runner: subprocess.Popen[object],
    tunnel: subprocess.Popen[object] | None = None,
    tunnel_spec: dict[str, object] | None = None,
    backup_root: Path = DEPLOYMENT_BACKUP_ROOT,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
) -> tuple[subprocess.Popen[object], subprocess.Popen[object], subprocess.Popen[object] | None]:
    try:
        target_dir, target_manifest = _load_verified_backup_by_id(
            request.backup_id,
            backup_root=backup_root,
        )
    except Exception:
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="failed",
            error_code="rollback_backup_validation_failed",
        )
        return server, runner, tunnel

    try:
        safety_backup_id, safety_backup_dir = _create_receipt_runtime_backup(
            request,
            backup_root=backup_root,
            installed_dir=installed_dir,
        )
        safety_manifest = json.loads(
            (safety_backup_dir / "manifest.json").read_text(encoding="utf-8")
        )
    except Exception:
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="failed",
            error_code="rollback_safety_backup_failed",
        )
        return server, runner, tunnel

    _wait_until_supervisor_request_due(request)
    try:
        _verify_current_rollback_consistency(target_manifest)
    except Exception:
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="failed",
            error_code="rollback_consistency_fence_changed",
            backup_id=safety_backup_id,
        )
        return server, runner, tunnel
    _stop_tunnel(tunnel)
    terminate_group(runner)
    terminate_group(server)
    try:
        _restore_verified_backup_dir(
            target_dir,
            target_manifest,
            installed_dir=installed_dir,
        )
        new_server, new_runner, new_tunnel = _start_supervised_children(
            containment,
            flags,
            config,
            control_dir,
            token,
            tunnel_spec,
        )
        _wait_for_post_cutover_health(new_server, new_runner, new_tunnel)
    except Exception:
        try:
            _restore_verified_backup_dir(
                safety_backup_dir,
                safety_manifest,
                installed_dir=installed_dir,
            )
            new_server, new_runner, new_tunnel = _start_supervised_children(
                containment,
                flags,
                config,
                control_dir,
                token,
                tunnel_spec,
            )
            _wait_for_post_cutover_health(new_server, new_runner, new_tunnel)
        except Exception:
            _write_supervisor_result(
                control_dir,
                token,
                request,
                status="outcome_unknown",
                error_code="rollback_recovery_failed",
                backup_id=safety_backup_id,
            )
            raise
        _write_supervisor_result(
            control_dir,
            token,
            request,
            status="failed",
            error_code="rollback_target_failed_recovered",
            backup_id=safety_backup_id,
        )
        return new_server, new_runner, new_tunnel

    _write_supervisor_result(
        control_dir,
        token,
        request,
        status="succeeded",
        backup_id=safety_backup_id,
    )
    return new_server, new_runner, new_tunnel


def _restart_core_children(
    containment: object,
    flags: int,
    config: Path,
    control_dir: Path,
    token: str,
    server: subprocess.Popen[object],
    runner: subprocess.Popen[object],
    tunnel: subprocess.Popen[object] | None = None,
    tunnel_spec: dict[str, object] | None = None,
) -> tuple[subprocess.Popen[object], subprocess.Popen[object], subprocess.Popen[object] | None]:
    _stop_tunnel(tunnel)
    terminate_group(runner)
    terminate_group(server)
    return _start_supervised_children(
        containment,
        flags,
        config,
        control_dir,
        token,
        tunnel_spec,
    )


def run_both() -> int:
    ensure_server_state()
    config = runner_config()
    require_file(CLI, "WebPi CLI")
    require_file(RUNNER_BIN, "WebPi Runner")
    if server_is_online():
        raise RuntimeError(
            f"{SERVER_URL} is already serving OpenAPI. Stop the existing WebPi Server "
            "or run only the Runner with webpi.cmd runner."
        )

    flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    containment = _create_process_containment()
    control_dir, supervisor_token = _prepare_supervisor()
    server: subprocess.Popen[object] | None = None
    runner: subprocess.Popen[object] | None = None
    try:
        server = _spawn_supervised_server(containment, flags, control_dir, supervisor_token)
        runner = _spawn_supervised_runner(containment, flags, config)
        json_print(
            {
                "status": "running",
                "server_url": SERVER_URL,
                "server_pid": server.pid,
                "runner_pid": runner.pid,
                "client_id": CLIENT_ID,
                "state_dir": str(STATE),
                "supervisor": True,
            }
        )
        while True:
            request = _consume_supervisor_request(control_dir, supervisor_token)
            if isinstance(request, SupervisorDeployRequest):
                server, runner, _ = _execute_deploy_request(
                    request,
                    containment=containment,
                    flags=flags,
                    config=config,
                    control_dir=control_dir,
                    token=supervisor_token,
                    server=server,
                    runner=runner,
                )
                json_print(
                    {
                        "status": "deployment-finished",
                        "server_pid": server.pid,
                        "runner_pid": runner.pid,
                        "receipt_id": request.receipt_id,
                    }
                )
                continue
            if isinstance(request, SupervisorRollbackRequest):
                server, runner, _ = _execute_rollback_request(
                    request,
                    containment=containment,
                    flags=flags,
                    config=config,
                    control_dir=control_dir,
                    token=supervisor_token,
                    server=server,
                    runner=runner,
                )
                json_print(
                    {
                        "status": "rollback-finished",
                        "server_pid": server.pid,
                        "runner_pid": runner.pid,
                        "receipt_id": request.receipt_id,
                    }
                )
                continue
            if request is not None:
                _wait_until_supervisor_request_due(request)
                try:
                    server, runner, _ = _restart_core_children(
                        containment,
                        flags,
                        config,
                        control_dir,
                        supervisor_token,
                        server,
                        runner,
                    )
                except Exception:
                    try:
                        server, runner, _ = _start_supervised_children(
                            containment,
                            flags,
                            config,
                            control_dir,
                            supervisor_token,
                        )
                    except Exception:
                        _write_supervisor_result(
                            control_dir,
                            supervisor_token,
                            request,
                            status="failed",
                            error_code="restart_failed_unrecovered",
                        )
                        raise
                    _write_supervisor_result(
                        control_dir,
                        supervisor_token,
                        request,
                        status="failed",
                        error_code="restart_failed_recovered",
                    )
                    json_print(
                        {
                            "status": "restart-failed-recovered",
                            "server_pid": server.pid,
                            "runner_pid": runner.pid,
                            "receipt_id": request.receipt_id,
                        }
                    )
                    continue
                _write_supervisor_result(
                    control_dir,
                    supervisor_token,
                    request,
                    status="succeeded",
                )
                json_print(
                    {
                        "status": "restarted",
                        "server_pid": server.pid,
                        "runner_pid": runner.pid,
                        "receipt_id": request.receipt_id,
                    }
                )
                continue
            server_code = server.poll()
            runner_code = runner.poll()
            if server_code is not None or runner_code is not None:
                return server_code if server_code is not None else int(runner_code or 0)
            time.sleep(0.25)
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
    control_dir, supervisor_token = _prepare_supervisor()
    server: subprocess.Popen[object] | None = None
    runner: subprocess.Popen[object] | None = None
    tunnel: subprocess.Popen[object] | None = None
    try:
        server = _spawn_supervised_server(containment, flags, control_dir, supervisor_token)
        runner = _spawn_supervised_runner(containment, flags, config)
        tunnel = containment.popen(
            tunnel_spec["argv"],
            cwd=ROOT,
            creationflags=flags,
            env=tunnel_spec["env"],
            stdin=subprocess.PIPE,
            shell=False,
        )
        json_print(
            {
                "status": "running-web",
                "server_url": SERVER_URL,
                "server_pid": server.pid,
                "runner_pid": runner.pid,
                "tunnel_pid": tunnel.pid,
                "client_id": CLIENT_ID,
                "state_dir": str(STATE),
                "supervisor": True,
            }
        )
        while True:
            request = _consume_supervisor_request(control_dir, supervisor_token)
            if isinstance(request, SupervisorDeployRequest):
                server, runner, tunnel = _execute_deploy_request(
                    request,
                    containment=containment,
                    flags=flags,
                    config=config,
                    control_dir=control_dir,
                    token=supervisor_token,
                    server=server,
                    runner=runner,
                    tunnel=tunnel,
                    tunnel_spec=tunnel_spec,
                )
                json_print(
                    {
                        "status": "deployment-finished-web",
                        "server_pid": server.pid,
                        "runner_pid": runner.pid,
                        "tunnel_pid": tunnel.pid if tunnel is not None else None,
                        "receipt_id": request.receipt_id,
                    }
                )
                continue
            if isinstance(request, SupervisorRollbackRequest):
                server, runner, tunnel = _execute_rollback_request(
                    request,
                    containment=containment,
                    flags=flags,
                    config=config,
                    control_dir=control_dir,
                    token=supervisor_token,
                    server=server,
                    runner=runner,
                    tunnel=tunnel,
                    tunnel_spec=tunnel_spec,
                )
                json_print(
                    {
                        "status": "rollback-finished-web",
                        "server_pid": server.pid,
                        "runner_pid": runner.pid,
                        "tunnel_pid": tunnel.pid if tunnel is not None else None,
                        "receipt_id": request.receipt_id,
                    }
                )
                continue
            if request is not None:
                _wait_until_supervisor_request_due(request)
                try:
                    server, runner, tunnel = _restart_core_children(
                        containment,
                        flags,
                        config,
                        control_dir,
                        supervisor_token,
                        server,
                        runner,
                        tunnel,
                        tunnel_spec,
                    )
                except Exception:
                    try:
                        server, runner, tunnel = _start_supervised_children(
                            containment,
                            flags,
                            config,
                            control_dir,
                            supervisor_token,
                            tunnel_spec,
                        )
                    except Exception:
                        _write_supervisor_result(
                            control_dir,
                            supervisor_token,
                            request,
                            status="failed",
                            error_code="restart_failed_unrecovered",
                        )
                        raise
                    _write_supervisor_result(
                        control_dir,
                        supervisor_token,
                        request,
                        status="failed",
                        error_code="restart_failed_recovered",
                    )
                    json_print(
                        {
                            "status": "restart-failed-recovered-web",
                            "server_pid": server.pid,
                            "runner_pid": runner.pid,
                            "tunnel_pid": tunnel.pid if tunnel is not None else None,
                            "receipt_id": request.receipt_id,
                        }
                    )
                    continue
                _write_supervisor_result(
                    control_dir,
                    supervisor_token,
                    request,
                    status="succeeded",
                )
                json_print(
                    {
                        "status": "restarted-web",
                        "server_pid": server.pid,
                        "runner_pid": runner.pid,
                        "tunnel_pid": tunnel.pid if tunnel is not None else None,
                        "receipt_id": request.receipt_id,
                    }
                )
                continue
            for process in (server, runner, tunnel):
                if process is not None:
                    code = process.poll()
                    if code is not None:
                        return int(code)
            time.sleep(0.25)
    except KeyboardInterrupt:
        return 0
    finally:
        _stop_tunnel(tunnel)
        if runner is not None:
            terminate_group(runner)
        if server is not None:
            terminate_group(server)
        containment.close()


def _valid_bootstrap_candidate_id(value: str) -> bool:
    if not value or len(value.encode("utf-8")) > 64:
        return False
    return (
        value[0].isascii()
        and value[0].isalnum()
        and all(ch.isascii() and (ch.isalnum() or ch in "._-") for ch in value)
    )


def _bootstrap_candidate_snapshot(
    candidate_id: str,
    *,
    candidate_root: Path = SERVICE_CANDIDATE_ROOT,
) -> tuple[Path, dict[str, dict[str, object]], dict[str, dict[str, object]]]:
    if not _valid_bootstrap_candidate_id(candidate_id):
        raise RuntimeError("bootstrap candidate id is invalid")
    candidate_root = candidate_root.resolve(strict=True)
    component = candidate_root / candidate_id
    unresolved = component / "dogfood"
    if _path_is_linklike(component) or _path_is_linklike(unresolved):
        raise RuntimeError("bootstrap candidate must not traverse a link or junction")
    candidate_dir = _assert_bounded_child(candidate_root, unresolved)
    artifacts: dict[str, dict[str, object]] = {}
    for name in DEPLOY_ARTIFACT_NAMES:
        artifacts[name] = _file_identity(candidate_dir / name, f"bootstrap candidate {name}")
    builds = _capture_runtime_builds(candidate_dir)
    return candidate_dir, artifacts, builds


def _parse_windows_standalone_processes(output: str, script_path: Path) -> list[tuple[int, str]]:
    expected = str(script_path.resolve()).lower().replace("/", "\\")
    current: dict[str, str] = {}
    found: list[tuple[int, str]] = []

    def flush() -> None:
        command = current.get("CommandLine", "")
        pid = current.get("ProcessId", "")
        normalized = command.lower().replace("/", "\\")
        if expected in normalized and pid.isdigit():
            match = re.search(
                r"standalone\.py\"?\s+(run-web|run)(?:\s|$)",
                command,
                re.IGNORECASE,
            )
            if match:
                found.append((int(pid), match.group(1).lower()))
        current.clear()

    for raw in output.splitlines():
        line = raw.strip()
        if line.startswith("CommandLine="):
            if current:
                flush()
            current["CommandLine"] = line.split("=", 1)[1].strip()
            continue
        if line.startswith("ProcessId="):
            current["ProcessId"] = line.split("=", 1)[1].strip()
            if "CommandLine" in current:
                flush()
    if current:
        flush()
    return found


def _discover_windows_standalone_parent() -> tuple[int, str]:
    if os.name != "nt":
        raise RuntimeError("bootstrap-upgrade is Windows-only; use the system service manager on Unix")
    try:
        completed = subprocess.run(
            [
                "wmic.exe",
                "process",
                "where",
                "Name='python.exe'",
                "get",
                "ProcessId,CommandLine",
                "/FORMAT:LIST",
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=15,
            check=True,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise RuntimeError("could not enumerate the current WebPi standalone process") from exc
    found = _parse_windows_standalone_processes(completed.stdout, Path(__file__).resolve())
    if len(found) != 1:
        raise RuntimeError(f"expected exactly one WebPi standalone parent, found {len(found)}")
    return found[0]


def _bootstrap_hashes(directory: Path) -> dict[str, str]:
    return {name: _sha256_file(directory / name) for name in DEPLOY_ARTIFACT_NAMES}


def _bootstrap_create_backup(
    mode: str,
    candidate_id: str,
    consistency: Mapping[str, object],
    *,
    installed_dir: Path = INSTALLED_RUNTIME_DIR,
    backup_root: Path = DEPLOYMENT_BACKUP_ROOT,
) -> Path:
    backup_root.mkdir(parents=True, exist_ok=True)
    backup_id = f"bootstrap-{time.strftime('%Y%m%d-%H%M%S')}-{os.getpid()}"
    backup_dir = _create_bounded_direct_child_dir(backup_root, backup_id)
    artifacts: list[dict[str, object]] = []
    for name in DEPLOY_ARTIFACT_NAMES:
        source = installed_dir / name
        destination = backup_dir / name
        shutil.copy2(source, destination)
        artifacts.append({"name": name, **_file_identity(destination, f"bootstrap backup {name}")})

    private = backup_dir / "private"
    private.mkdir()
    try:
        private.chmod(0o700)
    except OSError:
        pass
    for source, name in ((SERVER_ENV, "server.env"), (runner_config(), "runner-config")):
        destination = private / name
        shutil.copy2(source, destination)
        try:
            _copy_private_permissions(source, destination)
        except OSError:
            pass
    database = SERVER_DATA / "webcodex.db"
    database_snapshot = False
    if database.is_file() and not _path_is_linklike(database):
        source_db = sqlite3.connect(f"file:{database.resolve()}?mode=ro", uri=True, timeout=3.0)
        target_path = private / "webcodex.db"
        target_db = sqlite3.connect(target_path)
        try:
            source_db.backup(target_db)
            target_db.commit()
            database_snapshot = True
        finally:
            target_db.close()
            source_db.close()
        try:
            _copy_private_permissions(database, target_path)
        except OSError:
            pass
    manifest = {
        "version": 1,
        "kind": "bootstrap_binary_transition",
        "candidate_id": candidate_id,
        "mode": mode,
        "created_at": int(time.time()),
        "artifacts": artifacts,
        "consistency": dict(consistency),
        "private_recovery_payload": {
            "server_env": True,
            "runner_config": True,
            "database_snapshot": database_snapshot,
            "automatic_rollback_restores": ["runtime_binaries"],
        },
    }
    (backup_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True), encoding="utf-8"
    )
    return backup_dir


def _bootstrap_kill_tree(pid: int) -> None:
    subprocess.run(
        [r"C:\Windows\System32\taskkill.exe", "/PID", str(pid), "/T", "/F"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=20,
        check=False,
    )


def _bootstrap_wait_unlocked(installed_dir: Path = INSTALLED_RUNTIME_DIR, timeout: float = 20.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            for name in DEPLOY_ARTIFACT_NAMES:
                with (installed_dir / name).open("ab"):
                    pass
            return
        except PermissionError:
            time.sleep(0.2)
    raise RuntimeError("installed WebPi binaries remained locked after stopping the standalone tree")


def _bootstrap_install_runtime(source_dir: Path, installed_dir: Path = INSTALLED_RUNTIME_DIR) -> None:
    source_hashes = _bootstrap_hashes(source_dir)
    for name in DEPLOY_ARTIFACT_NAMES:
        _atomic_replace_runtime_file(source_dir / name, installed_dir / name)
    if _bootstrap_hashes(installed_dir) != source_hashes:
        raise RuntimeError("bootstrap runtime hash verification failed after install")


def _bootstrap_start_stack(mode: str) -> subprocess.Popen[object]:
    if mode not in {"run", "run-web"}:
        raise RuntimeError("unsupported bootstrap standalone mode")
    flags = subprocess.CREATE_NEW_CONSOLE | subprocess.CREATE_NEW_PROCESS_GROUP
    return subprocess.Popen(
        [sys.executable, "-I", "-B", str(Path(__file__).resolve()), mode],
        cwd=ROOT,
        creationflags=flags,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
    )


def _bootstrap_wait_healthy(parent: subprocess.Popen[object], timeout: float = 35.0) -> None:
    deadline = time.monotonic() + timeout
    last_reason = "runtime_status_unavailable"
    while time.monotonic() < deadline:
        if parent.poll() is not None:
            raise RuntimeError(f"standalone exited before bootstrap health gate: rc={parent.returncode}")
        try:
            wrapper = local_action_post(
                "runtime_status",
                {"client_id": CLIENT_ID, "compact": True},
                timeout=1.5,
            )
            output = wrapper.get("output") if wrapper.get("success") is True else None
            ready, reason = _runner_cutover_readiness(output, CLIENT_ID)
            if ready:
                return
            last_reason = reason
        except Exception:
            last_reason = "runtime_status_unavailable"
        time.sleep(0.25)
    raise RuntimeError(f"bootstrap health gate timed out: {last_reason}")


def bootstrap_upgrade(candidate_id: str, *, preflight_only: bool = False) -> int:
    if os.name != "nt":
        raise RuntimeError("bootstrap-upgrade is Windows-only; use the system service manager on Unix")
    candidate_dir, candidate_artifacts, candidate_builds = _bootstrap_candidate_snapshot(candidate_id)
    candidate_hashes = {name: str(candidate_artifacts[name]["sha256"]) for name in DEPLOY_ARTIFACT_NAMES}
    pid, mode = _discover_windows_standalone_parent()
    if _bootstrap_hashes(INSTALLED_RUNTIME_DIR) == candidate_hashes:
        json_print({"status": "already-deployed", "candidate_id": candidate_id, "mode": mode})
        return 0

    flags = subprocess.CREATE_NEW_PROCESS_GROUP
    containment = _create_process_containment()
    try:
        shadow = _shadow_candidate_preflight(
            containment,
            flags,
            candidate_dir,
            runner_config(),
            attempts=3,
            timeout_seconds=15.0,
        )
    finally:
        containment.close()
    if not all(
        shadow.get(key) is True
        for key in ("runner_config_valid", "server_shadow_healthy", "server_auth_verified", "loopback_only")
    ):
        raise RuntimeError("bootstrap candidate shadow preflight returned incomplete success evidence")
    consistency = _capture_rollback_consistency_identity()
    if _bootstrap_candidate_snapshot(candidate_id)[1:] != (candidate_artifacts, candidate_builds):
        raise RuntimeError("bootstrap candidate changed during preflight")
    if preflight_only:
        json_print(
            {
                "status": "bootstrap-preflight-passed",
                "candidate_id": candidate_id,
                "mode": mode,
                "shadow_attempts": shadow.get("attempts"),
                "candidate_sha256": candidate_hashes,
            }
        )
        return 0

    backup_dir = _bootstrap_create_backup(mode, candidate_id, consistency)
    if _capture_rollback_consistency_identity() != consistency:
        raise RuntimeError("configuration/database/plugin consistency changed before bootstrap cutover")
    if _bootstrap_candidate_snapshot(candidate_id)[1:] != (candidate_artifacts, candidate_builds):
        raise RuntimeError("bootstrap candidate changed before cutover")

    new_parent: subprocess.Popen[object] | None = None
    try:
        _bootstrap_kill_tree(pid)
        _bootstrap_wait_unlocked()
        _bootstrap_install_runtime(candidate_dir)
        new_parent = _bootstrap_start_stack(mode)
        _bootstrap_wait_healthy(new_parent)
        json_print(
            {
                "status": "bootstrap-upgrade-succeeded",
                "candidate_id": candidate_id,
                "mode": mode,
                "standalone_pid": new_parent.pid,
                "backup_id": backup_dir.name,
            }
        )
        return 0
    except BaseException:
        if new_parent is not None and new_parent.poll() is None:
            _bootstrap_kill_tree(new_parent.pid)
            try:
                new_parent.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
        try:
            _bootstrap_wait_unlocked()
            _bootstrap_install_runtime(backup_dir)
            rollback_parent = _bootstrap_start_stack(mode)
            _bootstrap_wait_healthy(rollback_parent)
            json_print(
                {
                    "status": "bootstrap-upgrade-failed-rolled-back",
                    "candidate_id": candidate_id,
                    "backup_id": backup_dir.name,
                    "standalone_pid": rollback_parent.pid,
                    "database_snapshot_retained_for_manual_recovery": True,
                }
            )
        except BaseException:
            json_print(
                {
                    "status": "bootstrap-upgrade-outcome-unknown",
                    "candidate_id": candidate_id,
                    "backup_id": backup_dir.name,
                }
            )
        raise


def help_text() -> str:
    return """WebPi local runtime

Common:
  webpi.cmd status                 Show safe local runtime status
  webpi.cmd doctor                 Diagnose runtime, Runner, Pi Bridge, and source alignment
  webpi.cmd run                    Run WebPi Server + Runner in one foreground lifecycle
  webpi.cmd init                   Initialize private standalone state
  webpi.cmd enroll                 Enroll the local Runner/project and provision the Action token
  webpi.cmd bootstrap-upgrade ID [--preflight-only]
                                  Windows-only first transition to supervised self-deploy

Security / public access:
  webpi.cmd verify [args...]       Run WebPi security smoke checks
  webpi.cmd cloudflare-config URL  Register the HTTPS public origin (no tunnel token is stored)
  webpi.cmd tunnel-config          Configure the WebPi-local secure MCP tunnel
  webpi.cmd run-web                Run Server + Runner + configured tunnel
  webpi.cmd migrate-config [--check]
                                  Migrate supported legacy WebPi env keys safely

Pi ecosystem:
  webpi.cmd pi <command>           Inspect/manage WebPi-local Pi trust, packages, and approvals
  webpi.cmd pi package-inspect npm:<pkg>[@version-or-tag]
                                  Preview npm metadata before any package mutation

Advanced foreground components:
  webpi.cmd server | runner | tunnel

Start with `webpi.cmd doctor` when something is wrong. Keep PATs, tunnel credentials,
and private keys out of chat, logs, and Git.
"""


def usage_text() -> str:
    return "usage: webpi.cmd [help|init|status|doctor|verify|migrate-config|cloudflare-config|public-url-config|tunnel-config|pi|enroll|bootstrap-upgrade|server|runner|tunnel|run|run-web]"


def main() -> int:
    action = sys.argv[1] if len(sys.argv) > 1 else "status"
    if action in {"help", "--help", "-h"}:
        print(help_text(), end="")
        return 0
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
            raise SystemExit(f"usage: webpi.cmd {action} https://webpi.example.com")
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
    if action == "bootstrap-upgrade":
        args = sys.argv[2:]
        if len(args) not in {1, 2} or (len(args) == 2 and args[1] != "--preflight-only"):
            raise SystemExit("usage: webpi.cmd bootstrap-upgrade CANDIDATE_ID [--preflight-only]")
        return bootstrap_upgrade(args[0], preflight_only=len(args) == 2)
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
    raise SystemExit(usage_text())


if __name__ == "__main__":
    raise SystemExit(main())
