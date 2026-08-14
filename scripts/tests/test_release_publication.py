from __future__ import annotations

import hashlib
import io
import json
import tarfile
import tempfile
import unittest
import urllib.error
from pathlib import Path
from unittest import mock

from scripts import collect_release_bundle as collector
from scripts import release_publication as publication

SOURCE = "a" * 40
REQUEST = "rb_" + "b" * 24
RUN_ID = 123456
VERSION = "0.3.8"
TAG = f"v{VERSION}"


def _state() -> dict:
    return {
        "schema_version": publication.BUILD_STATE_SCHEMA_VERSION,
        "kind": "release-build",
        "repo": collector.DEFAULT_REPO,
        "tag": TAG,
        "source_sha": SOURCE,
        "workflow_file": publication.BUILD_WORKFLOW_FILE,
        "workflow_path": publication.BUILD_WORKFLOW_PATH,
        "request_id": REQUEST,
        "run_name": publication._build_run_name(TAG, REQUEST),
        "dispatch_state": "dispatched",
        "created_at": 1234567890,
        "run_id": None,
        "run_head_sha": None,
        "source_matches": None,
        "run_url": None,
        "run_status": None,
        "run_conclusion": None,
        "last_observed_at": None,
    }


def _run(run_id: int = RUN_ID, source: str = SOURCE) -> dict:
    return {
        "id": run_id,
        "path": publication.BUILD_WORKFLOW_PATH,
        "event": "workflow_dispatch",
        "head_branch": "main",
        "head_sha": source,
        "display_title": publication._build_run_name(TAG, REQUEST),
        "html_url": f"https://github.com/yyjeqhc/webcodex/actions/runs/{run_id}",
        "status": "in_progress",
        "conclusion": None,
    }


def _archive_bytes(platform: str) -> bytes:
    output = io.BytesIO()
    suffix = ".exe" if platform.startswith("win32-") else ""
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for binary in collector.BINARIES:
            payload = f"{platform}:{binary}".encode("ascii")
            info = tarfile.TarInfo(f"{binary}{suffix}")
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))
    return output.getvalue()


def _write_bundle(root: Path) -> dict:
    stem = f"webcodex-v{VERSION}"
    artifact_payload = {}
    checksum_lines = []
    for platform in collector.PLATFORMS:
        name = f"{stem}-{platform}.tar.gz"
        payload = _archive_bytes(platform)
        (root / name).write_bytes(payload)
        digest = hashlib.sha256(payload).hexdigest()
        artifact_payload[platform] = {"filename": name, "sha256": digest}
        checksum_lines.append(f"{digest}  {name}")
    (root / "SHA256SUMS").write_text("\n".join(checksum_lines) + "\n", encoding="ascii")
    (root / "linux-x64-elf.txt").write_text("ELF x64\n", encoding="utf-8")
    (root / "linux-arm64-elf.txt").write_text("ELF arm64\n", encoding="utf-8")
    manifest = {
        "version": VERSION,
        "binaries": list(collector.BINARIES),
        "artifacts": {
            platform: {
                "url": f"https://github.com/{collector.DEFAULT_REPO}/releases/download/{TAG}/{stem}-{platform}.tar.gz",
                "sha256": artifact_payload[platform]["sha256"],
            }
            for platform in collector.PLATFORMS
        },
    }
    (root / "manifest.json").write_text(json.dumps(manifest) + "\n", encoding="utf-8")
    release_build = {
        "tag": TAG,
        "version": VERSION,
        "source_sha": SOURCE,
        "built_at": 1234567890,
        "build_kind": "release",
        "workflow_run_id": RUN_ID,
        "archive_stem": stem,
        "artifacts": artifact_payload,
    }
    (root / "release-build.json").write_text(json.dumps(release_build) + "\n", encoding="utf-8")
    return release_build


class PreflightTests(unittest.TestCase):
    def test_preflight_requires_exact_versions_and_unused_namespaces(self) -> None:
        client = mock.Mock()
        client.fetch_json.side_effect = lambda suffix: {"login": "publisher"} if suffix == "/user" else {}
        with mock.patch.object(publication, "_require_exact_clean_root"), mock.patch.object(
            publication, "_package_versions", return_value=(VERSION, VERSION)
        ), mock.patch.object(publication.collector, "resolve_github_token", return_value="fake"), mock.patch.object(
            publication.collector, "GitHubClient", return_value=client
        ), mock.patch.object(publication, "_github_main_sha", return_value=SOURCE), mock.patch.object(
            publication, "_github_optional_json", return_value=None
        ) as optional_json, mock.patch.object(
            publication, "_fetch_public_json_optional", return_value=None
        ), mock.patch.object(publication, "_git", return_value=""), mock.patch.object(
            publication, "_run_capture", return_value="npm-publisher"
        ) as run_capture:
            summary = publication.preflight_release(
                repo=collector.DEFAULT_REPO,
                version=VERSION,
                source_sha=SOURCE,
                root=Path("/tmp/exact-release-source"),
                timeout=5,
            )
        self.assertTrue(summary["tag_available"])
        self.assertTrue(summary["github_release_available"])
        self.assertTrue(summary["npm_version_available"])
        self.assertEqual(summary["github_user"], "publisher")
        self.assertEqual(summary["npm_user"], "npm-publisher")
        run_capture.assert_called_once_with(
            ["npm", "whoami", "--registry", publication.NPM_REGISTRY], timeout=5
        )
        self.assertEqual(optional_json.call_count, 2)

    def test_preflight_rejects_existing_local_tag(self) -> None:
        client = mock.Mock()
        with mock.patch.object(publication, "_require_exact_clean_root"), mock.patch.object(
            publication, "_package_versions", return_value=(VERSION, VERSION)
        ), mock.patch.object(publication.collector, "resolve_github_token", return_value="fake"), mock.patch.object(
            publication.collector, "GitHubClient", return_value=client
        ), mock.patch.object(publication, "_github_main_sha", return_value=SOURCE), mock.patch.object(
            publication, "_git", return_value=TAG
        ), self.assertRaises(publication.PublicationError):
            publication.preflight_release(
                repo=collector.DEFAULT_REPO,
                version=VERSION,
                source_sha=SOURCE,
                root=Path("/tmp/exact-release-source"),
                timeout=5,
            )

    def test_preflight_fails_before_namespace_checks_on_version_mismatch(self) -> None:
        with mock.patch.object(publication, "_require_exact_clean_root"), mock.patch.object(
            publication, "_package_versions", return_value=("0.3.7", VERSION)
        ), self.assertRaises(publication.PublicationError):
            publication.preflight_release(
                repo=collector.DEFAULT_REPO,
                version=VERSION,
                source_sha=SOURCE,
                root=Path("/tmp/exact-release-source"),
                timeout=5,
            )


class BuildStateTests(unittest.TestCase):
    def test_selects_exact_request_and_rejects_duplicate(self) -> None:
        selected = publication.select_build_run({"workflow_runs": [_run()]}, _state())
        self.assertEqual(selected["id"], RUN_ID)
        with self.assertRaises(publication.PublicationError):
            publication.select_build_run({"workflow_runs": [_run(1), _run(2)]}, _state())

    def test_bound_run_cannot_change_and_source_mismatch_fails_closed(self) -> None:
        state = _state()
        state["run_id"] = RUN_ID
        with self.assertRaises(publication.PublicationError):
            publication._apply_build_run_snapshot(state, _run(RUN_ID + 1))
        state = _state()
        publication._apply_build_run_snapshot(state, _run(source="c" * 40))
        self.assertFalse(state["source_matches"])

    def test_state_round_trip_rejects_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            path = root / "build.json"
            publication._write_state(path, _state())
            self.assertEqual(publication._load_state(path)["request_id"], REQUEST)
            path.unlink()
            target = root / "target.json"
            target.write_text("{}\n", encoding="utf-8")
            path.symlink_to(target)
            with self.assertRaises(publication.PublicationError):
                publication._load_state(path)


class _Response:
    status = 204

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self, _size=-1):
        return b""

    def getcode(self):
        return self.status


class _Opener:
    def __init__(self, outcome):
        self.outcome = outcome

    def open(self, request, timeout):
        if isinstance(self.outcome, Exception):
            raise self.outcome
        return self.outcome


class BuildDispatchTests(unittest.TestCase):
    def _client(self):
        return collector.GitHubClient(collector.DEFAULT_REPO, "fake-token", 5)

    def test_204_is_accepted(self) -> None:
        client = self._client()
        client.opener = _Opener(_Response())
        publication._post_build_dispatch(client, TAG, REQUEST)

    def test_4xx_is_rejected_and_transport_is_unknown(self) -> None:
        client = self._client()
        client.opener = _Opener(urllib.error.HTTPError("https://api.github.test", 422, "bad", {}, None))
        with self.assertRaises(publication.DispatchRejected):
            publication._post_build_dispatch(client, TAG, REQUEST)
        client.opener = _Opener(urllib.error.URLError("lost"))
        with self.assertRaises(publication.DispatchOutcomeUnknown):
            publication._post_build_dispatch(client, TAG, REQUEST)


class _FakeGitHubClient:
    def __init__(self, release: dict):
        self.release = release

    def fetch_json(self, suffix: str):
        if suffix.startswith("/releases/tags/"):
            return self.release
        raise AssertionError(suffix)


class NpmStagingTests(unittest.TestCase):
    def test_stage_npm_uses_existing_bundle_binaries_and_never_cargo(self) -> None:
        summary = {
            "tag": TAG,
            "version": VERSION,
            "source_sha": SOURCE,
            "built_at": 1234567890,
            "build_kind": "release",
            "workflow_run_id": RUN_ID,
            "archive_stem": f"webcodex-v{VERSION}",
            "artifacts": {platform: "a" * 64 for platform in collector.PLATFORMS},
        }
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bundle = root / "bundle"
            source = root / "source"
            output = root / "stage"
            bundle.mkdir()
            source.mkdir()
            (bundle / "manifest.json").write_text("{}\n", encoding="utf-8")
            calls: list[list[str]] = []

            def record(argv, **_kwargs):
                calls.append(list(argv))

            with mock.patch.object(publication, "verify_bundle", return_value=summary), mock.patch.object(
                publication, "_require_exact_clean_root"
            ), mock.patch.object(publication, "_git", return_value=SOURCE), mock.patch.object(
                publication, "_extract_linux_x64_binaries"
            ) as extract, mock.patch.object(publication, "_run_checked", side_effect=record):
                result = publication.stage_npm(
                    repo=collector.DEFAULT_REPO,
                    bundle_dir=bundle,
                    source_root=source,
                    output_dir=output,
                )

        self.assertEqual(result["npm_smoke"], "passed")
        self.assertEqual(len(calls), 2)
        self.assertTrue(calls[0][1].endswith("scripts/stage_npm_release.sh"))
        self.assertTrue(calls[1][1].endswith("scripts/npm_package_smoke.sh"))
        self.assertIn("--binary-dir", calls[1])
        self.assertFalse(any("cargo" in argument for call in calls for argument in call))
        extract.assert_called_once()


class DraftVerificationTests(unittest.TestCase):
    def test_draft_asset_digests_replace_full_redownload(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            meta = _write_bundle(root)
            names = ["SHA256SUMS", *(entry["filename"] for entry in meta["artifacts"].values())]
            assets = []
            for index, name in enumerate(names, 1):
                path = root / name
                assets.append(
                    {
                        "id": index,
                        "name": name,
                        "state": "uploaded",
                        "size": path.stat().st_size,
                        "digest": "sha256:" + collector.sha256_file(path),
                    }
                )
            release = {
                "id": 99,
                "tag_name": TAG,
                "draft": True,
                "prerelease": False,
                "html_url": f"https://github.com/{collector.DEFAULT_REPO}/releases/tag/{TAG}",
                "assets": assets,
            }
            with mock.patch.object(publication.collector, "resolve_github_token", return_value="fake"), mock.patch.object(
                publication.collector, "GitHubClient", return_value=_FakeGitHubClient(release)
            ):
                summary = publication.verify_draft_assets(
                    repo=collector.DEFAULT_REPO,
                    bundle_dir=root,
                    timeout=5,
                )
            self.assertTrue(summary["draft"])
            self.assertEqual(len(summary["assets"]), 6)

            release["assets"][0]["digest"] = "sha256:" + "0" * 64
            with mock.patch.object(publication.collector, "resolve_github_token", return_value="fake"), mock.patch.object(
                publication.collector, "GitHubClient", return_value=_FakeGitHubClient(release)
            ):
                with self.assertRaises(publication.PublicationError):
                    publication.verify_draft_assets(
                        repo=collector.DEFAULT_REPO,
                        bundle_dir=root,
                        timeout=5,
                    )


class NpmPublicationContractTests(unittest.TestCase):
    def test_package_pins_public_npm_registry(self) -> None:
        package = json.loads(Path("npm/webcodex/package.json").read_text(encoding="utf-8"))
        self.assertEqual(package["publishConfig"]["access"], "public")
        self.assertEqual(package["publishConfig"]["registry"], publication.NPM_REGISTRY)


class WorkflowContractTests(unittest.TestCase):
    def test_release_build_exposes_durable_request_id(self) -> None:
        workflow = Path(".github/workflows/release-build.yml").read_text(encoding="utf-8")
        self.assertIn("run-name: Release build ${{ inputs.tag }} ${{ inputs.request_id }}", workflow)
        self.assertIn("^rb_[0-9a-f]{24}$", workflow)
        self.assertIn("default: manual", workflow)


if __name__ == "__main__":
    unittest.main()
