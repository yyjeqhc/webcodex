from __future__ import annotations

import json
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.webpi import standalone


class BootstrapBackupPathTests(unittest.TestCase):
    def test_create_bounded_direct_child_allows_new_child_but_rejects_existing_and_invalid_names(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "backups"
            root.mkdir()

            created = standalone._create_bounded_direct_child_dir(root, "bootstrap-20260923-123")
            self.assertTrue(created.is_dir())
            self.assertEqual(created.parent, root.resolve(strict=True))
            self.assertEqual(created.name, "bootstrap-20260923-123")

            with self.assertRaisesRegex(RuntimeError, "already exists"):
                standalone._create_bounded_direct_child_dir(root, "bootstrap-20260923-123")

            for name in ["", ".", "..", "../escape", "a/b", "a\\b", "候选"]:
                with self.subTest(name=name):
                    with self.assertRaisesRegex(RuntimeError, "child name is invalid"):
                        standalone._create_bounded_direct_child_dir(root, name)

    def test_existing_bounded_child_helper_remains_strict_for_missing_children(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(FileNotFoundError):
                standalone._assert_bounded_child(root, root / "missing")

    def test_bootstrap_create_backup_creates_fresh_backup_child_before_writing_payload(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = root / "installed"
            installed.mkdir()
            for index, name in enumerate(standalone.DEPLOY_ARTIFACT_NAMES, start=1):
                (installed / name).write_bytes(bytes([index]) * (100 + index))

            server_env = root / "webpi.env"
            server_env.write_text("WEBPI_TOKEN=secret-server-token\n", encoding="utf-8")
            runner_config = root / "runner.toml"
            runner_config.write_text('token = "secret-runner-token"\n', encoding="utf-8")
            server_data = root / "server-data"
            server_data.mkdir()
            database = server_data / "webcodex.db"
            connection = sqlite3.connect(database)
            try:
                connection.execute("CREATE TABLE durable_state(id INTEGER PRIMARY KEY, value TEXT)")
                connection.execute("INSERT INTO durable_state(value) VALUES ('ok')")
                connection.commit()
            finally:
                connection.close()

            backup_root = root / "deployment-backups"
            consistency = {"safe_identity": True}
            with (
                patch.object(standalone, "SERVER_ENV", server_env),
                patch.object(standalone, "SERVER_DATA", server_data),
                patch("scripts.webpi.standalone.runner_config", return_value=runner_config),
            ):
                backup_dir = standalone._bootstrap_create_backup(
                    "run",
                    "candidate-real-path-test",
                    consistency,
                    installed_dir=installed,
                    backup_root=backup_root,
                )

            self.assertTrue(backup_dir.is_dir())
            self.assertEqual(backup_dir.parent, backup_root.resolve(strict=True))
            self.assertTrue(backup_dir.name.startswith("bootstrap-"))
            for name in standalone.DEPLOY_ARTIFACT_NAMES:
                self.assertEqual((backup_dir / name).read_bytes(), (installed / name).read_bytes())

            private = backup_dir / "private"
            self.assertEqual((private / "server.env").read_text(encoding="utf-8"), server_env.read_text(encoding="utf-8"))
            self.assertEqual((private / "runner-config").read_text(encoding="utf-8"), runner_config.read_text(encoding="utf-8"))
            restored = sqlite3.connect(private / "webcodex.db")
            try:
                row = restored.execute("SELECT value FROM durable_state WHERE id=1").fetchone()
            finally:
                restored.close()
            self.assertEqual(row, ("ok",))

            manifest = json.loads((backup_dir / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["candidate_id"], "candidate-real-path-test")
            self.assertEqual(manifest["mode"], "run")
            self.assertEqual(manifest["consistency"], consistency)
            self.assertEqual(manifest["private_recovery_payload"]["database_snapshot"], True)
            # Secrets belong only in private payload files, never the manifest.
            encoded_manifest = json.dumps(manifest, sort_keys=True)
            self.assertNotIn("secret-server-token", encoded_manifest)
            self.assertNotIn("secret-runner-token", encoded_manifest)


if __name__ == "__main__":
    unittest.main()
