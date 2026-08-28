from __future__ import annotations

import os
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts import prepare_server_deployment_assets as assets

ROOT = Path(__file__).resolve().parents[2]
BOOTSTRAP = ROOT / "deploy" / "docker" / "bootstrap.sh"
COMPOSE = ROOT / "compose.yaml"
BUILD_COMPOSE = ROOT / "compose.build.yaml"
DOCKERFILE = ROOT / "Dockerfile"
DIGEST = "sha256:" + "a" * 64
PINNED_IMAGE = f"{assets.SERVER_IMAGE}@{DIGEST}"
PUBLIC_URL = "https://webcodex.example.com"
RECEIPT = ".webcodex-bootstrap.receipt"
TOKEN = "b" * 64
SECOND_TOKEN = "c" * 64


class DeploymentAssetTests(unittest.TestCase):
    def test_release_bootstrap_embeds_digest_pinned_compose_without_duplicate_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            output = root / "out"
            result = assets.prepare_assets(
                compose_path=COMPOSE,
                bootstrap_path=BOOTSTRAP,
                output_dir=output,
                digest=DIGEST,
            )
            self.assertEqual(set(result), {assets.BOOTSTRAP_ASSET})
            generated_path = output / assets.BOOTSTRAP_ASSET
            generated = generated_path.read_text(encoding="utf-8")
            self.assertIn(PINNED_IMAGE, generated)
            self.assertNotIn(f"{assets.SERVER_IMAGE}:latest", generated)
            self.assertIn(f"compose_target={assets.MATERIALIZED_COMPOSE}", generated)
            self.assertIn("cmp -s", generated)
            self.assertIn("WEBCODEX_RELEASE_BOOTSTRAP=true", generated)
            self.assertNotIn("release_public_url=", generated)
            self.assertEqual(generated.count("validate_public_url()"), 1)
            self.assertEqual(
                assets.sha256_bytes(generated_path.read_bytes()),
                result[assets.BOOTSTRAP_ASSET],
            )
            subprocess.run(["sh", "-n", generated_path], check=True)

    def test_asset_preparation_rejects_noncanonical_digest(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                assets.prepare_assets(
                    compose_path=COMPOSE,
                    bootstrap_path=BOOTSTRAP,
                    output_dir=Path(temp) / "out",
                    digest="latest",
                )


class BootstrapTests(unittest.TestCase):
    def _fake_tools(self, root: Path) -> dict[str, str]:
        bin_dir = root / "bin"
        bin_dir.mkdir()
        log = root / "docker.log"
        state = root / "container.state"

        docker = bin_dir / "docker"
        docker.write_text(
            "#!/bin/sh\n"
            "set -eu\n"
            'printf "%s\\n" "$*" >> "$FAKE_DOCKER_LOG"\n'
            'args="$*"\n'
            'if [ "$1" = compose ] && [ "$2" = version ]; then exit 0; fi\n'
            'if [ "$1" = inspect ]; then printf "%s\\n" "${FAKE_HEALTH_STATUS:-healthy}"; exit 0; fi\n'
            'case "$args" in\n'
            '  *" config --images"*) printf "%s\\n" "$FAKE_PINNED_IMAGE"; exit 0 ;;\n'
            '  *" config"*) exit 0 ;;\n'
            '  *" pull webcodex"*) exit "${FAKE_PULL_EXIT:-0}" ;;\n'
            '  *" ps -aq webcodex"*|*" ps -q webcodex"*)\n'
            '    if [ -f "$FAKE_CONTAINER_STATE" ]; then printf "fake-container\\n"; fi; exit 0 ;;\n'
            '  *" up "*)\n'
            '    if [ "${FAKE_UP_LEAVES_CONTAINER:-0}" = 1 ]; then : > "$FAKE_CONTAINER_STATE"; fi\n'
            '    if [ "${FAKE_UP_EXIT:-0}" != 0 ]; then exit "$FAKE_UP_EXIT"; fi\n'
            '    : > "$FAKE_CONTAINER_STATE"; exit 0 ;;\n'
            '  *" exec -T webcodex curl "*) exit "${FAKE_OPENAPI_EXIT:-0}" ;;\n'
            '  *" exec -T webcodex sh -lc "*)\n'
            '    if [ "${FAKE_PAIRING_EXIT:-0}" != 0 ]; then exit "$FAKE_PAIRING_EXIT"; fi\n'
            '    printf "wc_pair_test_123\\n"; exit 0 ;;\n'
            '  *" down"*) rm -f "$FAKE_CONTAINER_STATE"; exit "${FAKE_DOWN_EXIT:-0}" ;;\n'
            'esac\n'
            "exit 0\n",
            encoding="utf-8",
        )
        docker.chmod(docker.stat().st_mode | stat.S_IXUSR)

        ss = bin_dir / "ss"
        ss.write_text(
            "#!/bin/sh\n"
            'if [ "${FAKE_PORT_BUSY:-0}" = 1 ]; then printf "LISTEN fake:8080\\n"; fi\n',
            encoding="utf-8",
        )
        ss.chmod(ss.stat().st_mode | stat.S_IXUSR)

        sync = bin_dir / "sync"
        sync.write_text(
            "#!/bin/sh\n"
            'case "${FAKE_SYNC_FAIL_FOR:-}" in\n'
            '  env) case "$*" in *".env."*) exit 31 ;; esac ;;\n'
            '  receipt) case "$*" in *".webcodex-bootstrap.receipt."*) exit 32 ;; esac ;;\n'
            '  receipt_after_env)\n'
            '    case "$*" in *".webcodex-bootstrap.receipt."*) [ -f .env ] && exit 33 ;; esac ;;\n'
            'esac\n'
            "exit 0\n",
            encoding="utf-8",
        )
        sync.chmod(sync.stat().st_mode | stat.S_IXUSR)

        token_count = root / "token.count"
        openssl = bin_dir / "openssl"
        openssl.write_text(
            "#!/bin/sh\n"
            'count=0\n'
            'if [ -f "$FAKE_TOKEN_COUNT" ]; then count=$(cat "$FAKE_TOKEN_COUNT"); fi\n'
            'printf "%s\\n" "$((count + 1))" > "$FAKE_TOKEN_COUNT"\n'
            f"if [ \"$count\" -eq 0 ]; then printf '%s\\n' '{TOKEN}'; else printf '%s\\n' '{SECOND_TOKEN}'; fi\n",
            encoding="utf-8",
        )
        openssl.chmod(openssl.stat().st_mode | stat.S_IXUSR)

        env = os.environ.copy()
        env.update(
            {
                "PATH": str(bin_dir) + os.pathsep + env.get("PATH", ""),
                "FAKE_DOCKER_LOG": str(log),
                "FAKE_CONTAINER_STATE": str(state),
                "FAKE_PINNED_IMAGE": PINNED_IMAGE,
                "FAKE_TOKEN_COUNT": str(token_count),
                "WEBCODEX_BOOTSTRAP_HEALTH_WAIT_SECS": "2",
            }
        )
        env.pop("COMPOSE_FILE", None)
        env.pop("WEBCODEX_SERVER_IMAGE", None)
        env.pop("WEBCODEX_RELEASE_BOOTSTRAP", None)
        return env

    def _generated_workspace(self) -> tuple[Path, dict[str, str], str]:
        root = Path(tempfile.mkdtemp(prefix="webcodex-bootstrap-test-"))
        self.addCleanup(shutil.rmtree, root, True)
        generated = root / "generated"
        assets.prepare_assets(
            compose_path=COMPOSE,
            bootstrap_path=BOOTSTRAP,
            output_dir=generated,
            digest=DIGEST,
        )
        shutil.copy2(generated / assets.BOOTSTRAP_ASSET, root / assets.BOOTSTRAP_ASSET)
        return root, self._fake_tools(root), assets.BOOTSTRAP_ASSET

    def _source_workspace(
        self, *, include_overlay: bool = True, include_dockerfile: bool = True
    ) -> tuple[Path, dict[str, str], str]:
        root = Path(tempfile.mkdtemp(prefix="webcodex-bootstrap-source-test-"))
        self.addCleanup(shutil.rmtree, root, True)
        shutil.copy2(BOOTSTRAP, root / "bootstrap.sh")
        shutil.copy2(COMPOSE, root / "compose.yaml")
        if include_overlay:
            shutil.copy2(BUILD_COMPOSE, root / "compose.build.yaml")
        if include_dockerfile:
            shutil.copy2(DOCKERFILE, root / "Dockerfile")
        return root, self._fake_tools(root), "bootstrap.sh"

    def _run(
        self,
        root: Path,
        env: dict[str, str],
        script: str,
        *args: str,
    ) -> subprocess.CompletedProcess[str]:
        if not args:
            args = (PUBLIC_URL,)
        return subprocess.run(
            ["sh", script, *args],
            cwd=root,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def _receipt(self, root: Path) -> dict[str, str]:
        lines = (root / RECEIPT).read_text(encoding="utf-8").splitlines()
        return dict(line.split("=", 1) for line in lines)

    def _docker_log(self, root: Path) -> str:
        path = root / "docker.log"
        return path.read_text(encoding="utf-8") if path.exists() else ""

    def test_clone_free_bootstrap_commits_secret_waits_for_health_and_creates_pairing(self) -> None:
        root, env, script = self._generated_workspace()
        result = self._run(root, env, script)
        self.assertEqual(result.returncode, 0, result.stderr)

        compose = root / assets.MATERIALIZED_COMPOSE
        self.assertTrue(compose.is_file())
        self.assertIn(PINNED_IMAGE, compose.read_text(encoding="utf-8"))
        env_file = root / ".env"
        self.assertTrue(env_file.is_file())
        self.assertEqual(stat.S_IMODE(env_file.stat().st_mode), 0o600)
        text = env_file.read_text(encoding="utf-8")
        self.assertIn(f"COMPOSE_FILE={assets.MATERIALIZED_COMPOSE}\n", text)
        self.assertIn(f"WEBCODEX_SERVER_IMAGE={PINNED_IMAGE}\n", text)

        receipt = self._receipt(root)
        self.assertEqual(receipt["phase"], "PairingReady")
        self.assertEqual(receipt["public_url"], PUBLIC_URL)
        self.assertEqual(receipt["compose_file"], assets.MATERIALIZED_COMPOSE)
        self.assertNotEqual(receipt["env_sha256"], "-")
        self.assertEqual(stat.S_IMODE((root / RECEIPT).stat().st_mode), 0o600)

        self.assertIn("wc_pair_test_123", result.stdout)
        self.assertIn("WebCodex server is healthy", result.stdout)
        self.assertNotIn("server container started", result.stdout.lower())
        self.assertIn("webcodex login", result.stdout)
        self.assertIn("webcodex runner install --scope user", result.stdout)
        self.assertIn("Do not copy", result.stdout)
        self.assertIn("webcodex connect", result.stdout)

        calls = self._docker_log(root)
        self.assertIn(f"compose -f {assets.MATERIALIZED_COMPOSE} config --images", calls)
        self.assertIn(f"compose -f {assets.MATERIALIZED_COMPOSE} pull webcodex", calls)
        self.assertIn(
            f"compose -f {assets.MATERIALIZED_COMPOSE} up -d --no-build --pull never", calls
        )
        self.assertIn("inspect --format", calls)
        self.assertIn("openapi.json", calls)
        self.assertIn("pairing create", calls)

        second = self._run(root, env, script)
        self.assertNotEqual(second.returncode, 0)
        self.assertIn("installation receipt already exists", second.stderr)

    def test_pull_failure_happens_before_receipt_or_secret(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_PULL_EXIT"] = "23"
        result = self._run(root, env, script)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((root / assets.MATERIALIZED_COMPOSE).is_file())
        self.assertFalse((root / ".env").exists())
        self.assertFalse((root / RECEIPT).exists())

    def test_source_overlay_and_dockerfile_are_preflighted_before_secret(self) -> None:
        root, env, script = self._source_workspace(include_overlay=False)
        result = self._run(root, env, script, PUBLIC_URL, "--build-from-source")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("compose.build.yaml is required", result.stderr)
        self.assertFalse((root / ".env").exists())
        self.assertFalse((root / RECEIPT).exists())

        root2, env2, script2 = self._source_workspace(include_dockerfile=False)
        result2 = self._run(root2, env2, script2, PUBLIC_URL, "--build-from-source")
        self.assertNotEqual(result2.returncode, 0)
        self.assertIn("Dockerfile is required", result2.stderr)
        self.assertFalse((root2 / ".env").exists())
        self.assertFalse((root2 / RECEIPT).exists())

    def test_source_build_uses_same_transaction_state_machine(self) -> None:
        root, env, script = self._source_workspace()
        result = self._run(root, env, script, PUBLIC_URL, "--build-from-source")
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = self._receipt(root)
        self.assertEqual(receipt["mode"], "source")
        self.assertEqual(receipt["phase"], "PairingReady")
        self.assertNotEqual(receipt["overlay_sha256"], "-")
        self.assertNotIn("WEBCODEX_SERVER_IMAGE=", (root / ".env").read_text(encoding="utf-8"))
        self.assertIn("-f compose.build.yaml up -d --build", self._docker_log(root))

    def test_up_failure_preserves_secret_and_resume_reuses_it(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_UP_EXIT"] = "42"
        first = self._run(root, env, script)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual(self._receipt(root)["phase"], "SecretCommitted")
        env_bytes = (root / ".env").read_bytes()

        env["FAKE_UP_EXIT"] = "0"
        resumed = self._run(root, env, script, "resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual(self._receipt(root)["phase"], "PairingReady")
        self.assertEqual((root / ".env").read_bytes(), env_bytes)
        self.assertGreaterEqual(self._docker_log(root).count(" up -d "), 2)

    def test_up_uncertain_container_is_reconciled_without_regenerating_secret(self) -> None:
        root, env, script = self._generated_workspace()
        env.update({"FAKE_UP_EXIT": "44", "FAKE_UP_LEAVES_CONTAINER": "1"})
        first = self._run(root, env, script)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual(self._receipt(root)["phase"], "SecretCommitted")
        env_bytes = (root / ".env").read_bytes()
        up_count = self._docker_log(root).count(" up -d ")

        env["FAKE_UP_EXIT"] = "0"
        resumed = self._run(root, env, script, "resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual((root / ".env").read_bytes(), env_bytes)
        self.assertEqual(self._docker_log(root).count(" up -d "), up_count)

    def test_health_failure_stops_before_pairing_and_resume_finishes(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_HEALTH_STATUS"] = "unhealthy"
        first = self._run(root, env, script)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual(self._receipt(root)["phase"], "ContainerStarted")
        self.assertNotIn("pairing create", self._docker_log(root))
        env_bytes = (root / ".env").read_bytes()

        env["FAKE_HEALTH_STATUS"] = "healthy"
        resumed = self._run(root, env, script, "resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual(self._receipt(root)["phase"], "PairingReady")
        self.assertEqual((root / ".env").read_bytes(), env_bytes)
        self.assertIn("pairing create", self._docker_log(root))

    def test_pairing_failure_is_retryable_from_server_healthy(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_PAIRING_EXIT"] = "47"
        first = self._run(root, env, script)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual(self._receipt(root)["phase"], "ServerHealthy")
        up_count = self._docker_log(root).count(" up -d ")

        env["FAKE_PAIRING_EXIT"] = "0"
        resumed = self._run(root, env, script, "resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual(self._receipt(root)["phase"], "PairingReady")
        self.assertEqual(self._docker_log(root).count(" up -d "), up_count)

    def test_atomic_env_sync_failure_leaves_no_partial_env_and_resume_recovers(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_SYNC_FAIL_FOR"] = "env"
        first = self._run(root, env, script)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual(self._receipt(root)["phase"], "AssetsPrepared")
        self.assertFalse((root / ".env").exists())
        self.assertEqual(list(root.glob(".env.*.tmp")), [])

        env.pop("FAKE_SYNC_FAIL_FOR")
        resumed = self._run(root, env, script, "resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual(self._receipt(root)["phase"], "PairingReady")

    def test_atomic_receipt_sync_failure_never_creates_secret(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_SYNC_FAIL_FOR"] = "receipt"
        result = self._run(root, env, script)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((root / RECEIPT).exists())
        self.assertFalse((root / ".env").exists())
        self.assertEqual(list(root.glob(".webcodex-bootstrap.receipt.*.tmp")), [])

    def test_receipt_failure_after_env_commit_reconciles_without_regenerating_token(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_SYNC_FAIL_FOR"] = "receipt_after_env"
        first = self._run(root, env, script)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual(self._receipt(root)["phase"], "AssetsPrepared")
        env_bytes = (root / ".env").read_bytes()
        self.assertIn(TOKEN.encode(), env_bytes)

        status = self._run(root, env, script, "status")
        self.assertEqual(status.returncode, 0, status.stderr)
        self.assertNotIn(TOKEN, status.stdout)
        self.assertIn("receipt reconciliation pending", status.stdout)
        self.assertNotIn(SECOND_TOKEN, status.stdout)

        env.pop("FAKE_SYNC_FAIL_FOR")
        resumed = self._run(root, env, script, "resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual(self._receipt(root)["phase"], "PairingReady")
        self.assertEqual((root / ".env").read_bytes(), env_bytes)
        self.assertEqual((root / "token.count").read_text(encoding="utf-8"), "1\n")

    def test_status_reports_phase_without_disclosing_admin_token(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_UP_EXIT"] = "49"
        self.assertNotEqual(self._run(root, env, script).returncode, 0)
        status = self._run(root, env, script, "status")
        self.assertEqual(status.returncode, 0, status.stderr)
        self.assertIn("phase:       SecretCommitted", status.stdout)
        self.assertIn("env present: yes", status.stdout)
        self.assertNotIn(TOKEN, status.stdout)
        self.assertNotIn(TOKEN, status.stderr)

    def test_rollback_preserves_secret_and_volume_checkpoint_then_resume_works(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_HEALTH_STATUS"] = "unhealthy"
        self.assertNotEqual(self._run(root, env, script).returncode, 0)
        self.assertEqual(self._receipt(root)["phase"], "ContainerStarted")
        env_bytes = (root / ".env").read_bytes()

        rolled = self._run(root, env, script, "rollback")
        self.assertEqual(rolled.returncode, 0, rolled.stderr)
        self.assertEqual(self._receipt(root)["phase"], "SecretCommitted")
        self.assertEqual((root / ".env").read_bytes(), env_bytes)
        self.assertIn(" down", self._docker_log(root))
        self.assertNotIn(" -v", self._docker_log(root))

        env["FAKE_HEALTH_STATUS"] = "healthy"
        resumed = self._run(root, env, script, "resume")
        self.assertEqual(resumed.returncode, 0, resumed.stderr)
        self.assertEqual(self._receipt(root)["phase"], "PairingReady")

    def test_env_fingerprint_drift_blocks_resume_before_docker_effect(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_UP_EXIT"] = "50"
        self.assertNotEqual(self._run(root, env, script).returncode, 0)
        before = self._docker_log(root)
        with (root / ".env").open("a", encoding="utf-8") as handle:
            handle.write("EXTRA=unexpected\n")

        env["FAKE_UP_EXIT"] = "0"
        resumed = self._run(root, env, script, "resume")
        self.assertNotEqual(resumed.returncode, 0)
        self.assertIn("fingerprint does not match", resumed.stderr)
        self.assertEqual(self._docker_log(root), before)

    def test_busy_port_fails_before_pull_receipt_or_secret(self) -> None:
        root, env, script = self._generated_workspace()
        env["FAKE_PORT_BUSY"] = "1"
        result = self._run(root, env, script)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already listening", result.stderr)
        self.assertFalse((root / RECEIPT).exists())
        self.assertFalse((root / ".env").exists())
        self.assertNotIn("pull webcodex", self._docker_log(root))

    def test_public_url_accepts_only_strict_https_origins(self) -> None:
        invalid = [
            "http://example.com",
            "https://user@example.com",
            "https://example.com/path",
            "https://example.com?query",
            "https://example.com#fragment",
            "https://example.com\nRUST_LOG=trace",
            "https://999.1.1.1",
            "https://[::::]",
            "https://example.com:0",
            "https://example.com:65536",
            "https://-bad.example.com",
        ]
        for value in invalid:
            with self.subTest(value=value):
                root, env, script = self._generated_workspace()
                result = self._run(root, env, script, value)
                self.assertNotEqual(result.returncode, 0, (value, result.stdout, result.stderr))
                self.assertFalse((root / ".env").exists())
                self.assertFalse((root / RECEIPT).exists())
                self.assertEqual(self._docker_log(root), "")
                # Release assets may materialize their deterministic pinned Compose
                # before the canonical body validates arguments; no secret/effect does.
                self.assertTrue((root / assets.MATERIALIZED_COMPOSE).exists())

        valid = [
            "https://example.com",
            "https://example.com:8443",
            "https://127.0.0.1:8443",
            "https://[::1]:8443",
            "https://[2001:db8::1]",
        ]
        for value in valid:
            with self.subTest(value=value):
                root, env, script = self._generated_workspace()
                result = self._run(root, env, script, value)
                self.assertEqual(result.returncode, 0, (value, result.stdout, result.stderr))
                self.assertEqual(self._receipt(root)["phase"], "PairingReady")

    def test_existing_different_generated_compose_fails_before_docker_or_secret(self) -> None:
        root, env, script = self._generated_workspace()
        (root / assets.MATERIALIZED_COMPOSE).write_text("different\n", encoding="utf-8")
        result = self._run(root, env, script)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("different content", result.stderr)
        self.assertFalse((root / ".env").exists())
        self.assertFalse((root / RECEIPT).exists())
        self.assertEqual(self._docker_log(root), "")

    def test_unmanaged_env_is_never_overwritten_or_rolled_back(self) -> None:
        root, env, script = self._generated_workspace()
        (root / ".env").write_text("WEBCODEX_TOKEN=keep-me\n", encoding="utf-8")
        install = self._run(root, env, script)
        self.assertNotEqual(install.returncode, 0)
        self.assertIn("without an installation receipt", install.stderr)
        rollback = self._run(root, env, script, "rollback")
        self.assertNotEqual(rollback.returncode, 0)
        self.assertEqual((root / ".env").read_text(encoding="utf-8"), "WEBCODEX_TOKEN=keep-me\n")


if __name__ == "__main__":
    unittest.main()
