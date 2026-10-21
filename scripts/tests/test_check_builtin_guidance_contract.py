"""No-build unit tests for fail-closed guidance source/test preflight."""

from pathlib import Path
import unittest

from scripts.check_builtin_guidance_contract import (
    CONTRACT,
    CONTRIBUTORS,
    TESTS,
    GuidancePreflightError,
    validate,
)


class GuidanceContractPreflightTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = CONTRIBUTORS.read_text(encoding="utf-8")
        cls.tests = TESTS.read_text(encoding="utf-8")
        cls.contract = CONTRACT.read_text(encoding="utf-8")

    def check(self, source=None, tests=None, contract=None):
        return validate(
            self.source if source is None else source,
            self.tests if tests is None else tests,
            self.contract if contract is None else contract,
        )

    def test_baseline_contains_all_canonical_rust_assertions(self):
        result = self.check()
        self.assertEqual(result["direct_items"], 7)
        self.assertEqual(result["host_items"], 8)
        self.assertGreaterEqual(result["direct_phrases"], 18)
        self.assertGreaterEqual(result["host_phrases"], 50)

    def test_duplicate_assertion_phrases_are_legal_in_canonical_rust_tests(self):
        needle = '        "binary files",'
        self.assertEqual(self.tests.count(needle), 1)
        tests = self.tests.replace(needle, needle + chr(10) + needle, 1)
        self.assertGreater(self.check(tests=tests)["direct_phrases"], 18)

    def test_reject_missing_direct_literal_without_compilation(self):
        broken = self.source.replace("binary files", "other materials", 1)
        self.assertNotEqual(self.source, broken)
        with self.assertRaisesRegex(GuidancePreflightError, "binary files"):
            self.check(source=broken)

    def test_reject_missing_host_literal_without_compilation(self):
        broken = self.source.replace("Promise.allSettled", "several promises", 1)
        with self.assertRaisesRegex(GuidancePreflightError, "Promise.allSettled"):
            self.check(source=broken)

    def test_reject_new_canonical_literal_assertion_without_guidance(self):
        original = '        "binary files",'
        self.assertEqual(self.tests.count(original), 1)
        missing = "guidance preflight missing-literal sentinel"
        self.assertNotIn(missing, self.source)
        changed = self.tests.replace(
            original,
            original + f'\n        "{missing}",',
            1,
        )
        with self.assertRaisesRegex(GuidancePreflightError, missing):
            self.check(tests=changed)

    def test_reject_host_items_above_target(self):
        needle = 'pub(super) static HOST_CONTINUATION:'
        assert needle in self.source
        head, tail = self.source.split(needle, 1)
        start = tail.index("items: &[")
        changed = head + needle + tail[:start] + tail[start:].replace(
            "items: &[",
            'items: &[\n        "Extra guidance field",\n        "Another guidance field",',
            1,
        )
        with self.assertRaisesRegex(GuidancePreflightError, "host: 10 items exceed 9"):
            self.check(source=changed)

    def test_reject_oversize_single_item(self):
        needle = 'items: &[\n        "Inspect'
        self.assertIn(needle, self.source)
        changed = self.source.replace(
            needle,
            'items: &[\n        "' + ("A" * 321) + '",\n        "Inspect',
            1,
        )
        with self.assertRaisesRegex(GuidancePreflightError, "320 chars"):
            self.check(source=changed)

    def test_reject_unknown_rust_literal_syntax(self):
        anchor = "pub(super) static DIRECT: GuidanceContributor ="
        self.assertEqual(self.source.count(anchor), 1)
        head, direct_and_rest = self.source.split(anchor, 1)
        # Corrupt the literal layout rather than an incidental guidance sentence.
        changed = head + anchor + direct_and_rest.replace(
            'items: &[\n        "',
            'items: &[\n        r#"',
            1,
        )
        self.assertNotEqual(changed, self.source)
        with self.assertRaisesRegex(GuidancePreflightError, "unsupported Rust literal"):
            self.check(source=changed)

    def test_reject_contract_constant_mismatch(self):
        changed = self.contract.replace(
            "BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS: usize = 9;",
            "BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS: usize = 7;",
        )
        self.assertNotEqual(self.contract, changed)
        with self.assertRaisesRegex(GuidancePreflightError, "host: 8 items exceed 7"):
            self.check(contract=changed)

    def test_reject_missing_exact_rust_assertion_function(self):
        changed = self.tests.replace(
            "fn host_code_mode_strategy_is_bounded_guidance_only() {",
            "fn deleted_assertion() {",
        )
        with self.assertRaisesRegex(GuidancePreflightError, "cannot locate"):
            self.check(tests=changed)

    def test_ci_runs_preflight_before_server_cargo_without_skipping_tests(self):
        text = (Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml").read_text()
        lane = text.split("\n  test-linux-rust:", 1)[1].split(
            "\n  test-linux-tooling:", 1
        )[0]
        preflight = "python3 scripts/check_builtin_guidance_contract.py"
        cargo = "python3 scripts/ci_rust_test.py"
        self.assertIn("if: matrix.shard == 'server'", lane)
        self.assertIn(preflight, lane)
        self.assertIn(cargo, lane)
        self.assertLess(lane.index(preflight), lane.index(cargo))
        self.assertIn("cargo test --locked", lane)
        self.assertIn("Run experimental Code Mode and receipt tests", lane)


if __name__ == "__main__":
    unittest.main()
