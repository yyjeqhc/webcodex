from __future__ import annotations

import json
import sqlite3
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.webpi import standalone


class RollbackManifestIdentityTests(unittest.TestCase):
    def test_sqlite_schema_identity_tracks_schema_not_row_contents(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = Path(directory) / "webcodex.db"
            connection = sqlite3.connect(db_path)
            try:
                connection.execute("CREATE TABLE demo(id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
                connection.execute("INSERT INTO demo(value) VALUES ('first')")
                connection.commit()
            finally:
                connection.close()

            first = standalone._sqlite_schema_identity(db_path)

            connection = sqlite3.connect(db_path)
            try:
                connection.execute("INSERT INTO demo(value) VALUES ('second')")
                connection.commit()
            finally:
                connection.close()
            rows_only = standalone._sqlite_schema_identity(db_path)
            self.assertEqual(rows_only["schema_sha256"], first["schema_sha256"])

            connection = sqlite3.connect(db_path)
            try:
                connection.execute("CREATE INDEX idx_demo_value ON demo(value)")
                connection.commit()
            finally:
                connection.close()
            changed = standalone._sqlite_schema_identity(db_path)
            self.assertNotEqual(changed["schema_sha256"], first["schema_sha256"])
            self.assertGreaterEqual(int(changed["schema_version"]), int(first["schema_version"]))

    def test_consistency_identity_contains_only_bounded_hashes_and_versions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            server_env = root / "webpi.env"
            server_env.write_text("WEBPI_TOKEN=super-secret-value\n", encoding="utf-8")
            runner_config = root / "runner.toml"
            runner_config.write_text('token = "runner-secret-value"\n', encoding="utf-8")
            enrollment = root / "manifest.json"
            enrollment.write_text(
                json.dumps({"version": 1, "runner_config": str(runner_config)}),
                encoding="utf-8",
            )
            db_path = root / "webcodex.db"
            connection = sqlite3.connect(db_path)
            try:
                connection.execute("CREATE TABLE durable_state(id INTEGER PRIMARY KEY)")
                connection.commit()
            finally:
                connection.close()
            pi_bridge = root / "plugin.js"
            pi_bridge.write_text("export default {}\n", encoding="utf-8")
            package_json = root / "package.json"
            package_json.write_text(
                json.dumps({
                    "name": "@webpi/pi-bridge-plugin",
                    "version": "0.1.0",
                    "dependencies": {"@earendil-works/pi-coding-agent": "0.85.1"},
                }),
                encoding="utf-8",
            )

            identity = standalone._capture_rollback_consistency_identity(
                enrollment_manifest=enrollment,
                server_env=server_env,
                runner_config_path=runner_config,
                database_path=db_path,
                pi_bridge=pi_bridge,
                pi_package=package_json,
            )
            self.assertTrue(standalone._validate_rollback_consistency_identity(identity))
            encoded = json.dumps(identity, sort_keys=True)
            self.assertNotIn("super-secret-value", encoded)
            self.assertNotIn("runner-secret-value", encoded)
            self.assertEqual(identity["plugin"]["package_version"], "0.1.0")
            self.assertEqual(identity["plugin"]["pi_coding_agent_version"], "0.85.1")
            self.assertEqual(len(identity["configuration"]["server_env"]["sha256"]), 64)
            self.assertEqual(len(identity["database_schema"]["schema_sha256"]), 64)

    def test_binary_build_identity_parser_is_strict(self) -> None:
        fake = Path("C:/webpi/webpi.exe")
        good = subprocess.CompletedProcess(
            [str(fake), "--version"],
            0,
            stdout="webpi 0.4.1 (commit aabbccddeeff00112233445566778899aabbccdd, dirty=false, built_at=1234567890)\n",
            stderr="",
        )
        with patch("scripts.webpi.standalone.subprocess.run", return_value=good):
            identity = standalone._binary_build_identity(fake)
        self.assertEqual(identity["version"], "0.4.1")
        self.assertEqual(identity["git_commit"], "aabbccddeeff00112233445566778899aabbccdd")
        self.assertFalse(identity["git_dirty"])
        self.assertEqual(identity["built_at"], 1234567890)

        bad = subprocess.CompletedProcess(
            [str(fake), "--version"],
            0,
            stdout="webpi version unknown\n",
            stderr="",
        )
        with patch("scripts.webpi.standalone.subprocess.run", return_value=bad):
            with self.assertRaisesRegex(RuntimeError, "build identity is invalid"):
                standalone._binary_build_identity(fake)

    def test_consistency_fence_rejects_changed_environment(self) -> None:
        expected = {
            "configuration": {
                "enrollment_manifest": {
                    "schema_version": 1,
                    "sha256": "1" * 64,
                    "size_bytes": 10,
                },
                "server_env": {"sha256": "2" * 64, "size_bytes": 20},
                "runner_config": {"sha256": "3" * 64, "size_bytes": 30},
            },
            "database_schema": {
                "user_version": 0,
                "schema_version": 1,
                "schema_sha256": "4" * 64,
            },
            "plugin": {
                "provider_id": "pi-bridge",
                "package_name": "@webpi/pi-bridge-plugin",
                "package_version": "0.1.0",
                "pi_coding_agent_version": "0.85.1",
                "package_manifest": {"sha256": "5" * 64, "size_bytes": 40},
                "entrypoint": {"sha256": "6" * 64, "size_bytes": 50},
            },
        }
        manifest = {"consistency": expected}
        changed = json.loads(json.dumps(expected))
        changed["database_schema"]["schema_sha256"] = "7" * 64
        with patch(
            "scripts.webpi.standalone._capture_rollback_consistency_identity",
            return_value=changed,
        ):
            with self.assertRaisesRegex(RuntimeError, "consistency fence mismatch"):
                standalone._verify_current_rollback_consistency(manifest)


if __name__ == "__main__":
    unittest.main()
