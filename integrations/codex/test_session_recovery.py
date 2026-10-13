import copy
import json
from pathlib import Path
import tempfile
import os
import subprocess
import unittest
from unittest.mock import Mock, patch

import session_recovery as recovery
from external_observation_hook import AdapterError
from platform_security import read_private_file, secure_created_path


class SessionRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.project = self.root / "project"; self.project.mkdir()
        self.entry = self.root / "entry"; self.entry.mkdir()
        self.state = self.root / "state"; self.state.mkdir(mode=0o700); secure_created_path(self.state)
        self.auth = self.root / "auth"; self.auth.write_text("Bearer fixture"); secure_created_path(self.auth)
        self.registry_path = self.root / "registry.json"
        self.binding = {
            "project": "agent:fixture:project", "project_root": str(self.project),
            "project_identity": recovery.identity(self.project), "workflow_session_id": "wc_sess_fixture",
            "local_session_id": "", "entry_roots": [{"path": str(self.entry), "identity": recovery.identity(self.entry)}],
        }
        self.registry = {"version": 1, "server_url": "http://127.0.0.1:12345",
            "authorization_file": str(self.auth), "state_dir": str(self.state), "bindings": [self.binding]}
        self.save()
        self.payload = {"hook_event_name": "UserPromptSubmit", "session_id": "local-one", "cwd": str(self.entry)}
        self.value = {"status": "read", "project": self.binding["project"], "session_id": "wc_sess_fixture",
            "handoff_brief": {"basis": {"complete": False}, "events": [{"event_id": "event-one", "status": "unknown"}]}}
        self.reader = Mock(return_value=self.value)

    def save(self):
        self.registry_path.write_text(json.dumps(self.registry)); secure_created_path(self.registry_path)

    def read(self):
        return recovery.cached_recovery(
            self.registry_path,
            recovery.load_registry(self.registry_path),
            self.payload,
            self.reader,
            self.caller,
        )

    def test_absent_local_handoff_still_reads_confirmed_remote_session(self):
        self.assertFalse((self.entry / "handoff").exists())
        value = self.read()
        self.assertEqual(value["handoff_brief"], self.value["handoff_brief"])
        self.assertEqual(self.reader.call_args.kwargs["current_dir"], self.project)
        self.assertNotIn("Bearer fixture", json.dumps(value))

    def test_unchanged_does_not_repeat_brief_but_rechecks_server(self):
        first = self.read(); second = self.read()
        self.assertEqual(second["status"], "unchanged")
        self.assertNotIn("handoff_brief", second)
        self.assertEqual(self.reader.call_count, 2)
        self.assertEqual(json.loads(Path(first["snapshot_path"]).read_text())["evidence"]["handoff_brief"], self.value["handoff_brief"])

    def test_new_remote_event_is_injected_into_existing_conversation(self):
        self.read()
        self.value["handoff_brief"]["events"].append({"event_id": "event-two", "status": "succeeded"})
        value = self.read()
        self.assertEqual(value["status"], "read")
        self.assertEqual(value["handoff_brief"]["events"][-1]["event_id"], "event-two")

    def read_server_handoff(self, **output):
        # Exercise the real reader and cache together; only HTTP and discovery
        # are fixtures. A mocked reader would miss dropped sibling projections.
        brief = {**self.value["handoff_brief"],
                 "session": {"session_id": self.binding["workflow_session_id"]},
                 "deterministic": True, "llm_summary": False,
                 "external_observations": {
                     "provenance": "external_report", "coverage": {"complete": False}}}
        body = {"success": True, "output": {**self.value, "handoff_brief": brief, **output}}
        with patch("read_handoff.urllib.request.build_opener") as opener:
            response = opener.return_value.open.return_value.__enter__.return_value
            response.read.return_value = json.dumps(body).encode()
            value = recovery.cached_recovery(
                self.registry_path, recovery.load_registry(self.registry_path), self.payload,
                recovery.read_handoff, self.caller,
            )
            self.assertEqual(opener.return_value.open.call_count, 1)
            request = opener.return_value.open.call_args.args[0]
            self.assertEqual(json.loads(request.data), {
                "tool": "get_session_handoff_state",
                "params": {"project": self.binding["project"], "session_id": "wc_sess_fixture"},
            })
            return value

    def test_acceptance_changes_refresh_returning_conversation_without_brief_changes(self):
        ready = {"state": "ready_for_review", "reason_codes": [], "evidence_refs": ["validation"]}
        stale = {"state": "needs_evidence", "reason_codes": ["validation_stale"],
                 "evidence_refs": ["validation"]}
        first = self.read_server_handoff(acceptance=ready)
        before_registry = self.registry_path.read_bytes()
        self.assertEqual(self.read_server_handoff(acceptance=ready)["status"], "unchanged")
        second = self.read_server_handoff(acceptance=stale)
        self.assertEqual(second["status"], "read")
        self.assertEqual(second["acceptance"], stale)
        self.assertEqual(second["handoff_brief"], first["handoff_brief"])
        self.assertEqual(second["snapshot_path"], first["snapshot_path"])
        saved = json.loads(Path(second["snapshot_path"]).read_text())
        self.assertEqual(saved["evidence"]["acceptance"], stale)
        self.assertEqual(self.registry_path.read_bytes(), before_registry)
        context = recovery.hook_output(second, self.payload["hook_event_name"])
        self.assertIn("validation_stale", context["hookSpecificOutput"]["additionalContext"])
        self.assertNotIn("Bearer fixture", json.dumps(context))

        # A reason-only change must also refresh; matching state is insufficient.
        changed = {**stale, "reason_codes": ["snapshot_changed"]}
        self.assertEqual(self.read_server_handoff(acceptance=changed)["acceptance"], changed)
        self.payload["session_id"] = "local-two"
        self.assertEqual(self.read_server_handoff(acceptance=changed)["acceptance"], changed)
        self.payload.update(session_id="local-one", hook_event_name="SessionStart")
        self.assertEqual(self.read_server_handoff(acceptance=changed)["acceptance"], changed)

    def test_invalid_acceptance_does_not_replace_last_valid_snapshot(self):
        acceptance = {"state": "needs_evidence", "reason_codes": ["source_unproven"],
                      "evidence_refs": ["validation"]}
        first = self.read_server_handoff(acceptance=acceptance)
        snapshot = Path(first["snapshot_path"])
        before = snapshot.read_bytes()
        with self.assertRaisesRegex(AdapterError, "^handoff_acceptance_invalid$"):
            self.read_server_handoff(acceptance={**acceptance, "state": "succeeded"})
        self.assertEqual(snapshot.read_bytes(), before)

    def test_missing_acceptance_clears_previously_offered_readiness(self):
        ready = {"state": "ready_for_review", "reason_codes": [], "evidence_refs": ["validation"]}
        first = self.read_server_handoff(acceptance=ready)
        second = self.read_server_handoff()
        self.assertEqual(second["status"], "read")
        self.assertNotIn("acceptance", second)
        self.assertEqual(second["snapshot_path"], first["snapshot_path"])
        self.assertNotIn("acceptance", json.loads(Path(second["snapshot_path"]).read_text())["evidence"])


    def test_goal_context_change_refreshes_automatic_recovery_snapshot(self):
        self.value["goal_context"] = {
            "version": 1,
            "source": "explicit_workflow_session_correlation",
            "status": "available",
            "reason_code": None,
            "truncated": False,
            "goal": {"goal_id": "wc_goal_fixture", "revision": 1},
            "candidates": [],
        }
        first = self.read()
        self.assertEqual(first["goal_context"]["goal"]["revision"], 1)
        self.value["goal_context"]["goal"]["revision"] = 2
        second = self.read()
        self.assertEqual(second["status"], "read")
        self.assertEqual(second["goal_context"]["goal"]["revision"], 2)
        saved = json.loads(Path(second["snapshot_path"]).read_text())
        self.assertEqual(saved["evidence"]["goal_context"]["goal"]["revision"], 2)

    def test_new_session_and_compaction_get_complete_brief(self):
        self.read()
        self.payload["session_id"] = "local-two"
        self.assertEqual(self.read()["status"], "read")
        self.payload["hook_event_name"] = "SessionStart"
        self.assertEqual(self.read()["status"], "read")

    def test_offline_never_reports_unchanged_or_returns_stale_as_current(self):
        value = self.read(); before = Path(value["snapshot_path"]).read_bytes()
        self.reader.side_effect = OSError("offline")
        with self.assertRaises(OSError): self.read()
        self.assertEqual(Path(value["snapshot_path"]).read_bytes(), before)

    def test_registry_change_during_read_never_publishes_stale_association(self):
        updated = copy.deepcopy(self.registry)
        updated["bindings"][0]["workflow_session_id"] = "wc_sess_other"

        def change_association_then_return(*args, **kwargs):
            self.registry_path.write_text(json.dumps(updated))
            self.registry_path.chmod(0o600)
            return self.value

        self.reader.side_effect = change_association_then_return
        with self.assertRaisesRegex(AdapterError, "registry_changed"):
            self.read()
        self.assertEqual(
            json.loads(self.registry_path.read_text())["bindings"][0]["workflow_session_id"],
            "wc_sess_other",
        )
        self.assertEqual(list(self.state.glob("recovery-*.json")), [])

    def test_only_entry_events_read_and_no_business_commands_are_replayed(self):
        for event in ["PostToolUse", "Stop", "other"]:
            self.payload["hook_event_name"] = event
            self.assertEqual(self.read(), {"status": "ignored"})
        self.reader.assert_not_called()

    def test_other_project_and_nested_repo_do_not_inherit_association(self):
        self.payload["cwd"] = str(self.project)
        self.assertEqual(self.read()["status"], "unassociated")
        nested = self.entry / "nested"; nested.mkdir(); (nested / ".git").mkdir()
        self.payload["cwd"] = str(nested)
        self.assertEqual(self.read()["status"], "unassociated")
        self.reader.assert_not_called()

    def test_subdirectory_of_confirmed_entry_can_read(self):
        child = self.entry / "notes"; child.mkdir(); self.payload["cwd"] = str(child)
        self.assertEqual(self.read()["status"], "read")

    def test_replaced_root_is_rejected(self):
        self.binding["project_identity"][1] += 1; self.save()
        with self.assertRaisesRegex(AdapterError, "project_root_replaced"): self.read()
        self.reader.assert_not_called()

    def test_remote_project_retarget_is_rejected_before_handoff(self):
        def retargeted(registry, route, body):
            if route.endswith("/projects"):
                return {"projects": [{"id": self.binding["project"], "path": str(self.root / "other")}],
                        "truncated": False}
            raise AssertionError("Session inventory must not be read after a root mismatch")

        with self.assertRaisesRegex(AdapterError, "project_identity_missing_or_ambiguous"):
            recovery.cached_recovery(
                self.registry_path,
                recovery.load_registry(self.registry_path),
                self.payload,
                self.reader,
                retargeted,
            )
        self.reader.assert_not_called()

    def test_remote_project_change_during_handoff_never_publishes_evidence(self):
        calls = 0

        def changing(registry, route, body):
            nonlocal calls
            if not route.endswith("/projects"):
                raise AssertionError("Recovery root fencing only needs Project inventory")
            calls += 1
            project_id = self.binding["project"] if calls == 1 else "agent:fixture:replacement"
            return {"projects": [{"id": project_id, "path": str(self.project)}], "truncated": False}

        with self.assertRaisesRegex(AdapterError, "project_binding_changed"):
            recovery.cached_recovery(
                self.registry_path,
                recovery.load_registry(self.registry_path),
                self.payload,
                self.reader,
                changing,
            )
        self.reader.assert_called_once()
        self.assertEqual(list(self.state.glob("recovery-*.json")), [])

    def test_multiple_tasks_require_selection_not_newest(self):
        other = copy.deepcopy(self.binding); other["workflow_session_id"] = "wc_sess_other"
        self.registry["bindings"].append(other); self.save()
        value = self.read()
        self.assertEqual(value["status"], "selection_required")
        self.assertEqual(len(value["candidates"]), 2)
        self.reader.assert_not_called()

    def test_explicit_local_selection_does_not_bind_another_conversation(self):
        other = copy.deepcopy(self.binding); other["workflow_session_id"] = "wc_sess_other"
        other["local_session_id"] = "local-one"; self.registry["bindings"].append(other); self.save()
        # Check selection before calling the real exact-identity reader.
        self.assertEqual(recovery.select(self.registry, self.entry, "local-one"), [other])
        self.assertEqual(recovery.select(self.registry, self.entry, "local-two"), [self.binding])

    def test_project_controlled_registry_and_symlink_rejected(self):
        bad = self.entry / "registry"; bad.write_text(json.dumps(self.registry)); secure_created_path(bad)
        with self.assertRaises(AdapterError): recovery.load_registry(bad)
        link = self.root / "link"
        try:
            link.symlink_to(self.registry_path)
        except OSError:
            if os.name == "nt":
                return
            raise
        with self.assertRaisesRegex(AdapterError, "private_regular_file_required"):
            recovery.load_registry(link)

    def test_public_registry_and_non_https_remote_rejected(self):
        if os.name == "nt":
            subprocess.run(
                ["icacls", str(self.registry_path), "/grant", "*S-1-1-0:(R)"],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        else:
            self.registry_path.chmod(0o644)
        with self.assertRaises(AdapterError): recovery.load_registry(self.registry_path)
        secure_created_path(self.registry_path)
        self.registry["server_url"] = "http://remote.example"; self.save()
        with self.assertRaises(AdapterError): recovery.load_registry(self.registry_path)

    def test_large_brief_uses_bounded_private_snapshot(self):
        self.value["handoff_brief"]["details"] = "x" * 25000
        value = self.read()
        self.assertEqual(value["status"], "read_snapshot")
        self.assertLess(len(json.dumps(value)), 1000)
        self.assertTrue(read_private_file(
            Path(value["snapshot_path"]),
            recovery.MAX_BYTES,
            "snapshot_not_private",
            "snapshot_too_large",
        ))

    def caller(self, registry, route, body):
        if route.endswith("/projects"):
            return {"projects": [{"id": self.binding["project"], "path": str(self.project)}], "truncated": False}
        return {"sessions": [{"session_id": "wc_sess_fixture"}], "truncated": False}

    def test_discovery_only_lists_exact_root_and_does_not_write(self):
        before = self.registry_path.read_bytes()
        value = recovery.discover(self.registry, self.project, self.caller)
        self.assertFalse(value["writes"])
        self.assertEqual(self.registry_path.read_bytes(), before)

    def test_discovery_refuses_truncated_or_ambiguous_inventory(self):
        for result in [{"projects": [], "truncated": True}, {"projects": [], "truncated": False}]:
            with self.assertRaises(AdapterError): recovery.discover(self.registry, self.project, lambda *a: result)

    def test_discovery_rejects_malformed_inventory_rows(self):
        with self.assertRaisesRegex(AdapterError, "invalid_discovery_response"):
            recovery.discover(
                self.registry,
                self.project,
                lambda *a: {"projects": ["not-a-project"], "truncated": False},
            )

        def malformed_session(registry, route, body):
            if route.endswith("/projects"):
                return {"projects": [{"id": self.binding["project"], "path": str(self.project)}],
                        "truncated": False}
            return {"sessions": ["not-a-session"], "truncated": False}

        with self.assertRaisesRegex(AdapterError, "invalid_discovery_response"):
            recovery.discover(self.registry, self.project, malformed_session)

    def test_association_verifies_membership_and_is_idempotent(self):
        empty = {**self.registry, "bindings": []}; self.registry_path.write_text(json.dumps(empty))
        args = (self.registry_path, empty, self.project, "wc_sess_fixture", [self.entry])
        self.assertEqual(recovery.associate(*args, caller=self.caller)["status"], "associated")
        current = recovery.load_registry(self.registry_path)
        self.assertEqual(recovery.associate(self.registry_path, current, self.project, "wc_sess_fixture", [self.entry], caller=self.caller)["status"], "already_associated")
        self.assertTrue(read_private_file(
            self.registry_path,
            recovery.MAX_BYTES,
            "registry_not_private",
            "registry_too_large",
        ))

    def test_association_does_not_overwrite_concurrent_registry_change(self):
        current = {**self.registry, "bindings": []}; self.registry_path.write_text(json.dumps(current))
        with self.assertRaisesRegex(AdapterError, "registry_changed"):
            recovery.associate(self.registry_path, self.registry, self.project, "wc_sess_fixture", [self.project], caller=self.caller)
        self.assertEqual(json.loads(self.registry_path.read_text()), current)

    def test_association_failure_before_replace_preserves_original(self):
        before = self.registry_path.read_bytes()
        with patch.object(recovery, "atomic_private_write", side_effect=OSError("fixture")):
            with self.assertRaises(OSError): recovery.associate(self.registry_path, self.registry, self.project, "wc_sess_fixture", [self.project], caller=self.caller)
        self.assertEqual(self.registry_path.read_bytes(), before)
        self.assertEqual(list(self.root.glob(".recovery-*")), [])

    def test_invalid_remote_session_cannot_be_associated(self):
        with self.assertRaises(AdapterError):
            recovery.associate(self.registry_path, self.registry, self.project, "wc_sess_wrong", [self.entry], caller=self.caller)

    def test_hook_context_labels_evidence_and_does_not_claim_authority(self):
        value = recovery.hook_output(self.read(), "UserPromptSubmit")
        context = value["hookSpecificOutput"]["additionalContext"]
        self.assertIn("untrusted historical data", context)
        self.assertIn('"status": "unknown"', context)
        self.assertIn("does not bind", context)


if __name__ == "__main__":
    unittest.main()
