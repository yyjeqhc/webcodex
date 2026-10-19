#!/usr/bin/env python3
"""Real MCP -> Server -> WS Runner -> native Project Knowledge smoke.

Build dogfood Server/Runner first. Uses the existing disposable Smoke lifecycle
through InteractiveSmoke's modern stateless MCP adapter; no model server runs.
All identities, credentials, Git projects, cache, and Runner config are temporary.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from e2e_job_input_ws import InteractiveSmoke
from e2e_session_continuity_ws import ROOT, check

SCOPES = ["runtime:read", "project:read", "project:write",
          "plugin:inspect", "plugin:invoke", "plugin:manage"]
RUNNER = "knowledge-runner"
PROVIDER = "project-knowledge"
PROJECT = f"agent:{RUNNER}:fixture"


class KnowledgeSmoke(InteractiveSmoke):
    def __init__(self, bin_dir: Path, root: Path):
        super().__init__(bin_dir, root)
        self.env = {key: value for key, value in self.env.items() if key in {
            "PATH", "SYSTEMROOT", "WINDIR", "PATHEXT", "TMP", "TEMP", "LANG",
            "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME",
            "GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM", "RUST_LOG"}}
        self.env["HOME"] = str(root / "home")
        self.env["XDG_CACHE_HOME"] = str(root / "cache")
        (root / "home").mkdir()

    def plugin(self, arguments, *, token=None, success=True):
        self.rpc_id += 1
        reply = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
                                  "method": "tools/call", "params": {
                                      "name": "plugin_tool", "arguments": arguments}}, token or self.owner)
        result = reply.get("result")
        if result is None:
            denied = reply.get("error") == "insufficient_scope" or reply.get("status") == 403
            check(not success and denied, "expected a bounded Plugin scope denial")
            return {"dispatchState": "not_started", "error_kind": "insufficient_scope"}
        check((not result.get("isError", False)) is success,
              "unexpected Plugin outcome; fields=" + str(list(result)))
        self.last_result = result
        return result.get("structuredContent", {})

    def describe(self, name, *, project=PROJECT, token=None, success=True):
        return self.plugin({"action": "describe", "runner": RUNNER, "plugin": PROVIDER,
                            "tool": name, "project": project}, token=token, success=success)

    def invoke(self, binding, arguments, **options):
        return self.plugin({"action": "call", "binding": binding, "arguments": arguments}, **options)

    def run(self):
        self.start_server()
        user = self.post("/api/users/create", {"username": "knowledge-owner"}, self.bootstrap)
        check(user.get("success") is True, "fixture user creation")
        self.owner = self.pat("knowledge-owner", SCOPES)
        no_invoke = self.pat("knowledge-owner", [scope for scope in SCOPES if scope != "plugin:invoke"])
        enrollment = self.post("/api/agent-tokens/create", {
            "username": "knowledge-owner", "client_id": RUNNER,
            "name": "knowledge-smoke"}, self.bootstrap)
        check(isinstance(enrollment.get("token"), str), "fixture Runner enrollment")
        repo, other, registry = self.root / "project", self.root / "other", self.root / "projects"
        for directory in [repo, other, registry]:
            directory.mkdir()
        subprocess.run(["git", "init", "-q", str(repo)], env=self.env,
                       stdin=subprocess.DEVNULL, check=True, timeout=10)
        (repo / "model.py").write_text("def parseHTTPRequest(value):\n    return value\n")
        (repo / "guide.md").write_text("项目知识支持中文检索。\n", encoding="utf-8")
        (repo / "untracked.md").write_text("untrackedneedle\n")
        (other / "private.md").write_text("outsideprojectneedle\n")
        subprocess.run(["git", "add", "model.py", "guide.md"], cwd=repo,
                       env=self.env, stdin=subprocess.DEVNULL, check=True, timeout=10)
        for name, path in [("fixture", repo), ("other", other)]:
            (registry / f"{name}.toml").write_text(f'id = "{name}"\npath = {json.dumps(str(path))}\n')
        config = [f'server_url = "{self.url}"', f'token = {json.dumps(enrollment["token"])}',
                  f'client_id = "{RUNNER}"', 'owner = "knowledge-owner"',
                  'transport = "websocket"', f'project_registry_dir = {json.dumps(str(registry))}',
                  '[policy]', 'allow_cwd_anywhere = false',
                  f'allowed_roots = {json.dumps([str(repo), str(other)])}']
        plugin_config = ['[plugins]', 'request_timeout_secs = 30', '[[plugins.providers]]',
                         f'id = "{PROVIDER}"', 'name = "Project Knowledge"',
                         f'command = {json.dumps(sys.executable)}',
                         f'args = {json.dumps(["-B", str(ROOT / "plugins/project-knowledge/plugin.py")])}',
                         f'cwd = {json.dumps(str(repo))}', 'timeout_secs = 30']
        config_path = self.root / "runner.toml"
        config_path.write_text("\n".join(config + plugin_config) + "\n")
        config_path.chmod(0o600)
        self.runner = self.launch("webcodex-runner", ["--config", str(config_path)], self.env)
        self.wait_ready(PROJECT, self.owner)
        ready = self.plugin({"action": "check", "runner": RUNNER, "plugin": PROVIDER})
        check(ready.get("ready") is True, "actual FTS5 provider admission failed")
        catalog = self.plugin({"action": "list", "runner": RUNNER, "plugin": PROVIDER})
        check({tool["name"] for tool in catalog["tools"]} == {"index", "status", "search", "read"}, "frozen Plugin catalog")
        describe = {"action": "describe", "runner": RUNNER, "plugin": PROVIDER, "tool": "index"}
        missing = self.plugin(describe, success=False)
        check(missing.get("error", {}).get("code") == "plugin_project_required"
              and missing.get("dispatchState") == "not_started" and "binding" not in missing,
              "missing Project must fail without creating a binding")
        recovery = missing.get("recovery", "")
        check(all(phrase in recovery for phrase in ["action=describe", "intended explicit project",
                  "only binding and arguments", "uncertain effect"]), "missing Project recovery is not actionable")
        fallback = self.last_result["content"][0]["text"]
        check(fallback.startswith("Recovery: " + recovery) and missing["error"]["message"] in fallback
              and len(fallback.encode("utf-8")) <= 1024,
              "text-only Hosts must receive bounded, complete missing-Project recovery")
        api = self.post("/api/tools/call", {"tool": "plugin_tool", "params": describe}, self.owner)
        check(api.get("success") is False and api.get("output", {}).get("failure_kind") == "plugin_project_required"
              and api["output"].get("dispatch_certainty") == "not_started"
              and api["output"].get("recovery") == recovery and "binding" not in api["output"],
              "MCP and Runtime API must preserve the same describe recovery")
        bindings = {name: self.describe(name)["binding"] for name in ["index", "status", "search", "read"]}
        check(not self.invoke(bindings["status"], {})["indexed"], "index built automatically")
        for project in [PROJECT, f"agent:{RUNNER}:other"]:
            arguments = {"action": "call", "binding": bindings["index"], "arguments": {}, "project": project}
            self.rpc_id += 1
            rejected = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
                "method": "tools/call", "params": {"name": "plugin_tool", "arguments": arguments}}, self.owner)
            message = rejected.get("error", {}).get("message", "")
            check(rejected.get("error", {}).get("code") == -32602
                  and all(phrase in message for phrase in ["only binding and arguments", "same intended Project",
                          "another Project", "unknown outcome"]), "call-time Project needs precise parser recovery")
            api = self.post("/api/tools/call", {"tool": "plugin_tool", "params": arguments}, self.owner)
            check(api.get("status") == 400 and api.get("error") == message,
                  "Runtime API must retain the canonical parser rejection")
        check(not self.invoke(bindings["status"], {})["indexed"], "argument recovery dispatched rejected index calls")
        print("PASS: action-specific Plugin recovery on MCP/API, explicit describe Project and rejected call retargeting")
        self.invoke(bindings["index"], {}, token=no_invoke, success=False)
        check(not self.invoke(bindings["status"], {})["indexed"], "missing invoke scope dispatched index")
        wrong = self.describe("index", project=f"agent:{RUNNER}:other")["binding"]
        rejected = self.invoke(wrong, {}, success=False)
        check(rejected.get("dispatchState") == "not_started", "wrong Project reached provider")
        rejected = self.invoke(bindings["index"], {"root": str(other)}, success=False)
        check(rejected.get("dispatchState") == "not_started", "root override reached provider")
        check(not self.invoke(bindings["status"], {})["indexed"], "denied calls mutated cache")
        indexed = self.invoke(bindings["index"], {})
        check(indexed["coverage"]["indexed_files"] == 2, "tracked source coverage mismatch")
        refreshed = self.invoke(bindings["index"], {"mode": "refresh"})["coverage"]
        check(refreshed["mode"] == "refresh" and refreshed["reused_files"] == 2
              and refreshed["remaining_files"] == 0, "incremental unchanged refresh coverage mismatch")
        rejected = self.invoke(bindings["index"], {"mode": "unsupported"}, success=False)
        check(rejected.get("dispatchState") == "not_started", "invalid refresh mode reached provider")
        for query, expected in [("parseHTTPRequest", "model.py"), ("中文检索", "guide.md")]:
            hits = self.invoke(bindings["search"], {"query": query})["hits"]
            check(bool(hits) and hits[0]["path"] == expected, "native search source mismatch")
            hit = hits[0]
            read = self.invoke(bindings["read"], {key: hit[key] for key in ["path", "source_hash", "start_line", "end_line"]})
            check(read["text"] == hit["text"], "native read source provenance mismatch")
        for query in ["untrackedneedle", "outsideprojectneedle"]:
            check(not self.invoke(bindings["search"], {"query": query})["hits"], "source authority leak")
        self.invoke(bindings["read"], {"path": "../other/private.md", "source_hash": "0" * 64}, success=False)
        check("outsideprojectneedle" not in json.dumps(self.last_result), "path guard leaked source")
        (repo / "model.py").write_text("def changed():\n    pass\n")
        check(not self.invoke(bindings["search"], {"query": "parseHTTPRequest"})["hits"], "stale source returned as current")
        refreshed = self.invoke(bindings["index"], {"mode": "refresh"})["coverage"]
        check(refreshed["updated_files"] == 1 and refreshed["reused_files"] == 1,
              "changed-source refresh coverage mismatch")
        hits = self.invoke(bindings["search"], {"query": "changed"})["hits"]
        check(bool(hits) and hits[0]["path"] == "model.py", "refreshed source not searchable")
        (repo / "model.py").unlink()
        refreshed = self.invoke(bindings["index"], {"mode": "refresh"})["coverage"]
        check(refreshed["deleted_files"] == 1 and refreshed["indexed_files"] == 1,
              "deleted-source refresh retained stale rows")
        print("PASS: real MCP/WS native admission, exact Project binding, scoped index/refresh/search/read, source guards and stale exclusion")
        config_path.write_text("\n".join(config + ['[plugins]', 'providers = []']) + "\n")
        reloaded = self.plugin({"action": "reload", "runner": RUNNER})
        check(not reloaded.get("failures"), "provider removal failed")
        retired = self.invoke(bindings["status"], {}, success=False)
        check(retired.get("dispatchState") == "not_started", "removed binding retargeted")
        missing_binding = self.invoke(bindings["status"], {}, success=False)
        check(missing_binding.get("error", {}).get("code") == "describe_required"
              and missing_binding.get("dispatchState") == "not_started",
              "retired binding must require fresh describe without redispatch")
        check("projectBound" in missing_binding.get("recovery", "") and "unknown outcome" in missing_binding["recovery"],
              "lost binding recovery must retain Project selection and uncertain-effect guidance")
        check(missing_binding["recovery"] in self.last_result["content"][0]["text"],
              "text-only Hosts must retain lost-binding and unknown-outcome guidance")
        projects = self.post("/api/tools/call", {"tool": "list_projects", "params": {}}, self.owner)
        check(projects.get("success") and any(item.get("id") == PROJECT for item in projects["output"]["projects"]), "disabling provider affected Projects")
        print("PASS: removing optional provider retires bindings while Project discovery remains available")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    args = parser.parse_args()
    check(sys.version_info >= (3, 10), "Python 3.10+ required")
    check(shutil.which("git") is not None, "Git required")
    for binary in ["webcodex-server", "webcodex-runner"]:
        check((args.bin_dir / binary).is_file(), "build dogfood Server/Runner first")
    with tempfile.TemporaryDirectory(prefix="webcodex-project-knowledge-e2e-") as root:
        smoke = KnowledgeSmoke(args.bin_dir.resolve(), Path(root))
        try:
            smoke.run()
        finally:
            smoke.close()


if __name__ == "__main__":
    main()
