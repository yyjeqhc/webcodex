from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from scripts.webpi.runtime_config import HARDENED_AUTH_ENV, build_non_tunnel_child_env
from scripts.webpi.standalone import harden_server_env, normalize_public_url, _replace_env_values


class DeploymentSecurityHardeningTests(unittest.TestCase):
    def test_inherited_auth_flags_and_case_variant_secrets_cannot_override_webpi(self):
        child = build_non_tunnel_child_env({"Path": "keep-path", "webcodex_token": "synthetic-bootstrap", "OpenAI_Api_Key": "synthetic-api", "Cloudflare_Api_Token": "synthetic-cloudflare", "WebCodex_Allow_Anonymous": "true", "WEBPI_SHARED_KEY_ENABLED": "true", "WEBPI_PUBLIC_URL": "https://wrong.example"})
        self.assertEqual(child["Path"], "keep-path")
        for key in ("webcodex_token", "OpenAI_Api_Key", "Cloudflare_Api_Token", "WebCodex_Allow_Anonymous", "WEBPI_PUBLIC_URL"):
            self.assertNotIn(key, child)
        for key, value in HARDENED_AUTH_ENV.items():
            self.assertEqual(child[key], value)

    def test_empty_bootstrap_configuration_fails_closed_without_modifying_file(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "webpi.env"
            for contents in ("WEBPI_ALLOW_ANONYMOUS=false\n", "WEBPI_TOKEN=\n"):
                file.write_text(contents, encoding="utf-8")
                with self.assertRaises(RuntimeError):
                    harden_server_env(file)
                self.assertEqual(file.read_text(encoding="utf-8"), contents)

    def test_public_origin_controls_are_rejected_before_url_normalization(self):
        for value in ("https://web\npi.example", "https://webpi.example\r\n", "https://webpi.\texample"):
            with self.assertRaises(ValueError):
                normalize_public_url(value)

    def test_environment_updates_reject_line_injection(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "webpi.env"
            file.write_text("WEBPI_TOKEN=synthetic-bootstrap\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                _replace_env_values(file, {"WEBPI_PUBLIC_URL": "https://example.com\nWEBCODEX_ALLOW_ANONYMOUS=true"})
            self.assertNotIn("ALLOW_ANONYMOUS", file.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
