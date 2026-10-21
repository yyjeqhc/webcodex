"""Validate offline discovery labels without asserting a retrieval algorithm wins."""
from __future__ import annotations

import json
import re
import unittest
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "scripts/agent_tool_discovery_cases.json"
CONTRACTS = ROOT / "crates/webcodex-tool-contracts/src"


def strict_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def reject_constant(value):
    raise ValueError(f"non-JSON constant: {value}")


def canonical_definitions():
    """Read names/category/primary scopes from real registry definitions offline.

    This deliberately does not emulate runtime eligibility or ranking. In
    particular, additional scopes, capability checks, admission, and Host
    visibility remain the evaluator/runtime's responsibility.
    """
    definitions = {}
    for path in sorted((CONTRACTS / "tool_definition").glob("*.rs")):
        source = path.read_text(encoding="utf-8")
        matches = list(re.finditer(r'\b(?:def|control_def)\(\s*"([a-z_]+)"', source))
        for index, match in enumerate(matches):
            end = matches[index + 1].start() if index + 1 < len(matches) else len(source)
            body = source[match.end():end]
            category = re.search(r'\bTOOL_CATEGORY_([A-Z_]+)\b', body)
            scope = re.search(
                r'ToolIdempotency::\w+\s*,?\s*},\s*Some\((?:crate::metadata::)?([A-Z_]+)\)',
                body,
            )
            if category and scope:
                definitions[match.group(1)] = {
                    "category": category.group(1).lower(),
                    "scope_constant": scope.group(1),
                    "model_visible": bool(re.search(r'\bModelVisible\s*,', body)),
                }
    authority = (ROOT / "crates/webcodex-core/src/authority.rs").read_text(encoding="utf-8")
    scopes = dict(re.findall(r'pub const SCOPE_([A-Z_]+): &str = "([a-z_:]+)";', authority))
    return definitions, scopes


class ToolDiscoveryCorpusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.corpus = json.loads(
            CORPUS.read_text(encoding="utf-8"),
            object_pairs_hook=strict_object,
            parse_constant=reject_constant,
        )
        cls.cases = cls.corpus["cases"]
        cls.definitions, cls.scopes = canonical_definitions()

    def test_control_definitions_preserve_category_scope_and_visibility(self):
        for name, category, scope, visible in [
            ("create_goal", "goal", "COMMUNICATION_MANAGE", True),
            ("list_agent_identities", "communication", "COMMUNICATION_READ", True),
            ("create_agent_task", "agent_task", "COMMUNICATION_MANAGE", True),
            ("wait_for_agent_events", "agent_wait", "COMMUNICATION_MANAGE", True),
            ("get_agent_wait_state", "agent_wait", "COMMUNICATION_READ", False),
        ]:
            with self.subTest(tool=name):
                self.assertEqual(self.definitions.get(name), {
                    "category": category, "scope_constant": scope, "model_visible": visible,
                })

    def test_versioned_structure_and_evidence_limits(self):
        self.assertEqual(set(self.corpus), {
            "schema_version", "corpus_version", "top_k", "metadata", "cases",
        })
        self.assertEqual(self.corpus["schema_version"], 1)
        self.assertRegex(self.corpus["corpus_version"], r'^\d+\.\d+\.\d+$')
        self.assertEqual(self.corpus["top_k"], 5)
        self.assertEqual(set(self.corpus["metadata"]), {
            "purpose", "evidence_limit", "split_policy", "scope_semantics", "label_semantics",
        })
        for value in self.corpus["metadata"].values():
            self.assertIsInstance(value, str)
            self.assertTrue(value.strip())
        self.assertIn("not live Host proof", self.corpus["metadata"]["evidence_limit"])
        self.assertIn("unordered", self.corpus["metadata"]["label_semantics"])
        self.assertIn("non-bootstrap OAuth", self.corpus["metadata"]["scope_semantics"])
        self.assertIn("non-OAuth", self.corpus["metadata"]["scope_semantics"])

    def test_ids_and_predesignated_quarter_heldout(self):
        self.assertGreaterEqual(len(self.cases), 40)
        self.assertEqual(len(self.cases) % 4, 0)
        ids = [case["id"] for case in self.cases]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual(ids, [f"discovery-{index:03}" for index in range(1, len(ids) + 1)])
        for case in self.cases:
            number = int(case["id"].split("-")[1])
            self.assertEqual(case["split"], "heldout" if number % 4 == 0 else "development")
        self.assertEqual(Counter(case["split"] for case in self.cases)["heldout"] * 4, len(self.cases))

    def test_bilingual_kind_coverage_in_both_splits(self):
        kinds = {"exact_name", "paraphrase", "ambiguous", "no_match", "scope_negative"}
        self.assertEqual({case["kind"] for case in self.cases}, kinds)
        for split in ("development", "heldout"):
            selected = [case for case in self.cases if case["split"] == split]
            self.assertEqual({case["kind"] for case in selected}, kinds)
            self.assertEqual({case["language"] for case in selected}, {"en", "zh"})
        for case in self.cases:
            if case["language"] == "zh":
                self.assertRegex(case["prompt"], r'[\u3400-\u9fff]')

    def test_heldout_distribution_is_balanced_before_baseline_capture(self):
        heldout = [case for case in self.cases if case["split"] == "heldout"]
        counts = Counter(case["language"] for case in heldout)
        self.assertEqual(counts["en"], counts["zh"])
        self.assertIn("Fixed before baseline capture", self.corpus["metadata"]["split_policy"])
        for kind in {case["kind"] for case in self.cases}:
            self.assertEqual({case["language"] for case in heldout if case["kind"] == kind}, {"en", "zh"})
        # Keep translated variants together so tuning sees neither counterpart
        # of a heldout paraphrase intent. No query text or ordering is frozen.
        paraphrases = {}
        for case in self.cases:
            if case["kind"] == "paraphrase":
                intent = tuple(sorted(case["expected_tools"]))
                paraphrases.setdefault(intent, []).append(case)
        for intent, variants in paraphrases.items():
            with self.subTest(intent=intent):
                self.assertEqual({case["language"] for case in variants}, {"en", "zh"})
                self.assertEqual(len({case["split"] for case in variants}), 1)

    def test_cases_use_real_manifest_inputs_and_known_canonical_labels(self):
        valid_scopes = set(self.scopes.values())
        for case in self.cases:
            with self.subTest(case=case["id"]):
                self.assertEqual(set(case), {
                    "id", "split", "language", "kind", "prompt", "request",
                    "allowed_scopes", "expected_tools", "forbidden_tools",
                })
                self.assertIn(case["language"], {"en", "zh"})
                self.assertIsInstance(case["prompt"], str)
                self.assertTrue(case["prompt"].strip())
                request = case["request"]
                self.assertLessEqual(set(request), {"tool_name", "query", "limit", "category", "intent"})
                self.assertEqual(request["limit"], self.corpus["top_k"])
                self.assertEqual("tool_name" in request, case["kind"] == "exact_name")
                if "tool_name" in request:
                    self.assertEqual(set(request), {"tool_name", "limit"})
                    self.assertEqual(case["expected_tools"], [request["tool_name"]])
                else:
                    self.assertIsInstance(request["query"], str)
                    self.assertTrue(request["query"].strip())
                    self.assertLessEqual(len(request["query"]), 200)
                    self.assertFalse(any(ord(char) < 32 for char in request["query"]))
                for field in ("allowed_scopes", "expected_tools", "forbidden_tools"):
                    values = case[field]
                    self.assertIsInstance(values, list)
                    self.assertTrue(all(isinstance(value, str) for value in values))
                    self.assertEqual(len(values), len(set(values)))
                self.assertLessEqual(set(case["allowed_scopes"]), valid_scopes)
                expected = set(case["expected_tools"])
                forbidden = set(case["forbidden_tools"])
                self.assertFalse(expected & forbidden)
                self.assertLessEqual(expected | forbidden, set(self.definitions))
                if case["kind"] == "no_match":
                    self.assertFalse(expected)
                elif case["kind"] != "scope_negative":
                    self.assertTrue(expected)
                for name in expected:
                    definition = self.definitions[name]
                    self.assertTrue(definition["model_visible"])
                    self.assertIn(self.scopes[definition["scope_constant"]], case["allowed_scopes"])
                    if "category" in request:
                        self.assertEqual(definition["category"], request["category"])
                if case["kind"] == "ambiguous":
                    self.assertGreaterEqual(len(expected), 2)
                if case["kind"] == "scope_negative":
                    self.assertTrue(forbidden)
                    for name in forbidden:
                        self.assertNotIn(
                            self.scopes[self.definitions[name]["scope_constant"]],
                            case["allowed_scopes"],
                        )

    def test_json_loader_rejects_duplicate_keys_and_non_json_numbers(self):
        with self.assertRaises(ValueError):
            json.loads('{"id":1,"id":2}', object_pairs_hook=strict_object)
        for value in ("NaN", "Infinity", "-Infinity"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                json.loads(value, parse_constant=reject_constant)


if __name__ == "__main__":
    unittest.main()
