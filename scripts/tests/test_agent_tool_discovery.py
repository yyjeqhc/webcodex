from __future__ import annotations

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import agent_tool_discovery as discovery


class ToolDiscoveryScoringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.corpus = discovery.load_corpus()

    def capture(self, results=None):
        if results is None:
            results = [{"case_id": case["id"], "returned_tools": list(case["expected_tools"])}
                       for case in self.corpus["cases"]]
        return {"schema_version": 1, "corpus_fingerprint": discovery.corpus_fingerprint(self.corpus),
                "base_revision": "a" * 40, "results": results}

    def test_fingerprint_covers_labels_and_preserves_json_key_order_independence(self):
        reordered = dict(reversed(list(self.corpus.items())))
        self.assertEqual(discovery.corpus_fingerprint(self.corpus), discovery.corpus_fingerprint(reordered))
        changed = copy.deepcopy(self.corpus)
        changed["cases"][8]["prompt"] += " please"
        self.assertNotEqual(discovery.corpus_fingerprint(self.corpus), discovery.corpus_fingerprint(changed))

    def test_complete_capture_scores_unordered_labels_and_rejections(self):
        capture = self.capture()
        for result in capture["results"]:
            result["returned_tools"].reverse()
        scored = discovery.evaluate_results(self.corpus, capture)
        self.assertEqual(scored["status"], "complete")
        self.assertIn("not live model or Host proof", scored["evidence_limit"])
        for split in scored["splits"].values():
            self.assertEqual(split["status"], "complete")
            self.assertEqual(split["missing_count"], 0)
            self.assertEqual(split["metrics"]["hit_rate_at_5"], 1)
            self.assertEqual(split["metrics"]["recall_at_5"], 1)
            self.assertEqual(split["metrics"]["forbidden_leakage_rate_at_5"], 0)
            self.assertEqual(split["metrics"]["correct_rejection_rate"], 1)

    def test_no_capture_and_partial_capture_are_unavailable_not_zero_success(self):
        empty = discovery.evaluate_results(self.corpus, self.capture([]))
        self.assertEqual(empty["status"], "unavailable")
        for split in empty["splits"].values():
            self.assertIsNone(split["metrics"])
            self.assertEqual(split["captured_count"], 0)
            self.assertEqual(split["missing_count"], split["case_count"])
        partial = discovery.evaluate_results(self.corpus, self.capture([
            {"case_id": "discovery-001", "returned_tools": ["read_files"]},
        ]))
        self.assertEqual(partial["status"], "incomplete")
        development = partial["splits"]["development"]
        self.assertEqual(development["observed_counts"]["positive_hits_at_5"], 1)
        self.assertIsNone(development["metrics"])
        self.assertEqual(partial["splits"]["heldout"]["status"], "unavailable")

    def test_complete_development_does_not_require_heldout_to_be_scored(self):
        capture = self.capture()
        capture["results"] = [result for result, case in zip(capture["results"], self.corpus["cases"])
                              if case["split"] == "development"]
        scored = discovery.evaluate_results(self.corpus, capture)
        self.assertEqual(scored["splits"]["development"]["status"], "complete")
        self.assertIsNotNone(scored["splits"]["development"]["metrics"])
        self.assertIsNone(scored["splits"]["heldout"]["metrics"])

    def test_canonical_nonlabel_returns_are_false_positives(self):
        capture = self.capture()
        capture["results"][0]["returned_tools"] = [
            "list_tools", "list_projects", "get_git_status", "create_goal", "run_process",
        ]
        scored = discovery.evaluate_results(self.corpus, capture)
        development = scored["splits"]["development"]
        self.assertLess(development["metrics"]["hit_rate_at_5"], 1)
        self.assertLess(development["metrics"]["precision_at_5"], 1)
        self.assertEqual(development["observed_counts"]["forbidden_returns_at_5"], 0)

    def test_canonical_helper_declarations_are_accepted_as_false_positives(self):
        resources = ["open_webcodex_workbench", "search_webcodex_resources", "read_webcodex_resource"]
        documents = ["present_docx", "present_pdf", "read_app_artifact_chunk"]
        controls = ["list_tools", "create_goal", "list_agent_identities", "create_agent_task", "wait_for_agent_events"]
        scored = discovery.evaluate_results(self.corpus, self.capture([
            {"case_id": "discovery-001", "returned_tools": resources},
            {"case_id": "discovery-002", "returned_tools": documents},
            {"case_id": "discovery-003", "returned_tools": controls},
        ]))
        self.assertEqual(scored["splits"]["development"]["observed_counts"]["positive_hits_at_5"], 0)
        self.assertEqual(scored["status"], "incomplete")

    def test_capture_cannot_hide_forbidden_tools_beyond_requested_limit(self):
        case = next(case for case in self.corpus["cases"] if case["kind"] == "scope_negative")
        capture = self.capture([{"case_id": case["id"], "returned_tools": [
            "list_tools", "list_projects", "get_git_status", "create_goal", "run_process",
            case["forbidden_tools"][0],
        ]}])
        with self.assertRaisesRegex(ValueError, "bounded unique strings"):
            discovery.evaluate_results(self.corpus, capture)

    def test_scope_and_category_leakage_and_negative_false_matches_are_counted(self):
        capture = self.capture()
        for result in capture["results"]:
            if result["case_id"] == "discovery-041":
                result["returned_tools"] = ["edit_project_files", "write_project_file"]
            if result["case_id"] == "discovery-037":
                result["returned_tools"] = ["get_runtime_status"]
        scored = discovery.evaluate_results(self.corpus, capture)
        counts = scored["splits"]["development"]["observed_counts"]
        self.assertEqual(counts["forbidden_leakage_cases_at_5"], 2)
        self.assertEqual(counts["forbidden_returns_at_5"], 3)
        self.assertLess(scored["splits"]["development"]["metrics"]["correct_rejection_rate"], 1)

    def test_capture_integrity_and_bounded_names_fail_closed(self):
        invalid = []
        for field, value in (("schema_version", True), ("base_revision", "main"),
                             ("base_revision", "a" * 64), ("corpus_fingerprint", "0" * 64)):
            capture = self.capture([])
            capture[field] = value
            invalid.append(capture)
        capture = self.capture([])
        capture["extra"] = "unsupported"
        invalid.append(capture)
        for results in (
            [{"case_id": "unknown", "returned_tools": []}],
            [{"case_id": "discovery-001", "returned_tools": []}] * 2,
            [{"case_id": "discovery-001", "returned_tools": ["read_files", "read_files"]}],
            [{"case_id": "discovery-001", "returned_tools": ["invented_tool"]}],
            [{"case_id": "discovery-001", "returned_tools": [1]}],
            [{"case_id": "discovery-001", "returned_tools": [], "extra": True}],
            [{"case_id": "discovery-001", "returned_tools": ["x" * 129]}],
        ):
            invalid.append(self.capture(results))
        for capture in invalid:
            with self.subTest(capture=capture), self.assertRaises(ValueError):
                discovery.evaluate_results(self.corpus, capture)

    def test_corpus_rejects_duplicate_keys_nan_and_oversize_input(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "corpus.json"
            for raw in ('{"schema_version":1,"schema_version":2}', '{"schema_version":NaN}',
                        ' ' * (discovery.MAX_BYTES + 1)):
                path.write_text(raw, encoding="utf-8")
                with self.subTest(raw=raw[:80]), self.assertRaises(ValueError):
                    discovery.load_corpus(path)

    def test_load_results_is_bounded_strict_json_and_does_not_invent_observations(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "results.json"
            capture = self.capture([])
            path.write_text(json.dumps(capture), encoding="utf-8")
            self.assertEqual(discovery.load_results(path), capture)
            for raw in ('{"schema_version":1,"schema_version":2}', 'NaN',
                        ' ' * (discovery.MAX_BYTES + 1), '[]'):
                path.write_text(raw, encoding="utf-8")
                with self.subTest(raw=raw[:80]), self.assertRaises(ValueError):
                    discovery.load_results(path)
            with self.assertRaises(ValueError):
                discovery.load_results(Path(tmp) / "missing.json")

    def test_regular_reads_enforce_cap_and_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "input.json"
            path.write_bytes(b"1234")
            self.assertEqual(discovery._read_regular_bytes(path, 4), b"1234")
            with self.assertRaises(ValueError):
                discovery._read_regular_bytes(path, 3)
            replacement = Path(tmp) / "replacement.json"
            replacement.write_bytes(b"5678")
            original_open = os.open
            def replace_before_open(target, flags):
                os.replace(replacement, path)
                return original_open(target, flags)
            with mock.patch.object(discovery.os, "open", side_effect=replace_before_open):
                with self.assertRaisesRegex(ValueError, "changed identity"):
                    discovery._read_regular_bytes(path, 4)

    @unittest.skipUnless(hasattr(os, "symlink"), "symlinks unavailable")
    def test_json_and_catalog_reads_reject_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            real = root / "real.json"
            real.write_text(json.dumps(self.capture([])), encoding="utf-8")
            link = root / "link.json"
            try:
                link.symlink_to(real)
            except OSError as exc:
                self.skipTest(f"symlinks unavailable: {exc}")
            with self.assertRaisesRegex(ValueError, "regular file"):
                discovery.load_results(link)
            definitions = root / "crates/webcodex-tool-contracts/src/tool_definition"
            definitions.mkdir(parents=True)
            (definitions.parent / "tool_definition.rs").symlink_to(real)
            with mock.patch.object(discovery, "ROOT", root):
                with self.assertRaisesRegex(ValueError, "regular file"):
                    discovery._catalog()

    @unittest.skipUnless(hasattr(os, "mkfifo") and hasattr(os, "O_NONBLOCK"), "POSIX FIFO checks")
    def test_fifo_and_replacement_fifo_cannot_block_reader(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fifo = root / "input.fifo"
            os.mkfifo(fifo)
            # The timeout protects the test suite if FIFO rejection regresses.
            code = (
                "from scripts import agent_tool_discovery as d; import sys; "
                "d._read_regular_bytes(sys.argv[1], 10)"
            )
            result = subprocess.run([sys.executable, "-c", code, str(fifo)],
                                    cwd=discovery.ROOT, capture_output=True, text=True, timeout=3)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("regular file", result.stderr)
            path = root / "regular.json"
            path.write_bytes(b"{}")
            original_open = os.open
            def replace_with_fifo(target, flags):
                self.assertTrue(flags & os.O_NONBLOCK)
                if hasattr(os, "O_NOFOLLOW"):
                    self.assertTrue(flags & os.O_NOFOLLOW)
                path.unlink()
                os.mkfifo(path)
                return original_open(target, flags)
            with mock.patch.object(discovery.os, "open", side_effect=replace_with_fifo):
                with self.assertRaisesRegex(ValueError, "changed identity"):
                    discovery._read_regular_bytes(path, 10)

    def test_source_catalog_has_per_file_and_total_bounds(self):
        with mock.patch.object(discovery, "MAX_CATALOG_FILE_BYTES", 1):
            with self.assertRaisesRegex(ValueError, "bounded size"):
                discovery._catalog()
        with mock.patch.object(discovery, "MAX_CATALOG_TOTAL_BYTES", 1):
            with self.assertRaisesRegex(ValueError, "total byte bound"):
                discovery._catalog()
        with mock.patch.object(discovery, "MAX_CATALOG_FILES", 1):
            with self.assertRaisesRegex(ValueError, "file count bound"):
                discovery._catalog()

    def test_corpus_rejects_drifted_split_labels_and_request(self):
        invalid = []
        for field, value in (("split", "heldout"), ("expected_tools", ["invented_tool"]),
                             ("forbidden_tools", ["read_files"]), ("allowed_scopes", ["root:all"]),
                             ("request", {"tool_name": "read_files", "query": "read", "limit": 5})):
            corpus = copy.deepcopy(self.corpus)
            corpus["cases"][0][field] = value
            invalid.append(corpus)
        for corpus in invalid:
            with self.subTest(case=corpus["cases"][0]), self.assertRaises(ValueError):
                discovery.corpus_fingerprint(corpus)


if __name__ == "__main__":
    unittest.main()
