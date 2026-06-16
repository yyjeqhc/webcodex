#!/usr/bin/env python3
"""End-to-end smoke test for the real private-drop-agent binary.

Starts a temporary private-drop server and a real private-drop-agent process,
then submits runShell and verifies the agent executes the command and returns
stdout/stderr/exit_code.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any


def post(port: int, token: str, path: str, body: dict[str, Any]) -> dict[str, Any]:
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=data,
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode())


def wait_for_health(port: int, proc: subprocess.Popen[bytes], server_log: Path) -> None:
    for _ in range(50):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/api/health", timeout=1).read()
            return
        except Exception:
            time.sleep(0.1)
    raise RuntimeError(f"server did not start; running={proc.poll() is None}\n{server_log.read_text()}")


def wait_for_client(port: int, token: str, client_id: str) -> None:
    for _ in range(50):
        try:
            response = post(port, token, "/api/shell/clients", {})
            if any(c["client_id"] == client_id and c["connected"] for c in response["clients"]):
                return
        except Exception:
            pass
        time.sleep(0.1)
    raise RuntimeError(f"client did not register: {client_id}")


def main() -> None:
    root = Path.cwd()
    port = int(os.environ.get("PRIVATE_DROP_E2E_PORT", "18081"))
    token = "test-agent"
    client_id = "e2e-agent"
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        server_log = tmp_path / "server.log"
        agent_log = tmp_path / "agent.log"
        agent_config = tmp_path / "agent.toml"
        agent_config.write_text(
            f'''
server_url = "http://127.0.0.1:{port}"
token = "{token}"
client_id = "{client_id}"
display_name = "E2E Agent"
owner = "test"
poll_interval_ms = 100

[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/tmp"]
max_timeout_secs = 10
max_output_bytes = 262144
'''.strip()
            + "\n"
        )
        server_env = os.environ.copy()
        server_env.update(
            {
                "DROP_ADDR": f"127.0.0.1:{port}",
                "DROP_TOKEN": token,
                "DROP_DATA": str(tmp_path / "data"),
                "PROJECTS_CONFIG": str(root / "projects.toml.example"),
            }
        )
        with server_log.open("wb") as slog, agent_log.open("wb") as alog:
            server = subprocess.Popen(
                [str(root / "target" / "release" / "private-drop")],
                stdout=slog,
                stderr=subprocess.STDOUT,
                env=server_env,
            )
            agent: subprocess.Popen[bytes] | None = None
            try:
                wait_for_health(port, server, server_log)
                agent = subprocess.Popen(
                    [
                        str(root / "target" / "release" / "private-drop-agent"),
                        "--config",
                        str(agent_config),
                    ],
                    stdout=alog,
                    stderr=subprocess.STDOUT,
                    env=os.environ.copy(),
                )
                wait_for_client(port, token, client_id)
                result = post(
                    port,
                    token,
                    "/api/shell/run",
                    {
                        "client_id": client_id,
                        "cwd": "/tmp",
                        "command": "printf agent-ok",
                        "timeout_secs": 5,
                        "wait_timeout_secs": 10,
                    },
                )
                assert result["success"], result
                assert result["stdout"] == "agent-ok", result
                assert result["exit_code"] == 0, result
                print("AGENT_E2E_OK", result["request_id"], result["client_id"])
            finally:
                if agent is not None:
                    agent.terminate()
                    try:
                        agent.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        agent.kill()
                        agent.wait(timeout=5)
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait(timeout=5)


if __name__ == "__main__":
    main()
