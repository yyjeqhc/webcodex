import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import read_handoff as recovery
from external_observation_hook import AdapterError
from platform_security import secure_created_path


class ReadHandoffTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name).resolve()
        self.project_root = root / "project"
        self.project_root.mkdir()
        self.auth = root / "auth"
        self.auth.write_text("Bearer fixture")
        secure_created_path(self.auth)
        self.config = {
            "server_url": "http://127.0.0.1:12345",
            "authorization_file": str(self.auth),
            "project": "agent:fixture:project",
            "project_root": str(self.project_root),
            "workflow_session_id": "wc_sess_fixture",
            "local_session_id": "local-fixture",
            "state_dir": str(root / "state"),
        }
        self.brief = {
            "session": {"session_id": self.config["workflow_session_id"]},
            "basis": {"complete": False, "reason_codes": ["workspace_unavailable"]},
            "external_observations": {
                "provenance": "external_report", "coverage": {"complete": False},
                "unknown_count": 1, "observations": [{"event_id": "a" * 64, "status": "unknown"}],
            },
            "deterministic": True,
            "llm_summary": False,
        }
        self.body = {"success": True, "output": {
            "project": self.config["project"],
            "session_id": self.config["workflow_session_id"],
            "handoff_brief": self.brief,
        }}

    def read(self):
        class Response:
            def __init__(self, body):
                self.body = body

            def __enter__(self):
                return self

            def __exit__(self, *args):
                return None

            def read(self, size):
                return json.dumps(self.body).encode()

        with patch.object(recovery.urllib.request, "build_opener") as opener:
            opener.return_value.open.return_value = Response(self.body)
            result = recovery.read_handoff(self.config, current_dir=self.project_root)
            return result, opener.return_value.open.call_args

    def test_exact_read_preserves_incomplete_and_unknown(self):
        result, call = self.read()
        self.assertEqual(result["status"], "read")
        self.assertEqual(result["handoff_brief"], self.brief)
        self.assertFalse(result["handoff_brief"]["basis"]["complete"])
        self.assertEqual(result["handoff_brief"]["external_observations"]["unknown_count"], 1)
        request = call.args[0]
        self.assertEqual(json.loads(request.data), {
            "tool": "get_session_handoff_state",
            "params": {"project": self.config["project"], "session_id": self.config["workflow_session_id"]},
        })
        self.assertEqual(request.get_header("Authorization"), "Bearer fixture")
        self.assertNotIn("Bearer fixture", json.dumps(result))


    def valid_goal_context(self):
        return {
            "version": 1,
            "source": "explicit_workflow_session_correlation",
            "status": "available",
            "reason_code": None,
            "truncated": False,
            "goal": {
                "goal_id": "wc_goal_fixture",
                "title": "Recovery goal",
                "title_truncated": False,
                "lifecycle": "active",
                "revision": 3,
                "objective": {"excerpt": "Recover the exact work", "truncated": False},
                "plan": {
                    "completion_conditions": {
                        "items": ["Focused verification"],
                        "total": 1,
                        "returned": 1,
                        "truncated": False,
                        "content_truncated": False,
                    },
                    "steps": {
                        "items": [{
                            "id": "verify",
                            "title": "Verify",
                            "title_truncated": False,
                            "status": "in_progress",
                        }],
                        "total": 1,
                        "returned": 1,
                        "truncated": False,
                    },
                    "current_step_id": "verify",
                    "checkpoint": {
                        "summary": {"excerpt": "Implementation complete", "truncated": False},
                        "at_unix_ms": 1234,
                    },
                },
            },
            "candidates": [],
        }

    def test_optional_goal_context_is_validated_and_preserved(self):
        goal_context = self.valid_goal_context()
        self.body["output"]["goal_context"] = goal_context
        result, _ = self.read()
        self.assertEqual(result["goal_context"], goal_context)

        selection = {
            "version": 1,
            "source": "explicit_workflow_session_correlation",
            "status": "selection_required",
            "reason_code": "multiple_active_goals",
            "truncated": False,
            "goal": None,
            "candidates": [
                {
                    "goal_id": "wc_goal_a",
                    "title": "A",
                    "title_truncated": False,
                    "lifecycle": "active",
                    "revision": 1,
                },
                {
                    "goal_id": "wc_goal_b",
                    "title": "B",
                    "title_truncated": False,
                    "lifecycle": "active",
                    "revision": 2,
                },
            ],
        }
        self.body["output"]["goal_context"] = selection
        result, _ = self.read()
        self.assertEqual(result["goal_context"], selection)

    def test_goal_context_never_accepts_implicit_selection_or_malformed_bounds(self):
        cases = []

        wrong_source = self.valid_goal_context()
        wrong_source["source"] = "project_recency"
        cases.append(wrong_source)

        leaked_selection = {
            "version": 1,
            "source": "explicit_workflow_session_correlation",
            "status": "selection_required",
            "reason_code": "multiple_active_goals",
            "truncated": False,
            "goal": self.valid_goal_context()["goal"],
            "candidates": [
                {
                    "goal_id": "wc_goal_a",
                    "title": "A",
                    "title_truncated": False,
                    "lifecycle": "active",
                    "revision": 1,
                },
                {
                    "goal_id": "wc_goal_b",
                    "title": "B",
                    "title_truncated": False,
                    "lifecycle": "active",
                    "revision": 2,
                },
            ],
        }
        cases.append(leaked_selection)

        oversized = self.valid_goal_context()
        oversized["goal"]["objective"]["excerpt"] = "x" * 1025
        cases.append(oversized)

        for goal_context in cases:
            with self.subTest(status=goal_context["status"], source=goal_context["source"]):
                self.body["output"]["goal_context"] = goal_context
                with self.assertRaisesRegex(AdapterError, "handoff_goal_context_invalid"):
                    self.read()
        self.body["output"].pop("goal_context", None)

    def test_wrong_project_or_session_or_nested_brief_is_rejected(self):
        for edit in (
            lambda: self.body["output"].update(project="agent:fixture:other"),
            lambda: self.body["output"].update(session_id="wc_sess_other"),
            lambda: self.brief["session"].update(session_id="wc_sess_other"),
        ):
            with self.subTest(edit=edit):
                saved = json.loads(json.dumps(self.body))
                edit()
                with self.assertRaisesRegex(AdapterError, "handoff_identity_mismatch"):
                    self.read()
                self.body = saved
                self.brief = self.body["output"]["handoff_brief"]

    def test_failed_result_or_missing_basis_is_not_presented_as_recovery(self):
        self.body["success"] = False
        with self.assertRaisesRegex(AdapterError, "handoff_read_failed"):
            self.read()
        self.body["success"] = True
        del self.brief["basis"]
        with self.assertRaisesRegex(AdapterError, "handoff_basis_missing"):
            self.read()
        self.brief["basis"] = {"complete": "true"}
        with self.assertRaisesRegex(AdapterError, "handoff_basis_missing"):
            self.read()

    def test_missing_external_contract_is_not_presented_as_recovery(self):
        for edit in (
            lambda: self.brief.pop("external_observations"),
            lambda: self.brief["external_observations"].update(provenance="native"),
            lambda: self.brief["external_observations"]["coverage"].update(complete=True),
        ):
            with self.subTest(edit=edit):
                saved = json.loads(json.dumps(self.body))
                edit()
                with self.assertRaisesRegex(AdapterError, "handoff_external_observations_missing"):
                    self.read()
                self.body = saved
                self.brief = self.body["output"]["handoff_brief"]

    def test_nondeterministic_or_llm_summary_is_rejected(self):
        for key, value in (("deterministic", False), ("llm_summary", True)):
            with self.subTest(key=key):
                saved = json.loads(json.dumps(self.body))
                self.brief[key] = value
                with self.assertRaisesRegex(AdapterError, "handoff_contract_invalid"):
                    self.read()
                self.body = saved
                self.brief = self.body["output"]["handoff_brief"]

    def test_outside_project_never_sends(self):
        with patch.object(recovery.urllib.request, "build_opener") as opener:
            with self.assertRaisesRegex(AdapterError, "outside_configured_project"):
                recovery.read_handoff(self.config, current_dir=self.project_root.parent)
            opener.assert_not_called()

    def test_bad_authorization_never_sends(self):
        self.auth.write_text("Bearer fixture\nInjected: header")
        with patch.object(recovery.urllib.request, "build_opener") as opener:
            with self.assertRaisesRegex(AdapterError, "invalid_authorization_file"):
                recovery.read_handoff(self.config, current_dir=self.project_root)
            opener.assert_not_called()


if __name__ == "__main__":
    unittest.main()
