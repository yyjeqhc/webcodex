#!/usr/bin/env python3
"""Opt-in real Codex/Claude -> local MCP Server/Runner acceptance.

Uses the caller's authenticated model accounts (paid requests), low effort, and
process-local MCP configuration. Services, credentials, repositories and logs
live in a new private directory; no shared service or client config is changed.
Build with the dogfood profile first. Reported tokens are client observations,
not estimates from JSON bytes. A model's success claim is independently checked.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from urllib.request import Request

from e2e_session_continuity_ws import ROOT, Smoke, check

SOURCE = '''def clamp(value, lower, upper):
    """Clamp inclusively; reject reversed bounds."""
    return min(value, upper)

SENTINEL = "preserve-me"
'''
TESTS = '''import unittest
from ranges import clamp, SENTINEL
class ClampTest(unittest.TestCase):
    def test_bounds(self):
        for value, lo, hi, expected in [(-3, 0, 5, 0), (9, 0, 5, 5), (2, 0, 5, 2), (-9, -4, -1, -4), (3, 3, 3, 3)]:
            self.assertEqual(clamp(value, lo, hi), expected)
    def test_reversed(self):
        with self.assertRaises(ValueError): clamp(2, 5, 0)
    def test_sentinel(self): self.assertEqual(SENTINEL, "preserve-me")
if __name__ == "__main__": unittest.main()
'''
LONG_CHECK = '''from pathlib import Path
import time
with Path("executions.txt").open("a") as log:
    log.write("started\\n")
    log.flush()
time.sleep(14)
Path("long-result.txt").write_text("LONG_CHECK_OK\\n")
print("LONG_CHECK_OK", flush=True)
'''
PROMPT = (
    "Work on the remote Project configured on the webcodex MCP server; your local cwd is unrelated. "
    "Fix ranges.py so clamp handles both inclusive bounds and raises ValueError when lower > upper. "
    "Preserve SENTINEL. Read the repository instructions and run the unit tests. "
    "Then run python3 long_check.py exactly once, wait for its completion and report its output. "
    "Do not modify the tests or long_check.py. Do not delegate. "
    "Finish with a brief result and candid feedback on friction or redundant tool interactions."
)


def private_json(path: Path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    path.chmod(0o600)


class Acceptance(Smoke):
    def prepare(self):
        # Model credentials must never reach the execution service's environment.
        self.env = {k: v for k, v in self.env.items()
                    if not any(word in k.upper() for word in ("TOKEN", "SECRET", "API_KEY", "PASSWORD"))}
        self.env.update({"WEBCODEX_ENV_FILE": os.devnull,
                         "WEBCODEX_MCP_HOST_PROFILE": "direct"})
        self.start_server()
        check(self.post("/api/users/create", {"username": "local-agent"}, self.bootstrap).get("success"),
              "fixture user creation failed")
        self.owner = self.pat("local-agent", ["runtime:read", "project:read", "project:write", "job:run"])
        enrollment = self.post("/api/agent-tokens/create", {
            "username": "local-agent", "client_id": "local-agent-runner", "name": "fixture"
        }, self.bootstrap)
        self.project_dir = self.root / "remote-project"
        self.project_dir.mkdir()
        self.client_dir = self.root / "client-cwd"
        self.client_dir.mkdir()
        registry = self.root / "projects"
        registry.mkdir()
        (registry / "fixture.toml").write_text(f'id = "fixture"\npath = {json.dumps(str(self.project_dir))}\n')
        config = self.root / "runner.toml"
        config.write_text("\n".join([
            f'server_url = "{self.url}"', f'token = {json.dumps(enrollment["token"])}',
            'client_id = "local-agent-runner"', 'owner = "local-agent"', 'transport = "websocket"',
            f'project_registry_dir = {json.dumps(str(registry))}', '[policy]', 'allow_raw_shell = true',
            f'allowed_roots = [{json.dumps(str(self.project_dir))}]',
        ]) + "\n")
        config.chmod(0o600)
        self.runner = self.launch("webcodex-runner", ["--config", str(config)], self.env)
        self.project = "agent:local-agent-runner:fixture"
        self.wait_ready(self.project, self.owner)
        self.headers = {"X-WebCodex-MCP-Surface": "cli", "X-WebCodex-MCP-Profile": "direct",
                        "X-WebCodex-MCP-Project": self.project}

    def rpc(self, method, params):
        request = Request(self.url + "/mcp", json.dumps({
            "jsonrpc": "2.0", "id": 1, "method": method, "params": params
        }).encode(), headers=self.headers | {"Authorization": "Bearer " + self.owner,
            "Content-Type": "application/json", "Accept": "application/json, text/event-stream"})
        with self.opener.open(request, timeout=60) as response:
            return json.load(response)

    def reset_fixture(self):
        for file in self.project_dir.iterdir():
            if file.is_dir(): shutil.rmtree(file)
            else: file.unlink()
        for name, text in {"ranges.py": SOURCE, "test_ranges.py": TESTS,
                           "long_check.py": LONG_CHECK,
                           "AGENTS.md": "This disposable repository uses Python unittest. Preserve SENTINEL.\n"}.items():
            (self.project_dir / name).write_text(text)
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=self.project_dir,
                       env=self.env, stdin=subprocess.DEVNULL, check=True)

    def run_client(self, client, args):
        self.reset_fixture()
        env = dict(os.environ) | {"WEBCODEX_LOCAL_TEST_TOKEN": self.owner}
        if client == "codex":
            server = '{url=' + json.dumps(self.url + "/mcp") + ',bearer_token_env_var="WEBCODEX_LOCAL_TEST_TOKEN",required=true,tool_timeout_sec=65,tools={edit_project_files={approval_mode="approve"},run_shell={approval_mode="approve"},stop_job={approval_mode="approve"}},http_headers={'
            server += ",".join(json.dumps(k) + "=" + json.dumps(v) for k, v in self.headers.items()) + '}}'
            command = [args.codex_path, "exec", "--ignore-user-config", "--strict-config",
                       "--sandbox", "read-only", "--ephemeral", "--skip-git-repo-check", "--json",
                       "--model", args.codex_model, "--config", 'model_reasoning_effort="low"',
                       "--config", "mcp_servers={webcodex=" + server + "}",
                       "--cd", str(self.client_dir), "-"]
        else:
            config = self.root / "claude-mcp.json"
            private_json(config, {"mcpServers": {"webcodex": {"type": "http", "url": self.url + "/mcp",
                "headers": self.headers | {"Authorization": "Bearer ${WEBCODEX_LOCAL_TEST_TOKEN}"}}}})
            command = [args.claude_path, "-p", "--model", args.claude_model, "--effort", "low",
                       "--strict-mcp-config", "--mcp-config", str(config), "--setting-sources", "",
                       "--permission-mode", "dontAsk", "--allowedTools", "mcp__webcodex__*",
                       "--no-session-persistence", "--output-format", "stream-json", "--verbose"]
        stdout, stderr = self.root / f"{client}.jsonl", self.root / f"{client}.stderr"
        start = time.monotonic()
        with stdout.open("wb") as output, stderr.open("wb") as errors:
            process = subprocess.Popen(command, cwd=self.client_dir, env=env, stdin=subprocess.PIPE,
                                       stdout=output, stderr=errors, start_new_session=True)
            try:
                process.stdin.write(PROMPT.encode())
                process.stdin.close()
                while process.poll() is None:
                    check(time.monotonic() - start < args.timeout, "model deadline exceeded")
                    check(stdout.stat().st_size < 8 * 1024 * 1024 and stderr.stat().st_size < 2 * 1024 * 1024,
                          "model output exceeded capture bound")
                    time.sleep(0.2)
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try: process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
        check(process.returncode == 0, f"{client} exited unsuccessfully; inspect private diagnostics")
        frames = [json.loads(line) for line in stdout.read_text().splitlines() if line.strip()]
        calls, feedback, usage, native_discovery = [], [], [], []
        if client == "codex":
            for frame in frames:
                item = frame.get("item", {})
                if frame.get("type") == "item.completed":
                    if item.get("type") == "mcp_tool_call": calls.append(item.get("tool", "unknown"))
                    if item.get("type") == "agent_message": feedback.append(item.get("text", ""))
                    check(item.get("type") != "command_execution", "model used local command execution")
                if frame.get("type") == "turn.completed": usage.append(frame.get("usage", {}))
        else:
            for frame in frames:
                if frame.get("type") == "assistant":
                    for block in frame.get("message", {}).get("content", []):
                        if block.get("type") == "tool_use":
                            name = block.get("name", "")
                            if name == "ToolSearch":
                                native_discovery.append(name)
                            else:
                                check(name.startswith("mcp__webcodex__"), "model used a non-fixture execution tool")
                                calls.append(name.removeprefix("mcp__webcodex__"))
                if frame.get("type") == "result":
                    feedback.append(frame.get("result", ""))
                    usage.append(frame.get("usage", {}))
        print(json.dumps({"client": client, "observed_calls": dict(Counter(calls)),
                          "native_discovery": dict(Counter(native_discovery))}), flush=True)
        check(calls and "observe_jobs" in calls, "model did not observe the asynchronous Job")
        check(not set(calls) & {"work_on_project", "run_process", "run_script"}, "unexpected default workflow")
        check((self.project_dir / "test_ranges.py").read_text() == TESTS, "model changed fixture tests")
        check((self.project_dir / "long_check.py").read_text() == LONG_CHECK, "model changed long check")
        check((self.project_dir / "executions.txt").read_text() == "started\n", "long command replayed")
        check((self.project_dir / "long-result.txt").read_text() == "LONG_CHECK_OK\n", "long command incomplete")
        result = subprocess.run([sys.executable, "-m", "unittest", "-v"], cwd=self.project_dir,
                                env=self.env, stdin=subprocess.DEVNULL, capture_output=True, timeout=20)
        check(result.returncode == 0, "independent tests failed")
        sessions = self.post("/api/tools/call", {"tool": "list_sessions", "params": {"project": self.project}}, self.bootstrap)
        check(sessions.get("success") and sessions["output"]["total"] == 0, "unexpected Workflow Session created")
        report = {"client": client, "model": args.codex_model if client == "codex" else args.claude_model,
                  "effort": "low", "elapsed_secs": round(time.monotonic() - start, 2),
                  "tool_calls": dict(Counter(calls)), "total_calls": len(calls), "usage": usage,
                  "native_discovery": dict(Counter(native_discovery)), "workflow_sessions": 0,
                  "feedback": "\n".join(feedback)[-8000:], "independent_tests_passed": True,
                  "long_command_executions": 1}
        serialized = json.dumps(report)
        for secret in [self.owner, self.bootstrap, *[v for k, v in os.environ.items()
                       if any(word in k.upper() for word in ("TOKEN", "SECRET", "API_KEY")) and len(v) > 10]]:
            check(secret not in serialized, "credential appeared in report")
        return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--clients", nargs="+", choices=["codex", "claude"], default=["codex", "claude"])
    parser.add_argument("--codex-path", default="codex")
    parser.add_argument("--claude-path", default="claude")
    parser.add_argument("--codex-model", default="gpt-6-astra")
    parser.add_argument("--claude-model", default="opus")
    parser.add_argument("--timeout", type=int, default=360)
    args = parser.parse_args()
    args.output_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    report = {"source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
              "source_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD", "--", "src", "crates"], cwd=ROOT)).hexdigest(),
              "client_versions": {name: subprocess.check_output([executable, "--version"], text=True).strip()
                                  for name, executable in [("codex", args.codex_path), ("claude", args.claude_path)] if name in args.clients},
              "binaries": {name: hashlib.sha256((args.bin_dir / name).read_bytes()).hexdigest()
                           for name in ("webcodex-server", "webcodex-runner")}, "clients": []}
    with tempfile.TemporaryDirectory(prefix="private-", dir=args.output_dir) as private:
        smoke = Acceptance(args.bin_dir.resolve(), Path(private))
        try:
            smoke.prepare()
            inventory = smoke.rpc("tools/list", {})["result"]
            report["discovery"] = {"tools": [tool["name"] for tool in inventory["tools"]],
                                   "json_bytes": len(json.dumps(inventory, separators=(",", ":")).encode())}
            for client in args.clients:
                report["clients"].append(smoke.run_client(client, args))
                private_json(args.output_dir / "report.json", report)
                print(json.dumps(report["clients"][-1], ensure_ascii=False), flush=True)
        finally:
            smoke.close()
            report["services_stopped"] = all(p is None or p.poll() is not None for p in [smoke.server, smoke.runner])
            private_json(args.output_dir / "report.json", report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
