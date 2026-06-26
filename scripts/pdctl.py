#!/usr/bin/env python3
"""Small Private Drop Codex workflow helper.

DEPRECATED. This helper targets the removed project_workflow / project_doctor /
project_hook / shell-job routes (/api/codex/project_workflow,
/api/codex/project_doctor, /api/shell/projects/*, /api/shell/jobs/shell*,
/api/shell/clients, /api/shell/projects). None of those routes are mounted in
the current runtime, so most subcommands will fail. It is kept only as a
historical reference. For the current runtime surface, see README.md,
docs/GPT_ACTIONS.md, and docs/AGENT_PROTOCOL.md.
"""

import argparse
import json
import os
import sys
import textwrap
import urllib.error
import urllib.request


DEFAULT_URL = "http://127.0.0.1:8000"


class CliError(Exception):
    def __init__(self, message, exit_code=2):
        super().__init__(message)
        self.exit_code = exit_code


def env_first(primary, fallback, default=None):
    value = os.environ.get(primary)
    if value:
        return value
    value = os.environ.get(fallback)
    if value:
        return value
    return default


def resolve_url(args):
    return (args.url or env_first("PRIVATE_DROP_URL", "DROP_URL", DEFAULT_URL)).rstrip("/")


def resolve_token(args):
    token = args.token or env_first("PRIVATE_DROP_TOKEN", "WEBCODEX_TOKEN")
    if not token:
        raise CliError("missing PRIVATE_DROP_TOKEN or WEBCODEX_TOKEN", 2)
    return token


def add_common_options(parser):
    parser.add_argument("--url", help="Private Drop base URL. Defaults to PRIVATE_DROP_URL, DROP_URL, then http://127.0.0.1:8000.")
    parser.add_argument("--token", help="Bearer token. Defaults to PRIVATE_DROP_TOKEN, then WEBCODEX_TOKEN.")
    parser.add_argument("--json", action="store_true", help="Print the complete response as pretty JSON.")
    parser.add_argument("--debug", action="store_true", help="Print short exception details on request errors.")


def add_workflow_options(parser, include_mode=False, include_hook=False):
    if include_mode:
        parser.add_argument("--mode", choices=["snapshot", "doctor", "hook", "precommit"], default="snapshot")
    if include_hook:
        parser.add_argument("--hook", dest="hook_opt", help="Workflow hook name.")
    parser.add_argument("--timeout-secs", type=int, help="Per-command timeout passed to the server.")
    parser.add_argument("--recent-jobs", type=int, help="Recent job count passed to doctor.")
    run_doctor = parser.add_mutually_exclusive_group()
    run_doctor.add_argument("--run-doctor", dest="run_doctor", action="store_true", default=None)
    run_doctor.add_argument("--no-run-doctor", dest="run_doctor", action="store_false")
    parser.add_argument("--run-doctor-hook", action="store_true", help="Allow mode=doctor to run the doctor hook.")
    parser.add_argument("--doctor-hook", default=None, help="Doctor hook name. Defaults to doctor.")


def add_agent_workflow_options(parser, include_mode=False, include_hook=False):
    if include_mode:
        parser.add_argument("--mode", choices=["snapshot", "doctor", "hook", "precommit"], default="snapshot")
    if include_hook:
        parser.add_argument("--hook", dest="hook_opt", help="Agent project hook name.")
    run_doctor = parser.add_mutually_exclusive_group()
    run_doctor.add_argument("--run-doctor", dest="run_doctor", action="store_true", default=None)
    run_doctor.add_argument("--no-run-doctor", dest="run_doctor", action="store_false")
    parser.add_argument("--run-doctor-hook", action="store_true", help="Allow mode=doctor to run the doctor hook.")
    parser.add_argument("--doctor-hook", default=None, help="Doctor hook name. Defaults to doctor.")
    parser.add_argument("--timeout-secs", type=int, help="Per-command timeout passed to the agent.")
    parser.add_argument("--wait-timeout-secs", type=int, help="Seconds for the server to wait for the agent result.")


def request_json(url, token, path, body, timeout_secs, debug=False):
    data = json.dumps(body).encode("utf-8")
    request = urllib.request.Request(
        url + path,
        data=data,
        headers={
            "Authorization": "Bearer " + token,
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
        method="POST",
    )
    request_timeout = max((timeout_secs or 120) + 30, 30)
    try:
        with urllib.request.urlopen(request, timeout=request_timeout) as response:
            raw = response.read().decode("utf-8")
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace").strip()
        message = "HTTP {} from {}".format(exc.code, path)
        if detail:
            try:
                payload = json.loads(detail)
                message = payload.get("error") or message
            except json.JSONDecodeError:
                message = detail.splitlines()[0]
        if debug:
            message = "{} ({})".format(message, repr(exc))
        raise CliError(message, 2)
    except urllib.error.URLError as exc:
        message = "request failed: {}".format(exc.reason)
        if debug:
            message = "{} ({})".format(message, repr(exc))
        raise CliError(message, 2)
    except TimeoutError as exc:
        message = "request timed out"
        if debug:
            message = "{} ({})".format(message, repr(exc))
        raise CliError(message, 2)

    try:
        return json.loads(raw)
    except json.JSONDecodeError as exc:
        message = "response was not JSON"
        if debug:
            message = "{} ({})".format(message, repr(exc))
        raise CliError(message, 2)


def compact_text(value):
    if value is None:
        return ""
    return str(value).strip()


def print_multiline(prefix, value):
    value = compact_text(value)
    if not value:
        print(prefix + " <clean>")
        return
    lines = value.splitlines()
    print(prefix)
    for line in lines:
        print("  " + line)


def print_git(label, git):
    git = git or {}
    head = compact_text(git.get("head"))
    subject = compact_text(git.get("head_subject"))
    if subject:
        head = "{} {}".format(head, subject).strip()
    print("{}:".format(label))
    print("  available: {}".format(git.get("available")))
    print("  branch: {}".format(compact_text(git.get("branch")) or "-"))
    print("  head: {}".format(head or "-"))
    print("  dirty: {}".format(git.get("dirty")))
    print_multiline("  status_short:", git.get("status_short"))
    if "diff_stat" in git:
        print_multiline("  diff_stat:", git.get("diff_stat"))
    changed = git.get("changed_files") or []
    if changed:
        print("  changed_files:")
        for item in changed:
            print("    " + str(item))
    if git.get("error"):
        print("  error: {}".format(git.get("error")))


def print_hook_result(hook_result):
    if not hook_result:
        print("hook_result: none")
        return
    steps = hook_result.get("steps") or []
    print("hook_result:")
    print("  hook: {}".format(hook_result.get("hook", "-")))
    print("  success: {}".format(hook_result.get("success")))
    print("  steps: {}".format(len(steps)))
    for step in steps:
        if step.get("exit_code") != 0:
            print("  failed_step:")
            print("    command: {}".format(step.get("command", "")))
            print("    exit_code: {}".format(step.get("exit_code")))
            stderr = compact_text(step.get("stderr_tail"))
            if stderr:
                print_multiline("    stderr_tail:", stderr)
            break
    if hook_result.get("error"):
        print("  error: {}".format(hook_result.get("error")))


def enabled_capabilities(capabilities):
    capabilities = capabilities or {}
    names = [name for name in sorted(capabilities) if capabilities.get(name)]
    return ",".join(names) if names else "-"


def print_clients(payload):
    print("success: {}".format(payload.get("success")))
    clients = payload.get("clients") or []
    print("clients: {}".format(len(clients)))
    for client in clients:
        projects = client.get("projects") or []
        print(
            "{client_id} owner={owner} hostname={hostname} connected={connected} capabilities={capabilities} projects={project_count}".format(
                client_id=client.get("client_id", "-"),
                owner=compact_text(client.get("owner")) or "-",
                hostname=compact_text(client.get("hostname")) or "-",
                connected=client.get("connected"),
                capabilities=enabled_capabilities(client.get("capabilities")),
                project_count=len(projects),
            )
        )
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_projects(payload):
    print("success: {}".format(payload.get("success")))
    print("client_id: {}".format(payload.get("client_id", "")))
    projects = payload.get("projects") or []
    print("projects: {}".format(len(projects)))
    for project in projects:
        hooks = project.get("hooks") or []
        git = "{}/{}".format(
            compact_text(project.get("git_branch")) or "-",
            compact_text(project.get("git_head")) or "-",
        )
        print(
            "{id} path={path} kind={kind} hooks={hooks} git={git} dirty={dirty}".format(
                id=project.get("id", "-"),
                path=project.get("path", "-"),
                kind=compact_text(project.get("kind")) or "-",
                hooks=",".join(hooks) if hooks else "-",
                git=git,
                dirty=project.get("git_dirty"),
            )
        )
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_project_create(payload):
    print("success: {}".format(payload.get("success")))
    print("client_id: {}".format(payload.get("client_id", "")))
    project = payload.get("project") or {}
    print("project_id: {}".format(project.get("id", "-")))
    print("path: {}".format(project.get("path", "-")))
    print("kind: {}".format(compact_text(project.get("kind")) or "-"))
    print("registry_file: {}".format(compact_text(payload.get("registry_file")) or "-"))
    print("git_initialized: {}".format(payload.get("git_initialized")))
    created_paths = payload.get("created_paths") or []
    print("created_paths: {}".format(len(created_paths)))
    for path in created_paths:
        print("  " + str(path))
    warnings = payload.get("warnings") or []
    if warnings:
        print("warnings:")
        for warning in warnings:
            print("  - " + str(warning))
    else:
        print("warnings: none")
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_agent_workflow(payload):
    print("success: {}".format(payload.get("success")))
    print("client_id: {}".format(payload.get("client_id", "")))
    print("project_id: {}".format(payload.get("project_id", "")))
    project = payload.get("project") or {}
    print("path: {}".format(project.get("path", "-")))
    print("kind: {}".format(compact_text(project.get("kind")) or "-"))
    print("mode: {}".format(payload.get("mode", "-")))
    print_git("git_before", payload.get("git_before"))
    print_git("git_after", payload.get("git_after"))
    print_hook_result(payload.get("hook_result"))
    warnings = payload.get("warnings") or []
    if warnings:
        print("warnings:")
        for warning in warnings:
            print("  - " + str(warning))
    else:
        print("warnings: none")
    print("recommended_next_action: {}".format(payload.get("recommended_next_action") or "-"))
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_job_create(payload):
    print("success: {}".format(payload.get("success")))
    print("job_id: {}".format(payload.get("job_id") or "-"))
    print("client_id: {}".format(payload.get("client_id") or "-"))
    if payload.get("project_id"):
        print("project_id: {}".format(payload.get("project_id")))
    print("status: {}".format(payload.get("status") or "-"))
    job = payload.get("job") or {}
    if job:
        print("kind: {}".format(job.get("kind") or "-"))
        print("command: {}".format(job.get("command_preview") or "-"))
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_job_status(payload):
    print("success: {}".format(payload.get("success")))
    print("job_id: {}".format(payload.get("job_id") or "-"))
    print("client_id: {}".format(payload.get("client_id") or "-"))
    print("kind: {}".format(payload.get("kind") or "-"))
    print("status: {}".format(payload.get("status") or "-"))
    print("elapsed_secs: {}".format(payload.get("elapsed_secs") if payload.get("elapsed_secs") is not None else "-"))
    print("exit_code: {}".format(payload.get("exit_code") if payload.get("exit_code") is not None else "-"))
    result = payload.get("result") or {}
    workflow = result.get("project_workflow") or {}
    if workflow:
        print("project_id: {}".format(workflow.get("project_id") or "-"))
        print("mode: {}".format(workflow.get("mode") or "-"))
        print("recommended_next_action: {}".format(workflow.get("recommended_next_action") or "-"))
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_job_log(payload):
    print("success: {}".format(payload.get("success")))
    print("job_id: {}".format(payload.get("job_id") or "-"))
    print("client_id: {}".format(payload.get("client_id") or "-"))
    print_multiline("stdout_tail:", payload.get("stdout_tail"))
    print_multiline("stderr_tail:", payload.get("stderr_tail"))
    print("next_stdout_line: {}".format(payload.get("next_stdout_line") or "-"))
    print("next_stderr_line: {}".format(payload.get("next_stderr_line") or "-"))
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_job_stop(payload):
    print("success: {}".format(payload.get("success")))
    print("job_id: {}".format(payload.get("job_id") or "-"))
    print("status: {}".format(payload.get("status") or "-"))
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_jobs(payload):
    print("success: {}".format(payload.get("success")))
    print("client_id: {}".format(payload.get("client_id") or "-"))
    jobs = payload.get("jobs") or []
    print("jobs: {}".format(len(jobs)))
    for job in jobs:
        print(
            "{job_id} kind={kind} status={status} command={command}".format(
                job_id=job.get("job_id", "-"),
                kind=job.get("kind", "-"),
                status=job.get("status", "-"),
                command=job.get("command_preview", "-"),
            )
        )
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def print_summary(payload):
    print("success: {}".format(payload.get("success")))
    print("project: {}".format(payload.get("project", "")))
    if payload.get("mode") is not None:
        print("mode: {}".format(payload.get("mode")))
    print("executor: {}".format(payload.get("executor", "-")))
    print("root: {}".format(payload.get("root", "-")))
    print("ssh_enabled: {}".format(payload.get("ssh_enabled", "-")))

    if "git_before" in payload or "git_after" in payload:
        print_git("git_before", payload.get("git_before"))
        print_git("git_after", payload.get("git_after"))
    else:
        print_git("git", payload.get("git"))

    warnings = payload.get("warnings") or []
    if warnings:
        print("warnings:")
        for warning in warnings:
            print("  - " + str(warning))
    else:
        print("warnings: none")

    print_hook_result(payload.get("hook_result"))

    doctor = payload.get("doctor")
    if doctor and doctor.get("warnings"):
        print("doctor_warnings:")
        for warning in doctor.get("warnings") or []:
            print("  - " + str(warning))
    if doctor and doctor.get("hook_result") and not payload.get("hook_result"):
        print_hook_result(doctor.get("hook_result"))

    recommended = payload.get("recommended_next_action")
    if not recommended and payload.get("hooks"):
        recommended = payload.get("hooks", {}).get("recommended_next")
    print("recommended_next_action: {}".format(recommended or "-"))
    if payload.get("error"):
        print("error: {}".format(payload.get("error")))


def add_if_present(body, args, names):
    for name in names:
        value = getattr(args, name, None)
        if value is not None:
            body[name] = value


def build_request(args):
    if args.command == "clients":
        return "/api/shell/clients", {}

    if args.command == "projects":
        return "/api/shell/projects", {"client_id": args.client_id}

    if args.command == "new":
        body = {
            "client_id": args.client_id,
            "project_id": args.project_id,
            "path": args.path,
            "template": args.template,
            "allow_existing": bool(args.allow_existing),
        }
        add_if_present(
            body,
            args,
            ["name", "kind", "description", "timeout_secs", "wait_timeout_secs"],
        )
        if args.git_init is not None:
            body["git_init"] = args.git_init
        return "/api/shell/projects/create", body

    if args.command == "shell-job":
        body = {
            "client_id": args.client_id,
            "command": args.command_text,
        }
        add_if_present(body, args, ["cwd", "timeout_secs", "max_runtime_secs"])
        return "/api/shell/jobs/shell", body

    if args.command == "shell-batch":
        body = {
            "client_id": args.client_id,
            "commands": args.commands,
        }
        add_if_present(body, args, ["cwd", "timeout_secs", "max_runtime_secs"])
        return "/api/shell/jobs/shell_batch", body

    if args.command == "workflow-job":
        body = {
            "client_id": args.client_id,
            "project_id": args.project_id,
            "mode": args.mode,
        }
        if args.hook_opt:
            body["hook"] = args.hook_opt
        add_if_present(
            body,
            args,
            ["run_doctor", "run_doctor_hook", "timeout_secs", "max_runtime_secs"],
        )
        if args.doctor_hook:
            body["doctor_hook"] = args.doctor_hook
        return "/api/shell/projects/workflow_job", body

    if args.command == "job-status":
        body = {"job_id": args.job_id}
        add_if_present(body, args, ["client_id"])
        return "/api/shell/jobs/status", body

    if args.command == "job-log":
        body = {"job_id": args.job_id}
        add_if_present(body, args, ["client_id", "tail_lines", "since_stdout_line", "since_stderr_line"])
        return "/api/shell/jobs/log", body

    if args.command == "job-stop":
        body = {"job_id": args.job_id}
        add_if_present(body, args, ["client_id"])
        return "/api/shell/jobs/stop", body

    if args.command == "jobs":
        body = {"client_id": args.client_id}
        add_if_present(body, args, ["status", "limit"])
        return "/api/shell/jobs/list", body

    if args.command == "agent-workflow":
        mode = args.mode
        hook = args.hook_opt
    elif args.command == "agent-snapshot":
        mode = "snapshot"
        hook = args.hook_opt
    elif args.command == "agent-precommit":
        mode = "precommit"
        hook = args.hook_opt
    elif args.command == "agent-hook":
        mode = "hook"
        hook = args.hook_name or args.hook_opt
        if not hook:
            raise CliError("hook name is required", 2)
    else:
        mode = None
        hook = None

    if mode is not None:
        body = {
            "client_id": args.client_id,
            "project_id": args.project_id,
            "mode": mode,
        }
        if hook:
            body["hook"] = hook
        add_if_present(
            body,
            args,
            ["run_doctor", "run_doctor_hook", "timeout_secs", "wait_timeout_secs"],
        )
        if args.doctor_hook:
            body["doctor_hook"] = args.doctor_hook
        return "/api/shell/projects/workflow", body

    if args.command == "doctor":
        body = {
            "project": args.project,
            "run_hook": bool(args.run_hook),
        }
        if args.run_hook:
            body["hook"] = args.doctor_hook or "doctor"
        add_if_present(body, args, ["recent_jobs", "timeout_secs"])
        return "/api/codex/project_doctor", body

    if args.command == "workflow":
        mode = args.mode
        hook = args.hook_opt
    elif args.command == "snapshot":
        mode = "snapshot"
        hook = args.hook_opt
    elif args.command == "precommit":
        mode = "precommit"
        hook = args.hook_opt
    elif args.command == "hook":
        mode = "hook"
        hook = args.hook_name or args.hook_opt
        if not hook:
            raise CliError("hook name is required", 2)
    else:
        raise CliError("unknown command: {}".format(args.command), 2)

    body = {
        "project": args.project,
        "mode": mode,
    }
    if hook:
        body["hook"] = hook
    add_if_present(
        body,
        args,
        ["run_doctor", "run_doctor_hook", "recent_jobs", "timeout_secs"],
    )
    if args.doctor_hook:
        body["doctor_hook"] = args.doctor_hook
    return "/api/codex/project_workflow", body


def build_parser():
    parser = argparse.ArgumentParser(
        prog="pdctl.py",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        description="DEPRECATED. Calls removed project workflow/doctor/hook/shell-job routes. Most subcommands will fail against the current runtime.",
        epilog=textwrap.dedent(
            """\
            Examples:
              python3 scripts/pdctl.py clients
              python3 scripts/pdctl.py projects oe
              python3 scripts/pdctl.py new oe foo /root/work/foo --template rust --git-init
              python3 scripts/pdctl.py shell-job oe --cwd /tmp --command "echo hello"
              python3 scripts/pdctl.py shell-batch oe --command "echo one" --command "echo two"
              python3 scripts/pdctl.py workflow-job oe foo --mode precommit
              python3 scripts/pdctl.py job-status JOB_ID
              python3 scripts/pdctl.py job-log JOB_ID
              python3 scripts/pdctl.py job-stop JOB_ID
              python3 scripts/pdctl.py jobs oe
              python3 scripts/pdctl.py agent-snapshot oe foo
              python3 scripts/pdctl.py agent-precommit oe foo
              python3 scripts/pdctl.py agent-hook oe foo doctor
              python3 scripts/pdctl.py doctor private-drop
              python3 scripts/pdctl.py workflow private-drop --mode snapshot --json
              python3 scripts/pdctl.py precommit private-drop
              python3 scripts/pdctl.py hook private-drop precommit
            """
        ),
    )
    subcommands = parser.add_subparsers(dest="command", required=True)

    clients = subcommands.add_parser("clients", help="List registered private-drop-agent clients.")
    add_common_options(clients)

    projects = subcommands.add_parser("projects", help="List projects reported by one agent client.")
    projects.add_argument("client_id")
    add_common_options(projects)

    new = subcommands.add_parser("new", help="Create a new project on an agent client.")
    new.add_argument("client_id")
    new.add_argument("project_id")
    new.add_argument("path")
    new.add_argument("--template", choices=["empty", "rust", "python", "docs"], default="empty")
    new.add_argument("--name")
    new.add_argument("--kind")
    new.add_argument("--description")
    git_init = new.add_mutually_exclusive_group()
    git_init.add_argument("--git-init", dest="git_init", action="store_true", default=None)
    git_init.add_argument("--no-git-init", dest="git_init", action="store_false")
    new.add_argument("--allow-existing", action="store_true")
    new.add_argument("--timeout-secs", type=int)
    new.add_argument("--wait-timeout-secs", type=int)
    add_common_options(new)

    shell_job = subcommands.add_parser("shell-job", help="Create an async shell job on an agent client.")
    shell_job.add_argument("client_id")
    shell_job.add_argument("--cwd")
    shell_job.add_argument("--command", dest="command_text", required=True)
    shell_job.add_argument("--timeout-secs", type=int)
    shell_job.add_argument("--max-runtime-secs", type=int)
    add_common_options(shell_job)

    shell_batch = subcommands.add_parser("shell-batch", help="Create multiple async shell jobs on one agent client.")
    shell_batch.add_argument("client_id")
    shell_batch.add_argument("--cwd")
    shell_batch.add_argument("--command", dest="commands", action="append", required=True)
    shell_batch.add_argument("--timeout-secs", type=int)
    shell_batch.add_argument("--max-runtime-secs", type=int)
    add_common_options(shell_batch)

    workflow_job = subcommands.add_parser("workflow-job", help="Create an async agent-native project workflow job.")
    workflow_job.add_argument("client_id")
    workflow_job.add_argument("project_id")
    add_agent_workflow_options(workflow_job, include_mode=True, include_hook=True)
    workflow_job.add_argument("--max-runtime-secs", type=int)
    add_common_options(workflow_job)

    job_status = subcommands.add_parser("job-status", help="Get async shell/client job status.")
    job_status.add_argument("job_id")
    job_status.add_argument("--client-id")
    add_common_options(job_status)

    job_log = subcommands.add_parser("job-log", help="Read async shell/client job stdout/stderr tails.")
    job_log.add_argument("job_id")
    job_log.add_argument("--client-id")
    job_log.add_argument("--tail-lines", type=int)
    job_log.add_argument("--since-stdout-line", type=int)
    job_log.add_argument("--since-stderr-line", type=int)
    add_common_options(job_log)

    job_stop = subcommands.add_parser("job-stop", help="Best-effort stop for an async shell/client job.")
    job_stop.add_argument("job_id")
    job_stop.add_argument("--client-id")
    add_common_options(job_stop)

    jobs = subcommands.add_parser("jobs", help="List async jobs for an agent client.")
    jobs.add_argument("client_id")
    jobs.add_argument("--status")
    jobs.add_argument("--limit", type=int)
    add_common_options(jobs)

    agent_workflow = subcommands.add_parser("agent-workflow", help="Run an agent-native project workflow.")
    agent_workflow.add_argument("client_id")
    agent_workflow.add_argument("project_id")
    add_agent_workflow_options(agent_workflow, include_mode=True, include_hook=True)
    add_common_options(agent_workflow)

    agent_snapshot = subcommands.add_parser("agent-snapshot", help="Run agent-native workflow mode=snapshot.")
    agent_snapshot.add_argument("client_id")
    agent_snapshot.add_argument("project_id")
    add_agent_workflow_options(agent_snapshot, include_hook=True)
    add_common_options(agent_snapshot)

    agent_precommit = subcommands.add_parser("agent-precommit", help="Run agent-native workflow mode=precommit.")
    agent_precommit.add_argument("client_id")
    agent_precommit.add_argument("project_id")
    add_agent_workflow_options(agent_precommit, include_hook=True)
    add_common_options(agent_precommit)

    agent_hook = subcommands.add_parser("agent-hook", help="Run agent-native workflow mode=hook.")
    agent_hook.add_argument("client_id")
    agent_hook.add_argument("project_id")
    agent_hook.add_argument("hook_name")
    add_agent_workflow_options(agent_hook, include_hook=True)
    add_common_options(agent_hook)

    doctor = subcommands.add_parser("doctor", help="Run project doctor without running hooks by default.")
    doctor.add_argument("project")
    doctor.add_argument("--run-hook", action="store_true", help="Run the doctor hook.")
    doctor.add_argument("--doctor-hook", default="doctor", help="Doctor hook name when --run-hook is used.")
    doctor.add_argument("--timeout-secs", type=int)
    doctor.add_argument("--recent-jobs", type=int)
    add_common_options(doctor)

    workflow = subcommands.add_parser("workflow", help="Run project workflow.")
    workflow.add_argument("project")
    add_workflow_options(workflow, include_mode=True, include_hook=True)
    add_common_options(workflow)

    snapshot = subcommands.add_parser("snapshot", help="Run workflow mode=snapshot.")
    snapshot.add_argument("project")
    add_workflow_options(snapshot, include_hook=True)
    add_common_options(snapshot)

    precommit = subcommands.add_parser("precommit", help="Run workflow mode=precommit.")
    precommit.add_argument("project")
    add_workflow_options(precommit, include_hook=True)
    add_common_options(precommit)

    hook = subcommands.add_parser("hook", help="Run workflow mode=hook for a configured hook.")
    hook.add_argument("project")
    hook.add_argument("hook_name", nargs="?")
    add_workflow_options(hook, include_hook=True)
    add_common_options(hook)

    return parser


def main(argv):
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        url = resolve_url(args)
        token = resolve_token(args)
        path, body = build_request(args)
        payload = request_json(url, token, path, body, body.get("timeout_secs"), args.debug)
        if args.json:
            print(json.dumps(payload, indent=2, sort_keys=True))
        elif args.command == "clients":
            print_clients(payload)
        elif args.command == "projects":
            print_projects(payload)
        elif args.command == "new":
            print_project_create(payload)
        elif args.command in ("shell-job", "shell-batch", "workflow-job"):
            print_job_create(payload)
        elif args.command == "job-status":
            print_job_status(payload)
        elif args.command == "job-log":
            print_job_log(payload)
        elif args.command == "job-stop":
            print_job_stop(payload)
        elif args.command == "jobs":
            print_jobs(payload)
        elif args.command.startswith("agent-"):
            print_agent_workflow(payload)
        else:
            print_summary(payload)
        if payload.get("success") is True:
            return 0
        if payload.get("success") is False:
            return 1
        print("error: response missing success", file=sys.stderr)
        return 2
    except CliError as exc:
        print(str(exc), file=sys.stderr)
        return exc.exit_code
    except Exception as exc:
        message = "unexpected error"
        if getattr(args, "debug", False):
            message = "{}: {}".format(type(exc).__name__, exc)
        print(message, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
