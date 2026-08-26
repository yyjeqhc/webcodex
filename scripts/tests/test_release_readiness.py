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
            state_path.unlink()
            target = root / "target.json"
            target.write_text("{}\n", encoding="utf-8")
            state_path.symlink_to(target)
            with self.assertRaises(readiness.ReadinessError):
                readiness._load_state(state_path)


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
        readiness._post_dispatch(client, SOURCE, REQUEST)

    def test_4xx_is_definite_rejection(self) -> None:
        client = self._client()
        error = urllib.error.HTTPError("https://api.github.test", 422, "bad", {}, None)
        client.opener = _Opener(error)
        with self.assertRaises(readiness.DispatchRejected):
            readiness._post_dispatch(client, SOURCE, REQUEST)

    def test_transport_failure_is_outcome_unknown(self) -> None:
        client = self._client()
        client.opener = _Opener(urllib.error.URLError("lost"))
        with self.assertRaises(readiness.DispatchOutcomeUnknown):
            readiness._post_dispatch(client, SOURCE, REQUEST)


class WorkflowContractTests(unittest.TestCase):
    def test_pre_tag_gate_covers_every_release_platform_without_uploads(self) -> None:
        workflow = Path(".github/workflows/release-readiness.yml").read_text(encoding="utf-8")
        for platform in ("linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64", "win32-arm64"):
            self.assertIn(platform, workflow)
        self.assertIn("runner: macos-15-intel", workflow)
        self.assertIn("rust_host: x86_64-apple-darwin", workflow)
        self.assertNotIn('rustc -vV | grep -Fxq "host:', workflow)
        self.assertNotIn('file "$binary" | grep -Fq "$EXPECTED_FILE_ARCH"', workflow)
        self.assertGreaterEqual(workflow.count("cargo build --locked --release"), 3)
        self.assertIn("quay.io/pypa/manylinux2014_x86_64", workflow)
        self.assertIn("quay.io/pypa/manylinux2014_aarch64", workflow)
        self.assertIn("npm_install_windows_smoke.ps1", workflow)
        self.assertIn("server-image:", workflow)
        self.assertIn("platform: linux/amd64", workflow)
        self.assertIn("platform: linux/arm64", workflow)
        self.assertIn("DOCKER_BUILDKIT=1 docker build", workflow)
        self.assertIn("scripts/prepare_server_deployment_assets.py", workflow)
        self.assertIn("ghcr.io/yyjeqhc/webcodex-server@$digest", workflow)
        self.assertNotIn("actions/upload-artifact", workflow)
        self.assertNotIn("docker/login-action", workflow)
        self.assertNotIn("packages: write", workflow)
        self.assertNotIn("push: true", workflow)

    def test_expensive_readiness_fanout_waits_for_fail_fast_correctness(self) -> None:
        workflow = Path(".github/workflows/release-readiness.yml").read_text(encoding="utf-8")
        self.assertIn("  release-contract:\n", workflow)
        self.assertIn("  core-tests:\n", workflow)
        self.assertIn("      fail-fast: true\n", workflow)
        self.assertIn('            packages: "-p webcodex"', workflow)
        self.assertIn('            packages: "-p webcodex-runner"', workflow)
        for package in (
            "webcodex-admin",
            "webcodex-agent-config",
            "webcodex-core",
            "webcodex-cli",
            "webcodex-persistent-shell",
            "webcodex-sandbox",
            "webcodex-workspace",
            "webcodex-process",
        ):
            self.assertIn(f"              -p {package}", workflow)
        self.assertIn(
            "run: cargo test --locked ${{ matrix.packages }} -- --nocapture",
            workflow,
        )
        self.assertNotIn("cargo test --locked --workspace -- --nocapture", workflow)

        build_gate_dependency = (
            "    needs: [release-contract, core-tests, frontend, e2e, eval, macos-tests]\n"
        )
        # Every expensive native/image build waits for the complete test gate,
        # including both native macOS Runner suites. Test jobs themselves have
        # no upstream build gate and therefore fan out immediately.
        self.assertEqual(workflow.count(build_gate_dependency), 4)
        for test_job in ("frontend", "e2e", "eval", "macos-tests"):
            start = workflow.index(f"  {test_job}:\n")
            end = workflow.find("\n  ", start + 1)
            block = workflow[start:] if end == -1 else workflow[start:end]
            self.assertNotIn("    needs:", block)
        macos_tests_start = workflow.index("  macos-tests:\n")
        macos_tests_end = workflow.index("  native-linux:\n", macos_tests_start)
        macos_tests = workflow[macos_tests_start:macos_tests_end]
        native_macos_start = workflow.index("  native-macos:\n")
        self.assertIn(
            "run: cargo test --locked -p webcodex-runner -- --nocapture",
            macos_tests,
        )
        self.assertNotIn("cargo build --locked --release", macos_tests)
        native_macos_end = workflow.index("  native-windows:\n", native_macos_start)
        native_macos = workflow[native_macos_start:native_macos_end]
        self.assertIn("cargo build --locked --release", native_macos)
        self.assertNotIn("cargo test --locked -p webcodex-runner", native_macos)
        self.assertIn(
            "needs: [release-contract, core-tests, frontend, e2e, eval, macos-tests, native-linux, server-image, native-macos, native-windows]",
            workflow,
        )
        self.assertNotIn("macos-runner", workflow)

    def test_owner_prs_run_complete_linux_ci_before_merge(self) -> None:
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")

        def job_block(name: str, next_name: str) -> str:
            start = workflow.index(f"  {name}:\n")
            end = workflow.index(f"  {next_name}:\n", start)
            return workflow[start:end]

        linux_rust = job_block("test-linux-rust", "test-linux-tooling")
        linux_tooling = job_block("test-linux-tooling", "test")
        aggregate = job_block("test", "test-macos")
        self.assertNotIn("pull_request.user.login", linux_rust)
        self.assertNotIn("contains(github.event.pull_request.labels.*.name, 'run-ci')", linux_rust)
        self.assertNotIn("pull_request.user.login", linux_tooling)
        self.assertNotIn("contains(github.event.pull_request.labels.*.name, 'run-ci')", linux_tooling)
        macos = job_block("test-macos", "test-windows-core")
        self.assertIn("platform: darwin-x64", macos)
        self.assertIn("runner: macos-15-intel", macos)
        self.assertIn("rust_host: x86_64-apple-darwin", macos)
        self.assertIn("contains(github.event.pull_request.labels.*.name, 'run-ci')", macos)
        self.assertIn("if: always()", aggregate)

    def test_macos_ci_host_check_does_not_use_quiet_grep_under_pipefail(self) -> None:
        workflow = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertNotIn('rustc -vV | grep -Fxq "host:', workflow)
        self.assertIn("rust_host=\"$(rustc -vV | sed -n 's/^host: //p')\"", workflow)


if __name__ == "__main__":
    unittest.main()
