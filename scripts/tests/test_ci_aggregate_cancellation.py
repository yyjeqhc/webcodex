"""Exercise checked-in aggregate scripts, not a second lane-policy implementation.

GitHub defines cancelled() as workflow cancellation, independently of each
needs.<job>.result. Tests pin the scheduler guard and run actual Bash bodies
against failed/cancelled/skipped children; they do not emulate hosted scheduling.
"""
from pathlib import Path
import os
import re
import shutil
import subprocess
import textwrap
import unittest

from scripts.tests.test_ci_contract_ownership import job

ROOT = Path(__file__).resolve().parents[2]
AGGREGATES = ("test", "test-macos", "test-windows", "test-native")


def aggregate(name: str) -> tuple[str, dict[str, str], str]:
    body = job((ROOT / ".github/workflows/ci.yml").read_text(), name)
    guard = re.search(r"^    if: (.+)$", body, re.MULTILINE)
    if guard is None:
        raise AssertionError(f"{name}: missing scheduler guard")
    env = dict(re.findall(r"^          ([A-Z_]+): \$\{\{ (needs\.[^\n]+) \}\}$", body, re.MULTILINE))
    script = re.search(r"^        run: \|\n((?:          [^\n]*\n|\n)+)", body, re.MULTILINE)
    if script is None:
        raise AssertionError(f"{name}: missing aggregate script")
    return guard.group(1), env, textwrap.dedent(script.group(1))


class AggregateCancellationTests(unittest.TestCase):
    def test_workflow_cancellation_guard_is_independent_of_child_results(self) -> None:
        for name in AGGREGATES:
            with self.subTest(name=name):
                guard, env, _ = aggregate(name)
                # Status function removes implicit success() gating, so failed
                # or skipped dependencies cannot suppress the aggregate.
                self.assertEqual(guard, "${{ !cancelled() }}")
                self.assertNotIn("needs.", guard)
                self.assertGreaterEqual(len(env), 4)
                self.assertNotIn("continue-on-error:", job((ROOT / ".github/workflows/ci.yml").read_text(), name))

    def run_gate(self, name: str, overrides: dict[str, str]) -> subprocess.CompletedProcess[str]:
        _, fields, script = aggregate(name)
        env = os.environ.copy()
        env.update({key: "true" if key.startswith("NEEDS_") else "success" for key in fields})
        env.update(overrides)
        bash = shutil.which("bash")
        if bash is None:
            self.fail("Bash is required to validate Linux aggregate scripts")
        return subprocess.run([bash, "--noprofile", "--norc", "-c", script],
                              env=env, input="", text=True, capture_output=True, timeout=5)

    def test_required_child_failure_cancellation_or_skip_never_passes(self) -> None:
        for name in AGGREGATES:
            self.assertEqual(self.run_gate(name, {}).returncode, 0)
            _, fields, _ = aggregate(name)
            for lane in (key for key in fields if key.endswith("_RESULT")):
                for result in ("failure", "cancelled", "skipped", ""):
                    with self.subTest(aggregate=name, lane=lane, result=result):
                        # Child cancellation is data, not workflow cancellation.
                        self.assertNotEqual(self.run_gate(name, {lane: result}).returncode, 0)

    def test_only_policy_optional_lanes_may_be_skipped(self) -> None:
        optional = {"test-macos": ("CORE", "DESKTOP"),
                    "test-windows": ("CORE", "RUNNER", "PACKAGE", "DESKTOP"),
                    "test-native": ("DOCKER",)}
        for name, lanes in optional.items():
            for lane in lanes:
                with self.subTest(aggregate=name, lane=lane):
                    self.assertEqual(self.run_gate(name, {f"NEEDS_{lane}": "false", f"{lane}_RESULT": "skipped"}).returncode, 0)
                    for result in ("failure", "cancelled", "success"):
                        self.assertNotEqual(self.run_gate(name, {f"NEEDS_{lane}": "false", f"{lane}_RESULT": result}).returncode, 0)


if __name__ == "__main__":
    unittest.main()
