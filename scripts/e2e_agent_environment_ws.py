#!/usr/bin/env python3
"""Opt-in real MCP -> Server -> WS Runner -> Plugin -> Pi acceptance.

Install plugins/agent-environment and build dogfood binaries first. All services,
identities, credentials, Pi settings, extension files and effects are disposable.
No production service, user's Pi configuration or remote model is contacted.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from urllib.error import HTTPError
from urllib.request import Request

from e2e_job_input_ws import InteractiveSmoke
from e2e_session_continuity_ws import ROOT, check

SCOPES = ["runtime:read", "project:read", "project:write", "job:run",
          "session:collaborate", "plugin:inspect", "plugin:invoke", "plugin:manage", "runner:manage"]
PACKAGE = ROOT / "plugins/agent-environment"


class AgentEnvironmentSmoke(InteractiveSmoke):
    def __init__(self, bin_dir: Path, root: Path):
        super().__init__(bin_dir, root)
        # Do not inherit the operator's API/MCP/SSH credentials into this fixture.
        self.env = {key: value for key, value in self.env.items() if key in {
            "PATH", "SYSTEMROOT", "WINDIR", "PATHEXT", "TMP", "TEMP", "LANG",
            "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME",
            "GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM", "RUST_LOG"}}
        self.env["HOME"] = str(root / "home")
        (root / "home").mkdir()
        self.extra_meta = {}
        self.project = "agent:agent-env-runner:fixture"
        self.native_status = {}
        self.delegation = {}
        self.config_text = ""

    def post(self, path, body, token):
        if path != "/mcp":
            return super().post(path, body, token)
        params = body.setdefault("params", {})
        params["_meta"] = {**self.extra_meta,
                           "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                           "io.modelcontextprotocol/clientCapabilities": {}}
        headers = {"Authorization": "Bearer " + token, "Content-Type": "application/json",
                   "Accept": "application/json, text/event-stream", "MCP-Protocol-Version": "2026-07-28",
                   "MCP-Method": body["method"]}
        if "name" in params:
            headers["MCP-Name"] = params["name"]
        request = Request(self.url + path, json.dumps(body).encode(), method="POST", headers=headers)
        try:
            with self.opener.open(request, timeout=40) as response:
                raw = response.read(2 * 1024 * 1024)
        except HTTPError as error:
            raw = error.read(2 * 1024 * 1024)
        return json.loads(raw)

    def names(self):
        self.rpc_id += 1
        result = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
                                  "method": "tools/list", "params": {}}, self.owner)
        return {tool["name"] for tool in result["result"]["tools"]}

    def plugin(self, arguments, *, token=None, success=True):
        self.last_plugin_result = self.raw_tool("plugin_tool", arguments, token or self.owner)
        check((not self.last_plugin_result.get("isError", False)) is success,
              "Plugin outcome mismatch: " + json.dumps(self.last_plugin_result, ensure_ascii=False)[:2500])
        return self.last_plugin_result["structuredContent"]

    def describe(self, *, project=None, token=None, success=True):
        return self.plugin({"action": "describe", "runner": "agent-env-runner", "plugin": "pi",
                            "tool": "agent_environment", "project": project or self.project},
                           token=token, success=success)

    def native(self, action, *, success=True, **extra):
        args = {"action": action, **({"session": self.native_status["session"]} if action != "sessions" else {}), **extra}
        result = self.plugin({"action": "call", "binding": self.binding, "arguments": args}, success=success)
        text = self.last_plugin_result["content"][0]["text"]
        prefix = "Trusted WebCodex delegation target: "
        suffix = ". Project binding is routing, not a native-tool sandbox."
        check(text.startswith(prefix) and text.endswith(suffix), "delegated target provenance missing")
        self.delegation = json.loads(text[len(prefix):-len(suffix)])
        check(self.delegation["project"] == self.project, "delegated result lost the authorized Project")
        check(self.delegation["runner"] == "agent-env-runner" and self.delegation["provider"] == "pi",
              "delegated Runner/provider provenance mismatch")
        check(result.get("native_source") == "pi", "native Pi provenance missing")
        if result.get("next_request"):
            self.native_status.update({key: result[key] for key in ["session", "native_session_id", "next_request"]})
        return result

    def batch(self, entries, *, success=True, **extra):
        calls = []
        for index, (name, args) in enumerate(entries):
            definition = self.native("describe", tool=name)["tool"]
            calls.append({"id": f"call-{index}", "tool": name, "binding": definition["binding"], "arguments": args})
        return self.native("call", calls=calls, request=self.native_status["next_request"], success=success, **extra)

    def run(self):
        self.start_server()
        for username in ["agent-env-owner", "agent-env-foreign"]:
            check(self.post("/api/users/create", {"username": username}, self.bootstrap).get("success"), "create fixture user")
        self.owner = self.pat("agent-env-owner", SCOPES)
        foreign = self.pat("agent-env-foreign", SCOPES)
        read_only = self.pat("agent-env-owner", [scope for scope in SCOPES if scope != "project:write"])
        no_project = self.pat("agent-env-owner", [scope for scope in SCOPES if not scope.startswith("project:")])
        baseline_names = self.names()
        check("plugin_tool" in baseline_names, "Plugin gateway missing")
        agent = self.post("/api/agent-tokens/create", {"username": "agent-env-owner",
                          "client_id": "agent-env-runner", "name": "agent-environment-smoke"}, self.bootstrap)
        check(isinstance(agent.get("token"), str), "temporary Runner token creation failed")
        repo, other, registry = self.root / "project", self.root / "other", self.root / "projects"
        agent_dir, state = self.root / "pi", self.root / "pi-state"
        for directory in [repo, other, registry, agent_dir / "extensions", state]:
            directory.mkdir(parents=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(repo)], env=self.env, check=True)
        (repo / "input.txt").write_text("native-input\n")
        for name, path in [("fixture", repo), ("other", other)]:
            (registry / f"{name}.toml").write_text(f'id = "{name}"\npath = {json.dumps(str(path))}\n')
        shutil.copyfile(PACKAGE / "test/fixtures/extension.mjs", agent_dir / "extensions/fixture.js")
        node = shutil.which("node")
        check(node is not None, "Node is required")
        (agent_dir / "mcp.json").write_text(json.dumps({"mcpServers": {"fixture": {
            "command": node, "args": [str(PACKAGE / "test/fixtures/mcp-server.mjs")], "exposure": "direct"}}}))
        original_settings = json.dumps({"defaultTools": ["read", "write", "edit", "bash", "codemode", "tool_search"]})
        (agent_dir / "settings.json").write_text(original_settings)
        config = [f'server_url = "{self.url}"', f'token = {json.dumps(agent["token"])}',
                  'client_id = "agent-env-runner"', 'owner = "agent-env-owner"', 'transport = "websocket"',
                  f'project_registry_dir = {json.dumps(str(registry))}', '[policy]', 'allow_raw_shell = true',
                  f'allowed_roots = {json.dumps([str(repo), str(other)])}', '[plugins]', 'request_timeout_secs = 30',
                  '[[plugins.providers]]', 'id = "pi"', 'name = "Native Pi dogfood"', f'command = {json.dumps(node)}',
                  f'args = {json.dumps([str(PACKAGE / "src/plugin.mjs"), "--agent-dir", str(agent_dir), "--state-dir", str(state)])}',
                  f'cwd = {json.dumps(str(repo))}', 'timeout_secs = 30']
        self.config_text = "\n".join(config) + "\n"
        config_path = self.root / "runner.toml"
        config_path.write_text(self.config_text)
        config_path.chmod(0o600)
        self.runner = self.launch("webcodex-runner", ["--config", str(config_path)], self.env)
        self.wait_ready(self.project, self.owner)
        check(self.names() == baseline_names, "native Pi tools leaked into outer tools/list")
        self.plugin({"action": "describe", "runner": "agent-env-runner", "plugin": "pi", "tool": "agent_environment"}, success=False)
        self.describe(token=foreign, success=False)
        self.describe(token=no_project, success=False)
        restricted = self.describe(token=read_only)["binding"]
        self.plugin({"action": "call", "binding": restricted, "arguments": {"action": "sessions"}}, token=read_only, success=False)
        wrong = self.describe(project="agent:agent-env-runner:other")["binding"]
        rejected = self.plugin({"action": "call", "binding": wrong, "arguments": {"action": "sessions"}}, success=False)
        check(rejected.get("dispatchState") == "not_started", "Project mismatch reached native execution")
        check(not (agent_dir / "native-hooks.jsonl").exists(), "denied targets initialized Pi")
        print("PASS: exact Project/Runner authority, foreign caller and scope denial; fixed outer surface")

        self.binding = self.describe()["binding"]
        registration = registry / "fixture.toml"
        original_registration = registration.read_text()
        registration.write_text(original_registration + "allow_patch = false\n")
        revoked = self.plugin({"action": "call", "binding": self.binding, "arguments": {"action": "sessions"}}, success=False)
        check(revoked["dispatchState"] == "not_started", "revoked Project write authority reached native execution")
        check(not (agent_dir / "native-hooks.jsonl").exists(), "read-only Project initialized Pi")
        registration.write_text(original_registration)
        self.binding = self.describe()["binding"]
        self.native("sessions")
        original_session = self.native_status["native_session_id"]
        original_instance = self.delegation["provider_instance"]
        self.batch([("write", {"path": "written.txt", "content": "before\n"})])
        self.batch([("edit", {"path": "written.txt", "oldText": "before", "newText": "after"})])
        result = self.batch([("read", {"path": "written.txt"}), ("bash", {"command": "printf native-shell"})])
        check(result["native_dispatch_state"] == "completed" and [r["id"] for r in result["results"]] == ["call-0", "call-1"], "native builtins/batch mapping")
        check((repo / "written.txt").read_text() == "after\n", "native edit did not occur")
        self.batch([("native_echo", {"value": "extension"}), ("mcp__fixture__echo", {"value": "mcp-chain"})])
        self.batch([("native_echo", {"value": "denied"})], success=False)
        partial = self.batch([("native_echo", {"value": "partial"}), ("native_fail", {})], success=False)
        check([r["is_error"] for r in partial["results"]] == [False, True], "native partial failure lost identity")
        code = self.batch([("codemode", {"code": "console.log(await tools.native_code({value:'native-code'})); console.log(await tools.native_deferred({value:'native-deferred'}));"})])
        check("native-deferred" in json.dumps(code), "native deferred/codemode callability lost")
        hooks = (agent_dir / "native-hooks.jsonl").read_text()
        check('"event":"tool_call"' in hooks and '"event":"tool_result"' in hooks and '"tool":"mcp__fixture__echo"' in hooks, "native pipeline hooks missing")
        print("PASS: Server -> WS Runner -> Plugin -> Pi read/bash/edit/write, extension, MCP, native hooks and partial batch")

        generic = self.post("/api/tools/call", {"tool": "plugin_tool", "params": {
            "action": "call", "binding": self.binding,
            "arguments": {"action": "history", "session": self.native_status["session"], "limit": 2}}}, self.owner)
        check(generic.get("success") and generic["output"]["project"] == self.project,
              "generic Runtime did not preserve Project provenance")
        check(generic["output"]["delegation"]["provider_instance"] == original_instance,
              "generic Runtime retargeted the provider")

        self.extra_meta = {"openai/session": "UNTRUSTED_OUTER_SESSION_SENTINEL"}
        direct = self.raw_tool("plugin_tool", {"action": "call", "binding": self.binding,
                               "arguments": {"action": "history", "session": self.native_status["session"], "limit": 4}}, self.owner, direct=True)
        self.extra_meta = {}
        check(direct["structuredContent"]["native_session_id"] == original_session, "outer metadata retargeted native session")
        check("UNTRUSTED_OUTER_SESSION_SENTINEL" not in json.dumps(direct), "outer metadata leaked to delegated output")
        check("Trusted WebCodex delegation target" in direct["content"][0]["text"], "direct MCP target provenance missing")
        candidate = self.call("check_runner_config", {"client_id": "agent-env-runner"})
        self.call("reload_runner_config", {"client_id": "agent-env-runner",
                                          "expected_generation": candidate["current_generation"]})
        self.native("sessions")
        check(self.native_status["native_session_id"] == original_session and self.delegation["provider_instance"] == original_instance,
              "unchanged provider reload replaced a native session")
        old_schema = self.native("describe", tool="native_echo")["tool"]["binding"]
        (agent_dir / "extensions/dynamic.js").write_text('export default function(pi){pi.registerTool({name:"dynamic_tool",label:"Dynamic",description:"Hot-loaded fixture",parameters:{type:"object",properties:{}},async execute(){return {content:[{type:"text",text:"hot-loaded"}]};}});}\n')
        self.native("refresh")
        check(self.native_status["native_session_id"] == original_session, "native reload silently changed session")
        stale = self.native("call", calls=[{"id": "stale", "tool": "native_echo", "binding": old_schema, "arguments": {"value": "must-not-run"}}],
                            request=self.native_status["next_request"], success=False)
        check(stale["native_dispatch_state"] == "not_started", "stale schema dispatched")
        self.batch([("dynamic_tool", {})])
        check((agent_dir / "settings.json").read_text() == original_settings, "user-style settings were rewritten")
        check(self.names() == baseline_names, "hot-loaded tool was flattened into global MCP catalog")
        print("PASS: dynamic native extension refresh without Server/Runner restart; unchanged-provider identity; metadata isolation")

        self.batch([("native_slow", {})], deadline=int(time.time() * 1000) - 1, success=False)
        unknown = self.batch([("native_slow", {})], timeout_ms=120, success=False)
        check(unknown["native_dispatch_state"] == "outcome_unknown" and unknown["quarantined"], "native deadline was incorrectly definite")
        self.native("refresh", success=False)
        time.sleep(0.8)
        events = [json.loads(line) for line in (agent_dir / "native-hooks.jsonl").read_text().splitlines()]
        check(sum(event["event"] == "slow_effect" for event in events) == 1, "native timed-out effect repeated or vanished")
        print("PASS: pre-dispatch deadline and post-dispatch unknown; deliberately uncancellable effect happened exactly once")

        # Operator config change explicitly replaces the quarantined provider;
        # it is not an automatic retry or a native-session default.
        old_binding, old_session = self.binding, self.native_status["session"]
        config_path.write_text(self.config_text.replace("timeout_secs = 30", "timeout_secs = 5"))
        self.plugin({"action": "reload", "runner": "agent-env-runner"})
        self.plugin({"action": "call", "binding": old_binding, "arguments": {"action": "sessions"}}, success=False)
        self.binding = self.describe()["binding"]
        self.native_status = {}
        self.native("sessions")
        check(self.delegation["provider_instance"] != original_instance, "changed provider retained stale process identity")
        rejected = self.native("tools", session=old_session, success=False)
        check(rejected["native_dispatch_state"] == "not_started", "restarted provider accepted an old native session")
        bash = self.native("describe", tool="bash")["tool"]
        args = {"action": "call", "session": self.native_status["session"], "request": self.native_status["next_request"], "timeout_ms": 30000,
                "calls": [{"id": "outer-deadline", "tool": "bash", "binding": bash["binding"],
                           "arguments": {"command": "printf 'started\\n' >> outer-deadline.log; sleep 8; printf 'finished\\n' >> outer-deadline.log"}}]}
        result = self.plugin({"action": "call", "binding": self.binding, "arguments": args}, success=False)
        check(result.get("dispatchState") == "outcome_unknown", "Runner timeout replayed or claimed no native effects")
        check((repo / "outer-deadline.log").read_text().count("started") == 1, "outer-deadline effect missing or duplicated")
        self.plugin({"action": "call", "binding": self.binding, "arguments": {"action": "sessions"}}, success=False)
        print("PASS: changed-provider/native-session fences and actual Runner deadline after Pi bash dispatch; no effect replay")

        # Existing Runner Hot config activates provider add/remove transactionally.
        # A changed provider set replaces the whole set; never silently rebind.
        short_config = self.config_text.replace("timeout_secs = 30", "timeout_secs = 5")
        extra = ['[[plugins.providers]]', 'id = "pi-extra"', 'name = "Second isolated Pi"',
                 f'command = {json.dumps(node)}',
                 f'args = {json.dumps([str(PACKAGE / "src/plugin.mjs"), "--agent-dir", str(agent_dir), "--state-dir", str(self.root / "pi-extra-state")])}',
                 f'cwd = {json.dumps(str(repo))}', 'timeout_secs = 5']
        config_path.write_text(short_config + "\n".join(extra) + "\n")
        observed = self.call("check_runner_config", {"client_id": "agent-env-runner"})
        self.call("reload_runner_config", {"client_id": "agent-env-runner", "expected_generation": observed["current_generation"]})
        extra_binding = self.plugin({"action": "describe", "runner": "agent-env-runner", "plugin": "pi-extra",
                                     "tool": "agent_environment", "project": self.project})["binding"]
        config_path.write_text(short_config)
        observed = self.call("check_runner_config", {"client_id": "agent-env-runner"})
        self.call("reload_runner_config", {"client_id": "agent-env-runner", "expected_generation": observed["current_generation"]})
        removed = self.plugin({"action": "call", "binding": extra_binding, "arguments": {"action": "sessions"}}, success=False)
        check(removed["dispatchState"] == "not_started", "removed provider accepted a stale binding")
        check(self.names() == baseline_names, "provider add/remove changed the model tool surface")
        print("PASS: live provider add/remove through generation-fenced Runner config; removed identity rejected")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    parser.add_argument("--keep", action="store_true", help="Retain disposable diagnostics locally; never commit their credentials")
    args = parser.parse_args()
    for binary in ["webcodex-server", "webcodex-runner"]:
        check((args.bin_dir / binary).is_file(), "build dogfood Server/Runner first")
    check((PACKAGE / "node_modules/@earendil-works/pi-coding-agent").is_dir(), "install the agent-environment package first")
    root = Path(tempfile.mkdtemp(prefix="webcodex-agent-environment-e2e-"))
    smoke = AgentEnvironmentSmoke(args.bin_dir.resolve(), root)
    try:
        smoke.run()
    finally:
        smoke.close()
        if args.keep:
            print("Disposable diagnostics retained locally:", root)
        else:
            shutil.rmtree(root)


if __name__ == "__main__":
    main()
