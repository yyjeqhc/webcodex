#!/usr/bin/env python3
"""Opt-in deterministic generic-Agent acceptance; not a ChatGPT/model benchmark.

Uses disposable loopback Server/Runner from e2e_job_input_ws. No real project,
production service, paid endpoint or model is involved. Build binaries separately.
Run each source with identical fixture scripts; compare report metrics, not tokens.
"""
from __future__ import annotations
import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
import subprocess
import struct
import sys
import tempfile
import time
import zlib
from urllib.error import HTTPError
from urllib.request import Request
from e2e_job_input_ws import InteractiveSmoke, SCOPES
from e2e_session_continuity_ws import ROOT, check

BASELINE = "05d45f376d3265490de28090b9c5b0150dbacd8b"
UPSTREAM = "05d45f37"
SENTINEL = "GENERIC_ACCEPTANCE_UNIQUE_SENTINEL_9d06"
EXTERNAL_SENTINEL = "EXTERNAL_WRITER_SENTINEL_c741"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def binary_fixtures():
    """Small deterministic valid raster/document fixtures; no external assets."""
    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff))
    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(b"\0\xff\0\0")) + chunk(b"IEND", b""))
    objects = [b"<< /Type /Catalog /Pages 2 0 R >>",
               b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
               b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> /Contents 4 0 R >>",
               b"<< /Length 0 >>\nstream\n\nendstream"]
    pdf = b"%PDF-1.4\n"
    offsets = []
    for number, data in enumerate(objects, 1):
        offsets.append(len(pdf))
        pdf += f"{number} 0 obj\n".encode() + data + b"\nendobj\n"
    xref = len(pdf)
    pdf += b"xref\n0 5\n0000000000 65535 f \n"
    for offset in offsets:
        pdf += f"{offset:010d} 00000 n \n".encode()
    pdf += f"trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    return {"pixel.png": png, "document.pdf": pdf, "bytes.bin": bytes(range(256)) * 4}


def totals(calls):
    return {"meaningful_outer_calls": len(calls),
            "actual_mcp_response_bytes": sum(c["response_bytes"] for c in calls),
            "actual_request_bytes": sum(c["request_bytes"] for c in calls),
            "tool_failures": sum(c["success"] is False for c in calls),
            "wall_seconds": round(sum(c["wall_seconds"] for c in calls), 6)}


class GenericSmoke(InteractiveSmoke):
    def __init__(self, args, root):
        super().__init__(args.bin_dir.resolve(), root)
        self.env.update({"LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "TZ": "UTC",
                         "PYTHONHASHSEED": "0", "PYTHONNOUSERSITE": "1"})
        for key in ("PYTHONPATH", "PYTHONHOME"):
            self.env.pop(key, None)
        self.args = args
        self.calls, self.scenarios, self.extra_checks = [], [], []
        self.scenario = "setup"
        self.app_fixture = False
        self.binaries = {"webcodex-server": args.server, "webcodex-runner": args.runner}

    def launch(self, binary, args, env):
        # Reuse the existing lifecycle and log ownership, with explicit per-binary paths.
        selected = self.binaries.get(binary)
        if selected is None:
            return super().launch(binary, args, env)
        old = self.bin_dir
        try:
            self.bin_dir = selected.resolve().parent
            return super().launch(selected.name, args, env)
        finally:
            self.bin_dir = old

    def post(self, path, body, token):
        if path != "/mcp":
            return super().post(path, body, token)
        params = body.setdefault("params", {})
        params["_meta"] = {"io.modelcontextprotocol/protocolVersion": "2026-07-28",
                           "io.modelcontextprotocol/clientCapabilities": {}}
        if self.app_fixture:
            params["_meta"].update({
                "io.modelcontextprotocol/clientCapabilities": {"extensions": {
                    "io.modelcontextprotocol/ui": {"mimeTypes": ["text/html;profile=mcp-app"]}}},
                "io.modelcontextprotocol/clientInfo": {"name": "generic-acceptance-app-fixture", "version": "1"},
                "openai/session": "generic-acceptance-app-fixture-window"})
        payload = json.dumps(body).encode()
        headers = {"Authorization": "Bearer " + token, "Content-Type": "application/json",
                   "Accept": "application/json, text/event-stream", "MCP-Protocol-Version": "2026-07-28",
                   "MCP-Method": body["method"]}
        if "name" in params:
            headers["MCP-Name"] = params["name"]
        started = time.monotonic()
        request = Request(self.url + path, payload, method="POST", headers=headers)
        try:
            with self.opener.open(request, timeout=40) as response:
                raw = response.read(2 * 1024 * 1024 + 1)
        except HTTPError as error:
            raw = error.read(2 * 1024 * 1024 + 1)
        check(len(raw) <= 2 * 1024 * 1024, "MCP response exceeded fixture capture bound")
        reply = json.loads(raw)
        if body["method"] == "tools/call":
            result = reply.get("result", {})
            self.calls.append({"scenario": self.scenario, "tool": params["arguments"].get("tool", params["name"]), "surface": params["name"],
                "request_bytes": len(payload), "response_bytes": len(raw),
                "wall_seconds": round(time.monotonic() - started, 6),
                "success": result.get("structuredContent", {}).get("success"),
                "client_role": "independent_mcp_app_fixture" if self.app_fixture else "model_tool_fixture",
                "content_types": [c.get("type") for c in result.get("content", [])],
                "response_sha256": hashlib.sha256(raw).hexdigest(),
                "sentinel_occurrences": raw.count(SENTINEL.encode())})
        return reply

    def prepare(self):
        self.env["WEBCODEX_MCP_HOST_PROFILE"] = "host_code_mode"
        self.start_server()
        check(self.post("/api/users/create", {"username": "generic-owner"}, self.bootstrap).get("success"), "create user")
        self.owner = self.pat("generic-owner", SCOPES)
        agent = self.post("/api/agent-tokens/create", {"username": "generic-owner", "client_id": "generic-runner", "name": "generic-fixture"}, self.bootstrap)
        check(isinstance(agent.get("token"), str), "Runner enrollment")
        self.project_dir = self.root / "project"
        self.code_dir = self.root / "code"
        registry = self.root / "projects"
        for directory in [self.project_dir, self.code_dir, registry]:
            directory.mkdir()
        for name, directory in [("data", self.project_dir), ("code", self.code_dir)]:
            (registry / f"{name}.toml").write_text(f'id = "{name}"\npath = {json.dumps(str(directory))}\n')
        (self.code_dir / "calc.py").write_text("def add(a, b):\n    return a - b\n")
        child_env = self.env | {"GIT_AUTHOR_NAME": "Fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
            "GIT_COMMITTER_NAME": "Fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid",
            "GIT_AUTHOR_DATE": "2026-01-01T00:00:00Z", "GIT_COMMITTER_DATE": "2026-01-01T00:00:00Z"}
        for argv in [["git", "init", "-q", "-b", "main"], ["git", "add", "calc.py"], ["git", "commit", "-qm", "fixture"]]:
            subprocess.run(argv, cwd=self.code_dir, env=child_env, stdin=subprocess.DEVNULL,
                           stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, check=True)
        self.initial_code_sha = sha(self.code_dir / "calc.py")
        config = self.root / "runner.toml"
        config.write_text('\n'.join([f'server_url = "{self.url}"', f'token = {json.dumps(agent["token"])}',
            'client_id = "generic-runner"', 'owner = "generic-owner"', 'transport = "websocket"',
            f'project_registry_dir = {json.dumps(str(registry))}', '[policy]', 'allow_raw_shell = true',
            f'allowed_roots = {json.dumps([str(self.project_dir), str(self.code_dir)])}']) + '\n')
        config.chmod(0o600)
        self.runner = self.launch("webcodex-runner", ["--config", str(config)], self.env)
        self.project = "agent:generic-runner:data"
        self.wait_ready(self.project, self.owner)
        self.initialize(self.owner)
        started = self.call("work_on_project", {"project": self.project, "instruction": "Disposable general task acceptance"}, direct=True)
        self.project_ref, self.session_ref = started["project_ref"], started["session_ref"]

    def case(self, name, operation, *, extra=False):
        self.scenario = name
        started, offset = time.monotonic(), len(self.calls)
        result = {"name": name}
        try:
            result["evidence"] = operation()
            result["correctness"] = "passed"
        except Exception as error:
            result.update(correctness="failed", failure=f"{type(error).__name__}: {error}")
        result.update(totals(self.calls[offset:]))
        result["wall_seconds"] = round(time.monotonic() - started, 6)
        (self.extra_checks if extra else self.scenarios).append(result)
        check(result["correctness"] == "passed", result.get("failure", name))

    def script(self, script, *, success=True, purpose=None):
        args = {"project": self.project_ref, "session_id": self.session_ref,
                "language": "python", "script": script, "timeout_secs": 30}
        if purpose:
            args["purpose"] = purpose
        out = self.call("run_script", args, direct=True, success=success)
        if out.get("execution_state") == "pending":
            out = self.terminal(out["job_id"])
        return out

    def files(self):
        src = self.project_dir / "input"
        src.mkdir()
        (src / "note.txt").write_text("original note\n")
        for name, payload in binary_fixtures().items():
            (src / name).write_bytes(payload)
        destination = self.project_dir / "sorted"
        destination.mkdir()
        (destination / "note.txt").write_text("keep conflict\n")
        original = {p.name: sha(p) for p in src.iterdir()}
        for name in binary_fixtures():
            self.call("transfer_project_artifact", {"source_project": self.project_ref,
                "source_path": "input/" + name, "destination_project": self.project_ref,
                "destination_path": "sorted/" + name}, direct=False)
        self.script("from pathlib import Path\nimport shutil\nsrc=Path('input/note.txt'); dst=Path('sorted/note.txt')\nif dst.exists(): dst=dst.with_name('note-2.txt')\nshutil.copyfile(src,dst)\n")
        check({p.name: sha(p) for p in src.iterdir()} == original, "input changed")
        for name in binary_fixtures():
            check(sha(destination / name) == original[name], "binary transfer SHA: " + name)
        check(sha(destination / "note-2.txt") == original["note.txt"], "text copy SHA")
        check((destination / "note.txt").read_text() == "keep conflict\n", "conflict overwritten")
        check(len(list(src.iterdir())) == 4 and len(list(destination.iterdir())) == 5, "independent file counts")
        return {"input_sha256": original, "output_sha256": {p.name: sha(p) for p in destination.iterdir()},
                "input_count": 4, "output_count": 5, "conflict_preserved": True, "binary_path": "artifact transfer"}

    def data(self):
        (self.project_dir / "a.csv").write_text("name,value\nAlice,2\nBob,3\n", encoding="utf-8")
        (self.project_dir / "b.csv").write_bytes("姓名,数量\n陈,5\n丁,7\n".encode("utf-16"))
        inputs = {p: sha(self.project_dir / p) for p in ["a.csv", "b.csv"]}
        self.script("import csv,json\nrows=[]\nfor file,encoding,n,v in [('a.csv','utf-8','name','value'),('b.csv','utf-16','姓名','数量')]:\n with open(file,encoding=encoding,newline='') as f:\n  rows.extend({'name':r[n],'value':int(r[v])} for r in csv.DictReader(f))\nwith open('merged.csv','w',encoding='utf-8',newline='') as f:\n w=csv.DictWriter(f,fieldnames=['name','value'],lineterminator='\\n');w.writeheader();w.writerows(rows)\nwith open('report.json','w',encoding='utf-8') as f: json.dump({'count':len(rows),'total':sum(r['value'] for r in rows)},f)\n")
        expected = "name,value\nAlice,2\nBob,3\n陈,5\n丁,7\n".encode()
        actual = (self.project_dir / "merged.csv").read_bytes()
        check(actual == expected, "independent CSV bytes")
        rows = list(csv.DictReader(io.StringIO(actual.decode("utf-8"))))
        check(len(rows) == 4 and sum(int(r["value"]) for r in rows) == 17, "independent counts")
        check(json.loads((self.project_dir / "report.json").read_text()) == {"count": 4, "total": 17}, "report counts")
        check(inputs == {p: sha(self.project_dir / p) for p in inputs}, "CSV inputs changed")
        finish_args = {"project": self.project_ref, "session_id": self.session_ref, "summary_only": True}
        if self.args.profile == "upgraded":
            finish_args["outputs"] = ["merged.csv", "report.json"]
        finish = self.call("finish_coding_task", finish_args, direct=False)
        if self.args.profile == "upgraded":
            receipt = finish.get("task_outputs", {})
            check(receipt.get("verified_count") == 2 and receipt.get("missing_count") == 0
                  and receipt.get("unavailable_count") == 0, "finish did not verify both outputs")
            check({item.get("path") for item in receipt.get("items", [])} == {"merged.csv", "report.json"},
                  "finish receipt paths disagree with requested outputs")
            for item in receipt.get("items", []):
                check(item.get("status") == "verified" and item.get("sha256") == sha(self.project_dir / item["path"]),
                      "finish output SHA disagrees with independent file check")
        return {"count": 4, "total": 17, "input_sha256": inputs,
                "output_sha256": sha(self.project_dir / "merged.csv"),
                "finish_outputs_requested": self.args.profile == "upgraded"}

    def failure_logs(self):
        failed = self.script(f"import sys\nfor i in range(2500): print('pressure-' + str(i))\nprint({SENTINEL!r})\nprint('expected failure',file=sys.stderr)\nsys.exit(7)\n", success=False)
        check(failed.get("exit_code") == 7, "nonzero command lost exit status")
        fixed = self.script(f"for i in range(2500): print('pressure-' + str(i))\nprint({SENTINEL!r})\n")
        check(fixed.get("exit_code", 0) == 0, "repair failed")
        calls = [c for c in self.calls if c["scenario"] == self.scenario]
        check(any(c["sentinel_occurrences"] for c in calls), "unique tail sentinel missing from actual MCP envelope")
        return {"expected_exit": 7, "repair_exit": 0,
                "sentinel_occurrences_per_response": [c["sentinel_occurrences"] for c in calls]}

    def pending(self):
        offset = len(self.calls)
        job = self.start_pipe("import sys; sys.stdin.read(); print('controlled-job-complete',flush=True)", 30)
        self.script("from pathlib import Path\nPath('independent.txt').write_text('independent work completed')\n")
        self.write(job, "release", close=True)
        item = self.terminal(job)  # Exactly one readiness join, then observe the SAME Job.
        check(item.get("job_id") == job, "terminal observation replaced the original Job")
        check(item.get("exit_code") == 0, "controlled Job failed")
        check((self.project_dir / "independent.txt").read_bytes() == b"independent work completed",
              "independent action bytes disagree")
        tools = [call["tool"] for call in self.calls[offset:]]
        check(tools == ["run_process", "run_script", "job_write_input", "wait_for_job_readiness", "observe_jobs"],
              "pending scenario call order or dispatch/wait count changed")
        return {"job_id": job, "redispatches": tools.count("run_process") - 1,
                "readiness_joins": tools.count("wait_for_job_readiness"),
                "order": ["pending", "independent action", "release input", "readiness join", "observe same Job"]}

    def coding(self):
        started = self.call("work_on_project", {"project": "agent:generic-runner:code", "instruction": "Repair small code and review"}, direct=True)
        saved = self.project_ref, self.session_ref
        self.project_ref, self.session_ref = started["project_ref"], started["session_ref"]
        try:
            first = self.script("from pathlib import Path\nnamespace={}\nexec(compile(Path('calc.py').read_text(),'calc.py','exec'),namespace)\nassert namespace['add'](2,3)==5\n", success=False, purpose="validation")
            check(first.get("exit_code") != 0, "negative validation unexpectedly passed")
            before = self.call("read_files", {"project": self.project_ref, "items": [{"path": "calc.py"}]}, direct=True)
            revision = before["items"][0]["output"]["read_revision"]
            self.call("edit_project_files", {"project": self.project_ref, "_wc": {"record": self.session_ref}, "changes": [{"kind": "edit", "path": "calc.py", "expected_read_revision": revision,
                "edits": [{"kind": "replace_exact", "old_text": "return a - b", "new_text": "return a + b"}]}]}, direct=True)
            repaired = self.script("from pathlib import Path\nnamespace={}\nexec(compile(Path('calc.py').read_text(),'calc.py','exec'),namespace)\nassert namespace['add'](2,3)==5\n", purpose="validation")
            check(repaired.get("exit_code", 0) == 0, "repaired validation failed")
            reviewed = self.call("review_changes", {"project": self.project_ref, "session_id": self.session_ref, "scope": {"kind": "workspace"}, "paths": ["calc.py"]}, direct=True)
            check(bool(reviewed), "empty review")
            check((self.code_dir / "calc.py").read_text() == "def add(a, b):\n    return a + b\n", "independent code check")
            return {"initial_sha256": self.initial_code_sha, "final_sha256": sha(self.code_dir / "calc.py"), "review": "real Git fixture"}
        finally:
            self.project_ref, self.session_ref = saved

    def stale_edit(self):
        path = self.project_dir / "stale.txt"
        initial = b"task value: before\n"
        path.write_bytes(initial)
        read_args = {"project": self.project_ref, "items": [{"path": "stale.txt"}]}
        before = self.call("read_files", read_args, direct=True)
        revision = before["items"][0]["output"]["read_revision"]
        # Fixture-side write bypasses all tool caches: model/tool success alone
        # cannot prove preservation of a concurrent external writer's change.
        external = initial + (EXTERNAL_SENTINEL + "\n").encode()
        path.write_bytes(external)
        changes = [{"kind": "edit", "path": "stale.txt", "expected_read_revision": revision,
            "edits": [{"kind": "replace_exact", "old_text": "task value: before",
                       "new_text": "task value: after"}]}]
        edit_args = {"project": self.project_ref, "_wc": {"record": self.session_ref}, "changes": changes}
        rejected = self.call("edit_project_files", edit_args, direct=True, success=False)
        check(rejected.get("error_kind") == "stale_file_revision", "stale edit rejection kind")
        check(rejected.get("state_changed") is False, "stale edit claimed a mutation")
        check(path.read_bytes() == external, "stale edit changed external writer bytes")
        fresh = self.call("read_files", read_args, direct=True)
        fresh_revision = fresh["items"][0]["output"]["read_revision"]
        check(fresh_revision != revision, "fresh read retained stale revision")
        changes[0]["expected_read_revision"] = fresh_revision
        self.call("edit_project_files", edit_args, direct=True)
        expected = external.replace(b"task value: before", b"task value: after")
        check(path.read_bytes() == expected, "fresh edit lost external writer bytes or task change")
        return {"initial_sha256": hashlib.sha256(initial).hexdigest(),
                "external_sha256": hashlib.sha256(external).hexdigest(), "final_sha256": sha(path),
                "rejected_error_kind": rejected["error_kind"], "rejected_bytes_unchanged": True,
                "external_change_preserved": True, "fresh_read_before_successful_edit": True}

    def missing_output(self):
        check(not (self.project_dir / "missing-result.csv").exists(), "negative fixture unexpectedly exists")
        started = self.call("work_on_project", {"project": self.project, "instruction": "Verify missing-output closeout blocking"}, direct=True)
        output = self.call("finish_coding_task", {"project": started["project_ref"],
            "session_id": started["session_ref"], "summary_only": True,
            "outputs": ["missing-result.csv"]}, direct=False)
        receipt = output.get("task_outputs", {})
        outcome = output.get("task_outcome", {})
        check(not (self.project_dir / "missing-result.csv").exists(), "negative fixture unexpectedly exists")
        check(receipt.get("missing_count") == 1 and receipt.get("verified_count") == 0,
              "missing output falsely verified")
        check("task_outputs_unverified" in outcome.get("blocking_reasons", []),
              "missing output has no explicit unverified-output blocking reason")
        check(outcome.get("status") == "fail" and outcome.get("blocking") is True,
              "missing output failed to block task outcome")
        return {"task_outcome": outcome, "task_outputs": receipt}

    def app_state(self, project, session):
        """Independent MCP App host fixture, never the model gateway path."""
        self.app_fixture = True
        try:
            self.rpc_id += 1
            listed = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
                "method": "tools/list", "params": {}}, self.owner)
            state = next((tool for tool in listed.get("result", {}).get("tools", [])
                          if tool.get("name") == "work_result_state"), {})
            check(state.get("_meta", {}).get("ui", {}).get("visibility") == ["app"],
                  "independent UI-capable client did not discover App-only WorkResult")
            return self.call("work_result_state", {"project": project, "session_id": session}, direct=True)
        finally:
            self.app_fixture = False

    def large_output(self):
        path = self.project_dir / "big.bin"
        path.write_bytes(b"x" * (10 * 1024 * 1024 + 1))
        expected_sha, expected_size = sha(path), path.stat().st_size
        # Bootstrap exact Sessions independently; no implicit Session/Window affinity.
        def bootstrap(instruction):
            started = self.post("/api/tools/call", {"tool": "work_on_project", "params": {
                "project": self.project, "instruction": instruction}}, self.owner)
            check(started.get("success") is True, "large-output API Session bootstrap failed")
            identity = started.get("output", {}).get("session_id")
            check(isinstance(identity, str) and identity.startswith("wc_sess_"), "API bootstrap omitted exact Session identity")
            return identity
        session = bootstrap("Verify large output metadata and retained WorkResult")
        output = self.call("finish_coding_task", {"project": self.project,
            "session_id": session, "summary_only": True, "outputs": ["big.bin"]}, direct=False)
        receipt = output.get("task_outputs", {})
        check(receipt.get("verified_count") == 1 and receipt.get("missing_count") == 0
              and receipt.get("unavailable_count") == 0, "10MiB+1 output was not verified")
        items = receipt.get("items", [])
        check(len(items) == 1, "large-output receipt item count")
        item = items[0]
        check(item.get("path") == "big.bin" and item.get("status") == "verified"
              and item.get("file_bytes") == expected_size and item.get("sha256") == expected_sha,
              "large-output metadata disagrees with independent SHA/size")
        refreshed = self.app_state(self.project, session)
        retained = refreshed.get("work_result", {}).get("task_outputs")
        check(retained == receipt, f"WorkResult did not retain the exact observed output receipt: expected={receipt!r}; retained={retained!r}; output_keys={list(refreshed)}")
        # W already owns a different valid receipt. Recording C in W must neither
        # attribute C's output to W nor invalidate W's earlier business finish.
        recorder = bootstrap("Verify recorder/business Session isolation")
        check(recorder != session, "API bootstrap reused a Session unexpectedly")
        own_path = self.project_dir / "recorder-owned.txt"
        own_path.write_bytes(b"recorder-owned-result\n")
        own_finish = self.call("finish_coding_task", {"project": self.project,
            "session_id": recorder, "summary_only": True, "outputs": ["recorder-owned.txt"]}, direct=False)
        own_receipt = own_finish.get("task_outputs", {})
        own_items = own_receipt.get("items", [])
        check(own_receipt.get("verified_count") == 1 and len(own_items) == 1
              and own_items[0].get("path") == "recorder-owned.txt"
              and own_items[0].get("sha256") == sha(own_path)
              and own_items[0].get("file_bytes") == own_path.stat().st_size,
              "recorder's independent receipt was not verified")
        check(self.app_state(self.project, recorder).get("work_result", {}).get("task_outputs") == own_receipt,
              "recorder's own business finish was not retained")
        foreign_finish = self.call("finish_coding_task", {"project": self.project,
            "session_id": session, "summary_only": True, "outputs": ["big.bin"],
            "_wc": {"record": recorder}}, direct=False)
        foreign_receipt = foreign_finish.get("task_outputs", {})
        check(foreign_receipt.get("items") == receipt.get("items"), "foreign recorder changed verified metadata")
        check(self.app_state(self.project, session).get("work_result", {}).get("task_outputs") == foreign_receipt,
              "business Session lost its receipt with an external recorder")
        check(self.app_state(self.project, recorder).get("work_result", {}).get("task_outputs") == own_receipt,
              "foreign business finish replaced or invalidated recorder's own receipt")
        # A latest finish in C that omits outputs intentionally clears C's receipt.
        self.call("finish_coding_task", {"project": self.project,
            "session_id": session, "summary_only": True}, direct=False)
        check(self.app_state(self.project, session).get("work_result", {}).get("task_outputs") is None,
              "latest business finish without outputs reused an old receipt")
        check(self.app_state(self.project, recorder).get("work_result", {}).get("task_outputs") == own_receipt,
              "business omission changed the unrelated recorder receipt")
        return {"path": "big.bin", "file_bytes": expected_size, "sha256": expected_sha,
                "task_outputs": receipt, "work_result_retained": True,
                "normal_finish_requires_extra_recorder": False,
                "foreign_recorder_preserved_own_receipt": True,
                "foreign_finish_retained_in_business_session": True,
                "latest_business_omission_clears_receipt": True,
                "work_result_transport": "independent UI-capable stateless MCP App fixture with explicit synthetic host Window; real Host rendering unmeasured",
                "canonical_api_calls_outside_mcp_metrics": 2}



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    parser.add_argument("--server", type=Path)
    parser.add_argument("--runner", type=Path)
    parser.add_argument("--profile", choices=["baseline", "upgraded"], required=True)
    parser.add_argument("--source-version", required=True, help="Actual source/build identity, including dirty state")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    for name in ["server", "runner"]:
        path = getattr(args, name) or args.bin_dir / f"webcodex-{name}"
        check(path.is_file(), f"missing {name} binary: {path}")
        setattr(args, name, path.resolve())
    report = {"kind": "deterministic_tool_acceptance_not_model_benchmark", "profile": args.profile,
        "source_version": args.source_version, "baseline_reference": BASELINE, "upstream_reference": UPSTREAM,
        "binary_sha256": {name: sha(getattr(args, name)) for name in ["server", "runner"]},
        "fixture_script_sha256": sha(Path(__file__)),
        "expected_negative_cases": ["exit 7 command", "broken add validation", "stale read_revision edit"],
        "comparison_caveat": "Use upgraded for both current versions to compare identical output assertions. The baseline profile and baseline_reference identify the historical pre-output contract; do not pool different profiles. source_version identifies the actual build.",
        "measurement": "Actual JSON-RPC MCP response body bytes, including all content blocks and structuredContent; no credential/header capture",
        "unmeasured": {"desktop_browser": "Outside this loopback file/command acceptance; real Desktop/browser/ChatGPT Host behavior unmeasured",
                       "existing_regressions_not_run": ["scripts/tests/test_desktop_runtime_manifest.py",
                           "crates/webcodex-core/src/desktop_runtime_contract/tests.rs",
                           "crates/webcodex-browser/src/supervisor/tests/batch.rs",
                           "plugins/agent-browser/tests/core.test.mjs"]}}
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="webcodex-generic-agent-") as directory:
        smoke = GenericSmoke(args, Path(directory).resolve())
        try:
            smoke.prepare()
            for name, operation in [("mixed_files_conflict", smoke.files), ("encoded_csv", smoke.data),
                    ("failure_repair_log_pressure", smoke.failure_logs), ("pending_independent_join", smoke.pending),
                    ("code_repair_review", smoke.coding), ("stale_read_revision", smoke.stale_edit)]:
                smoke.case(name, operation)
            if args.profile == "upgraded":
                smoke.case("extra_missing_output", smoke.missing_output, extra=True)
                smoke.case("extra_large_output_retention", smoke.large_output, extra=True)
            report["correctness"] = "passed"
        except Exception as error:
            report.update(correctness="failed", failure=f"{type(error).__name__}: {error}")
        finally:
            smoke.close()
            report.update(scenarios=smoke.scenarios, extra_checks=smoke.extra_checks, calls=smoke.calls,
                totals=totals([c for c in smoke.calls if c["scenario"] != "setup" and not c["scenario"].startswith("extra_")]),
                setup_totals=totals([c for c in smoke.calls if c["scenario"] == "setup"]))
    report["wall_seconds"] = round(time.monotonic() - started, 6)
    report["comparable_task_wall_seconds"] = round(sum(s["wall_seconds"] for s in smoke.scenarios), 6)
    report["extra_checks_wall_seconds"] = round(sum(s["wall_seconds"] for s in smoke.extra_checks), 6)
    report["timing_caveat"] = "wall_seconds includes setup/cleanup and extra_checks; comparable_task_wall_seconds and totals exclude extra_checks"
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"{report['correctness']}: {args.report}")
    return 0 if report["correctness"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
