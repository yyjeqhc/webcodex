from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile
import unittest
import uuid
from unittest import mock

from scripts import agent_knowledge_evidence as evidence
from scripts import agent_temporal_evidence as trace
from scripts.agent_knowledge_fixture import KnowledgeFixture, SKILL
from scripts.tests import test_agent_loop_report as audit_tests
from scripts.tests import test_agent_temporal_evidence as temporal_tests


def call(tool, arguments, output, index=1, success=True):
    value = temporal_tests.call(tool, arguments, output, index=index, success=success)
    value["trace_id"] = str(uuid.uuid4())
    return value


def answer(fixture):
    return {"facts": fixture.expected_facts(), "sources": [
        {key: source[key] for key in ("path", "start_line", "end_line")}
        for source in fixture.sources()]}


def ordinary_reads(fixture, stage=0, index=1):
    return [call("read_files", {"items": [{"path": source["path"]}]},
                 {"items": [{"path": source["path"], "output": {
                     key: source[key] for key in ("start_line", "end_line", "text")}}]}, index + offset)
            for offset, source in enumerate(fixture.snapshot_sources(stage))]


def plugin_reads(fixture, stage=0, index=1):
    describe = call("plugin_tool", {"action": "describe"}, {
        "binding": "opaque-binding", "project": "project", "plugin": "project-knowledge",
        "tool": {"name": "search"}}, index)
    search = call("plugin_tool", {"action": "call", "binding": "opaque-binding"},
                  {"hits": fixture.snapshot_sources(stage)}, index + 1)
    return [describe, search]


def skill_reads(fixture, stage=0, index=1):
    sources = fixture.snapshot_sources(stage)
    definition = next(source for source in sources if source["path"] == SKILL)
    calls = [call("list_skills", {}, {"skills": [{"name": "receipt-audit", "skill_id": "skill-one"}]}, index)]
    for offset, source in enumerate(sources):
        output = dict(source, skill_path=Path(source["path"]).name,
                      project_path=source["path"], source_scope="project", skill_id="skill-one",
                      definition_revision=definition["source_hash"], sha256=source["source_hash"])
        output.pop("path")
        output.pop("source_hash")
        tool = "load_skill" if source["path"] == SKILL else "read_skill_file"
        calls.append(call(tool, {"skill_id": "skill-one"}, output, index + offset + 1))
    return calls


class KnowledgeSemanticsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def fixture(self, case="knowledge_identifier"):
        return KnowledgeFixture(self.root / case, case).prepare()

    def check(self, fixture, calls, advance=None):
        return evidence.check_calls(calls, fixture, {"project"}, advance)

    def test_free_ordinary_read_succeeds_without_claiming_resource_adoption(self):
        fixture = self.fixture()
        checks, adoption = self.check(fixture, ordinary_reads(fixture))
        self.assertTrue(all(checks.values()), checks)
        self.assertEqual(adoption, {"knowledge_plugin": False, "skill": False})
        self.assertTrue(evidence.check_answer(answer(fixture), fixture))

    def test_numbered_read_matches_exact_canonical_line_prefixes(self):
        fixture = self.fixture()
        calls = ordinary_reads(fixture)
        block = calls[0]["output"]["items"][0]["output"]
        # Exercise a selected range starting after line one, as well as full reads.
        for start in (1, 2):
            with self.subTest(start=start):
                source = fixture.sources()[0]
                block.update(format="numbered", start_line=start,
                    text="\n".join(f"{index} | {line}" for index, line in
                        enumerate(source["text"].splitlines()[start - 1:], start)))
                checks, adoption = self.check(fixture, calls)
                self.assertTrue(all(checks.values()), checks)
                self.assertEqual(adoption, {"knowledge_plugin": False, "skill": False})
        for text in ("1 | invented", block["text"].replace("2 | ", "02 | "),
                     block["text"].replace("3 | ", "4 | "),
                     block["text"].replace("2 | ", "2: ")):
            wrong = copy.deepcopy(calls)
            wrong[0]["output"]["items"][0]["output"]["text"] = text
            with self.subTest(text=text), self.assertRaises(trace.EvidenceUnavailable):
                self.check(fixture, wrong)
        block["format"] = "plain"
        with self.assertRaises(trace.EvidenceUnavailable):
            self.check(fixture, calls)

    def test_directed_ordinary_reads_prove_content_but_not_requested_resource(self):
        for case in ("knowledge_paraphrase", "knowledge_skill_read"):
            with self.subTest(case=case):
                fixture = self.fixture(case)
                checks, adoption = self.check(fixture, ordinary_reads(fixture))
                self.assertTrue(checks["source_content_observed"])
                self.assertFalse(checks["required_resource_path"])
                self.assertEqual(adoption, {"knowledge_plugin": False, "skill": False})

    def test_project_bound_plugin_describe_and_hash_bound_search_can_supply_all_facts(self):
        fixture = self.fixture("knowledge_paraphrase")
        calls = plugin_reads(fixture)
        checks, adoption = self.check(fixture, calls)
        self.assertTrue(all(checks.values()), checks)
        self.assertEqual(adoption, {"knowledge_plugin": True, "skill": False})
        for wrong in (calls[1:], list(reversed(calls))):
            if len(wrong) == 2:
                wrong = copy.deepcopy(wrong)
                wrong[0]["started"], wrong[0]["ended"] = 0, 1
                wrong[1]["started"], wrong[1]["ended"] = 10, 11
            checks, _ = self.check(fixture, wrong)
            self.assertFalse(checks["required_resource_path"])

    def test_direct_skill_reads_can_supply_discovery_and_revision_bound_sources(self):
        fixture = self.fixture("knowledge_skill_read")
        calls = skill_reads(fixture)[1:]
        for item in calls:
            item["tool"] = "read_skill_file"
            item["output"]["name"] = "receipt-audit"
        checks, adoption = self.check(fixture, calls)
        self.assertTrue(all(checks.values()), checks)
        self.assertEqual(adoption, {"knowledge_plugin": False, "skill": True})
        calls[-1]["output"]["definition_revision"] = "0" * 64
        with self.assertRaises(trace.EvidenceUnavailable):
            self.check(fixture, calls)

    def test_plugin_wrong_project_hash_and_range_are_not_source_evidence(self):
        fixture = self.fixture("knowledge_paraphrase")
        for field, value in (("project", "foreign"), ("plugin", "other-plugin")):
            calls = plugin_reads(fixture)
            calls[0]["output"][field] = value
            with self.subTest(field=field), self.assertRaises(trace.EvidenceUnavailable):
                self.check(fixture, calls)
        for field, value in (("source_hash", "0" * 64), ("start_line", 0),
                             ("start_line", True), ("end_line", 999), ("text", "invented facts")):
            calls = plugin_reads(fixture)
            calls[1]["output"]["hits"][0][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(trace.EvidenceUnavailable):
                self.check(fixture, calls)

    def test_path_correct_excerpt_without_facts_is_not_success(self):
        fixture = self.fixture("knowledge_paraphrase")
        calls = plugin_reads(fixture)
        source = calls[1]["output"]["hits"][0]
        source.update(start_line=1, end_line=1, text=source["text"].splitlines()[0])
        checks, adoption = self.check(fixture, calls)
        self.assertTrue(adoption["knowledge_plugin"])
        self.assertTrue(checks["required_resource_path"])
        self.assertFalse(checks["source_content_observed"])
        claimed = answer(fixture)
        claimed["sources"][0].update(start_line=1, end_line=1)
        self.assertFalse(evidence.check_answer(claimed, fixture))

    def test_skill_list_load_and_reference_require_bound_definition_revision(self):
        fixture = self.fixture("knowledge_skill_read")
        calls = skill_reads(fixture)
        checks, adoption = self.check(fixture, calls)
        self.assertTrue(all(checks.values()), checks)
        self.assertEqual(adoption, {"knowledge_plugin": False, "skill": True})
        for field, value in (("definition_revision", "f" * 64), ("sha256", "e" * 64),
                             ("end_line", True)):
            wrong = copy.deepcopy(calls)
            wrong[-1]["output"][field] = value
            with self.subTest(field=field), self.assertRaises(trace.EvidenceUnavailable):
                self.check(fixture, wrong)
        checks, _ = self.check(fixture, calls[1:])
        self.assertFalse(checks["source_content_observed"])
        self.assertFalse(checks["required_resource_path"])

    def test_skill_definition_update_rejects_old_revision_even_when_reference_unchanged(self):
        fixture = self.fixture("knowledge_skill_update")
        before = skill_reads(fixture)
        advance = fixture.advance()
        advance.update(at_ms=50, trigger_trace_id=before[1]["trace_id"])
        after = skill_reads(fixture, stage=1, index=6)
        checks, _ = self.check(fixture, before + after, advance)
        self.assertTrue(all(checks.values()), checks)
        wrong = copy.deepcopy(after)
        wrong[-1]["output"]["definition_revision"] = before[-1]["output"]["definition_revision"]
        with self.assertRaises(trace.EvidenceUnavailable):
            self.check(fixture, before + wrong, advance)

    def test_skill_current_paths_bind_load_and_reference_to_exact_fixture_folder(self):
        fixture = self.fixture("knowledge_skill_read")
        calls = skill_reads(fixture)
        self.assertEqual([item["tool"] for item in calls[1:]], ["load_skill", "read_skill_file"])
        self.assertEqual(calls[1]["output"]["skill_path"], "SKILL.md")
        self.assertEqual(calls[2]["output"]["skill_path"], "reference.md")
        checks, adoption = self.check(fixture, calls)
        self.assertTrue(all(checks.values()), checks)
        self.assertTrue(adoption["skill"])

    def test_skill_rejects_mixed_missing_wrong_scope_and_noncanonical_locators(self):
        fixture = self.fixture("knowledge_skill_read")
        mutations = [
            {"path": "reference.md"},
            {"source_scope": "runner"},
            {"source_scope": None},
            {"project_path": ".agents/skills/other/reference.md"},
            {"project_path": "reference.md"},
            {"project_path": ".agents/skills/receipt-audit/../receipt-audit/reference.md"},
            {"project_path": None},
            {"skill_path": None},
        ]
        for path in ("../reference.md", "./reference.md", "/reference.md", "reference.md/",
                     "nested//reference.md", "nested/../reference.md", " reference.md",
                     "reference.md ", "reference\\md", "C:/reference.md", "reference.md\n"):
            mutations.append({"skill_path": path, "project_path": ".agents/skills/receipt-audit/" + path})
        for mutation in mutations:
            calls = skill_reads(fixture)
            calls[-1]["output"].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaises(trace.EvidenceUnavailable):
                self.check(fixture, calls)
        for missing in ("skill_path", "project_path", "source_scope"):
            calls = skill_reads(fixture)
            del calls[-1]["output"][missing]
            with self.subTest(missing=missing), self.assertRaises(trace.EvidenceUnavailable):
                self.check(fixture, calls)

    def test_historical_skill_paths_remain_revision_and_hash_bound(self):
        fixture = self.fixture("knowledge_skill_read")
        calls = skill_reads(fixture)
        for item in calls[1:]:
            output = item["output"]
            output["path"] = output.pop("skill_path")
            del output["project_path"]
            del output["source_scope"]
        checks, adoption = self.check(fixture, calls)
        self.assertTrue(all(checks.values()), checks)
        self.assertTrue(adoption["skill"])
        for field, value in (("definition_revision", "0" * 64), ("sha256", "0" * 64),
                             ("path", "../reference.md"), ("source_scope", "runner")):
            wrong = copy.deepcopy(calls)
            wrong[-1]["output"][field] = value
            with self.subTest(field=field), self.assertRaises(trace.EvidenceUnavailable):
                self.check(fixture, wrong)

    def test_source_update_requires_two_stages_and_exact_trigger_hash_time(self):
        fixture = self.fixture("knowledge_source_update")
        before = plugin_reads(fixture)
        advance = fixture.advance()
        advance.update(at_ms=50, trigger_trace_id=before[1]["trace_id"])
        after = plugin_reads(fixture, stage=1, index=6)
        checks, _ = self.check(fixture, before + after, advance)
        self.assertTrue(all(checks.values()), checks)
        for field, value in (("trigger_trace_id", before[0]["trace_id"]), ("at_ms", 15),
                             ("at_ms", True), ("old_sha256", "0" * 64),
                             ("new_sha256", "1" * 64), ("path", "other.md")):
            wrong = dict(advance, **{field: value})
            with self.subTest(field=field, value=value):
                try:
                    checks, _ = self.check(fixture, before + after, wrong)
                except trace.EvidenceUnavailable:
                    continue
                self.assertFalse(checks["coordinator_update_order"])
        checks, _ = self.check(fixture, after, advance)
        self.assertFalse(checks["coordinator_update_order"])
        checks, _ = self.check(fixture, before, advance)
        self.assertFalse(checks["source_content_observed"])
        with self.assertRaises(trace.EvidenceUnavailable):
            self.check(fixture, before + plugin_reads(fixture, stage=0, index=6), advance)

    def test_session_messages_bind_without_borrowing_project_source_authority(self):
        fixture = self.fixture()
        bootstrap = call("work_on_project", {"client_id": "runner", "path": str(fixture.root)},
                         {"session_id": "wc_sess_test", "session_ref": "~s2",
                          "resolved_project": "project"}, index=0)
        reads = ordinary_reads(fixture, index=1)
        message = call("post_session_message", {"session_id": "wc_sess_test", "kind": "progress",
            "message": "Checkpoint", "tags": ["checkpoint"], "delivery_key": "checkpoint-1"},
            {"session_id": "wc_sess_test", "message": {"session_id": "wc_sess_test",
             "author_session_id": None}}, index=2)
        message["project"] = None
        calls = [bootstrap, *reads, message]
        for selector in ("wc_sess_test", "~s2"):
            message["arguments"]["session_id"] = selector
            self.assertEqual(evidence._bind_fixture(calls, fixture.root, "wc_sess_test"), {"project"})
        # Session messages do not supply the required source observation themselves.
        with self.assertRaises(trace.EvidenceUnavailable):
            evidence._bind_fixture([bootstrap, message], fixture.root, "wc_sess_test")
        variants = []
        for selector in ("wc_sess_foreign", "~s3", "~s02", None, []):
            wrong = copy.deepcopy(calls); wrong[-1]["arguments"]["session_id"] = selector
            variants.append(wrong)
        for field in ("project", "source_session_id", "target_session_id", "client_id"):
            wrong = copy.deepcopy(calls); wrong[-1]["arguments"][field] = "foreign"
            variants.append(wrong)
        wrong = copy.deepcopy(calls); wrong[-1]["project"] = "foreign"; variants.append(wrong)
        wrong = copy.deepcopy(calls); wrong[-1]["output"]["session_id"] = "foreign"; variants.append(wrong)
        wrong = copy.deepcopy(calls); wrong[-1]["output"]["message"]["session_id"] = "foreign"; variants.append(wrong)
        wrong = copy.deepcopy(calls); wrong[-1]["output"]["message"]["author_session_id"] = "foreign"; variants.append(wrong)
        wrong = copy.deepcopy(calls); wrong[-1]["started"] = 0; variants.append(wrong)
        wrong = copy.deepcopy(calls); wrong[1]["project"] = "foreign"; variants.append(wrong)
        for index, wrong in enumerate(variants):
            with self.subTest(index=index), self.assertRaises(trace.EvidenceUnavailable):
                evidence._bind_fixture(wrong, fixture.root, "wc_sess_test")
        checks, adoption = evidence.check_calls([bootstrap, message], fixture, {"project"})
        self.assertFalse(checks["source_content_observed"])
        self.assertEqual(adoption, {"knowledge_plugin": False, "skill": False})

    def test_answers_reject_fabrication_duplicates_wrong_paths_ranges_and_booleans(self):
        fixture = self.fixture()
        original = answer(fixture)
        self.assertTrue(evidence.check_answer(original, fixture))
        variants = []
        for field, value in (("path", "src/receipts_policy.py"), ("path", []), ("start_line", True),
                             ("end_line", False), ("start_line", 0), ("end_line", 999)):
            wrong = copy.deepcopy(original)
            wrong["sources"][0][field] = value
            variants.append(wrong)
        for facts in (original["facts"][:1], original["facts"] + ["fabricated"],
                      [original["facts"][0]] * 2, [True, False]):
            variants.append(dict(original, facts=facts))
        variants.extend([dict(original, sources=[]), dict(original, extra=True), None])
        for wrong in variants:
            self.assertFalse(evidence.check_answer(wrong, fixture), wrong)


class ReadCallsTests(unittest.TestCase):
    def event(self, **overrides):
        value = {"action_name": "toolsCall", "operation": "read_files", "project": "project",
                 "summary": {"model_ergonomics": {"tool_name": "read_files", "success": True}},
                 "server_trace_id": str(uuid.uuid4()), "request_observed_at_ms": 10,
                 "response_handed_at_ms": 11, "response_streaming": False,
                 "client_window_key": "window", "principal_correlation_kind": "username",
                 "principal_correlation_id": "principal"}
        value.update(overrides)
        return value

    def payload(self, arguments=None, output=None):
        return {"effective_arguments": arguments or {}, "final_response": {"result": {
            "structuredContent": {"success": True, "output": output or {}}}}}

    def read(self, events, payload):
        with mock.patch.object(trace, "_payloads", return_value=payload):
            return evidence.read_calls(events, Path("/unused"), "session")

    def test_direct_wrapped_and_native_plugin_results_remain_distinct(self):
        event = self.event()
        self.assertFalse(self.read([event], self.payload())[0]["native_plugin"])
        wrapped = self.event(operation="call_runtime_tool")
        value = self.payload({"tool": "read_files", "arguments": {"items": []}})
        self.assertEqual(self.read([wrapped], value)[0]["arguments"], {"items": []})
        value["effective_arguments"]["tool"] = "other"
        with self.assertRaises(trace.EvidenceUnavailable):
            self.read([wrapped], value)
        native = self.event(operation="plugin_tool", summary={})
        payload = {"effective_arguments": {"action": "call"}, "final_response": {"result": {
            "isError": False, "structuredContent": {"hits": []}}}}
        result = self.read([native], payload)[0]
        self.assertTrue(result["native_plugin"])
        self.assertEqual(result["output"], {"hits": []})
        payload["final_response"]["result"]["isError"] = 0
        with self.assertRaises(trace.EvidenceUnavailable):
            self.read([native], payload)

    def test_duplicate_scope_timing_streaming_and_boolean_timestamp_rejected(self):
        original = self.event()
        variants = [[original, copy.deepcopy(original)]]
        for field, value in (("request_observed_at_ms", True), ("response_handed_at_ms", False),
                             ("response_handed_at_ms", 9), ("response_streaming", 0)):
            variants.append([dict(original, **{field: value})])
        second = self.event(client_window_key="foreign-window")
        variants.append([original, second])
        for events in variants:
            with self.subTest(events=events), self.assertRaises(trace.EvidenceUnavailable):
                self.read(events, self.payload())

    def test_expected_failure_without_structured_output_preserves_call(self):
        native = self.event(operation="plugin_tool", summary={})
        payload = {"effective_arguments": {"action": "call"}, "final_response": {"result": {
            "isError": True, "content": [{"type": "text", "text": "stale source"}]}}}
        rejected = self.read([native], payload)[0]
        self.assertFalse(rejected["result"]["success"])
        self.assertEqual(rejected["output"], {})
        event = self.event(operation="read_skill_file", summary={}, status="failed")
        payload = self.payload()
        payload["final_response"]["result"]["structuredContent"] = {"success": False, "error": "stale revision"}
        rejected = self.read([event], payload)[0]
        self.assertFalse(rejected["result"]["success"])
        self.assertEqual(rejected["output"], {})

    def test_result_success_must_be_boolean_and_match_audit(self):
        for value in (1, None, False):
            payload = self.payload()
            payload["final_response"]["result"]["structuredContent"]["success"] = value
            with self.subTest(value=value), self.assertRaises(trace.EvidenceUnavailable):
                self.read([self.event()], payload)

    def test_audit_success_integer_cannot_impersonate_boolean(self):
        event = self.event(summary={"model_ergonomics": {"tool_name": "read_files", "success": 1}})
        with self.assertRaises(trace.EvidenceUnavailable):
            self.read([event], self.payload())


@unittest.skipUnless(os.name == "posix" and shutil.which("zstd"), "private captures require POSIX and zstd")
class KnowledgeCaptureTests(unittest.TestCase):
    # Reuse the actual compressed payload/digest writer rather than simulate decoding.
    private_write = temporal_tests.TemporalCaptureTests.private_write
    payload = temporal_tests.TemporalCaptureTests.payload
    flush = temporal_tests.TemporalCaptureTests.flush

    def setUp(self):
        self.audit = audit_tests.AgentLoopReportTests()
        self.audit.setUp()
        self.addCleanup(self.audit.tearDown)
        self.fixture = KnowledgeFixture(self.audit.root / "fixture", "knowledge_identifier").prepare()
        self.root = self.audit.root / "capture"
        self.base, self.fingerprint = "a" * 40, "b" * 64
        trace.prepare_capture(self.root, self.fixture.case_id, self.base, self.fingerprint,
                              fixture_root=self.fixture.root)
        self.bootstrap = self.capture("work_on_project", {
            "client_id": "runner", "path": str(self.fixture.root)}, {
            "session_id": "wc_sess_test", "resolved_project": "agent:test:project"}, index=1)
        observed = ordinary_reads(self.fixture)[0]
        self.read_trace = self.capture(observed["tool"], observed["arguments"], observed["output"], index=2)

    def capture(self, tool, arguments, output, index):
        self.trace = str(uuid.uuid4())
        self.directory = self.root / self.trace
        self.directory.mkdir(mode=0o700)
        (self.directory / "payloads").mkdir(mode=0o700)
        self.private_write(self.directory / ".webcodex-tool-trace", b"")
        self.events = []
        self.audit.insert_event(self.trace, tool=tool, trace_id=self.trace,
                                started=index * 10, handed=index * 10 + 1)
        self.payload("effective_arguments", arguments)
        self.payload("final_response", {"result": {"structuredContent": {"success": True, "output": output}}})
        return self.trace

    def verify(self):
        return evidence.verify(capture_root=self.root, audit_db=self.audit.audit_db,
            workflow_session_id="wc_sess_test", fixture=self.fixture, base_revision=self.base,
            fingerprint=self.fingerprint, answer=answer(self.fixture))

    def test_real_zstd_audit_and_exact_bootstrap_prove_free_read_without_adoption(self):
        result = self.verify()
        self.assertTrue(result["available"], result)
        self.assertTrue(result["passed"], result)
        self.assertEqual(set(result["checks"]), evidence.CHECK_NAMES)
        self.assertEqual(result["canonical_call_count"], 2)
        self.assertEqual(result["adapter_rejection_count"], 0)
        self.assertEqual(result["resource_adoption"], {"knowledge_plugin": False, "skill": False})
        serialized = json.dumps(result)
        for private in (str(self.fixture.root), self.bootstrap, self.read_trace, "wc_sess_test"):
            self.assertNotIn(private, serialized)

    def test_real_capture_session_message_with_null_audit_project_binds(self):
        identity = self.capture("post_session_message", {"session_id": "wc_sess_test", "kind": "progress",
            "message": "Checkpoint", "tags": ["checkpoint"], "delivery_key": "checkpoint-1"},
            {"session_id": "wc_sess_test", "message": {"session_id": "wc_sess_test",
             "author_session_id": None}}, index=3)
        with sqlite3.connect(self.audit.audit_db) as connection:
            connection.execute("UPDATE action_events SET project=NULL WHERE event_id=?", (identity,))
        result = self.verify()
        self.assertTrue(result["passed"], result)
        self.assertEqual(result["canonical_call_count"], 3)
        with sqlite3.connect(self.audit.audit_db) as connection:
            connection.execute("UPDATE action_events SET client_window_key='foreign' WHERE event_id=?", (identity,))
        self.assertFalse(self.verify()["available"])

    def parser_rejection(self, *, message=evidence._PLUGIN_PROJECT_VALIDATION_ERROR, action="call"):
        self.trace = str(uuid.uuid4())
        self.directory = self.root / self.trace
        self.directory.mkdir(mode=0o700)
        (self.directory / "payloads").mkdir(mode=0o700)
        self.private_write(self.directory / ".webcodex-tool-trace", b"")
        self.events = []
        self.audit.insert_event(self.trace, tool="plugin_tool", trace_id=self.trace,
            status="failed", success=False, link=False, include_telemetry=False)
        self.payload("raw_arguments", {"action": action, "runner": None, "plugin": None,
            "tool": None, "binding_present": True, "arguments_present": True})
        self.payload("final_response", {"jsonrpc": "2.0", "id": 1, "error": {
            "code": -32602, "message": message}})
        for name in ("mcp_tool_dispatch_failed", "mcp_tool_dispatch_finished", "mcp_tool_handler_returned"):
            self.events.append({"server_trace_id": self.trace, "event": name, "tool_name": "plugin_tool",
                "client_window_key": "hashed-window-a", "protocol_success": False,
                "tool_success": False, "http_status": 400,
                "category": "bad_request" if name.endswith("handler_returned") else "invalid_arguments"})
        self.flush()
        return self.trace

    def assert_current_parser_rejection(self, message, action):
        self.parser_rejection(message=message, action=action)
        result = self.verify()
        self.assertTrue(result["passed"], result)
        self.assertEqual(result["canonical_call_count"], 2)
        self.assertEqual(result["adapter_rejection_count"], 1)
        self.assertEqual(result["resource_adoption"], {"knowledge_plugin": False, "skill": False})
        original = copy.deepcopy(self.events)
        for event in ({"event": "tool_runner_request_enqueued"}, {"phase": "effective_arguments"},
                      {"event": "mcp_tool_handler_returned", "client_window_key": "foreign"}):
            self.events = original + [dict(event, server_trace_id=self.trace)]
            self.flush()
            self.assertFalse(self.verify()["available"])
        self.events = []
        self.payload("raw_arguments", {"action": "describe" if action == "call" else "call"})
        self.payload("final_response", {"error": {"code": -32602, "message": message}})
        self.events.extend(original[2:]); self.flush()
        self.assertFalse(self.verify()["available"], "current diagnostic must match its action")

    def test_current_call_project_recovery_is_not_execution_or_source_evidence(self):
        self.assert_current_parser_rejection(evidence._PLUGIN_CALL_PROJECT_RECOVERY_ERROR, "call")

    def test_current_describe_project_recovery_is_not_execution_or_source_evidence(self):
        self.assert_current_parser_rejection(evidence._PLUGIN_DESCRIBE_PROJECT_RECOVERY_ERROR, "describe")

    def test_captured_parser_rejection_is_not_session_execution_or_source_evidence(self):
        self.parser_rejection()
        result = self.verify()
        self.assertTrue(result["passed"], result)
        self.assertEqual(result["canonical_call_count"], 2)
        self.assertEqual(result["adapter_rejection_count"], 1)
        self.assertEqual(result["resource_adoption"], {"knowledge_plugin": False, "skill": False})
        selected = trace._audit_events(self.audit.audit_db, "wc_sess_test")
        trace._coverage(self.root, selected)
        # Without independent audit provenance the extra trace remains unverified.
        with self.assertRaises(trace.EvidenceUnavailable):
            evidence._coverage(self.root, selected)

    def test_parser_rejection_rejects_unknown_foreign_corrupt_and_runtime_evidence(self):
        identity = self.parser_rejection()
        original = copy.deepcopy(self.events)
        for field, value in (("principal_correlation_id", "foreign"), ("client_window_key", "foreign"),
                             ("status", "success"), ("operation", "read_files")):
            with sqlite3.connect(self.audit.audit_db) as connection:
                connection.execute(f"UPDATE action_events SET {field}=? WHERE event_id=?", (value, identity))
            self.assertFalse(self.verify()["available"], field)
            with sqlite3.connect(self.audit.audit_db) as connection:
                original_value = {"principal_correlation_id": "principal-a", "client_window_key": "hashed-window-a",
                                  "status": "failed", "operation": "plugin_tool"}[field]
                connection.execute(f"UPDATE action_events SET {field}=? WHERE event_id=?", (original_value, identity))
        with sqlite3.connect(self.audit.audit_db) as connection:
            connection.execute("INSERT INTO action_event_workflow_links VALUES (?,?)", (identity, "foreign-session"))
        self.assertFalse(self.verify()["available"])
        with sqlite3.connect(self.audit.audit_db) as connection:
            connection.execute("DELETE FROM action_event_workflow_links WHERE event_id=?", (identity,))
        variants = []
        wrong = copy.deepcopy(original); wrong[0]["payload_sha256"] = "f" * 64; variants.append(wrong)
        wrong = copy.deepcopy(original); wrong[-1]["http_status"] = 200; variants.append(wrong)
        wrong = copy.deepcopy(original); wrong[-1]["client_window_key"] = "foreign"; variants.append(wrong)
        for event in ({"event": "tool_runner_request_enqueued"}, {"phase": "kernel_arguments"},
                      {"phase": "effective_arguments"},
                      {"event": "tool_trace_payload_captured", "phase": "runner_result"}):
            variants.append(original + [dict(event, server_trace_id=identity)])
        for wrong in variants:
            self.events = wrong; self.flush()
            self.assertFalse(self.verify()["available"])
        self.events = []
        self.payload("raw_arguments", {"action": "call"})
        self.payload("final_response", {"error": {"code": -32602, "message": "unknown rejection"}})
        self.events.extend(original[2:]); self.flush()
        self.assertFalse(self.verify()["available"])

    def test_bootstrap_only_is_unavailable_even_with_correct_final_answer(self):
        with sqlite3.connect(self.audit.audit_db) as connection:
            connection.execute("DELETE FROM action_events WHERE event_id=?", (self.read_trace,))
        shutil.rmtree(self.root / self.read_trace)
        self.assertFalse(self.verify()["available"])

    def test_wrong_project_or_session_cannot_borrow_real_capture(self):
        with sqlite3.connect(self.audit.audit_db) as connection:
            connection.execute("UPDATE action_events SET project='foreign' WHERE event_id=?", (self.read_trace,))
        self.assertFalse(self.verify()["available"])
        with sqlite3.connect(self.audit.audit_db) as connection:
            connection.execute("UPDATE action_events SET project='agent:test:project' WHERE event_id=?", (self.read_trace,))
            connection.execute("UPDATE action_event_workflow_links SET workflow_session_id='foreign' WHERE event_id=?",
                               (self.bootstrap,))
        self.assertFalse(self.verify()["available"])

    def test_tampered_digest_or_missing_phase_is_unavailable(self):
        original = copy.deepcopy(self.events)
        for events in (original[:1], original + original[:1]):
            self.events = events
            self.flush()
            self.assertFalse(self.verify()["available"])
        self.events = copy.deepcopy(original)
        self.events[0]["payload_sha256"] = "f" * 64
        self.flush()
        self.assertFalse(self.verify()["available"])

    def test_non_exact_bootstrap_path_cannot_bind_valid_source_bytes(self):
        bootstrap = self.root / self.bootstrap
        raw = trace._payloads(self.root, self.bootstrap)
        self.trace, self.directory = self.bootstrap, bootstrap
        self.events = []
        self.payload("effective_arguments", dict(raw["effective_arguments"], path=str(self.fixture.root / "other")))
        self.payload("final_response", raw["final_response"])
        self.assertFalse(self.verify()["available"])


if __name__ == "__main__":
    unittest.main()
