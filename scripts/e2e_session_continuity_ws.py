#!/usr/bin/env python3
"""Real MCP -> Server -> Runner -> model API continuity smoke, loopback only.

Build dogfood Server/Runner binaries first; --bin-dir accepts a shared target.
All users, credentials, repository, service state and model responses are temporary.
No production service, ChatGPT account or paid model endpoint is contacted.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.error import HTTPError, URLError
from urllib.request import ProxyHandler, Request, build_opener


ROOT = Path(__file__).resolve().parents[1]
SCOPES = ["runtime:read", "project:read", "project:write", "job:run",
          "session:collaborate", "communication:read", "coding_agent:run"]
DECISION = "Retain the current database schema"
PROGRESS = "Migration checks passed; accessibility review remains"
MODEL_TEXT = "Independent review: 检查恢复上下文 🌎"


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def stop(process: subprocess.Popen | None) -> None:
    if process is None or process.poll() is not None:
        return
    process.send_signal(signal.SIGINT)
    try:
        # Runner allows 12 seconds to drain its owned provider process trees.
        process.wait(timeout=20)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


class Smoke:
    def __init__(self, bin_dir: Path, root: Path):
        self.bin_dir, self.root = bin_dir, root
        self.server = self.runner = None
        self.logs = []
        self.requests = []
        self.opener = build_opener(ProxyHandler({}))
        self.rpc_id = 0
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            self.port = listener.getsockname()[1]
        self.url = f"http://127.0.0.1:{self.port}"
        self.bootstrap = secrets.token_urlsafe(32)
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith(("WEBCODEX_", "GIT_"))}
        self.env.update({"XDG_CONFIG_HOME": str(root / "config"),
                         "XDG_DATA_HOME": str(root / "xdg-data"),
                         "XDG_STATE_HOME": str(root / "state"), "RUST_LOG": "warn",
                         "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"})
        self.api = None
        self.api_thread = None

    def launch(self, binary: str, args: list[str], env: dict) -> subprocess.Popen:
        log = (self.root / f"{binary}-{len(self.logs)}.log").open("wb")
        self.logs.append(log)
        return subprocess.Popen([str(self.bin_dir / binary), *args], env=env,
                                stdin=subprocess.DEVNULL, stdout=log, stderr=log)

    def post(self, path: str, body: dict, token: str) -> dict:
        request = Request(self.url + path, json.dumps(body).encode(), method="POST",
                          headers={"Authorization": "Bearer " + token,
                                   "Content-Type": "application/json",
                                   "Accept": "application/json, text/event-stream"})
        try:
            with self.opener.open(request, timeout=20) as response:
                raw = response.read(2 * 1024 * 1024)
        except HTTPError as error:
            raw = error.read(2 * 1024 * 1024)
        return json.loads(raw)

    def wait_ready(self, project: str | None = None, token: str | None = None) -> None:
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            check(self.server.poll() is None, "temporary Server exited")
            if self.runner:
                check(self.runner.poll() is None, "temporary Runner exited")
            try:
                result = self.post("/api/tools/call", {"tool": "list_projects", "params": {}},
                                   token or self.bootstrap)
                projects = result.get("output", {}).get("projects", [])
                if result.get("success") and (project is None or any(
                        item.get("id") == project for item in projects)):
                    return
            except (OSError, URLError, ValueError):
                pass
            time.sleep(0.2)
        raise AssertionError("temporary Server/Runner readiness deadline exceeded")

    def start_server(self) -> None:
        env = self.env | {"WEBCODEX_ADDR": f"127.0.0.1:{self.port}",
                          "WEBCODEX_DATA": str(self.root / "data"),
                          "WEBCODEX_TOKEN": self.bootstrap}
        self.server = self.launch("webcodex-server", [], env)
        self.wait_ready()

    def pat(self, username: str, scopes: list[str]) -> str:
        response = self.post("/api/tokens/create", {"username": username,
                             "name": "continuity-smoke", "scopes": scopes}, self.bootstrap)
        check(isinstance(response.get("token"), str), "temporary PAT creation failed")
        return response["token"]

    def initialize(self, token: str) -> None:
        response = self.post("/mcp", {"jsonrpc": "2.0", "id": 1, "method": "initialize",
                             "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                                        "clientInfo": {"name": "continuity-smoke", "version": "1"}}}, token)
        check("protocolVersion" in response.get("result", {}), "MCP initialization failed")

    def tool(self, name: str, args: dict, token: str, expect_success: bool = True) -> dict:
        self.rpc_id += 1
        response = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
                             "method": "tools/call", "params": {"name": "call_runtime_tool",
                             "arguments": {"tool": name, "arguments": args}}}, token)
        result = response.get("result", {}).get("structuredContent")
        if result is None:
            # HTTP scope admission can reject a call before MCP dispatch.
            denied = response.get("error") == "insufficient_scope" or (
                response.get("status") == 403 and isinstance(response.get("error"), str)
                and response["error"].startswith("missing required scope: "))
            check(not expect_success and denied,
                  f"{name}: missing MCP result; response fields={list(response)}")
            return {"error_kind": "insufficient_scope"}
        error = result.get("output", {}).get("error_kind")
        check(result.get("success") is expect_success,
              f"{name}: expected success={expect_success}; error={error}")
        return result.get("output", {})

    def start_model_server(self) -> None:
        requests = self.requests

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                requests.append((self.path, self.headers.get("Authorization"), body))
                if self.path == "/v1/responses":
                    events = [{"type": "response.output_text.delta", "delta": MODEL_TEXT},
                              {"type": "response.completed", "response": {
                                  "status": "completed", "output": []}}]
                elif self.path == "/v1/chat/completions":
                    events = [{"choices": [{"delta": {"content": MODEL_TEXT},
                                            "finish_reason": None}]},
                              {"choices": [{"delta": {}, "finish_reason": "stop"}]}, "[DONE]"]
                else:
                    self.send_error(404)
                    return
                data = "".join("data: " + (event if isinstance(event, str) else
                                          json.dumps(event, ensure_ascii=False)) + "\n\n"
                               for event in events).encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *_):
                pass

        self.api = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.api_thread = threading.Thread(target=self.api.serve_forever, daemon=True)
        self.api_thread.start()

    def run(self) -> None:
        self.start_model_server()
        self.start_server()
        for username in ["continuity-owner", "continuity-foreign"]:
            user = self.post("/api/users/create", {"username": username}, self.bootstrap)
            check(user.get("success") is True, "temporary user creation failed")
        first = self.pat("continuity-owner", SCOPES)
        second = self.pat("continuity-owner", SCOPES)
        foreign = self.pat("continuity-foreign", SCOPES)
        restricted = self.pat("continuity-owner", [s for s in SCOPES if s != "coding_agent:run"])
        agent = self.post("/api/agent-tokens/create", {"username": "continuity-owner",
                          "client_id": "continuity-runner", "name": "continuity-smoke"}, self.bootstrap)
        check(isinstance(agent.get("token"), str), "temporary Runner token creation failed")
        project_dir = self.root / "project"
        registry = self.root / "projects"
        project_dir.mkdir()
        registry.mkdir()
        subprocess.run(["git", "init", "-q", "-b", "main", str(project_dir)],
                       env=self.env, stdin=subprocess.DEVNULL, check=True)
        (project_dir / "README.md").write_text("# Continuity smoke\n")
        (registry / "project.toml").write_text(f'id = "continuity"\npath = {json.dumps(str(project_dir))}\n')
        config = [f'server_url = "{self.url}"', f'token = {json.dumps(agent["token"])}',
                  'client_id = "continuity-runner"', 'owner = "continuity-owner"',
                  f'project_registry_dir = {json.dumps(str(registry))}', 'transport = "websocket"',
                  '[policy]', 'allow_cwd_anywhere = false', 'allow_raw_shell = true',
                  f'allowed_roots = [{json.dumps(str(project_dir))}]', '[acp]', 'max_concurrent_runs = 2']
        for api in ["responses", "chat-completions"]:
            args = [str(ROOT / "integrations/model_gateway/adapter.py"), "--api", api,
                    "--base-url", f"http://127.0.0.1:{self.api.server_port}/v1", "--model", api,
                    "--api-key-env", "MODEL_API_KEY"]
            config.extend(['[[acp.agents]]', f'id = "{api}"', f'name = "{api}"',
                           f'executable = {json.dumps(sys.executable)}', f'args = {json.dumps(args)}',
                           '[acp.agents.env_from_env]', 'MODEL_API_KEY = "CONTINUITY_API_KEY"'])
        runner_config = self.root / "runner.toml"
        runner_config.write_text("\n".join(config) + "\n")
        runner_config.chmod(0o600)
        self.runner = self.launch("webcodex-runner", ["--config", str(runner_config)],
                                  self.env | {"CONTINUITY_API_KEY": "local-fixture-credential"})
        project = "agent:continuity-runner:continuity"
        self.wait_ready(project, first)
        self.initialize(first)
        started = self.tool("work_on_project", {"project": project,
                            "instruction": "Complete the continuity implementation"}, first)
        session = started["session_id"]
        for kind, message in [("decision", DECISION), ("progress", PROGRESS),
                              ("decision", "API_KEY=private-fixture-value")]:
            self.tool("post_session_message", {"session_id": session, "kind": kind,
                      "message": message, "delivery_key": "save-" + kind + "-" + str(len(message))}, first)
        stop(self.server)
        self.start_server()
        self.wait_ready(project, second)
        self.initialize(second)
        sessions = self.tool("list_sessions", {"project": project}, second)
        check(sessions["total"] == 1, "restart/new credential must discover exactly the saved task")
        candidate = sessions["sessions"][0]
        check(candidate["session_id"] == session and candidate["lifecycle"] == "active",
              "discovery changed Session identity/lifecycle")
        context = candidate["session_ref"]
        handoff = self.tool("read_session_handoff", {"project": project, "session_id": context,
                            "include_workspace": False}, second)
        brief = json.dumps(handoff["handoff_brief"], ensure_ascii=False)
        check(DECISION in brief and PROGRESS in brief, "saved decision/progress missing after restart")
        check("private-fixture-value" not in brief, "handoff leaked a secret-like note")
        check(len(brief.encode()) <= 8192, "handoff exceeded bounded recovery payload")
        resumed = self.tool("work_on_project", {"project": project, "session_id": session,
                            "instruction": "Continue the explicit saved task"}, second)
        check(resumed["session_id"] == session and resumed["continuation"] == "resumed_explicitly",
              "explicit recovery created a different Session")
        self.tool("list_sessions", {"project": project}, foreign, False)
        self.tool("read_session_handoff", {"project": project, "session_id": session}, foreign, False)
        self.tool("start_coding_agent", {"project": project, "provider_id": "responses",
                  "idempotency_key": "restricted", "instruction": "Review", "context_session_id": context},
                  restricted, False)
        check(not self.requests, "denied calls dispatched a model request")
        print("PASS: MCP discovery, persisted notes, explicit recovery and authority after Server restart")
        for api in ["responses", "chat-completions"]:
            params = {"project": project, "provider_id": api, "idempotency_key": "review-" + api,
                      "instruction": "Review recovery independently", "context_session_id": context,
                      "timeout_secs": 30}
            run = self.tool("start_coding_agent", params, second)
            deadline = time.monotonic() + 35
            # Read retained output first, even if the model finished during start.
            observation = {}
            text = ""
            while time.monotonic() < deadline:
                observation = self.tool("observe_coding_agent", {"run_id": run["run_id"], "wait_secs": 1,
                                       "after_observation_token": observation.get("observation_token")}, second)
                text += "".join(event.get("text") or "" for event in observation["events"]
                                if event["kind"] == "agent_message")
                if observation["state"] in ["completed", "failed", "cancelled", "lost"]:
                    break
            check(observation["state"] == "completed", f"{api}: ACP Run state={observation['state']}")
            check(MODEL_TEXT in text, f"{api}: initial observation omitted retained model text")
            request_path, auth, body = self.requests[-1]
            check(request_path == "/v1/" + ("responses" if api == "responses" else "chat/completions"),
                  "provider routing mismatch")
            check(auth == "Bearer local-fixture-credential" and body["model"] == api and body["stream"],
                  "Runner-owned credential/model mapping mismatch")
            prompt = json.dumps(body, ensure_ascii=False)
            check(DECISION in prompt and PROGRESS in prompt and "private-fixture-value" not in prompt,
                  "delegation did not receive the authorized sanitized recovery context")
            check("tools" not in body and (api != "responses" or body["store"] is False),
                  "text adapter unexpectedly sent tools or enabled provider storage")
            count = len(self.requests)
            replay = self.tool("start_coding_agent", params, second)
            check(replay["run_id"] == run["run_id"] and len(self.requests) == count,
                  "exact initiation retry dispatched another request")
            self.tool("post_session_message", {"session_id": session, "kind": "progress",
                      "message": "Reviewed " + api, "delivery_key": "reviewed-" + api}, second)
            conflict = self.tool("start_coding_agent", params, second, False)
            check(conflict["error_kind"] == "idempotency_conflict"
                  and conflict["run_id"] == run["run_id"]
                  and conflict["execution_state"] == "not_started" and len(self.requests) == count,
                  "changed recovery snapshot did not fence redispatch")
            print(f"PASS: {api} MCP -> Runner -> ACP -> SSE roundtrip; exact replay and changed-context fence")
        self.tool("close_session", {"session_id": session}, second)
        closed = self.tool("list_sessions", {"project": project, "lifecycle": "closed"}, second)
        check(closed["total"] == 1, "closed history disappeared from discovery")
        self.tool("read_session_handoff", {"project": project, "session_id": context,
                  "include_workspace": False}, second)
        check(len(self.requests) == 2, "smoke sent unexpected duplicate model requests")
        print("PASS: closed history remains readable; two model requests total; all services isolated")

    def close(self) -> None:
        stop(self.runner)
        stop(self.server)
        if self.api:
            self.api.shutdown()
            self.api.server_close()
            self.api_thread.join(timeout=5)
        for log in self.logs:
            log.close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    args = parser.parse_args()
    check(sys.version_info >= (3, 10), "Python 3.10+ is required")
    check(shutil.which("git") is not None, "git is required")
    for binary in ["webcodex-server", "webcodex-runner"]:
        check((args.bin_dir / binary).is_file(), "build dogfood Server/Runner binaries first")
    with tempfile.TemporaryDirectory(prefix="webcodex-continuity-e2e-") as root:
        smoke = Smoke(args.bin_dir.resolve(), Path(root))
        try:
            smoke.run()
        finally:
            smoke.close()


if __name__ == "__main__":
    main()
