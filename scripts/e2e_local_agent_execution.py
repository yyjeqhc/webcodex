#!/usr/bin/env python3
"""Opt-in real Codex/Claude -> local MCP Server/Runner acceptance.

Without --probe-only, uses the caller's authenticated model accounts (paid
requests), low effort, and process-local MCP configuration. --probe-only runs
Server/Runner checks and installed clients against a loopback canned provider. Services, credentials, repositories and logs
live in a new private directory; no shared service or client config is changed.
Build with the dogfood profile first. Reported tokens are client observations,
not estimates from JSON bytes. A model's success claim is independently checked.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
from urllib.request import Request
from urllib.error import HTTPError

from e2e_session_continuity_ws import ROOT, Smoke, check
from local_agent_metrics import client_metrics, discovery_metrics
from local_agent_probe import inventory_probe, run_captured

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
        tool_returns = []
        def call(tool, arguments):
            response = self.rpc("tools/call", {"name":tool, "arguments":arguments})
            check("error" not in response, f"{tool}: RPC error")
            text = response_text(response)
            tool_returns.append({"tool":tool, "returned_text_chars":len(text)})
            if list(next_calls(text)):
                check(not re.search(r"^(job_id|observation_token):", text, re.M), "next call duplicates Job selectors")
                check('wake_on="terminal"' not in text, "next call repeats CLI default")
            check(not ("log_delta_status: reset" in text and "_delta_reset: true" in text), "duplicate log reset")
            return response, text
        for tool in ["list_project_files", "list_project_tracked_files", "read_workspace_changes"]:
            _, text = call("read_tool_manifest", {"tool_name":tool})
            check('"mode":"gateway"' in text, f"{tool} lost gateway route")
            check("session_event_limit" not in text, "legacy Session field still advertised")
        for tool in ["list_project_files", "list_project_tracked_files", "list_projects"]:
            result, _ = call("call_runtime_tool", {"tool":tool, "arguments":{}})
            check(not result["result"].get("isError"), f"{tool} unavailable through gateway")
        result, text = call("call_runtime_tool", {"tool":"read_workspace_changes", "arguments":{}})
        check(not result["result"].get("isError") and "?? ranges.py" in text, "workspace status failed")
        check("diff_status:" not in text and "diff_stat_status:" not in text, "unrequested/successful diff looks failed")
        check('"kind":"untracked_file"' not in text, "untracked warning duplicates status")
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
        # Observe once while running, then require the final stdout error on the
        # very next terminal observation, without paging through the burst.
        result, text = call("run_shell", {"command":"printf 'START\\n'; sleep 4; seq 300; printf 'FINAL_STDOUT_FAILURE\\n'; exit 1", "sync_wait_secs":1})
        tool, arguments = next(next_calls(text))
        arguments["wait_secs"] = 1
        result, text = call(tool, arguments)
        check("FINAL_STDOUT_FAILURE" not in text and "next: observe_jobs" in text, "intermediate observation did not remain running")
        tool, arguments = next(next_calls(text))
        arguments["wait_secs"] = 10
        result, text = call(tool, arguments)
        check("FINAL_STDOUT_FAILURE" in text and "exit_code: 1" in text, "terminal incremental tail hid stdout failure")
        incremental_terminal_chars = len(text)
        result, text = call("run_shell", {"command":"printf 'TIMEOUT_START\\n'; sleep 10", "timeout_secs":2})
        check("Effects may be incomplete" in text and "inspect target state" in text and "retained Job" not in text,
              "direct timeout must not invent retained Job recovery")
        result, text = call("run_shell", {"command":"printf 'JOB_TIMEOUT_START\\n'; sleep 10", "timeout_secs":3, "sync_wait_secs":1})
        tool, arguments = next(next_calls(text))
        arguments["wait_secs"] = 10
        result, text = call(tool, arguments)
        check("Effects may be incomplete" in text and ("job_id:" in text or list(next_calls(text))) and "timeout" in text,
              "durable timeout omitted uncertainty or retained Job identity")
        result, text = call("run_shell", {"command":"python3 noisy_check.py"})
        check(result["result"]["isError"] and "FIRST_ERROR" in text and "build detail 499" in text, "head/tail omitted first error or final output")
        seen = text
        pages = 0
        pending = list(next_calls(text))
        check(pending, "truncated sync output lost retained log continuation")
        first_page = True
        while pending and pages < 20:
            tool, arguments = pending[0]
            result, text = call(tool, arguments)
            if first_page:
                check("stdout:\nbuild detail 99\n" in text and "FIRST_ERROR" not in text,
                      "log recovery repeated the displayed preview prefix")
                first_page = False
            seen += text
            pages += 1
            pending = list(next_calls(text))
        check("build detail 250" in seen and not pending and pages == 3, "retained log paging must use three 200-line pages")
        (self.project_dir / "occurrence.txt").write_text("repeat\nrepeat\n")
        _, text = call("read_files", {"items":[{"path":"occurrence.txt"}]})
        fence = int(re.search(r"read_revision (\d+)", text).group(1))
        result, text = call("edit_project_files", {"changes":[{"path":"occurrence.txt", "expected_read_revision":fence,
            "edits":[{"old_text":"repeat", "new_text":"second", "occurrence":2}]}]})
        check(not result["result"].get("isError") and (self.project_dir / "occurrence.txt").read_text() == "repeat\nsecond\n",
              "unadvertised canonical occurrence field did not take effect")
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
        outside = self.root / "outside"
        outside.mkdir()
        (outside / "existing.txt").write_text("OUTSIDE_SECRET_SENTINEL")
        (self.project_dir / "escape").symlink_to(outside, target_is_directory=True)
        result, text = call("read_files", {"items":[{"path":"escape/existing.txt"}]})
        check("outside the Project" in text and "OUTSIDE_SECRET_SENTINEL" not in text, "symlink read did not report project boundary safely")
        result, text = call("edit_project_files", {"changes":[{"kind":"create", "path":"escape/new.txt", "content":"must not write"}]})
        check(result["result"].get("isError") and "No files were modified" in text and "outcome_unknown" not in text and not (outside / "new.txt").exists(), "preflight symlink refusal was misclassified as uncertain effects")
        check("outside the Project or allowed roots" in text and str(outside) not in text and "cwd " not in text,
              "symlink create refusal exposed resolved outside path")
        (self.project_dir / "escape").unlink()
        protected = self.project_dir / "node_modules"
        protected.mkdir()
        (protected / "existing.txt").write_text("PROTECTED_SENTINEL")
        (self.project_dir / "deps").symlink_to(protected, target_is_directory=True)
        result, text = call("edit_project_files", {"changes":[{"kind":"create", "path":"deps/new.txt", "content":"must not write"}]})
        check(result["result"].get("isError") and "path_policy_rejected" in text
              and "protected path" in text and "No files were modified" in text
              and "outcome_unknown" not in text and not (protected / "new.txt").exists()
              and (protected / "existing.txt").read_text() == "PROTECTED_SENTINEL",
              "resolved protected path refusal lost its reason or modified files")
        check(str(protected) not in text, "protected-path refusal exposed the resolved path")
        (self.project_dir / "deps").unlink()
        shutil.rmtree(protected)
        result, text = call("edit_project_files", {"changes":[{"kind":"create", "path":"ranges.py", "content":"must not overwrite"}]})
        check("kind=write" in text, "existing create did not suggest write")
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
        # Neither explicit timeout nor short handoff is supplied: exercise both
        # the default 50s handoff and the relaxed 600s process lifetime.
        start = time.monotonic()
        result, text = call("run_shell", {"command":"printf 'LONG_START\\n'; sleep 65; printf 'PAST_SIXTY_OK\\n'"})
        handoff_elapsed = time.monotonic() - start
        check(45 < handoff_elapsed < 58 and "LONG_START" in text, "default handoff did not preserve output around 50s")
        tool, arguments = next(next_calls(text))
        arguments["wait_secs"] = 50
        result, text = call(tool, arguments)
        lifetime_elapsed = time.monotonic() - start
        check("PAST_SIXTY_OK" in text and "exit_code: 0" in text and lifetime_elapsed >= 65, "default lifetime still killed the command at 60s")
        return {"tool_returns":tool_returns, "hidden_occurrence_applied":True, "next_deduplicated":True,
                "workspace_status_sparse":True, "log_page_skips_preview_head":True, "sync_16s_elapsed":round(sync_elapsed, 2), "tokenless_wait_elapsed":round(wait_elapsed, 2),
                "retained_log_pages":pages, "terminal_incremental_stdout":True,
                "incremental_terminal_chars":incremental_terminal_chars, "default_handoff_elapsed":round(handoff_elapsed,2),
                "default_lifetime_65s_elapsed":round(lifetime_elapsed,2), "timeout_job_identity":True, "symlink_refusal_no_effect":True, "protected_symlink_refusal_no_effect":True, "absolute_read_and_edit":True, "revision_fenced_overwrite":True,
                "unique_exact_without_revision":True, "stale_write_rejected":True, "outside_path_rejected":True,
                "large_read_chars":large_read_chars, "revision_error_field_path":True,
                "stop_without_confirmation":True}

    def reset_fixture(self):
        for file in self.project_dir.iterdir():
            if file.is_dir() and not file.is_symlink(): shutil.rmtree(file)
            else: file.unlink()
        for name, text in {"ranges.py": SOURCE, "test_ranges.py": TESTS,
                           "long_check.py": LONG_CHECK, "noisy_check.py": NOISY_CHECK,
                           "AGENTS.md": INSTRUCTIONS, "CLAUDE.md": INSTRUCTIONS}.items():
            (self.project_dir / name).write_text(text)
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=self.project_dir,
                       env=self.env, stdin=subprocess.DEVNULL, check=True)

    def client_command(self, client, args, mode):
        native = mode == "native"
        env = dict(os.environ)
        if not native: env["WEBCODEX_LOCAL_TEST_TOKEN"] = self.owner
        label = client + "-" + mode
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
            config = self.root / f"{label}-config.json"
            private_json(config, {"mcpServers": {}} if native else json.loads(generated))
            command = [args.claude_path, "-p", "--model", args.claude_model, "--effort", "low",
                       "--strict-mcp-config", "--mcp-config", str(config), "--setting-sources", "project",
                       "--permission-mode", "dontAsk", "--allowedTools", "Bash,Read,Edit,Write,Glob,Grep" if native else "mcp__webcodex__*",
                       "--no-session-persistence", "--output-format", "stream-json", "--verbose"]
        if mode == "mcp-no-native":
            if client == "codex":
                command[-1:-1] = ["--disable", "shell_tool"]
            else:
                command.extend(["--tools", ""])
        return command, env, cwd, prompt

    def probe_client_tools(self, args):
        self.reset_fixture()
        results = {}
        for client in args.clients:
            modes = {}
            for mode in ["mcp", "mcp-no-native"]:
                command, env, cwd, _ = self.client_command(client, args, mode)
                modes[mode] = inventory_probe(client, command, cwd, env, self.root, self.owner)
            before, after = set(modes["mcp"]["tools"]), set(modes["mcp-no-native"]["tools"])
            modes["removed_tools"] = sorted(before - after)
            available = set(modes["mcp-no-native"]["client_init_tools"]) if client == "claude" else after
            modes["remaining_native_tools"] = sorted(name for name in available if not name.startswith("mcp__webcodex__"))
            modes["request_only_entries"] = sorted(after - available)
            if client == "claude":
                # Claude's provider request retains DeferredToolPlaceholder;
                # its init event is the authoritative executable-tool inventory.
                check(not modes["remaining_native_tools"], "Claude --tools did not disable native tools")
                check("Bash" in before and "Read" in before, "Claude baseline did not expose duplicate tools")
            else:
                check(any(name in before for name in ["exec_command", "shell", "shell_command"]), "Codex baseline has no shell tool")
                check(not after.intersection({"exec_command", "write_stdin", "shell", "shell_command"}), "Codex shell_tool switch did not disable shell")
            check(any(name.startswith("mcp__webcodex__") for name in after) or "mcp__webcodex" in modes["mcp-no-native"]["deferred_mcp_namespaces"], "native switch removed MCP tools")
            results[client] = modes
        return results

    def run_client(self, client, args, mode):
        self.reset_fixture()
        native = mode == "native"
        label = client + "-" + mode
        command, env, cwd, prompt = self.client_command(client, args, mode)
        stdout, stderr = self.root / f"{label}.jsonl", self.root / f"{label}.stderr"
        start = time.monotonic()
        frames = run_captured(command, cwd=cwd, env=env, prompt=prompt, stdout=stdout, stderr=stderr, timeout=args.timeout)
        metrics = client_metrics(client, frames)
        calls = metrics["tool_calls"]
        if not native:
            allowed = {t["name"] for t in self.rpc("tools/list", {})["result"]["tools"]}
            check(set(calls) <= allowed, "model used a non-fixture execution tool")
        print(json.dumps({"client":client,"mode":mode,"observed_calls":calls,
                          "native_discovery":metrics["native_discovery"]}), flush=True)
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
        report = {"client": client, "mode": mode, "instructions_followed": instructions_followed,
                  "model": args.codex_model if client == "codex" else args.claude_model,
                  "effort": "low", "elapsed_secs": round(time.monotonic() - start, 2),
                  **metrics, "workflow_sessions": 0, "independent_tests_passed": True,
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
              "source_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "diff", "HEAD", "--", "src", "crates", "scripts"], cwd=ROOT)).hexdigest(),
              "client_versions": {name: subprocess.check_output([executable, "--version"], text=True).strip()
                                  for name, executable in [("codex", args.codex_path), ("claude", args.claude_path)] if name in args.clients},
              "binaries": {name: hashlib.sha256((args.bin_dir / name).read_bytes()).hexdigest()
                           for name in ("webcodex-server", "webcodex-runner", "webcodex")}, "clients": []}
    with tempfile.TemporaryDirectory(prefix="private-", dir=args.output_dir) as private:
        smoke = Acceptance(args.bin_dir.resolve(), Path(private))
        try:
            smoke.prepare()
            inventory = smoke.rpc("tools/list", {})["result"]
            report["discovery"] = discovery_metrics(inventory)
            expected = {"run_shell", "observe_jobs", "stop_job", "read_files", "edit_project_files", "search_project_texts", "read_tool_manifest", "call_runtime_tool"}
            check(set(report["discovery"]["tools"]) == expected, "unexpected direct CLI tools")
            check(report["discovery"]["json_bytes"] <= 10000, "default-Project discovery exceeds 10000 bytes")
            smoke.headers.pop("X-WebCodex-MCP-Project")
            report["discovery_without_default"] = discovery_metrics(smoke.rpc("tools/list", {})["result"])
            check(set(report["discovery_without_default"]["tools"]) == expected | {"list_projects"}, "missing Project discovery")
            check(report["discovery_without_default"]["json_bytes"] <= 12000, "no-default discovery exceeds 12000 bytes")
            init = smoke.rpc("initialize", {"protocolVersion":"2025-11-25", "capabilities":{}, "clientInfo":{"name":"probe","version":"1"}})
            check("list_projects" in init["result"]["instructions"], "missing Project selection guidance")
            picker = smoke.rpc("tools/call", {"name":"list_projects", "arguments":{}})
            check(not picker["result"].get("isError"), "Project picker is not directly callable without a default")
            smoke.headers["X-WebCodex-MCP-Project"] = smoke.project
            report["client_tool_probes"] = smoke.probe_client_tools(args)
            report["probes"] = smoke.probe_native_contracts()
            for client in ([] if args.probe_only else args.clients):
                for mode in ["mcp", "mcp-no-native", "native"]:
                    result = smoke.run_client(client, args, mode)
                    if mode == "mcp-no-native":
                        result["remaining_native_tools"] = report["client_tool_probes"][client]["remaining_native_tools"]
                        result["remaining_native_tools_source"] = "loopback inventory with same flags; Claude also reports actual init tools"
                    report["clients"].append(result)
                    private_json(args.output_dir / "report.json", report)
                    print(json.dumps(report["clients"][-1], ensure_ascii=False), flush=True)
        finally:
            smoke.close()
            report["services_stopped"] = all(p is None or p.poll() is not None for p in [smoke.server, smoke.runner])
            private_json(args.output_dir / "report.json", report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
