#!/usr/bin/env python3
"""Opt-in real Server/Runner -> MCP CLI output evidence on one disposable fixture.

Build dogfood binaries separately. No production configuration, credentials,
service, external model or Host conversation is used. This records UTF-8/JSON
bytes, never Host tokens or an end-to-end performance claim.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from urllib.error import HTTPError
from urllib.request import Request

from e2e_job_input_ws import InteractiveSmoke
from e2e_session_continuity_ws import ROOT, check


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def recovery_calls(text: str) -> list[dict]:
    calls = []
    for line in text.splitlines():
        try:
            value = json.loads(line.strip())
        except ValueError:
            continue
        if isinstance(value, dict) and isinstance(value.get("tool"), str) and isinstance(
                value.get("arguments"), dict):
            calls.append(value)
    return calls


def job_identity(text: str) -> str:
    for call in recovery_calls(text):
        if call["tool"] == "observe_jobs":
            items = call["arguments"].get("items", [])
            if items and isinstance(items[0].get("job_id"), str):
                return items[0]["job_id"]
    match = re.search(r"(?:^|; )job_id: (wc_job_[A-Za-z0-9_-]+)", text, re.MULTILINE)
    check(match is not None, "CLI execution receipt lost its original Job identity")
    return match.group(1)


class CliRuntimeSmoke(InteractiveSmoke):
    def __init__(self, bin_dir: Path, root: Path, output_dir: Path | None):
        super().__init__(bin_dir, root)
        self.output_dir = output_dir
        self.samples = []
        self.direct_names = set()
        self.extra_secrets = []
        # A local rust-analyzer may run Cargo metadata; this fixture has no
        # dependencies and must not acquire anything from the network.
        self.env["CARGO_NET_OFFLINE"] = "true"

    def cli_rpc(self, method: str, params: dict, token: str | None = None):
        self.rpc_id += 1
        params = dict(params)
        params["_meta"] = {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {},
        }
        body = {"jsonrpc": "2.0", "id": self.rpc_id, "method": method, "params": params}
        headers = {
            "Authorization": "Bearer " + (token or self.owner),
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            "MCP-Protocol-Version": "2026-07-28",
            "MCP-Method": method,
            "x-webcodex-mcp-surface": "cli",
        }
        if "name" in params:
            headers["MCP-Name"] = params["name"]
        request = Request(self.url + "/mcp", json.dumps(body).encode(),
                          method="POST", headers=headers)
        try:
            with self.opener.open(request, timeout=45) as response:
                status, raw = response.status, response.read(2 * 1024 * 1024)
        except HTTPError as error:
            status, raw = error.code, error.read(2 * 1024 * 1024)
            error.close()
        check(len(raw) < 2 * 1024 * 1024, "fixture MCP response exceeded capture bound")
        return status, json.loads(raw), len(raw)

    def sample(self, name: str, tool: str, arguments: dict, *, error=False,
               token=None, http_denial=False) -> str:
        params = ({"name": tool, "arguments": arguments} if tool in self.direct_names else
                  {"name": "call_runtime_tool",
                   "arguments": {"tool": tool, "arguments": arguments}})
        status, reply, wire_bytes = self.cli_rpc("tools/call", params, token)
        result = reply.get("result")
        if http_denial:
            check(status == 403, f"{name}: expected HTTP scope denial")
            check(result is None, f"{name}: denied request became a successful tool result")
            text = None
        else:
            check(status == 200 and isinstance(result, dict), f"{name}: missing MCP result")
            check("structuredContent" not in result, f"{name}: CLI returned structured content")
            if error is not None:
                check(bool(result.get("isError")) is error, f"{name}: unexpected isError")
            blocks = result.get("content", [])
            check(all(block.get("type") == "text" for block in blocks),
                  f"{name}: unexpected non-text fixture output")
            text = "".join(block.get("text", "") for block in blocks)
        serialized = json.dumps(reply, ensure_ascii=False, separators=(",", ":"))
        for secret in [self.bootstrap, self.owner, self.foreign, *self.extra_secrets]:
            check(secret not in serialized, "a temporary credential appeared in captured output")
        metric = {
            "name": name, "tool": tool, "http_status": status,
            "is_error": http_denial or (bool(result.get("isError")) if result else False),
            "text_utf8_bytes": len(text.encode("utf-8")) if text is not None else None,
            "text_chars": len(text) if text is not None else None,
            "wire_response_bytes": wire_bytes,
        }
        self.samples.append(metric)
        if self.output_dir:
            # Only fixture responses are persisted. Authorization headers,
            # token-creation responses and Runner configuration are never saved.
            (self.output_dir / f"{name}.json").write_text(
                json.dumps(reply, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            if text is not None:
                (self.output_dir / f"{name}.txt").write_text(text, encoding="utf-8")
        print(json.dumps(metric, ensure_ascii=False, sort_keys=True))
        return text or ""

    def run_samples(self) -> dict:
        self.prepare()  # Exactly one disposable Server and one Runner.
        status, inventory, _ = self.cli_rpc("tools/list", {})
        check(status == 200, "CLI tool discovery failed")
        self.direct_names = {item["name"] for item in inventory["result"]["tools"]}
        check({"read_files", "edit_project_files", "run_shell", "observe_jobs"} <= self.direct_names,
              "CLI direct surface is incomplete")

        (self.project_dir / "src").mkdir()
        (self.project_dir / "apps").mkdir()
        (self.project_dir / "src/sample.txt").write_text(
            "before\nneedle 中文\nbetween\nneedle again\nafter\n", encoding="utf-8")
        (self.project_dir / "apps/main.txt").write_text("app\n", encoding="utf-8")
        subprocess.run(["git", "add", "--", "src", "apps"], cwd=self.project_dir,
                       env=self.env, stdin=subprocess.DEVNULL, check=True)
        for name, tool, args in [
            ("projects", "list_projects", {"client_id": "input-runner", "summary_only": True}),
            ("bootstrap", "work_on_project", {"project": self.project,
             "instruction": "Inspect the temporary CLI regression fixture"}),
            ("files", "list_project_files", {"project": self.project, "path": ".", "limit": 30}),
            ("tracked_folded", "list_project_tracked_files",
             {"project": self.project, "depth": 1, "limit": 30}),
            ("search_context", "search_project_texts",
             {"project": self.project, "queries": [{"pattern": "needle", "pattern_mode": "literal",
               "path": "src", "context_before": 1, "context_after": 1, "limit": 5}]}),
            ("search_file_context", "search_file_context",
             {"project": self.project, "queries": [{"pattern": "needle", "pattern_mode": "literal",
               "path": "src", "limit": 5}], "read_before": 1, "read_after": 1}),
        ]:
            text = self.sample(name, tool, args)
            if name == "tracked_folded":
                check("src//" not in text and "apps//" not in text,
                      "tracked directory separator was duplicated")

        before = self.sample("read_versioned", "read_files",
                             {"project": self.project, "items": [{"path": "one.txt"}]})
        match = re.search(r"read_revision ([0-9]+)", before)
        check(match is not None, "read revision not visible in CLI")
        revision = int(match.group(1))
        changes = [{"kind": "edit", "path": "one.txt", "expected_read_revision": revision,
                    "edits": [{"kind": "replace_exact", "old_text": "before", "new_text": "after"}]}]
        initial_sha = digest(self.project_dir / "one.txt")
        self.sample("edit_dry_run", "edit_project_files",
                    {"project": self.project, "dry_run": True, "changes": changes})
        check(digest(self.project_dir / "one.txt") == initial_sha, "dry run modified the file")
        self.sample("edit_applied", "edit_project_files",
                    {"project": self.project, "changes": changes})
        check((self.project_dir / "one.txt").read_text() == "after one\n",
              "native Runner edit did not apply expected contents")
        after_sha = digest(self.project_dir / "one.txt")
        self.sample("edit_stale", "edit_project_files",
                    {"project": self.project, "changes": changes}, error=True)
        check(digest(self.project_dir / "one.txt") == after_sha, "stale edit modified the file")

        self.sample("shell_success", "run_shell",
                    {"project": self.project, "command": "printf 'hello\\n'", "timeout_secs": 30})
        self.sample("shell_failure", "run_shell",
                    {"project": self.project, "command": "printf 'denied\\n' >&2; exit 7",
                     "timeout_secs": 30}, error=True)
        (self.project_dir / "subdir").mkdir()
        configured = self.sample("shell_nondefault", "run_shell", {
            "project": self.project, "session_id": self.session_ref,
            "cwd": "subdir", "shell": "bash", "timeout_secs": 30,
            "command": "printf '%s\\n' \"$PWD\" \"$BASH_VERSION\"",
        })
        check(any(str(path / "subdir") in configured for path in
                  [self.project_dir, self.project_dir.resolve()]),
              "explicit cwd did not determine the actual command directory")
        check("cwd: subdir" in configured and "shell: bash" in configured,
              "CLI hid semantically explicit execution configuration")
        timed_out = self.sample("shell_timeout", "run_shell", {
            "project": self.project, "command": "printf 'partial\\n'; sleep 3", "timeout_secs": 1,
        }, error=True)
        check("timed_out" in timed_out and "partial" in timed_out,
              "timeout lost execution state or actual partial output")

        started = self.sample("job_started", "run_job", {
            "project": self.project, "command": "printf 'job-first\\njob-last\\n'",
            "timeout_secs": 30,
        })
        job = job_identity(started)
        self.sample("job_ready", "wait_for_job_readiness",
                    {"job_ids": [job], "mode": "all", "wait_secs": 30})
        observed = self.sample("job_observed", "observe_jobs",
                               {"items": [{"job_id": job}], "tail_lines": 1})
        check("job-last" in observed and job in observed, "Job logs/identity lost")
        partial = self.sample("jobs_partial_unknown", "observe_jobs", {
            "items": [{"job_id": "missing-fixture-job"}, {"job_id": job}], "tail_lines": 1,
        })
        check("unknown_job" in partial and "missing-fixture-job" in partial,
              "unknown Job lost failure identity")
        check(not any(line.strip() == "null" for line in partial.splitlines()),
              "partial Job failure emitted a meaningless null")

        restricted = self.pat("input-owner", ["runtime:read"])
        self.extra_secrets.append(restricted)
        self.sample("pat_scope_denied", "run_shell", {
            "project": self.project, "command": "printf forbidden > scope-effect.txt",
        }, token=restricted, http_denial=True)
        check(not (self.project_dir / "scope-effect.txt").exists(),
              "scope denial executed a command")

        lsp = {"state": "skipped", "reason": "installed rust-analyzer unavailable"}
        if shutil.which("rust-analyzer"):
            (self.project_dir / "Cargo.toml").write_text(
                '[package]\nname="cli-output-fixture"\nversion="0.0.0"\nedition="2021"\n')
            (self.project_dir / "src/lib.rs").write_text("pub fn fixture_symbol() {}\n")
            # One bounded request; no installation or indexing bypass. An explicit
            # readiness failure is useful evidence, not a successful symbol sample.
            params = {"project": self.project, "query": "fixture_symbol", "limit": 5}
            self.sample("workspace_symbols", "list_workspace_symbols", params, error=None)
            measured = self.samples[-1]
            lsp = {"state": "unavailable" if measured["is_error"] else "ready",
                   "http_status": measured["http_status"]}
            print(json.dumps({"workspace_symbols": lsp}, sort_keys=True))
        else:
            print(json.dumps({"workspace_symbols": lsp}, sort_keys=True))
        return {"samples": self.samples, "workspace_symbols": lsp,
                "edit_oracle": {"dry_run_unchanged": True, "actual_edit_applied": True,
                                "stale_edit_unchanged": True},
                "scope_oracle": {"denied_before_execution": True},
                "measurement": "real loopback MCP CLI UTF-8/JSON bytes; no Host tokens or latency"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    parser.add_argument("--output-dir", type=Path,
                        help="new directory for fixture responses and byte ledger; must not exist")
    args = parser.parse_args()
    check(os.name == "posix", "this opt-in real-process fixture requires POSIX")
    check(shutil.which("git") is not None and shutil.which("bash") is not None,
          "git and bash must already be installed")
    bin_dir = args.bin_dir.resolve()
    for binary in ["webcodex-server", "webcodex-runner"]:
        check((bin_dir / binary).is_file(), "build dogfood Server/Runner binaries first")
    output_dir = args.output_dir.resolve() if args.output_dir else None
    if output_dir:
        output_dir.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="webcodex-cli-output-") as root:
        smoke = CliRuntimeSmoke(bin_dir, Path(root), output_dir)
        try:
            ledger = smoke.run_samples()
            ledger["binaries"] = {name: digest(bin_dir / name)
                                  for name in ["webcodex-server", "webcodex-runner"]}
            if output_dir:
                (output_dir / "measurements.json").write_text(
                    json.dumps(ledger, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            print("PASS: isolated native edits, execution, Jobs, scope denial and CLI output capture")
        finally:
            smoke.close()


if __name__ == "__main__":
    main()
