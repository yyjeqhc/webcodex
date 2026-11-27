#!/usr/bin/env python3
"""Resume original v0.5.0 Sessions on a candidate, directly and via official v0.5.1."""
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import http.client
import json
from pathlib import Path
import secrets
import sqlite3
import tempfile
from urllib.parse import parse_qs, urlencode, urlsplit
import time

from e2e_session_continuity_ws import Smoke, SCOPES, check, stop


def oauth_client(smoke, owner):
    """Authorize once on v0.5.0. All later MCP calls use this same access token."""
    redirect = "http://127.0.0.1/callback"
    client = smoke.post("/api/oauth/clients/create", {
        "name": "upgrade fixture", "redirect_uris": [redirect], "allowed_scopes": SCOPES,
    }, owner)
    check(client.get("success"), "could not create fixture OAuth client")
    verifier = secrets.token_urlsafe(48)
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
    common = {"client_id": client["client"]["client_id"], "redirect_uri": redirect,
              "resource": smoke.url + "/mcp"}
    parameters = common | {"response_type": "code", "scope": " ".join(SCOPES),
                           "code_challenge": challenge, "code_challenge_method": "S256",
                           "state": secrets.token_urlsafe(16)}
    conn = http.client.HTTPConnection("127.0.0.1", smoke.port, timeout=10)
    try:
        conn.request("GET", "/oauth/authorize?" + urlencode(parameters),
                     headers={"Authorization": "Bearer " + owner})
        response = conn.getresponse()
        check(response.status == 302, f"OAuth authorization failed ({response.status})")
        query = parse_qs(urlsplit(response.getheader("Location")).query)
        check(query.get("state") == [parameters["state"]] and "code" in query, "OAuth code missing")
        response.read()
        conn.request("POST", "/oauth/token", urlencode(common | {
            "grant_type": "authorization_code", "code": query["code"][0],
            "client_secret": client["client_secret"], "code_verifier": verifier,
        }), {"Content-Type": "application/x-www-form-urlencoded"})
        response = conn.getresponse()
        check(response.status == 200, f"OAuth exchange failed ({response.status})")
        return json.load(response)["access_token"]
    finally:
        conn.close()


def run(smoke, released, candidate):
    project = "agent:upgrade-runner:fixture"
    smoke.env.update({"WEBCODEX_OAUTH2_ENABLED": "true", "WEBCODEX_OAUTH2_ISSUER": smoke.url})
    smoke.start_server()
    check(smoke.post("/api/users/create", {"username": "owner"}, smoke.bootstrap)["success"],
          "could not create test owner")
    owner = smoke.pat("owner", SCOPES + ["account:manage"])
    oauth = oauth_client(smoke, owner)
    limited = smoke.pat("owner", ["runtime:read", "session:collaborate"])
    check(smoke.post("/api/users/create", {"username": "other"}, smoke.bootstrap)["success"],
          "could not create second test owner")
    other = smoke.pat("other", SCOPES)
    agent = smoke.post("/api/agent-tokens/create", {"client_id": "upgrade-runner",
                      "username": "owner", "name": "upgrade"}, smoke.bootstrap)
    registry = smoke.root / "projects"
    registry.mkdir()
    repo = smoke.root / "fixture"
    repo.mkdir()
    workdir = repo / "validation"
    workdir.mkdir()
    (repo / "AGENTS.md").write_text("Preserve upgrade fixture data.\n")
    registration = registry / "fixture.toml"
    registration.write_text(f'id = "fixture"\npath = {json.dumps(str(repo))}\n')
    other_repo = smoke.root / "other-project"
    other_repo.mkdir()
    (registry / "other.toml").write_text(f'id = "other"\npath = {json.dumps(str(other_repo))}\n')
    config = smoke.root / "runner.toml"
    config.touch(mode=0o600)
    config.write_text("\n".join([f'server_url = "{smoke.url}"',
                      f'token = {json.dumps(agent["token"])}', 'client_id = "upgrade-runner"',
                      'owner = "owner"', 'transport = "websocket"',
                      f'project_registry_dir = {json.dumps(str(registry))}',
                      '[policy]', 'allow_raw_shell = true', f'allowed_roots = {json.dumps([str(repo), str(other_repo)])}']))

    last_instance = None

    def runner(*, enabled=True):
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
                        and (not enabled or any(p["id"] == "fixture" for p in row["projects"]))):
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

    def resume(selector, success=True):
        result = call("work_on_project", success, project=project, session_id=selector,
                      instruction="Check upgrade continuity")
        if not success:
            check(result["error_kind"] == "session_project_identity_changed", "wrong resume fence")
        return result

    runner()
    call("start_session", project=project, execution_context={"default_cwd": "validation"})
    original, = call("list_sessions", project=project)["sessions"]
    session, ref = original["session_id"], original["session_ref"]
    for selector in (session, ref):
        check(resume(selector, True)["session_id"] == session, "original selector failed")
    call("post_session_message", session_id=session, kind="decision", message="Keep fixture data")
    call("run_shell", project=project, session_id=session, command="exit 0", purpose="validation")
    call("post_session_message", session_id=session, kind="todo", message="Review fixture", requires_ack=True)
    old_shell = call("open_session_shell", project=project, session_id=session)["shell_id"]
    call("run_job", project=project, session_id=session, command="echo old-job >> old-job.log")
    marker = workdir / "old-job.log"
    deadline = time.monotonic() + 10
    while not marker.exists() and time.monotonic() < deadline:
        time.sleep(0.05)
    check(marker.read_text() == "old-job\n", "original Job did not execute once")
    second_legacy = call("start_session", project=project, deny_shell_tools=True)["session_id"]
    closed = call("start_session", project=project)["session_id"]
    call("close_session", session_id=closed)
    smoke.tool("read_session_summary", {"session_id": ref}, oauth)
    messages = call("list_session_messages", session_id=ref)["messages"]
    summary = call("read_session_summary", session_id=ref)
    check(any(event.get("tool_name") == "run_shell" and event.get("kind") == "tool_call_finished"
              for event in summary["events"]), "missing v0.5.0 validation event")
    check("registration_incarnation" not in registration.read_text(), "expected v0.5.0 registry")
    # Interrupt an actual pending Runner request after proving the old command started.
    pending_id = call("start_session", project=project)["session_id"]
    pending_shell = call("open_session_shell", project=project, session_id=pending_id)["shell_id"]
    pending_marker = repo / "old-request.log"
    with ThreadPoolExecutor(max_workers=1) as pool:
        pending = pool.submit(smoke.post, "/api/tools/call", {"tool": "execute_session_shell",
                              "params": {"project": project, "session_id": pending_id, "shell_id": pending_shell,
                                         "command": "echo entered >> old-request.log && sleep 60"}}, owner)
        deadline = time.monotonic() + 10
        while not pending_marker.exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        check(pending_marker.exists() and not pending.done(), "old request never reached the Runner")
        stop(smoke.runner)
        smoke.runner = None
        check(pending.result().get("success") is False, "interrupted Shell request reported success")
    current = None
    if released:
        restart(released)
        for selector in (session, ref):
            resume(selector, False)
        print("Official v0.5.1 rejected both original selectors.", flush=True)
        current = call("start_session", project=project)["session_id"]
    restart(candidate)
    migrated = registration.read_bytes()
    check(b"registration_incarnation" in migrated, "registration was not upgraded")
    if current:
        check(resume(current)["session_id"] == current, "v0.5.1 binding changed")

    def binding(session_id):
        # Observe the committed field without mutating the fixture or invoking a live tool.
        database = smoke.root / "data" / "webcodex.db"
        with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as db:
            row, = db.execute("SELECT state_json FROM workflow_sessions WHERE session_id=?", (session_id,))
        return json.loads(row[0]).get("project_identity")

    def denied(tool, token, **params):
        result = smoke.post("/api/tools/call", {"tool": tool, "params": params}, token)
        check(result.get("success") is False or result.get("status") in (400, 403, 404),
              f"{tool} unexpectedly admitted restricted request")
        return result

    def check_history(after_live_use=False):
        saved = call("read_session_summary", session_id=ref)
        for key in ("session_id", "session_ref", "created_at", "project", "mode", "guards",
                    "execution_context", "title", "lifecycle"):
            check(saved.get(key) == summary.get(key), f"upgrade changed {key}")
        events = saved["events"]
        if after_live_use:
            # Project-file instructions are fetched from the Runner on resume.
            check(saved["project_instructions"]["files"] == summary["project_instructions"]["files"],
                  "resumed Session did not reload its original Project instructions")
            events = [e for e in events if e["event_id"] in original_events]
        check(events == summary["events"], "upgrade changed historical events or validation evidence")
        check(call("list_session_messages", session_id=ref)["messages"] == messages,
              "messages, Todo or ACK state changed")

    original_events = {e["event_id"] for e in summary["events"]}
    check_history()
    call("list_sessions", project=project)
    for selector in (session, ref):
        foreign = denied("read_session_summary", other, session_id=selector)
        if selector == ref:
            check(foreign.get("status") == 400 and "unknown_session_ref" in foreign["error"],
                  "foreign Session Ref was not concealed")
        else:
            check(foreign["output"].get("error_kind") == "unknown_session_id"
                  and "events" not in foreign["output"], "foreign history was exposed")
    for token in (other, limited):
        for selector in (session, ref):
            denied("work_on_project", token, project=project, session_id=selector, instruction="Resume")
    wrong_project = denied("work_on_project", owner, project="agent:upgrade-runner:other", session_id=ref,
                           instruction="Resume")
    check(wrong_project["output"].get("error_kind") == "session_project_mismatch",
          "wrong registered Project was not fenced")
    check(binding(session) is None, "wrong Project triggered binding")
    denied("work_on_project", owner, project=project, session_id=closed, instruction="Resume")
    check(binding(closed) is None, "closed legacy Session acquired a binding")
    stop(smoke.runner)
    smoke.runner = None
    check_history()
    unavailable = call("work_on_project", False, project=project, session_id=ref, instruction="Resume")
    check(unavailable.get("error_kind") in ("project_registration_identity_unavailable", "unknown_project"),
          "offline resume lost the unavailable diagnostic")
    runner()
    stop(smoke.runner)
    smoke.runner = None
    registration.write_bytes(migrated.replace(b"disabled = false", b"disabled = true")
                             if b"disabled =" in migrated else b"disabled = true\n" + migrated)
    runner(enabled=False)
    denied("work_on_project", owner, project=project, session_id=ref, instruction="Resume")
    check(binding(session) is None, "history or a denied request triggered binding")
    stop(smoke.runner)
    smoke.runner = None
    registration.write_bytes(migrated)
    runner()

    # The stepped path first presents a Ref via the pre-upgrade OAuth client.
    # The direct path races two first uses of the full ID via ordinary API calls.
    if released:
        result = smoke.tool("work_on_project", {"session_id": ref,
                            "instruction": "Check upgrade continuity"}, oauth)
        check(result["session_id"] == session, "OAuth Ref resume returned another Session")
    else:
        with ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(resume, (session, session)))
        check(all(result["session_id"] == session for result in results), "concurrent resume failed")
    identity = binding(session)
    check(isinstance(identity, str) and len(identity) == 64, "first request did not commit binding")
    call("post_session_message", session_id=pending_id, kind="decision", message="Inspect interrupted call")
    check(binding(pending_id) == identity, "past interrupted request blocked future work")
    for selector in (session, ref):
        check(resume(selector)["session_id"] == session, "original selector failed")
    check_history(after_live_use=True)
    # A different legacy Session binds through a live non-startup operation.
    call("post_session_message", session_id=second_legacy, kind="decision", message="Live use")
    check(binding(second_legacy) == identity, "non-startup call did not persist the binding")
    guarded = call("run_shell", False, project=project, session_id=second_legacy, command="exit 0")
    check(guarded.get("error_kind") == "session_guard_denied", "binding changed Session guards")
    lost = call("get_session_shell_status", False, project=project, session_id=ref, shell_id=old_shell)
    check(lost.get("error_code") == "persistent_shell_not_found", "expired Shell became accessible")
    call("run_shell", project=project, session_id=ref,
         command="echo validated > new-validation.log", purpose="validation")
    check((workdir / "new-validation.log").read_text() == "validated\n", "new validation did not execute")
    stop(smoke.runner)
    smoke.runner = None
    runner()
    check(resume(ref)["session_id"] == session, "original Ref failed after Runner restart")
    restart(candidate)
    for selector in (session, ref):
        check(resume(selector)["session_id"] == session, "resume failed after Server/Runner restart")
    if current:
        check(resume(current)["session_id"] == current, "v0.5.1 Session failed after restart")
    check(binding(session) == identity and registration.read_bytes() == migrated,
          "binding changed across restart")
    stop(smoke.runner)
    smoke.runner = None
    check(call("close_session", session_id=second_legacy)["already_closed"] is False, "offline close")
    registration.write_text(f'id = "fixture"\npath = {json.dumps(str(repo))}\n')
    runner()
    check(registration.read_bytes() != migrated, "expected replacement registration")
    for selector in (session, ref):
        resume(selector, False)
    if current:
        resume(current, False)
    check(binding(session) == identity, "replaced registration overwrote a nonempty binding")
    check(call("close_session", session_id=second_legacy)["already_closed"] is True, "durable close")
    check(call("list_sessions", project=project)["total"] == 4 + bool(current), "duplicate Session")
    check(marker.read_text() == "old-job\n", "old Job or request replayed")
    check(pending_marker.read_text() == "entered\n", "interrupted Runner request was replayed")
    print(f"PASS v0.5.0 → {'v0.5.1 → ' if released else ''}candidate: original Session and new validation",
          flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for version in ("v050", "v051", "candidate"):
        parser.add_argument("--" + version, type=Path, required=True, help="Server/Runner bin directory")
    args = parser.parse_args()
    for intermediate in (args.v051.resolve(), None):
        with tempfile.TemporaryDirectory(prefix="webcodex-upgrade-") as root:
            fixture = Smoke(args.v050.resolve(), Path(root))
            try:
                run(fixture, intermediate, args.candidate.resolve())
            finally:
                fixture.close()
