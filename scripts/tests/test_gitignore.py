from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


class RuntimeDataIgnoreTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="webcodex-ignore-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = os.environ.copy()
        for name in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"):
            self.env.pop(name, None)
        self.env["GIT_CONFIG_NOSYSTEM"] = "1"
        self.env["GIT_CONFIG_GLOBAL"] = os.devnull
        subprocess.run(
            ["git", "init", "--quiet"], cwd=self.root, env=self.env, check=True,
            capture_output=True,
        )
        source = Path(__file__).resolve().parents[2] / ".gitignore"
        (self.root / ".gitignore").write_bytes(source.read_bytes())

    def ignored(self, paths: list[str]) -> set[str]:
        result = subprocess.run(
            ["git", "-c", f"core.excludesFile={os.devnull}", "check-ignore",
             "--no-index", "--stdin", "-z"],
            cwd=self.root, env=self.env,
            input="\0".join(paths) + "\0", text=True, capture_output=True,
        )
        self.assertIn(result.returncode, (0, 1), result.stderr)
        return set(filter(None, result.stdout.split("\0")))

    def test_data_prefixed_source_files_and_nested_modules_remain_visible(self) -> None:
        paths = [
            "src/data_model.rs",
            "crates/webcodex-store/src/agent_task/database.rs",
            "crates/webcodex-store/src/database/mod.rs",
            "src/data/mod.rs",
            "data_model.rs",
            "database.rs",
            "docs/data-model.md",
        ]
        self.assertEqual(self.ignored(paths), set())

    def test_root_runtime_directories_and_backups_remain_protected(self) -> None:
        paths = [
            "data/webcodex.db", "data/webcodex.db-wal", "data/webcodex.db-shm",
            "data/tool-request-traces/private.json",
            "data/secrets/credentials.json",
            "data-dogfood/webcodex.db", "data_0917/runtime-state.json",
            "data20261002/webcodex.db", "data.backup/private.json",
        ]
        self.assertEqual(self.ignored(paths), set(paths))

    def test_existing_sensitive_configuration_and_legacy_paths_stay_protected(self) -> None:
        paths = [
            ".env", ".webcodex-bootstrap.receipt", "runner.toml", "agent.toml",
            "operator.local.toml", "webcodex.env", "project-registry/private.json",
            "projects.d/private.json", ".codex/auth.json", ".claude/settings.json",
            "ignore/diagnostic.log",
        ]
        self.assertEqual(self.ignored(paths), set(paths))


if __name__ == "__main__":
    unittest.main()
