#!/usr/bin/env python3
"""Scripted real MCP/WS knowledge+Skill verifier integration, never model proof."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from urllib.request import ProxyHandler, build_opener

try:
    from scripts.agent_knowledge_fixture import KnowledgeFixture, CASE_IDS, SKILL
    from scripts.agent_knowledge_host import HostSample, SCOPES, runtime_identity, read_private, write_private
    from scripts.e2e_generic_agent_ws import GenericSmoke
    from scripts import agent_knowledge_evidence as evidence, agent_temporal_evidence as trace, agent_loop_report as report, agent_loop_benchmark as benchmark
except ModuleNotFoundError:
    from agent_knowledge_fixture import KnowledgeFixture, CASE_IDS, SKILL
    from agent_knowledge_host import HostSample, SCOPES, runtime_identity, read_private, write_private
    from e2e_generic_agent_ws import GenericSmoke
    import agent_knowledge_evidence as evidence
    import agent_temporal_evidence as trace
    import agent_loop_report as report
    import agent_loop_benchmark as benchmark


class RecordedSample(HostSample):
    """Reuse the existing explicit MCP App fixture identity, not a real Host."""
    app_fixture = True
    scenario = "knowledge_scripted_fixture"

    def post(self, path, body, token):
        if path == "/mcp":
            return GenericSmoke.post(self, path, body, token)
        return super().post(path, body, token)

    def raw_tool(self, name, args, token, *, direct=False):
        args = dict(args)
        if getattr(self, "session_ref", None):
            args.setdefault("_wc", {"record": self.session_ref})
        return super().raw_tool(name, args, token, direct=direct)

    def plugin(self, arguments, *, success=True):
        # Native rejected Plugin calls legally omit structuredContent. The
        # generic smoke raw_tool helper expects ordinary ToolResult envelopes.
        self.rpc_id += 1
        reply = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
            "method": "tools/call", "params": {"name": "plugin_tool",
            "arguments": dict(arguments, _wc={"record": self.session_ref})}}, self.owner)
        result = reply.get("result", {})
        if type(result.get("isError")) is not bool:
            raise AssertionError("missing native Plugin outcome")
        if (result.get("isError") is True) == success:
            raise AssertionError("unexpected native Plugin result")
        return result.get("structuredContent", {})

    def source_reads(self, fixture, *, initial=False):
        if fixture.case_id in evidence.DIRECTED_KNOWLEDGE:
            if initial:
                binding = self.plugin({"action": "describe", "runner": self.client_id,
                    "plugin": "project-knowledge", "tool": "index", "project": self.project})["binding"]
                self.plugin({"action": "call", "binding": binding, "arguments": {}})
            else:
                binding = self.plugin({"action": "describe", "runner": self.client_id,
                    "plugin": "project-knowledge", "tool": "index", "project": self.project})["binding"]
                self.plugin({"action": "call", "binding": binding, "arguments": {"mode": "refresh"}})
            binding = self.plugin({"action": "describe", "runner": self.client_id,
                "plugin": "project-knowledge", "tool": "search", "project": self.project})["binding"]
            self.plugin({"action": "call", "binding": binding, "arguments": {"query": "ledger"}})
            self.old_source = fixture.snapshot_sources(0)[0]
        elif fixture.case_id in evidence.DIRECTED_SKILL:
            if fixture.case_id == "knowledge_skill_read":
                catalog = self.call("list_skills", {"project": self.project_ref})
                selected = [item for item in catalog["skills"] if item.get("name") == "receipt-audit"]
                if len(selected) != 1:
                    raise AssertionError("expected one exact project Skill")
                definition = self.call("read_skill_file", {"project": self.project_ref,
                    "skill_id": selected[0]["skill_id"], "path": "SKILL.md"}, direct=True)
            else:
                definition = self.call("load_skill", {"project": self.project_ref, "name": "receipt-audit"}, direct=True)
            self.old_definition = definition
            reference = self.call("read_skill_file", {"project": self.project_ref, "skill_id": definition["skill_id"],
                "path": "reference.md", "expected_definition_revision": definition["definition_revision"]}, direct=True)
            for output, skill_path in ((definition, "SKILL.md"), (reference, "reference.md")):
                if (output.get("skill_path") != skill_path or "path" in output
                        or output.get("project_path") != SKILL.rsplit("/", 1)[0] + "/" + skill_path):
                    raise AssertionError("Skill read did not return canonical citation locators")
        else:
            self.call("read_files", {"project": self.project_ref,
                "items": [{"path": path} for path in fixture.required_sources],
                "with_line_numbers": True}, direct=True)
            if fixture.case_id == "knowledge_identifier":
                self.call("post_session_message", {"session_id": self.session_ref,
                    "kind": "progress", "message": "Scripted verifier marker; no operator handoff."})


def host_handoff_smoke(bin_dir, source_revision, manifest):
    """Exercise real driver handoff through the existing runner, with no model."""
    with tempfile.TemporaryDirectory(prefix="webcodex-knowledge-host-smoke-") as temporary:
        root = Path(temporary)
        state = root / "state"; state.mkdir(mode=0o700)
        for name in ("worktrees", "receipts", "annotations"):
            (root / name).mkdir()
        config = state / "model.json"
        write_private(config, {"host": "chatgpt_web", "test_only": "scripted handoff, no model",
            "mcp_apps_enabled": False})
        # Restore an existing account rather than silently enrolling the Runner
        # under the default owner. Its PAT must actually see the new Project.
        seed_root, seed_capture = root / "seed", root / "seed-capture"
        seed_root.mkdir(mode=0o700); seed_capture.mkdir(mode=0o700)
        seed = HostSample(bin_dir, state, seed_root, root, seed_capture)
        try:
            seed.start_server()
            username = "restored-fixture-owner"
            if seed.post("/api/users/create", {"username": username}, seed.bootstrap).get("success") is not True:
                raise AssertionError("existing owner setup failed")
            owner = seed.pat(username, SCOPES)
            imported = dict(seed.connection, owner=owner, username=username)
        finally:
            seed.close()
        replacement = state / "restored-connection.json"
        write_private(replacement, imported)
        os.replace(replacement, state / "connection.json")
        case = report._case_by_id(manifest, "knowledge_identifier")
        def operator():
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline:
                ready_files = list(state.glob("samples/*/ready.json"))
                if ready_files:
                    break
                time.sleep(0.1)
            else:
                raise AssertionError("scripted operator received no handoff")
            ready_path = ready_files[0]
            try:
                ready = read_private(ready_path)
                prompt = (ready_path.parent / "prompt.txt").read_bytes()
                if (ready.get("host_protocol") != "knowledge-host-v3"
                        or ready.get("mcp_apps_enabled") is not False
                        or ready.get("prompt_sha256") != hashlib.sha256(prompt).hexdigest()):
                    raise AssertionError("operator prompt identity mismatch")
                adapter = RecordedSample.__new__(RecordedSample)
                adapter.url, adapter.owner = ready["loopback_url"], read_private(state / "connection.json")["owner"]
                adapter.rpc_id, adapter.calls = 0, []
                adapter.opener = build_opener(ProxyHandler({}))
                adapter.rpc_id += 1
                listed = adapter.post("/mcp", {"jsonrpc": "2.0", "id": adapter.rpc_id,
                    "method": "tools/list", "params": {}}, adapter.owner)
                tools = listed.get("result", {}).get("tools")
                if not isinstance(tools, list) or not {"work_on_project", "plugin_tool"} <= {tool.get("name") for tool in tools}:
                    raise AssertionError("Apps-disabled canonical tool surface missing")
                for tool in tools:
                    meta = tool.get("_meta", {})
                    if tool.get("name") == "read_workbench_state" or "ui" in meta or "openai/outputTemplate" in meta:
                        raise AssertionError("Apps-disabled surface exposed an App or presentation metadata")
                fixture = KnowledgeFixture(next((root / "worktrees").iterdir()), case["id"])
                started = adapter.call("work_on_project", {"client_id": ready["runner"], "path": str(fixture.root),
                    "instruction": "Scripted operator handoff test, no model"}, direct=True)
                adapter.project_ref, adapter.session_ref = started["project_ref"], started["session_ref"]
                adapter.source_reads(fixture)
                answer = {"facts": fixture.expected_facts(), "sources": [
                    {key: item[key] for key in ("path", "start_line", "end_line")} for item in fixture.snapshot_sources(0)]}
                write_private(ready_path.parent / "submission.json", {"workflow_session_id": started["session_id"], "answer": answer})
            except Exception:
                # Unblock the owned driver with an invalid handoff; never leave
                # it waiting for a human after a scripted operator failure.
                if not (ready_path.parent / "submission.json").exists():
                    write_private(ready_path.parent / "submission.json", {})
                raise
        driver = [sys.executable, str(Path(__file__).with_name("agent_knowledge_host.py")), "drive",
            "--state-dir", str(state), "--bin-dir", str(bin_dir), "--model-config", str(config)]
        with ThreadPoolExecutor(max_workers=1) as pool:
            pending = pool.submit(operator)
            result = benchmark._run_sample(Path(__file__).resolve().parents[1], root, driver, case,
                variant="tool_selection", pair_index=0, ordinal=0, base_revision=source_revision,
                case_manifest=report.DEFAULT_CASE_MANIFEST)
            pending.result()
        if result["status"] != "pass":
            raise AssertionError("scripted Host handoff failed: " + str(result.get("reason_code")))
        if not list(state.glob("samples/*/capture/" + trace.CAPTURE_MARKER)):
            raise AssertionError("private capture archive missing")
        matrix = {"experiment": "tool_selection", "cases": [{"case_id": case["id"], "samples": [result]}]}
        if not benchmark._strict_passed(matrix, [case["id"]], 1, manifest):
            raise AssertionError("scripted Host handoff failed strict acceptance")
        return {"passed": True, "imported_owner_preserved": True, "apps_disabled_surface_verified": True,
            "model_samples": 0}


def run(bin_dir, source_revision, *, mcp_apps_enabled=True):
    builds = runtime_identity(bin_dir, source_revision)
    manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
    results = []
    for case_id in CASE_IDS:
        with tempfile.TemporaryDirectory(prefix="webcodex-knowledge-ws-") as temporary:
            root = Path(temporary)
            root.chmod(0o700)
            fixture = KnowledgeFixture(root / "fixture", case_id).prepare()
            case = report._case_by_id(manifest, case_id)
            capture = root / "capture"
            trace.prepare_capture(capture, case_id, source_revision, report._case_fingerprint(case), fixture_root=fixture.root)
            state, sample_root = root / "state", root / "sample"
            state.mkdir(mode=0o700); sample_root.mkdir(mode=0o700)
            sample = RecordedSample(bin_dir, state, sample_root, fixture.root, capture,
                mcp_apps_enabled=mcp_apps_enabled)
            sample.calls = []
            advance = None
            try:
                sample.prepare()
                started = sample.call("work_on_project", {"client_id": sample.client_id, "path": str(fixture.root),
                    "instruction": "Scripted knowledge verifier integration; no model runs"}, direct=True)
                session = started["session_id"]
                sample.project_ref, sample.session_ref = started["project_ref"], started["session_ref"]
                sample.source_reads(fixture, initial=True)
                if case_id == "knowledge_identifier":
                    # These real pre-Session parser failures are counted only as
                    # adapter rejections, never as execution or source evidence.
                    for arguments in [
                        {"action": "call", "binding": "wc_pbind_AAAAAAAAAAAAAAAAAAAAAA",
                         "arguments": {}, "project": sample.project},
                        {"action": "describe", "runner": sample.client_id,
                         "plugin": "project-knowledge", "tool": "index", "project": ""},
                    ]:
                        sample.rpc_id += 1
                        reply = sample.post("/mcp", {"jsonrpc": "2.0", "id": sample.rpc_id,
                            "method": "tools/call", "params": {"name": "plugin_tool",
                                "arguments": dict(arguments, _wc={"record": sample.session_ref})}}, sample.owner)
                        if reply.get("error", {}).get("code") != -32602 or "result" in reply:
                            raise AssertionError("invalid Project arguments must fail before provider dispatch")
                if case_id in evidence.UPDATES:
                    deadline = time.monotonic() + 5
                    while True:
                        try:
                            calls = evidence.read_calls(trace._audit_events(state / "data/webcodex.db", session), capture, session)
                            advance = evidence.advance_after_read(capture_root=capture, audit_db=state / "data/webcodex.db",
                                workflow_session_id=session, fixture=fixture, trace_id=calls[-1]["trace_id"])
                            break
                        except (OSError, ValueError):
                            if time.monotonic() >= deadline:
                                raise
                            time.sleep(0.1)
                    if case_id == "knowledge_source_update":
                        binding = sample.plugin({"action": "describe", "runner": sample.client_id,
                            "plugin": "project-knowledge", "tool": "read", "project": sample.project})["binding"]
                        sample.plugin({"action": "call", "binding": binding, "arguments": {
                            key: sample.old_source[key] for key in ("path", "source_hash")}}, success=False)
                    else:
                        sample.call("read_skill_file", {"project": sample.project_ref,
                            "skill_id": sample.old_definition["skill_id"], "path": "reference.md",
                            "expected_definition_revision": sample.old_definition["definition_revision"]},
                            direct=True, success=False)
                    sample.source_reads(fixture)
            finally:
                sample.close()
            answer = {"facts": fixture.expected_facts(), "sources": [
                {key: source[key] for key in ("path", "start_line", "end_line")} for source in fixture.sources()]}
            proof = evidence.verify(capture_root=capture, audit_db=state / "data/webcodex.db",
                workflow_session_id=session, fixture=fixture, base_revision=source_revision,
                fingerprint=report._case_fingerprint(case), answer=answer, advance=advance)
            if proof["passed"] is not True:
                raise AssertionError("scripted knowledge verifier failed: " + json.dumps(proof))
            if proof["adapter_rejection_count"] != (2 if case_id == "knowledge_identifier" else 0):
                raise AssertionError("only the two actual parser rejections may be classified separately")
            results.append({"case_id": case_id, "checks": proof["checks"], "passed": True})
    return {"kind": "scripted_knowledge_mcp_ws_verifier", "model_samples": 0,
            "runtime_identity": builds, "mcp_apps_enabled": mcp_apps_enabled, "cases": results,
            "host_handoff": host_handoff_smoke(bin_dir, source_revision, manifest)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--disable-mcp-apps", action="store_true")
    args = parser.parse_args()
    os.umask(0o077)
    print(json.dumps(run(args.bin_dir, args.source_revision, mcp_apps_enabled=not args.disable_mcp_apps), indent=2))


if __name__ == "__main__":
    main()
