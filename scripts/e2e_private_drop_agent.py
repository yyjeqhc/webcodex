#!/usr/bin/env python3
"""End-to-end smoke test for the real private-drop-agent binary.

Starts a temporary private-drop server and a real private-drop-agent process,
then submits runShell and verifies the agent executes the command and returns
stdout/stderr/exit_code.
"""

from __future__ import annotations

import json
import hashlib
import os
import subprocess
import tempfile
import time
import urllib.error
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
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as exc:
        body_text = exc.read().decode(errors="replace")
        try:
            body = json.loads(body_text)
        except Exception:
            body = body_text
        raise RuntimeError(f"POST {path} returned HTTP {exc.code}: {body}") from exc


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
        project_dir = tmp_path / "agent-project"
        project_dir.mkdir()
        (project_dir / "marker.txt").write_text("project-marker\n")
        subprocess.run(["git", "init"], cwd=project_dir, check=True, stdout=subprocess.DEVNULL)
        subprocess.run(["git", "config", "user.email", "e2e@example.invalid"], cwd=project_dir, check=True)
        subprocess.run(["git", "config", "user.name", "Private Drop E2E"], cwd=project_dir, check=True)
        subprocess.run(["git", "add", "marker.txt"], cwd=project_dir, check=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=project_dir, check=True, stdout=subprocess.DEVNULL)
        projects_config = tmp_path / "projects.toml"
        projects_config.write_text(
            f'''
[projects.agent_demo]
executor = "agent"
client_id = "{client_id}"
path = "{project_dir}"
allowed_checks = ["test"]

[projects.agent_demo.checks]
test = "printf check-ok"

[projects.agent_demo.commands]
smoke = "printf command-ok && test -f marker.txt"
'''.strip()
            + "\n"
        )
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
                "PROJECTS_CONFIG": str(projects_config),
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

                projects = post(port, token, "/api/codex/projects", {})
                assert projects["success"], projects
                agent_info = next(p for p in projects["projects"] if p["name"] == "agent_demo")
                assert agent_info["executor"] == "agent", agent_info
                command_result = post(
                    port,
                    token,
                    "/api/codex/command",
                    {"project": "agent_demo", "command": "smoke"},
                )
                assert command_result["success"], command_result
                assert command_result["stdout_tail"].strip() == "command-ok", command_result
                check_result = post(
                    port,
                    token,
                    "/api/codex/check",
                    {"project": "agent_demo", "suite": "test"},
                )
                assert check_result["success"], check_result
                assert check_result["stdout_tail"].strip() == "check-ok", check_result
                context_result = post(
                    port,
                    token,
                    "/api/codex/context_batch",
                    {
                        "project": "agent_demo",
                        "requests": [
                            {"mode": "overview"},
                            {"mode": "tree", "limit": 20, "max_depth": 2},
                            {"mode": "read_file", "path": "marker.txt", "limit": 5},
                            {"mode": "grep_context", "query": "project-marker", "limit": 5},
                            {"mode": "git_status"},
                            {"mode": "git_diff"},
                        ],
                    },
                )
                assert context_result["success"], context_result
                assert len(context_result["results"]) == 6, context_result
                assert "Project: agent_demo" in context_result["results"][0]["content"], context_result
                assert "marker.txt" in context_result["results"][1]["items"], context_result
                assert "project-marker" in context_result["results"][2]["content"], context_result
                assert "marker.txt" in context_result["results"][3]["content"], context_result
                edit_result = post(
                    port,
                    token,
                    "/api/codex/edit",
                    {
                        "project": "agent_demo",
                        "edits": [
                            {"type": "replace_text", "path": "marker.txt", "old_text": "project-marker", "new_text": "project-marker-updated"},
                            {"type": "append_file", "path": "marker.txt", "text": "append-ok\n"},
                            {"type": "create_file", "path": "created.txt", "content": "created-ok\n"},
                            {"type": "write_file", "path": "dir/written.txt", "content": "written-ok\n", "allow_overwrite": True},
                        ],
                        "response_mode": "summary",
                    },
                )
                assert edit_result["success"], edit_result
                assert sorted(edit_result["changed_files"]) == ["created.txt", "dir/written.txt", "marker.txt"], edit_result
                edited_read = post(
                    port,
                    token,
                    "/api/codex/context_batch",
                    {
                        "project": "agent_demo",
                        "requests": [
                            {"mode": "read_file", "path": "marker.txt", "limit": 10},
                            {"mode": "read_file", "path": "created.txt", "limit": 10},
                            {"mode": "read_file", "path": "dir/written.txt", "limit": 10},
                        ],
                    },
                )
                assert edited_read["success"], edited_read
                assert "project-marker-updated" in edited_read["results"][0]["content"], edited_read
                assert "append-ok" in edited_read["results"][0]["content"], edited_read
                assert "created-ok" in edited_read["results"][1]["content"], edited_read
                assert "written-ok" in edited_read["results"][2]["content"], edited_read
                dry_run_edit = post(
                    port,
                    token,
                    "/api/codex/edit",
                    {
                        "project": "agent_demo",
                        "dry_run": True,
                        "edits": [
                            {"type": "write_file", "path": "dry-run.txt", "content": "no-write\n", "allow_overwrite": True}
                        ],
                    },
                )
                assert dry_run_edit["success"], dry_run_edit
                dry_run_check = post(
                    port,
                    token,
                    "/api/codex/context_batch",
                    {
                        "project": "agent_demo",
                        "requests": [{"mode": "read_file", "path": "dry-run.txt", "limit": 10}],
                    },
                )
                assert not dry_run_check["success"], dry_run_check
                git_status = post(
                    port,
                    token,
                    "/api/codex/git",
                    {"project": "agent_demo", "operation": "status"},
                )
                assert git_status["success"], git_status
                assert "created.txt" in git_status["stdout_tail"], git_status
                git_diff = post(
                    port,
                    token,
                    "/api/codex/git",
                    {"project": "agent_demo", "operation": "diff", "paths": ["marker.txt"]},
                )
                assert git_diff["success"], git_diff
                assert "project-marker-updated" in git_diff["stdout_tail"], git_diff
                git_add = post(
                    port,
                    token,
                    "/api/codex/git",
                    {"project": "agent_demo", "operation": "add", "paths": ["marker.txt", "created.txt", "dir/written.txt"]},
                )
                assert git_add["success"], git_add
                git_commit = post(
                    port,
                    token,
                    "/api/codex/git",
                    {"project": "agent_demo", "operation": "commit", "paths": ["marker.txt", "created.txt", "dir/written.txt"], "message": "agent git e2e"},
                )
                assert git_commit["success"], git_commit
                git_log = post(
                    port,
                    token,
                    "/api/codex/git",
                    {"project": "agent_demo", "operation": "log"},
                )
                assert git_log["success"], git_log
                assert "agent git e2e" in git_log["stdout_tail"], git_log

                file_path = "/tmp/private-drop-agent-e2e-file.txt"
                file_write = post(
                    port,
                    token,
                    "/api/shell/file",
                    {
                        "op": "write",
                        "client_id": client_id,
                        "path": file_path,
                        "content": "file-ok\n",
                        "wait_timeout_secs": 10,
                    },
                )
                assert file_write["success"], file_write
                assert file_write["bytes"] == len("file-ok\n"), file_write
                expected_hash = hashlib.sha256(b"file-ok\n").hexdigest()
                assert file_write["sha256"] == expected_hash, file_write
                file_read = post(
                    port,
                    token,
                    "/api/shell/file",
                    {
                        "op": "read",
                        "client_id": client_id,
                        "path": file_path,
                        "wait_timeout_secs": 10,
                    },
                )
                assert file_read["success"], file_read
                assert file_read["content"] == "file-ok\n", file_read
                assert file_read["sha256"] == expected_hash, file_read
                stale_write = post(
                    port,
                    token,
                    "/api/shell/file",
                    {
                        "op": "write",
                        "client_id": client_id,
                        "path": file_path,
                        "content": "bad-write\n",
                        "expected_sha256": "0" * 64,
                        "wait_timeout_secs": 10,
                    },
                )
                assert not stale_write["success"], stale_write
                assert "expected_sha256 mismatch" in stale_write["error"], stale_write
                nested_path = "/tmp/private-drop-agent-e2e-dir/nested/file.txt"
                nested_write = post(
                    port,
                    token,
                    "/api/shell/file",
                    {
                        "op": "write",
                        "client_id": client_id,
                        "path": nested_path,
                        "content": "nested-ok\n",
                        "create_dirs": True,
                        "wait_timeout_secs": 10,
                    },
                )
                assert nested_write["success"], nested_write
                nested_read = post(
                    port,
                    token,
                    "/api/shell/file",
                    {
                        "op": "read",
                        "client_id": client_id,
                        "path": nested_path,
                        "wait_timeout_secs": 10,
                    },
                )
                assert nested_read["content"] == "nested-ok\n", nested_read
                file_list = post(
                    port,
                    token,
                    "/api/shell/file",
                    {
                        "op": "list",
                        "client_id": client_id,
                        "path": "/tmp",
                        "wait_timeout_secs": 10,
                    },
                )
                assert file_list["success"], file_list
                assert "private-drop-agent-e2e-file.txt" in file_list["entries"], file_list

                job_start = post(
                    port,
                    token,
                    "/api/shell/job",
                    {
                        "op": "start",
                        "client_id": client_id,
                        "cwd": "/tmp",
                        "command": "printf job-ok",
                        "timeout_secs": 5,
                    },
                )
                assert job_start["success"], job_start
                job_id = job_start["job"]["job_id"]
                for _ in range(50):
                    job_status = post(
                        port,
                        token,
                        "/api/shell/job",
                        {"op": "status", "job_id": job_id},
                    )
                    assert job_status["success"], job_status
                    if job_status["job"]["status"] in ("completed", "failed"):
                        break
                    time.sleep(0.1)
                else:
                    raise RuntimeError(f"job did not finish: {job_id}")
                assert job_status["job"]["status"] == "completed", job_status
                job_log = post(
                    port,
                    token,
                    "/api/shell/job",
                    {"op": "log", "job_id": job_id, "since_stdout_line": 1},
                )
                assert job_log["success"], job_log
                assert job_log["stdout"] == "job-ok\n", job_log

                stop_start = post(
                    port,
                    token,
                    "/api/shell/job",
                    {
                        "op": "start",
                        "client_id": client_id,
                        "cwd": "/tmp",
                        "command": "for i in 1 2 3 4 5; do echo tick-$i; sleep 1; done",
                        "timeout_secs": 20,
                    },
                )
                assert stop_start["success"], stop_start
                stop_job_id = stop_start["job"]["job_id"]
                for _ in range(50):
                    stop_log = post(
                        port,
                        token,
                        "/api/shell/job",
                        {"op": "log", "job_id": stop_job_id, "since_stdout_line": 1},
                    )
                    assert stop_log["success"], stop_log
                    if "tick-1" in (stop_log.get("stdout") or ""):
                        break
                    time.sleep(0.1)
                else:
                    raise RuntimeError(f"job produced no realtime log: {stop_job_id}")
                stopped = post(
                    port,
                    token,
                    "/api/shell/job",
                    {"op": "stop", "job_id": stop_job_id},
                )
                assert stopped["success"], stopped
                for _ in range(50):
                    stop_status = post(
                        port,
                        token,
                        "/api/shell/job",
                        {"op": "status", "job_id": stop_job_id},
                    )
                    assert stop_status["success"], stop_status
                    if stop_status["job"]["status"] in ("stopped", "failed", "timeout"):
                        break
                    time.sleep(0.1)
                else:
                    raise RuntimeError(f"job did not stop: {stop_job_id}")
                assert stop_status["job"]["status"] == "stopped", stop_status
                action_stats = post(
                    port,
                    token,
                    "/api/codex/action_sessions",
                    {"op": "stats", "limit": 5},
                )
                assert action_stats["success"], action_stats
                assert action_stats["sessions"], action_stats
                shell_count = action_stats["sessions"][0]["stats"]["shell_count"]
                assert shell_count >= 4, action_stats
                assert action_stats["sessions"][0]["stats"]["by_endpoint"]["/api/shell/run"] >= 1
                assert action_stats["sessions"][0]["stats"]["by_endpoint"]["/api/shell/file"] >= 1
                assert action_stats["sessions"][0]["stats"]["by_endpoint"]["/api/shell/job"] >= 1
                print("AGENT_E2E_OK", result["request_id"], result["client_id"], job_id, stop_job_id)
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
