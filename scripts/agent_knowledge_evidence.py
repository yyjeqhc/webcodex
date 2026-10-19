"""Private, opt-in knowledge/Skill trace checks for disposable model samples.

Reuse the existing bounded capture reader and ActionAudit identity checks. Native
Plugin arguments stay redacted: successful native results are associated with
the coordinator-pinned single provider and observed Project-bound descriptions,
not a reconstructed opaque binding. No production capture policy is changed.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import sqlite3
import subprocess

try:
    from scripts import agent_temporal_evidence as trace
    from scripts.agent_knowledge_fixture import CASE_IDS, SKILL, REFERENCE
except ModuleNotFoundError:
    import agent_temporal_evidence as trace
    from agent_knowledge_fixture import CASE_IDS, SKILL, REFERENCE

TARGET = "knowledge_skill_disposable_fixture"
UPDATES = frozenset(("knowledge_source_update", "knowledge_skill_update"))
DIRECTED_KNOWLEDGE = frozenset(("knowledge_paraphrase", "knowledge_source_update"))
DIRECTED_SKILL = frozenset(("knowledge_skill_read", "knowledge_skill_update"))
CHECK_NAMES = frozenset(("fixture_root_session_project", "fixture_integrity",
    "source_content_observed", "answer_facts_and_citations", "required_resource_path",
    "coordinator_update_order"))
MAX_ANSWER_BYTES = 32 * 1024
SETUP = frozenset(("work_on_project", "read_tool_manifest", "get_runtime_status", "list_projects"))


def _source_match(item, source, *, hash_key=None, numbered=False):
    """Match returned source bytes/range, never a caller's citation alone."""
    start, end, text = item.get("start_line"), item.get("end_line"), item.get("text")
    lines = source["text"].splitlines()
    if (type(start) is not int or type(end) is not int or not 1 <= start <= end <= len(lines)
            or not isinstance(text, str)):
        return False
    if hash_key is not None and item.get(hash_key) != source["source_hash"]:
        return False
    selected = lines[start - 1:end]
    # Match the canonical read_files renderer exactly, including each line's
    # number and delimiter. Never strip arbitrary prefixes from source text.
    expected = "\n".join(selected)
    if numbered:
        expected = "\n".join(f"{start + index} | {line}" for index, line in enumerate(selected))
    return text.removesuffix("\n") == expected


def _walk(value, depth=0):
    if depth > 12:
        raise trace.EvidenceUnavailable("capture_limit")
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk(child, depth + 1)
    elif isinstance(value, list):
        for child in value:
            yield from _walk(child, depth + 1)


def read_calls(events, root, session):
    """Normalize ordinary ToolResult and the native Plugin MCP result separately."""
    calls, scope, seen = [], None, set()
    for event in events:
        if event.get("action_name") != "toolsCall":
            continue
        telemetry = (event.get("summary") or {}).get("model_ergonomics") or {}
        tool = telemetry.get("tool_name")
        native = event.get("operation") == "plugin_tool" and tool is None
        legacy_skill = event.get("operation") in {"list_skills", "read_skill_file"} and tool is None
        if native:
            tool = "plugin_tool"
        elif legacy_skill:
            tool = event["operation"]
            if event.get("status") not in {"success", "failed", "timeout"}:
                raise trace.EvidenceUnavailable("canonical_result_mismatch")
        if not isinstance(tool, str) or not tool:
            raise trace.EvidenceUnavailable("missing_canonical_call")
        identity = event.get("server_trace_id")
        if identity in seen:
            raise trace.EvidenceUnavailable("ambiguous_capture")
        seen.add(identity)
        started, ended = event.get("request_observed_at_ms"), event.get("response_handed_at_ms")
        if (type(started) is not int or type(ended) is not int or ended < started
                or event.get("response_streaming") is not False):
            raise trace.EvidenceUnavailable("missing_order")
        current = tuple(event.get(key) for key in
                        ("client_window_key", "principal_correlation_kind", "principal_correlation_id"))
        if not all(isinstance(value, str) and value for value in current) or scope not in (None, current):
            raise trace.EvidenceUnavailable("scope_mismatch")
        scope = current
        payload = trace._payloads(Path(root), identity)
        arguments = payload["effective_arguments"]
        if event.get("operation") == "call_runtime_tool":
            if arguments.get("tool") != tool or not isinstance(arguments.get("arguments"), dict):
                raise trace.EvidenceUnavailable("canonical_call_mismatch")
            arguments = arguments["arguments"]
        envelope = payload["final_response"]
        mcp = envelope.get("result", {})
        value = mcp.get("structuredContent")
        if not isinstance(arguments, dict):
            raise trace.EvidenceUnavailable("missing_mcp_result")
        if native:
            if type(mcp.get("isError")) is not bool:
                raise trace.EvidenceUnavailable("canonical_result_mismatch")
            # A legal rejected Plugin call may contain only MCP text content.
            # Keep its identity and failure; it is no source-read evidence.
            if not mcp["isError"] and not isinstance(value, dict):
                raise trace.EvidenceUnavailable("missing_mcp_result")
            if mcp["isError"] and value is None:
                value = {}
            if not isinstance(value, dict):
                raise trace.EvidenceUnavailable("missing_mcp_result")
            success, output = not mcp["isError"], value
        else:
            if not isinstance(value, dict):
                raise trace.EvidenceUnavailable("missing_mcp_result")
            success, output = value.get("success"), value.get("output")
            if success is False and output is None:
                output = {}
            observed_success = (event.get("status") == "success" if legacy_skill
                                else telemetry.get("success"))
            if (type(success) is not bool or type(observed_success) is not bool
                    or success != observed_success or not isinstance(output, dict)):
                raise trace.EvidenceUnavailable("canonical_result_mismatch")
            # The canonical Plugin gateway carrier contains its native result.
            if tool == "plugin_tool" and isinstance(output.get("structuredContent"), dict):
                output = output["structuredContent"]
        calls.append({"tool": tool, "arguments": arguments, "output": output,
            "result": {"success": success, "output": output}, "project": event.get("project"),
            "started": started, "ended": ended, "trace_id": identity,
            "envelope": envelope, "native_plugin": native})
    if not 0 < len(calls) <= trace.MAX_CALLS:
        raise trace.EvidenceUnavailable("capture_limit")
    calls.sort(key=lambda item: item["started"])
    return calls


def _bind_fixture(calls, fixture_root, workflow_session_id):
    """Bind Project work normally; messages carry Session authority, not files."""
    messages = [call for call in calls if call["tool"] == "post_session_message"]
    project_calls = [call for call in calls if call["tool"] != "post_session_message"]
    projects = trace._bind_fixture(project_calls, fixture_root, workflow_session_id)
    bootstraps = [call for call in project_calls if call["tool"] == "work_on_project"
                  and call["result"]["success"] is True]
    allowed_arguments = {"session_id", "kind", "message", "tags", "reply_to", "priority",
                         "requires_ack", "delivery_key"}
    for call in messages:
        args, output = call["arguments"], call["output"]
        sessions = {workflow_session_id}
        observed_bootstraps = [bootstrap for bootstrap in bootstraps
                               if bootstrap["ended"] <= call["started"]]
        for bootstrap in observed_bootstraps:
            reference = bootstrap["output"].get("session_ref")
            if isinstance(reference, str) and re.fullmatch(r"~s[1-9][0-9]{0,18}", reference):
                sessions.add(reference)
        message = output.get("message")
        if (not observed_bootstraps or set(args) - allowed_arguments
                or not isinstance(args.get("session_id"), str)
                or args["session_id"] not in sessions
                or call["project"] not in (None, *projects)
                or call["result"]["success"] is not True
                or output.get("session_id") != workflow_session_id
                or not isinstance(message, dict)
                or message.get("session_id") != workflow_session_id
                or message.get("author_session_id") not in (None, workflow_session_id)):
            raise trace.EvidenceUnavailable("fixture_binding_mismatch")
    return projects


# Exact historical/current PluginToolCall.validate diagnostics, emitted before
# Session canonicalization or provider invocation. Keep the legacy observation
# readable; current diagnostics must also match their action. None proves a read.
_PLUGIN_PROJECT_VALIDATION_ERROR = (
    "invalid arguments for tool 'plugin_tool': "
    "project must be an exact bounded Project on action=describe")
_PLUGIN_CALL_PROJECT_RECOVERY_ERROR = (
    "invalid arguments for tool 'plugin_tool': "
    "action=call accepts only binding and arguments; both are required. Project is selected on action=describe. "
    "Reuse a retained binding only for the same intended Project. If no binding is retained or you intend another Project, "
    "describe the exact runner, plugin, and tool (with the intended project when projectBound) and use its binding. "
    "Reconcile any earlier unknown outcome before retrying an effect.")
_PLUGIN_DESCRIBE_PROJECT_RECOVERY_ERROR = (
    "invalid arguments for tool 'plugin_tool': "
    "project on action=describe must be an exact bounded Project: a non-empty authorized Project selector of at most "
    "512 bytes without control characters. Select the intended Project explicitly; do not infer it from a Session or recorder metadata.")
_PLUGIN_PROJECT_VALIDATION_ACTIONS = {
    _PLUGIN_PROJECT_VALIDATION_ERROR: ("call", "describe"),
    _PLUGIN_CALL_PROJECT_RECOVERY_ERROR: ("call",),
    _PLUGIN_DESCRIBE_PROJECT_RECOVERY_ERROR: ("describe",),
}


def _captured_plugin_rejection(root, identity, index, selected, audit_db):
    """Recognize an audited adapter rejection without assigning it a Session."""
    if audit_db is None or len(index) > 4096:
        return False
    scope_fields = ("client_window_key", "principal_correlation_kind", "principal_correlation_id")
    scopes = {tuple(event.get(key) for key in scope_fields) for event in selected
              if event.get("action_name") == "toolsCall"}
    if len(scopes) != 1 or not all(isinstance(value, str) and value for value in next(iter(scopes))):
        return False
    scope = next(iter(scopes))
    allowed_events = {"mcp_tool_request_received", "mcp_request_policy_selected", "mcp_tool_request_parsed",
        "tool_trace_diagnostic", "tool_trace_payload_captured", "mcp_tool_dispatch_started",
        "mcp_tool_dispatch_failed", "mcp_tool_dispatch_finished", "mcp_tool_response_serialized",
        "mcp_tool_handler_returned"}
    if any(item.get("event") not in allowed_events
           or item.get("phase") in {"effective_arguments", "kernel_arguments"}
           or (item.get("event") == "tool_trace_payload_captured"
               and item.get("phase") not in {"raw_arguments", "final_response"})
           or item.get("tool_name") not in (None, "plugin_tool")
           or item.get("client_window_key") not in (None, scope[0]) for item in index):
        return False
    for name in ("mcp_tool_dispatch_failed", "mcp_tool_dispatch_finished", "mcp_tool_handler_returned"):
        matches = [item for item in index if item.get("event") == name]
        if len(matches) != 1:
            return False
        event = matches[0]
        if event.get("client_window_key") != scope[0] or event.get("protocol_success") is not False:
            return False
        if name == "mcp_tool_handler_returned":
            if event.get("http_status") != 400 or event.get("category") != "bad_request":
                return False
        elif event.get("category") != "invalid_arguments" or event.get("tool_success") is not False:
            return False
    payload = trace._payloads(root, identity, frozenset(("raw_arguments", "final_response")))
    raw, final = payload["raw_arguments"], payload["final_response"]
    if (not isinstance(raw, dict) or raw.get("action") not in ("call", "describe")
            or not isinstance(final, dict) or "result" in final):
        return False
    error = final.get("error")
    if (not isinstance(error, dict) or set(error) != {"code", "message"}
            or type(error["code"]) is not int or error["code"] != -32602
            or not isinstance(error["message"], str)
            or raw["action"] not in _PLUGIN_PROJECT_VALIDATION_ACTIONS.get(error["message"], ())):
        return False
    connection = trace.report._open_sqlite_readonly(Path(audit_db))
    deadline = trace.time.monotonic() + 5
    try:
        connection.set_progress_handler(lambda: int(trace.time.monotonic() >= deadline), 10000)
        rows = connection.execute(f"""SELECT {trace.report._AUDIT_COLUMNS},
            EXISTS(SELECT 1 FROM action_event_workflow_links l WHERE l.event_id=e.event_id) AS linked
            FROM action_events e WHERE e.server_trace_id=? LIMIT 2""", (identity,)).fetchall()
        if len(rows) != 1 or rows[0]["linked"]:
            return False
        audit = trace.report._row_to_audit_event(rows[0])
        return (audit.get("action_name") == "toolsCall" and audit.get("operation") == "plugin_tool"
                and audit.get("status") == "failed"
                and tuple(audit.get(key) for key in scope_fields) == scope)
    finally:
        connection.close()


def _coverage(root, events, *, audit_db=None):
    trace._coverage(root, events)
    adapter_rejections = 0
    selected = {event.get("server_trace_id") for event in events if event.get("action_name") == "toolsCall"}
    for directory in root.iterdir():
        if directory.name == trace.CAPTURE_MARKER or directory.name in selected:
            continue
        index = [json.loads(line) for line in
                 trace._read(directory / "events.jsonl", trace.MAX_INDEX_BYTES).splitlines()]
        if any(isinstance(event.get("tool_name"), str) and event["tool_name"] not in SETUP
               for event in index):
            if not _captured_plugin_rejection(root, directory.name, index, events, audit_db):
                raise trace.EvidenceUnavailable("unlinked_runtime_call")
            adapter_rejections += 1
    return adapter_rejections


def check_answer(answer, fixture):
    """Frozen factual rubric; prompts request facts and source ranges as JSON."""
    if not isinstance(answer, dict) or set(answer) != {"facts", "sources"}:
        return False
    if len(json.dumps(answer, ensure_ascii=False).encode()) > MAX_ANSWER_BYTES:
        return False
    facts, citations = answer["facts"], answer["sources"]
    expected = fixture.expected_facts()
    if (not isinstance(facts, list) or any(not isinstance(fact, str) for fact in facts)
            or len(facts) != len(expected) or set(facts) != set(expected)
            or not isinstance(citations, list) or not 0 < len(citations) <= 12):
        return False
    sources = {source["path"]: source for source in fixture.sources()}
    covered = set()
    for item in citations:
        if not isinstance(item, dict) or set(item) != {"path", "start_line", "end_line"}:
            return False
        if not isinstance(item["path"], str):
            return False
        source = sources.get(item["path"])
        if source is None:
            return False
        start, end = item["start_line"], item["end_line"]
        lines = source["text"].splitlines()
        if type(start) is not int or type(end) is not int or not 1 <= start <= end <= len(lines):
            return False
        cited = "\n".join(lines[start - 1:end])
        covered.update(fact for fact in expected if fact in cited)
    return covered == set(expected)


def _skill_project_path(output):
    """Bind current locators or historical captures to the exact fixture folder."""
    canonical = "skill_path" in output or "project_path" in output
    if canonical:
        if "path" in output or output.get("source_scope") != "project":
            raise trace.EvidenceUnavailable("fixture_binding_mismatch")
        relative = output.get("skill_path")
    else:
        # Persisted pre-citation-contract captures used package-relative path.
        if output.get("source_scope", "project") != "project":
            raise trace.EvidenceUnavailable("fixture_binding_mismatch")
        relative = output.get("path", "SKILL.md")
    if (not isinstance(relative, str) or not relative or relative != relative.strip()
            or any(ord(char) < 32 or ord(char) == 127 for char in relative)
            or "\\" in relative or ":" in relative
            or any(part in {"", ".", ".."} for part in relative.split("/"))):
        raise trace.EvidenceUnavailable("fixture_binding_mismatch")
    # This folder is part of the pinned fixture, not derived from Skill metadata.
    project_path = SKILL.rsplit("/", 1)[0] + "/" + relative
    if canonical and output.get("project_path") != project_path:
        raise trace.EvidenceUnavailable("fixture_binding_mismatch")
    return project_path


def check_calls(calls, fixture, projects, advance=None):
    """Return truthful task/path checks, including observed model failures."""
    current_sources = {item["path"]: item for item in fixture.sources()}
    before_sources = {item["path"]: item for item in fixture.snapshot_sources(0)}
    versions = {0: before_sources, 1: current_sources}
    # Separate facts for adoption from task success. Search excerpts can already
    # contain enough source; do not prescribe a redundant read or fixed sequence.
    observed = {0: set(), 1: set()}
    plugin_sources, skill_sources = set(), set()
    descriptions, skill_ids = {}, set()
    first_sources = {}
    observed_facts = set()
    update_time = advance.get("at_ms") if isinstance(advance, dict) else None
    update_ok = fixture.case_id not in UPDATES
    if fixture.case_id in UPDATES:
        changed = [(path, data) for path, data in fixture.source_versions.items() if data[0] != data[1]]
        if len(changed) == 1 and fixture.advanced and type(update_time) is int:
            path, data = changed[0]
            trigger = next((call for call in calls if call["trace_id"] == advance.get("trigger_trace_id")), None)
            update_ok = (trigger is not None and trigger["ended"] <= update_time
                and advance.get("path") == path
                and advance.get("old_sha256") == hashlib.sha256(data[0]).hexdigest()
                and advance.get("new_sha256") == hashlib.sha256(data[1]).hexdigest())
    for call in calls:
        if call["result"]["success"] is not True:
            continue
        tool, args, output = call["tool"], call["arguments"], call["output"]
        stage = int(type(update_time) is int and call["started"] >= update_time)
        sources = versions[stage]
        if tool == "plugin_tool":
            if args.get("action") == "describe" and isinstance(output.get("binding"), str):
                spec = output.get("tool", {})
                if (output.get("project") not in projects or output.get("plugin") != "project-knowledge"
                        or not isinstance(spec, dict) or spec.get("name") not in {"index", "status", "search", "read"}):
                    raise trace.EvidenceUnavailable("project_mismatch")
                descriptions[spec["name"]] = call["ended"]
            if args.get("action") == "call":
                kind = "search" if isinstance(output.get("hits"), list) else "read" if "source_hash" in output else None
                if kind is not None and descriptions.get(kind, float("inf")) <= call["started"]:
                    items = output["hits"] if kind == "search" else [output]
                    for item in items:
                        if not isinstance(item, dict):
                            raise trace.EvidenceUnavailable("canonical_result_mismatch")
                        source = sources.get(item.get("path"))
                        if source is not None:
                            if not _source_match(item, source, hash_key="source_hash"):
                                raise trace.EvidenceUnavailable("fixture_binding_mismatch")
                            observed[stage].add(item["path"])
                            if stage == int(fixture.advanced):
                                plugin_sources.add(item["path"])
                            first_sources.setdefault(call["trace_id"], set()).add(item["path"])
                            if stage == int(fixture.advanced):
                                observed_facts.update(fact for fact in fixture.expected_facts() if fact in item["text"])
        elif tool in {"list_skills", "load_skill", "read_skill_file"}:
            for descriptor in output.get("skills", []):
                if descriptor.get("name") == "receipt-audit" and isinstance(descriptor.get("skill_id"), str):
                    skill_ids.add(descriptor["skill_id"])
            if output.get("name") == "receipt-audit" and isinstance(output.get("skill_id"), str):
                skill_ids.add(output["skill_id"])
            selected = output.get("skill_id") or args.get("skill_id")
            if selected in skill_ids and "text" in output:
                path = _skill_project_path(output)
                source = sources.get(path)
                if source is not None:
                    definition = sources.get(SKILL)
                    if (definition is None or output.get("definition_revision") != definition["source_hash"]
                            or not _source_match(output, source, hash_key="sha256")):
                        raise trace.EvidenceUnavailable("fixture_binding_mismatch")
                    observed[stage].add(path)
                    if stage == int(fixture.advanced):
                        skill_sources.add(path)
                    first_sources.setdefault(call["trace_id"], set()).add(path)
                    if stage == int(fixture.advanced):
                        observed_facts.update(fact for fact in fixture.expected_facts() if fact in output["text"])
        elif tool in {"read_files", "search_file_context"}:
            for item in _walk(output):
                path = item.get("path")
                source = sources.get(path) if isinstance(path, str) else None
                block = item.get("output", item)
                if source is not None and isinstance(block, dict) and "text" in block:
                    block = dict(block)
                    if tool == "read_files":
                        requested = args.get("items", [])
                        index = item.get("index")
                        if index is None:
                            matches = [entry for entry in requested if entry.get("path") == path]
                        else:
                            matches = ([requested[index]] if type(index) is int and 0 <= index < len(requested) else [])
                        if len(matches) != 1 or matches[0].get("path") != path:
                            raise trace.EvidenceUnavailable("canonical_call_mismatch")
                        # Canonical compact reads omit start_line=1/end_line.
                        block.setdefault("start_line", matches[0].get("start_line", 1))
                    if "end_line" not in block and isinstance(block.get("text"), str) and type(block.get("start_line")) is int:
                        block["end_line"] = block["start_line"] + len(block["text"].splitlines()) - 1
                    if not _source_match(block, source, numbered=(
                            tool == "read_files" and block.get("format") == "numbered")):
                        raise trace.EvidenceUnavailable("fixture_binding_mismatch")
                    observed[stage].add(path)
                    first_sources.setdefault(call["trace_id"], set()).add(path)
                    if stage == int(fixture.advanced):
                        observed_facts.update(fact for fact in fixture.expected_facts() if fact in block["text"])
    if fixture.case_id in UPDATES:
        update_ok = (update_ok and bool(first_sources.get(advance.get("trigger_trace_id")))
                     and advance.get("path") in observed[0])
    needed = set(fixture.required_sources)
    actual = observed[int(fixture.advanced)]
    required_path = (needed <= plugin_sources if fixture.case_id in DIRECTED_KNOWLEDGE
                     else needed <= skill_sources if fixture.case_id in DIRECTED_SKILL else True)
    return {"source_content_observed": needed <= actual and observed_facts == set(fixture.expected_facts()),
            "required_resource_path": required_path, "coordinator_update_order": bool(update_ok)}, {
        "knowledge_plugin": bool(plugin_sources), "skill": bool(skill_sources)}


def advance_after_read(*, capture_root, audit_db, workflow_session_id, fixture, trace_id):
    """Coordinator-only update after independently observing the old source."""
    root = Path(capture_root)
    if fixture.case_id not in UPDATES or fixture.advanced:
        raise ValueError("fixture update is unavailable")
    for parent in (root, *root.parents):
        if parent.is_symlink():
            raise trace.EvidenceUnavailable("unsafe_capture")
    trace._private(root, True)
    marker = json.loads(trace._read(root / trace.CAPTURE_MARKER, 4096))
    if (marker.get("case_id") != fixture.case_id
            or marker.get("fixture_identity") != trace.fixture_identity(fixture.root)):
        raise trace.EvidenceUnavailable("capture_identity_mismatch")
    events = trace._audit_events(audit_db, workflow_session_id)
    _coverage(root, events, audit_db=audit_db)
    calls = read_calls(events, root, workflow_session_id)
    projects = _bind_fixture(calls, fixture.root, workflow_session_id)
    selected = next((call for call in calls if call["trace_id"] == trace_id), None)
    if selected is None or selected["result"]["success"] is not True:
        raise ValueError("update requires the actual first read")
    # Checking the prefix rather than trusting a trigger declaration proves that
    # the source and its requested resource path were materialized before change.
    checks, _ = check_calls([call for call in calls if call["ended"] <= selected["ended"]], fixture, projects)
    if not checks["source_content_observed"] or not checks["required_resource_path"]:
        raise ValueError("first source observation is incomplete")
    import time
    record = fixture.advance()
    record.update(trigger_trace_id=trace_id, at_ms=int(time.time() * 1000))
    return record


def verify(*, capture_root, audit_db, workflow_session_id, fixture, base_revision,
           fingerprint, answer, advance=None):
    identity = None
    try:
        if fixture.case_id not in CASE_IDS:
            raise trace.EvidenceUnavailable("unsupported_case")
        if fixture.case_id in UPDATES and (not fixture.advanced or not isinstance(advance, dict)):
            raise trace.EvidenceUnavailable("missing_order")
        root = Path(capture_root)
        for parent in (root, *root.parents):
            if parent.is_symlink():
                raise trace.EvidenceUnavailable("unsafe_capture")
        trace._private(root, True)
        identity = trace.fixture_identity(fixture.root)
        marker = json.loads(trace._read(root / trace.CAPTURE_MARKER, 4096))
        if marker != {"case_id": fixture.case_id, "base_revision": base_revision,
                "case_fingerprint": fingerprint, "fixture_identity": identity}:
            raise trace.EvidenceUnavailable("capture_identity_mismatch")
        events = trace._audit_events(audit_db, workflow_session_id)
        adapter_rejections = _coverage(root, events, audit_db=audit_db)
        calls = read_calls(events, root, workflow_session_id)
        projects = _bind_fixture(calls, fixture.root, workflow_session_id)
        checks, adoption = check_calls(calls, fixture, projects, advance)
        checks.update(fixture_root_session_project=True, fixture_integrity=fixture.oracle()["passed"] is True,
                      answer_facts_and_citations=check_answer(answer, fixture))
        if trace.fixture_identity(fixture.root) != identity:
            raise trace.EvidenceUnavailable("fixture_identity_mismatch")
        return {"schema_version": 1, "case_id": fixture.case_id, "case_fingerprint": fingerprint,
            "base_revision": base_revision, "fixture_identity": identity, "available": True,
            "passed": all(checks.values()), "checks": checks, "resource_adoption": adoption,
            "canonical_call_count": len(calls), "adapter_rejection_count": adapter_rejections, "reason": None}
    except (OSError, ValueError, TypeError, KeyError, AttributeError, RecursionError,
            sqlite3.Error, subprocess.SubprocessError):
        return {"schema_version": 1, "case_id": fixture.case_id, "case_fingerprint": fingerprint,
            "base_revision": base_revision, "fixture_identity": identity, "available": False,
            "passed": None, "checks": {}, "resource_adoption": None,
            "canonical_call_count": None, "adapter_rejection_count": None, "reason": "knowledge_evidence_unavailable"}
