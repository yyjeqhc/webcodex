"""No-build guidance bounds and source-shape tests, independent of wording."""

import json
from pathlib import Path
import unittest

from scripts.check_builtin_guidance_contract import (
    CONTRACT,
    CONTRIBUTORS,
    GuidancePreflightError,
    validate,
)


def contributor(name="FIRST", target="Core", items=("Advice may be reworded.",)):
    lines = "\n".join(f"        {json.dumps(item)}," for item in items)
    return (
        f"pub(super) static {name}: GuidanceContributor = GuidanceContributor {{\n"
        f"    target: GuidanceTarget::{target},\n"
        f"    items: &[\n{lines}\n    ],\n}};\n"
    )


def limits(items=9, chars=320):
    return (
        f"pub const BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS: usize = {items};\n"
        f"pub const BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEM_CHARS: usize = {chars};\n"
    )


class GuidanceContractPreflightTests(unittest.TestCase):
    def test_repository_contributors_fit_canonical_limits(self):
        result = validate(CONTRIBUTORS.read_text(encoding="utf-8"),
                          CONTRACT.read_text(encoding="utf-8"))
        self.assertTrue(result)
        self.assertTrue(all(count > 0 for count in result.values()))

    def test_wording_is_not_a_contract(self):
        original = contributor()
        changed = original.replace("Advice may be reworded.", "Different words express the advice.")
        self.assertEqual(validate(original, limits()), validate(changed, limits()))

    def test_target_comes_from_contributor_not_another_profile_list(self):
        self.assertEqual(validate(contributor(target="NewTaskProfile"), limits()), {"NewTaskProfile": 1})

    def test_budget_combines_contributors_for_the_same_target(self):
        source = contributor(items=("first",)) + contributor(name="SECOND", items=("second",))
        with self.assertRaisesRegex(GuidancePreflightError, "2 items exceed 1"):
            validate(source, limits(items=1))

    def test_budget_does_not_combine_different_targets(self):
        source = contributor(items=("first",)) + contributor(name="SECOND", target="Coding", items=("second",))
        self.assertEqual(validate(source, limits(items=1)), {"Core": 1, "Coding": 1})

    def test_reject_oversize_item(self):
        with self.assertRaisesRegex(GuidancePreflightError, "exceeds 3 chars"):
            validate(contributor(items=("four",)), limits(chars=3))

    def test_reject_empty_and_duplicate_content(self):
        for source in [contributor(items=()), contributor(items=("",)),
                       contributor(items=("same", "same"))]:
            with self.subTest(source=source), self.assertRaises(GuidancePreflightError):
                validate(source, limits())

    def test_reject_duplicate_declaration(self):
        with self.assertRaisesRegex(GuidancePreflightError, "not uniquely found"):
            validate(contributor() + contributor(), limits())

    def test_reject_unknown_source_shape(self):
        for source in ["", contributor().replace('"Advice', 'r#"Advice'),
                       contributor().replace("target:", "other:")]:
            with self.subTest(source=source), self.assertRaises(GuidancePreflightError):
                validate(source, limits())

    def test_reject_missing_or_invalid_limits(self):
        for contract in ["", limits(items=0), limits(chars=0), limits() + limits()]:
            with self.subTest(contract=contract), self.assertRaises(GuidancePreflightError):
                validate(contributor(), contract)

    def test_ci_keeps_preflight_before_server_tests(self):
        text = (Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml").read_text()
        lane = text.split("\n  test-linux-rust:", 1)[1].split("\n  test-linux-tooling:", 1)[0]
        self.assertLess(lane.index("python3 scripts/check_builtin_guidance_contract.py"),
                        lane.index("python3 scripts/ci_rust_test.py"))
        self.assertIn("cargo test --locked", lane)
        self.assertIn("Run experimental Code Mode and receipt tests", lane)


if __name__ == "__main__":
    unittest.main()
