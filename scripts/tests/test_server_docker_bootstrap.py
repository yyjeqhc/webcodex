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
DIGEST = "sha256:" + "a" * 64
PINNED_IMAGE = f"{assets.SERVER_IMAGE}@{DIGEST}"


class DeploymentAssetTests(unittest.TestCase):
    def test_release_bootstrap_embeds_digest_pinned_compose(self) -> None:
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
            generated = (output / assets.BOOTSTRAP_ASSET).read_text(encoding="utf-8")
            self.assertIn(PINNED_IMAGE, generated)
            self.assertNotIn(f"{assets.SERVER_IMAGE}:latest", generated)
            self.assertIn(f"compose_target={assets.MATERIALIZED_COMPOSE}", generated)
            self.assertIn("cmp -s", generated)
            self.assertEqual(
                assets.sha256_bytes((output / assets.BOOTSTRAP_ASSET).read_bytes()),
                result[assets.BOOTSTRAP_ASSET],
            )

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
    def _workspace(self, *, pull_ok: bool = True) -> tuple[Path, dict[str, str]]:
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

        bin_dir = root / "bin"
        bin_dir.mkdir()
        log = root / "docker.log"
        fake = bin_dir / "docker"
        fake.write_text(
            "#!/bin/sh\n"
            "set -eu\n"
            'printf "%s\\n" "$*" >> "$FAKE_DOCKER_LOG"\n'
            'if [ "$1" = compose ] && [ "$2" = version ]; then exit 0; fi\n'
            'if [ "$1" = compose ] && [ "$4" = config ] && [ "$5" = --images ]; then\n'
            f"  printf '%s\\n' '{PINNED_IMAGE}'\n"
            "  exit 0\n"
            "fi\n"
            'if [ "$1" = compose ] && [ "$4" = pull ]; then\n'
            f"  exit {0 if pull_ok else 23}\n"
            "fi\n"
            'if [ "$1" = compose ] && [ "$4" = up ]; then exit 0; fi\n'
            "exit 0\n",
            encoding="utf-8",
        )
        fake.chmod(fake.stat().st_mode | stat.S_IXUSR)
        env = os.environ.copy()
        env.update(
            {
                "PATH": str(bin_dir) + os.pathsep + env.get("PATH", ""),
                "FAKE_DOCKER_LOG": str(log),
            }
        )
        env.pop("COMPOSE_FILE", None)
        env.pop("WEBCODEX_SERVER_IMAGE", None)
        return root, env

    def _run(self, root: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["sh", assets.BOOTSTRAP_ASSET, "https://webcodex.example.com"],
            cwd=root,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def test_clone_free_bootstrap_materializes_and_uses_pinned_compose(self) -> None:
        root, env = self._workspace()
        result = self._run(root, env)
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
        calls = (root / "docker.log").read_text(encoding="utf-8")
        self.assertIn(f"compose -f {assets.MATERIALIZED_COMPOSE} config --images", calls)
        self.assertIn(f"compose -f {assets.MATERIALIZED_COMPOSE} pull webcodex", calls)
        self.assertIn(f"compose -f {assets.MATERIALIZED_COMPOSE} up -d --no-build --pull never", calls)

    def test_pull_failure_keeps_retryable_compose_but_no_secret_env(self) -> None:
        root, env = self._workspace(pull_ok=False)
        result = self._run(root, env)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((root / assets.MATERIALIZED_COMPOSE).is_file())
        self.assertFalse((root / ".env").exists())

    def test_invalid_url_does_not_materialize_compose(self) -> None:
        root, env = self._workspace()
        result = subprocess.run(
            ["sh", assets.BOOTSTRAP_ASSET, "http://not-https.example.com"],
            cwd=root,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((root / assets.MATERIALIZED_COMPOSE).exists())
        self.assertFalse((root / ".env").exists())
        self.assertFalse((root / "docker.log").exists())

    def test_existing_different_compose_fails_before_docker_or_secret_creation(self) -> None:
        root, env = self._workspace()
        (root / assets.MATERIALIZED_COMPOSE).write_text("different\n", encoding="utf-8")
        result = self._run(root, env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("different content", result.stderr)
        self.assertFalse((root / ".env").exists())
        self.assertFalse((root / "docker.log").exists())


if __name__ == "__main__":
    unittest.main()
