#!/usr/bin/env python3
"""Disposable authenticated MCP -> Server -> WS Runner -> real gofmt smoke.

Build Server/Runner with --profile dogfood first, then run:
  PATH=/path/to/go/bin:$PATH python3 scripts/e2e_go_project_format_ws.py \
    --bin-dir target/dogfood --expected-commit FULL_BUILD_COMMIT

--expected-commit identifies the selected source build, including dogfood builds
with uncommitted changes (reported as dirty). No build, module download, model,
production endpoint, or existing user configuration is used. Private logs and
temporary credentials are deleted on exit and never printed on failure.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from e2e_job_input_ws import InteractiveSmoke
from e2e_session_continuity_ws import ROOT, check

SCOPES = ["runtime:read", "project:read", "project:write", "job:run"]
RUNNER = "go-format-runner"
PROJECT = f"agent:{RUNNER}:fixture"


def build_identity(bin_dir: Path, expected: str) -> None:
    identities = []
    for name in ["webcodex-server", "webcodex-runner"]:
        binary = bin_dir / name
        check(binary.is_file(), "build dogfood Server/Runner binaries first")
        result = subprocess.run([str(binary), "--build-info-json"],
                                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, timeout=10, check=False,
                                env={key: os.environ[key] for key in ["PATH", "LANG"]
                                     if key in os.environ})
        check(result.returncode == 0 and len(result.stdout) <= 128 * 1024,
              f"{name}: bounded build-info probe failed")
        info = json.loads(result.stdout)
        check(info.get("schema_version") == 1 and info.get("binary") == name,
              f"{name}: build-info identity mismatch")
        embedded = info.get("git_commit")
        check(isinstance(embedded, str) and re.fullmatch(r"[0-9a-f]{7,40}", embedded),
              f"{name}: invalid embedded source commit")
        resolved = subprocess.run(["git", "rev-parse", "--verify", "--end-of-options",
                                   embedded + "^{commit}"], cwd=ROOT,
                                  stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, timeout=5, check=False,
                                  env={key: value for key, value in os.environ.items()
                                       if not key.startswith("GIT_")})
        check(resolved.returncode == 0 and resolved.stdout.decode("ascii").strip() == expected,
              f"{name}: selected source commit mismatch; rebuild dogfood binaries")
        info["git_commit"] = expected
        check(isinstance(info.get("git_dirty"), bool) and info.get("built_at"),
              f"{name}: source provenance is missing")
        identities.append(info)
    for field in ["version", "git_commit", "git_dirty", "target", "architecture"]:
        check(identities[0].get(field) == identities[1].get(field),
              "Server/Runner were built from different source identities")
    print(f"BUILD: commit={expected} dirty={str(identities[0]['git_dirty']).lower()}")


class GoFormatSmoke(InteractiveSmoke):
    def __init__(self, bin_dir: Path, root: Path):
        super().__init__(bin_dir, root)
        # Inherit only executable/locale essentials, never provider credentials or
        # user Go/Rust/Runner configuration. All caches and state are disposable.
        keep = {"PATH", "SYSTEMROOT", "WINDIR", "PATHEXT", "LANG",
                "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME",
                "GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM", "RUST_LOG"}
        self.env = {key: value for key, value in self.env.items() if key in keep}
        for name in ["home", "cache", "tmp"]:
            (root / name).mkdir()
        self.env.update({"HOME": str(root / "home"),
                         "XDG_CACHE_HOME": str(root / "cache"),
                         "TMPDIR": str(root / "tmp"), "TMP": str(root / "tmp"),
                         "TEMP": str(root / "tmp"), "GOENV": "off",
                         "GOPROXY": "off", "GOSUMDB": "off", "GOTOOLCHAIN": "local",
                         "GOCACHE": str(root / "cache/go-build"),
                         "GOMODCACHE": str(root / "cache/go-mod"),
                         "WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS": "1"})
        self.handoffs = 0

    def inventory(self) -> dict:
        output = self.call("list_jobs", {"project": PROJECT, "limit": 100})
        check(not output.get("truncated"), "fixture Job inventory unexpectedly truncated")
        return {job["job_id"]: job for job in output["jobs"]}

    def format(self, files: list[str], adapter: str, *, changed=None, fails=False):
        before = self.inventory()
        result = self.raw_tool("project_format", {"project": PROJECT,
            "adapter": adapter, "files": files, "timeout_secs": 20}, self.owner,
            direct=True)["structuredContent"]
        output = result.get("output", {})
        job = output.get("job_id")
        pending = output.get("execution_state") == "pending"
        if pending:
            check(result.get("success") is True and isinstance(job, str),
                  "pending format lost its original Job identity")
            continuation = output.get("continuation", {})
            check(continuation.get("tool") == "observe_jobs" and
                  continuation.get("arguments", {}).get("items", [{}])[0].get("job_id") == job,
                  "pending format continuation does not observe its exact Job")
            self.handoffs += 1
        else:
            check(result.get("success") is (not fails), "unexpected synchronous format outcome")
        after = self.inventory()
        admitted = set(after) - set(before)
        check(len(admitted) == 1, "format did not create exactly one durable Job")
        admitted_job = admitted.pop()
        check(job is None or job == admitted_job, "format response retargeted its admitted Job")
        # Fast real gofmt can finish within the minimum supported 1s Host grace.
        # Its successful synchronous result omits identity; recover that retained
        # Job via exact Project inventory and still verify the public tool flow.
        terminal = self.terminal(admitted_job)
        check(terminal.get("job_id") == admitted_job, "observation changed Job identity")
        if fails:
            check(terminal.get("status") == "failed" and terminal.get("failure_kind"),
                  "syntax error did not retain a bounded failed Job")
            # Failed Jobs conservatively retain unknown mutation evidence; the
            # fixture separately proves unchanged bytes and file identities.
            check(terminal.get("state_changed") is None,
                  "syntax failure incorrectly claimed a known mutation receipt")
        else:
            check(terminal.get("status") == "completed" and terminal.get("exit_code") == 0,
                  "real gofmt Job did not complete")
            check(terminal.get("state_changed") is changed,
                  "retained mutation receipt disagrees with expected effect")
            if not pending:
                check(output.get("state_changed") is changed,
                      "synchronous mutation receipt disagrees with retained Job")

    def run(self):
        self.start_server()
        check(self.post("/api/users/create", {"username": "go-format-owner"},
                        self.bootstrap).get("success"), "temporary user creation failed")
        self.owner = self.pat("go-format-owner", SCOPES)
        enrollment = self.post("/api/agent-tokens/create", {
            "username": "go-format-owner", "client_id": RUNNER,
            "name": "go-format-smoke"}, self.bootstrap)
        check(isinstance(enrollment.get("token"), str), "temporary Runner enrollment failed")
        repo, registry = self.root / "project", self.root / "projects"
        repo.mkdir()
        registry.mkdir()
        (registry / "fixture.toml").write_text(f'id = "fixture"\npath = {json.dumps(str(repo))}\n')
        config = self.root / "runner.toml"
        config.write_text("\n".join([
            f'server_url = "{self.url}"', f'token = {json.dumps(enrollment["token"])}',
            f'client_id = "{RUNNER}"', 'owner = "go-format-owner"',
            'transport = "websocket"', f'project_registry_dir = {json.dumps(str(registry))}',
            '[policy]', 'allow_cwd_anywhere = false',
            f'allowed_roots = {json.dumps([str(repo)])}']) + "\n")
        config.chmod(0o600)
        manifest = b"module example.invalid/format-fixture\n\ngo 1.20\n"
        untouched = b"package fixture\nfunc untouched( ){println(  3 )}\n"
        (repo / "go.mod").write_bytes(manifest)
        (repo / "untouched.go").write_bytes(untouched)
        self.runner = self.launch("webcodex-runner", ["--config", str(config)], self.env)
        self.wait_ready(PROJECT, self.owner)
        self.rpc_id += 1
        discovery = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
                              "method": "tools/list", "params": {}}, self.owner)
        check("project_format" in {item.get("name") for item in
              discovery.get("result", {}).get("tools", [])}, "project_format missing from MCP discovery")
        (repo / "selected.go").write_bytes(b"package fixture\nfunc selected( ){println(  1 )}\n")
        for missing in ["project:write", "job:run"]:
            restricted = self.pat("go-format-owner", [scope for scope in SCOPES if scope != missing])
            before = self.inventory()
            self.rpc_id += 1
            rejected = self.post("/mcp", {"jsonrpc": "2.0", "id": self.rpc_id,
                "method": "tools/call", "params": {"name": "project_format", "arguments": {
                    "project": PROJECT, "adapter": "go", "files": ["selected.go"]}}}, restricted)
            error = rejected.get("error")
            denied = error == "insufficient_scope" or (
                rejected.get("status") == 403 and isinstance(error, str)
                and error.startswith("missing required scope: "))
            check(denied, "missing scope did not produce the canonical scope rejection")
            check(self.inventory() == before, "missing scope dispatched a formatting Job")
        for adapter in ["go", "auto"]:
            selected = repo / "selected.go"
            original = b"package fixture\nfunc selected( ){println(  1 )}\n"
            selected.write_bytes(original)
            expected = subprocess.run([shutil.which("gofmt", path=self.env["PATH"])],
                                      input=original, stdout=subprocess.PIPE,
                                      stderr=subprocess.DEVNULL, env=self.env,
                                      timeout=5, check=True).stdout
            check(expected != original, "Go fixture was already formatted")
            self.format(["selected.go"], adapter, changed=True)
            check(selected.read_bytes() == expected, "MCP formatting differs from installed real gofmt")
            stat = selected.stat()
            self.format(["selected.go"], adapter, changed=False)
            current = selected.stat()
            check(selected.read_bytes() == expected and
                  (stat.st_ino, stat.st_mtime_ns, stat.st_ctime_ns) ==
                  (current.st_ino, current.st_mtime_ns, current.st_ctime_ns),
                  "unchanged formatting rewrote the selected file")
            check((repo / "untouched.go").read_bytes() == untouched and
                  (repo / "go.mod").read_bytes() == manifest,
                  "format changed an unselected source or module marker")
        # Candidate generation for the first file must never commit when the
        # second selected file has a syntax error.
        (repo / "selected.go").write_bytes(original)
        invalid = b"package fixture\nfunc broken( {\n"
        (repo / "broken.go").write_bytes(invalid)
        snapshot = {path.name: (path.read_bytes(), path.stat()) for path in repo.iterdir()}
        self.format(["selected.go", "broken.go"], "go", fails=True)
        for path in repo.iterdir():
            data, stat = snapshot[path.name]
            current = path.stat()
            check(path.read_bytes() == data and
                  (stat.st_ino, stat.st_mtime_ns, stat.st_ctime_ns) ==
                  (current.st_ino, current.st_mtime_ns, current.st_ctime_ns),
                  "syntax error wrote fixture files")
        print("PASS: authenticated MCP/WS explicit Go + auto, selected bytes, unchanged no-write, "
              "unselected/module isolation, syntax-error no-write and exact durable Job observations")
        print(f"HANDOFF: {self.handoffs} pending responses; remaining Jobs completed within supported 1s Host grace")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    parser.add_argument("--expected-commit", required=True,
                        help="full commit embedded in the source-selected dogfood binaries")
    args = parser.parse_args()
    check(sys.version_info >= (3, 10), "Python 3.10+ required")
    check(re.fullmatch(r"[0-9a-f]{40}", args.expected_commit), "full expected source commit required")
    check(shutil.which("gofmt") is not None, "installed real gofmt required on PATH")
    build_identity(args.bin_dir.resolve(), args.expected_commit)
    with tempfile.TemporaryDirectory(prefix="webcodex-go-format-e2e-") as directory:
        smoke = GoFormatSmoke(args.bin_dir.resolve(), Path(directory))
        try:
            smoke.run()
        finally:
            smoke.close()


if __name__ == "__main__":
    main()
