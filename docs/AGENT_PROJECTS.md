# Agent Project Registry

The `private-drop-agent` is the source of truth for local project metadata on
its host. The server only caches summaries reported by each agent client during
registration and polling. This registry is still live and is reported through
the `list_agents` runtime tool / `ShellClientView`.

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
remain local to the agent. Files with `disabled = true` are skipped and not
reported.

## Reporting

The agent reports project summaries (id, name, path, kind, description, hook
names, disabled flag, best-effort git branch/head/dirty state, `updated_at`)
during registration and poll requests. It rescans the projects directory through
a short local cache, so a new `projects.d/foo.toml` file should appear on the
server after the next poll cycle plus the cache interval.

The server cache is visible via `list_agents` (a `ToolRuntime` tool) and the
`agents` section of `runtime_status` (`POST /api/runtime/status`). See
[AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) and
[RUNTIME_STATUS.md](RUNTIME_STATUS.md).

## Removed: project workflow / doctor / hook execution routes

> **Deprecated.** The following routes are **not mounted** in the current
> runtime and must not be used:
>
> - `POST /api/shell/projects/create`
> - `POST /api/shell/projects/workflow`
> - `POST /api/shell/projects/workflow_job`
> - `POST /api/shell/jobs/shell`
> - `POST /api/shell/jobs/shell_batch`
> - `POST /api/shell/clients`
> - `POST /api/shell/projects`
> - `POST /api/codex/project_workflow`
> - `POST /api/codex/project_doctor`
>
> The `pdctl.py new`, `agent-snapshot`, `agent-precommit`, `agent-hook`,
> `workflow-job`, `shell-job`, `shell-batch`, `doctor`, `workflow`, `snapshot`,
> `precommit`, and `hook` subcommands depend on these removed routes and are
> legacy (see the `scripts/pdctl.py` deprecation notice).

The `[hooks]` table in a `projects.d` file is still parsed and hook **names**
are still reported to the server, but no current runtime route invokes hook
**commands**. To run checks or long work, use the current runtime surface:

- `runCodexTask` (`POST /api/codex/run`) for Codex CLI tasks.
- `getRuntimeJobStatus` / `getRuntimeJobLog` to poll jobs.
- `callRuntimeTool` (`POST /api/tools/call`) with `run_shell` / `run_job` for
  direct execution subject to agent policy.

## Current Limits

- Create / delete project on an agent through a server route: not available
  (the create route was removed).
- Run workflow / doctor / hook by agent project id through a server route: not
  available (those routes were removed).
- Path access is controlled by the local agent policy
  (`allow_raw_shell`, `allow_cwd_anywhere`, `allowed_roots`,
  `max_timeout_secs`, `max_output_bytes`).
