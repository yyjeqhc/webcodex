#!/usr/bin/env python3
"""End-to-end smoke test for the shell-client HTTP-poll MVP.

Starts a temporary private-drop server on localhost, registers a synthetic client,
submits a shell request, simulates agent poll/result, and verifies runShell gets
that result.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import threading
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
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read().decode())


def wait_for_health(port: int, server_log: Path, proc: subprocess.Popen[bytes]) -> None:
    for _ in range(50):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/api/health", timeout=1).read()
            return
        except Exception:
            time.sleep(0.1)
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)
    raise RuntimeError(f"server did not start:\n{server_log.read_text()}")


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path.cwd()
        tmp_path = Path(tmp)
        port = int(os.environ.get("PRIVATE_DROP_E2E_PORT", "18080"))
        token = "test-shell"
        env = os.environ.copy()
        env.update(
            {
                "DROP_ADDR": f"127.0.0.1:{port}",
                "DROP_TOKEN": token,
                "DROP_DATA": str(tmp_path / "data"),
                "PROJECTS_CONFIG": str(root / "projects.toml.example"),
            }
        )
        server_log = tmp_path / "server.log"
        with server_log.open("wb") as log:
            proc = subprocess.Popen(
                [str(root / "target" / "release" / "private-drop")],
                stdout=log,
                stderr=subprocess.STDOUT,
                env=env,
            )
            try:
                wait_for_health(port, server_log, proc)

                registered = post(
                    port,
                    token,
                    "/api/shell/agent/register",
                    {
                        "client_id": "xrh",
                        "display_name": "XRH",
                        "owner": "yyjeqhc",
                        "hostname": "fineserver",
                        "capabilities": {"shell": True},
                    },
                )
                assert registered["success"], registered

                run_result: dict[str, Any] = {}

                def run_shell() -> None:
                    run_result["value"] = post(
                        port,
                        token,
                        "/api/shell/run",
                        {
                            "client_id": "xrh",
                            "cwd": "/tmp",
                            "command": "echo hello-from-agent",
                            "timeout_secs": 10,
                            "wait_timeout_secs": 5,
                        },
                    )

                thread = threading.Thread(target=run_shell)
                thread.start()
                time.sleep(0.2)

                poll = post(port, token, "/api/shell/agent/poll", {"client_id": "xrh"})
                assert poll["success"], poll
                assert poll["request"], poll
                request_id = poll["request"]["request_id"]

                completed = post(
                    port,
                    token,
                    "/api/shell/agent/result",
                    {
                        "client_id": "xrh",
                        "request_id": request_id,
                        "exit_code": 0,
                        "stdout": "hello-from-agent\n",
                        "stderr": "",
                        "duration_ms": 12,
                    },
                )
                assert completed["success"], completed
                thread.join(timeout=10)
                assert "value" in run_result, run_result
                result = run_result["value"]
                assert result["success"], result
                assert result["stdout"] == "hello-from-agent\n", result
                print("SHELL_FLOW_OK", result["request_id"], result["client_id"])
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)


if __name__ == "__main__":
    main()
