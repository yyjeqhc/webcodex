import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from scripts.ci_rust_test import TestFacts, run

class TestCiRustReport(unittest.TestCase):
    def test_signal_is_distinct_and_does_not_blame_last_completed_test(self):
        facts = TestFacts()
        facts.feed("test tests::unrelated ... ok")
        facts.feed("process didn't exit successfully: `/private/target` (signal: 11, SIGSEGV: invalid memory reference)")
        facts.feed("fixture secret MUST_NOT_APPEAR /private/path")
        report = facts.report(101, 2.0, {})
        self.assertEqual(report["outcome"], "signal")
        self.assertEqual(report["signals"], [{"number": 11, "name": "SIGSEGV"}])
        self.assertNotIn("culprit", report)
        self.assertNotIn("/private", json.dumps(report))
        self.assertNotIn("MUST_NOT_APPEAR", json.dumps(report))

    def test_failure_compile_and_incomplete_evidence_are_separate(self):
        facts = TestFacts()
        facts.feed("\x1b[31mtest tests::broken ... FAILED\x1b[0m")
        facts.feed("test result: FAILED. 2 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.23s")
        self.assertEqual(facts.report(101, 2, {})["outcome"], "test_failure")
        self.assertEqual(facts.summaries[0]["failed"], 1)
        compile_facts = TestFacts()
        compile_facts.feed("error[E0308]: mismatched types")
        self.assertEqual(compile_facts.report(101, 2, {})["outcome"], "compile_failure")
        self.assertEqual(TestFacts().report(0, 1, {})["outcome"], "no_test_summary")
        self.assertEqual(TestFacts().report(1, 1, {})["outcome"], "command_failure")

    def test_command_runs_once_and_preserves_failure_status(self):
        with tempfile.TemporaryDirectory() as directory:
            report_path = Path(directory) / "report.json"
            marker = Path(directory) / "marker"
            code = "from pathlib import Path; import sys; p=Path(sys.argv[1]); p.write_text(p.read_text()+'x' if p.exists() else 'x'); print('test tests::fixture ... FAILED'); sys.exit(23)"
            with contextlib.redirect_stdout(io.StringIO()):
                result = run([sys.executable, "-c", code, str(marker)], report_path)
            self.assertEqual(result, 23)
            self.assertEqual(marker.read_text(), "x")
            report = json.loads(report_path.read_text())
            self.assertEqual(report["attempts_executed"], 1)
            self.assertEqual(report["exit_code"], 23)
            self.assertFalse(report["raw_output_included"])

    def test_output_bounds_and_empty_stdin(self):
        facts = TestFacts()
        for _ in range(20001): facts.feed("test tests::fixture ... ok")
        self.assertTrue(facts.truncated)
        self.assertEqual(len(facts.tests), 20000)
        with tempfile.TemporaryDirectory() as directory:
            with contextlib.redirect_stdout(io.StringIO()):
                code = run([sys.executable, "-c", "import sys; assert sys.stdin.read() == ''"], Path(directory)/"result.json")
            self.assertEqual(code, 0)

if __name__ == "__main__":
    unittest.main()
