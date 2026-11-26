#!/usr/bin/env python3
"""Exercise v0.5.0 -> v0.5.1 -> candidate using disposable loopback runtimes."""
import argparse
import json
from pathlib import Path
import tempfile
import time

from e2e_session_continuity_ws import Smoke, SCOPES, check, stop


def run(smoke, released, candidate):
    project = "agent:upgrade-runner:fixture"
    smoke.start_server()
    check(smoke.post("/api/users/create", {"username": "owner"}, smoke.bootstrap)["success"],
          "could not create test owner")
    owner = smoke.pat("owner", SCOPES)
    check(smoke.post("/api/users/create", {"username": "other"}, smoke.bootstrap)["success"],
          "could not create second test owner")
    other = smoke.pat("other", SCOPES)
    agent = smoke.post("/api/agent-tokens/create", {"client_id": "upgrade-runner",
                      "username": "owner", "name": "upgrade"}, smoke.bootstrap)
    registry = smoke.root / "projects"
    registry.mkdir()
    repo = smoke.root / "fixture"
    repo.mkdir()
    registration = registry / "fixture.toml"
    registration.write_text(f'id = "fixture"\npath = {json.dumps(str(repo))}\n')
    config = smoke.root / "runner.toml"
    config.touch(mode=0o600)
    config.write_text("\n".join([f'server_url = "{smoke.url}"',
                      f'token = {json.dumps(agent["token"])}', 'client_id = "upgrade-runner"',
                      'owner = "owner"', 'transport = "websocket"',
                      f'project_registry_dir = {json.dumps(str(registry))}',
                      '[policy]', 'allow_raw_shell = true', f'allowed_roots = [{json.dumps(str(repo))}]']))

    last_instance = None

    def runner():
        nonlocal last_instance
        smoke.runner = smoke.launch("webcodex-runner", ["--config", str(config)], smoke.env)
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            check(smoke.runner.poll() is None, "temporary Runner exited")
            rows = call("list_runners", client_id="upgrade-runner",
                        summary_only=False, include_projects=True)["runners"]
            for row in rows:
                if (row["connected"] and row["runner_instance_id"] != last_instance
                        and row["project_inventory"]["sync_state"] == "complete"
                        and any(p["id"] == "fixture" for p in row["projects"])):
                    last_instance = row["runner_instance_id"]
                    return
            time.sleep(0.1)
        raise AssertionError("new Runner did not publish its Project inventory")

    def restart(binaries):
        stop(smoke.runner)
        smoke.runner = None
        stop(smoke.server)
        smoke.bin_dir = binaries
        smoke.start_server()
        runner()

    def call(name, success=True, **params):
        result = smoke.post("/api/tools/call", {"tool": name, "params": params}, owner)
        output = result.get("output", {})
        check(result.get("success") is success, f"{name} failed check ({output.get('error_kind')})")
        return output

    def resume(selector, success):
        result = call("work_on_project", success, project=project, session_id=selector,
                      instruction="Check upgrade continuity")
        if not success:
            check(result["error_kind"] == "session_project_identity_changed", "wrong resume fence")
        return result

    runner()
    call("start_session", project=project)
    original, = call("list_sessions", project=project)["sessions"]
    session, ref = original["session_id"], original["session_ref"]
    for selector in (session, ref):
        check(resume(selector, True)["session_id"] == session, "original selector failed")
    call("post_session_message", session_id=session, kind="decision", message="Keep fixture data")
    call("run_shell", project=project, session_id=session, command="exit 0", purpose="validation")
    summary = call("read_session_summary", session_id=ref)
    check(any(event.get("tool_name") == "run_shell" and event.get("kind") == "tool_call_finished"
              for event in summary["events"]), "missing v0.5.0 validation event")
    check("registration_incarnation" not in registration.read_text(), "expected v0.5.0 registry")
    restart(released)
    migrated = registration.read_bytes()
    check(b"registration_incarnation" in migrated, "release did not upgrade registration")
    for selector in (session, ref):
        resume(selector, False)
    print("Official v0.5.0 ID/Ref resume passed, v0.5.1 rejected both after upgrade.", flush=True)
    current = call("start_session", project=project)["session_id"]
    current_ref = call("read_session_summary", session_id=current)["session_ref"]
    restart(candidate)
    check(resume(current_ref, True)["session_id"] == current, "v0.5.1 binding changed")
    for selector in (session, ref):
        saved = call("read_session_summary", session_id=selector)
        for key in ("session_id", "session_ref", "created_at", "lifecycle", "events"):
            check(saved[key] == summary[key], f"upgrade changed {key}")
        resume(selector, False)
        foreign = smoke.post("/api/tools/call", {
            "tool": "read_session_summary", "params": {"session_id": selector},
        }, other)
        if selector == ref:
            check(foreign.get("status") == 400 and "unknown_session_ref" in foreign["error"],
                  "foreign Ref was not concealed")
        else:
            check(foreign.get("success") is False
                  and foreign["output"].get("error_kind") == "unknown_session_id"
                  and "events" not in foreign["output"], "foreign Session ID was not concealed")
    check(call("list_session_messages", session_id=ref)["messages"][0]["message"]
          == "Keep fixture data", "lost decision")
    fresh = call("work_on_project", project=project, instruction="Continue the saved decision")["session_id"]
    check(fresh != session, "handoff reused stale authority")
    stop(smoke.runner)
    smoke.runner = None
    check(call("read_session_summary", session_id=ref)["events"] == summary["events"],
          "offline history changed")
    check(call("close_session", session_id=ref)["already_closed"] is False, "first close")
    runner()
    check(resume(fresh, True)["session_id"] == fresh, "Runner restart changed the binding")
    restart(candidate)
    check(call("close_session", session_id=ref)["already_closed"] is True, "durable close")
    check(resume(fresh, True)["session_id"] == fresh, "new registration did not survive restart")
    check(call("list_sessions", project=project)["total"] == 3, "duplicate Session")
    check(registration.read_bytes() == migrated, "registration changed across restart")
    print("Candidate retained history, refused old execution, and resumed the new ID after restart.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for version in ("v050", "v051", "candidate"):
        parser.add_argument("--" + version, type=Path, required=True, help="Server/Runner bin directory")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="webcodex-upgrade-") as root:
        fixture = Smoke(args.v050.resolve(), Path(root))
        try:
            run(fixture, args.v051.resolve(), args.candidate.resolve())
        finally:
            fixture.close()
