from __future__ import annotations

import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest.mock import patch

from scripts.webpi.standalone import configured_public_url, harden_server_env, live_unknown_bearer_status, normalize_public_url, _env_values, _replace_env_values


class PublicUrlAndAuthHardeningTests(unittest.TestCase):
    def test_normalize_requires_https_origin_only(self) -> None:
        self.assertEqual(normalize_public_url("https://piforme.vip/"), "https://piforme.vip")
        self.assertEqual(normalize_public_url(" https://piforme.vip:8443 "), "https://piforme.vip:8443")
        for invalid in (
            "http://piforme.vip",
            "https://user@example.com",
            "https://piforme.vip/openapi.json",
            "https://piforme.vip?x=1",
            "https://piforme.vip/#fragment",
            "piforme.vip",
        ):
            with self.subTest(invalid=invalid):
                with self.assertRaises(ValueError):
                    normalize_public_url(invalid)

    def test_env_updates_preserve_bootstrap_secret_and_deduplicate_keys(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            env_file = Path(td) / "webpi.env"
            env_file.write_text(
                "WEBPI_TOKEN=keep-secret\n"
                "WEBPI_SHARED_KEY_ENABLED=true\n"
                "WEBPI_ALLOW_ANONYMOUS=true\n"
                "WEBPI_PUBLIC_URL=https://old.example.com\n"
                "WEBPI_PUBLIC_URL=https://duplicate.example.com\n",
                encoding="utf-8",
            )
            harden_server_env(env_file)
            _replace_env_values(env_file, {"WEBPI_PUBLIC_URL": "https://piforme.vip"})
            content = env_file.read_text(encoding="utf-8")
            self.assertIn("WEBPI_TOKEN=keep-secret", content)
            self.assertEqual(content.count("WEBPI_PUBLIC_URL="), 1)
            self.assertEqual(configured_public_url(env_file), "https://piforme.vip")
            values = _env_values(env_file)
            self.assertEqual(values["WEBPI_SHARED_KEY_ENABLED"], "false")
            self.assertEqual(values["WEBPI_ALLOW_ANONYMOUS"], "false")
            self.assertEqual(values["WEBPI_OAUTH2_SHARED_KEY_BRIDGE"], "false")
            self.assertEqual(values["WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED"], "false")
            self.assertEqual(values["WEBPI_PUBLIC_ACTIONS_ONLY"], "true")
            self.assertEqual(values["WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_ENABLED"], "true")
            self.assertEqual(values["WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_MAX"], "120")
            self.assertEqual(values["WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_WINDOW_SECS"], "60")
            self.assertEqual(values["WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_PENALTY_SECS"], "60")

    def test_live_auth_probe_distinguishes_401_from_accepted_fake_bearer(self) -> None:
        denied = urllib.error.HTTPError(
            "http://127.0.0.1:56542/api/actions/runtime_status",
            401,
            "unauthorized",
            {},
            None,
        )
        with patch("scripts.webpi.standalone.urllib.request.urlopen", side_effect=denied):
            self.assertEqual(live_unknown_bearer_status(), 401)

        class Accepted:
            status = 200

            def __enter__(self):
                return self

            def __exit__(self, _exc_type, _exc, _tb):
                return False

        with patch("scripts.webpi.standalone.urllib.request.urlopen", return_value=Accepted()):
            self.assertEqual(live_unknown_bearer_status(), 200)


if __name__ == "__main__":
    unittest.main()
