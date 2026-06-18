# Agent Project Registry

Private Drop now supports an agent-owned project registry for local projects.
The agent is the source of truth for local project metadata. The server only
caches summaries reported by each `private-drop-agent` client.

## Default Location

By default, the agent scans:

```bash
~/.config/private-drop-agent/projects.d/*.toml
```

An agent config may override this path:

```toml
projects_dir = "/root/.config/private-drop-agent/projects.d"
```

Each file describes one local project:

```toml
id = "private-drop"
path = "/root/git/private-drop"
name = "private-drop"
kind = "rust"
description = "Private Drop server and agent"
disabled = false

[hooks]
doctor = [
  "git status --short",
  "git log --oneline -5"
]

precommit = [
  ". /root/.cargo/env && cargo fmt --check",
  ". /root/.cargo/env && cargo test"
]
```

`id` and `path` are required. `id` must contain only ASCII letters, digits,
`-`, `_`, and `.`. Hook names are reported to the server, but hook commands
remain local to the agent and are not managed by the server. Files with
`disabled = true` are skipped and not reported.

## Reporting

The agent reports project summaries during registration and during poll
requests. It rescans the projects directory through a short local cache, so a
new `projects.d/foo.toml` file should appear on the server after the next poll
cycle plus the cache interval.

The server cache includes project id, name, path, kind, description, hook names,
disabled flag, best-effort git branch/head/dirty state, and `updated_at`.

## Creating Projects Through The Agent

Use `pdctl.py new` to ask a specific agent to create a project locally:

```bash
python3 scripts/pdctl.py new oe foo /root/work/foo --template rust --git-init
python3 scripts/pdctl.py new oe notes /root/work/notes --template docs
python3 scripts/pdctl.py new oe py-demo /root/work/py-demo --template python --allow-existing
```

The server checks that the caller may operate the target `client_id`, then
queues a structured `create_project` request for the agent. The agent creates
the directory, writes template files, optionally runs `git init`, and writes:

```bash
~/.config/private-drop-agent/projects.d/<project-id>.toml
```

The generated `projects.d` file is the source of truth. The server only caches
the summary returned by the agent and summaries reported in later polls. Hook
commands remain in the agent-local TOML file; the server only sees hook names.

Verify the new project after the agent reports it:

```bash
python3 scripts/pdctl.py projects oe
```

Manual creation is still possible:

1. Create the project directory on the agent host.
2. Write `~/.config/private-drop-agent/projects.d/foo.toml`.
3. Wait for the agent to poll and report the new summary.
4. Run `python3 scripts/pdctl.py projects oe` to confirm it is visible.

`projects.toml` remains supported on the server for existing project workflow,
doctor, and hook APIs.

## Agent-native Workflow

Server-configured `project_workflow`, `project_doctor`, and `project_hook` APIs
still exist and still use server `projects.toml` ProjectConfig entries.

Agent-native workflow uses `client_id` plus the agent-local `project_id` from
`projects.d`. It does not depend on server `projects.toml`: the server checks
the caller owns the target client, queues a structured `project_workflow`
request, and the agent reads its local registry before running git snapshots or
configured hook commands.

For GPT Actions and new windows, prefer this sequence:

1. `listShellClients`
2. `listShellClientProjects`
3. `createShellClientProject` if the project does not exist yet
4. `runShellClientProjectWorkflow`
5. For long-running work, `createShellClientProjectWorkflowJob` and then poll
   the returned `job_id`.

CLI examples:

```bash
python3 scripts/pdctl.py agent-snapshot oe foo
python3 scripts/pdctl.py agent-precommit oe foo
python3 scripts/pdctl.py agent-hook oe foo doctor
```

The agent does not call any large-model API. Model reasoning happens in
ChatGPT / GPT Action. The agent only executes structured workflow requests from
the Private Drop server.

## Agent-native Async Jobs

Async jobs are for commands or project workflows that may outlive a single GPT
Action HTTP request. The server creates a job record, queues work for the
target agent, and immediately returns `job_id` with `status=queued`.

The `job_id` is the isolation unit. Private Drop does not create a
`session_id`, cookie, or one-process-per-GPT-window mapping for agent jobs.
Different GPT windows can create jobs concurrently; each window only needs to
keep its own `job_id` to query status, read logs, stop, or fetch the final
structured result.

Async API operations:

- `createShellClientShellJob`: `POST /api/shell/jobs/shell`
- `createShellClientShellJobBatch`: `POST /api/shell/jobs/shell_batch`
- `createShellClientProjectWorkflowJob`: `POST /api/shell/projects/workflow_job`
- `getShellClientJobStatus`: `POST /api/shell/jobs/status`
- `getShellClientJobLog`: `POST /api/shell/jobs/log`
- `stopShellClientJob`: `POST /api/shell/jobs/stop`
- `listShellClientJobs`: `POST /api/shell/jobs/list`

CLI examples:

```bash
python3 scripts/pdctl.py shell-job oe --cwd /tmp --command "echo hello"
python3 scripts/pdctl.py shell-batch oe --command "echo one" --command "echo two"
python3 scripts/pdctl.py workflow-job oe foo --mode precommit
python3 scripts/pdctl.py job-status <job-id>
python3 scripts/pdctl.py job-log <job-id> --tail-lines 80
python3 scripts/pdctl.py job-stop <job-id>
python3 scripts/pdctl.py jobs oe
```

The server is the control plane: it checks auth and client ownership, checks
agent capabilities, queues work, stores status/log tails/result, and exposes
the GPT Action query APIs. The agent is the execution plane: it polls jobs,
executes them in the background, streams bounded stdout/stderr tails, and
reports final status and structured results.

`private-drop-agent` registers `async_jobs`, `async_shell_jobs`, and
`async_project_workflow_jobs`. Older agents that do not advertise those
capabilities are rejected with a clear capability error.

Each agent runs up to `max_concurrent_jobs` background jobs at once. The default
is `2`; extra jobs remain queued in simple FIFO order on that agent. There is
no project-level lock, so two jobs against the same project directory may
interfere with each other. Coordinate at the caller level when running
mutating workflows.

Stopping is best-effort. Shell jobs try to terminate the process group and then
the child process. Workflow jobs set a stop flag; if a hook command is running,
the current command is stopped best-effort. A stop request may briefly return
`stop_requested` before the agent reports `stopped`.

The agent never calls OpenAI, GLM, Claude, Anthropic, local model APIs, or any
other LLM API. It only runs local shell/file/project workflow operations
requested by the server.

## Current Limits

- Create project on an agent: available through `pdctl.py new` and
  `POST /api/shell/projects/create`.
- Delete project: not implemented.
- Run workflow by agent project id: available through
  `POST /api/shell/projects/workflow`.
- Run async shell and project workflow jobs: available through
  `POST /api/shell/jobs/shell` and
  `POST /api/shell/projects/workflow_job`.
- Path access is controlled by the local agent policy.
