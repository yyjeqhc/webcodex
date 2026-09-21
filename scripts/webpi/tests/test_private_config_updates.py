from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from scripts.webpi import standalone, install_runner_provider


class PrivateConfigurationUpdateTests(unittest.TestCase):
    def test_env_permissions_are_applied_before_secret_bytes(self):
        with tempfile.TemporaryDirectory() as td:
            file = Path(td) / "webpi.env"
            file.write_text("WEBPI_TOKEN=synthetic\n", encoding="utf-8")
            observed = []
            def protect(source, target):
                observed.append((source, target.read_bytes()))
            with patch.object(standalone, "_copy_private_permissions", side_effect=protect):
                standalone._replace_env_values(file, {"WEBPI_PUBLIC_URL": "https://webpi.example.test"})
            self.assertEqual(observed, [(file, b"")])
            self.assertIn("WEBPI_TOKEN=synthetic", file.read_text())

    def test_failed_acl_update_does_not_replace_original(self):
        with tempfile.TemporaryDirectory() as td:
            file = Path(td) / "webpi.env"
            original = b"WEBPI_TOKEN=synthetic\n"
            file.write_bytes(original)
            with patch.object(standalone, "_copy_private_permissions", side_effect=OSError("denied")):
                with self.assertRaises(OSError): standalone._replace_env_values(file, {"WEBPI_PUBLIC_URL": "https://webpi.example.test"})
            self.assertEqual(file.read_bytes(), original)
            self.assertEqual(list(Path(td).iterdir()), [file])

    def test_runner_replacement_and_backup_preserve_protection_before_writing(self):
        with tempfile.TemporaryDirectory() as td:
            file = Path(td) / "runner.toml"
            file.write_bytes(b'token = "synthetic"\n')
            observed = []
            def protect(source, target):
                observed.append(target.read_bytes())
            with patch.object(install_runner_provider, "_copy_private_permissions", side_effect=protect):
                backup = Path(td) / "runner.toml.bak"
                install_runner_provider.backup_private_config(file, backup)
                install_runner_provider.atomic_write(file, b'token = "synthetic"\n# updated\n')
            self.assertEqual(observed, [b"", b""])
            self.assertEqual(backup.read_bytes(), b'token = "synthetic"\n')

    def test_pi_bridge_managed_timeout_upgrade_is_exact_and_rollbackable(self):
        with tempfile.TemporaryDirectory() as td:
            config = Path(td) / "runner.toml"
            config.write_text(
                "server_url = \"http://127.0.0.1:8000\"\n"
                "# WEBPI-PI-BRIDGE-BEGIN\n"
                "[[plugins.providers]]\n"
                "id = \"pi-bridge\"\n"
                "name = \"WebPi Pi Bridge\"\n"
                "command = \"node.exe\"\n"
                "args = [\"plugin.js\"]\n"
                "cwd = \"C:/webpi\"\n"
                "timeout_secs = 30\n"
                "# WEBPI-PI-BRIDGE-END\n",
                encoding="utf-8",
            )
            original = config.read_bytes()
            install_runner_provider.upgrade_timeout(config)
            self.assertIn(b"timeout_secs = 120", config.read_bytes())
            backup = install_runner_provider.backup_path(config)
            self.assertTrue(backup.exists())
            self.assertEqual(backup.read_bytes(), original)
            install_runner_provider.rollback(config)
            self.assertEqual(config.read_bytes(), original)
            self.assertFalse(backup.exists())

    def test_pi_bridge_provider_uses_long_package_mutation_timeout(self):
        with tempfile.TemporaryDirectory() as td:
            config = Path(td) / "runner.toml"
            config.write_text("server_url = \"http://127.0.0.1:8000\"\n", encoding="utf-8")
            install_runner_provider.install(
                config,
                "C:/Program Files/nodejs/node.exe",
                "E:/WebPi/webpi-core/plugins/pi-bridge/dist/plugin.js",
                "E:/WebPi/webpi-core",
            )
            text = config.read_text(encoding="utf-8")
            self.assertIn(
                f"timeout_secs = {install_runner_provider.PI_BRIDGE_TIMEOUT_SECS}",
                text,
            )
            self.assertEqual(install_runner_provider.PI_BRIDGE_TIMEOUT_SECS, 120)


if __name__ == "__main__": unittest.main()
