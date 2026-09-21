from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))

from scripts.webpi.runtime_config import save_tunnel_credentials
from scripts.webpi.standalone import (
    _merge_windows_persistent_environment,
    isolated_child_environment,
    tunnel_process_spec,
)


VALID_ID = "tunnel_" + "c" * 32


class StandaloneEnvironmentIsolationTests(unittest.TestCase):
    def test_server_and_runner_scrub_tunnel_and_global_openai_secrets(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            env = {
                "WEBPI_CONTROL_PLANE_TUNNEL_ID": VALID_ID,
                "WEBPI_CONTROL_PLANE_API_KEY": "webpi-key",
                "CONTROL_PLANE_TUNNEL_ID": "tunnel_" + "d" * 32,
                "CONTROL_PLANE_API_KEY": "legacy-key",
                "WEBPI_TUNNEL_CLIENT_BIN": r"C:\legacy\tunnel-client.exe",
                "OPENAI_ADMIN_KEY": "admin",
                "OPENAI_API_KEY": "api",
                "WEBPI_TOKEN": "global-bootstrap",
                "PATH": r"C:\Windows\System32",
            }
            for role in ("server", "runner", "admin"):
                with self.subTest(role=role):
                    child = isolated_child_environment(role, Path(td), env)
                    self.assertEqual(child["PATH"], r"C:\Windows\System32")
                    for name in (
                        "WEBPI_CONTROL_PLANE_TUNNEL_ID",
                        "WEBPI_CONTROL_PLANE_API_KEY",
                        "CONTROL_PLANE_TUNNEL_ID",
                        "CONTROL_PLANE_API_KEY",
                        "WEBPI_TUNNEL_CLIENT_BIN",
                        "OPENAI_ADMIN_KEY",
                        "OPENAI_API_KEY",
                        "WEBPI_TOKEN",
                    ):
                        self.assertNotIn(name, child)

    def test_windows_persistent_environment_refresh_replaces_stale_path_without_losing_other_state(self) -> None:
        merged = _merge_windows_persistent_environment(
            {"PATH": "stale", "KEEP": "value"},
            r"C:\\Windows\\System32;C:\\Program Files\\nodejs",
            r"C:\\Users\\user\\bin",
            {"JAVA_HOME": r"C:\\Java", "MAVEN_HOME": r"C:\\Maven"},
        )
        self.assertEqual(
            merged["PATH"],
            r"C:\\Windows\\System32;C:\\Program Files\\nodejs;C:\\Users\\user\\bin",
        )
        self.assertEqual(merged["KEEP"], "value")
        self.assertEqual(merged["JAVA_HOME"], r"C:\\Java")
        self.assertEqual(merged["MAVEN_HOME"], r"C:\\Maven")

    def test_tunnel_role_uses_webpi_saved_credentials_not_legacy_environment(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            state = Path(td)
            client = state / "tunnel-client.exe"
            client.write_bytes(b"test-client")
            save_tunnel_credentials(state, VALID_ID, "saved-webpi-key", str(client.resolve()))
            child = isolated_child_environment(
                "tunnel",
                state,
                {
                    "CONTROL_PLANE_TUNNEL_ID": "tunnel_" + "d" * 32,
                    "CONTROL_PLANE_API_KEY": "legacy-key",
                    "OPENAI_API_KEY": "broad-key",
                    "WEBPI_TOKEN": "global-bootstrap",
                },
            )
            self.assertEqual(child["CONTROL_PLANE_TUNNEL_ID"], VALID_ID)
            self.assertEqual(child["CONTROL_PLANE_API_KEY"], "saved-webpi-key")
            self.assertNotIn("OPENAI_API_KEY", child)
            self.assertNotIn("WEBPI_TOKEN", child)

    def test_tunnel_process_spec_never_places_credentials_on_argv(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            state = Path(td)
            client = state / "tunnel-client.exe"
            client.write_bytes(b"test-client")
            save_tunnel_credentials(state, VALID_ID, "saved-webpi-key", str(client.resolve()))
            spec = tunnel_process_spec(
                state,
                {
                    "CONTROL_PLANE_TUNNEL_ID": "tunnel_" + "d" * 32,
                    "CONTROL_PLANE_API_KEY": "legacy-key",
                    "PATH": r"C:\Windows\System32",
                },
                cli=Path(r"C:\WebPi\webcodex.exe"),
                server_env=Path(r"C:\WebPi\state\webpi.env"),
            )
            joined = " ".join(spec["argv"])
            self.assertNotIn(VALID_ID, joined)
            self.assertNotIn("saved-webpi-key", joined)
            self.assertEqual(spec["env"]["CONTROL_PLANE_TUNNEL_ID"], VALID_ID)
            self.assertEqual(spec["env"]["CONTROL_PLANE_API_KEY"], "saved-webpi-key")
            self.assertEqual(
                spec["argv"],
                [
                    r"C:\WebPi\webcodex.exe",
                    "server",
                    "tunnel",
                    "--provider",
                    "openai",
                    "--env-file",
                    r"C:\WebPi\state\webpi.env",
                    "--json",
                    "--stop-on-stdin-eof",
                ],
            )

    def test_tunnel_role_fails_closed_without_explicit_or_webpi_local_client(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            state = Path(td)
            save_tunnel_credentials(state, VALID_ID, "saved-webpi-key")
            with self.assertRaises(RuntimeError):
                isolated_child_environment("tunnel", state, {}, runtime_root=state)

    def test_tunnel_role_fails_closed_when_webpi_credentials_are_missing(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            with self.assertRaises(RuntimeError):
                isolated_child_environment(
                    "tunnel",
                    Path(td),
                    {
                        "CONTROL_PLANE_TUNNEL_ID": "tunnel_" + "d" * 32,
                        "CONTROL_PLANE_API_KEY": "legacy-key",
                    },
                )


if __name__ == "__main__":
    unittest.main()
