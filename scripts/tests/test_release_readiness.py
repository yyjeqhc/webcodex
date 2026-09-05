from __future__ import annotations

import tempfile
import unittest
import urllib.error
from pathlib import Path

from scripts import collect_release_bundle as collector
from scripts import release_readiness as readiness


SOURCE = "a" * 40
REQUEST = "rr_" + "b" * 24


def _state() -> dict:
    return {
        "schema_version": readiness.STATE_SCHEMA_VERSION,
        "kind": "release-readiness",
        "repo": collector.DEFAULT_REPO,
        "source_sha": SOURCE,
        "workflow_file": readiness.READINESS_WORKFLOW_FILE,
        "workflow_path": readiness.READINESS_WORKFLOW_PATH,
        "request_id": REQUEST,
        "run_name": readiness._run_name(REQUEST, SOURCE),
        "dispatch_state": "dispatched",
        "created_at": 1234567890,
        "ci_run_id": 777,
        "ci_run_attempt": 2,
        "ci_run_url": "https://github.com/yyjeqhc/webcodex/actions/runs/777",
        "ci_run_head_sha": SOURCE,
        "ci_run_conclusion": "success",
        "run_id": None,
        "run_head_sha": None,
        "source_matches": None,
        "run_url": None,
        "run_status": None,
        "run_conclusion": None,
        "last_observed_at": None,
    }


def _run(run_id: int = 123) -> dict:
    return {
        "id": run_id,
        "path": readiness.READINESS_WORKFLOW_PATH,
        "event": "workflow_dispatch",
        "head_branch": "main",
        "head_sha": SOURCE,
        "display_title": readiness._run_name(REQUEST, SOURCE),
        "html_url": f"https://github.com/yyjeqhc/webcodex/actions/runs/{run_id}",
        "status": "in_progress",
        "conclusion": None,
    }


def _ci_run(run_id: int = 777, *, attempt: int = 2, source: str = SOURCE, conclusion: str = "success") -> dict:
    return {
        "id": run_id,
        "run_attempt": attempt,
        "path": readiness.CI_WORKFLOW_PATH,
        "event": "push",
        "head_branch": "main",
        "head_sha": source,
        "html_url": f"https://github.com/yyjeqhc/webcodex/actions/runs/{run_id}",
        "status": "completed",
        "conclusion": conclusion,
    }


class ReadinessSelectionTests(unittest.TestCase):
    def test_selects_exact_request_source_and_workflow(self) -> None:
        payload = {"workflow_runs": [_run(), dict(_run(456), display_title="other")]}
        selected = readiness.select_readiness_run(payload, _state())
        self.assertIsNotNone(selected)
        assert selected is not None
        self.assertEqual(selected["id"], 123)

    def test_duplicate_exact_runs_fail_closed(self) -> None:
        payload = {"workflow_runs": [_run(1), _run(2)]}
        with self.assertRaises(readiness.ReadinessError):
            readiness.select_readiness_run(payload, _state())

    def test_wrong_run_head_still_resolves_by_request_identity(self) -> None:
        payload = {"workflow_runs": [dict(_run(), head_sha="c" * 40)]}
        selected = readiness.select_readiness_run(payload, _state())
        self.assertIsNotNone(selected)
        state = _state()
        assert selected is not None
        readiness._apply_run_snapshot(state, selected)
        self.assertFalse(state["source_matches"])
        self.assertEqual(state["run_head_sha"], "c" * 40)


class MainCiProofTests(unittest.TestCase):
    def test_selects_exact_successful_main_push_ci(self) -> None:
        selected = readiness.select_successful_main_ci_run({"workflow_runs": [_ci_run()]}, SOURCE)
        self.assertEqual(selected["id"], 777)
        self.assertEqual(selected["run_attempt"], 2)

    def test_main_ci_proof_fails_closed_on_failure_wrong_source_or_duplicate(self) -> None:
        with self.assertRaises(readiness.ReadinessError):
            readiness.select_successful_main_ci_run({"workflow_runs": [_ci_run(conclusion="failure")]}, SOURCE)
        with self.assertRaises(readiness.ReadinessError):
            readiness.select_successful_main_ci_run({"workflow_runs": [_ci_run(source="c" * 40)]}, SOURCE)
        with self.assertRaises(readiness.ReadinessError):
            readiness.select_successful_main_ci_run({"workflow_runs": [_ci_run(1), _ci_run(2)]}, SOURCE)


class SnapshotFenceTests(unittest.TestCase):
    def test_bound_run_id_cannot_change(self) -> None:
        state = _state()
        state["run_id"] = 123
        with self.assertRaises(readiness.ReadinessError):
            readiness._apply_run_snapshot(state, _run(456))

    def test_terminal_snapshot_records_conclusion(self) -> None:
        state = _state()
        run = dict(_run(), status="completed", conclusion="success")
        readiness._apply_run_snapshot(state, run)
        self.assertEqual(state["dispatch_state"], "completed")
        self.assertEqual(state["run_status"], "completed")
        self.assertEqual(state["run_conclusion"], "success")
        self.assertTrue(state["source_matches"])
        self.assertEqual(state["run_head_sha"], SOURCE)


class ReadinessStateTests(unittest.TestCase):
    def test_state_round_trip_and_symlink_rejection(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            state_path = root / "readiness.json"
            readiness._write_state(state_path, _state())
            loaded = readiness._load_state(state_path)
            self.assertEqual(loaded["request_id"], REQUEST)
            self.assertEqual(loaded["ci_run_id"], 777)
            self.assertEqual(loaded["ci_run_attempt"], 2)
            state_path.unlink()
            target = root / "target.json"
            target.write_text("{}\n", encoding="utf-8")
            state_path.symlink_to(target)
            with self.assertRaises(readiness.ReadinessError):
                readiness._load_state(state_path)


    def test_legacy_v1_state_remains_readable(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            state = _state()
            state["schema_version"] = readiness.LEGACY_STATE_SCHEMA_VERSION
            for field in ("ci_run_id", "ci_run_attempt", "ci_run_url", "ci_run_head_sha", "ci_run_conclusion"):
                state.pop(field)
            path = Path(temp) / "legacy.json"
            readiness._write_state(path, state)
            loaded = readiness._load_state(path)
            self.assertEqual(loaded["schema_version"], readiness.LEGACY_STATE_SCHEMA_VERSION)
            self.assertNotIn("ci_run_id", loaded)


class _Response:
    status = 204

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self, _size=-1):
        return b""

    def getcode(self):
        return self.status


class _Opener:
    def __init__(self, outcome):
        self.outcome = outcome

    def open(self, request, timeout):
        if isinstance(self.outcome, Exception):
            raise self.outcome
        return self.outcome


class DispatchClassificationTests(unittest.TestCase):
    def test_resolved_wrong_source_is_never_a_successful_gate(self) -> None:
        state = _state()
        readiness._apply_run_snapshot(state, dict(_run(), head_sha="c" * 40))
        self.assertFalse(state["source_matches"])

    def _client(self):
        return collector.GitHubClient(collector.DEFAULT_REPO, "fake-token", 5)

    def test_204_is_accepted(self) -> None:
        client = self._client()
        client.opener = _Opener(_Response())
        readiness._post_dispatch(client, SOURCE, REQUEST, 777, 2)

    def test_4xx_is_definite_rejection(self) -> None:
        client = self._client()
        error = urllib.error.HTTPError("https://api.github.test", 422, "bad", {}, None)
        client.opener = _Opener(error)
        with self.assertRaises(readiness.DispatchRejected):
            readiness._post_dispatch(client, SOURCE, REQUEST, 777, 2)

    def test_transport_failure_is_outcome_unknown(self) -> None:
        client = self._client()
        client.opener = _Opener(urllib.error.URLError("lost"))
        with self.assertRaises(readiness.DispatchOutcomeUnknown):
            readiness._post_dispatch(client, SOURCE, REQUEST, 777, 2)


class WorkflowContractTests(unittest.TestCase):
    def test_pre_tag_gate_reuses_exact_main_ci_and_never_builds_native_release_candidates(self) -> None:
        workflow = Path(".github/workflows/release-readiness.yml").read_text(encoding="utf-8")
        self.assertIn("  ci-proof:\n", workflow)
        self.assertIn("actions: read", workflow)
        self.assertIn("ci_run_id:", workflow)
        self.assertIn("ci_run_attempt:", workflow)
        self.assertIn("/actions/runs/{run_id}/attempts/{attempt}", workflow)
        self.assertIn('"path": ".github/workflows/ci.yml"', workflow)
        self.assertIn('"event": "push"', workflow)
        self.assertIn('"conclusion": "success"', workflow)
        self.assertNotIn("cargo build --locked --release", workflow)
        for removed_job in ("release-contract", "core-tests", "frontend", "macos-tests", "native-linux", "native-macos", "native-windows"):
            self.assertNotIn(f"  {removed_job}:\n", workflow)
        self.assertIn("server-image:", workflow)
        self.assertIn("platform: linux/amd64", workflow)
        self.assertIn("platform: linux/arm64", workflow)
        self.assertIn("DOCKER_BUILDKIT=1 docker build", workflow)
        self.assertIn("scripts/prepare_server_deployment_assets.py", workflow)
        self.assertIn("ghcr.io/yyjeqhc/webcodex-server@$digest", workflow)
        self.assertIn("e2e:\n    needs: ci-proof", workflow)
        self.assertIn("eval:\n    needs: ci-proof", workflow)
        self.assertIn("server-image:\n    needs: [ci-proof, e2e, eval]", workflow)
        self.assertIn("summary:\n    needs: [ci-proof, e2e, eval, server-image]", workflow)
        self.assertNotIn("actions/upload-artifact", workflow)
        self.assertNotIn("docker/login-action", workflow)
        self.assertNotIn("packages: write", workflow)
        self.assertNotIn("push: true", workflow)

    def test_owner_prs_run_complete_linux_ci_before_merge(self) -> None:
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        release_build = Path(".github/workflows/release-build.yml").read_text(encoding="utf-8")

        def job_block(name: str, next_name: str) -> str:
            start = workflow.index(f"  {name}:\n")
            end = workflow.index(f"  {next_name}:\n", start)
            return workflow[start:end]

        linux_rust = job_block("test-linux-rust", "test-linux-tooling")
        linux_tooling = job_block("test-linux-tooling", "test-linux-arm64")
        linux_arm64 = job_block("test-linux-arm64", "test")
        aggregate = job_block("test", "test-macos")
        self.assertNotIn("pull_request.user.login", linux_rust)
        self.assertNotIn("contains(github.event.pull_request.labels.*.name, 'run-ci')", linux_rust)
        self.assertNotIn("pull_request.user.login", linux_tooling)
        self.assertNotIn("contains(github.event.pull_request.labels.*.name, 'run-ci')", linux_tooling)
        self.assertIn("runs-on: ubuntu-24.04-arm", linux_arm64)
        self.assertIn("cargo check --locked -p webcodex -p webcodex-cli -p webcodex-runner", linux_arm64)
        self.assertIn("contains(github.event.pull_request.labels.*.name, 'run-ci')", linux_arm64)
        self.assertIn("cargo check --locked --workspace --all-targets", linux_tooling)
        self.assertIn("bash scripts/release_check.sh --static-only", linux_tooling)
        macos = job_block("test-macos", "test-windows-core")
        self.assertIn("platform: darwin-x64", macos)
        self.assertIn("runner: macos-15-intel", macos)
        self.assertIn("rust_host: x86_64-apple-darwin", macos)
        self.assertIn("contains(github.event.pull_request.labels.*.name, 'run-ci')", macos)
        self.assertIn("cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner", macos)
        self.assertIn("--bin-dir target/dogfood", macos)
        windows_desktop = job_block("test-windows-desktop", "test-windows-arm64")
        self.assertIn("cargo build --locked --profile dogfood -p webcodex -p webcodex-cli -p webcodex-runner", windows_desktop)
        self.assertIn('Join-Path "target\\dogfood"', windows_desktop)
        windows_arm64 = job_block("test-windows-arm64", "test-windows")
        self.assertIn("runs-on: windows-11-arm", windows_arm64)
        self.assertIn("aarch64-pc-windows-msvc", windows_arm64)
        self.assertIn("cargo check --locked -p webcodex -p webcodex-cli -p webcodex-runner", windows_arm64)
        windows_aggregate = job_block("test-windows", "test-native")
        native_aggregate = workflow[workflow.index("  test-native:\n"):]
        for required_aggregate in (aggregate, windows_aggregate, native_aggregate):
            self.assertIn("if: always()", required_aggregate)
        self.assertIn("FULL_NATIVE_REQUESTED", windows_aggregate)
        self.assertIn("requested Windows CI lane failed or was unexpectedly skipped", windows_aggregate)
        self.assertIn("fast owner PR expected every heavy Windows lane to be intentionally skipped", windows_aggregate)
        self.assertIn("FULL_NATIVE_REQUESTED", native_aggregate)
        self.assertIn("requested native CI lane failed or was unexpectedly skipped", native_aggregate)
        self.assertIn("fast owner PR native lane shape was not the intentional policy skip", native_aggregate)
        self.assertIn("cargo build --locked --release -p webcodex -p webcodex-cli -p webcodex-runner", release_build)

    def test_ci_native_policy_truth_table(self) -> None:
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        expression = "FULL_NATIVE_REQUESTED: ${{ github.event_name == 'push' || github.event.pull_request.user.login != github.repository_owner || contains(github.event.pull_request.labels.*.name, 'run-ci') }}"
        self.assertIn(expression, workflow)

        def full_native_requested(event_name: str, actor: str, owner: str, labels: set[str]) -> bool:
            return event_name == "push" or actor != owner or "run-ci" in labels

        scenarios = (
            ("owner PR without run-ci", "pull_request", "owner", "owner", set(), False),
            ("owner PR with run-ci", "pull_request", "owner", "owner", {"run-ci"}, True),
            ("external PR", "pull_request", "contributor", "owner", set(), True),
            ("main push", "push", "owner", "owner", set(), True),
        )
        for name, event_name, actor, owner, labels, expected in scenarios:
            with self.subTest(name=name):
                self.assertEqual(full_native_requested(event_name, actor, owner, labels), expected)

        def windows_aggregate_ok(full_native: bool, contract: str, lanes: tuple[str, ...]) -> bool:
            expected = "success" if full_native else "skipped"
            return contract == "success" and all(result == expected for result in lanes)

        def native_aggregate_ok(
            full_native: bool,
            contract: str,
            linux_arm64: str,
            macos: str,
            windows: str,
        ) -> bool:
            if contract != "success":
                return False
            if full_native:
                return (linux_arm64, macos, windows) == ("success", "success", "success")
            return (linux_arm64, macos, windows) == ("skipped", "skipped", "success")

        skipped_windows = ("skipped",) * 5
        successful_windows = ("success",) * 5
        self.assertTrue(windows_aggregate_ok(False, "success", skipped_windows))
        self.assertTrue(native_aggregate_ok(False, "success", "skipped", "skipped", "success"))
        self.assertTrue(windows_aggregate_ok(True, "success", successful_windows))
        self.assertTrue(native_aggregate_ok(True, "success", "success", "success", "success"))
        self.assertFalse(windows_aggregate_ok(True, "success", ("skipped",) + successful_windows[1:]))
        self.assertFalse(native_aggregate_ok(True, "success", "success", "skipped", "success"))

    def test_macos_ci_host_check_does_not_use_quiet_grep_under_pipefail(self) -> None:
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertNotIn('rustc -vV | grep -Fxq "host:', workflow)
        self.assertIn("rust_host=\"$(rustc -vV | sed -n 's/^host: //p')\"", workflow)


if __name__ == "__main__":
    unittest.main()
