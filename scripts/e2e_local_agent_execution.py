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
import re
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
from urllib.request import Request
from urllib.error import HTTPError

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
time.sleep(16)
Path("long-result.txt").write_text("LONG_CHECK_OK\\n")
print("LONG_CHECK_OK", flush=True)
'''
PROMPT = (
    "Work on the remote Project configured on the webcodex MCP server; your local cwd is unrelated. "
    "Fix ranges.py so clamp handles both inclusive bounds and raises ValueError when lower > upper. "
    "First run python3 noisy_check.py to reproduce the failure, then fix the code and run the unit tests. "
    "Then run python3 long_check.py exactly once, wait for its completion and report its output. "
    "Do not modify the tests, noisy_check.py or long_check.py. Do not delegate. "
    "Finish with a brief result and candid feedback on friction or redundant tool interactions."
)

NOISY_CHECK = """from ranges import clamp
if clamp(-3, 0, 5) != 0:
    print("FIRST_ERROR: lower bound is not enforced", flush=True)
    for i in range(500): print(f"build detail {i}")
    raise SystemExit(3)
print("NOISY_CHECK_OK")
"""
INSTRUCTIONS = ("This disposable repository uses Python unittest. Preserve SENTINEL. "
                "After changing ranges.py, write instructions-read.txt containing REMOTE_RULE_OK and a newline.\n")


def response_text(response):
    return "\n".join(block.get("text", "") for block in response.get("result", {}).get("content", []) if block.get("type") == "text")


def next_calls(text):
    decoder = json.JSONDecoder()
    for line in text.splitlines():
        if not line.startswith("next: "): continue
        tool, _, rest = line[6:].partition(" ")
        arguments = {}
        while rest.strip():
            key, _, value = rest.lstrip().partition("=")
            parsed, used = decoder.raw_decode(value)
            arguments[key] = parsed
            rest = value[used:]
        yield tool, arguments


def inline_toml(value):
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k) + "=" + inline_toml(v) for k, v in value.items()) + "}"
    return json.dumps(value)


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

    def probe_native_contracts(self):
        self.reset_fixture()
        def call(tool, arguments):
            response = self.rpc("tools/call", {"name":tool, "arguments":arguments})
            check("error" not in response, f"{tool}: RPC error")
            return response, response_text(response)
        start = time.monotonic()
        result, text = call("run_shell", {"command":"python3 long_check.py"})
        check(not result["result"].get("isError") and "LONG_CHECK_OK" in text, "16s default wait did not finish")
        check("next: observe_jobs" not in text, "16s command unexpectedly handed off")
        sync_elapsed = time.monotonic() - start
        result, text = call("run_shell", {"command":"printf 'EARLY_OUTPUT\\n'; sleep 4; printf 'ASYNC_DONE\\n'", "sync_wait_secs":1})
        check("EARLY_OUTPUT" in text, "handoff lost initial output")
        tool, arguments = next(next_calls(text))
        job_id = arguments["items"][0]["job_id"]
        start = time.monotonic()
        result, text = call("observe_jobs", {"items":[{"job_id":job_id}], "wait_secs":10})
        wait_elapsed = time.monotonic() - start
        check(wait_elapsed > 1 and "ASYNC_DONE" in text, f"tokenless terminal wait failed after {wait_elapsed:.2f}s: {text[:3000]}")
        result, text = call("run_shell", {"command":"python3 noisy_check.py"})
        check(result["result"]["isError"] and "FIRST_ERROR" in text and "build detail 499" in text, "head/tail omitted first error or final output")
        seen = text
        pages = 0
        pending = list(next_calls(text))
        while pending and pages < 20:
            tool, arguments = pending[0]
            result, text = call(tool, arguments)
            seen += text
            pages += 1
            pending = list(next_calls(text))
        check("build detail 250" in seen and not pending, "retained log paging failed")
        absolute = str(self.project_dir / "ranges.py")
        result, text = call("read_files", {"items":[{"path":absolute}]})
        revision = int(re.search(r"read_revision (\d+)", text).group(1))
        result, text = call("edit_project_files", {"changes":[{"kind":"write", "path":absolute, "expected_read_revision":revision, "content":SOURCE + "# overwritten\n"}]})
        check(not result["result"].get("isError") and "read_revision" in text, "revision-fenced overwrite failed")
        result, text = call("edit_project_files", {"changes":[{"path":absolute, "edits":[{"old_text":"# overwritten", "new_text":"# exact"}]}]})
        check(not result["result"].get("isError"), "revisionless unique exact edit failed")
        result, text = call("edit_project_files", {"changes":[{"kind":"write", "path":absolute, "expected_read_revision":revision, "content":"stale"}]})
        check(result["result"].get("isError"), "stale overwrite unexpectedly accepted")
        check((self.project_dir / "ranges.py").read_text() == SOURCE + "# exact\n", "stale write modified file")
        result, text = call("read_files", {"items":[{"path":str(self.root / "runner.toml")}]})
        check("server_url" not in text, "outside-root read exposed content")
        (self.project_dir / "large.txt").write_text(("large line " + "x" * 120 + "\n") * 500)
        result, text = call("read_files", {"items":[{"path":"large.txt", "limit":1000}]})
        check(not result["result"].get("isError") and len(text) < 50000 and "more lines available" in text,
              "default read budget exceeded client threshold or lost coverage")
        large_read_chars = len(text)
        try:
            self.rpc("tools/call", {"name":"edit_project_files", "arguments":{"changes":[{
                "kind":"write", "path":"ranges.py", "expected_read_revision":"bad",
                "content":"PRIVATE_CONTENT_MUST_NOT_APPEAR"}]}})
            raise AssertionError("invalid revision type accepted")
        except HTTPError as error:
            diagnostic = error.read().decode()
            check(error.code == 400 and "expected_read_revision" in diagnostic
                  and "PRIVATE_CONTENT_MUST_NOT_APPEAR" not in diagnostic,
                  "invalid revision did not identify the field safely")
        result, text = call("run_shell", {"command":"python3 -c 'import time; print(1, flush=True); time.sleep(30)'", "sync_wait_secs":1})
        _, arguments = next(next_calls(text))
        stopped_job = arguments["items"][0]["job_id"]
        result, text = call("stop_job", {"job_id":stopped_job})
        check(not result["result"].get("isError") and "job_session_unknown" not in text,
              "CLI stop required redundant confirmation or emitted a Session warning")
        result, text = call("observe_jobs", {"items":[{"job_id":stopped_job}], "wait_secs":10})
        check("cancelled" in text or "stopped" in text, "stopped process did not reach terminal state")
        return {"sync_16s_elapsed":round(sync_elapsed, 2), "tokenless_wait_elapsed":round(wait_elapsed, 2),
                "retained_log_pages":pages, "absolute_read_and_edit":True, "revision_fenced_overwrite":True,
                "unique_exact_without_revision":True, "stale_write_rejected":True, "outside_path_rejected":True,
                "large_read_chars":large_read_chars, "revision_error_field_path":True,
                "stop_without_confirmation":True}

    def reset_fixture(self):
        for file in self.project_dir.iterdir():
            if file.is_dir(): shutil.rmtree(file)
            else: file.unlink()
        for name, text in {"ranges.py": SOURCE, "test_ranges.py": TESTS,
                           "long_check.py": LONG_CHECK, "noisy_check.py": NOISY_CHECK,
                           "AGENTS.md": INSTRUCTIONS, "CLAUDE.md": INSTRUCTIONS}.items():
            (self.project_dir / name).write_text(text)
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=self.project_dir,
                       env=self.env, stdin=subprocess.DEVNULL, check=True)

    def run_client(self, client, args, native=False):
        self.reset_fixture()
        env = dict(os.environ)
        if not native: env["WEBCODEX_LOCAL_TEST_TOKEN"] = self.owner
        label = client + ("-native" if native else "-mcp")
        cwd = self.project_dir if native else self.client_dir
        prompt = PROMPT if not native else PROMPT.replace(
            "Work on the remote Project configured on the webcodex MCP server; your local cwd is unrelated. ",
            "Work in the current local repository using your native tools. ")
        generated = None
        if not native:
            generator = [str(self.bin_dir / "webcodex"), "mcp-config", "--client", client,
                         "--url", self.url + "/mcp", "--project", self.project,
                         "--token-env", "WEBCODEX_LOCAL_TEST_TOKEN"]
            if client == "codex": generator.append("--approve-execution")
            generated = subprocess.check_output(generator, env=self.env, stdin=subprocess.DEVNULL, text=True)
        if client == "codex":
            servers = {} if native else tomllib.loads(generated)["mcp_servers"]
            command = [args.codex_path, "exec", "--ignore-user-config", "--strict-config",
                       "--sandbox", "danger-full-access" if native else "read-only", "--ephemeral", "--skip-git-repo-check", "--json",
                       "--model", args.codex_model, "--config", 'model_reasoning_effort="low"',
                       "--config", "mcp_servers=" + inline_toml(servers),
                       "--cd", str(cwd), "-"]
        else:
            config = self.root / "claude-mcp.json"
            private_json(config, {"mcpServers": {}} if native else json.loads(generated))
            command = [args.claude_path, "-p", "--model", args.claude_model, "--effort", "low",
                       "--strict-mcp-config", "--mcp-config", str(config), "--setting-sources", "",
                       "--permission-mode", "dontAsk", "--allowedTools", "Bash,Read,Edit,Write,Glob,Grep" if native else "mcp__webcodex__*",
                       "--no-session-persistence", "--output-format", "stream-json", "--verbose"]
        stdout, stderr = self.root / f"{label}.jsonl", self.root / f"{label}.stderr"
        start = time.monotonic()
        with stdout.open("wb") as output, stderr.open("wb") as errors:
            process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE,
                                       stdout=output, stderr=errors, start_new_session=True)
            try:
                process.stdin.write(prompt.encode())
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
                    if native and item.get("type") in {"command_execution", "file_change"}: calls.append(item["type"])
                    if not native: check(item.get("type") != "command_execution", "model used local command execution")
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
                                check(native or name.startswith("mcp__webcodex__"), "model used a non-fixture execution tool")
                                calls.append(name.removeprefix("mcp__webcodex__"))
                if frame.get("type") == "result":
                    feedback.append(frame.get("result", ""))
                    usage.append(frame.get("usage", {}))
        print(json.dumps({"client": client, "observed_calls": dict(Counter(calls)),
                          "native_discovery": dict(Counter(native_discovery))}), flush=True)
        check(calls, "model did not use tools")
        check(not set(calls) & {"work_on_project", "run_process", "run_script"}, "unexpected default workflow")
        check((self.project_dir / "test_ranges.py").read_text() == TESTS, "model changed fixture tests")
        check((self.project_dir / "long_check.py").read_text() == LONG_CHECK, "model changed long check")
        check((self.project_dir / "noisy_check.py").read_text() == NOISY_CHECK, "model changed noisy check")
        instructions_followed = ((self.project_dir / "instructions-read.txt").exists() and
            (self.project_dir / "instructions-read.txt").read_text() == "REMOTE_RULE_OK\n")
        check((self.project_dir / "executions.txt").read_text() == "started\n", "long command replayed")
        check((self.project_dir / "long-result.txt").read_text() == "LONG_CHECK_OK\n", "long command incomplete")
        result = subprocess.run([sys.executable, "-m", "unittest", "-v"], cwd=self.project_dir,
                                env=self.env, stdin=subprocess.DEVNULL, capture_output=True, timeout=20)
        check(result.returncode == 0, "independent tests failed")
        sessions = self.post("/api/tools/call", {"tool": "list_sessions", "params": {"project": self.project}}, self.bootstrap)
        check(sessions.get("success") and sessions["output"]["total"] == 0, "unexpected Workflow Session created")
        report = {"client": client, "mode": "native" if native else "mcp", "instructions_followed": instructions_followed,
                  "model": args.codex_model if client == "codex" else args.claude_model,
                  "effort": "low", "elapsed_secs": round(time.monotonic() - start, 2),
                  "tool_calls": dict(Counter(calls)), "total_calls": len(calls), "usage": usage,
                  "call_metric": "completed_action_events; native stdin polls are not separately emitted" if client == "codex" else "tool_use_blocks",
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
    parser.add_argument("--probe-only", action="store_true")
    parser.add_argument("--timeout", type=int, default=360)
    args = parser.parse_args()
    args.output_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
    report = {"source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
              "source_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD", "--", "src", "crates"], cwd=ROOT)).hexdigest(),
              "client_versions": {name: subprocess.check_output([executable, "--version"], text=True).strip()
                                  for name, executable in [("codex", args.codex_path), ("claude", args.claude_path)] if name in args.clients},
              "binaries": {name: hashlib.sha256((args.bin_dir / name).read_bytes()).hexdigest()
                           for name in ("webcodex-server", "webcodex-runner", "webcodex")}, "clients": []}
    with tempfile.TemporaryDirectory(prefix="private-", dir=args.output_dir) as private:
        smoke = Acceptance(args.bin_dir.resolve(), Path(private))
        try:
            smoke.prepare()
            inventory = smoke.rpc("tools/list", {})["result"]
            report["discovery"] = {"tools": [tool["name"] for tool in inventory["tools"]],
                                   "json_bytes": len(json.dumps(inventory, separators=(",", ":")).encode()),
                                   "input_schema_bytes": {tool["name"]:len(json.dumps(tool["inputSchema"], separators=(",", ":")).encode()) for tool in inventory["tools"]}}
            report["probes"] = smoke.probe_native_contracts()
            for client in ([] if args.probe_only else args.clients):
                for native in [False, True]:
                    report["clients"].append(smoke.run_client(client, args, native))
                    private_json(args.output_dir / "report.json", report)
                    print(json.dumps(report["clients"][-1], ensure_ascii=False), flush=True)
        finally:
            smoke.close()
            report["services_stopped"] = all(p is None or p.poll() is not None for p in [smoke.server, smoke.runner])
            private_json(args.output_dir / "report.json", report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
