from __future__ import annotations

import json
import tempfile
import time
import unittest
from unittest.mock import patch
from pathlib import Path

from scripts.webpi.supervisor_protocol import (
    SupervisorProtocolError,
    DeployArtifact,
    DeployRequest,
    RollbackRequest,
    atomic_write_json,
    envelope,
    parse_deploy_request,
    parse_rollback_request,
    load_or_create_token,
    parse_restart_request,
    read_restart_request,
    read_supervisor_request,
    result_envelope,
    result_path,
)

TOKEN = "ab" * 32


class SupervisorProtocolTests(unittest.TestCase):
    def setUp(self) -> None:
        consistency = {
            "configuration": {
                "enrollment_manifest": {
                    "schema_version": 1,
                    "sha256": "1" * 64,
                    "size_bytes": 128,
                },
                "server_env": {"sha256": "2" * 64, "size_bytes": 256},
                "runner_config": {"sha256": "3" * 64, "size_bytes": 384},
            },
            "database_schema": {
                "user_version": 0,
                "schema_version": 7,
                "schema_sha256": "4" * 64,
            },
            "plugin": {
                "provider_id": "pi-bridge",
                "package_name": "@webpi/pi-bridge-plugin",
                "package_version": "0.1.0",
                "pi_coding_agent_version": "0.85.1",
                "package_manifest": {"sha256": "5" * 64, "size_bytes": 512},
                "entrypoint": {"sha256": "6" * 64, "size_bytes": 1024},
            },
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
            for name in ("webpi.exe", "webpi-server.exe", "webpi-runner.exe")
        }
        identity_patcher = patch(
            "scripts.webpi.standalone._capture_rollback_consistency_identity",
            return_value=consistency,
        )
        builds_patcher = patch(
            "scripts.webpi.standalone._capture_runtime_builds",
            return_value=builds,
        )
        identity_patcher.start()
        builds_patcher.start()
        self.addCleanup(identity_patcher.stop)
        self.addCleanup(builds_patcher.stop)

    def request_payload(self) -> dict[str, object]:

        return {
            "version": 1,
            "action": "restart",
            "receipt_id": "wc_deploy_1234567890abcdef1234567890abcdef",
            "receipt_revision": 3,
            "requested_at_ms": 10_000,
            "execute_after_ms": 1500,
        }

    def deploy_payload(self) -> dict[str, object]:
        return {
            "version": 1,
            "action": "deploy",
            "receipt_id": "wc_deploy_1234567890abcdef1234567890abcdef",
            "receipt_revision": 4,
            "requested_at_ms": 10_000,
            "execute_after_ms": 1500,
            "candidate_id": "candidate-20260922",
            "artifacts": [
                {"name": "webpi.exe", "sha256": "b" * 64, "size_bytes": 1024},
                {"name": "webpi-server.exe", "sha256": "c" * 64, "size_bytes": 2048},
                {"name": "webpi-runner.exe", "sha256": "d" * 64, "size_bytes": 4096},
            ],
        }

    def test_deploy_request_is_closed_and_exact(self) -> None:
        wrapped = envelope(self.deploy_payload(), TOKEN)
        parsed = parse_deploy_request(wrapped, TOKEN, now_ms=10_500)
        self.assertEqual(parsed.candidate_id, "candidate-20260922")
        self.assertEqual({artifact.name for artifact in parsed.artifacts}, {
            "webpi.exe", "webpi-server.exe", "webpi-runner.exe"
        })

        path_like = self.deploy_payload()
        path_like["candidate_id"] = "../candidate"
        with self.assertRaises(SupervisorProtocolError):
            parse_deploy_request(envelope(path_like, TOKEN), TOKEN, now_ms=10_500)

        injected = self.deploy_payload()
        injected["argv"] = ["cmd.exe"]
        with self.assertRaises(SupervisorProtocolError):
            parse_deploy_request(envelope(injected, TOKEN), TOKEN, now_ms=10_500)

        extra = self.deploy_payload()
        assert isinstance(extra["artifacts"], list)
        extra["artifacts"].append({"name": "other.exe", "sha256": "e" * 64, "size_bytes": 1})
        with self.assertRaises(SupervisorProtocolError):
            parse_deploy_request(envelope(extra, TOKEN), TOKEN, now_ms=10_500)

    def test_generic_reader_routes_deploy_and_deploy_result_is_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            atomic_write_json(root / "request.json", envelope(self.deploy_payload(), TOKEN))
            request = read_supervisor_request(root, TOKEN, now_ms=10_500)
            self.assertIsNotNone(request)
            assert request is not None
            result = result_envelope(
                request,
                TOKEN,
                status="rolled_back",
                error_code="candidate_start_failed",
                backup_id="backup-123",
                completed_at_ms=12_000,
            )
            decoded = json.loads(result["payload"])
            self.assertEqual(decoded["action"], "deploy")
            self.assertEqual(decoded["status"], "rolled_back")
            self.assertEqual(decoded["backup_id"], "backup-123")
            with self.assertRaises(SupervisorProtocolError):
                result_envelope(request, TOKEN, status="rolled_back", backup_id="../backup")

    def test_rollback_request_is_fixed_action_and_safe_backup_only(self) -> None:
        payload = {
            "version": 1,
            "action": "rollback",
            "receipt_id": "wc_deploy_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "receipt_revision": 2,
            "requested_at_ms": 10_000,
            "execute_after_ms": 1500,
            "backup_id": "backup-source-r4",
        }
        wrapped = envelope(payload, TOKEN)
        parsed = parse_rollback_request(wrapped, TOKEN, now_ms=10_500)
        self.assertIsInstance(parsed, RollbackRequest)
        self.assertEqual(parsed.backup_id, "backup-source-r4")

        path_like = dict(payload)
        path_like["backup_id"] = "../backup"
        with self.assertRaises(SupervisorProtocolError):
            parse_rollback_request(envelope(path_like, TOKEN), TOKEN, now_ms=10_500)

        injected = dict(payload)
        injected["argv"] = ["cmd.exe"]
        with self.assertRaises(SupervisorProtocolError):
            parse_rollback_request(envelope(injected, TOKEN), TOKEN, now_ms=10_500)

    def test_generic_reader_routes_rollback_and_result_is_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = {
                "version": 1,
                "action": "rollback",
                "receipt_id": "wc_deploy_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "receipt_revision": 2,
                "requested_at_ms": 10_000,
                "execute_after_ms": 1500,
                "backup_id": "backup-source-r4",
            }
            atomic_write_json(root / "request.json", envelope(payload, TOKEN))
            request = read_supervisor_request(root, TOKEN, now_ms=10_500)
            self.assertIsInstance(request, RollbackRequest)
            assert isinstance(request, RollbackRequest)
            result = result_envelope(
                request,
                TOKEN,
                status="succeeded",
                backup_id=request.backup_id,
                completed_at_ms=12_000,
            )
            decoded = json.loads(result["payload"])
            self.assertEqual(decoded["action"], "rollback")
            self.assertEqual(decoded["backup_id"], "backup-source-r4")
            with self.assertRaises(SupervisorProtocolError):
                result_envelope(request, TOKEN, status="rolled_back", backup_id=request.backup_id)

    def test_restart_envelope_round_trip_and_tamper_rejection(self) -> None:
        wrapped = envelope(self.request_payload(), TOKEN)
        parsed = parse_restart_request(wrapped, TOKEN, now_ms=10_500)
        self.assertEqual(parsed.receipt_revision, 3)
        self.assertEqual(parsed.execute_after_ms, 1500)

        tampered = dict(wrapped)
        payload = json.loads(tampered["payload"])
        payload["receipt_revision"] = 4
        tampered["payload"] = json.dumps(payload, separators=(",", ":"), sort_keys=True)
        with self.assertRaises(SupervisorProtocolError):
            parse_restart_request(tampered, TOKEN, now_ms=10_500)

    def test_restart_request_is_closed_vocabulary_and_fresh(self) -> None:
        payload = self.request_payload()
        payload["argv"] = ["cmd.exe"]
        with self.assertRaises(SupervisorProtocolError):
            parse_restart_request(envelope(payload, TOKEN), TOKEN, now_ms=10_500)

        stale = self.request_payload()
        with self.assertRaises(SupervisorProtocolError):
            parse_restart_request(envelope(stale, TOKEN), TOKEN, now_ms=80_001)

    def test_atomic_request_and_signed_result_are_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            request = envelope(self.request_payload(), TOKEN)
            atomic_write_json(root / "request.json", request)
            parsed = read_restart_request(root, TOKEN, now_ms=10_500)
            self.assertIsNotNone(parsed)
            assert parsed is not None
            result = result_envelope(parsed, TOKEN, status="succeeded", completed_at_ms=12_000)
            path = result_path(root, parsed.receipt_id)
            atomic_write_json(path, result)
            self.assertTrue(path.is_file())

    def test_supervisor_token_persists_across_parent_epochs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = load_or_create_token(root)
            second = load_or_create_token(root)
            self.assertEqual(first, second)
            self.assertEqual(len(first), 64)
            self.assertTrue((root / "supervisor.key").is_file())

    def test_parent_holds_request_lock_until_signed_result_is_written(self) -> None:
        from scripts.webpi import standalone

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = self.request_payload()
            payload["requested_at_ms"] = int(time.time() * 1000)
            atomic_write_json(root / "request.json", envelope(payload, TOKEN))
            (root / "request.lock").write_text("locked", encoding="ascii")

            request = standalone._consume_restart_request(root, TOKEN)
            self.assertIsNotNone(request)
            self.assertFalse((root / "request.json").exists())
            self.assertTrue((root / "request.lock").exists())
            assert request is not None
            standalone._write_supervisor_result(
                root,
                TOKEN,
                request,
                status="succeeded",
            )
            self.assertFalse((root / "request.lock").exists())
            self.assertTrue(result_path(root, request.receipt_id).is_file())

    def test_deployment_snapshot_is_verified_before_cutover_and_backup_is_reusable(self) -> None:
        from scripts.webpi import standalone

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_root = root / "candidates"
            candidate_dir = candidate_root / "candidate-1" / "dogfood"
            installed = root / "installed"
            staging = root / "staging"
            backups = root / "backups"
            candidate_dir.mkdir(parents=True)
            installed.mkdir()

            artifacts: list[DeployArtifact] = []
            for index, name in enumerate(("webpi.exe", "webpi-server.exe", "webpi-runner.exe"), start=1):
                new_bytes = (f"new-{name}-" * index).encode()
                old_bytes = (f"old-{name}-" * index).encode()
                (candidate_dir / name).write_bytes(new_bytes)
                (installed / name).write_bytes(old_bytes)
                artifacts.append(
                    DeployArtifact(name, __import__("hashlib").sha256(new_bytes).hexdigest(), len(new_bytes))
                )
            request = DeployRequest(
                "wc_deploy_1234567890abcdef1234567890abcdef",
                4,
                int(time.time() * 1000),
                1500,
                "candidate-1",
                tuple(artifacts),
            )
            prepared = standalone._prepare_deployment_snapshots(
                request,
                candidate_root=candidate_root,
                staging_root=staging,
                backup_root=backups,
                installed_dir=installed,
            )
            self.assertTrue(prepared.staged_dir.is_dir())
            self.assertTrue(prepared.backup_dir.is_dir())
            self.assertTrue((prepared.backup_dir / "manifest.json").is_file())
            manifest = json.loads((prepared.backup_dir / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["version"], 2)
            self.assertEqual(manifest["restorable_components"], ["runtime_binaries"])
            self.assertEqual(
                manifest["consistency_fences"],
                ["configuration", "database_schema", "plugin"],
            )
            self.assertEqual(manifest["consistency"]["database_schema"]["schema_version"], 7)
            self.assertEqual(manifest["artifacts"][0]["build"]["git_commit"], "a" * 40)
            old_snapshot = {
                name: (prepared.backup_dir / name).read_bytes()
                for name in standalone.DEPLOY_ARTIFACT_NAMES
            }

            # The immutable staged snapshot no longer depends on the mutable candidate.
            (candidate_dir / "webpi.exe").write_bytes(b"tampered-after-stage")
            self.assertNotEqual(
                (prepared.staged_dir / "webpi.exe").read_bytes(),
                (candidate_dir / "webpi.exe").read_bytes(),
            )

            # Replaying preparation with a restored exact candidate reuses the same
            # receipt-bound backup instead of replacing it with the current install.
            expected = artifacts[0]
            original_new = ("new-webpi.exe-" * 1).encode()
            (candidate_dir / "webpi.exe").write_bytes(original_new)
            self.assertEqual(__import__("hashlib").sha256(original_new).hexdigest(), expected.sha256)
            replay = standalone._prepare_deployment_snapshots(
                request,
                candidate_root=candidate_root,
                staging_root=staging,
                backup_root=backups,
                installed_dir=installed,
            )
            self.assertEqual(replay.backup_id, prepared.backup_id)
            for name, content in old_snapshot.items():
                self.assertEqual((replay.backup_dir / name).read_bytes(), content)

    def test_partial_install_can_be_fully_restored_from_receipt_bound_backup(self) -> None:
        from scripts.webpi import standalone

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_root = root / "candidates"
            candidate_dir = candidate_root / "candidate-2" / "dogfood"
            installed = root / "installed"
            staging = root / "staging"
            backups = root / "backups"
            candidate_dir.mkdir(parents=True)
            installed.mkdir()
            artifacts: list[DeployArtifact] = []
            old_bytes: dict[str, bytes] = {}
            for index, name in enumerate(standalone.DEPLOY_ARTIFACT_NAMES, start=1):
                new = (f"candidate-{name}-" * index).encode()
                old = (f"installed-{name}-" * index).encode()
                (candidate_dir / name).write_bytes(new)
                (installed / name).write_bytes(old)
                old_bytes[name] = old
                artifacts.append(
                    DeployArtifact(name, __import__("hashlib").sha256(new).hexdigest(), len(new))
                )
            request = DeployRequest(
                "wc_deploy_fedcbafedcbafedcbafedcbafedcbafe",
                6,
                int(time.time() * 1000),
                1500,
                "candidate-2",
                tuple(artifacts),
            )
            prepared = standalone._prepare_deployment_snapshots(
                request,
                candidate_root=candidate_root,
                staging_root=staging,
                backup_root=backups,
                installed_dir=installed,
            )
            original_replace = standalone._atomic_replace_runtime_file
            calls = 0

            def fail_second(source: Path, destination: Path) -> None:
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise OSError("simulated replacement failure")
                original_replace(source, destination)

            with patch("scripts.webpi.standalone._atomic_replace_runtime_file", side_effect=fail_second):
                with self.assertRaisesRegex(OSError, "simulated replacement failure"):
                    standalone._install_prepared_deployment(
                        request,
                        prepared,
                        installed_dir=installed,
                    )

            standalone._restore_deployment_backup(
                request,
                prepared,
                installed_dir=installed,
            )
            for name, content in old_bytes.items():
                self.assertEqual((installed / name).read_bytes(), content)

    def test_deployment_snapshot_hash_mismatch_fails_before_backup(self) -> None:
        from scripts.webpi import standalone

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate_root = root / "candidates"
            candidate_dir = candidate_root / "candidate-1" / "dogfood"
            installed = root / "installed"
            staging = root / "staging"
            backups = root / "backups"
            candidate_dir.mkdir(parents=True)
            installed.mkdir()
            artifacts: list[DeployArtifact] = []
            for name in standalone.DEPLOY_ARTIFACT_NAMES:
                (candidate_dir / name).write_bytes(b"candidate")
                (installed / name).write_bytes(b"installed")
                artifacts.append(DeployArtifact(name, "0" * 64, len(b"candidate")))
            request = DeployRequest(
                "wc_deploy_abcdefabcdefabcdefabcdefabcdefab",
                2,
                int(time.time() * 1000),
                1500,
                "candidate-1",
                tuple(artifacts),
            )
            with self.assertRaisesRegex(RuntimeError, "hash mismatch"):
                standalone._prepare_deployment_snapshots(
                    request,
                    candidate_root=candidate_root,
                    staging_root=staging,
                    backup_root=backups,
                    installed_dir=installed,
                )
            self.assertFalse(any(backups.iterdir()) if backups.exists() else False)

    def test_legacy_v1_backup_is_rejected_for_complete_rollback(self) -> None:
        from scripts.webpi import standalone

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            backups = root / "backups"
            backup = backups / "backup-legacy"
            backup.mkdir(parents=True)
            (backup / "manifest.json").write_text(
                json.dumps({
                    "version": 1,
                    "receipt_id": "wc_deploy_11111111111111111111111111111111",
                    "receipt_revision": 1,
                    "backup_id": "backup-legacy",
                    "artifacts": [],
                }),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(RuntimeError, "legacy WebPi rollback manifest"):
                standalone._load_verified_backup_by_id(
                    "backup-legacy",
                    backup_root=backups,
                )

    def test_shadow_candidate_preflight_is_loopback_authenticated_and_cleans_up(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            pid = 444

            def poll(self):
                return None

        class FakeContainment:
            def __init__(self) -> None:
                self.process = FakeProcess()

            def popen(self, *_args, **_kwargs):
                return self.process

        with tempfile.TemporaryDirectory() as directory:
            staged = Path(directory) / "staged"
            staged.mkdir()
            for name in ("webpi.exe", "webpi-runner.exe"):
                (staged / name).write_bytes(b"candidate")
            containment = FakeContainment()

            def response(_origin, route, _payload, _authorization=None, _timeout=1.0):
                if route == "/openapi.json":
                    return 200, {"info": {"title": "WebPi GPT Actions"}}, None
                if route == "/api/actions/runtime_status":
                    return 200, {
                        "success": True,
                        "output": {"service": "webpi", "auth_enabled": True},
                    }, None
                raise AssertionError(route)

            with (
                patch(
                    "scripts.webpi.standalone._check_candidate_runner_offline",
                    return_value={
                        "status": "valid",
                        "client_id": standalone.CLIENT_ID,
                        "transport": "websocket",
                        "project_registry_configured": True,
                        "max_concurrent_jobs": 4,
                        "mcp_provider_count": 0,
                        "plugin_provider_count": 1,
                    },
                ),
                patch("scripts.webpi.standalone._allocate_shadow_loopback_port", return_value=61234),
                patch("scripts.webpi.standalone.subprocess.run"),
                patch("scripts.webpi.standalone._bootstrap_key_from_env", return_value="shadow-secret"),
                patch("scripts.webpi.standalone.security_request_json", side_effect=response),
                patch(
                    "scripts.webpi.standalone.require_authenticated_origin",
                    side_effect=[RuntimeError("transient auth probe"), None],
                ) as auth_origin,
                patch("scripts.webpi.standalone.terminate_group") as terminate,
            ):
                result = standalone._shadow_candidate_preflight(
                    containment,
                    0,
                    staged,
                    Path("runner.toml"),
                    attempts=1,
                    timeout_seconds=1,
                )
            self.assertEqual(result["runner_config_valid"], True)
            self.assertEqual(result["server_shadow_healthy"], True)
            self.assertEqual(result["server_auth_verified"], True)
            self.assertEqual(result["loopback_only"], True)
            self.assertEqual(result["attempts"], 1)
            self.assertEqual(auth_origin.call_count, 2)
            auth_origin.assert_called_with("http://127.0.0.1:61234", timeout=1.0)
            terminate.assert_called_once_with(containment.process)
            self.assertFalse((staged / ".shadow-preflight-1").exists())

    def test_shadow_preflight_failure_never_stops_production_children_or_waits_for_cutover(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            def __init__(self, pid: int) -> None:
                self.pid = pid

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staged = root / "staged"
            backup = root / "backup"
            staged.mkdir()
            backup.mkdir()
            request = DeployRequest(
                "wc_deploy_88888888888888888888888888888888",
                4,
                int(time.time() * 1000),
                1500,
                "candidate-shadow-failure",
                tuple(),
            )
            prepared = standalone._PreparedDeployment(
                candidate_id="candidate-shadow-failure",
                staged_dir=staged,
                backup_id="backup-shadow-r4",
                backup_dir=backup,
            )
            old_server, old_runner = FakeProcess(11), FakeProcess(12)
            with (
                patch("scripts.webpi.standalone._prepare_deployment_snapshots", return_value=prepared),
                patch(
                    "scripts.webpi.standalone._shadow_candidate_preflight",
                    side_effect=RuntimeError("shadow failed"),
                ),
                patch("scripts.webpi.standalone._wait_until_supervisor_request_due") as wait_due,
                patch("scripts.webpi.standalone._stop_tunnel") as stop_tunnel,
                patch("scripts.webpi.standalone.terminate_group") as terminate,
                patch("scripts.webpi.standalone._write_supervisor_result") as write_result,
            ):
                server, runner, tunnel = standalone._execute_deploy_request(
                    request,
                    containment=object(),
                    flags=0,
                    config=Path("runner.toml"),
                    control_dir=root / "control",
                    token=TOKEN,
                    server=old_server,
                    runner=old_runner,
                    installed_dir=root / "installed",
                )
            self.assertIs(server, old_server)
            self.assertIs(runner, old_runner)
            self.assertIsNone(tunnel)
            wait_due.assert_not_called()
            stop_tunnel.assert_not_called()
            terminate.assert_not_called()
            self.assertEqual(write_result.call_args.kwargs["status"], "failed")
            self.assertEqual(
                write_result.call_args.kwargs["error_code"],
                "candidate_shadow_preflight_failed",
            )

    def test_deploy_consistency_fence_change_fails_before_stopping_children(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            def __init__(self, pid: int) -> None:
                self.pid = pid

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staged = root / "staged"
            backup = root / "backup"
            staged.mkdir()
            backup.mkdir()
            request = DeployRequest(
                "wc_deploy_99999999999999999999999999999999",
                5,
                int(time.time() * 1000),
                1500,
                "candidate-fence",
                tuple(),
            )
            prepared = standalone._PreparedDeployment(
                candidate_id="candidate-fence",
                staged_dir=staged,
                backup_id="backup-fence-r5",
                backup_dir=backup,
            )
            consistency = standalone._capture_rollback_consistency_identity()
            builds = standalone._capture_runtime_builds(Path("unused"))
            manifest = {
                "version": standalone.ROLLBACK_MANIFEST_VERSION,
                "receipt_id": request.receipt_id,
                "receipt_revision": request.receipt_revision,
                "backup_id": prepared.backup_id,
                "artifacts": [
                    {
                        "name": name,
                        "sha256": str(index) * 64,
                        "size_bytes": index,
                        "build": builds[name],
                    }
                    for index, name in enumerate(standalone.DEPLOY_ARTIFACT_NAMES, start=1)
                ],
                "consistency": consistency,
                "restorable_components": ["runtime_binaries"],
                "consistency_fences": ["configuration", "database_schema", "plugin"],
            }
            (backup / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
            changed = json.loads(json.dumps(consistency))
            changed["database_schema"]["schema_sha256"] = "7" * 64
            old_server, old_runner = FakeProcess(1), FakeProcess(2)
            with (
                patch("scripts.webpi.standalone._prepare_deployment_snapshots", return_value=prepared),
                patch(
                    "scripts.webpi.standalone._shadow_candidate_preflight",
                    return_value={
                        "runner_config_valid": True,
                        "server_shadow_healthy": True,
                        "server_auth_verified": True,
                        "loopback_only": True,
                        "attempts": 1,
                    },
                ),
                patch("scripts.webpi.standalone._wait_until_supervisor_request_due"),
                patch(
                    "scripts.webpi.standalone._capture_rollback_consistency_identity",
                    return_value=changed,
                ),
                patch("scripts.webpi.standalone._stop_tunnel") as stop_tunnel,
                patch("scripts.webpi.standalone.terminate_group") as terminate,
                patch("scripts.webpi.standalone._write_supervisor_result") as write_result,
            ):
                server, runner, tunnel = standalone._execute_deploy_request(
                    request,
                    containment=object(),
                    flags=0,
                    config=Path("runner.toml"),
                    control_dir=root / "control",
                    token=TOKEN,
                    server=old_server,
                    runner=old_runner,
                    installed_dir=root / "installed",
                )
            self.assertIs(server, old_server)
            self.assertIs(runner, old_runner)
            self.assertIsNone(tunnel)
            stop_tunnel.assert_not_called()
            terminate.assert_not_called()
            self.assertEqual(write_result.call_args.kwargs["status"], "failed")
            self.assertEqual(
                write_result.call_args.kwargs["error_code"],
                "consistency_fence_changed",
            )

    def test_rollback_restores_verified_target_and_keeps_reversible_safety_backup(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            def __init__(self, pid: int) -> None:
                self.pid = pid

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = root / "installed"
            backups = root / "backups"
            installed.mkdir()
            target_bytes: dict[str, bytes] = {}
            current_bytes: dict[str, bytes] = {}
            for index, name in enumerate(standalone.DEPLOY_ARTIFACT_NAMES, start=1):
                target = (f"target-{name}-" * index).encode()
                current = (f"current-{name}-" * index).encode()
                target_bytes[name] = target
                current_bytes[name] = current
                (installed / name).write_bytes(target)
            source_request = DeployRequest(
                "wc_deploy_11111111111111111111111111111111",
                4,
                int(time.time() * 1000),
                1500,
                "unused",
                tuple(),
            )
            target_backup_id, _ = standalone._create_receipt_runtime_backup(
                source_request,
                backup_root=backups,
                installed_dir=installed,
            )
            for name, content in current_bytes.items():
                (installed / name).write_bytes(content)

            request = RollbackRequest(
                "wc_deploy_22222222222222222222222222222222",
                2,
                int(time.time() * 1000),
                1500,
                target_backup_id,
            )
            old_server, old_runner = FakeProcess(1), FakeProcess(2)
            new_server, new_runner = FakeProcess(3), FakeProcess(4)
            with (
                patch("scripts.webpi.standalone._wait_until_supervisor_request_due"),
                patch("scripts.webpi.standalone._stop_tunnel"),
                patch("scripts.webpi.standalone.terminate_group"),
                patch(
                    "scripts.webpi.standalone._start_supervised_children",
                    return_value=(new_server, new_runner, None),
                ),
                patch("scripts.webpi.standalone._wait_for_post_cutover_health"),
                patch("scripts.webpi.standalone._write_supervisor_result") as write_result,
            ):
                server, runner, tunnel = standalone._execute_rollback_request(
                    request,
                    containment=object(),
                    flags=0,
                    config=Path("runner.toml"),
                    control_dir=root / "control",
                    token=TOKEN,
                    server=old_server,
                    runner=old_runner,
                    backup_root=backups,
                    installed_dir=installed,
                )

            self.assertIs(server, new_server)
            self.assertIs(runner, new_runner)
            self.assertIsNone(tunnel)
            for name, content in target_bytes.items():
                self.assertEqual((installed / name).read_bytes(), content)
            kwargs = write_result.call_args.kwargs
            self.assertEqual(kwargs["status"], "succeeded")
            safety_backup_id = kwargs["backup_id"]
            self.assertNotEqual(safety_backup_id, target_backup_id)
            for name, content in current_bytes.items():
                self.assertEqual((backups / safety_backup_id / name).read_bytes(), content)

    def test_rollback_invalid_target_fails_before_stopping_children(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            def __init__(self, pid: int) -> None:
                self.pid = pid

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            backups = root / "backups"
            installed = root / "installed"
            backups.mkdir()
            installed.mkdir()
            request = RollbackRequest(
                "wc_deploy_33333333333333333333333333333333",
                2,
                int(time.time() * 1000),
                1500,
                "backup-does-not-exist",
            )
            old_server, old_runner = FakeProcess(1), FakeProcess(2)
            with (
                patch("scripts.webpi.standalone.terminate_group") as terminate,
                patch("scripts.webpi.standalone._write_supervisor_result") as write_result,
            ):
                server, runner, tunnel = standalone._execute_rollback_request(
                    request,
                    containment=object(),
                    flags=0,
                    config=Path("runner.toml"),
                    control_dir=root / "control",
                    token=TOKEN,
                    server=old_server,
                    runner=old_runner,
                    backup_root=backups,
                    installed_dir=installed,
                )
            terminate.assert_not_called()
            self.assertIs(server, old_server)
            self.assertIs(runner, old_runner)
            self.assertIsNone(tunnel)
            self.assertEqual(write_result.call_args.kwargs["status"], "failed")
            self.assertEqual(
                write_result.call_args.kwargs["error_code"],
                "rollback_backup_validation_failed",
            )

    def test_rollback_health_failure_restores_pre_rollback_safety_backup(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            def __init__(self, pid: int) -> None:
                self.pid = pid

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = root / "installed"
            backups = root / "backups"
            installed.mkdir()
            current_bytes: dict[str, bytes] = {}
            for index, name in enumerate(standalone.DEPLOY_ARTIFACT_NAMES, start=1):
                (installed / name).write_bytes((f"target-{name}-" * index).encode())
            target_request = DeployRequest(
                "wc_deploy_44444444444444444444444444444444",
                3,
                int(time.time() * 1000),
                1500,
                "unused",
                tuple(),
            )
            target_backup_id, _ = standalone._create_receipt_runtime_backup(
                target_request,
                backup_root=backups,
                installed_dir=installed,
            )
            for index, name in enumerate(standalone.DEPLOY_ARTIFACT_NAMES, start=1):
                current = (f"current-{name}-" * index).encode()
                current_bytes[name] = current
                (installed / name).write_bytes(current)
            request = RollbackRequest(
                "wc_deploy_55555555555555555555555555555555",
                2,
                int(time.time() * 1000),
                1500,
                target_backup_id,
            )
            old_server, old_runner = FakeProcess(1), FakeProcess(2)
            recovered_server, recovered_runner = FakeProcess(5), FakeProcess(6)
            with (
                patch("scripts.webpi.standalone._wait_until_supervisor_request_due"),
                patch("scripts.webpi.standalone._stop_tunnel"),
                patch("scripts.webpi.standalone.terminate_group"),
                patch(
                    "scripts.webpi.standalone._start_supervised_children",
                    return_value=(recovered_server, recovered_runner, None),
                ),
                patch(
                    "scripts.webpi.standalone._wait_for_post_cutover_health",
                    side_effect=[RuntimeError("target health failed"), None],
                ),
                patch("scripts.webpi.standalone._write_supervisor_result") as write_result,
            ):
                server, runner, _ = standalone._execute_rollback_request(
                    request,
                    containment=object(),
                    flags=0,
                    config=Path("runner.toml"),
                    control_dir=root / "control",
                    token=TOKEN,
                    server=old_server,
                    runner=old_runner,
                    backup_root=backups,
                    installed_dir=installed,
                )
            self.assertIs(server, recovered_server)
            self.assertIs(runner, recovered_runner)
            for name, content in current_bytes.items():
                self.assertEqual((installed / name).read_bytes(), content)
            self.assertEqual(write_result.call_args.kwargs["status"], "failed")
            self.assertEqual(
                write_result.call_args.kwargs["error_code"],
                "rollback_target_failed_recovered",
            )
            self.assertIn("backup_id", write_result.call_args.kwargs)

    def test_restart_core_children_replaces_managed_processes_only(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            def __init__(self, pid: int) -> None:
                self.pid = pid

        old_server = FakeProcess(1)
        old_runner = FakeProcess(2)
        new_server = FakeProcess(3)
        new_runner = FakeProcess(4)
        terminated: list[object] = []

        with (
            patch("scripts.webpi.standalone._stop_tunnel") as stop_tunnel,
            patch("scripts.webpi.standalone.terminate_group", side_effect=terminated.append),
            patch(
                "scripts.webpi.standalone._start_supervised_children",
                return_value=(new_server, new_runner, None),
            ) as start_children,
        ):
            server, runner, tunnel = standalone._restart_core_children(
                object(),
                0,
                Path("runner.toml"),
                Path("control"),
                TOKEN,
                old_server,
                old_runner,
            )

        stop_tunnel.assert_called_once_with(None)
        self.assertEqual(terminated, [old_runner, old_server])
        start_children.assert_called_once()
        self.assertIs(server, new_server)
        self.assertIs(runner, new_runner)
        self.assertIsNone(tunnel)

    def test_partial_supervised_start_is_cleaned_before_recovery(self) -> None:
        from scripts.webpi import standalone

        class FakeProcess:
            pid = 123

        server = FakeProcess()
        terminated: list[object] = []
        with (
            patch("scripts.webpi.standalone._spawn_supervised_server", return_value=server),
            patch(
                "scripts.webpi.standalone._spawn_supervised_runner",
                side_effect=RuntimeError("runner failed"),
            ),
            patch("scripts.webpi.standalone.terminate_group", side_effect=terminated.append),
        ):
            with self.assertRaisesRegex(RuntimeError, "runner failed"):
                standalone._start_supervised_children(
                    object(),
                    0,
                    Path("runner.toml"),
                    Path("control"),
                    TOKEN,
                )
        self.assertEqual(terminated, [server])

    def test_result_rejects_unbounded_error_text(self) -> None:
        parsed = parse_restart_request(envelope(self.request_payload(), TOKEN), TOKEN, now_ms=10_500)
        with self.assertRaises(SupervisorProtocolError):
            result_envelope(parsed, TOKEN, status="failed", error_code="secret value with spaces")


if __name__ == "__main__":
    unittest.main()
