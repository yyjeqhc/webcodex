# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

Use MCP when the client can connect to the project-bound WebCodex endpoint.
Complete [Quick Start](QUICK_START.md) first. WebCodex serves MCP over the
same Server and the same project/authentication model as the CLI and REST.

## Endpoint and authentication

Local clients can use:

```text
http://127.0.0.1:<configured-port>/mcp
```

Hosted clients need HTTPS. There are three paths:

- **Hosted:** `webcodex connect <server>` uses an existing hosted Server; only
  the Runner runs locally. The MCP URL is `https://your-server.example/mcp`
  and the bearer credential is the generated shared key.
- **Local Share:** `webcodex share` starts the local Server + Agent and a
  Cloudflare Quick Tunnel, then prints a temporary HTTPS `/mcp` URL and a
  separate temporary Bearer credential. Ctrl-C revokes that access by stopping
  the runtime/tunnel and removing the temporary share state. Use `--tunnel
  none` only for local testing.
- **Self-hosted:** use a stable HTTPS domain/tunnel, durable service
  management, and OAuth or scoped credentials for long-lived operation.

For a managed or self-hosted Server, use a user API token (`wc_pat_*`) as the
bearer credential, or OAuth when enabled. Do not use the bootstrap/admin
token, account credentials, Runner tokens, or the persistent project-first
Connector credential as a public sharing secret. `share` creates and prints
its own temporary credential for that session.

In ChatGPT Developer Mode, create a custom app with the printed `/mcp` URL.
If the authentication menu offers **Access token/API key**, choose it, paste
the bearer credential, and run **Scan Tools**. ChatGPT UI labels and
availability can vary by workspace and rollout.

### OAuth2

When OAuth is enabled on a managed or self-hosted Server, MCP clients can use
the authorization-code flow instead of a static token. Register the exact
ChatGPT callback URL as an OAuth client redirect URI; keep `offline_access`
enabled when offered (it is a protocol-level refresh-token scope and grants no
extra permission). Server-side OAuth setup is in
[Deployment](DEPLOYMENT.md#oauth2).

## The project-bound surface

A configured local project exposes the Connector surface. MCP `tools/list`
then contains exactly these twelve operations:

```text
task_start
task_list
task_resume
files_list
files_read
files_search
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
```

The Connector context already binds the configured repository. Start with
`task_start`; do not call project-discovery, session, or runtime tools, and do
not put a runtime project id in the prompt. The same chat window continues the
current repository automatically. `task_list` and `task_resume` are explicit
recovery tools when a client can no longer present its transport window
identity.

## Golden coding loop

```text
task_start
→ files_list
→ files_read / files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

- `files_list` answers "what is in this project" from the Git index, so
  ignored directories never appear. Call it before guessing paths.
- `edits_apply` is the guarded edit tool; `commands_run` is the bounded escape
  hatch for commands that need a shell.
- `checks_run` validates. Use a stable `operation_id` so an exact retry reuses
  the operation.
- `task_finish` produces a stable result; a human reviews and accepts or
  rejects it locally with `webcodex task accept <id>` / `webcodex task reject
  <id>`. The model can never accept its own work.

### Validation recipes

`checks_run` accepts `format`, `check`, and `test` plus an optional `recipe`
enum (`rust`, `node`, `python`, `go`). Omit `recipe` for automatic resolution
from the nearest `Cargo.toml`, `package.json`, `pyproject.toml`, or `go.mod`
relative to the task `cwd`. Recipes do not install dependencies, mutate
lockfiles, or use the network. A missing tool is an executor failure; a
started validator returning non-zero is an assertion failure.

| Recipe | Marker | `format` | `check` | `test` |
| --- | --- | --- | --- | --- |
| Rust | `Cargo.toml` | `cargo fmt -- --check` | `cargo check --all-targets` | `cargo test` |
| Node | `package.json` | first of `format:check`, `format-check`, `check:format` | first of `check`, `typecheck`, `lint` | exact `test` |
| Python | `pyproject.toml` | configured Ruff/Black | configured Ruff/Mypy | configured pytest |
| Go | `go.mod` | unavailable | `go vet ./...` | `go test ./...` |

### Long validation continues as a Job

A long `checks_run` (or `cargo_*`) that outlives the synchronous grace period
continues as a queryable Job with the same `job_id`. Poll `job_status` /
`job_log`, or read `validation_summary`; do not re-run the command to find the
answer. `stop_job(confirm=true)` stops a promoted job.

## First safe prompt

```text
Use the configured WebCodex project. Start a read-only task, read README.md,
summarize the project, review the result, and finish. Do not edit files.
```

No project discovery or runtime identifier belongs in this prompt.

## Read and search bounds

- `read_file` is a bounded streaming range reader: `start_line` (default 1),
  `limit` (default 2000, max 2000), returns the range plus the complete-file
  SHA-256 and line metadata, and a `next_start_line` to continue.
- `read_files` batches up to 8 single-file reads with independent item
  results.
- `search_project_text` is the default search tool (ripgrep first, bounded in
  work and bytes); `search_project_texts` batches up to 8 queries.

Failures return small structured errors with project-relative paths only —
never absolute paths, commands, or Runner stderr.

## Common errors

| Code | Meaning | Action |
| --- | --- | --- |
| `project_not_configured` | No canonical setup exists | Run `webcodex setup` |
| `project_credential_invalid` | Private Project Credential is missing or mismatched | Restore both matching private files or recreate the profile |
| `project_credential_rejected` | The reachable server rejected the credential | Restore the server-matching credential |
| `workspace_unavailable` | The configured Git workspace is unavailable | Restore the workspace, then run doctor |
| `server_unreachable` / `agent_offline` | The project runtime or Agent is unavailable | Run `webcodex run` / `webcodex doctor` |
| `required_capability_unavailable` | The Agent lacks a coding capability | Upgrade all binaries |
| `task_not_active` | The task can no longer mutate or execute | Start a new task |
| `execution_not_terminal` | Finish is blocked by active/unknown work | Review/wait/cancel |
| `checks_required` | A normal task has not run checks | Call `checks_run` |
| `checks_stale` | The workspace changed after the last check | Run a new check |

## Advanced runtime surface

Beyond the project-bound Connector, WebCodex can run as a multi-project
management ToolRuntime with discovery, session, LSP, raw job, and artifact
tools. That is an advanced surface for operators, not the canonical project
Connector and not a prerequisite for ordinary coding. See
[Architecture](ARCHITECTURE.md) and the `webcodex` CLI for operator tooling.
