#!/usr/bin/env python3
"""Windows + WSL2 regression for #1448. Supply a stopped, disposable distribution.

python scripts/e2e_windows_wsl_idle.py --distro MyTestDistro [--bin-dir target/dogfood]
The test starts and terminates only the named distribution. It uses /tmp as an
existing project directory, a temporary native Server/Runner, and a loopback WebSocket.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from e2e_session_continuity_ws import ROOT, Smoke, check


def wsl(*args):
    return subprocess.run(["wsl.exe", *args], stdin=subprocess.DEVNULL, capture_output=True,
                          timeout=30, check=True).stdout


def names(*args):
    raw = wsl("--list", "--quiet", *args)
    encoding = "utf-16-le" if b"\x00" in raw else "utf-8-sig"
    return {line.strip().casefold() for line in raw.decode(encoding).lstrip("\ufeff").splitlines()}


def stop_distribution(distro):
    wsl("--terminate", distro)
    deadline = time.monotonic() + 15
    while distro.casefold() in names("--running"):
        check(time.monotonic() < deadline, "distribution did not reach the stopped state")
        time.sleep(0.2)


def run(smoke, distro):
    name = distro.casefold()
    remote = rf"\\wsl.localhost\{distro}\tmp"
    local = smoke.root / "local"
    registry = smoke.root / "registry"
    local.mkdir()
    registry.mkdir()
    # Keep both WebCodex processes' per-user state inside the temporary fixture.
    for key in ["APPDATA", "LOCALAPPDATA", "USERPROFILE"]:
        path = smoke.root / key
        path.mkdir()
        smoke.env[key] = str(path)
    smoke.start_server()
    client = "wsl-smoke"
    check(smoke.post("/api/users/create", {"username": "owner"}, smoke.bootstrap)["success"],
          "test owner creation failed")
    owner = smoke.pat("owner", ["runtime:read", "runner:manage", "project:read", "project:write", "job:run", "session:collaborate"])
    agent = smoke.post("/api/agent-tokens/create", {"client_id": client, "username": "owner",
                      "name": "wsl-smoke"}, smoke.bootstrap)
    config = smoke.root / "runner.toml"

    def configure(roots):
        config.write_text("\n".join([f'server_url = "{smoke.url}"', f'token = {json.dumps(agent["token"])}',
            f'client_id = "{client}"', 'owner = "owner"', 'transport = "websocket"',
            f'project_registry_dir = {json.dumps(str(registry))}', '[policy]', 'allow_raw_shell = true',
            'allow_cwd_anywhere = false', f'allowed_roots = {json.dumps(roots)}']), encoding="utf-8")

    def call(tool, **params):
        response = smoke.post("/api/tools/call", {"tool": tool, "params": params}, owner)
        check(response.get("success") is True, f"{tool} did not succeed")
        return response["output"]

    def stopped():
        check(name not in names("--running"), "WSL was started by an unrelated operation")
        check(smoke.runner.poll() is None and smoke.server.poll() is None, "fixture exited")

    configure([remote, str(local)])
    (registry / "local.toml").write_text(f'id = "local"\npath = {json.dumps(str(local))}\n', encoding="utf-8")
    smoke.runner = smoke.launch("webcodex-runner", ["--config", str(config)], smoke.env)
    local_id, remote_id = [f"agent:{client}:{p}" for p in ("local", "wsl")]
    smoke.wait_ready(local_id, owner)
    stopped()
    call("register_project", client_id=client, id="wsl", name="WSL", path=remote)
    check(name in names("--running"), "explicit WSL registration did not start the test distribution")
    stop_distribution(distro)

    def local_task():
        call("run_shell", project=local_id, command="if ((Get-Item -LiteralPath .).Name -ne 'local') { exit 1 }")
        stopped()

    # Force fresh inventory from persisted registrations with the distro stopped.
    previous = call("list_runners", client_id=client, summary_only=False)["runners"][0]["runner_instance_id"]
    smoke.runner.terminate()
    smoke.runner.wait(timeout=20)
    smoke.runner = smoke.launch("webcodex-runner", ["--config", str(config)], smoke.env)
    deadline = time.monotonic() + 45
    while True:
        stopped()
        rows = call("list_runners", client_id=client, summary_only=False, include_projects=True)["runners"]
        if any(row["connected"] and row["runner_instance_id"] != previous
               and row["project_inventory"]["sync_state"] == "complete"
               and any(p["id"] == "wsl" for p in row["projects"]) for row in rows):
            break
        check(time.monotonic() < deadline, "fresh Runner inventory did not arrive")
        time.sleep(0.2)

    def idle_window():
        for _ in range(12):
            time.sleep(5)
            stopped()

    idle_window()
    local_task()
    call("run_shell", project=remote_id, command="if ((Get-Item -LiteralPath .).Name -ne 'tmp') { exit 1 }")
    check(name in names("--running"), "explicit WSL task did not start its distribution")
    started = call("work_on_project", project=remote_id, instruction="WSL identity regression")
    session_id = started["session_id"]
    stop_distribution(distro)
    idle_window()
    local_task()
    sessions = call("list_sessions", project=remote_id)
    check(any(s["session_id"] == session_id for s in sessions["sessions"]), "passive Session discovery lost the task")
    stopped()
    resumed = call("work_on_project", project=remote_id, session_id=session_id,
                   instruction="Resume the exact WSL task")
    check(resumed["session_id"] == session_id, "WSL root deferral changed Session identity")
    check(name in names("--running"), "explicit Session resume did not observe its WSL root")
    stop_distribution(distro)
    listed = call("list_projects", project=remote_id, limit=1, summary_only=False)
    removed = call("unregister_project", project=remote_id, expected_revision=listed["projects"][0]["revision"])
    check(removed["outcome"] == "unregistered", "selected registration was not removed")
    rows = call("list_runners", client_id=client, summary_only=False)["runners"]
    check(remote in rows[0]["policy"]["allowed_roots"], "unregister unexpectedly revoked a grant")
    idle_window()
    local_task()
    configure([str(local)])
    generation = call("check_runner_config", client_id=client)["current_generation"]
    call("reload_runner_config", client_id=client, expected_generation=generation)
    deadline = time.monotonic() + 15
    while True:
        rows = call("list_runners", client_id=client, summary_only=False)["runners"]
        if rows and rows[0]["policy"]["allowed_roots"] == [str(local)]:
            break
        check(time.monotonic() < deadline, "Server still reports the revoked file access")
        time.sleep(0.2)
    local_task()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--distro", required=True)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/dogfood")
    args = parser.parse_args()
    check(os.name == "nt", "This smoke requires a native Windows host with WSL2")
    check(args.distro and not any(c in args.distro for c in "/\\\r\n") and not args.distro.startswith("-"),
          "Supply a distribution name")
    name = args.distro.casefold()
    check(name in names() and name not in names("--running"), "Supply a stopped test distribution")
    with tempfile.TemporaryDirectory(prefix="webcodex-wsl-smoke-") as root:
        smoke = Smoke(args.bin_dir.resolve(), Path(root))
        try:
            run(smoke, args.distro)
        finally:
            for process in [smoke.runner, smoke.server]:
                if process and process.poll() is None:
                    process.terminate()
                    process.wait(timeout=20)
            for log in smoke.logs:
                log.close()
            if name in names("--running"):
                stop_distribution(args.distro)
    print("PASS: WSL stayed stopped during idle inventory, Windows tasks and revocation")


if __name__ == "__main__":
    main()
