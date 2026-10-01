#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "mcp_conformance_report.py"
SPEC = importlib.util.spec_from_file_location("mcp_conformance_report", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
report_gate = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = report_gate
SPEC.loader.exec_module(report_gate)


class ReportGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.raw = self.root / "raw"
        self.raw.mkdir()
        self.metadata = self.root / "metadata.json"
        self.baseline = self.root / "baseline.json"
        self.profile = "2026-07-28"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def write_metadata(
        self,
        scenarios: list[str],
        not_scored: list[dict[str, str]] | None = None,
        harness_exit_code: int = 0,
        server_capabilities: dict[str, object] | None = None,
    ) -> None:
        self.metadata.write_text(
            json.dumps(
                {
                    "profile": self.profile,
                    "schema_version": 1,
                    "server_sha": "a" * 40,
                    "harness_sha": "b" * 40,
                    "harness_exit_code": harness_exit_code,
                    "server_capabilities": server_capabilities
                    if server_capabilities is not None
                    else {"tools": {"listChanged": False}},
                    "required_scenarios": scenarios,
                    "not_scored_scenarios": not_scored or [],
                }
            ),
            encoding="utf-8",
        )

    def write_baseline(
        self,
        entries: list[dict[str, str]],
        expected_server_capabilities: dict[str, object] | None = None,
    ) -> None:
        self.baseline.write_text(
            json.dumps(
                {
                    "schema_version": 2,
                    "harness_commit": "b" * 40,
                    "profiles": {
                        self.profile: {
                            "expected_server_capabilities": expected_server_capabilities
                            if expected_server_capabilities is not None
                            else {"tools": {"listChanged": False}},
                            "classifications": entries,
                        }
                    },
                }
            ),
            encoding="utf-8",
        )

    def write_checks(self, scenario: str, checks: list[dict[str, object]]) -> None:
        result = self.raw / f"server-{scenario}-2026-09-14T00-00-00-000Z"
        result.mkdir()
        (result / "checks.json").write_text(json.dumps(checks), encoding="utf-8")

    @staticmethod
    def check(
        check_id: str,
        status: str,
        *,
        error_message: str | None = None,
        description: str | None = None,
    ) -> dict[str, object]:
        result: dict[str, object] = {
            "id": check_id,
            "name": check_id,
            "description": description or check_id,
            "status": status,
            "timestamp": "2026-09-14T00:00:00Z",
        }
        if error_message is not None:
            result["errorMessage"] = error_message
        return result

    @staticmethod
    def classification(
        check: dict[str, object],
        kind: str = "genuine_protocol_failure",
        *,
        scenario: str = "scenario-a",
    ) -> dict[str, str]:
        return {
            "scenario": scenario,
            "check_id": str(check["id"]),
            "expected_status": str(check["status"]),
            "evidence_sha256": report_gate.check_evidence_sha256(check),
            "classification": kind,
            "reason": "reviewed baseline reason",
            "evidence": "MCP 2026-07-28 section/example",
        }

    def evaluate(self):
        return report_gate.evaluate_reports(self.raw, self.metadata, self.baseline, self.profile)

    def test_successful_nonempty_coverage_passes(self) -> None:
        self.write_metadata(["scenario-a"])
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("wire-shape", "SUCCESS")])
        summary = self.evaluate()
        self.assertTrue(summary["gate_passed"])
        self.assertEqual(summary["applicable_check_count"], 1)

    def test_exact_classified_failure_passes(self) -> None:
        failed = self.check("known-gap", "FAILURE", error_message="stable protocol mismatch")
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(failed)])
        self.write_checks("scenario-a", [failed])
        summary = self.evaluate()
        self.assertTrue(summary["gate_passed"])

    def test_all_skipped_is_not_mistaken_for_coverage(self) -> None:
        skipped = self.check("not-applicable", "SKIPPED")
        self.write_metadata(["scenario-a"])
        self.write_baseline(
            [self.classification(skipped, "inconclusive_infrastructure")]
        )
        self.write_checks("scenario-a", [skipped])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("zero applicable" in problem for problem in summary["problems"]))

    def test_missing_required_scenario_fails(self) -> None:
        self.write_metadata(["scenario-a", "scenario-b"])
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertEqual(summary["missing_scenarios"], ["scenario-b"])

    def test_unexpected_scenario_fails(self) -> None:
        self.write_metadata(["scenario-a"])
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks("scenario-extra", [self.check("extra", "SUCCESS")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertEqual(summary["unexpected_scenarios"], ["scenario-extra"])

    def test_duplicate_scenario_reports_are_rejected(self) -> None:
        self.write_metadata(["scenario-a"])
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        duplicate = self.raw / "server-scenario-a-2026-09-14T00-00-01-000Z"
        duplicate.mkdir()
        (duplicate / "checks.json").write_text(
            json.dumps([self.check("ok-again", "SUCCESS")]), encoding="utf-8"
        )
        with self.assertRaisesRegex(report_gate.GateError, "multiple raw reports"):
            self.evaluate()

    def test_required_and_not_scored_overlap_is_rejected(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "scenario-a", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        with self.assertRaisesRegex(report_gate.GateError, "both required and not_scored"):
            self.evaluate()

    def test_required_empty_report_fails_even_with_other_coverage(self) -> None:
        self.write_metadata(["scenario-a", "scenario-b"])
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks("scenario-b", [])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("scenario-b" in p and "no verdict" in p for p in summary["problems"]))

    def test_required_info_only_report_fails_even_with_other_coverage(self) -> None:
        self.write_metadata(["scenario-a", "scenario-b"])
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks("scenario-b", [self.check("info", "INFO")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("scenario-b" in p and "no verdict" in p for p in summary["problems"]))

    def test_unknown_status_is_rejected(self) -> None:
        self.write_metadata(["scenario-a"])
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("broken", "ERROR")])
        with self.assertRaisesRegex(report_gate.GateError, "unknown status"):
            self.evaluate()

    def test_duplicate_success_check_id_is_allowed(self) -> None:
        self.write_metadata(["scenario-a"])
        self.write_baseline([])
        self.write_checks(
            "scenario-a",
            [self.check("duplicate", "SUCCESS"), self.check("duplicate", "SUCCESS")],
        )
        summary = self.evaluate()
        self.assertTrue(summary["gate_passed"])

    def test_duplicate_non_success_check_id_fails_closed(self) -> None:
        first = self.check("duplicate", "FAILURE", error_message="first failure")
        second = self.check("duplicate", "FAILURE", error_message="second failure")
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(first)])
        self.write_checks("scenario-a", [first, second])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("duplicate non-success check IDs" in p for p in summary["problems"]))

    def test_metadata_harness_sha_must_match_baseline_pin(self) -> None:
        self.write_metadata(["scenario-a"])
        self.write_baseline([])
        metadata = json.loads(self.metadata.read_text(encoding="utf-8"))
        metadata["harness_sha"] = "c" * 40
        self.metadata.write_text(json.dumps(metadata), encoding="utf-8")
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        with self.assertRaisesRegex(report_gate.GateError, "does not match pinned baseline"):
            self.evaluate()

    def test_not_scored_failure_is_visible_but_does_not_require_classification(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "extension-a", "reason": "extension"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks("extension-a", [self.check("pending", "FAILURE")])
        summary = self.evaluate()
        self.assertTrue(summary["gate_passed"])
        self.assertEqual(summary["informational_status_counts"], {"FAILURE": 1})
        self.assertEqual(summary["informational_non_success"][0]["reason"], "extension")

    def test_not_scored_timeout_is_infrastructure_failure(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "extension-a", "reason": "extension"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks(
            "extension-a",
            [
                self.check(
                    "scenario-timeout",
                    "FAILURE",
                    error_message="Scenario 'extension-a' did not complete within 10000ms.",
                )
            ],
        )
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("infrastructure failures" in p for p in summary["problems"]))

    def test_not_scored_duplicate_check_ids_remain_informational(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "extension-a", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks(
            "extension-a",
            [self.check("repeated", "FAILURE"), self.check("repeated", "FAILURE")],
        )
        summary = self.evaluate()
        self.assertTrue(summary["gate_passed"])
        self.assertEqual(summary["informational_status_counts"], {"FAILURE": 2})

    def test_missing_not_scored_scenario_fails_coverage_integrity(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "pending-a", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertEqual(summary["missing_not_scored_scenarios"], ["pending-a"])

    def test_empty_not_scored_report_fails_coverage_integrity(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "pending-a", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks("pending-a", [])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("not-scored scenario" in p and "empty" in p for p in summary["problems"]))

    def test_classified_harness_error_remains_inconclusive(self) -> None:
        failed = self.check("scenario-a", "FAILURE", error_message="unreviewable harness result")
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline(
            [self.classification(failed, "inconclusive_infrastructure")]
        )
        self.write_checks("scenario-a", [failed])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("inconclusive infrastructure" in p for p in summary["problems"]))

    def test_abnormal_harness_exit_is_infrastructure_failure(self) -> None:
        self.write_metadata(["scenario-a"], harness_exit_code=137)
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("exited abnormally" in p for p in summary["problems"]))

    def test_exit_one_without_scored_failure_is_rejected(self) -> None:
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("exit code 1 had no scored FAILURE" in p for p in summary["problems"]))

    def test_exit_zero_with_scored_failure_is_rejected(self) -> None:
        failed = self.check("unexpected-failure", "FAILURE", error_message="protocol mismatch")
        self.write_metadata(["scenario-a"], harness_exit_code=0)
        self.write_baseline([self.classification(failed)])
        self.write_checks("scenario-a", [failed])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("exit code 0" in p and "scored FAILURE" in p for p in summary["problems"]))

    def test_new_failure_requires_exact_classification(self) -> None:
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("new-regression", "FAILURE")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("new-regression" in p for p in summary["problems"]))

    def test_changed_failure_evidence_requires_review(self) -> None:
        expected = self.check(
            "tools-call-simple-text",
            "FAILURE",
            error_message="Unknown tool: test_simple_text",
        )
        actual = self.check(
            "tools-call-simple-text",
            "FAILURE",
            error_message="Tool returned a different protocol error",
        )
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected, "missing_harness_fixture")])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_changed_failure_details_with_same_message_requires_review(self) -> None:
        expected = self.check(
            "stable-message",
            "FAILURE",
            error_message="protocol assertion failed",
        )
        expected["details"] = {"actual": 200, "expected": 400}
        actual = self.check(
            "stable-message",
            "FAILURE",
            error_message="protocol assertion failed",
        )
        actual["details"] = {"actual": 500, "expected": 400}
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_timestamp_jsonrpc_id_does_not_change_evidence(self) -> None:
        expected = self.check(
            "sep-2575-http-server-no-independent-requests-on-stream",
            "FAILURE",
            error_message="diagnostic tool is unavailable",
        )
        expected["details"] = {
            "untestable": True,
            "response": {
                "jsonrpc": "2.0",
                "id": 1_789_000_000_001,
                "error": {"code": -32602, "message": "tool unavailable"},
            },
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["response"]["id"] = 1_789_000_000_999
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected, "missing_harness_fixture")])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertTrue(summary["gate_passed"])

    def test_missing_timestamp_jsonrpc_id_requires_review(self) -> None:
        expected = self.check(
            "sep-2575-http-server-no-independent-requests-on-stream",
            "FAILURE",
            error_message="diagnostic tool is unavailable",
        )
        expected["details"] = {
            "response": {
                "jsonrpc": "2.0",
                "id": 1_789_000_000_001,
                "error": {"code": -32602, "message": "tool unavailable"},
            }
        }
        actual = json.loads(json.dumps(expected))
        del actual["details"]["response"]["id"]
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected, "missing_harness_fixture")])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_timestamp_jsonrpc_id_type_change_requires_review(self) -> None:
        expected = self.check(
            "sep-2575-http-server-no-independent-requests-on-stream",
            "FAILURE",
            error_message="diagnostic tool is unavailable",
        )
        expected["details"] = {
            "response": {"jsonrpc": "2.0", "id": 1_789_000_000_001, "error": {"code": -32602}}
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["response"]["id"] = "<dynamic-millisecond-jsonrpc-id>"
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected, "missing_harness_fixture")])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_fixed_jsonrpc_id_change_still_requires_review(self) -> None:
        expected = self.check(
            "sep-2575-http-server-no-independent-requests-on-stream",
            "FAILURE",
            error_message="protocol assertion failed",
        )
        expected["details"] = {
            "response": {"jsonrpc": "2.0", "id": 1, "error": {"code": -32602}}
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["response"]["id"] = 2
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_timestamp_id_on_unrelated_check_is_not_normalized(self) -> None:
        expected = self.check("other-check", "FAILURE", error_message="protocol assertion failed")
        expected["details"] = {
            "response": {"jsonrpc": "2.0", "id": 1_789_000_000_001, "error": {"code": -32602}}
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["response"]["id"] = 1_789_000_000_999
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_wire_schema_full_message_drift_does_not_change_evidence(self) -> None:
        expected = self.check(
            "wire-schema-valid",
            "FAILURE",
            error_message='Schema validation failed; offending message: {"tools":[{"name":"old-tool"}]}',
        )
        expected["details"] = {
            "messagesValidated": 5,
            "violations": [
                {
                    "origin": "implementation",
                    "specVersion": "2025-11-25",
                    "context": "stateful response to 'tools/list'",
                    "errors": ["outputSchema: must have required property 'type'"],
                    "message": {
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": {"tools": [{"name": "old-tool"}]},
                    },
                }
            ],
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["violations"][0]["message"]["result"] = {
            "tools": [{"name": "new-unrelated-tool", "description": "changed"}]
        }
        actual["errorMessage"] = (
            'Schema validation failed; offending message: '
            '{"tools":[{"name":"new-unrelated-tool","description":"changed"}]}'
        )
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertTrue(summary["gate_passed"])

    def test_wire_schema_tool_index_shift_uses_tool_identity(self) -> None:
        expected = self.check("wire-schema-valid", "FAILURE", error_message="Schema validation failed")
        expected["details"] = {
            "messagesValidated": 5,
            "violations": [{
                "origin": "implementation",
                "specVersion": "2025-11-25",
                "context": "stateful response to 'tools/list'",
                "errors": ["ListToolsResult/tools/1/outputSchema: must have required property 'type' (result of 'tools/list')"],
                "message": {"jsonrpc": "2.0", "id": 1, "result": {"tools": [{"name": "other"}, {"name": "manage_ssh_resource"}]}},
            }],
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["violations"][0]["errors"] = [
            "ListToolsResult/tools/2/outputSchema: must have required property 'type' (result of 'tools/list')"
        ]
        actual["details"]["violations"][0]["message"]["result"]["tools"] = [
            {"name": "inserted"}, {"name": "other"}, {"name": "manage_ssh_resource"}
        ]
        self.assertEqual(
            report_gate.check_evidence_sha256(expected),
            report_gate.check_evidence_sha256(actual),
        )

    def test_wire_schema_tool_identity_change_requires_review(self) -> None:
        expected = self.check("wire-schema-valid", "FAILURE", error_message="Schema validation failed")
        expected["details"] = {
            "messagesValidated": 5,
            "violations": [{
                "origin": "implementation",
                "specVersion": "2025-11-25",
                "context": "stateful response to 'tools/list'",
                "errors": ["ListToolsResult/tools/0/outputSchema: must have required property 'type' (result of 'tools/list')"],
                "message": {"jsonrpc": "2.0", "id": 1, "result": {"tools": [{"name": "manage_ssh_resource"}]}},
            }],
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["violations"][0]["message"]["result"]["tools"][0]["name"] = "different_tool"
        self.assertNotEqual(
            report_gate.check_evidence_sha256(expected),
            report_gate.check_evidence_sha256(actual),
        )

    def test_wire_schema_error_change_still_requires_review(self) -> None:
        expected = self.check(
            "wire-schema-valid",
            "FAILURE",
            error_message="Schema validation failed",
        )
        expected["details"] = {
            "messagesValidated": 5,
            "violations": [
                {
                    "origin": "implementation",
                    "specVersion": "2025-11-25",
                    "context": "stateful response to 'tools/list'",
                    "errors": ["outputSchema: must have required property 'type'"],
                    "message": {"jsonrpc": "2.0", "id": 1, "result": {}},
                }
            ],
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["violations"][0]["errors"] = [
            "inputSchema: must have required property 'type'"
        ]
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_business_message_change_is_not_wire_schema_noise(self) -> None:
        expected = self.check(
            "business-error",
            "FAILURE",
            error_message="tool failed",
        )
        expected["details"] = {
            "origin": "application",
            "context": "tool-call",
            "errors": [],
            "specVersion": "business-v1",
            "message": {"code": "FIXTURE_MISSING"},
        }
        actual = json.loads(json.dumps(expected))
        actual["details"]["message"]["code"] = "AUTH_DENIED"
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected, "missing_harness_fixture")])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_empty_structured_evidence_falls_back_to_description(self) -> None:
        expected = self.check("empty-details", "FAILURE", description="first diagnostic")
        expected["details"] = {}
        actual = self.check("empty-details", "FAILURE", description="changed diagnostic")
        actual["details"] = {}
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("evidence changed" in p for p in summary["problems"]))

    def test_not_scored_wire_schema_harness_error_is_infrastructure_failure(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "extension-a", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks(
            "extension-a",
            [self.check("wire-schema-harness-error", "FAILURE", error_message="harness emitted invalid JSON-RPC")],
        )
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("infrastructure failures" in p for p in summary["problems"]))

    def test_not_scored_scenario_catch_error_is_infrastructure_failure(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "json-schema-2020-12", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks(
            "json-schema-2020-12",
            [self.check("json-schema-2020-12-error", "FAILURE", error_message="Failed: fetch failed")],
        )
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("infrastructure failures" in p for p in summary["problems"]))

    def test_not_scored_top_level_scenario_error_is_infrastructure_failure(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "server-sse-polling", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks(
            "server-sse-polling",
            [
                self.check(
                    "server-sse-polling-error",
                    "FAILURE",
                    error_message="Error: unexpected scenario exception",
                )
            ],
        )
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("top-level scenario failure" in p for p in summary["problems"]))

    def test_not_scored_transport_exception_is_infrastructure_failure(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "tasks-mrtr-composition", "reason": "extension"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks(
            "tasks-mrtr-composition",
            [
                self.check(
                    "sep-2663-mrtr-synchronous-before-task-creation",
                    "FAILURE",
                    error_message="fetch failed",
                )
            ],
        )
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("transport exception" in p for p in summary["problems"]))

    def test_not_scored_setup_failure_is_infrastructure_failure(self) -> None:
        self.write_metadata(
            ["scenario-a"],
            [{"scenario": "http-header-validation", "reason": "pending"}],
        )
        self.write_baseline([])
        self.write_checks("scenario-a", [self.check("ok", "SUCCESS")])
        self.write_checks(
            "http-header-validation",
            [self.check("sep-2243-server-standard-setup", "FAILURE", error_message="Failed to set up tests: fetch failed")],
        )
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("infrastructure failures" in p for p in summary["problems"]))

    def test_connection_failure_cannot_hide_behind_fixture_classification(self) -> None:
        expected = self.check(
            "tools-call-simple-text",
            "FAILURE",
            error_message="Unknown tool: test_simple_text",
        )
        actual = self.check(
            "tools-call-simple-text",
            "FAILURE",
            error_message="connect ECONNREFUSED 127.0.0.1:12345",
            description="Failed to run scenario",
        )
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected, "missing_harness_fixture")])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("infrastructure failures" in p for p in summary["problems"]))

    def test_status_change_requires_review(self) -> None:
        expected = self.check("subscription-ack", "SKIPPED")
        actual = self.check(
            "subscription-ack",
            "FAILURE",
            error_message="subscription protocol assertion failed",
        )
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline(
            [self.classification(expected, "optional_capability_not_implemented")]
        )
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("expected status SKIPPED" in p for p in summary["problems"]))

    def test_warning_to_failure_requires_review(self) -> None:
        expected = self.check("advisory", "WARNING", error_message="SHOULD mismatch")
        actual = self.check("advisory", "FAILURE", error_message="SHOULD mismatch")
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [actual])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("expected status WARNING" in p for p in summary["problems"]))

    def test_optional_classification_requires_same_capability_advertisement(self) -> None:
        failed = self.check(
            "prompts-list",
            "FAILURE",
            error_message="Failed: method not found",
        )
        self.write_metadata(
            ["scenario-a"],
            harness_exit_code=1,
            server_capabilities={"tools": {"listChanged": False}, "prompts": {}},
        )
        self.write_baseline(
            [self.classification(failed, "optional_capability_not_implemented")],
            expected_server_capabilities={"tools": {"listChanged": False}},
        )
        self.write_checks("scenario-a", [failed])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("capability advertisement changed" in p for p in summary["problems"]))

    def test_passing_expected_failure_is_stale(self) -> None:
        expected = self.check("fixed-check", "FAILURE", error_message="known bug")
        self.write_metadata(["scenario-a"])
        self.write_baseline([self.classification(expected)])
        self.write_checks("scenario-a", [self.check("fixed-check", "SUCCESS")])
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("now passing" in p for p in summary["problems"]))

    def test_info_only_expected_failure_is_uncovered(self) -> None:
        expected = self.check("no-verdict", "FAILURE", error_message="known bug")
        self.write_metadata(["scenario-a"])
        self.write_baseline([self.classification(expected)])
        self.write_checks(
            "scenario-a",
            [self.check("no-verdict", "INFO"), self.check("ok", "SUCCESS")],
        )
        summary = self.evaluate()
        self.assertFalse(summary["gate_passed"])
        self.assertTrue(any("no verdict emitted" in p for p in summary["problems"]))

    def test_placeholder_evidence_hash_is_rejected(self) -> None:
        self.write_metadata(["scenario-a"], harness_exit_code=1)
        self.write_baseline(
            [
                {
                    "scenario": "scenario-a",
                    "check_id": "failure",
                    "expected_status": "FAILURE",
                    "evidence_sha256": "0" * 64,
                    "classification": "genuine_protocol_failure",
                    "reason": "not reviewed",
                    "evidence": "placeholder",
                }
            ]
        )
        self.write_checks("scenario-a", [self.check("failure", "FAILURE")])
        with self.assertRaisesRegex(report_gate.GateError, "placeholder evidence_sha256"):
            self.evaluate()

    def test_broad_expected_failure_masks_are_rejected(self) -> None:
        self.write_metadata(["scenario-a"])
        self.write_baseline(
            [
                {
                    "scenario": "scenario-a",
                    "check_id": "*",
                    "expected_status": "FAILURE",
                    "evidence_sha256": "0" * 64,
                    "classification": "genuine_protocol_failure",
                    "reason": "too broad",
                    "evidence": "none",
                }
            ]
        )
        self.write_checks("scenario-a", [self.check("failure", "FAILURE")])
        with self.assertRaisesRegex(report_gate.GateError, "broad scenario masks"):
            self.evaluate()


if __name__ == "__main__":
    unittest.main()
