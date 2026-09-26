from __future__ import annotations

import os
from pathlib import Path
import unittest
from unittest.mock import MagicMock, call, patch

from scripts.webpi import standalone


class BootstrapUpgradeTests(unittest.TestCase):
    def test_candidate_id_is_ascii_bounded_and_traversal_safe(self) -> None:
        for value in ["candidate-1", "release_2026.09", "A1"]:
            self.assertTrue(standalone._valid_bootstrap_candidate_id(value), value)
        for value in ["", ".hidden", "../escape", "a/b", "a\\b", "候选", "a" * 65]:
            self.assertFalse(standalone._valid_bootstrap_candidate_id(value), value)

    def test_windows_process_parser_ignores_blank_lines_between_wmic_properties(self) -> None:
        script = Path(r"E:\WebPi\webpi-core\scripts\webpi\standalone.py")
        output = (
            '\nCommandLine="C:\\Python314\\python.exe" -I -B '
            'E:\\WebPi\\webpi-core\\scripts\\webpi\\standalone.py run\n\n'
            'ParentProcessId=4908\n\nProcessId=5192\n\n'
            'CommandLine="C:\\Python314\\python.exe" -c "print(1)"\n\nProcessId=9999\n'
        )
        self.assertEqual(
            standalone._parse_windows_standalone_processes(output, script),
            [(5192, "run")],
        )

    @staticmethod
    def _candidate_snapshot() -> tuple[Path, dict[str, dict[str, object]], dict[str, dict[str, object]]]:
        artifacts = {
            name: {"sha256": str(index) * 64, "size_bytes": index}
            for index, name in enumerate(standalone.DEPLOY_ARTIFACT_NAMES, start=1)
        }
        builds = {
            name: {
                "binary": name,
                "product": name.removesuffix(".exe"),
                "version": "0.4.1",
                "git_commit": "a" * 40,
                "git_dirty": False,
                "built_at": 1234567890,
            }
            for name in standalone.DEPLOY_ARTIFACT_NAMES
        }
        return Path("candidate/dogfood"), artifacts, builds

    @staticmethod
    def _shadow_success() -> dict[str, object]:
        return {
            "runner_config_valid": True,
            "server_shadow_healthy": True,
            "server_auth_verified": True,
            "loopback_only": True,
            "attempts": 1,
        }

    def test_runner_cutover_readiness_accepts_aligned_and_dirty_same_build(self) -> None:
        aligned = {
            "service": "webpi",
            "focus": {
                "client_id": standalone.CLIENT_ID,
                "connected": True,
                "status": "online",
                "compatibility_status": "compatible",
                "build": {"version": "0.4.1", "git_commit": "abc", "git_dirty": False},
                "source_alignment": {"status": "aligned"},
            },
            "server": {
                "version": "0.4.1",
                "build": {"git_commit": "abc", "git_dirty": False},
            },
        }
        self.assertEqual(
            standalone._runner_cutover_readiness(aligned, standalone.CLIENT_ID),
            (True, "aligned"),
        )

        dirty = {
            "service": "webpi",
            "focus": {
                "client_id": standalone.CLIENT_ID,
                "connected": True,
                "status": "online",
                "compatibility_status": "compatible",
                "build": {"version": "0.4.1", "git_commit": "abc", "git_dirty": True},
                "source_alignment": {
                    "status": "different",
                    "reason_code": "dirty_build_prevents_exact_source_alignment",
                },
            },
            "server": {
                "version": "0.4.1",
                "build": {"git_commit": "abc", "git_dirty": True},
            },
        }
        self.assertEqual(
            standalone._runner_cutover_readiness(dirty, standalone.CLIENT_ID),
            (True, "dirty_same_build"),
        )

    def test_runner_cutover_readiness_rejects_dirty_identity_mismatch_and_incompatible(self) -> None:
        value = {
            "service": "webpi",
            "focus": {
                "client_id": standalone.CLIENT_ID,
                "connected": True,
                "status": "online",
                "compatibility_status": "compatible",
                "build": {"version": "0.4.1", "git_commit": "abc", "git_dirty": True},
                "source_alignment": {
                    "status": "different",
                    "reason_code": "dirty_build_prevents_exact_source_alignment",
                },
            },
            "server": {
                "version": "0.4.1",
                "build": {"git_commit": "def", "git_dirty": True},
            },
        }
        self.assertEqual(
            standalone._runner_cutover_readiness(value, standalone.CLIENT_ID),
            (False, "dirty_build_identity_mismatch"),
        )
        value["focus"]["compatibility_status"] = "incompatible"
        self.assertEqual(
            standalone._runner_cutover_readiness(value, standalone.CLIENT_ID),
            (False, "runner_incompatible"),
        )

    def test_runner_cutover_readiness_rejects_other_source_mismatch(self) -> None:
        value = {
            "service": "webpi",
            "focus": {
                "client_id": standalone.CLIENT_ID,
                "connected": True,
                "status": "online",
                "compatibility_status": "compatible",
                "build": {"version": "0.4.1", "git_commit": "abc", "git_dirty": False},
                "source_alignment": {
                    "status": "different",
                    "reason_code": "git_commit_mismatch",
                },
            },
            "server": {
                "version": "0.4.1",
                "build": {"git_commit": "def", "git_dirty": False},
            },
        }
        self.assertEqual(
            standalone._runner_cutover_readiness(value, standalone.CLIENT_ID),
            (False, "source_mismatch:git_commit_mismatch"),
        )

    @unittest.skipUnless(os.name == "nt", "bootstrap-upgrade is Windows-only")
    def test_preflight_only_never_backs_up_stops_or_installs(self) -> None:
        snapshot = self._candidate_snapshot()
        containment = MagicMock()
        with (
            patch("scripts.webpi.standalone._bootstrap_candidate_snapshot", return_value=snapshot),
            patch("scripts.webpi.standalone._discover_windows_standalone_parent", return_value=(123, "run")),
            patch("scripts.webpi.standalone._bootstrap_hashes", return_value={name: "f" * 64 for name in standalone.DEPLOY_ARTIFACT_NAMES}),
            patch("scripts.webpi.standalone._create_process_containment", return_value=containment),
            patch("scripts.webpi.standalone._shadow_candidate_preflight", return_value=self._shadow_success()),
            patch("scripts.webpi.standalone.runner_config", return_value=Path("runner.toml")),
            patch("scripts.webpi.standalone._capture_rollback_consistency_identity", return_value={"safe": True}),
            patch("scripts.webpi.standalone._bootstrap_create_backup") as create_backup,
            patch("scripts.webpi.standalone._bootstrap_kill_tree") as kill_tree,
            patch("scripts.webpi.standalone._bootstrap_install_runtime") as install_runtime,
            patch("scripts.webpi.standalone.json_print") as output,
        ):
            rc = standalone.bootstrap_upgrade("candidate-1", preflight_only=True)
        self.assertEqual(rc, 0)
        containment.close.assert_called_once()
        create_backup.assert_not_called()
        kill_tree.assert_not_called()
        install_runtime.assert_not_called()
        self.assertEqual(output.call_args.args[0]["status"], "bootstrap-preflight-passed")

    @unittest.skipUnless(os.name == "nt", "bootstrap-upgrade is Windows-only")
    def test_candidate_change_after_shadow_fails_before_backup_or_stop(self) -> None:
        first = self._candidate_snapshot()
        changed = (
            first[0],
            {**first[1], "webpi.exe": {"sha256": "9" * 64, "size_bytes": 1}},
            first[2],
        )
        containment = MagicMock()
        with (
            patch("scripts.webpi.standalone._bootstrap_candidate_snapshot", side_effect=[first, changed]),
            patch("scripts.webpi.standalone._discover_windows_standalone_parent", return_value=(123, "run")),
            patch("scripts.webpi.standalone._bootstrap_hashes", return_value={name: "f" * 64 for name in standalone.DEPLOY_ARTIFACT_NAMES}),
            patch("scripts.webpi.standalone._create_process_containment", return_value=containment),
            patch("scripts.webpi.standalone._shadow_candidate_preflight", return_value=self._shadow_success()),
            patch("scripts.webpi.standalone.runner_config", return_value=Path("runner.toml")),
            patch("scripts.webpi.standalone._capture_rollback_consistency_identity", return_value={"safe": True}),
            patch("scripts.webpi.standalone._bootstrap_create_backup") as create_backup,
            patch("scripts.webpi.standalone._bootstrap_kill_tree") as kill_tree,
        ):
            with self.assertRaisesRegex(RuntimeError, "candidate changed during preflight"):
                standalone.bootstrap_upgrade("candidate-1", preflight_only=True)
        create_backup.assert_not_called()
        kill_tree.assert_not_called()

    @unittest.skipUnless(os.name == "nt", "bootstrap-upgrade is Windows-only")
    def test_failed_new_stack_restores_binary_backup_and_restarts_old_mode(self) -> None:
        snapshot = self._candidate_snapshot()
        candidate_dir = snapshot[0]
        backup_dir = Path("backup-dir")
        containment = MagicMock()
        new_parent = MagicMock(pid=200)
        new_parent.poll.return_value = None
        rollback_parent = MagicMock(pid=201)
        rollback_parent.poll.return_value = None
        with (
            patch("scripts.webpi.standalone._bootstrap_candidate_snapshot", return_value=snapshot),
            patch("scripts.webpi.standalone._discover_windows_standalone_parent", return_value=(123, "run-web")),
            patch("scripts.webpi.standalone._bootstrap_hashes", return_value={name: "f" * 64 for name in standalone.DEPLOY_ARTIFACT_NAMES}),
            patch("scripts.webpi.standalone._create_process_containment", return_value=containment),
            patch("scripts.webpi.standalone._shadow_candidate_preflight", return_value=self._shadow_success()),
            patch("scripts.webpi.standalone.runner_config", return_value=Path("runner.toml")),
            patch("scripts.webpi.standalone._capture_rollback_consistency_identity", return_value={"safe": True}),
            patch("scripts.webpi.standalone._bootstrap_create_backup", return_value=backup_dir),
            patch("scripts.webpi.standalone._bootstrap_kill_tree") as kill_tree,
            patch("scripts.webpi.standalone._bootstrap_wait_unlocked"),
            patch("scripts.webpi.standalone._bootstrap_install_runtime") as install_runtime,
            patch("scripts.webpi.standalone._bootstrap_start_stack", side_effect=[new_parent, rollback_parent]) as start_stack,
            patch("scripts.webpi.standalone._bootstrap_wait_healthy", side_effect=[RuntimeError("new unhealthy"), None]),
            patch("scripts.webpi.standalone.json_print"),
        ):
            with self.assertRaisesRegex(RuntimeError, "new unhealthy"):
                standalone.bootstrap_upgrade("candidate-1")
        self.assertEqual(install_runtime.call_args_list, [call(candidate_dir), call(backup_dir)])
        self.assertEqual(start_stack.call_args_list, [call("run-web"), call("run-web")])
        self.assertEqual(kill_tree.call_args_list[0], call(123))
        self.assertEqual(kill_tree.call_args_list[1], call(200))


if __name__ == "__main__":
    unittest.main()
