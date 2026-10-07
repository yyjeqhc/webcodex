from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from scripts import agent_loop_benchmark as benchmark
from scripts import agent_loop_report as report


class AgentLoopBenchmarkTests(unittest.TestCase):
    def _install_manifest(self, repo: Path) -> None:
        target = repo / "scripts/agent_loop_cases.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            report.DEFAULT_CASE_MANIFEST.read_text(encoding="utf-8"),
            encoding="utf-8",
        )

    def test_pair_order_alternates(self) -> None:
        self.assertEqual(benchmark._pair_order(0), ("direct", "code_mode"))
        self.assertEqual(benchmark._pair_order(1), ("code_mode", "direct"))
        self.assertEqual(benchmark._pair_order(2), ("direct", "code_mode"))

    @unittest.skipIf(os.name == "nt", "paired Host driver is explicitly POSIX-only")
    def test_driver_stdin_is_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "stdin.txt"
            benchmark._run_driver(
                [
                    sys.executable,
                    "-c",
                    "from pathlib import Path; import sys; Path(sys.argv[1]).write_text(sys.stdin.read())",
                    str(output),
                ],
                cwd=Path(tmp),
                env={},
            )
            self.assertEqual(output.read_text(encoding="utf-8"), "")

    @unittest.skipIf(os.name == "nt", "paired Host driver is explicitly POSIX-only")
    def test_driver_interruption_terminates_owned_process_tree(self) -> None:
        process = mock.Mock()
        process.pid = 12345
        process.wait.side_effect = KeyboardInterrupt
        with (
            mock.patch.object(benchmark.subprocess, "Popen", return_value=process),
            mock.patch.object(benchmark, "_terminate_driver_tree") as terminate,
        ):
            with self.assertRaises(KeyboardInterrupt):
                benchmark._run_driver(
                    [sys.executable, "-c", "pass"],
                    cwd=Path("."),
                    env={},
                )
        terminate.assert_called_once_with(process)

    def test_windows_host_driver_fails_closed_as_unsupported(self) -> None:
        with mock.patch.object(benchmark.os, "name", "nt"):
            with self.assertRaisesRegex(
                benchmark.DriverPlatformUnsupported,
                "POSIX process-group ownership",
            ):
                benchmark._run_driver(
                    [sys.executable, "-c", "pass"],
                    cwd=Path("."),
                    env={},
                )

    def test_runtime_evidence_gate_fails_closed(self) -> None:
        direct = {
            "outer_calls": {"total": 1},
            "canonical_calls": {"total": 1},
            "availability": {"code_mode_composition": {"available": True}},
            "tools": {"outer_by_name": {"read_files": 1}},
        }
        self.assertTrue(benchmark._runtime_evidence_proven(direct, "direct", "direct"))
        direct["availability"]["code_mode_composition"]["available"] = False
        self.assertFalse(benchmark._runtime_evidence_proven(direct, "direct", "direct"))

        code_mode = {
            "outer_calls": {"total": 1},
            "availability": {"code_mode_composition": {"available": True}},
            "composition": {"outer_code_mode_calls": 1},
            "tools": {"outer_by_name": {"execute_mutating_code_mode": 1}},
        }
        self.assertTrue(
            benchmark._runtime_evidence_proven(code_mode, "code_mode", "guarded_edit")
        )
        self.assertFalse(
            benchmark._runtime_evidence_proven(code_mode, "code_mode", "read_only")
        )
        read_only = {
            "outer_calls": {"total": 2},
            "availability": {"code_mode_composition": {"available": True}},
            "composition": {"outer_code_mode_calls": 1},
            "tools": {
                "outer_by_name": {
                    "work_on_project": 1,
                    "execute_code_mode": 1,
                }
            },
        }
        self.assertTrue(
            benchmark._runtime_evidence_proven(read_only, "code_mode", "read_only")
        )
        read_only["outer_calls"]["total"] = 3
        read_only["tools"]["outer_by_name"]["read_tool_manifest"] = 1
        self.assertTrue(
            benchmark._runtime_evidence_proven(read_only, "code_mode", "read_only")
        )
        read_only["tools"]["outer_by_name"]["read_tool_manifest"] = 2
        read_only["outer_calls"]["total"] = 4
        self.assertFalse(
            benchmark._runtime_evidence_proven(read_only, "code_mode", "read_only")
        )
        read_only["tools"]["outer_by_name"]["read_tool_manifest"] = 1
        read_only["outer_calls"]["total"] = 4
        read_only["tools"]["outer_by_name"]["read_files"] = 1
        self.assertFalse(
            benchmark._runtime_evidence_proven(read_only, "code_mode", "read_only")
        )
        del read_only["tools"]["outer_by_name"]["read_files"]
        del read_only["tools"]["outer_by_name"]["read_tool_manifest"]
        read_only["outer_calls"]["total"] = 1
        del read_only["tools"]["outer_by_name"]["work_on_project"]
        self.assertFalse(
            benchmark._runtime_evidence_proven(read_only, "code_mode", "read_only")
        )
        code_mode["tools"]["outer_by_name"]["execute_code_mode"] = 1
        code_mode["composition"]["outer_code_mode_calls"] = 2
        self.assertFalse(
            benchmark._runtime_evidence_proven(code_mode, "code_mode", "guarded_edit")
        )

    def test_long_handoff_requires_one_launch_and_exact_terminal_relation(self) -> None:
        direct = {
            "canonical_calls": {"by_name": {"cargo_test": 1}},
            "job_convergence": {
                "pending_handoff_count": 1,
                "pending_followup_known_count": 1,
                "pending_to_terminal_ms": {"samples": 1},
                "pending_handoff_by_origin_tool": {"cargo_test": 1},
                "pending_followup_known_by_origin_tool": {"cargo_test": 1},
                "pending_to_terminal_ms_by_origin_tool": {
                    "cargo_test": {"samples": 1}
                },
                "selected_terminal_by_origin_tool": {"cargo_test": 1},
            },
        }
        self.assertTrue(benchmark._long_handoff_terminal_proven(direct, "direct"))
        direct["job_convergence"]["selected_terminal_by_origin_tool"] = {}
        self.assertFalse(benchmark._long_handoff_terminal_proven(direct, "direct"))
        direct["job_convergence"]["selected_terminal_by_origin_tool"] = {"cargo_test": 1}
        direct["canonical_calls"]["by_name"]["cargo_test"] = 2
        self.assertFalse(benchmark._long_handoff_terminal_proven(direct, "direct"))

        code_mode = {
            "availability": {"code_mode_composition": {"available": True}},
            "composition": {
                "nested_tool_counts": {"cargo_test": 1},
                "job_handoffs": {"total": 1},
                "consequential_calls": {"total": 1},
                "known_results": {"total": 0},
                "outcome_unknown": {"total": 0},
            },
            "job_convergence": {
                "pending_handoff_count": 1,
                "pending_followup_known_count": 1,
                "pending_to_terminal_ms": {"samples": 1},
                "pending_handoff_by_origin_tool": {
                    "execute_effectful_code_mode": 1
                },
                "pending_followup_known_by_origin_tool": {
                    "execute_effectful_code_mode": 1
                },
                "pending_to_terminal_ms_by_origin_tool": {
                    "execute_effectful_code_mode": {"samples": 1}
                },
                "selected_terminal_by_origin_tool": {
                    "execute_effectful_code_mode": 1
                },
            },
        }
        self.assertTrue(benchmark._long_handoff_terminal_proven(code_mode, "code_mode"))
        code_mode["job_convergence"]["selected_terminal_by_origin_tool"] = {}
        self.assertFalse(
            benchmark._long_handoff_terminal_proven(code_mode, "code_mode")
        )
        code_mode["job_convergence"]["selected_terminal_by_origin_tool"] = {
            "execute_effectful_code_mode": 1
        }
        code_mode["job_convergence"]["pending_handoff_by_origin_tool"] = {
            "run_process": 1
        }
        self.assertFalse(
            benchmark._long_handoff_terminal_proven(code_mode, "code_mode")
        )
        code_mode["job_convergence"]["pending_handoff_by_origin_tool"] = {
            "execute_effectful_code_mode": 1
        }
        code_mode["job_convergence"]["pending_to_terminal_ms"]["samples"] = 0
        self.assertFalse(benchmark._long_handoff_terminal_proven(code_mode, "code_mode"))
        code_mode["job_convergence"]["pending_to_terminal_ms"]["samples"] = 1
        code_mode["composition"]["nested_tool_counts"]["cargo_test"] = 2
        self.assertFalse(benchmark._long_handoff_terminal_proven(code_mode, "code_mode"))

    def test_multi_file_contract_requires_one_edit_search_and_post_read_capacity(self) -> None:
        direct = {
            "canonical_calls": {
                "by_name": {
                    "edit_project_files": 1,
                    "search_project_texts": 1,
                    "read_files": 2,
                }
            },
            "outer_calls": {"failed": 0, "timeout_or_unknown": 0},
            "edit_outcomes": {
                "by_tool": {"edit_project_files": {"applied": 1}},
            },
            "availability": {"edit_outcomes": {"available": True}},
        }
        self.assertTrue(benchmark._guarded_multi_file_contract_proven(direct, "direct"))
        direct["outer_calls"]["failed"] = 1
        self.assertFalse(benchmark._guarded_multi_file_contract_proven(direct, "direct"))
        direct["outer_calls"]["failed"] = 0
        direct["edit_outcomes"]["by_tool"]["edit_project_files"] = {
            "dry_run_would_change": 1
        }
        self.assertFalse(benchmark._guarded_multi_file_contract_proven(direct, "direct"))
        direct["edit_outcomes"]["by_tool"]["edit_project_files"] = {"applied": 1}
        direct["canonical_calls"]["by_name"]["edit_project_files"] = 2
        self.assertFalse(benchmark._guarded_multi_file_contract_proven(direct, "direct"))

        code_mode = {
            "availability": {"code_mode_composition": {"available": True}},
            "composition": {
                "nested_tool_counts": {
                    "edit_project_files": 1,
                    "search_project_texts": 1,
                    "read_files": 2,
                },
                "consequential_calls": {"total": 1},
                "known_results": {"total": 1},
                "job_handoffs": {"total": 0},
                "outcome_unknown": {"total": 0},
                "mutation_state_changed": {"total": 1},
                "mutation_no_change": {"total": 0},
            },
            "outer_calls": {"failed": 0, "timeout_or_unknown": 0},
            "child_calls": {"failed": 0},
        }
        self.assertTrue(benchmark._guarded_multi_file_contract_proven(code_mode, "code_mode"))
        code_mode["child_calls"]["failed"] = 1
        self.assertFalse(benchmark._guarded_multi_file_contract_proven(code_mode, "code_mode"))
        code_mode["child_calls"]["failed"] = 0
        code_mode["composition"]["consequential_calls"]["total"] = 2
        self.assertFalse(benchmark._guarded_multi_file_contract_proven(code_mode, "code_mode"))
        code_mode["composition"]["consequential_calls"]["total"] = 1
        code_mode["composition"]["mutation_state_changed"]["total"] = 0
        self.assertFalse(benchmark._guarded_multi_file_contract_proven(code_mode, "code_mode"))
        code_mode["composition"]["mutation_state_changed"]["total"] = 1
        code_mode["composition"]["nested_tool_counts"]["read_files"] = 1
        self.assertFalse(benchmark._guarded_multi_file_contract_proven(code_mode, "code_mode"))

    def test_fixture_oracle_checks_both_files_and_exact_diff(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        case = report._case_by_id(manifest, "guarded_multi_file_edit")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-b", "main"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=root, check=True)
            fixture = root / "tests/fixtures/agent-loop-multi-file"
            fixture.mkdir(parents=True)
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=before\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=before\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-m", "fixture"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=after\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=after\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            result = benchmark._fixture_oracle(case, root)
            self.assertTrue(result["available"])
            self.assertTrue(result["passed"])

            subprocess.run(["git", "add", "."], cwd=root, check=True)
            staged = benchmark._fixture_oracle(case, root)
            self.assertTrue(staged["passed"])

            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=after\nBETA_UNRELATED_SENTINEL=lost\n",
                encoding="utf-8",
            )
            result = benchmark._fixture_oracle(case, root)
            self.assertFalse(result["passed"])

            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=after\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (root / "unexpected.txt").write_text("unexpected\n", encoding="utf-8")
            result = benchmark._fixture_oracle(case, root)
            self.assertFalse(result["passed"])
            changed_check = next(check for check in result["checks"] if check.get("kind") == "changed_files")
            self.assertIn("unexpected.txt", changed_check["actual"])

            (root / "unexpected.txt").unlink()
            (fixture / "beta.txt").write_bytes(b"\xff\xfe")
            result = benchmark._fixture_oracle(case, root)
            self.assertFalse(result["passed"])
            beta_check = next(check for check in result["checks"] if check.get("path", "").endswith("beta.txt"))
            self.assertEqual(beta_check["reason"], "file is not UTF-8")

            (fixture / "beta.txt").write_bytes(b"x" * (benchmark.MAX_FIXTURE_ORACLE_BYTES + 1))
            result = benchmark._fixture_oracle(case, root)
            self.assertFalse(result["passed"])
            beta_check = next(check for check in result["checks"] if check.get("path", "").endswith("beta.txt"))
            self.assertEqual(beta_check["reason"], "file exceeds oracle byte limit")

    def test_fixture_oracle_reports_git_path_query_failure(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        case = report._case_by_id(manifest, "guarded_multi_file_edit")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fixture = root / "tests/fixtures/agent-loop-multi-file"
            fixture.mkdir(parents=True)
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=after\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=after\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            with mock.patch.object(
                benchmark,
                "_bounded_git_paths",
                side_effect=[(False, False, []), (True, False, [])],
            ):
                result = benchmark._fixture_oracle(case, root)
            changed = next(
                check for check in result["checks"] if check.get("kind") == "changed_files"
            )
            self.assertFalse(changed["passed"])
            self.assertEqual(changed["reason"], "changed path query failed")

    @unittest.skipIf(os.name == "nt", "FIFO is POSIX-only")
    def test_fixture_oracle_rejects_fifo_without_blocking(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        case = report._case_by_id(manifest, "guarded_multi_file_edit")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-b", "main"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=root, check=True)
            fixture = root / "tests/fixtures/agent-loop-multi-file"
            fixture.mkdir(parents=True)
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=after\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            beta = fixture / "beta.txt"
            beta.write_text(
                "BENCH_TARGET_BETA=before\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-m", "fixture"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            beta.unlink()
            os.mkfifo(beta)
            started = time.monotonic()
            result = benchmark._fixture_oracle(case, root)
            self.assertLess(time.monotonic() - started, 0.5)
            self.assertFalse(result["passed"])
            beta_check = next(check for check in result["checks"] if check.get("path", "").endswith("beta.txt"))
            self.assertEqual(beta_check["reason"], "file is not regular")

    def test_multi_file_fixture_is_pinned_to_lf_checkout(self) -> None:
        repo = Path(__file__).resolve().parents[2]
        completed = subprocess.run(
            [
                "git",
                "check-attr",
                "eol",
                "--",
                "tests/fixtures/agent-loop-multi-file/alpha.txt",
                "tests/fixtures/agent-loop-multi-file/beta.txt",
            ],
            cwd=repo,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        )
        lines = [line for line in completed.stdout.splitlines() if line]
        self.assertEqual(len(lines), 2)
        self.assertTrue(all(line.endswith(": lf") for line in lines))

    def test_malformed_receipt_status_is_a_contract_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            receipt = Path(tmp) / "receipt.json"
            receipt.write_text('{"status": []}', encoding="utf-8")
            with self.assertRaisesRegex(benchmark.BenchmarkError, "status must be"):
                benchmark._load_driver_receipt(receipt)

    def test_driver_receipt_pathological_json_is_a_contract_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, raw in (
                ("deep.json", '{"status":"unsupported","extra":' + "[" * 10000 + "0" + "]" * 10000 + "}"),
                ("huge-int.json", '{"status":"unsupported","extra":' + "1" * 10000 + "}"),
            ):
                receipt = root / name
                receipt.write_text(raw, encoding="utf-8")
                with self.assertRaisesRegex(benchmark.BenchmarkError, "bounded JSON"):
                    benchmark._load_driver_receipt(receipt)

    @unittest.skipIf(os.name == "nt", "FIFO is POSIX-only")
    def test_driver_receipt_rejects_fifo_without_blocking(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            receipt = Path(tmp) / "receipt.json"
            os.mkfifo(receipt)
            started = time.monotonic()
            with self.assertRaisesRegex(benchmark.BenchmarkError, "regular file"):
                benchmark._load_driver_receipt(receipt)
            self.assertLess(time.monotonic() - started, 0.5)

    def test_driver_receipt_rejects_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target.json"
            target.write_text('{"status":"pass"}', encoding="utf-8")
            receipt = root / "receipt.json"
            try:
                receipt.symlink_to(target)
            except OSError:
                self.skipTest("symlinks unavailable")
            with self.assertRaisesRegex(benchmark.BenchmarkError, "regular file"):
                benchmark._load_driver_receipt(receipt)

    def test_comparison_requires_two_effective_pass_samples(self) -> None:
        summary = {"benchmark": {}}
        self.assertIsNone(
            benchmark._comparison(
                {"status": "partial", "summary": summary},
                {"status": "pass", "summary": summary},
            )
        )
        self.assertIsNone(
            benchmark._comparison(
                {"status": "pass", "summary": None},
                {"status": "pass", "summary": summary},
            )
        )

    def test_annotation_normalizes_omitted_correctness_verdicts(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        readonly = report._case_by_id(manifest, "readonly_review")
        annotation = benchmark._annotation(
            readonly,
            variant="direct",
            surface="direct",
            base_revision="a" * 40,
            receipt={"correctness": {"task_verdict": "pass"}},
            oracle={"available": False, "passed": None, "checks": [], "reason": None},
        )
        self.assertEqual(
            annotation["correctness"],
            {"task_verdict": "pass", "validation_verdict": "not_required"},
        )

        validation = report._case_by_id(manifest, "long_validation_handoff")
        annotation = benchmark._annotation(
            validation,
            variant="direct",
            surface="direct",
            base_revision="a" * 40,
            receipt={"correctness": {"task_verdict": "pass"}},
            oracle={"available": False, "passed": None, "checks": [], "reason": None},
        )
        self.assertEqual(
            annotation["correctness"],
            {"task_verdict": "pass", "validation_verdict": "unavailable"},
        )

    def test_relative_receipt_artifacts_are_anchored_to_workspace(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        case = report._case_by_id(manifest, "readonly_review")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            annotation = root / "annotation.json"
            case_manifest = root / "manifest.json"
            case_manifest.write_text(
                report.DEFAULT_CASE_MANIFEST.read_text(encoding="utf-8"),
                encoding="utf-8",
            )
            receipt = {
                "status": "pass",
                "audit_db": ".webcodex/action_audit.db",
                "trace_root": ".webcodex/traces",
                "workflow_session_id": "wc_sess_test",
            }
            with mock.patch.object(report, "summarize", return_value={"ok": True}) as summarize:
                value = benchmark._summary_for_receipt(
                    case,
                    variant="direct",
                    surface="direct",
                    base_revision="0" * 40,
                    receipt=receipt,
                    annotation_path=annotation,
                    case_manifest=case_manifest,
                    workspace=root,
                )
            self.assertEqual(value, {"ok": True})
            kwargs = summarize.call_args.kwargs
            self.assertEqual(kwargs["audit_db"], root / ".webcodex/action_audit.db")
            self.assertEqual(kwargs["trace_root"], root / ".webcodex/traces")

    def test_fixture_oracle_bounds_changed_path_listing(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        case = report._case_by_id(manifest, "guarded_multi_file_edit")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-b", "main"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=root, check=True)
            fixture = root / "tests/fixtures/agent-loop-multi-file"
            fixture.mkdir(parents=True)
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=before\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=before\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-m", "fixture"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=after\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=after\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            overflow = root / "overflow"
            overflow.mkdir()
            for index in range(benchmark.MAX_GIT_PATHS + 1):
                (overflow / f"path-{index:04d}.txt").write_text("x", encoding="utf-8")

            result = benchmark._fixture_oracle(case, root)
            changed = next(
                check for check in result["checks"] if check.get("kind") == "changed_files"
            )
            self.assertFalse(result["passed"])
            self.assertFalse(changed["passed"])
            self.assertEqual(changed["reason"], "changed path listing exceeds oracle limit")
            self.assertIsNone(changed["actual"])

    def test_missing_receipts_are_retained_as_failed_samples(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            result = benchmark.run_benchmark(
                repo=repo,
                base_revision=base,
                driver_argv=[sys.executable, "-c", "pass"],
                case_ids=["readonly_review"],
                pairs=1,
            )
            self.assertEqual(result["status_counts"]["fail"], 2)
            for sample in result["cases"][0]["pairs"][0]["samples"]:
                self.assertEqual(sample["reason_code"], "driver_receipt_missing")
                self.assertIsNone(sample["summary"])
            self.assertIsNone(result["cases"][0]["pairs"][0]["comparison"])

    def test_unsupported_driver_samples_are_retained_and_worktrees_cleaned(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()

            driver = root / "driver.py"
            driver.write_text(
                "import json, os\n"
                "from pathlib import Path\n"
                "Path(os.environ['WEBCODEX_BENCH_DRIVER_RESULT']).write_text("
                "json.dumps({'status':'unsupported','message':'fixture driver'}), encoding='utf-8')\n",
                encoding="utf-8",
            )
            result = benchmark.run_benchmark(
                repo=repo,
                base_revision=base,
                driver_argv=[sys.executable, str(driver)],
                case_ids=["readonly_review"],
                pairs=2,
            )
            self.assertEqual(result["sample_count"], 4)
            self.assertEqual(result["status_counts"]["unsupported"], 4)
            self.assertEqual(result["cases"][0]["pairs"][0]["order"], ["direct", "code_mode"])
            self.assertEqual(result["cases"][0]["pairs"][1]["order"], ["code_mode", "direct"])
            self.assertIsNone(result["cases"][0]["pairs"][0]["comparison"])

            worktrees = subprocess.check_output(["git", "worktree", "list", "--porcelain"], cwd=repo, text=True)
            self.assertEqual(worktrees.count("worktree "), 1)

    @unittest.skipIf(os.name == "nt", "post-checkout hook regression is POSIX-only")
    def test_failed_worktree_checkout_rolls_back_owned_registration(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()

            unrelated = root / "unrelated"
            subprocess.run(
                ["git", "worktree", "add", "--detach", str(unrelated), base],
                cwd=repo,
                check=True,
                stdout=subprocess.DEVNULL,
            )
            hook = repo / ".git/hooks/post-checkout"
            hook.write_text("#!/bin/sh\nexit 1\n", encoding="utf-8")
            hook.chmod(0o755)

            result = benchmark.run_benchmark(
                repo=repo,
                base_revision=base,
                driver_argv=[sys.executable, "-c", "pass"],
                case_ids=["readonly_review"],
                pairs=1,
            )
            self.assertEqual(result["status_counts"]["fail"], 2)
            worktrees = subprocess.check_output(
                ["git", "worktree", "list", "--porcelain"],
                cwd=repo,
                text=True,
            )
            self.assertIn(str(unrelated), worktrees)
            self.assertNotIn("webcodex-agent-loop-bench-", worktrees)
            subprocess.run(
                ["git", "worktree", "remove", "--force", str(unrelated)],
                cwd=repo,
                check=True,
            )

    def test_cleanup_does_not_prune_unrelated_worktree(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()

            unrelated = root / "unrelated"
            subprocess.run(["git", "worktree", "add", "--detach", str(unrelated), base], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            driver = root / "driver.py"
            driver.write_text(
                "import json, os\n"
                "from pathlib import Path\n"
                "Path(os.environ['WEBCODEX_BENCH_DRIVER_RESULT']).write_text("
                "json.dumps({'status':'unsupported'}), encoding='utf-8')\n",
                encoding="utf-8",
            )
            benchmark.run_benchmark(
                repo=repo,
                base_revision=base,
                driver_argv=[sys.executable, str(driver)],
                case_ids=["readonly_review"],
                pairs=1,
            )
            status = subprocess.run(
                ["git", "status", "--porcelain"],
                cwd=unrelated,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(status.returncode, 0, status.stderr)
            subprocess.run(["git", "worktree", "remove", "--force", str(unrelated)], cwd=repo, check=True)

    def test_manifest_blob_is_bounded_and_utf8(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-b", "main"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=root, check=True)
            manifest = root / "scripts/agent_loop_cases.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_bytes(b"\xff\xfe")
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-m", "bad utf8"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            with self.assertRaisesRegex(benchmark.BenchmarkError, "UTF-8"):
                benchmark._read_manifest_at_revision(root, base)

            manifest.write_bytes(b"x" * (benchmark.MAX_CASE_MANIFEST_BYTES + 1))
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-m", "too large"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            with self.assertRaisesRegex(benchmark.BenchmarkError, "1 MiB"):
                benchmark._read_manifest_at_revision(root, base)

    def test_case_manifest_is_pinned_to_base_revision(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()

            manifest_path = repo / "scripts/agent_loop_cases.json"
            dirty = json.loads(manifest_path.read_text(encoding="utf-8"))
            dirty["cases"][0]["prompt"] = "dirty prompt must not be observed"
            manifest_path.write_text(json.dumps(dirty), encoding="utf-8")

            observed = root / "observed.txt"
            driver = root / "driver.py"
            driver.write_text(
                "import os, sys\n"
                "from pathlib import Path\n"
                "Path(sys.argv[1]).write_text(os.environ['WEBCODEX_BENCH_PROMPT'], encoding='utf-8')\n"
                "Path(os.environ['WEBCODEX_BENCH_DRIVER_RESULT']).write_text('{\"status\":\"unsupported\"}', encoding='utf-8')\n",
                encoding="utf-8",
            )
            benchmark.run_benchmark(
                repo=repo,
                base_revision=base,
                driver_argv=[sys.executable, str(driver), str(observed)],
                case_ids=["readonly_review"],
                pairs=1,
            )
            pinned = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
            expected = report._case_by_id(pinned, "readonly_review")["prompt"]
            self.assertEqual(observed.read_text(encoding="utf-8"), expected)

    def test_base_revision_must_match_checkout_head(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("one\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "one"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            old = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            (repo / "README.md").write_text("two\n", encoding="utf-8")
            subprocess.run(["git", "commit", "-am", "two"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            with self.assertRaisesRegex(benchmark.BenchmarkError, "current checkout HEAD"):
                benchmark.run_benchmark(
                    repo=repo,
                    base_revision=old,
                    driver_argv=[sys.executable, "-c", "raise SystemExit(0)"],
                    case_ids=["readonly_review"],
                    pairs=1,
                )

    def test_fixture_oracle_uses_recorded_base_when_head_advances(self) -> None:
        manifest = report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
        case = report._case_by_id(manifest, "guarded_multi_file_edit")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init", "-b", "main"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=root, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=root, check=True)
            fixture = root / "tests/fixtures/agent-loop-multi-file"
            fixture.mkdir(parents=True)
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=before\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=before\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-m", "base"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            (fixture / "alpha.txt").write_text(
                "BENCH_TARGET_ALPHA=after\nALPHA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            (fixture / "beta.txt").write_text(
                "BENCH_TARGET_BETA=after\nBETA_UNRELATED_SENTINEL=keep\n",
                encoding="utf-8",
            )
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(["git", "commit", "-m", "driver commit"], cwd=root, check=True, stdout=subprocess.DEVNULL)
            oracle = benchmark._fixture_oracle(case, root, base)
            changed = next(check for check in oracle["checks"] if check.get("kind") == "changed_files")
            self.assertTrue(changed["passed"])
            self.assertEqual(changed["actual"], sorted(case["correctness"]["changed_files"]))
            self.assertNotEqual(subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(), base)

    @unittest.skipIf(os.name == "nt", "POSIX process-group regression")
    def test_driver_timeout_terminates_descendant_process_group(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            marker = root / "descendant-wrote"
            driver = root / "driver.py"
            driver.write_text(
                "import subprocess, sys, time\n"
                "subprocess.Popen([sys.executable, '-c', "
                "'import pathlib,time,sys; time.sleep(1); pathlib.Path(sys.argv[1]).write_text(\\\"late\\\")', "
                "sys.argv[1]])\n"
                "time.sleep(10)\n",
                encoding="utf-8",
            )
            old_timeout = benchmark.MAX_DRIVER_SECS
            benchmark.MAX_DRIVER_SECS = 0.1
            try:
                with self.assertRaisesRegex(benchmark.BenchmarkError, "exceeded"):
                    benchmark._run_driver(
                        [sys.executable, str(driver), str(marker)],
                        cwd=root,
                        env=dict(os.environ),
                    )
            finally:
                benchmark.MAX_DRIVER_SECS = old_timeout
            time.sleep(1.2)
            self.assertFalse(marker.exists())

    def test_duplicate_case_ids_are_rejected_before_driver_execution(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()

            marker = root / "driver-ran"
            driver = root / "driver.py"
            driver.write_text(
                "from pathlib import Path; import sys; Path(sys.argv[1]).write_text('ran')\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(benchmark.BenchmarkError, "duplicate case ids"):
                benchmark.run_benchmark(
                    repo=repo,
                    base_revision=base,
                    driver_argv=[sys.executable, str(driver), str(marker)],
                    case_ids=["readonly_review", "readonly_review"],
                    pairs=1,
                )
            self.assertFalse(marker.exists())

    def test_run_sample_clears_stale_receipt_before_driver(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            manifest = report.load_case_manifest(repo / "scripts/agent_loop_cases.json")
            case = report._case_by_id(manifest, "readonly_review")

            bench_root = root / "bench"
            for name in ("worktrees", "receipts", "annotations"):
                (bench_root / name).mkdir(parents=True, exist_ok=True)
            stale = bench_root / "receipts" / "readonly_review-p1-1-direct.json"
            stale.write_text('{"status":"unsupported"}', encoding="utf-8")
            sample = benchmark._run_sample(
                repo,
                bench_root,
                [sys.executable, "-c", "pass"],
                case,
                variant="direct",
                pair_index=0,
                ordinal=0,
                base_revision=base,
                case_manifest=repo / "scripts/agent_loop_cases.json",
            )
            self.assertEqual(sample["status"], "fail")
            self.assertEqual(sample["reason_code"], "driver_receipt_missing")

    def test_driver_timeout_has_distinct_reason_code(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            manifest_path = repo / "scripts/agent_loop_cases.json"
            manifest = report.load_case_manifest(manifest_path)
            case = report._case_by_id(manifest, "readonly_review")
            bench_root = root / "bench"
            for name in ("worktrees", "receipts", "annotations"):
                (bench_root / name).mkdir(parents=True, exist_ok=True)

            with mock.patch.object(
                benchmark,
                "_run_driver",
                side_effect=benchmark.DriverTimeoutError("timeout"),
            ):
                sample = benchmark._run_sample(
                    repo,
                    bench_root,
                    [sys.executable, "-c", "pass"],
                    case,
                    variant="direct",
                    pair_index=0,
                    ordinal=0,
                    base_revision=base,
                    case_manifest=manifest_path,
                )
            self.assertEqual(sample["status"], "fail")
            self.assertEqual(sample["reason_code"], "driver_timeout")
            self.assertIsNone(sample["driver_exit_code"])

    def test_contract_error_preserves_driver_exit_code(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            manifest = report.load_case_manifest(repo / "scripts/agent_loop_cases.json")
            case = report._case_by_id(manifest, "readonly_review")
            bench_root = root / "bench"
            for name in ("worktrees", "receipts", "annotations"):
                (bench_root / name).mkdir(parents=True, exist_ok=True)
            driver = root / "driver.py"
            driver.write_text(
                "import os\n"
                "from pathlib import Path\n"
                "Path(os.environ['WEBCODEX_BENCH_DRIVER_RESULT']).write_text('not-json', encoding='utf-8')\n"
                "raise SystemExit(7)\n",
                encoding="utf-8",
            )
            sample = benchmark._run_sample(
                repo,
                bench_root,
                [sys.executable, str(driver)],
                case,
                variant="direct",
                pair_index=0,
                ordinal=0,
                base_revision=base,
                case_manifest=repo / "scripts/agent_loop_cases.json",
            )
            self.assertEqual(sample["status"], "fail")
            self.assertEqual(sample["reason_code"], "sample_contract_error")
            self.assertEqual(sample["driver_exit_code"], 7)

    def test_fallback_task_timing_starts_after_worktree_setup(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            manifest_path = repo / "scripts/agent_loop_cases.json"
            manifest = report.load_case_manifest(manifest_path)
            case = report._case_by_id(manifest, "readonly_review")
            bench_root = root / "bench"
            for name in ("worktrees", "receipts", "annotations"):
                (bench_root / name).mkdir(parents=True, exist_ok=True)
            driver = root / "driver.py"
            driver.write_text(
                "import os\n"
                "from pathlib import Path\n"
                "Path(os.environ['WEBCODEX_BENCH_DRIVER_RESULT']).write_text('{\"status\":\"unsupported\"}', encoding='utf-8')\n",
                encoding="utf-8",
            )

            setup_done_ms: list[int] = []
            original = benchmark._worktree_add

            def recorded_add(repo_arg: Path, path_arg: Path, revision_arg: str) -> None:
                original(repo_arg, path_arg, revision_arg)
                setup_done_ms.append(time.time_ns() // 1_000_000)

            benchmark._worktree_add = recorded_add
            try:
                benchmark._run_sample(
                    repo,
                    bench_root,
                    [sys.executable, str(driver)],
                    case,
                    variant="direct",
                    pair_index=0,
                    ordinal=0,
                    base_revision=base,
                    case_manifest=manifest_path,
                )
            finally:
                benchmark._worktree_add = original

            annotation = json.loads(
                (bench_root / "annotations" / "readonly_review-p1-1-direct.json").read_text(encoding="utf-8")
            )
            self.assertGreaterEqual(annotation["task_timing"]["started_at_ms"], setup_done_ms[0])

    def test_non_repository_case_target_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            with self.assertRaisesRegex(benchmark.BenchmarkError, "supports only target=webcodex_repository"):
                benchmark.run_benchmark(
                    repo=repo,
                    base_revision=base,
                    driver_argv=[sys.executable, "-c", "pass"],
                    case_ids=["focused_edit_validation"],
                    pairs=1,
                )

    def test_repo_argument_is_canonicalized_from_subdirectory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            driver = root / "driver.py"
            driver.write_text(
                "import os\n"
                "from pathlib import Path\n"
                "Path(os.environ['WEBCODEX_BENCH_DRIVER_RESULT']).write_text('{\"status\":\"unsupported\"}', encoding='utf-8')\n",
                encoding="utf-8",
            )
            result = benchmark.run_benchmark(
                repo=repo / "scripts",
                base_revision=base,
                driver_argv=[sys.executable, str(driver)],
                case_ids=["readonly_review"],
                pairs=1,
            )
            self.assertEqual(result["status_counts"]["unsupported"], 2)

    def test_cli_prints_human_summary_and_json_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-b", "main"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            subprocess.run(["git", "config", "user.email", "bench@test.local"], cwd=repo, check=True)
            subprocess.run(["git", "config", "user.name", "Bench"], cwd=repo, check=True)
            (repo / "README.md").write_text("fixture\n", encoding="utf-8")
            self._install_manifest(repo)
            subprocess.run(["git", "add", "."], cwd=repo, check=True)
            subprocess.run(["git", "commit", "-m", "init"], cwd=repo, check=True, stdout=subprocess.DEVNULL)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
            driver = root / "driver.py"
            driver.write_text(
                "import json, os\n"
                "from pathlib import Path\n"
                "Path(os.environ['WEBCODEX_BENCH_DRIVER_RESULT']).write_text("
                "json.dumps({'status':'unsupported'}), encoding='utf-8')\n",
                encoding="utf-8",
            )
            output = root / "result.json"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(Path(benchmark.__file__)),
                    "--repo",
                    str(repo),
                    "--base-revision",
                    base,
                    "--driver",
                    f"{sys.executable} {driver}",
                    "--case-id",
                    "readonly_review",
                    "--pairs",
                    "1",
                    "--output",
                    str(output),
                ],
                cwd=Path(__file__).resolve().parents[2],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertIn("Agent Loop benchmark", completed.stdout)
            value = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(value["status_counts"]["unsupported"], 2)


if __name__ == "__main__":
    unittest.main()
