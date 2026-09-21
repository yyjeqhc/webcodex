from __future__ import annotations
import json
from pathlib import Path
import tempfile
import unittest
from scripts.webpi.config_migration import migrate_text, migrate_env_file
from scripts.webpi.runtime_config import build_non_tunnel_child_env, HARDENED_AUTH_ENV

ROOT = Path(__file__).resolve().parents[3]

class WebPiIdentityIsolationTests(unittest.TestCase):
    def test_all_foreign_and_ambient_product_variables_are_scrubbed(self):
        names = ["WEBCODEX_DATA", "webcodex_addr", "WEBCODEX_PAT", "WEBCODEX_AGENT_TOKEN", "WEBCODEX_ENV_FILE", "WEBCODEX_QUIC_ENABLED", "WEBCODEX_OAUTH2_ISSUER", "WEBCODEX_AUTHORITY_MODE", "WEBCODEX_FUTURE_SETTING", "WEBPI_DATA", "WEBPI_ADDR", "WEBPI_PAT", "WEBPI_ENV_FILE", "NODE_OPTIONS", "PYTHONPATH"]
        child = build_non_tunnel_child_env({**dict.fromkeys(names, "synthetic-never-used"), "PATH": "keep"})
        self.assertEqual(child["PATH"], "keep")
        self.assertTrue(all(name not in child for name in names))
        self.assertEqual({k:v for k,v in child.items() if k.startswith("WEBPI_")}, HARDENED_AUTH_ENV)

    def test_migration_changes_keys_not_secret_or_path_values(self):
        value = "WEBCODEX_TOKEN=synthetic-WEBCODEX_unchanged\nWEBCODEX_DATA=E:/WebPi/state/data\nWEBCODEX_SHARED_KEY_ENABLED=true\n"
        output = migrate_text(value)
        self.assertIn("WEBPI_TOKEN=synthetic-WEBCODEX_unchanged", output)
        self.assertIn("WEBPI_DATA=E:/WebPi/state/data", output)
        self.assertIn("WEBPI_SHARED_KEY_ENABLED=false", output)
        self.assertEqual(migrate_text(output), output)

    def test_conflicting_or_empty_config_never_migrates(self):
        for text in ("WEBCODEX_TOKEN=one\nWEBPI_TOKEN=two\n", "WEBPI_TOKEN=one\nWEBPI_TOKEN=one\n", "WEBCODEX_TOKEN=\n", "WEBCODEX_DATA=x\n", "bad line\n"):
            with self.subTest(text=text):
                with self.assertRaises(ValueError): migrate_text(text)

    def test_migration_is_explicit_and_backup_preserves_exact_bytes(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            file = root / "webpi.env"
            original = b"WEBCODEX_TOKEN=synthetic\r\nWEBCODEX_DATA=unchanged\r\n"
            file.write_bytes(original)
            self.assertFalse(migrate_env_file(file, root)["applied"])
            self.assertEqual(file.read_bytes(), original)
            report = migrate_env_file(file, root, apply=True)
            self.assertTrue(report["applied"])
            self.assertEqual(Path(report["backup"]).read_bytes(), original)
            self.assertIn(b"WEBPI_TOKEN=synthetic", file.read_bytes())
            self.assertFalse(migrate_env_file(file, root, apply=True)["changed"])

    def test_migration_rejects_outside_file_without_changes(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td) / "root"
            root.mkdir()
            file = Path(td) / "outside.env"
            file.write_text("WEBCODEX_TOKEN=synthetic\n", encoding="utf-8")
            with self.assertRaises(ValueError): migrate_env_file(file, root, apply=True)
            self.assertEqual(file.read_text(), "WEBCODEX_TOKEN=synthetic\n")

    def test_product_binary_targets_are_distinct(self):
        import tomllib
        for name, binary in {"Cargo.toml": "webpi-server", "crates/webcodex-cli/Cargo.toml": "webpi", "crates/webcodex-runner/Cargo.toml": "webpi-runner"}.items():
            config = tomllib.loads((ROOT/name).read_text(encoding="utf-8"))
            self.assertIn(binary, [b["name"] for b in config.get("bin", [])])

    def test_protocol_and_token_compatibility_not_cosmetically_renamed(self):
        self.assertIn('"webcodex-plugin-v1"', (ROOT/"crates/webcodex-core/src/plugin.rs").read_text(encoding="utf-8"))
        self.assertIn('"webcodex-plugin-v1"', (ROOT/"npm/plugin-sdk/src/protocol.ts").read_text(encoding="utf-8"))
        self.assertIn('format!("wc_pat_', (ROOT/"src/auth/pat.rs").read_text(encoding="utf-8"))

    def test_desktop_and_upstream_npm_cannot_publish_as_webcodex(self):
        config = json.loads((ROOT/"apps/desktop/src-tauri/tauri.conf.json").read_text())
        self.assertNotEqual(config["identifier"], "dev.webcodex.desktop")
        self.assertFalse(config["bundle"]["active"])
        package = json.loads((ROOT/"npm/webcodex/package.json").read_text())
        self.assertTrue(package["private"])
        self.assertNotIn("webcodex", package["bin"])

if __name__ == "__main__": unittest.main()
