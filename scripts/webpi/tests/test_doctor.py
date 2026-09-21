from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts.webpi import standalone


class WebPiDoctorTests(unittest.TestCase):
    def runtime_payload(self, *, service="webpi", runner_status="online", connected=True, alignment="current"):
        return {
            "success": True,
            "output": {
                "service": service,
                "version": "0.4.1",
                "configured_public_url": "https://webpi.example.test",
                "auth_enabled": True,
                "focus": {
                    "client_id": standalone.CLIENT_ID,
                    "status": runner_status,
                    "connected": connected,
                    "compatibility_status": "compatible",
                    "project_count": 1,
                    "active_jobs": 0,
                    "job_concurrency": {"limit": 4, "running": 0, "queued": 0},
                    "source_alignment": {
                        "status": alignment,
                        "source_matches_server": alignment == "current",
                    },
                },
                "jobs": {"active_count": 0, "running_count": 0, "queued_count": 0},
                "version_compatibility": {
                    "status": "compatible",
                    "source_alignment": {"status": alignment},
                },
            },
        }

    def plugin_payload(self, *, status="ready"):
        return {
            "success": True,
            "output": {
                "name": "WebPi Pi Bridge",
                "plugin": "pi-bridge",
                "runner": standalone.CLIENT_ID,
                "status": status,
                "toolCount": 23,
            },
        }

    def static_status(self):
        return {
            "server_online": True,
            "native_binaries_present": True,
            "runner_config_present": True,
            "pi_bridge_present": True,
            "action_token_present": True,
            "auth_hardened": True,
            "public_surface_hardened": True,
            "public_url": "https://webpi.example.test",
        }

    def test_doctor_passes_for_healthy_runtime_and_ready_pi_bridge(self):
        with patch.object(standalone, "safe_status", return_value=self.static_status()), patch.object(
            standalone,
            "local_action_post",
            side_effect=[self.runtime_payload(), self.plugin_payload()],
        ):
            report = standalone.doctor_status()
        self.assertEqual(report["status"], "pass")
        self.assertTrue(report["healthy"])
        self.assertEqual(report["runner"]["status"], "online")
        self.assertEqual(report["pi_bridge"]["status"], "ready")
        self.assertNotIn("token", str(report).lower())

    def test_doctor_fails_on_wrong_service_identity(self):
        with patch.object(standalone, "safe_status", return_value=self.static_status()), patch.object(
            standalone,
            "local_action_post",
            side_effect=[self.runtime_payload(service="other"), self.plugin_payload()],
        ):
            report = standalone.doctor_status()
        self.assertEqual(report["status"], "fail")
        self.assertFalse(report["healthy"])
        self.assertIn("runtime_service_identity", report["failed_checks"])

    def test_doctor_fails_when_runner_is_offline(self):
        with patch.object(standalone, "safe_status", return_value=self.static_status()), patch.object(
            standalone,
            "local_action_post",
            side_effect=[self.runtime_payload(runner_status="offline", connected=False), self.plugin_payload()],
        ):
            report = standalone.doctor_status()
        self.assertEqual(report["status"], "fail")
        self.assertIn("runner_online", report["failed_checks"])

    def test_doctor_fails_when_pi_bridge_is_not_ready(self):
        with patch.object(standalone, "safe_status", return_value=self.static_status()), patch.object(
            standalone,
            "local_action_post",
            side_effect=[self.runtime_payload(), self.plugin_payload(status="failed")],
        ):
            report = standalone.doctor_status()
        self.assertEqual(report["status"], "fail")
        self.assertIn("pi_bridge_ready", report["failed_checks"])

    def test_doctor_warns_but_does_not_fail_on_dirty_source_alignment(self):
        with patch.object(standalone, "safe_status", return_value=self.static_status()), patch.object(
            standalone,
            "local_action_post",
            side_effect=[self.runtime_payload(alignment="different"), self.plugin_payload()],
        ):
            report = standalone.doctor_status()
        self.assertEqual(report["status"], "warn")
        self.assertTrue(report["healthy"])
        self.assertIn("source_alignment", report["warning_checks"])

    def test_local_action_post_requires_private_action_token_file(self):
        with tempfile.TemporaryDirectory() as td, patch.object(
            standalone, "ACTION_TOKEN_FILE", Path(td) / "missing-token"
        ):
            with self.assertRaisesRegex(RuntimeError, "action token"):
                standalone.local_action_post("runtime_status", {})

    def test_action_token_scopes_add_recommended_read_only_without_control(self):
        scopes = standalone.action_token_scopes(
            [
                "runtime:read",
                "runner:manage",
                "session:collaborate",
                "project:read",
                "project:write",
                "job:run",
            ]
        )
        for required in (
            "plugin:inspect",
            "plugin:invoke",
            "computer:read",
            "computer:display_read",
            "browser:read",
        ):
            self.assertIn(required, scopes)
        for forbidden in (
            "computer:control",
            "computer:pointer_control",
            "computer:keyboard_control",
            "computer:clipboard_read",
            "computer:clipboard_write",
            "browser:control",
            "browser:launch",
        ):
            self.assertNotIn(forbidden, scopes)
        self.assertEqual(len(scopes), len(set(scopes)))


if __name__ == "__main__":
    unittest.main()
