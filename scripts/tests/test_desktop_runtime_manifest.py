from __future__ import annotations
import importlib.util
import json
from pathlib import Path
import unittest
import sys

SCRIPT = Path(__file__).resolve().parents[1] / "desktop_runtime_manifest.py"
spec = importlib.util.spec_from_file_location("desktop_runtime_manifest", SCRIPT)
manifest = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = manifest
spec.loader.exec_module(manifest)

class DesktopRuntimeManifestTests(unittest.TestCase):
    def builds(self, ranges=((1, 2), (1, 3), (2, 4))):
        return [{"schema_version": 1, "binary": name, "version": "0.5.0", "git_commit": str(index) * 12, "git_dirty": False,
                 "desktop_runtime_contract": {"min_generation": pair[0], "max_generation": pair[1]}, "future_field": True}
                for index, (name, pair) in enumerate(zip(manifest.BINARIES, ranges))]

    def test_manifest_uses_advertised_common_range_not_a_copied_generation(self):
        value = manifest.from_builds(self.builds(), "0.5.0")
        self.assertEqual(value["desktop_runtime_contract"], {"min_generation": 2, "max_generation": 2})

    def test_different_source_revisions_are_not_a_contract_failure(self):
        self.assertEqual(manifest.from_builds(self.builds(), "0.5.0")["runtime_version"], "0.5.0")

    def test_disjoint_or_invalid_ranges_fail_release_validation(self):
        for ranges in [((1, 1), (2, 2), (2, 2)), ((0, 1), (1, 1), (1, 1)), ((3, 1), (1, 1), (1, 1))]:
            with self.subTest(ranges=ranges), self.assertRaises(manifest.ManifestError):
                manifest.from_builds(self.builds(ranges), "0.5.0")

    def test_unknown_schema_wrong_binary_and_wrong_official_release_version_fail(self):
        for key, value in [("schema_version", 2), ("binary", "wrong"), ("version", "0.6.0")]:
            builds = self.builds(); builds[0][key] = value
            with self.subTest(key=key), self.assertRaises(manifest.ManifestError):
                manifest.from_builds(builds, "0.5.0")

    def test_additive_fields_are_accepted_but_missing_fields_not_inferred(self):
        value = manifest.from_builds(self.builds(), "0.5.0")
        value["future_field"] = "ignored"
        self.assertEqual(manifest.validate(value, "0.5.0")["schema_version"], 1)
        del value["desktop_runtime_contract"]
        with self.assertRaises(manifest.ManifestError): manifest.validate(value, "0.5.0")

    def test_booleans_are_not_numeric_generations(self):
        with self.assertRaises(manifest.ManifestError): manifest.contract({"min_generation": True, "max_generation": 1})

if __name__ == "__main__": unittest.main()
