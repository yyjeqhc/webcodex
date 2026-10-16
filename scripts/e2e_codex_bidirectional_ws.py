#!/usr/bin/env python3
"""Opt-in real Codex MCP consumption and managed ACP execution acceptance.

This calls the configured, already-authenticated Codex account. The model is
fixed to gpt-5.6-luna/medium. All WebCodex users, credentials, projects and
services are local to a NEW output directory; an existing directory is rejected.
No shared service is restarted. Authentication files are never read by this
script. Private recovery credentials remain under output-dir/private (0700/0600).
Raw service logs are reduced to fixed diagnostic counts and deleted during cleanup.

Build dogfood binaries first, then supply explicit local executable/package paths:
  python3 scripts/e2e_codex_bidirectional_ws.py --output-dir /tmp/new-codex-run \
    --bin-dir target/dogfood --adapter-script /path/to/codex-acp/dist/index.js \
    --codex-path /path/to/codex

A completed ACP turn is not acceptance: fixed fixture bytes and independent
Python tests must also pass. A transparent test-only ACP relay records only
process identity, negotiated model/effort and prompt-dispatch counts, never
prompt bodies, session ids, credentials or model/tool text.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from urllib.error import HTTPError
from urllib.request import Request

from e2e_session_continuity_ws import ROOT, SCOPES, Smoke, stop

MODEL = "gpt-5.6-luna"
EFFORT = "medium"
ADAPTER_VERSION = "2.1.1"
TERMINAL = {"completed", "failed", "cancelled", "lost"}
CONSUMER_SCOPES = ["runtime:read", "project:read"]
BINDING_FIELDS = (
    "task_id", "attempt_id", "run_id", "project", "provider_id",
    "dispatch_state", "run_state", "execution_state", "execution_status",
    "execution_recovery", "replayed", "task_state", "attempt_state",
)
# Instrumentation, not a new execution owner: the relay stays in the Runner's
# ManagedChild tree and never approves ACP requests.
ACP_RELAY = r'''import json, os, subprocess, sys, threading, time
audit_path, executable, *args = sys.argv[1:]
lock = threading.Lock()
current_config = {}
def record(event, **fields):
    raw = (json.dumps({"event": event, "time_ns": time.time_ns(), **fields}) + "\n").encode()
    with lock:
        fd = os.open(audit_path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        try:
            os.write(fd, raw)
            os.fsync(fd)
        finally:
            os.close(fd)
def inspect(frame, direction):
    global current_config
    try:
        obj = json.loads(frame)
    except (ValueError, UnicodeError):
        return
    if not isinstance(obj, dict):
        return
    method = obj.get("method")
    if direction == "request" and method == "session/prompt":
        record("prompt_dispatch", effective_config=current_config.copy())
    if direction != "response":
        return
    result = obj.get("result")
    if not isinstance(result, dict):
        return
    info = result.get("agentInfo")
    if isinstance(info, dict):
        record("adapter_identity", name=str(info.get("name", ""))[:100],
               version=str(info.get("version", ""))[:40])
    options = result.get("configOptions")
    if isinstance(options, list):
        current = {}
        for option in options:
            if isinstance(option, dict) and option.get("id") in ("model", "reasoning_effort"):
                value = option.get("currentValue")
                if isinstance(value, (str, bool)):
                    current[option["id"]] = value
        current_config = current
        record("config_observed", values=current)
def relay(source, destination, direction):
    while True:
        frame = source.readline(2 * 1024 * 1024 + 1)
        if not frame:
            break
        if len(frame) > 2 * 1024 * 1024:
            record("frame_limit")
            child.terminate()
            break
        inspect(frame, direction)
        try:
            destination.write(frame)
            destination.flush()
        except (BrokenPipeError, OSError):
            break
    if direction == "request":
        try:
            destination.close()
        except OSError:
            pass
child = subprocess.Popen([executable, *args], stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
record("adapter_process_started", pid=child.pid, relay_pid=os.getpid())
diagnostic_counts = {}
def diagnose(text):
    for needle in ("refresh_token_expired", "refresh_token_reused", "refresh_token_invalidated",
                   "invalid_grant", "failed to refresh token", "401", "unauthorized"):
        if needle in text.lower():
            diagnostic_counts[needle] = diagnostic_counts.get(needle, 0) + 1
            if diagnostic_counts[needle] == 1:
                record("diagnostic", category=needle)
def drain_stderr():
    while True:
        data = child.stderr.readline(8192)
        if not data:
            return
        diagnose(data.decode("utf-8", "replace"))
threading.Thread(target=relay, args=(sys.stdin.buffer, child.stdin, "request"), daemon=True).start()
stderr_thread = threading.Thread(target=drain_stderr, daemon=True)
stderr_thread.start()
relay(child.stdout, sys.stdout.buffer, "response")
code = child.wait()
stderr_thread.join(timeout=2)
record("adapter_process_exited", exit_code=code, stderr_drained=not stderr_thread.is_alive())
sys.exit(code)
'''


DIAGNOSTIC_MARKERS = {
    "refresh_token_expired": "refresh_token_expired",
    "refresh_token_reused": "refresh_token_reused",
    "refresh_token_invalidated": "refresh_token_invalidated",
    "invalid_grant": "invalid_grant",
    "failed_to_refresh_token": "failed to refresh token",
    "http_401": "401",
    "unauthorized": "unauthorized",
    "permission_denied": "permission denied",
    "unknown_config_field": "unknown field",
}


def drain_diagnostics(stream, summary: dict) -> None:
    """Drain concurrently; retain fixed categories and counts, never source text."""
    while True:
        chunk = stream.readline(8192)
        if not chunk:
            return
        summary["bytes_seen"] += len(chunk)
        lowered = chunk.decode("utf-8", "replace").lower()
        for name, needle in DIAGNOSTIC_MARKERS.items():
            if needle in lowered:
                summary["categories"][name] += 1


def summarize_and_remove_service_log(path: Path) -> dict:
    """Project one explicitly owned, closed service log, then remove its raw bytes."""
    maximum = 2 * 1024 * 1024
    summary = {"log": path.name, "bytes_inspected": 0, "limit_bytes": maximum,
               "truncated": False, "categories": {}, "raw_deleted": False}
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        with os.fdopen(fd, "rb") as stream:
            data = stream.read(maximum)
            summary["bytes_inspected"] = len(data)
            summary["truncated"] = os.fstat(stream.fileno()).st_size > len(data)
        lowered = data.lower()
        summary["categories"] = {name: count for name, needle in DIAGNOSTIC_MARKERS.items()
                                 if (count := lowered.count(needle.encode("utf-8")))}
    except OSError as error:
        summary["observation"] = "unavailable"
        summary["read_errno"] = error.errno
    finally:
        try:
            path.unlink()
            summary["raw_deleted"] = True
        except FileNotFoundError:
            summary["raw_deleted"] = True
        except OSError as error:
            summary["delete_errno"] = error.errno
    return summary


class AcceptanceFailure(Exception):
    """Only fixed, credential-free error codes may be exposed."""


def require(condition: bool, code: str) -> None:
    if not condition:
        raise AcceptanceFailure(code)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def private_json(path: Path, value: dict) -> None:
    temporary = path.with_name(path.name + ".tmp")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def bounded_jsonl(path: Path, maximum: int = 8 * 1024 * 1024) -> list[dict]:
    require(path.stat().st_size <= maximum, "structured_log_limit")
    result = []
    with path.open() as stream:
        for line in stream:
            if line.strip():
                value = json.loads(line)
                require(isinstance(value, dict), "unexpected_jsonl_frame")
                result.append(value)
    return result


def process_version(executable: Path) -> str:
    result = subprocess.run([str(executable), "--version"], stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=15, check=False)
    require(result.returncode == 0, "version_probe_failed")
    text = result.stdout.strip()
    require(len(text) <= 512 and "\n" not in text, "unexpected_version_output")
    return text


def stop_group(process: subprocess.Popen | None) -> bool:
    """Clean the consumer's explicitly created session even if its leader exited."""
    if process is None:
        return True
    pgid = process.pid  # Popen(start_new_session=True), never discovered by name.
    require(pgid != os.getpgrp(), "consumer_group_must_be_independent")
    for sig, budget in ((signal.SIGTERM, 10), (signal.SIGKILL, 2)):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            process.poll()
            return True
        deadline = time.monotonic() + budget
        while time.monotonic() < deadline:
            process.poll()  # Reap the owned leader; descendants may outlive it.
            try:
                os.killpg(pgid, 0)
            except ProcessLookupError:
                return True
            time.sleep(0.05)
    # Zombies or descendants still visible in the group are an explicit limit.
    return False


def require_consumer_read_only_activity(frames: list[dict]) -> None:
    """Reject attempted non-read tools, including events without completion.

    A later successful read cannot erase a previously started or failed shell,
    web, file-change, or out-of-scope MCP operation from acceptance evidence.
    The completed read's exact routing and receipt are checked separately.
    """
    for frame in frames:
        event = frame.get("type")
        if event not in ("item.started", "item.updated", "item.completed", "item.failed"):
            continue
        item = frame.get("item")
        if not isinstance(item, dict):
            continue
        kind = item.get("type")
        if kind in ("command_execution", "file_change", "web_search"):
            raise AcceptanceFailure("consumer_used_unrequested_tools")
        if kind == "mcp_tool_call":
            if ((item.get("server") is not None and item["server"] != "bidirectional")
                    or (item.get("tool") is not None and item["tool"] != "read_files")):
                raise AcceptanceFailure("consumer_called_wrong_mcp_tool")
            if event == "item.failed":
                raise AcceptanceFailure("consumer_mcp_call_failed")


def mcp_result(item: dict) -> dict:
    result = item.get("result")
    require(item.get("status") == "completed" and not item.get("error")
            and isinstance(result, dict), "consumer_mcp_call_failed")
    # Codex native JSONL uses structured_content; MCP itself uses structuredContent.
    structured = result.get("structured_content", result.get("structuredContent"))
    if structured is None:
        candidates = []
        for block in result.get("content", []):
            if isinstance(block, dict) and block.get("type") == "text":
                try:
                    candidate = json.loads(block["text"])
                except (ValueError, KeyError):
                    continue
                if isinstance(candidate, dict) and "success" in candidate:
                    candidates.append(candidate)
        require(len(candidates) == 1, "consumer_missing_structured_tool_result")
        structured = candidates[0]
    require(isinstance(structured, dict) and structured.get("success") is True,
            "consumer_webcodex_tool_unsuccessful")
    return structured["output"]


class BidirectionalSmoke(Smoke):
    def __init__(self, args: argparse.Namespace):
        super().__init__(args.bin_dir, args.output_dir)
        self.args = args
        self.private = self.root / "private"
        self.private.mkdir(mode=0o700)
        self.fixture = self.root / "project"
        self.registry = self.root / "projects"
        self.report = {
            "format": "webcodex-codex-bidirectional-v1", "state": "running",
            "started_at_unix": time.time(), "source_head": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            "script_sha256": sha256(Path(__file__)), "model": MODEL, "effort": EFFORT,
            "host_job_id": args.host_job_id, "process_id": os.getpid(),
            "output_dir": str(self.root), "consumer": {"passed": False},
            "managed": {"passed": False}, "recovery": {"passed": False},
            "cleanup": {"process_cleanup_finished": False, "execution_terminal_observed": False},
        }
        self.operator = self.consumer_token = ""
        self.consumer_process = None
        self.binding = None
        self.checkpoint("preflight")

    def checkpoint(self, phase: str) -> None:
        self.report["phase"] = phase
        self.report["updated_at_unix"] = time.time()
        private_json(self.root / "report.json", self.report)

    def launch(self, binary: str, args: list[str], env: dict) -> subprocess.Popen:
        # Retain the common smoke lifecycle, but keep raw logs private.
        path = self.private / f"{binary}-{len(self.logs)}.log"
        log = path.open("xb")
        path.chmod(0o600)
        self.logs.append(log)
        return subprocess.Popen([str(self.bin_dir / binary), *args], env=env,
                                stdin=subprocess.DEVNULL, stdout=log, stderr=log)

    def post(self, path: str, body: dict, token: str) -> dict:
        # CodingAgent start can legitimately wait up to 32 seconds. The common
        # smoke's 20-second HTTP default is too short for that admitted operation.
        request = Request(self.url + path, json.dumps(body).encode(), method="POST",
                          headers={"Authorization": "Bearer " + token,
                                   "Content-Type": "application/json",
                                   "Accept": "application/json, text/event-stream"})
        try:
            with self.opener.open(request, timeout=45) as response:
                raw = response.read(2 * 1024 * 1024 + 1)
        except HTTPError as error:
            raw = error.read(2 * 1024 * 1024 + 1)
        require(len(raw) <= 2 * 1024 * 1024, "http_result_limit")
        return json.loads(raw)

    def tool(self, name: str, args: dict, token: str, expect_success: bool = True) -> dict:
        self.rpc_id += 1
        response = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
            "method": "tools/call", "params": {"name": "call_runtime_tool",
            "arguments": {"tool": name, "arguments": args}}}, token)
        result = response.get("result", {}).get("structuredContent")
        require(isinstance(result, dict), "missing_mcp_result")
        output = result.get("output", {})
        require(isinstance(output, dict), "invalid_mcp_output")
        self.report["last_tool"] = name
        if name in ("observe_coding_agent", "reconcile_agent_task_coding_run"):
            state = output.get("state", output.get("run_state"))
            if state:
                self.report["managed"]["last_run_state"] = state
            terminal = output.get("terminal") or {}
            self.report["managed"]["last_terminal"] = {
                key: terminal[key] for key in ("stop_reason", "error_code") if key in terminal}
        if name == "start_agent_task_coding_run" and output.get("run_id"):
            # Keep even a failed or uncertain initiation's exact identity.
            self.binding = output
            self.report["managed"]["run_id"] = output["run_id"]
            self.report["managed"]["latest_binding"] = {
                key: output[key] for key in BINDING_FIELDS if key in output}
        if result.get("success") is not expect_success:
            code = output.get("error_kind", "failed")
            if not isinstance(code, str) or re.fullmatch(r"[a-z0-9_]{1,100}", code) is None:
                code = "failed"
            self.report["last_tool_failure"] = {"tool": name, "error_kind": code,
                **{key: output[key] for key in BINDING_FIELDS if key in output}}
            self.checkpoint("tool_failure")
            raise AcceptanceFailure("tool_" + name + "_" + code)
        return output

    def fixtures(self) -> None:
        self.fixture.mkdir(mode=0o700)
        self.registry.mkdir(mode=0o700)
        source = ROOT / "crates/webcodex-runner/src/webcodex_runner/coding_agent/tests/dogfood.rs"
        match = re.search(r'const FIXTURE_TESTS: &str = r#"(.*?)"#;', source.read_text(), re.S)
        require(match is not None, "canonical_clamp_tests_missing")
        (self.fixture / "test_clamp.py").write_text(match.group(1))
        (self.fixture / "clamp.py").write_text(
            "def clamp(value, lower, upper):\n    raise NotImplementedError\n")
        (self.fixture / ".gitignore").write_text("__pycache__/\n")
        (self.fixture / "README.md").write_text("# Disposable Codex acceptance fixture\n")
        self.expected_files = {}
        for name in ("alpha.txt", "beta.txt"):
            relative = Path("tests/fixtures/agent-loop-multi-file") / name
            destination = self.fixture / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, destination)
            self.expected_files[str(relative)] = destination.read_text()
        self.fixed_hash = sha256(self.fixture / "test_clamp.py")
        for command in (
            ["git", "-c", "init.templateDir=", "-c", "core.hooksPath=/dev/null",
             "init", "-q", "-b", "main"],
            ["git", "-c", "core.hooksPath=/dev/null", "add", "."],
            ["git", "-c", "user.name=Codex acceptance", "-c",
             "user.email=acceptance@example.invalid", "-c", "core.hooksPath=/dev/null",
             "commit", "-qm", "Fixed independent acceptance fixture"],
        ):
            result = subprocess.run(command, cwd=self.fixture, env=self.env,
                                    stdin=subprocess.DEVNULL, capture_output=True, timeout=15)
            require(result.returncode == 0, "fixture_git_setup_failed")
        self.report["fixture"] = {
            "path": str(self.fixture), "fixed_test_source": str(source.relative_to(ROOT)),
            "fixed_test_sha256": self.fixed_hash,
            "read_fixture_sha256": {path: sha256(self.fixture / path) for path in self.expected_files},
        }

    def provision(self) -> None:
        self.start_server()
        self.report["server_pid"] = self.server.pid
        username, client_id = "codex-acceptance-owner", "codex-acceptance-runner"
        response = self.post("/api/users/create", {"username": username}, self.bootstrap)
        require(response.get("success") is True, "local_user_provisioning_failed")
        self.operator = self.pat(username, sorted(set(SCOPES + ["communication:manage"])))
        self.consumer_token = self.pat(username, CONSUMER_SCOPES)
        runner_token = self.post("/api/agent-tokens/create", {
            "username": username, "client_id": client_id, "name": "codex-bidirectional"
        }, self.bootstrap).get("token")
        require(isinstance(runner_token, str), "local_runner_provisioning_failed")
        self.project = f"agent:{client_id}:fixture"
        (self.registry / "fixture.toml").write_text(
            'id = "fixture"\npath = ' + json.dumps(str(self.fixture)) + "\n")
        relay = self.private / "acp_audit.py"
        relay.write_text(ACP_RELAY)
        relay.chmod(0o600)
        self.audit = self.private / "acp-metadata.jsonl"
        runner_env = self.env | {"CODEX_PATH": str(self.args.codex_path)}
        env_names = [
            "HOME", "PATH", "CODEX_HOME", "CODEX_PATH", "USER", "SHELL",
            "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
            "http_proxy", "https_proxy", "all_proxy", "no_proxy", "SSL_CERT_FILE", "SSL_CERT_DIR",
        ]
        mappings = {name: name for name in env_names if name in runner_env}
        require("HOME" in mappings and "PATH" in mappings, "codex_environment_missing")
        config = [
            "server_url = " + json.dumps(self.url), "token = " + json.dumps(runner_token),
            "client_id = " + json.dumps(client_id), "owner = " + json.dumps(username),
            "project_registry_dir = " + json.dumps(str(self.registry)), 'transport = "websocket"',
            "[policy]", "allow_cwd_anywhere = false", "allow_raw_shell = true",
            "allowed_roots = [" + json.dumps(str(self.fixture)) + "]",
            "[acp]", "max_concurrent_runs = 1", "permission_timeout_secs = 3",
            "[acp.forced_config]", "model = " + json.dumps(MODEL),
            "reasoning_effort = " + json.dumps(EFFORT),
            "[[acp.agents]]", 'id = "codex"', 'name = "Pinned Codex ACP acceptance"',
            "executable = " + json.dumps(sys.executable),
            "args = " + json.dumps([str(relay), str(self.audit),
                                   str(self.args.node_path), str(self.args.adapter_script)]),
            "allowed_config_options = []", "[acp.agents.env_from_env]",
        ]
        config.extend(name + " = " + json.dumps(source) for name, source in mappings.items())
        self.runner_config = self.private / "runner.toml"
        self.runner_config.write_text("\n".join(config) + "\n")
        self.runner_config.chmod(0o600)
        private_json(self.private / "recovery.json", {
            "url": self.url, "bootstrap": self.bootstrap, "operator_token": self.operator,
            "consumer_token": self.consumer_token, "runner_config": str(self.runner_config),
            "server_data": str(self.root / "data"),
        })
        self.runner = self.launch("webcodex-runner", ["--config", str(self.runner_config)], runner_env)
        self.wait_ready(self.project, self.operator)
        self.initialize(self.operator)
        self.report["runner_pid"] = self.runner.pid
        self.report["project"] = self.project
        self.report["environment_mappings"] = list(mappings)
        self.checkpoint("services_ready")

    def no_lifecycle(self) -> dict:
        counts = {}
        for tool, key, params in (
            ("list_sessions", "sessions", {"project": self.project}),
            ("list_goals", "goals", {}),
            ("list_agent_identities", "agents", {}),
            ("list_agent_tasks", "tasks", {}),
        ):
            output = self.tool(tool, params, self.operator)
            require(isinstance(output.get(key), list), "lifecycle_inventory_shape")
            counts[key] = len(output[key])
            require(counts[key] == 0, "consumer_created_durable_lifecycle")
        return counts

    def consumer(self) -> None:
        try:
            self.consumer_turn()
        finally:
            self.report["cleanup"]["consumer_group_stopped"] = stop_group(self.consumer_process)
            raw = self.private / "consumer.jsonl"
            if raw.exists():
                # Auth failures can also be native JSON error events. Preserve
                # only fixed diagnostic counts, never those raw event messages.
                summary = {"bytes_seen": 0, "categories": dict.fromkeys(DIAGNOSTIC_MARKERS, 0)}
                try:
                    with raw.open("rb") as stream:
                        drain_diagnostics(stream, summary)
                    self.report["consumer"]["stdout_diagnostics"] = summary
                finally:
                    raw.unlink()

    def consumer_turn(self) -> None:
        self.no_lifecycle()
        server = (
            '{url=' + json.dumps(self.url + "/mcp")
            + ',bearer_token_env_var="WEBCODEX_BIDIRECTIONAL_READ_TOKEN"'
            + ',enabled_tools=["read_files"]'
            + ',http_headers={"X-WebCodex-MCP-Profile"="direct"}}'
        )
        command = [
            str(self.args.codex_path), "exec", "--ignore-user-config", "--strict-config",
            "--sandbox", "read-only", "--ephemeral", "--json", "--model", MODEL,
            "--config", 'model_reasoning_effort="medium"',
            "--config", "mcp_servers={bidirectional=" + server + "}",
            "--cd", str(self.fixture), "-",
        ]
        prompt = (
            "Use only the WebCodex bidirectional MCP read_files tool for this read-only task. "
            f"The exact registered project is {self.project}. "
            "Read tests/fixtures/agent-loop-multi-file/alpha.txt and "
            "tests/fixtures/agent-loop-multi-file/beta.txt in one batch. "
            "Do not use shell, filesystem, web or other tools. Do not create or resume a Goal, "
            "Workflow Session, durable Agent or AgentTask; do not delegate work. "
            "Return one JSON object containing exactly BENCH_TARGET_ALPHA, BENCH_TARGET_BETA, "
            "ALPHA_UNRELATED_SENTINEL and BETA_UNRELATED_SENTINEL with their actual string values."
        )
        stdout = self.private / "consumer.jsonl"
        diagnostic = {"bytes_seen": 0, "categories": dict.fromkeys(DIAGNOSTIC_MARKERS, 0)}
        self.report["consumer"]["diagnostics"] = diagnostic
        with stdout.open("xb") as output:
            stdout.chmod(0o600)
            self.consumer_process = subprocess.Popen(
                command, cwd=self.fixture,
                env=self.env | {"WEBCODEX_BIDIRECTIONAL_READ_TOKEN": self.consumer_token},
                stdin=subprocess.PIPE, stdout=output, stderr=subprocess.PIPE, start_new_session=True)
            diagnostics_thread = threading.Thread(target=drain_diagnostics,
                args=(self.consumer_process.stderr, diagnostic), daemon=True)
            diagnostics_thread.start()
            self.report["consumer"]["pid"] = self.consumer_process.pid
            self.checkpoint("consumer_running")
            self.consumer_process.stdin.write(prompt.encode())
            self.consumer_process.stdin.close()
            deadline = time.monotonic() + self.args.timeout
            while self.consumer_process.poll() is None:
                require(time.monotonic() < deadline, "consumer_deadline")
                require(stdout.stat().st_size <= 8 * 1024 * 1024
                        and diagnostic["bytes_seen"] <= 2 * 1024 * 1024, "consumer_output_limit")
                time.sleep(0.2)
            diagnostics_thread.join(timeout=2)
        self.report["consumer"]["exit_code"] = self.consumer_process.returncode
        require(self.consumer_process.returncode == 0, "consumer_process_failed")
        frames = bounded_jsonl(stdout)
        require_consumer_read_only_activity(frames)
        completed = [frame["item"] for frame in frames
                     if frame.get("type") == "item.completed" and isinstance(frame.get("item"), dict)]
        calls = [item for item in completed if item.get("type") == "mcp_tool_call"]
        require(bool(calls), "consumer_did_not_call_mcp")
        observed = {}
        call_evidence = []
        for item in calls:
            require(item.get("server") == "bidirectional" and item.get("tool") == "read_files",
                    "consumer_called_wrong_mcp_tool")
            arguments = item.get("arguments")
            if isinstance(arguments, str):
                arguments = json.loads(arguments)
            require(isinstance(arguments, dict) and arguments.get("project") == self.project,
                    "consumer_project_mismatch")
            require("session_id" not in arguments and "attempt_ref" not in arguments
                    and "_wc" not in arguments, "consumer_used_lifecycle_context")
            result = mcp_result(item)
            require(result.get("failed_count") == 0, "consumer_file_read_failed")
            files = []
            for value in result.get("items", []):
                require(value.get("success") is True, "consumer_read_item_failed")
                file = value.get("output", {})
                path = file.get("path", value.get("path"))
                require(path in self.expected_files and file.get("format") != "numbered",
                        "consumer_unexpected_read_range")
                require(file.get("start_line") == 1 and not file.get("has_more"),
                        "consumer_incomplete_file_read")
                require(file.get("text") == self.expected_files[path], "consumer_tool_result_mismatch")
                observed[path] = file["text"]
                files.append({"path": path, "read_revision": file.get("read_revision"),
                              "text": file["text"], "sha256": sha256(self.fixture / path)})
            call_evidence.append({"native_event_id": item.get("id"), "status": item["status"],
                                  "tool": "read_files", "project": self.project, "files": files})
            self.report["consumer"]["tool_calls"] = call_evidence
            self.checkpoint("consumer_tool_receipt_verified")
        require(set(observed) == set(self.expected_files), "consumer_missing_file_evidence")
        messages = [item.get("text", "") for item in completed if item.get("type") == "agent_message"]
        require(bool(messages), "consumer_final_message_missing")
        final = messages[-1].strip()
        if final.startswith(chr(96) * 3) and final.endswith(chr(96) * 3):
            final = "\n".join(final.splitlines()[1:-1])
        values = json.loads(final)
        expected = dict(line.split("=", 1) for text in self.expected_files.values()
                        for line in text.splitlines() if "=" in line)
        require(values == expected, "consumer_report_mismatch")
        status = subprocess.check_output(["git", "status", "--porcelain"], cwd=self.fixture, env=self.env)
        require(not status, "consumer_changed_fixture")
        counts = self.no_lifecycle()
        self.report["consumer"] |= {
            "passed": True, "scopes": CONSUMER_SCOPES, "tool_calls": call_evidence,
            "model_report": values, "native_file_comparison": True,
            "lifecycle_counts_after": counts, "fixture_unchanged": True,
        }
        self.checkpoint("consumer_accepted")

    def native_tests(self, pattern: str) -> dict:
        result = subprocess.run(
            [sys.executable, "-I", "-m", "unittest", "discover", "-s", ".", "-p", pattern],
            cwd=self.fixture, env=self.env, stdin=subprocess.DEVNULL,
            capture_output=True, timeout=30, check=False)
        require(len(result.stdout) + len(result.stderr) < 1024 * 1024, "native_test_output_limit")
        match = re.search(rb"Ran (\d+) tests?", result.stderr)
        require(match is not None, "native_tests_missing_count")
        return {"exit_code": result.returncode, "tests_run": int(match.group(1)),
                "output_sha256": hashlib.sha256(result.stdout + result.stderr).hexdigest()}

    def observe_before_restart(self, run_id: str) -> tuple[dict, list[str], Counter]:
        """Wait for a dispatched live Run, or retain the narrower observed case."""
        deadline = time.monotonic() + self.args.timeout
        token = None
        states, kinds = [], Counter()
        while True:
            require(time.monotonic() < deadline, "recovery_observation_deadline")
            require(self.runner.poll() is None, "runner_exited_before_restart")
            request = {"run_id": run_id, "wait_secs": 1}
            if token:
                request["after_observation_token"] = token
            observation = self.tool("observe_coding_agent", request, self.operator)
            require(observation.get("run_id") == run_id, "recovery_observed_wrong_run")
            state, execution = observation.get("state"), observation.get("execution_state")
            require(state in ("starting", "running", "waiting_permission", *TERMINAL)
                    and execution in ("not_started", "started", "completed", "outcome_unknown"),
                    "recovery_execution_state_missing")
            require(observation.get("history_lost") is False, "managed_event_history_lost")
            if not states or states[-1] != state:
                states.append(state)
            kinds.update(event["kind"] for event in observation.get("events", []))
            self.report["managed"].update(states=states, event_kinds=dict(kinds), last_run_state=state)
            self.report["recovery"].update(
                run_state_before_disconnect=state, execution_state_before_disconnect=execution)
            self.checkpoint("observing_before_server_restart")
            require(time.monotonic() < deadline, "recovery_observation_deadline")
            # Run state alone is insufficient: running may precede the durable
            # prompt-write acknowledgement. The canonical execution state owns it.
            if execution == "outcome_unknown":
                phase = "outcome_unknown"
            elif state in TERMINAL:
                phase = "before_prompt" if execution == "not_started" else "terminal"
            elif state == "running" and execution == "started":
                phase = "in_flight"
            elif state == "waiting_permission":
                phase = "waiting_permission"
            else:
                token = observation.get("observation_token")
                require(isinstance(token, str) and bool(token), "recovery_observation_token_missing")
                continue
            self.report["recovery"]["phase_before_disconnect"] = phase
            if phase != "in_flight":
                self.report["recovery"]["failure"] = "in_flight_restart_not_observed"
            self.checkpoint("before_server_restart")
            return observation, states, kinds

    def require_original_binding(self, binding: dict) -> None:
        require(all(binding.get(key) == self.report["managed"][key]
                    for key in ("task_id", "attempt_id", "run_id")),
                "recovery_changed_execution_identity")

    def managed(self) -> None:
        baseline = self.native_tests("test_clamp.py")
        require(baseline["exit_code"] != 0 and baseline["tests_run"] == 7, "baseline_should_fail")
        agent = self.tool("create_agent_identity", {
            "handle": "codex-acceptance", "display_name": "Codex acceptance executor",
            "idempotency_key": "codex-bidirectional-agent",
        }, self.operator)["agent"]
        instruction = (
            "Implement clamp(value, lower, upper) in clamp.py: return value bounded inclusively "
            "to lower/upper, and raise ValueError if lower > upper. Read test_clamp.py and leave "
            "that file unchanged. Add a separate small unittest test file for one additional case, "
            "run the tests, and briefly report the result. Work only in this disposable Git project. "
            "Use Python's standard library; do not install dependencies, access network or credentials, "
            "request elevated permissions, delegate work, or create WebCodex lifecycle objects. "
            "Do not change any other pre-existing file."
        )
        task = self.tool("create_agent_task", {
            "title": "Implement and verify Python clamp", "instruction": instruction,
            "assignee_agent_id": agent["agent_id"], "referenced_project_id": self.project,
            "idempotency_key": "codex-bidirectional-clamp",
        }, self.operator)["task"]["summary"]
        attempt = self.tool("start_agent_task_attempt", {
            "task_id": task["task_id"], "assignee_agent_id": agent["agent_id"],
            "idempotency_key": "codex-bidirectional-clamp-attempt",
        }, self.operator)
        attempt_id = attempt["attempt"]["attempt_id"]
        self.report["managed"] |= {
            "agent_id": agent["agent_id"], "task_id": task["task_id"], "attempt_id": attempt_id,
            "attempt_ref": attempt["attempt_ref"],
            "attempt_controller_generation": attempt["attempt"]["attempt_controller_generation"],
            "baseline_expected_failure": baseline,
        }
        private_json(self.private / "attempt.json", attempt)
        self.checkpoint("attempt_claimed")
        params = {"project": self.project, "provider_id": "codex",
                  "attempt_ref": attempt["attempt_ref"], "timeout_secs": self.args.timeout}
        self.binding = self.tool("start_agent_task_coding_run", params, self.operator)
        run_id = self.binding["run_id"]
        self.report["managed"]["run_id"] = run_id
        self.report["managed"]["initial_binding"] = {k: self.binding[k] for k in BINDING_FIELDS
                                                     if k in self.binding}
        self.checkpoint("coding_run_bound")
        # Exact-bound replay is intentional acceptance; actual prompt count is
        # checked after completion, rather than treating Run ID equality as proof.
        replay = self.tool("start_agent_task_coding_run", params, self.operator)
        require(replay.get("run_id") == run_id, "binding_replay_changed_run")
        self.report["recovery"]["exact_binding_replay_same_run"] = True
        observation, states, kinds = self.observe_before_restart(run_id)
        terminal = observation.get("terminal") or {}
        self.report["managed"]["last_terminal"] = {
            key: terminal[key] for key in ("stop_reason", "error_code") if key in terminal}
        old_server, runner_pid = self.server.pid, self.runner.pid
        stop(self.server)
        require(self.runner.poll() is None and self.runner.pid == runner_pid,
                "runner_did_not_survive_server_stop")
        self.start_server()
        self.wait_ready(self.project, self.operator)
        self.initialize(self.operator)
        self.report["recovery"] |= {
            "old_server_pid": old_server, "new_server_pid": self.server.pid,
            "preserved_runner_pid": runner_pid, "run_state_before_disconnect": states[-1],
            "same_server_data": str(self.root / "data"), "new_server_process": self.server.pid != old_server,
        }
        recovered_task = self.tool("read_agent_task", {"task_id": task["task_id"]}, self.operator)
        require(recovered_task["task"]["summary"]["task_id"] == task["task_id"],
                "server_restart_changed_task")
        recovered = self.tool("reconcile_agent_task_coding_run", {
            "task_id": task["task_id"], "attempt_id": attempt_id,
        }, self.operator)
        self.require_original_binding(recovered)
        self.checkpoint("server_recovered_original_run")
        deadline = time.monotonic() + self.args.timeout + 30
        token = None  # A new Server process may reset public observation tokens.
        while True:
            request = {"run_id": run_id, "wait_secs": 10}
            if token:
                request["after_observation_token"] = token
            observation = self.tool("observe_coding_agent", request, self.operator)
            require(observation.get("run_id") == run_id, "recovery_observed_wrong_run")
            token = observation.get("observation_token")
            if not states or states[-1] != observation["state"]:
                states.append(observation["state"])
            kinds.update(event["kind"] for event in observation.get("events", []))
            self.report["managed"]["event_kinds"] = dict(kinds)
            self.report["managed"]["last_run_state"] = observation["state"]
            self.checkpoint("observing_original_run")
            require(not observation.get("history_lost"), "managed_event_history_lost")
            if observation["state"] in TERMINAL and not observation.get("has_more"):
                break
            require(time.monotonic() < deadline, "managed_observation_deadline")
            require(self.runner.poll() is None, "runner_exited_during_run")
        reconciled = self.tool("reconcile_agent_task_coding_run", {
            "task_id": task["task_id"], "attempt_id": attempt_id,
        }, self.operator)
        self.require_original_binding(reconciled)
        self.report["managed"]["terminal_binding"] = {
            k: reconciled[k] for k in BINDING_FIELDS if k in reconciled
        }
        require(observation["state"] == "completed"
                and reconciled.get("run_id") == run_id
                and reconciled.get("task_state") == "succeeded"
                and (observation.get("terminal") or {}).get("stop_reason") == "end_turn",
                "managed_turn_not_completed")
        require(kinds["agent_message"] > 0 and any(kinds[key] > 0 for key in
                ("tool_activity", "file_change", "terminal_activity")),
                "managed_normalized_activity_missing")
        require(kinds["permission_request"] == 0, "managed_permission_required")
        audit = bounded_jsonl(self.audit, 1024 * 1024)
        prompts = [event for event in audit if event["event"] == "prompt_dispatch"]
        processes = [event for event in audit if event["event"] == "adapter_process_started"]
        require(len(prompts) == 1 and len(processes) == 1, "managed_prompt_redispatched")
        require(prompts[0].get("effective_config") == {
            "model": MODEL, "reasoning_effort": EFFORT}, "actual_forced_model_not_observed")
        identities = [event for event in audit if event["event"] == "adapter_identity"]
        require(any(event.get("version") == ADAPTER_VERSION for event in identities),
                "live_adapter_version_mismatch")
        fixed_unchanged = sha256(self.fixture / "test_clamp.py") == self.fixed_hash
        require(fixed_unchanged, "agent_modified_fixed_acceptance")
        extra_tests = sorted(path.name for path in self.fixture.glob("test*.py")
                             if path.name != "test_clamp.py")
        require(extra_tests, "agent_did_not_add_independent_case")
        implementation_hash = sha256(self.fixture / "clamp.py")
        fixed_tests = self.native_tests("test_clamp.py")
        require(fixed_tests["exit_code"] == 0 and fixed_tests["tests_run"] == 7,
                "independent_fixed_acceptance_failed")
        self.report["managed"]["fixed_acceptance_tests"] = fixed_tests
        final_tests = self.native_tests("test*.py")
        require(final_tests["exit_code"] == 0 and final_tests["tests_run"] > 7,
                "additional_tests_failed")
        require(sha256(self.fixture / "test_clamp.py") == self.fixed_hash
                and sha256(self.fixture / "clamp.py") == implementation_hash,
                "tests_mutated_acceptance_source")
        changed = subprocess.check_output(
            ["git", "diff", "HEAD", "--name-only"], cwd=self.fixture, env=self.env, text=True).splitlines()
        require(changed == ["clamp.py"], "agent_changed_unrequested_tracked_files")
        diff = subprocess.check_output(["git", "diff", "HEAD", "--", "clamp.py"],
                                       cwd=self.fixture, env=self.env)
        (self.root / "implementation.diff").write_bytes(diff)
        self.report["managed"] |= {
            "passed": True, "fixed_tests_unchanged": fixed_unchanged,
            "fixed_acceptance_tests": fixed_tests, "all_tests": final_tests,
            "additional_tests": extra_tests, "implementation_sha256": implementation_hash,
            "changed_tracked_files": changed, "implementation_diff_sha256": hashlib.sha256(diff).hexdigest(),
            "model_configuration_observed": {"model": MODEL, "reasoning_effort": EFFORT},
            "adapter_identity": [{"name": event["name"], "version": event["version"]} for event in identities],
            "execution_completion": True, "business_acceptance": True,
        }
        self.report["recovery"] |= {
            "passed": self.report["recovery"]["phase_before_disconnect"] == "in_flight",
            "same_task_attempt_run": True, "adapter_processes": len(processes),
            "actual_prompt_dispatches": len(prompts), "recovery_layer": "real-server-process-and-runner-acp",
            "lease_expiry_tested": False,
        }
        self.checkpoint("managed_and_recovery_accepted" if self.report["recovery"]["passed"]
                        else "managed_accepted_recovery_incomplete")

    def run(self) -> None:
        self.report["binaries"] = {name: {
            "path": str(self.bin_dir / name), "version": process_version(self.bin_dir / name),
            "sha256": sha256(self.bin_dir / name),
        } for name in ("webcodex-server", "webcodex-runner")}
        self.report["codex"] = {"path": str(self.args.codex_path),
                                "version": process_version(self.args.codex_path)}
        metadata = json.loads(self.args.adapter_script.parent.parent.joinpath("package.json").read_text())
        require(metadata.get("name") == "@agentclientprotocol/codex-acp"
                and metadata.get("version") == ADAPTER_VERSION, "adapter_package_mismatch")
        self.report["adapter"] = {"name": metadata["name"], "version": metadata["version"],
                                   "script": str(self.args.adapter_script),
                                   "script_sha256": sha256(self.args.adapter_script)}
        self.fixtures()
        self.provision()
        for name in ("consumer", "managed"):
            try:
                getattr(self, name)()
            except Exception as error:
                self.report[name]["failure"] = (
                    str(error) if isinstance(error, AcceptanceFailure) else type(error).__name__)
                self.checkpoint(name + "_failed")
        passed = all(self.report[name]["passed"] for name in ("consumer", "managed", "recovery"))
        self.report["state"] = "passed" if passed else "failed"
        self.checkpoint("accepted" if passed else "acceptance_incomplete")

    def close(self) -> None:
        if not self.report["cleanup"].get("consumer_group_stopped"):
            self.report["cleanup"]["consumer_group_stopped"] = stop_group(self.consumer_process)
        if self.report["managed"].get("attempt_id") and self.server and self.server.poll() is None:
            try:
                recovered = self.tool("reconcile_agent_task_coding_run", {
                    key: self.report["managed"][key] for key in ("task_id", "attempt_id")
                }, self.operator)
                if recovered.get("run_id"):
                    self.binding = recovered
                    self.report["managed"]["run_id"] = recovered["run_id"]
            except Exception:
                self.report["cleanup"]["binding_reconcile"] = "unavailable"
        if self.binding and self.server and self.server.poll() is None:
            run_id = self.binding.get("run_id")
            try:
                observed = self.tool("observe_coding_agent", {
                    "run_id": run_id, "wait_secs": 0}, self.operator)
                if observed["state"] not in TERMINAL:
                    self.tool("cancel_coding_agent", {"run_id": run_id}, self.operator)
                    self.report["cleanup"]["cancel_requested_for"] = run_id
                    deadline = time.monotonic() + 20
                    while time.monotonic() < deadline:
                        observed = self.tool("observe_coding_agent", {
                            "run_id": run_id, "wait_secs": 5}, self.operator)
                        if observed["state"] in TERMINAL:
                            break
                self.report["cleanup"]["execution_terminal_observed"] = (
                    observed["state"] in {"completed", "failed", "cancelled"})
                self.report["cleanup"]["execution_outcome"] = (
                    "outcome_unknown" if observed["state"] == "lost" else observed["state"])
            except Exception:
                self.report["cleanup"]["execution_terminal_observed"] = False
                self.report["cleanup"]["execution_outcome"] = "outcome_unknown"
        super().close()
        self.report["cleanup"]["service_logs"] = [
            summarize_and_remove_service_log(Path(log.name)) for log in self.logs
        ]
        if hasattr(self, "audit") and self.audit.exists():
            try:
                audit = bounded_jsonl(self.audit, 1024 * 1024)
                self.report["managed"]["acp_audit"] = {
                    "adapter_processes": sum(event["event"] == "adapter_process_started" for event in audit),
                    "prompt_dispatches": sum(event["event"] == "prompt_dispatch" for event in audit),
                    "effective_configs_at_prompt": [event.get("effective_config") for event in audit
                                                    if event["event"] == "prompt_dispatch"],
                    "diagnostic_categories": dict(Counter(event["category"] for event in audit
                                                         if event["event"] == "diagnostic")),
                }
            except Exception:
                self.report["managed"]["acp_audit"] = {"observation": "unavailable"}
        self.report["cleanup"] |= {
            "process_cleanup_finished": True,
            "server_stopped": self.server is None or self.server.poll() is not None,
            "runner_stopped": self.runner is None or self.runner.poll() is not None,
            "consumer_stopped": self.consumer_process is None or self.consumer_process.poll() is not None,
            "private_recovery_state_retained": True,
        }
        self.report["finished_at_unix"] = time.time()
        self.checkpoint("closed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    parser.add_argument("--adapter-script", type=Path, required=True)
    parser.add_argument("--codex-path", type=Path, required=True)
    parser.add_argument("--node-path", type=Path, default=Path(shutil.which("node") or "node"))
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--host-job-id", help="Optional already-known outer Job id; never inferred from a PID")
    args = parser.parse_args()
    require(os.name == "posix", "posix_test_harness_required")
    require(sys.version_info >= (3, 10), "python_310_required")
    require(30 <= args.timeout <= 900, "invalid_timeout")
    for name in ("bin_dir", "adapter_script", "codex_path", "node_path"):
        setattr(args, name, getattr(args, name).resolve(strict=True))
    require(args.adapter_script.is_file(), "adapter_script_required")
    args.output_dir = args.output_dir.absolute()
    args.output_dir.mkdir(mode=0o700)
    previous_umask = os.umask(0o077)
    smoke = None
    try:
        smoke = BidirectionalSmoke(args)
        try:
            smoke.run()
        except BaseException as error:
            smoke.report["state"] = "failed"
            smoke.report["failure"] = str(error) if isinstance(error, AcceptanceFailure) else type(error).__name__
            smoke.checkpoint("failed")
        finally:
            smoke.close()
        print(json.dumps(smoke.report, ensure_ascii=False))
        return 0 if smoke.report["state"] == "passed" else 1
    finally:
        os.umask(previous_umask)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(json.dumps({"state": "failed", "failure": str(error)
                          if isinstance(error, AcceptanceFailure) else type(error).__name__}))
        sys.exit(1)
