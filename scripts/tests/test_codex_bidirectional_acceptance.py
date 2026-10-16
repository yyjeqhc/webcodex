"""Deterministic harness checks; no services, credentials, or model calls."""
import argparse
import copy
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import e2e_codex_bidirectional_ws as harness


def observation(state="running", execution="started", **extra):
    return {
        "run_id": "run-original", "state": state, "execution_state": execution,
        "observation_token": "cursor-" + state + "-" + execution,
        "history_lost": False, "has_more": False,
        "events": [{"kind": "agent_message"}, {"kind": "tool_activity"}],
        "terminal": {"stop_reason": "end_turn"} if state == "completed" else None,
        **extra,
    }


class CodexRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        # Bypass constructor provisioning: these tests exercise decisions and
        # report transitions with controlled transport/process observations.
        self.smoke = harness.BidirectionalSmoke.__new__(harness.BidirectionalSmoke)
        self.smoke.args = argparse.Namespace(timeout=30)
        self.smoke.env = {}
        self.smoke.root = root
        self.smoke.private = root / "private"
        self.smoke.private.mkdir()
        self.smoke.fixture = root / "project"
        self.smoke.fixture.mkdir()
        for name in ("clamp.py", "test_clamp.py", "test_extra.py"):
            (self.smoke.fixture / name).write_text("# fixture\n")
        self.smoke.fixed_hash = harness.sha256(self.smoke.fixture / "test_clamp.py")
        self.smoke.audit = root / "audit.jsonl"
        self.smoke.project = "agent:fixture:project"
        self.smoke.operator = "fixture-only"
        self.smoke.report = {"managed": {"passed": False}, "recovery": {"passed": False}}
        self.smoke.checkpoint = Mock()
        self.smoke.server = Mock(pid=101)
        self.smoke.runner = Mock(pid=202)
        self.smoke.runner.poll.return_value = None
        self.smoke.start_server = Mock(side_effect=self.restart)
        self.smoke.wait_ready = Mock()
        self.smoke.initialize = Mock()
        self.smoke.native_tests = Mock(side_effect=[
            {"exit_code": 1, "tests_run": 7},
            {"exit_code": 0, "tests_run": 7},
            {"exit_code": 0, "tests_run": 8},
        ])
        self.binding = {"task_id": "task-original", "attempt_id": "attempt-original",
                        "run_id": "run-original", "task_state": "succeeded"}
        self.trace = []
        self.observations = []
        self.smoke.tool = Mock(side_effect=self.tool)

    def restart(self):
        self.trace.append("restart")
        self.smoke.server = Mock(pid=303)

    def tool(self, name, args, token):
        self.trace.append((name, copy.deepcopy(args)))
        if name == "create_agent_identity":
            return {"agent": {"agent_id": "agent-original"}}
        if name == "create_agent_task":
            return {"task": {"summary": {"task_id": "task-original"}}}
        if name == "start_agent_task_attempt":
            return {"attempt": {"attempt_id": "attempt-original", "attempt_controller_generation": 1},
                    "attempt_ref": "attempt-ref-original"}
        if name in ("start_agent_task_coding_run", "reconcile_agent_task_coding_run"):
            return dict(self.binding)
        if name == "observe_coding_agent":
            self.assertEqual(args["run_id"], "run-original")
            self.assertTrue(self.observations, "unexpected additional observation")
            return self.observations.pop(0)
        if name == "read_agent_task":
            return {"task": {"summary": {"task_id": "task-original"}}}
        self.fail("unexpected tool: " + name)

    def run_managed(self, observations):
        self.observations = list(observations)
        audit = [
            {"event": "adapter_process_started"},
            {"event": "prompt_dispatch", "effective_config": {
                "model": harness.MODEL, "reasoning_effort": harness.EFFORT}},
            {"event": "adapter_identity", "name": "fixture", "version": harness.ADAPTER_VERSION},
        ]
        with patch.object(harness, "stop", side_effect=lambda process: self.trace.append("stop")), \
                patch.object(harness, "bounded_jsonl", return_value=audit), \
                patch.object(harness.subprocess, "check_output", side_effect=["clamp.py\n", b"fixture diff"]):
            self.smoke.managed()

    def test_restart_waits_for_execution_ack_and_preserves_original_identity(self):
        starting = observation("starting", "not_started", events=[])
        pre_prompt = observation("running", "not_started", events=[])
        active = observation()
        self.run_managed([starting, pre_prompt, active, observation("completed", "completed")])
        stop_index = self.trace.index("stop")
        reads = [entry[1] for entry in self.trace[:stop_index]
                 if isinstance(entry, tuple) and entry[0] == "observe_coding_agent"]
        self.assertEqual(len(reads), 3)
        self.assertNotIn("after_observation_token", reads[0])
        self.assertEqual(reads[1]["after_observation_token"], starting["observation_token"])
        self.assertEqual(reads[2]["after_observation_token"], pre_prompt["observation_token"])
        resumed_reads = [entry[1] for entry in self.trace[stop_index:]
                         if isinstance(entry, tuple) and entry[0] == "observe_coding_agent"]
        self.assertNotIn("after_observation_token", resumed_reads[0])
        starts = [entry for entry in self.trace if isinstance(entry, tuple)
                  and entry[0] == "start_agent_task_coding_run"]
        self.assertEqual(len(starts), 2)  # Original start plus exact-bound replay only.
        self.assertEqual(starts[0], starts[1])
        self.assertTrue(self.smoke.report["managed"]["passed"])
        self.assertTrue(self.smoke.report["recovery"]["passed"])
        self.assertEqual(self.smoke.report["recovery"]["phase_before_disconnect"], "in_flight")
        self.assertEqual(self.smoke.report["recovery"]["execution_state_before_disconnect"], "started")

    def test_already_completed_run_cannot_pass_in_flight_recovery(self):
        terminal = observation("completed", "completed")
        self.run_managed([terminal, terminal])
        self.assertTrue(self.smoke.report["managed"]["passed"])
        self.assertFalse(self.smoke.report["recovery"]["passed"])
        self.assertEqual(self.smoke.report["recovery"]["phase_before_disconnect"], "terminal")
        self.assertEqual(self.smoke.report["recovery"]["failure"], "in_flight_restart_not_observed")
        self.assertIn("restart", self.trace)

    def test_startup_failure_is_reconciled_without_claiming_live_execution(self):
        failed = observation("failed", "not_started", events=[])
        with self.assertRaisesRegex(harness.AcceptanceFailure, "managed_turn_not_completed"):
            self.run_managed([observation("starting", "not_started", events=[]), failed, failed])
        self.assertEqual(self.smoke.report["recovery"]["phase_before_disconnect"], "before_prompt")
        self.assertFalse(self.smoke.report["managed"]["passed"])
        self.assertFalse(self.smoke.report["recovery"]["passed"])
        self.assertIn("restart", self.trace)
        self.assertEqual(sum(isinstance(entry, tuple) and entry[0] == "reconcile_agent_task_coding_run"
                             for entry in self.trace), 2)

    def test_uncertain_execution_never_becomes_live_recovery_evidence(self):
        self.run_managed([observation("running", "outcome_unknown"), observation("completed", "completed")])
        self.assertTrue(self.smoke.report["managed"]["passed"])
        self.assertFalse(self.smoke.report["recovery"]["passed"])
        self.assertEqual(self.smoke.report["recovery"]["phase_before_disconnect"], "outcome_unknown")

    def test_permission_wait_and_lost_runs_are_not_live_execution(self):
        for state, execution, phase in (
            ("waiting_permission", "started", "waiting_permission"),
            ("lost", "outcome_unknown", "outcome_unknown"),
            ("failed", "started", "terminal"),
        ):
            with self.subTest(state=state):
                self.observations = [observation(state, execution)]
                self.smoke.observe_before_restart("run-original")
                self.assertEqual(self.smoke.report["recovery"]["phase_before_disconnect"], phase)
                self.assertFalse(self.smoke.report["recovery"]["passed"])

    def test_changed_attempt_after_restart_cannot_reach_acceptance(self):
        def changed_binding(name, args, token):
            result = self.tool(name, args, token)
            if name == "reconcile_agent_task_coding_run":
                result["attempt_id"] = "attempt-replacement"
            return result

        self.smoke.tool.side_effect = changed_binding
        with self.assertRaisesRegex(harness.AcceptanceFailure, "recovery_changed_execution_identity"):
            self.run_managed([observation(), observation("completed", "completed")])
        self.assertFalse(self.smoke.report["managed"]["passed"])
        self.assertFalse(self.smoke.report["recovery"]["passed"])
        self.assertIn("restart", self.trace)
        self.assertEqual(self.smoke.native_tests.call_count, 1)  # Baseline only.

    def test_wrong_run_missing_state_and_lost_history_stop_before_restart(self):
        for change, error in (
            ({"run_id": "run-other"}, "recovery_observed_wrong_run"),
            ({"execution_state": None}, "recovery_execution_state_missing"),
            ({"history_lost": True}, "managed_event_history_lost"),
            ({"history_lost": None}, "managed_event_history_lost"),
        ):
            with self.subTest(change=change):
                self.observations = [observation(**change)]
                with self.assertRaisesRegex(harness.AcceptanceFailure, error):
                    self.smoke.observe_before_restart("run-original")
                self.assertFalse(self.smoke.report["recovery"]["passed"])
        self.smoke.start_server.assert_not_called()

    def test_starting_observation_needs_a_cursor_and_live_runner(self):
        self.observations = [observation("starting", "not_started", observation_token=None)]
        with self.assertRaisesRegex(harness.AcceptanceFailure, "recovery_observation_token_missing"):
            self.smoke.observe_before_restart("run-original")
        self.smoke.runner.poll.return_value = 1
        self.smoke.tool.reset_mock()
        with self.assertRaisesRegex(harness.AcceptanceFailure, "runner_exited_before_restart"):
            self.smoke.observe_before_restart("run-original")
        self.smoke.tool.assert_not_called()

    def test_observation_deadline_cannot_reset_or_accept_late_running_state(self):
        for state, execution in (("starting", "not_started"), ("running", "started")):
            with self.subTest(state=state):
                self.observations = [observation(state, execution)]
                with patch.object(harness.time, "monotonic", side_effect=[0, 0, 31]):
                    with self.assertRaisesRegex(harness.AcceptanceFailure, "recovery_observation_deadline"):
                        self.smoke.observe_before_restart("run-original")
                self.assertFalse(self.smoke.report["recovery"]["passed"])
        self.smoke.start_server.assert_not_called()

    def test_reconciliation_rejects_changed_task_attempt_or_run(self):
        self.smoke.report["managed"].update(self.binding)
        self.smoke.require_original_binding(self.binding)
        for key in ("task_id", "attempt_id", "run_id"):
            for value in (None, "replacement"):
                with self.subTest(key=key, value=value):
                    with self.assertRaisesRegex(harness.AcceptanceFailure, "recovery_changed_execution_identity"):
                        self.smoke.require_original_binding({**self.binding, key: value})


class ConsumerActivityGuardTests(unittest.TestCase):
    def test_unrequested_activity_fails_even_if_it_never_completed(self):
        for event, kind, fields in (
            ("item.started", "command_execution", {}),
            ("item.failed", "web_search", {}),
            ("item.updated", "file_change", {}),
            ("item.started", "mcp_tool_call", {"server": "other", "tool": "read_files"}),
            ("item.failed", "mcp_tool_call", {"server": "bidirectional", "tool": "run_shell"}),
        ):
            with self.subTest(event=event, kind=kind, fields=fields):
                frames = [{"type": event, "item": {"type": kind, **fields}},
                          {"type": "item.completed", "item": {"type": "mcp_tool_call",
                           "server": "bidirectional", "tool": "read_files", "status": "completed"}}]
                with self.assertRaisesRegex(harness.AcceptanceFailure,
                                            "consumer_used_unrequested_tools|consumer_called_wrong_mcp_tool"):
                    harness.require_consumer_read_only_activity(frames)

    def test_exact_read_call_is_allowed_across_item_lifecycle(self):
        frames = [
            {"type": "item.started", "item": {"type": "mcp_tool_call", "server": "bidirectional"}},
            {"type": "item.updated", "item": {"type": "mcp_tool_call", "server": "bidirectional",
                                                 "tool": "read_files"}},
            {"type": "item.completed", "item": {"type": "mcp_tool_call", "server": "bidirectional",
                                                   "tool": "read_files", "status": "completed"}},
            {"type": "item.completed", "item": {"type": "agent_message", "text": "{}"}},
        ]
        harness.require_consumer_read_only_activity(frames)



if __name__ == "__main__":
    unittest.main()
