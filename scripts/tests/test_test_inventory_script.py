from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
INVENTORY_SCRIPT = REPO_ROOT / "scripts" / "test_inventory.sh"


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


class TestInventoryScriptTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        (self.root / "scripts").mkdir()
        shutil.copyfile(INVENTORY_SCRIPT, self.root / "scripts" / "test_inventory.sh")

        write_text(
            self.root / "src" / "lib.rs",
            """#[test]\nfn root_test() {\n    std::thread::sleep(std::time::Duration::from_millis(1));\n}\n""",
        )
        write_text(
            self.root / "crates" / "webcodex-runner" / "src" / "lib.rs",
            """#[tokio::test]\n#[ignore = \"process fixture\"]\nasync fn runner_test() {\n    let _endpoint = \"127.0.0.1:0\";\n    std::env::set_var(\"WEBCODEX_FIXTURE_TOKEN\", \"SECRET_SENTINEL\");\n}\n""",
        )
        write_text(
            self.root / "crates" / "webcodex-cli" / "tests" / "help.rs",
            """#[test]\nfn cli_help() {}\n""",
        )

        # These files deliberately remain outside the Git index. A workspace-wide
        # inventory should not count build output or arbitrary local scratch files.
        write_text(
            self.root / "target" / "generated.rs",
            """#[test]\nfn generated_test() {}\n""",
        )
        write_text(
            self.root / "crates" / "webcodex-runner" / "tests" / "untracked.rs",
            """#[test]\nfn untracked_test() {\n    std::env::set_var(\"UNTRACKED_SECRET\", \"UNTRACKED_SENTINEL\");\n}\n""",
        )

        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(
            [
                "git",
                "add",
                "scripts/test_inventory.sh",
                "src/lib.rs",
                "crates/webcodex-runner/src/lib.rs",
                "crates/webcodex-cli/tests/help.rs",
            ],
            cwd=self.root,
            check=True,
        )

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def run_inventory(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", "scripts/test_inventory.sh", *args],
            cwd=self.root,
            check=False,
            text=True,
            capture_output=True,
        )

    def area_rows(self, output: str) -> tuple[list[str], dict[str, list[str]]]:
        lines = output.splitlines()
        marker = "[inventory] area summary (tab-separated)"
        marker_index = lines.index(marker)
        header = lines[marker_index + 1].split("\t")
        rows: dict[str, list[str]] = {}
        for line in lines[marker_index + 2 :]:
            if not line:
                break
            fields = line.split("\t")
            rows[fields[0]] = fields[1:]
        return header, rows

    def test_scans_all_tracked_workspace_rust_and_groups_areas(self) -> None:
        result = self.run_inventory()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("scope: Git-tracked Rust files across the workspace", result.stdout)
        self.assertIn("rust files: 3", result.stdout)
        self.assertIn("#[test]: 2", result.stdout)
        self.assertIn("#[tokio::test]: 1", result.stdout)

        header, rows = self.area_rows(result.stdout)
        self.assertEqual(
            header,
            [
                "area",
                "rust_files",
                "test",
                "tokio_test",
                "ignore",
                "sleep",
                "timeout",
                "loopback_or_listener",
                "env_mutation",
                "test_env_lock",
            ],
        )
        self.assertEqual(
            list(rows),
            ["crates/webcodex-cli", "crates/webcodex-runner", "webcodex"],
        )
        self.assertEqual(rows["webcodex"], ["1", "1", "0", "0", "1", "0", "0", "0", "0"])
        self.assertEqual(
            rows["crates/webcodex-runner"],
            ["1", "0", "1", "1", "0", "0", "1", "1", "0"],
        )
        self.assertEqual(
            rows["crates/webcodex-cli"],
            ["1", "1", "0", "0", "0", "0", "0", "0", "0"],
        )
        self.assertNotIn("generated.rs", result.stdout)
        self.assertNotIn("untracked.rs", result.stdout)

    def test_details_report_locations_without_source_values(self) -> None:
        result = self.run_inventory("--details")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertRegex(
            result.stdout,
            r"crates/webcodex-runner/src/lib\.rs:\d+:env_mutation",
        )
        self.assertRegex(
            result.stdout,
            r"crates/webcodex-runner/src/lib\.rs:\d+:loopback_or_listener",
        )
        self.assertNotIn("SECRET_SENTINEL", result.stdout)
        self.assertNotIn("WEBCODEX_FIXTURE_TOKEN", result.stdout)
        self.assertNotIn("UNTRACKED_SENTINEL", result.stdout)
        self.assertNotIn("SECRET_SENTINEL", result.stderr)


if __name__ == "__main__":
    unittest.main()
