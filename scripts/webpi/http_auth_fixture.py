"""Exercise real WebPi HTTP auth in a temporary loopback deployment; never touch the live service."""
from __future__ import annotations

import hashlib
import json
import os
import socket
import subprocess
import tempfile
import time
from pathlib import Path

from security_smoke import request_json, verify
from standalone import ROOT, CLI, _env_values, _replace_env_values, harden_server_env, isolated_child_environment, terminate_group
from native_identity_checks import startup_checks, api_identity_checks, runner_plugin_checks


def cli_step(label: str, argv: list[str], env: dict[str, str], stdin: str | None = None) -> dict[str, object]:
    result = subprocess.run(argv, cwd=ROOT, env=env, input=stdin, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=45, shell=False)
    if result.returncode:
        # CLI output may include pairing material: report only a fixed step label.
        raise RuntimeError("HTTP authentication fixture CLI step failed: " + label)
    value = json.loads(result.stdout)
    if not isinstance(value, dict):
        raise RuntimeError("HTTP authentication fixture CLI returned malformed JSON: " + label)
    return value


def main() -> int:
    runtime = ROOT / ".webpi-runtime"
    if runtime.is_symlink() or runtime.is_junction() or not runtime.resolve().is_relative_to(ROOT.resolve()):
        raise RuntimeError("fixture runtime directory must stay in the authorized checkout")
    if not CLI.is_file():
        raise RuntimeError("build the WebPi dogfood CLI and server before this integration check")
    runtime.mkdir(exist_ok=True)
    # A real fixture gets entirely fresh state and credentials. Strip all inherited
    # WebCodex configuration so no existing database/listener can be selected.
    env = {key: value for key, value in isolated_child_environment("admin").items() if not key.upper().startswith("WEBPI_")}
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    server = None
    report: dict[str, object] | None = None
    with tempfile.TemporaryDirectory(prefix="auth-fixture-", dir=runtime) as directory:
        fixture = Path(directory)
        env_file = fixture / "webpi.env"
        identities = startup_checks(fixture, env)
        if not all(item["passed"] for item in identities):
            print(json.dumps({"identity_checks": identities, "passed": False}))
            raise RuntimeError("native identity startup acceptance failed")
        foreign_data = fixture / "foreign-product-data"
        env.update({"WEBCODEX_DATA": str(foreign_data), "WEBCODEX_ADDR": "127.0.0.1:0", "WEBCODEX_TOKEN": "synthetic-foreign-not-a-real-key", "WEBCODEX_SHARED_KEY_ENABLED": "true", "WEBCODEX_ALLOW_ANONYMOUS": "true", "WEBCODEX_OAUTH2_ENABLED": "true"})
        try:
            cli_step("initialize", [str(CLI), "server", "init", "--listen", f"127.0.0.1:{port}", "--data-dir", str(fixture / "data"), "--env-file", str(env_file), "--json"], env)
            harden_server_env(env_file)
            _replace_env_values(env_file, {"WEBPI_PUBLIC_URL": "https://webpi.fixture.invalid"})
            assert _env_values(env_file)["WEBPI_SHARED_KEY_ENABLED"] == "false"
            flags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
            server = subprocess.Popen([str(CLI), "server", "run", "--env-file", str(env_file)], cwd=ROOT, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=flags, shell=False)
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                if server.poll() is not None:
                    raise RuntimeError("temporary WebPi server exited before readiness")
                status, value, _ = request_json(origin, "/openapi.json", None, timeout=0.5)
                if status == 200 and isinstance(value, dict) and "paths" in value:
                    break
                time.sleep(0.1)
            else:
                raise RuntimeError("temporary WebPi server readiness deadline expired")

            # Pair a fixture-only client to obtain a real managed user PAT, rather
            # than treating a bootstrap/admin key as evidence for Action auth.
            pairing = cli_step("pairing", [str(CLI), "pairing", "create", "--server-url", origin, "--env-file", str(env_file), "--username", "webpi-auth-fixture", "--client-id", "webpi-auth-fixture", "--display-name", "WebPi auth fixture", "--ttl-secs", "120", "--json"], env)
            code = pairing.get("pairing_code")
            if not isinstance(code, str) or not code.startswith("wc_pair_"):
                raise RuntimeError("fixture pairing did not return a valid one-time code")
            project = fixture / "project"
            project.mkdir()
            login = cli_step("enroll", [str(CLI), "login", origin, "--code-stdin", "--device", "webpi-auth-fixture", "--allowed-root", str(project), "--project", str(project), "--transport", "websocket", "--dir", str(fixture / "connections"), "--json"], env, code + "\n")
            raw_token_path = login.get("user_token_file")
            if not isinstance(raw_token_path, str):
                raise RuntimeError("fixture enrollment did not return a credential file")
            token_path = Path(raw_token_path)
            physical_token = token_path.resolve(strict=True)
            # Rust canonicalization may return a Windows \\?\ drive prefix while
            # pathlib retains a normal drive root. Compare physical ancestor
            # identities, not unequal textual drive spellings, without relaxing
            # containment or following an escaping junction.
            inside_fixture = any(parent.samefile(fixture) for parent in physical_token.parents)
            if not inside_fixture or token_path.is_symlink():
                raise RuntimeError("fixture credential escaped the temporary deployment")
            token = token_path.read_text(encoding="utf-8").strip()
            if not token.startswith("wc_pat_"):
                raise RuntimeError("fixture enrollment did not return a managed PAT")
            report = verify(origin, expected_public_origin="https://webpi.fixture.invalid", token=token)
            checks = report["checks"]
            checks.extend(identities)
            checks.extend(api_identity_checks(origin, token))
            checks.extend(runner_plugin_checks(origin, token, _env_values(env_file)["WEBPI_TOKEN"], login, project, env))
            checks.append({"check": "foreign_data_directory_never_used", "passed": not foreign_data.exists()})
            report["passed"] = all(item["passed"] for item in checks)
            report["fixture"] = "isolated_loopback_new_state_and_managed_pat"
            report["production_service_modified"] = False
            with CLI.open("rb") as binary:
                report["cli_sha256"] = hashlib.file_digest(binary, "sha256").hexdigest()
        finally:
            if server is not None:
                terminate_group(server)
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    raise RuntimeError("temporary WebPi server failed to terminate")
            with socket.socket() as probe:
                probe.settimeout(0.5)
                closed = probe.connect_ex(("127.0.0.1", port)) != 0
            if not closed:
                raise RuntimeError("temporary listener remains open; fixture cleanup failed")
    assert report is not None
    report["fixture_listener_closed"] = True
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        # Our RuntimeError strings are fixed step labels. Do not render subprocess
        # objects, response bodies, token paths, or transport error details.
        message = str(error) if type(error) is RuntimeError else type(error).__name__
        print(json.dumps({"passed": False, "fixture_error": message, "production_service_modified": False}))
        raise SystemExit(1)
