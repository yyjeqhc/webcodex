# Agent Project Registry

The `webcodex-agent` is the source of truth for local project metadata on
its host. The server only caches summaries reported by each agent client during
registration and polling. This registry is still live and is reported through
the `list_agents` runtime tool / `ShellClientView`.

## Default Location

By default, the agent scans:

```bash
~/.config/webcodex/projects.d/*.toml
```

An agent config may override this path:

```toml
projects_dir = "/root/.config/webcodex/projects.d"
```

Each file describes one local project:

```toml
id = "webcodex"
path = "/root/git/webcodex"
name = "webcodex"
kind = "rust"
description = "WebCodex server and agent"
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
> Legacy local CLI subcommands such as `new`, `agent-snapshot`, `agent-precommit`,
> `agent-hook`, `workflow-job`, `shell-job`, `shell-batch`, `doctor`, `workflow`,
> `snapshot`, `precommit`, and `hook` depended on these removed routes and must
> not be used.

The `[hooks]` table in a `projects.d` file is still parsed and hook **names**
are still reported to the server, but no current runtime route invokes hook
**commands**. To run checks or long work, use the current runtime surface:

- `runCodexTask` (`POST /api/codex/run`) for Codex CLI tasks.
- `getRuntimeJobStatus` / `getRuntimeJobLog` to poll jobs.
- `callRuntimeTool` (`POST /api/tools/call`) with `run_shell` / `run_job` for
  direct execution subject to agent policy.

## Current Limits

- Run workflow / doctor / hook by agent project id through a server route: not
  available (those routes were removed).
- Path access is controlled by the local agent policy
  (`allow_raw_shell`, `allow_cwd_anywhere`, `allowed_roots`,
  `max_timeout_secs`, `max_output_bytes`).

## Agent-side project management: `register_project` and `create_project`

Two runtime tools let a GPT Action or MCP caller register or create projects on
a selected agent without the server ever writing project config files directly
on the agent host. Both are **mutations with side effects** and require Bearer
auth.

- **`register_project`** (GPT Action: `registerProject`, REST:
  `POST /api/projects/register`) — registers an **existing** directory as a
  WebCodex project. The agent validates that the path exists, is a directory,
  and is allowed by agent policy. It then writes
  `projects_dir/<id>.toml` atomically (temp file → fsync → rename) and
  refreshes its local project list. The server upserts the new project summary
  into its cache so `listProjects` sees it immediately.

- **`create_project`** (GPT Action: `createProject`, REST:
  `POST /api/projects/create`) — creates a **new** directory on the selected
  agent and registers it as a WebCodex project. Supports a minimal `empty`
  template (directory only) or a `basic` template (README.md + .gitignore),
  optional `git init`, and the same atomic TOML write + cache refresh as
  `register_project`. Never deletes or overwrites existing non-empty content.

### Architecture

- `projects.d/*.toml` remains the **source of truth** for registered projects
  on each agent host. `register_project` and `create_project` are the supported
  way to create those files programmatically.
- The **server never writes** project config files or creates directories on
  the agent host directly. It validates the request shape, checks the owner
  boundary, routes the operation to the selected agent by `client_id`, and
  parses the structured JSON response.
- The **agent** does the authoritative path/policy validation, writes the TOML
  file, creates directories/templates for `create_project`, and returns a
  structured result.
- There is **no workspace abstraction**. OS permissions and agent policy
  (`allow_cwd_anywhere` / `allowed_roots`) are the real boundary. Unsafe system
  roots (`/`, `/etc`, `/bin`, `/usr`, `/var`, …) are always rejected unless the
  path is under an explicit `allowed_roots` entry.
- `projects_dir` comes from the agent config, not from the server request.

### Validation summary

| Field | Rules |
|-------|-------|
| `client_id` | Must refer to a currently registered agent. |
| `id` | Non-empty, ≤ 64 chars, ASCII letters/digits/`-`/`_` only; no slash, backslash, dot-dot, or NUL. |
| `name` | Non-empty after trim, ≤ 120 chars, no NUL. |
| `description` | Optional, ≤ 500 chars, no NUL. |
| `path` | Non-empty, absolute, no NUL. Agent validates existence, directory type, and policy. |

## Troubleshooting: unexpected runtime project ids (e.g. `agent:<client>:tmp-hello`)

The server is a zero-project-config relay: every runtime project id of the form
`agent:<client_id>:<project_id>` comes from a `projects.d/*.toml` file on the
**agent** host, never from server-side `projects.toml`. If an unexpected id such
as `agent:oe:tmp-hello` appears in `listProjects` / `runtime_status`, it means
the agent identified by `<client_id>` (`oe` in the example) has a `tmp-hello`
project file locally.

To find and remove it, on the agent host (the machine running
`webcodex-agent` for that `client_id`):

```bash
# 1. Find the agent's projects_dir (from its agent.toml).
grep -n 'projects_dir' /etc/webcodex/agent.toml \
  ~/.config/webcodex/agent.toml 2>/dev/null

# 2. List the project files the agent scans (default shown below).
ls -1 ~/.config/webcodex/projects.d/*.toml

# 3. Locate the offending id.
grep -rl 'id = "tmp-hello"' ~/.config/webcodex/projects.d/

# 4. Remove or disable it, then let the agent rescan (next poll/register).
rm ~/.config/webcodex/projects.d/tmp-hello.toml
#   or set `disabled = true` inside the file.
```

The server cache refreshes on the agent's next register/poll plus the short
local scan cache. Do not commit real `agent.toml` or `projects.d/` files to
this repository; they are host-local configuration.

