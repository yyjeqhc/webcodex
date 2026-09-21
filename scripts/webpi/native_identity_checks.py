"""Real-process WebPi acceptance helpers. Operate only on caller-owned fixtures."""
from __future__ import annotations
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from security_smoke import request_json
from standalone import CLI, RUNNER_BIN, NODE, PI_BRIDGE, INSTALL_PROVIDER, terminate_group


def rejected_start(argv: list[str], cwd: Path, env: dict[str, str]) -> bool:
    flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               creationflags=flags, shell=False)
    try:
        return process.wait(timeout=8) != 0
    except subprocess.TimeoutExpired:
        return False
    finally:
        terminate_group(process)
        process.wait(timeout=5)


def startup_checks(fixture: Path, env: dict[str, str]) -> list[dict[str, object]]:
    checks = []
    for binary in (CLI, RUNNER_BIN, CLI.with_name("webpi-server.exe")):
        result = subprocess.run([str(binary), "--version"], env=env, cwd=fixture,
                                capture_output=True, text=True, timeout=10, shell=False)
        checks.append({"check": binary.stem + "_version_identity", "passed": result.returncode == 0 and result.stdout.startswith(binary.stem + " ")})
    negative = fixture / "negative.env"
    foreign = dict(env, WEBCODEX_TOKEN="synthetic-foreign-not-a-real-key")
    for label, contents in (
        ("foreign_bootstrap_does_not_enable_webpi", "WEBPI_ADDR=127.0.0.1:0\n"),
        ("legacy_env_file_rejected", "WEBCODEX_TOKEN=synthetic\nWEBPI_ADDR=127.0.0.1:0\n"),
        ("anonymous_mode_rejected", "WEBPI_TOKEN=synthetic\nWEBPI_ADDR=127.0.0.1:0\nWEBPI_ALLOW_ANONYMOUS=true\n"),
        ("shared_key_mode_rejected", "WEBPI_TOKEN=synthetic\nWEBPI_ADDR=127.0.0.1:0\nWEBPI_SHARED_KEY_ENABLED=true\n"),
    ):
        negative.write_text(contents, encoding="utf-8")
        checks.append({"check": label, "passed": rejected_start([str(CLI), "server", "run", "--env-file", str(negative)], fixture, foreign)})
    isolated = fixture / "cli-only"
    isolated.mkdir()
    copied = isolated / CLI.name
    shutil.copy2(CLI, copied)
    # Use a VALID configuration here: a fallback server would stay running and
    # fail this test. An invalid file could hide an accidental fallback.
    valid = isolated / "valid.env"
    valid.write_text("WEBPI_TOKEN=synthetic\nWEBPI_ADDR=127.0.0.1:0\nWEBPI_SHARED_KEY_ENABLED=false\nWEBPI_ALLOW_ANONYMOUS=false\n", encoding="utf-8")
    path_env = dict(env, PATH=str(CLI.parent) + os.pathsep + env.get("PATH", ""))
    checks.append({"check": "missing_sibling_never_falls_back_to_path", "passed": rejected_start([str(copied), "server", "run", "--env-file", str(valid)], isolated, path_env)})
    return checks


def api_identity_checks(origin: str, token: str) -> list[dict[str, object]]:
    checks = []
    status, schema, _ = request_json(origin, "/openapi.json", None)
    checks.append({"check": "openapi_product_identity", "passed": status == 200 and isinstance(schema, dict) and schema.get("info", {}).get("title") == "WebPi GPT Actions"})
    status, runtime, _ = request_json(origin, "/api/actions/runtime_status", {"compact": True}, "Bearer " + token)
    checks.append({"check": "runtime_product_identity", "passed": status == 200 and isinstance(runtime, dict) and runtime.get("output", {}).get("service") == "webpi"})
    status, initialized, _ = request_json(origin, "/mcp", {"jsonrpc": "2.0", "id": 10, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "webpi-fixture", "version": "1"}}}, "Bearer " + token)
    checks.append({"check": "mcp_product_identity", "passed": status == 200 and isinstance(initialized, dict) and initialized.get("result", {}).get("serverInfo", {}).get("name") == "webpi"})
    return checks


def runner_plugin_checks(origin: str, token: str, bootstrap: str, login: dict[str, object], project: Path, env: dict[str, str]) -> list[dict[str, object]]:
    """Own the new Runner for the whole test; never reload an existing provider."""
    config = str(login["runner_config"])
    subprocess.run([sys.executable, str(INSTALL_PROVIDER), "install", config, str(NODE), str(PI_BRIDGE), str(project)], cwd=project, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True, timeout=15, shell=False)
    (project / "README.md").write_text("# WebPi identity fixture\n", encoding="utf-8")
    flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    runner = subprocess.Popen([str(RUNNER_BIN), "--config", config], cwd=project, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=flags, shell=False)
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if runner.poll() is not None:
                raise RuntimeError("temporary WebPi Runner exited before readiness")
            status, runtime, _ = request_json(origin, "/api/actions/runtime_status", {"compact": False}, "Bearer " + token, timeout=1)
            output = runtime.get("output", {}) if isinstance(runtime, dict) else {}
            if output.get("agents", {}).get("online_count") == 1:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("temporary WebPi Runner readiness deadline expired")
        checks: list[dict[str, object]] = [{"check": "native_runner_registered", "passed": True}]
        status, described, _ = request_json(origin, "/api/actions/plugin_tool", {"action": "describe", "runner": "webpi-auth-fixture", "plugin": "pi-bridge", "tool": "pi_read"}, "Bearer " + bootstrap)
        binding = described.get("output", {}).get("binding") if isinstance(described, dict) else None
        if not isinstance(binding, str):
            raise RuntimeError("temporary Pi provider could not be described")
        status, called, _ = request_json(origin, "/api/actions/plugin_tool", {"action": "call", "binding": binding, "arguments": {"path": "README.md"}}, "Bearer " + bootstrap)
        plugin = called.get("output", {}).get("structuredContent", {}) if isinstance(called, dict) else {}
        checks.append({"check": "pi_protocol_and_guarded_read_end_to_end", "passed": status == 200 and plugin.get("engine") == "pi" and "WebPi identity fixture" in plugin.get("text", "")})
        return checks
    finally:
        terminate_group(runner)
        runner.wait(timeout=5)
