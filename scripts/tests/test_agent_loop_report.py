from __future__ import annotations

import copy
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

from scripts import agent_loop_report as report


def code_mode_composition(**overrides: object) -> dict[str, object]:
    value: dict[str, object] = {
        "nested_calls": 3,
        "nested_successes": 3,
        "nested_failures": 0,
        "max_in_flight": 1,
        "duration_ms": 15,
        "slot_wait_ms": 0,
        "returned_bytes": 60,
        "nested_raw_result_bytes_total": 2895,
        "nested_tool_counts": {"apply_text_edits": 1, "read_files": 2},
        "consequential_calls": 1,
        "known_results": 1,
        "job_handoffs": 0,
        "outcome_unknown": 0,
    }
    value.update(overrides)
    return value


class AgentLoopReportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.audit_db = self.root / "webcodex.db"
        connection = sqlite3.connect(self.audit_db)
        connection.executescript(
            """
            CREATE TABLE action_events (
                event_id TEXT PRIMARY KEY,
                operation TEXT,
                action_name TEXT,
                project TEXT,
                status TEXT,
                ids_json TEXT,
                summary_json TEXT,
                client_window_key TEXT,
                server_trace_id TEXT,
                principal_correlation_kind TEXT,
                principal_correlation_id TEXT,
                window_started_at_ms INTEGER,
                request_observed_at_ms INTEGER,
                response_handed_at_ms INTEGER,
                window_transition_kind TEXT,
                response_streaming INTEGER,
                window_continuity_eligible INTEGER,
                window_meaningful INTEGER,
                started_at INTEGER
            );
            CREATE TABLE action_event_workflow_links (
                event_id TEXT NOT NULL,
                workflow_session_id TEXT NOT NULL
            );
            """
        )
        connection.commit()
        connection.close()

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def insert_event(
        self,
        event_id: str,
        *,
        session: str = "wc_sess_test",
        tool: str = "read_files",
        status: str = "success",
        success: bool = True,
        meaningful: bool = True,
        started: int = 100,
        handed: int | None = 120,
        transition: str = "unavailable",
        eligible: bool = True,
        window: str = "hashed-window-a",
        principal: str = "principal-a",
        trace_id: str | None = None,
        result_bytes: int | None = 100,
        duration_ms: int = 5,
        action_name: str = "toolsCall",
        include_telemetry: bool = True,
        link: bool = True,
        ids: dict[str, object] | None = None,
        composition: dict[str, object] | None = None,
    ) -> None:
        telemetry: dict[str, object] = {
            "schema_version": 5,
            "tool_name": tool,
            "success": success,
            "duration_ms": duration_ms,
            "serialized_result_bytes": result_bytes,
        }
        if not success:
            telemetry.update(
                {
                    "error_kind": "validation_failed",
                    "failure_kind": "completed_failure",
                    "recovery_kind": "inspect_diagnostic",
                }
            )
        summary: dict[str, object] = {}
        if include_telemetry:
            summary["model_ergonomics"] = telemetry
        if composition is not None:
            summary["code_mode_composition"] = composition
        with sqlite3.connect(self.audit_db) as connection:
            connection.execute(
                """
                INSERT INTO action_events (
                    event_id, operation, action_name, project, status,
                    ids_json, summary_json, client_window_key, server_trace_id,
                    principal_correlation_kind, principal_correlation_id,
                    window_started_at_ms, request_observed_at_ms,
                    response_handed_at_ms, window_transition_kind,
                    response_streaming, window_continuity_eligible,
                    window_meaningful, started_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)
                """,
                (
                    event_id,
                    tool,
                    action_name,
                    "agent:test:project",
                    status,
                    json.dumps(ids or {}),
                    json.dumps(summary),
                    window,
                    trace_id,
                    "username",
                    principal,
                    started,
                    started,
                    handed,
                    transition,
                    int(eligible),
                    int(meaningful),
                    started // 1000,
                ),
            )
            if link:
                connection.execute(
                    "INSERT INTO action_event_workflow_links (event_id, workflow_session_id) VALUES (?, ?)",
                    (event_id, session),
                )

    def summarize(self, **kwargs: object) -> dict[str, object]:
        arguments: dict[str, object] = {
            "trace_root": None,
            "audit_db": self.audit_db,
            "workflow_session_id": "wc_sess_test",
            "case_manifest": None,
            "case_id": None,
            "variant": "direct",
            "surface": None,
            "base_revision": None,
        }
        arguments.update(kwargs)
        return report.summarize(**arguments)  # type: ignore[arg-type]

    def write_trace(self, trace_id: str, events: list[dict[str, object]]) -> Path:
        trace_root = self.root / "traces"
        trace_dir = trace_root / trace_id
        trace_dir.mkdir(parents=True, exist_ok=True)
        (trace_dir / "events.jsonl").write_text(
            "".join(json.dumps(event, sort_keys=True) + "\n" for event in events),
            encoding="utf-8",
        )
        return trace_root

    def write_annotation(
        self,
        name: str,
        *,
        case_id: str = "readonly_review",
        variant: str = "direct",
        surface: str = "direct",
        base_revision: str = "a" * 40,
        case_fingerprint: str | None = None,
        repair_turns: dict[str, object] | None = None,
        task_timing: dict[str, object] | None = None,
        correctness: dict[str, object] | None = None,
    ) -> Path:
        if case_fingerprint is None:
            manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
            case_fingerprint = report._case_fingerprint(report._case_by_id(manifest, case_id))
        value: dict[str, object] = {
            "schema_version": 1,
            "case_id": case_id,
            "variant": variant,
            "surface": surface,
            "base_revision": base_revision,
            "case_fingerprint": case_fingerprint,
        }
        if repair_turns is not None:
            value["repair_turns"] = repair_turns
        if task_timing is not None:
            value["task_timing"] = task_timing
        if correctness is not None:
            value["correctness"] = correctness
        annotation = self.root / name
        annotation.write_text(
            json.dumps(value, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        return annotation

    def test_multiple_traces_same_window_and_nonmeaningful_call_preserves_predecessor(self) -> None:
        self.insert_event("read_only", started=100, handed=120, trace_id="trace-a")
        self.insert_event(
            "noise",
            tool="tool_manifest",
            meaningful=False,
            started=130,
            handed=135,
            eligible=False,
            trace_id="trace-noise",
        )
        self.insert_event(
            "e2",
            tool="search_project_texts",
            started=150,
            handed=170,
            transition="serial",
            trace_id="trace-b",
        )

        result = self.summarize()

        self.assertEqual(result["outer_calls"]["total"], 3)
        self.assertEqual(result["outer_calls"]["meaningful"], 2)
        gap = result["timing"]["outside_webcodex_gap_ms"]
        self.assertEqual(gap["samples"], 1)
        self.assertEqual(gap["total"], 30)
        self.assertEqual(gap["p50"], 30)
        self.assertEqual(result["canonical_calls"]["total"], 3)

    def test_overlap_is_counted_without_fabricating_negative_gap(self) -> None:
        self.insert_event("read_only", started=100, handed=180)
        self.insert_event("e2", started=150, handed=190, transition="overlap")
        self.insert_event("e3", started=210, handed=220, transition="serial")

        result = self.summarize()

        self.assertEqual(result["timing"]["overlap_count"], 1)
        gap = result["timing"]["outside_webcodex_gap_ms"]
        self.assertEqual(gap["samples"], 1)
        self.assertEqual(gap["total"], 20)

    def test_failed_call_and_result_bytes_are_aggregated_from_telemetry(self) -> None:
        self.insert_event("ok", result_bytes=120, duration_ms=3)
        self.insert_event(
            "failed",
            tool="cargo_test",
            status="failed",
            success=False,
            started=150,
            handed=180,
            transition="serial",
            result_bytes=80,
            duration_ms=9,
        )

        result = self.summarize()

        self.assertEqual(result["outer_calls"]["successful"], 1)
        self.assertEqual(result["outer_calls"]["failed"], 1)
        self.assertEqual(result["results"]["serialized_tool_result_bytes"]["total"], 200)
        self.assertEqual(
            result["failures"]["failure_kind_by_name"],
            {"completed_failure": 1},
        )
        self.assertEqual(
            result["failures"]["recovery_guidance_by_kind"],
            {"inspect_diagnostic": 1},
        )
        self.assertIsNone(result["failures"]["resolved_recoveries"])

    def test_missing_optional_result_bytes_never_become_zero(self) -> None:
        self.insert_event("known", result_bytes=90)
        self.insert_event(
            "missing",
            status="failed",
            success=False,
            result_bytes=None,
            started=150,
            handed=170,
            transition="serial",
        )

        result = self.summarize()
        metric = result["results"]["serialized_tool_result_bytes"]

        self.assertIsNone(metric["total"])
        self.assertEqual(metric["observed_total"], 90)
        self.assertEqual(metric["missing"], 1)
        self.assertFalse(result["availability"]["serialized_tool_result_bytes"]["available"])

    def test_runner_request_count_dedupes_duplicate_and_out_of_order_events(self) -> None:
        self.insert_event("read_only", trace_id="trace-a")
        self.insert_event(
            "e2",
            started=150,
            handed=170,
            transition="serial",
            trace_id="trace-b",
        )
        event_b = {
            "event": "tool_runner_request_enqueued",
            "server_trace_id": "trace-b",
            "runner_request_id": "req-b",
            "runner_request_kind": "read_files",
            "runner_job_id": "job-shared",
        }
        event_a = {
            "event": "tool_runner_request_enqueued",
            "server_trace_id": "trace-a",
            "runner_request_id": "req-a",
            "runner_request_kind": "search_project_texts",
            "runner_job_id": "job-shared",
        }
        trace_root = self.write_trace("trace-a", [event_b, event_a, event_a])

        result = self.summarize(trace_root=trace_root)

        self.assertEqual(result["runner"]["requests_observed"], 2)
        self.assertEqual(
            result["runner"]["by_kind"],
            {"read_files": 1, "search_project_texts": 1},
        )
        self.assertFalse(result["availability"]["runner_requests"]["complete"])
        self.assertEqual(result["jobs"]["runner_job_ids_observed"], 1)
        self.assertIsNone(result["jobs"]["handoffs"])
        self.assertFalse(result["availability"]["job_handoffs"]["available"])

    def test_trace_filter_excludes_unrelated_session_trace(self) -> None:
        self.insert_event("selected", trace_id="trace-selected")
        self.insert_event(
            "other",
            session="wc_sess_other",
            trace_id="trace-other",
            window="hashed-window-other",
            principal="principal-other",
        )
        trace_root = self.write_trace(
            "mixed",
            [
                {
                    "event": "tool_runner_request_enqueued",
                    "server_trace_id": "trace-selected",
                    "runner_request_id": "req-selected",
                    "runner_request_kind": "read_files",
                },
                {
                    "event": "tool_runner_request_enqueued",
                    "server_trace_id": "trace-other",
                    "runner_request_id": "req-other",
                    "runner_request_kind": "run_process",
                },
            ],
        )

        result = self.summarize(trace_root=trace_root)

        self.assertEqual(result["outer_calls"]["total"], 1)
        self.assertEqual(result["runner"]["requests_observed"], 1)
        self.assertEqual(result["runner"]["by_kind"], {"read_files": 1})

    def test_empty_session_selection_excludes_unrelated_trace_supplement(self) -> None:
        self.insert_event("other", session="wc_sess_other", trace_id="trace-other")
        trace_root = self.write_trace(
            "trace-other",
            [
                {
                    "event": "tool_runner_request_enqueued",
                    "server_trace_id": "trace-other",
                    "runner_request_id": "req-other",
                    "runner_request_kind": "run_process",
                }
            ],
        )

        result = self.summarize(
            trace_root=trace_root,
            workflow_session_id="wc_sess_empty",
        )

        self.assertEqual(result["evidence"]["audit_events"], 0)
        self.assertEqual(result["outer_calls"]["total"], 0)
        self.assertEqual(result["runner"]["requests_observed"], 0)
        self.assertEqual(result["runner"]["by_kind"], {})

    def test_workflow_session_selection_requires_action_audit(self) -> None:
        trace_root = self.write_trace(
            "trace-a",
            [{"event": "tool_handler_returned", "server_trace_id": "trace-a"}],
        )

        with self.assertRaisesRegex(report.ReportError, "requires --audit-db"):
            report.summarize(
                trace_root=trace_root,
                audit_db=None,
                workflow_session_id="wc_sess_test",
                case_manifest=None,
                case_id=None,
                variant=None,
                surface=None,
                base_revision=None,
            )

    def test_empty_trace_root_does_not_turn_missing_runner_evidence_into_zero(self) -> None:
        trace_root = self.root / "empty-traces"
        trace_root.mkdir()

        result = report.summarize(
            trace_root=trace_root,
            audit_db=None,
            workflow_session_id=None,
            case_manifest=None,
            case_id=None,
            variant=None,
            surface=None,
            base_revision=None,
        )

        self.assertIsNone(result["runner"]["requests_observed"])
        self.assertFalse(result["availability"]["runner_requests"]["available"])
        self.assertIsNone(result["jobs"]["runner_job_ids_observed"])

    def test_malformed_jsonl_fails_closed(self) -> None:
        trace_root = self.root / "traces"
        path = trace_root / "trace-a" / "events.jsonl"
        path.parent.mkdir(parents=True)
        path.write_text('{"event":"ok"}\nnot-json\n', encoding="utf-8")

        with self.assertRaisesRegex(report.ReportError, "malformed JSONL"):
            report.load_trace_events(trace_root)

    def test_duplicate_trace_event_is_ignored_deterministically(self) -> None:
        event = {
            "event": "tool_runner_request_enqueued",
            "server_trace_id": "trace-a",
            "runner_request_id": "req-a",
            "runner_request_kind": "read_files",
        }
        trace_root = self.write_trace("trace-a", [event, event])

        trace_ids, events, files = report.load_trace_events(trace_root)

        self.assertEqual(trace_ids, {"trace-a"})
        self.assertEqual(len(events), 1)
        self.assertEqual(files, 1)

    def test_other_session_meaningful_call_blocks_selected_session_gap(self) -> None:
        self.insert_event(
            "selected-1",
            session="wc_sess_test",
            started=100,
            handed=120,
            window="hashed-window-a",
        )
        self.insert_event(
            "other-session",
            session="wc_sess_other",
            tool="run_process",
            started=150,
            handed=170,
            transition="serial",
            window="hashed-window-a",
        )
        self.insert_event(
            "selected-2",
            session="wc_sess_test",
            started=200,
            handed=220,
            transition="serial",
            window="hashed-window-a",
        )
        with sqlite3.connect(self.audit_db) as connection:
            connection.execute(
                "UPDATE action_events SET summary_json = 'not-json' WHERE event_id = 'other-session'"
            )

        result = self.summarize()
        metric = result["timing"]["outside_webcodex_gap_ms"]

        self.assertEqual(result["outer_calls"]["total"], 2)
        self.assertEqual(result["tools"]["outer_by_name"], {"read_files": 2})
        self.assertIsNone(metric["total"])
        self.assertEqual(metric["observed_total"], 0)
        self.assertEqual(metric["samples"], 0)
        self.assertEqual(metric["missing"], 1)
        self.assertFalse(result["availability"]["window_timing"]["available"])

    def test_unrelated_window_does_not_participate_in_gap(self) -> None:
        self.insert_event("a1", started=100, handed=120, window="hashed-window-a")
        self.insert_event(
            "b1",
            started=130,
            handed=140,
            transition="serial",
            window="hashed-window-b",
        )
        self.insert_event(
            "a2",
            started=160,
            handed=180,
            transition="serial",
            window="hashed-window-a",
        )

        result = self.summarize()
        metric = result["timing"]["outside_webcodex_gap_ms"]

        self.assertIsNone(metric["total"])
        self.assertEqual(metric["observed_total"], 40)
        self.assertEqual(metric["samples"], 1)
        self.assertEqual(metric["missing"], 1)

    def test_trace_only_marks_window_bytes_and_canonical_calls_unavailable(self) -> None:
        trace_root = self.write_trace(
            "trace-a",
            [
                {
                    "event": "tool_handler_returned",
                    "server_trace_id": "trace-a",
                    "tool_name": "read_files",
                    "tool_success": True,
                    "duration_ms": 8,
                    "response_bytes_estimate": 999,
                }
            ],
        )

        result = report.summarize(
            trace_root=trace_root,
            audit_db=None,
            workflow_session_id=None,
            case_manifest=None,
            case_id=None,
            variant=None,
            surface=None,
            base_revision=None,
        )

        self.assertEqual(result["outer_calls"]["total"], 1)
        self.assertIsNone(result["outer_calls"]["meaningful"])
        self.assertIsNone(result["model_round_trips"])
        self.assertFalse(result["availability"]["model_round_trips"]["available"])
        self.assertIsNone(result["canonical_calls"]["total"])
        self.assertIsNone(result["results"]["serialized_tool_result_bytes"]["total"])
        self.assertFalse(result["availability"]["window_timing"]["available"])
        self.assertIsNone(result["timing"]["webcodex_service_ms"]["total"])
        self.assertFalse(result["availability"]["webcodex_service_timing"]["available"])

    def test_code_mode_canonical_count_stays_separate_from_persisted_child_count(self) -> None:
        self.insert_event(
            "code",
            tool="code_mode_exec_mutating",
            composition=code_mode_composition(),
        )
        self.insert_event("validation", tool="cargo_check", started=140, handed=160)
        result = self.summarize(variant="code_mode")

        self.assertIsNone(result["canonical_calls"]["total"])
        self.assertEqual(result["canonical_calls"]["observed_outer_runtime_records"], 2)
        self.assertFalse(result["availability"]["canonical_calls"]["available"])
        self.assertTrue(result["availability"]["code_mode_composition"]["available"])
        composition = result["composition"]
        self.assertEqual(composition["outer_code_mode_calls"], 1)
        self.assertEqual(composition["outer_calls_with_summary"], 1)
        self.assertEqual(composition["nested_calls"]["total"], 3)
        self.assertEqual(
            composition["nested_tool_counts"],
            {"apply_text_edits": 1, "read_files": 2},
        )

    def test_code_mode_composition_aggregates_effect_and_projection_counters(self) -> None:
        self.insert_event(
            "guarded_edit",
            tool="code_mode_exec_mutating",
            composition=code_mode_composition(input_bytes=80),
        )
        self.insert_event(
            "validation",
            tool="code_mode_exec_effectful",
            started=140,
            handed=170,
            composition=code_mode_composition(
                nested_calls=1,
                nested_successes=1,
                nested_failures=0,
                duration_ms=20,
                input_bytes=40,
                returned_bytes=30,
                nested_raw_result_bytes_total=1200,
                nested_tool_counts={"cargo_check": 1},
                consequential_calls=1,
                known_results=0,
                job_handoffs=1,
            ),
        )
        result = self.summarize(variant="code_mode")
        composition = result["composition"]

        self.assertEqual(composition["nested_calls"]["total"], 4)
        self.assertEqual(composition["consequential_calls"]["total"], 2)
        self.assertEqual(composition["known_results"]["total"], 1)
        self.assertEqual(composition["job_handoffs"]["total"], 1)
        self.assertEqual(composition["outcome_unknown"]["total"], 0)
        self.assertEqual(composition["duration_ms"]["total"], 35)
        self.assertEqual(composition["input_bytes"]["total"], 120)
        self.assertEqual(composition["returned_bytes"]["total"], 90)
        self.assertEqual(composition["nested_raw_result_bytes_total"]["total"], 4095)
        self.assertEqual(
            composition["nested_tool_counts"],
            {"apply_text_edits": 1, "cargo_check": 1, "read_files": 2},
        )

    def test_historical_code_mode_composition_without_input_bytes_stays_valid(self) -> None:
        self.insert_event(
            "historical",
            tool="code_mode_exec",
            composition=code_mode_composition(),
        )
        result = self.summarize(variant="code_mode")

        self.assertTrue(result["availability"]["code_mode_composition"]["available"])
        self.assertEqual(result["composition"]["nested_calls"]["total"], 3)
        input_bytes = result["composition"]["input_bytes"]
        self.assertIsNone(input_bytes["total"])
        self.assertEqual(input_bytes["observed_total"], 0)
        self.assertEqual(input_bytes["missing"], 1)

    def test_direct_variant_with_code_mode_call_fails_composition_closed(self) -> None:
        self.insert_event(
            "code",
            tool="code_mode_exec",
            composition=code_mode_composition(
                consequential_calls=0,
                known_results=0,
            ),
        )
        result = self.summarize(variant="direct")

        self.assertFalse(result["availability"]["code_mode_composition"]["available"])
        self.assertIsNone(result["composition"]["nested_calls"]["total"])

    def test_missing_code_mode_composition_fails_closed(self) -> None:
        self.insert_event("code", tool="code_mode_exec_mutating")
        result = self.summarize(variant="code_mode")

        self.assertFalse(result["availability"]["code_mode_composition"]["available"])
        metric = result["composition"]["nested_calls"]
        self.assertIsNone(metric["total"])
        self.assertEqual(metric["observed_total"], 0)
        self.assertEqual(metric["missing"], 1)

    def test_code_mode_variant_without_code_mode_outer_call_flags_selection_gap(self) -> None:
        self.insert_event("read", tool="read_files")
        result = self.summarize(variant="code_mode")

        availability = result["availability"]["code_mode_composition"]
        self.assertFalse(availability["available"])
        self.assertIn("recording_session_id", availability["reason"])
        self.assertIsNone(result["composition"]["nested_calls"]["total"])

    def test_comparison_with_missing_metric_is_not_comparable(self) -> None:
        self.insert_event("direct")
        direct = self.summarize(variant="direct")
        candidate = copy.deepcopy(direct)
        candidate["canonical_calls"]["total"] = None

        comparison = report.compare_reports(direct, candidate)
        canonical = next(
            metric
            for metric in comparison["metrics"]
            if metric["metric"] == "canonical_calls.total"
        )

        self.assertFalse(canonical["comparable"])
        self.assertIsNone(canonical["delta"])

    def test_nearest_rank_percentiles_are_deterministic(self) -> None:
        values = [4, 1, 3, 2]
        self.assertEqual(report._nearest_rank(values, 0.50), 2)
        self.assertEqual(report._nearest_rank(values, 0.95), 4)
        self.assertIsNone(report._nearest_rank([], 0.95))

    def test_manifest_validation_rejects_duplicate_case_ids(self) -> None:
        manifest = {
            "schema_version": 1,
            "cases": [
                {
                    "id": "same",
                    "title": "A",
                    "target": "repo",
                    "prompt": "do a",
                    "correctness": {"required": True},
                    "validation": {"required": False},
                },
                {
                    "id": "same",
                    "title": "B",
                    "target": "repo",
                    "prompt": "do b",
                    "correctness": {"required": True},
                    "validation": {"required": False},
                },
            ],
        }

        with self.assertRaisesRegex(report.ReportError, "duplicate case id"):
            report.validate_case_manifest(manifest)

    def test_manifest_requires_fields_and_stable_serialization(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        serialized_once = report._stable_json(manifest)
        serialized_twice = report._stable_json(json.loads(serialized_once))
        self.assertEqual(serialized_once, serialized_twice)
        self.assertEqual(len(manifest["cases"]), 10)
        typed_cases = [case for case in manifest["cases"] if "code_mode_surface" in case]
        self.assertEqual(len(typed_cases), 10)
        self.assertEqual(
            {case["code_mode_surface"] for case in typed_cases},
            {"read_only", "validation", "guarded_edit"},
        )

        legacy = copy.deepcopy(manifest)
        legacy_case = legacy["cases"][0]
        legacy_case["code_mode_surface"] = "e1"
        legacy_fingerprint = report._case_fingerprint(legacy_case)
        validated_legacy = report.validate_case_manifest(legacy)
        self.assertEqual(validated_legacy["cases"][0]["code_mode_surface"], "e1")
        self.assertEqual(
            report._case_fingerprint(validated_legacy["cases"][0]), legacy_fingerprint
        )

        broken = copy.deepcopy(manifest)
        del broken["cases"][0]["validation"]
        with self.assertRaisesRegex(report.ReportError, "validation"):
            report.validate_case_manifest(broken)

        bad_surface = copy.deepcopy(manifest)
        bad_surface["cases"][-1]["code_mode_surface"] = "generic_code_mode"
        with self.assertRaisesRegex(report.ReportError, "code_mode_surface"):
            report.validate_case_manifest(bad_surface)

        bad_surface_type = copy.deepcopy(manifest)
        bad_surface_type["cases"][-1]["code_mode_surface"] = ["read_only"]
        with self.assertRaisesRegex(report.ReportError, "code_mode_surface"):
            report.validate_case_manifest(bad_surface_type)

        bad_focus = copy.deepcopy(manifest)
        bad_focus["cases"][-1]["dogfood_focus"] = ["same", "same"]
        with self.assertRaisesRegex(report.ReportError, "dogfood_focus"):
            report.validate_case_manifest(bad_focus)
        bad_validation_required = copy.deepcopy(manifest)
        bad_validation_required["cases"][0]["validation"]["required"] = "false"
        with self.assertRaisesRegex(report.ReportError, "validation.required"):
            report.validate_case_manifest(bad_validation_required)

    def test_benchmark_case_requires_exact_git_base_revision(self) -> None:
        with self.assertRaisesRegex(report.ReportError, "exact 40-hex Git commit"):
            report._benchmark_metadata(
                case_manifest=None,
                case_id="focused_edit_validation",
                variant="direct",
                surface=None,
                base_revision="main",
            )

    def test_benchmark_code_mode_requires_explicit_surface(self) -> None:
        with self.assertRaisesRegex(report.ReportError, "require --surface"):
            report._benchmark_metadata(
                case_manifest=None,
                case_id="focused_edit_validation",
                variant="code_mode",
                surface=None,
                base_revision="a" * 40,
            )

        metadata = report._benchmark_metadata(
            case_manifest=None,
            case_id="focused_edit_validation",
            variant="code_mode",
            surface="guarded_edit",
            base_revision="a" * 40,
        )
        self.assertEqual(metadata["surface"], "guarded_edit")

        legacy_metadata = report._benchmark_metadata(
            case_manifest=None,
            case_id="focused_edit_validation",
            variant="code_mode",
            surface="e2b",
            base_revision="a" * 40,
        )
        self.assertEqual(legacy_metadata["surface"], "e2b")

    def test_benchmark_case_rejects_wrong_declared_code_mode_surface(self) -> None:
        with self.assertRaisesRegex(
            report.ReportError,
            "requires Code Mode surface read_only",
        ):
            report._benchmark_metadata(
                case_manifest=None,
                case_id="readonly_review",
                variant="code_mode",
                surface="validation",
                base_revision="a" * 40,
            )

    def test_benchmark_direct_surface_is_inferred(self) -> None:
        metadata = report._benchmark_metadata(
            case_manifest=None,
            case_id="readonly_review",
            variant="direct",
            surface=None,
            base_revision="a" * 40,
        )
        self.assertEqual(metadata["surface"], "direct")

    def test_run_annotation_validation_is_bounded_and_payload_safe(self) -> None:
        value = {
            "schema_version": 1,
            "case_id": "readonly_review",
            "variant": "direct",
            "surface": "direct",
            "base_revision": "a" * 40,
            "case_fingerprint": report._case_fingerprint(
                report._case_by_id(
                    report.load_case_manifest(report.DEFAULT_CASE_MANIFEST),
                    "readonly_review",
                )
            ),
            "repair_turns": {
                "total": 2,
                "by_reason": {
                    "invalid_arguments": 1,
                    "wrong_result_shape": 1,
                },
            },
            "task_timing": {"started_at_ms": 1000, "ended_at_ms": 1450},
            "correctness": {
                "task_verdict": "pass",
                "validation_verdict": "not_required",
            },
        }
        self.assertEqual(report.validate_run_annotation(copy.deepcopy(value)), value)

        legacy = copy.deepcopy(value)
        legacy["variant"] = "code_mode"
        legacy["surface"] = "e1"
        self.assertEqual(report.validate_run_annotation(copy.deepcopy(legacy)), legacy)

        leaked = copy.deepcopy(value)
        leaked["session_id"] = "wc_sess_should_not_be_stored"
        with self.assertRaisesRegex(report.ReportError, "unsupported fields"):
            report.validate_run_annotation(leaked)

        bad_reason = copy.deepcopy(value)
        bad_reason["repair_turns"]["by_reason"] = {"speculative_future_taxonomy": 2}  # type: ignore[index]
        with self.assertRaisesRegex(report.ReportError, "unsupported repair reason"):
            report.validate_run_annotation(bad_reason)
        missing_fingerprint = copy.deepcopy(value)
        del missing_fingerprint["case_fingerprint"]
        with self.assertRaisesRegex(report.ReportError, "case_fingerprint"):
            report.validate_run_annotation(missing_fingerprint)

    def test_annotation_metrics_remain_unavailable_without_sidecar(self) -> None:
        self.insert_event("event")
        result = self.summarize(
            case_id="readonly_review",
            base_revision="a" * 40,
        )

        self.assertIsNone(result["model_round_trips"])
        self.assertEqual(result["model_round_trip_proxy"], 1)
        self.assertFalse(result["availability"]["model_round_trips"]["available"])
        self.assertTrue(result["availability"]["model_round_trip_proxy"]["available"])
        self.assertIsNone(result["repair_turns"]["total"])
        self.assertIsNone(result["task_wall_time_ms"])
        self.assertIsNone(result["correctness"]["task_verdict"])
        self.assertFalse(result["availability"]["repair_turns"]["available"])
        self.assertFalse(result["availability"]["task_wall_time"]["available"])
        self.assertFalse(result["availability"]["correctness"]["available"])

    def test_run_annotation_projects_repair_wall_time_and_correctness(self) -> None:
        self.insert_event("event")
        annotation = self.write_annotation(
            "direct-annotation.json",
            repair_turns={
                "total": 1,
                "by_reason": {"invalid_arguments": 1},
            },
            task_timing={"started_at_ms": 1000, "ended_at_ms": 1625},
            correctness={
                "task_verdict": "pass",
                "validation_verdict": "not_required",
            },
        )
        result = self.summarize(
            case_id="readonly_review",
            base_revision="a" * 40,
            run_annotation=annotation,
        )

        self.assertEqual(result["repair_turns"]["total"], 1)
        self.assertEqual(
            result["repair_turns"]["by_reason"],
            {"invalid_arguments": 1},
        )
        self.assertEqual(result["task_wall_time_ms"], 625)
        self.assertEqual(result["correctness"]["task_verdict"], "pass")
        self.assertTrue(result["availability"]["repair_turns"]["available"])
        self.assertTrue(result["availability"]["task_wall_time"]["available"])
        self.assertTrue(result["availability"]["correctness"]["available"])

    def test_run_annotation_must_match_case_variant_surface_and_base(self) -> None:
        self.insert_event("event")
        annotation = self.write_annotation(
            "mismatched.json",
            base_revision="b" * 40,
        )
        with self.assertRaisesRegex(report.ReportError, "base_revision does not match"):
            self.summarize(
                case_id="readonly_review",
                base_revision="a" * 40,
                run_annotation=annotation,
            )
        stale_case_annotation = self.write_annotation(
            "stale-case.json",
            case_fingerprint="0" * 64,
        )
        with self.assertRaisesRegex(report.ReportError, "case_fingerprint does not match"):
            self.summarize(
                case_id="readonly_review",
                base_revision="a" * 40,
                run_annotation=stale_case_annotation,
            )

    def test_failed_child_calls_use_authoritative_composition_but_kinds_stay_unavailable(self) -> None:
        self.insert_event(
            "code",
            tool="code_mode_exec",
            composition=code_mode_composition(
                nested_successes=2,
                nested_failures=1,
            ),
        )
        result = self.summarize(variant="code_mode")

        self.assertEqual(result["child_calls"]["failed"], 1)
        self.assertTrue(result["availability"]["failed_child_calls"]["available"])
        self.assertIsNone(result["failures"]["child_failure_kind_by_name"])
        self.assertFalse(result["availability"]["child_failure_kinds"]["available"])

    def test_paired_comparison_exposes_correctness_gate_and_required_deltas(self) -> None:
        base_revision = "c" * 40
        self.insert_event(
            "direct-failed",
            session="wc_sess_direct",
            status="failed",
            success=False,
            started=100,
            handed=120,
        )
        self.insert_event(
            "direct-repair",
            session="wc_sess_direct",
            started=150,
            handed=170,
            transition="serial",
        )
        self.insert_event(
            "code",
            session="wc_sess_code",
            tool="code_mode_exec",
            composition=code_mode_composition(),
        )
        direct_annotation = self.write_annotation(
            "direct-paired.json",
            base_revision=base_revision,
            repair_turns={
                "total": 1,
                "by_reason": {"invalid_arguments": 1},
            },
            task_timing={"started_at_ms": 0, "ended_at_ms": 1000},
            correctness={
                "task_verdict": "pass",
                "validation_verdict": "not_required",
            },
        )
        code_annotation = self.write_annotation(
            "code-paired.json",
            variant="code_mode",
            surface="read_only",
            base_revision=base_revision,
            repair_turns={"total": 0, "by_reason": {}},
            task_timing={"started_at_ms": 0, "ended_at_ms": 700},
            correctness={
                "task_verdict": "pass",
                "validation_verdict": "not_required",
            },
        )
        direct = self.summarize(
            workflow_session_id="wc_sess_direct",
            case_id="readonly_review",
            variant="direct",
            surface="direct",
            base_revision=base_revision,
            run_annotation=direct_annotation,
        )
        code = self.summarize(
            workflow_session_id="wc_sess_code",
            case_id="readonly_review",
            variant="code_mode",
            surface="read_only",
            base_revision=base_revision,
            run_annotation=code_annotation,
        )
        comparison = report.compare_reports(direct, code)
        metrics = {item["metric"]: item for item in comparison["metrics"]}

        self.assertTrue(comparison["case_compatibility"]["comparable"])
        self.assertTrue(comparison["pair_compatibility"]["comparable"])
        legacy_code = copy.deepcopy(code)
        legacy_code["benchmark"]["surface"] = "e1"
        self.assertTrue(
            report.compare_reports(direct, legacy_code)["pair_compatibility"]["comparable"]
        )
        self.assertTrue(comparison["correctness_compatibility"]["comparable"])
        self.assertTrue(comparison["throughput_compatibility"]["comparable"])
        self.assertFalse(metrics["model_round_trips"]["comparable"])
        self.assertIsNone(metrics["model_round_trips"]["delta"])
        self.assertEqual(metrics["model_round_trip_proxy"]["delta"], -1)
        self.assertEqual(metrics["repair_turns.total"]["delta"], -1)
        self.assertEqual(metrics["outer_calls.failed"]["delta"], -1)
        self.assertEqual(metrics["task_wall_time_ms"]["delta"], -300)
        self.assertEqual(metrics["child_calls.failed"]["candidate"], 0)
        self.assertEqual(metrics["composition.max_in_flight.total"]["candidate"], 1)
        self.assertEqual(
            metrics["composition.nested_raw_result_bytes_total.total"]["candidate"],
            2895,
        )
        self.assertEqual(metrics["composition.returned_bytes.total"]["candidate"], 60)
        self.assertEqual(
            comparison["repair_turns"]["baseline_by_reason"],
            {"invalid_arguments": 1},
        )
        self.assertEqual(
            comparison["contract_surface_evidence"]["baseline"]["failure_kind_by_name"],
            {"completed_failure": 1},
        )
        self.assertEqual(
            report._stable_json(comparison),
            report._stable_json(copy.deepcopy(comparison)),
        )

    def test_required_validation_must_pass_correctness_gate(self) -> None:
        base_revision = "e" * 40
        self.insert_event("direct", session="wc_sess_direct")
        self.insert_event(
            "code",
            session="wc_sess_code",
            tool="code_mode_exec_mutating",
            composition=code_mode_composition(
                nested_calls=1,
                nested_successes=1,
                nested_failures=0,
                nested_tool_counts={"apply_text_edits": 1},
                consequential_calls=1,
                known_results=1,
            ),
        )
        direct = self.summarize(
            workflow_session_id="wc_sess_direct",
            case_id="focused_edit_validation",
            variant="direct",
            surface="direct",
            base_revision=base_revision,
            run_annotation=self.write_annotation(
                "direct-validation.json",
                case_id="focused_edit_validation",
                base_revision=base_revision,
                correctness={
                    "task_verdict": "pass",
                    "validation_verdict": "pass",
                },
            ),
        )
        code = self.summarize(
            workflow_session_id="wc_sess_code",
            case_id="focused_edit_validation",
            variant="code_mode",
            surface="guarded_edit",
            base_revision=base_revision,
            run_annotation=self.write_annotation(
                "code-validation.json",
                case_id="focused_edit_validation",
                variant="code_mode",
                surface="guarded_edit",
                base_revision=base_revision,
                correctness={
                    "task_verdict": "pass",
                    "validation_verdict": "not_required",
                },
            ),
        )
        comparison = report.compare_reports(direct, code)

        self.assertFalse(comparison["correctness_compatibility"]["comparable"])
        self.assertIn(
            "required validation verdict is not pass",
            comparison["correctness_compatibility"]["reason"],
        )
        self.assertFalse(comparison["throughput_compatibility"]["comparable"])

    def test_correctness_failure_closes_throughput_gate_without_hiding_metrics(self) -> None:
        base_revision = "d" * 40
        self.insert_event("direct", session="wc_sess_direct")
        self.insert_event(
            "code",
            session="wc_sess_code",
            tool="code_mode_exec",
            composition=code_mode_composition(),
        )
        direct = self.summarize(
            workflow_session_id="wc_sess_direct",
            case_id="readonly_review",
            variant="direct",
            surface="direct",
            base_revision=base_revision,
            run_annotation=self.write_annotation(
                "direct-good.json",
                base_revision=base_revision,
                repair_turns={"total": 0, "by_reason": {}},
                correctness={
                    "task_verdict": "pass",
                    "validation_verdict": "not_required",
                },
            ),
        )
        code = self.summarize(
            workflow_session_id="wc_sess_code",
            case_id="readonly_review",
            variant="code_mode",
            surface="read_only",
            base_revision=base_revision,
            run_annotation=self.write_annotation(
                "code-bad.json",
                variant="code_mode",
                surface="read_only",
                base_revision=base_revision,
                repair_turns={"total": 0, "by_reason": {}},
                correctness={
                    "task_verdict": "fail",
                    "validation_verdict": "not_required",
                },
            ),
        )
        comparison = report.compare_reports(direct, code)

        self.assertFalse(comparison["correctness_compatibility"]["comparable"])
        self.assertFalse(comparison["throughput_compatibility"]["comparable"])
        metric = next(
            item
            for item in comparison["metrics"]
            if item["metric"] == "model_round_trip_proxy"
        )
        self.assertTrue(metric["comparable"])

    def test_report_privacy_omits_raw_runtime_identity_and_payload_sentinels(self) -> None:
        self.insert_event(
            "event",
            session="wc_sess_private_sentinel",
            window="raw-window-private-sentinel",
            principal="principal-private-sentinel",
            trace_id="trace-private-sentinel",
            ids={
                "command": "secret-command-sentinel",
                "payload": "secret-payload-sentinel",
                "tunnel": "tunnel-private-sentinel",
            },
        )
        result = self.summarize(
            workflow_session_id="wc_sess_private_sentinel",
        )
        serialized = report._stable_json(result)

        for forbidden in (
            "wc_sess_private_sentinel",
            "raw-window-private-sentinel",
            "principal-private-sentinel",
            "trace-private-sentinel",
            "secret-command-sentinel",
            "secret-payload-sentinel",
            "tunnel-private-sentinel",
            "agent:test:project",
        ):
            self.assertNotIn(forbidden, serialized)

    def test_compare_case_compatibility_requires_exact_git_base_revision(self) -> None:
        baseline = {"benchmark": {"case_id": "focused_edit_validation", "base_revision": "main"}}
        candidate = {"benchmark": {"case_id": "focused_edit_validation", "base_revision": "main"}}
        self.assertEqual(
            report._case_compatibility(baseline, candidate),
            {
                "comparable": False,
                "reason": "both reports must record an exact 40-hex Git base revision",
            },
        )

    def test_compare_case_compatibility_rejects_changed_case_definition(self) -> None:
        baseline = {
            "benchmark": {
                "case_id": "readonly_review",
                "base_revision": "a" * 40,
                "case_fingerprint": "1" * 64,
            }
        }
        candidate = {
            "benchmark": {
                "case_id": "readonly_review",
                "base_revision": "a" * 40,
                "case_fingerprint": "2" * 64,
            }
        }
        self.assertEqual(
            report._case_compatibility(baseline, candidate),
            {
                "comparable": False,
                "reason": "benchmark case definitions differ",
            },
        )

    def test_compare_case_compatibility_requires_same_case_and_base(self) -> None:
        self.insert_event("event")
        baseline = self.summarize()
        candidate = copy.deepcopy(baseline)
        baseline["benchmark"] = {
            "case_id": "focused_edit_validation",
            "base_revision": "a" * 40,
            "variant": "direct",
            "case_fingerprint": "f" * 64,
        }
        candidate["benchmark"] = {
            "case_id": "focused_edit_validation",
            "base_revision": "a" * 40,
            "variant": "code_mode",
            "case_fingerprint": "f" * 64,
        }
        candidate["job_convergence"]["pending_followed_immediately_by_observe_count"] = 2
        candidate["job_convergence"]["passive_terminal_delivery_count"] = 1
        comparison = report.compare_reports(baseline, candidate)
        self.assertEqual(
            comparison["case_compatibility"],
            {"comparable": True, "reason": None},
        )
        metrics = {item["metric"]: item for item in comparison["metrics"]}
        self.assertEqual(
            metrics["job_convergence.pending_followed_immediately_by_observe_count"]["delta"],
            2,
        )
        self.assertEqual(
            metrics["job_convergence.passive_terminal_delivery_count"]["delta"],
            1,
        )



    def test_job_convergence_loads_exact_unlinked_observe_from_audit_context(self):
        for trace, link in [(1, True), (2, False), (3, True)]:
            self.insert_event(str(trace), trace_id=str(trace), started=trace * 1000,
                              handed=trace * 1000 + 100, transition="serial" if trace > 1 else "unavailable", link=link)
        values = [JobConvergenceTests.row(1, kind="pending_handoff"),
                  JobConvergenceTests.row(2, 1, "explicit_observe", terminal=2000),
                  JobConvergenceTests.row(3, 2)]
        with sqlite3.connect(self.audit_db) as connection:
            for row in values:
                connection.execute("UPDATE action_events SET summary_json = ? WHERE event_id = ?",
                                   (json.dumps(row["summary"]), row["event_id"]))
        result = self.summarize()["job_convergence"]
        self.assertEqual(result["pending_followed_immediately_by_observe_count"], 1)
        self.assertEqual(result["pending_followup_known_count"], 1)
        self.assertEqual(result["pending_to_terminal_ms"]["observed_total"], 900)

    def test_job_convergence_loads_exact_unlinked_successor_after_selected_tail(self):
        for trace, link in [(1, True), (2, True), (3, False)]:
            self.insert_event(str(trace), trace_id=str(trace), started=trace * 1000,
                              handed=trace * 1000 + 100, transition="serial" if trace > 1 else "unavailable", link=link)
        pending = JobConvergenceTests.row(1, kind="pending_handoff")
        terminal = JobConvergenceTests.row(2, 1, "passive_terminal", failure=True, terminal=2000)
        terminal["summary"]["model_ergonomics"]["job_convergence"]["events"][0]["validation_failure"] = True
        observe = JobConvergenceTests.row(3, 2, "explicit_observe", terminal=2000)
        with sqlite3.connect(self.audit_db) as connection:
            for row in [pending, terminal, observe]:
                connection.execute("UPDATE action_events SET summary_json = ? WHERE event_id = ?",
                                   (json.dumps(row["summary"]), row["event_id"]))
        result = self.summarize()["job_convergence"]
        self.assertEqual(result["passive_terminal_before_explicit_observe_count"], 1)
        self.assertEqual(result["terminal_failure_followed_by_observe_count"], 1)
        self.assertEqual(result["terminal_validation_failure_followed_by_observe_count"], 1)


class JobConvergenceTests(unittest.TestCase):
    @staticmethod
    def row(trace, previous=None, kind=None, relation="a" * 64, failure=None, terminal=None):
        event = {"kind": kind, "relation": relation}
        if failure is not None:
            event["failure"] = failure
        if terminal is not None:
            event["terminal_observed_at_ms"] = terminal
        facts = {
            "pending_handoff_count": int(kind == "pending_handoff"),
            "passive_terminal_delivery_count": int(kind == "passive_terminal"),
            "passive_failure_delivery_count": int(failure is True),
            "wait_for_job_terminal_count": 0,
            "correlation_complete": True,
            "events": [event] if kind else [],
        }
        return {
            "event_id": str(trace), "server_trace_id": str(trace),
            "client_window_key": "window", "principal_correlation_kind": "user",
            "principal_correlation_id": "principal", "window_meaningful": True,
            "window_continuity_eligible": True, "response_streaming": False,
            "window_transition_kind": "serial" if previous else "unavailable",
            "request_observed_at_ms": trace * 1000,
            "response_handed_at_ms": trace * 1000 + 100,
            "summary": {"previous_meaningful_call": str(previous) if previous else None,
                        "model_ergonomics": {"schema_version": 10, "job_convergence": facts}},
        }

    def test_exact_immediate_observe_and_unrelated_relations(self):
        pending = self.row(1, kind="pending_handoff")
        observe = self.row(2, 1, "explicit_observe")
        result = report._summarize_job_convergence([pending, observe], [])
        self.assertEqual(result["pending_handoff_count"], 1)
        self.assertEqual(result["pending_followed_immediately_by_observe_count"], 1)
        unrelated = self.row(2, 1, "explicit_observe", relation="b" * 64)
        self.assertEqual(report._summarize_job_convergence([pending, unrelated], [])["pending_followed_immediately_by_observe_count"], 0)
        intervening_read = self.row(2, 1)
        later = self.row(3, 2, "explicit_observe")
        self.assertEqual(report._summarize_job_convergence([pending, intervening_read, later], [])["pending_followed_immediately_by_observe_count"], 0)

    def test_passive_failure_then_fix_or_observe_and_authoritative_timing(self):
        pending = self.row(1, kind="pending_handoff")
        terminal = self.row(2, 1, "passive_terminal", failure=True, terminal=2000)
        terminal["summary"]["model_ergonomics"]["job_convergence"]["events"][0]["validation_failure"] = True
        edit = self.row(3, 2)
        result = report._summarize_job_convergence([pending, terminal, edit], [])
        self.assertEqual(result["passive_terminal_delivery_count"], 1)
        self.assertEqual(result["passive_failure_delivery_count"], 1)
        self.assertEqual(result["passive_terminal_before_explicit_observe_count"], 1)
        self.assertEqual(result["terminal_failure_followed_by_observe_count"], 0)
        self.assertEqual(result["pending_to_terminal_ms"]["observed_total"], 900)
        observe = self.row(3, 2, "explicit_observe")
        second_observe = self.row(4, 3, "explicit_observe")
        result = report._summarize_job_convergence([pending, terminal, observe, second_observe], [])
        self.assertEqual(result["terminal_failure_followed_by_observe_count"], 1)
        self.assertEqual(result["terminal_validation_failure_followed_by_observe_count"], 1)
        self.assertEqual(result["passive_validation_failure_delivery_count"], 1)
        unrelated = self.row(3, 2, "explicit_observe", relation="b" * 64)
        self.assertEqual(report._summarize_job_convergence([pending, terminal, unrelated], [])["terminal_failure_followed_by_observe_count"], 0)

    def test_prior_observe_does_not_count_as_passive_before_observe(self):
        rows = [self.row(1, kind="pending_handoff"), self.row(2, 1, "explicit_observe"), self.row(3, 2, "passive_terminal", failure=False, terminal=3000)]
        result = report._summarize_job_convergence(rows, [])
        self.assertEqual(result["passive_terminal_before_explicit_observe_count"], 0)
        self.assertEqual(result["passive_failure_delivery_count"], 0)

    def test_gaps_overlap_wrong_principal_window_and_absent_timestamps_are_unknown(self):
        pending = self.row(1, kind="pending_handoff")
        for change in [
            {"window_transition_kind": "overlap"},
            {"client_window_key": "other-window"},
            {"principal_correlation_id": "other-principal"},
            {"request_observed_at_ms": None},
            {"response_streaming": True},
        ]:
            observe = self.row(2, 1, "explicit_observe")
            observe.update(change)
            self.assertEqual(report._summarize_job_convergence([pending, observe], [])["pending_followed_immediately_by_observe_count"], 0)
        # Even if the last exported row was pending, an absent exact predecessor
        # never becomes adjacency. This also covers a dropped audit write.
        observe = self.row(3, 2, "explicit_observe")
        self.assertEqual(report._summarize_job_convergence([pending, observe], [])["pending_followed_immediately_by_observe_count"], 0)
        for timestamp in [None, 1000]:
            terminal = self.row(2, 1, "passive_terminal", failure=False, terminal=timestamp)
            result = report._summarize_job_convergence([pending, terminal], [])
            self.assertEqual(result["pending_to_terminal_ms"]["samples"], 0)
            self.assertEqual(result["pending_to_terminal_ms"]["missing"], 1)

    def test_failed_observe_correlation_does_not_prove_absence_of_observation(self):
        pending = self.row(1, kind="pending_handoff")
        failed_observe = self.row(2, 1)
        failed_observe["summary"]["model_ergonomics"]["job_convergence"]["correlation_complete"] = False
        terminal = self.row(3, 2, "passive_terminal", terminal=3000)
        result = report._summarize_job_convergence([pending, failed_observe, terminal], [])
        self.assertEqual(result["passive_terminal_before_explicit_observe_count"], 0)
        self.assertEqual(result["pending_followup_known_count"], 0)
        self.assertEqual(result["unknown_relations"], 1)

    def test_wait_count_and_bounded_identity_free_aggregate(self):
        row = self.row(1)
        row["summary"]["model_ergonomics"]["job_convergence"]["wait_for_job_terminal_count"] = 1
        row["summary"]["source"] = "SECRET_SOURCE_COMMAND_STDOUT_STDERR"
        result = report._summarize_job_convergence([row], [])
        self.assertEqual(result["wait_for_job_terminal_count"], 1)
        encoded = json.dumps(result)
        for private in ["SECRET", "principal", "window", "a" * 64]:
            self.assertNotIn(private, encoded)
        with self.assertRaisesRegex(report.ReportError, "100000"):
            report._summarize_job_convergence([row] * 100001, [])


if __name__ == "__main__":
    unittest.main()
