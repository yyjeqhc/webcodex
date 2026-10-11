import copy
import json
import subprocess
import sys
from pathlib import Path
import sqlite3
import tempfile
import unittest

from scripts import host_timing_dogfood as report


def policy(name="baseline", wait=5, budget=55, cont=5, revision="a" * 40):
    return {
        "schema_version": 1, "cohort": name, "case_id": "long_validation",
        "base_revision": revision, "host_profile": "host_code_mode",
        "host_budget_secs": budget, "sync_wait_secs": wait,
        "continuation_wait_secs": cont,
    }


class HostTimingReportTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.db = Path(self.tmp.name) / "audit.db"
        conn = sqlite3.connect(self.db)
        conn.execute("""CREATE TABLE action_events (
          event_id TEXT PRIMARY KEY, action_name TEXT, operation TEXT,
          duration_ms INTEGER, summary_json TEXT, window_meaningful INTEGER,
          started_at INTEGER, project TEXT)""")
        self.rows = [
            ("1", "run_process", 1100, "pending", True, {}),
            ("2", "run_process", 250, "completed", True, {}),
            ("3", "observe_jobs", 400, "completed", True, {}),
            ("4", "wait_for_job_readiness", 1500, None, True, {"waited_ms": 1250}),
            ("5", "run_process", 3900, "outcome_unknown", False, {}),
        ]
        for id, tool, ms, state, success, extra in self.rows:
            record = {"schema_version": 13, "tool_name": tool, "success": success,
                      "execution_state": state, "duration_ms": ms, **(
                          {"readiness": extra} if tool == "wait_for_job_readiness" else {}
                      )}
            conn.execute("INSERT INTO action_events VALUES (?,?,?,?,?,?,?,?)",
                         (id, "toolsCall", tool, ms, json.dumps({"model_ergonomics": record}),
                          1, 100, "agent:special:webcodex"))
        # A malformed record and an unrelated project must not contaminate metrics.
        conn.execute("INSERT INTO action_events VALUES (?,?,?,?,?,?,?,?)",
                     ("6", "toolsCall", "run_process", 300, json.dumps({"private": "secret"}), 1,
                      100, "agent:special:webcodex"))
        conn.execute("INSERT INTO action_events VALUES (?,?,?,?,?,?,?,?)",
                     ("7", "toolsCall", "run_process", 999, json.dumps({"model_ergonomics": {
                         "schema_version": 13, "tool_name": "run_process", "success": True,
                         "execution_state": "completed",
                     }}), 1, 100, "other"))
        conn.commit()
        conn.close()

    def summarize(self):
        return report.summarize(self.db, policy(), "agent:special:webcodex", 90, 120)

    def test_aggregate_only_and_missing_evidence_stays_null(self):
        result = self.summarize()
        counts = result["counts"]
        self.assertEqual(result["selection"]["events_selected"], 6)
        self.assertEqual(counts["outer_tool_calls"], 6)
        self.assertEqual(counts["canonical_calls"], 5)
        self.assertEqual(counts["structured_calls"], 3)
        self.assertEqual(counts["handoff_state_calls"], 1)
        self.assertEqual(counts["in_call_completed_state_calls"], 1)
        self.assertEqual(counts["structured_state_unclassified"], 1)
        self.assertEqual(counts["observation_calls"], 1)
        self.assertEqual(counts["readiness_calls"], 1)
        self.assertEqual(counts["outcome_unknown_calls"], 1)
        self.assertEqual(counts["canonical_record_unavailable"], 1)
        self.assertEqual(result["timing_ms"]["structured_request_p90"], 3900)
        self.assertEqual(result["timing_ms"]["readiness_wait_p50"], 1250)
        self.assertIsNone(result["model_turns"])
        self.assertIsNone(result["final_job_outcomes"])
        self.assertIsNone(result["task_wall_time_ms"])
        dump = json.dumps(result)
        self.assertNotIn("secret", dump)
        self.assertNotIn("agent:special:webcodex", dump)
        self.assertNotIn('"event_id"', dump)

    def test_strict_policy_ranges_and_no_implicit_baseline(self):
        with tempfile.TemporaryDirectory() as root:
            p = Path(root) / "cohort.json"
            p.write_text(json.dumps(policy()), encoding="utf-8")
            self.assertEqual(report._load_policy(p), policy())
            for invalid in (policy(wait=0), policy(wait=61), policy(cont=101),
                            policy(budget=7, wait=5), policy(revision="abc"),
                            {**policy(), "token": "should-not-leak"}):
                p.write_text(json.dumps(invalid), encoding="utf-8")
                with self.assertRaises(report.ReportError):
                    report._load_policy(p)

    def test_comparison_is_configuration_only_not_causal(self):
        left = self.summarize()
        right = self.summarize()
        right["cohort"] = policy("candidate", wait=8)
        compared = report.compare(left, right)
        self.assertTrue(compared["comparable_configuration"])
        self.assertEqual(compared["descriptive_deltas"]["handoff_state_calls"], 0)
        self.assertFalse(compared["causal_claim_supported"])
        self.assertFalse(compared["final_job_outcomes_comparable"])
        right["cohort"] = policy("candidate", wait=8, cont=8)
        compared = report.compare(left, right)
        self.assertFalse(compared["comparable_configuration"])
        self.assertIsNone(compared["descriptive_deltas"]["handoff_state_calls"])

    def test_compare_rejects_untrusted_and_fabricated_summaries(self):
        baseline = self.summarize()
        candidate = copy.deepcopy(baseline)
        candidate["cohort"] = policy("candidate", wait=8)
        corruptions = [
            [],
            None,
            "private-secret",
            {},
            {**baseline, "schema_version": True},
            {**baseline, "cohort": None},
            {**baseline, "cohort": []},
            {**baseline, "cohort": {**policy(), "sync_wait_secs": -1}},
            {**baseline, "cohort": {**policy(), "host_budget_secs": True}},
            {**baseline, "counts": None},
            {**baseline, "counts": {**baseline["counts"], "structured_calls": -1}},
            {**baseline, "counts": {**baseline["counts"], "handoff_state_calls": True}},
            {**baseline, "counts": {**baseline["counts"], "structured_calls": 100001}},
            {**baseline, "counts": {**baseline["counts"], "handoff_state_calls": 4}},
            {**baseline, "counts": {**baseline["counts"], "observation_calls": "1"}},
            {**baseline, "counts": {key: value for key, value in baseline["counts"].items()
                                     if key != "readiness_calls"}},
            {**baseline, "availability": {**baseline["availability"],
                                          "structured_request_duration_samples": -1}},
            {**baseline, "timing_ms": {**baseline["timing_ms"],
                                      "structured_request_p90": -20}},
            {**baseline, "timing_ms": {}},
            {**baseline, "timing_ms": {key: value for key, value in baseline["timing_ms"].items()
                                        if key != "readiness_wait_p90"}},
            {**baseline, "selection": {**baseline["selection"], "project_filter_applied": False}},
            {**baseline, "model_turns": 2},
            {**baseline, "final_job_outcomes": {"success": True}},
        ]
        for corrupt in corruptions:
            with self.subTest(corrupt=repr(corrupt)[:70]):
                with self.assertRaises(report.ReportError):
                    report.compare(corrupt, candidate)
                with self.assertRaises(report.ReportError):
                    report.compare(candidate, corrupt)

    def test_cli_malformed_json_is_rejected_with_static_error(self):
        with tempfile.TemporaryDirectory() as root:
            base = Path(root) / "base.json"
            candidate = Path(root) / "candidate.json"
            for invalid in (["private-secret"], {"cohort": None}, {"counts": {"structured_calls": -1}}):
                base.write_text(json.dumps(invalid), encoding="utf-8")
                candidate.write_text(json.dumps(self.summarize()), encoding="utf-8")
                process = subprocess.run([
                    sys.executable, str(Path(report.__file__).resolve()),
                    "compare", "--baseline", str(base), "--candidate", str(candidate)
                ], text=True, capture_output=True, check=False)
                self.assertEqual(process.returncode, 2)
                self.assertEqual(process.stdout, "")
                self.assertIn("host_timing_dogfood:", process.stderr)
                self.assertNotIn("Traceback", process.stderr)
                self.assertNotIn("private-secret", process.stderr)
                self.assertNotIn(str(base), process.stderr)

    def test_legacy_duration_is_not_labeled_host_latency(self):
        result = self.summarize()
        self.assertEqual(
            result["duration_basis"],
            "legacy_action_audit_pre_response_handoff_not_host_latency"
        )
        self.assertIn("before response handoff", result["interpretation"])
        self.assertIsNone(result["task_wall_time_ms"])

    def test_empty_range_marks_unmeasured_metrics_unavailable(self):
        result = report.summarize(self.db, policy(), "agent:special:webcodex", 110, 120)
        self.assertEqual(result["counts"]["structured_calls"], 0)
        self.assertIsNone(result["timing_ms"]["structured_request_p90"])
        self.assertIsNone(result["final_job_outcomes"])
        self.assertEqual(result["availability"]["structured_request_duration_samples"], 0)

    def test_read_only_db_and_time_boundaries(self):
        before = self.db.read_bytes()
        self.summarize()
        self.assertEqual(before, self.db.read_bytes())
        with self.assertRaises(report.ReportError):
            report.summarize(self.db, policy(), "", 90, 120)
        with self.assertRaises(report.ReportError):
            report.summarize(self.db, policy(), "agent:special:webcodex", 120, 90)
        with self.assertRaises(report.ReportError):
            report._timestamp("2026-10-10T00:00:00")
        with self.assertRaises(report.ReportError):
            report._timestamp("2026-10-10T00:00:00+08:00")


if __name__ == "__main__":
    unittest.main()
