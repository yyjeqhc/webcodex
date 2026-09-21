from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))

from scripts.webpi.runtime_config import (
    TunnelConfigError,
    build_tunnel_child_env,
    inspect_tunnel_configuration,
    load_tunnel_credentials,
    save_tunnel_credentials,
)


VALID_ID = "tunnel_" + "a" * 32
VALID_KEY = "test-runtime-key"
LEGACY_ID = "tunnel_" + "b" * 32


class TunnelCredentialIsolationTests(unittest.TestCase):
    def test_legacy_webcodex_environment_is_never_used_as_webpi_input(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            env = {
                "CONTROL_PLANE_TUNNEL_ID": LEGACY_ID,
                "CONTROL_PLANE_API_KEY": "legacy-key",
                "WEBCODEX_TUNNEL_CLIENT_BIN": r"C:\legacy\tunnel-client.exe",
            }
            self.assertIsNone(load_tunnel_credentials(Path(td), env))

    def test_webpi_environment_maps_only_into_tunnel_child(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            env = {
                "WEBPI_CONTROL_PLANE_TUNNEL_ID": VALID_ID,
                "WEBPI_CONTROL_PLANE_API_KEY": VALID_KEY,
                "WEBPI_TUNNEL_CLIENT_BIN": r"C:\webpi\tunnel-client.exe",
                "CONTROL_PLANE_TUNNEL_ID": LEGACY_ID,
                "CONTROL_PLANE_API_KEY": "legacy-key",
                "WEBCODEX_TUNNEL_CLIENT_BIN": r"C:\legacy\tunnel-client.exe",
                "OPENAI_ADMIN_KEY": "admin-secret",
                "OPENAI_API_KEY": "api-secret",
                "WEBPI_TOKEN": "global-bootstrap-secret",
                "PATH": r"C:\Windows\System32",
            }
            credentials = load_tunnel_credentials(Path(td), env)
            self.assertIsNotNone(credentials)
            child = build_tunnel_child_env(env, credentials)
            self.assertEqual(child["CONTROL_PLANE_TUNNEL_ID"], VALID_ID)
            self.assertEqual(child["CONTROL_PLANE_API_KEY"], VALID_KEY)
            self.assertEqual(child["WEBPI_TUNNEL_CLIENT_BIN"], r"C:\webpi\tunnel-client.exe")
            self.assertEqual(child["PATH"], r"C:\Windows\System32")
            for name in (
                "WEBPI_CONTROL_PLANE_TUNNEL_ID",
                "WEBPI_CONTROL_PLANE_API_KEY",
                "OPENAI_ADMIN_KEY",
                "OPENAI_API_KEY",
                "WEBPI_TOKEN",
                "WEBCODEX_TUNNEL_CLIENT_BIN",
            ):
                self.assertNotIn(name, child)

    def test_partial_webpi_environment_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            with self.assertRaises(TunnelConfigError):
                load_tunnel_credentials(
                    Path(td),
                    {"WEBPI_CONTROL_PLANE_TUNNEL_ID": VALID_ID},
                )

    def test_saved_pair_wins_over_environment_and_invalid_file_never_falls_back(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            state = Path(td)
            save_tunnel_credentials(state, VALID_ID, VALID_KEY)
            credentials = load_tunnel_credentials(
                state,
                {
                    "WEBPI_CONTROL_PLANE_TUNNEL_ID": LEGACY_ID,
                    "WEBPI_CONTROL_PLANE_API_KEY": "env-key",
                },
            )
            self.assertEqual(credentials.tunnel_id, VALID_ID)
            self.assertEqual(credentials.api_key, VALID_KEY)

            config = state / "secrets" / "openai-tunnel.json"
            config.write_text(json.dumps({"tunnel_id": "bad", "api_key": VALID_KEY}), encoding="utf-8")
            with self.assertRaises(TunnelConfigError):
                load_tunnel_credentials(
                    state,
                    {
                        "WEBPI_CONTROL_PLANE_TUNNEL_ID": LEGACY_ID,
                        "WEBPI_CONTROL_PLANE_API_KEY": "env-key",
                    },
                )

    def test_safe_inspection_reports_source_without_secret_values(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            state = Path(td)
            legacy = inspect_tunnel_configuration(
                state,
                {
                    "CONTROL_PLANE_TUNNEL_ID": LEGACY_ID,
                    "CONTROL_PLANE_API_KEY": "legacy-key",
                },
            )
            self.assertEqual(legacy, {"configured": False, "source": "none"})

            environment = inspect_tunnel_configuration(
                state,
                {
                    "WEBPI_CONTROL_PLANE_TUNNEL_ID": VALID_ID,
                    "WEBPI_CONTROL_PLANE_API_KEY": VALID_KEY,
                },
            )
            self.assertEqual(environment, {"configured": True, "source": "environment"})

            save_tunnel_credentials(state, VALID_ID, VALID_KEY)
            file_status = inspect_tunnel_configuration(state, {})
            self.assertEqual(file_status, {"configured": True, "source": "file"})

            config = state / "secrets" / "openai-tunnel.json"
            config.write_text('{"tunnel_id":"bad","api_key":"hidden"}', encoding="utf-8")
            invalid = inspect_tunnel_configuration(
                state,
                {
                    "WEBPI_CONTROL_PLANE_TUNNEL_ID": VALID_ID,
                    "WEBPI_CONTROL_PLANE_API_KEY": VALID_KEY,
                },
            )
            self.assertEqual(invalid["configured"], False)
            self.assertEqual(invalid["source"], "invalid-file")
            self.assertNotIn("hidden", json.dumps(invalid))

    def test_save_rejects_invalid_or_empty_values(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            state = Path(td)
            for tunnel_id, api_key in (("", VALID_KEY), ("bad", VALID_KEY), (VALID_ID, "")):
                with self.subTest(tunnel_id=tunnel_id, api_key=bool(api_key)):
                    with self.assertRaises(TunnelConfigError):
                        save_tunnel_credentials(state, tunnel_id, api_key)


if __name__ == "__main__":
    unittest.main()
