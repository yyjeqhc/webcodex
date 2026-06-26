# Private Drop Runtime

Private Drop is a self-hosted tool runtime for ChatGPT. It exposes local project
capabilities through a single `ToolRuntime` that is shared by GPT Actions,
MCP, and the REST wrappers used by this GPT.

```text
ChatGPT GPT Action      ChatGPT MCP client
        |                       |
        v                       v
   /openapi.json              /mcp
        \                       /
         v                     v
              ToolRuntime
        read | git | patch | shell | jobs | codex
              |
    private-drop-agent -> local working tree
```

Current direction:

- GPT Actions import `GET /openapi.json`.
- MCP clients connect to `POST /mcp`.
- Projects are registered by `private-drop-agent` clients.
- The server is a **zero-project-config relay** for the normal runtime surface;
  do not set `PROJECTS_CONFIG` expecting it to register runtime projects.
- Codex is an **optional advanced capability**. Read, diff, patch, and shell
  actions work without Codex installed.
- The old file-drop / Web UI / workflow / SSH product direction is removed from
  the active server surface.

## Build and install

Private Drop needs a Rust toolchain with `cargo`:

```bash
cargo build --release
```

The release build produces:

```text
target/release/private-drop
target/release/private-drop-agent
```

See [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md) for the short build, install,
server, agent, GPT Actions, and MCP setup guide.

## Run the server locally

```bash
DROP_TOKEN="change-me" \
DROP_ADDR="127.0.0.1:8080" \
DROP_PUBLIC_URL="http://127.0.0.1:8080" \
cargo run --bin private-drop
```

Useful endpoints:

```text
GET  /openapi.json   GPT Actions schema
POST /mcp            MCP JSON-RPC endpoint
POST /api/tools/list Runtime tool discovery
```

Protected endpoints use Bearer auth:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/runtime/status \
  -H "Content-Type: application/json" \
  -d '{}'
```

## Run an agent

Agent-side project files live under `projects_dir`, one `*.toml` per project:

```toml
# /etc/private-drop-agent/projects.d/private-drop.toml
id = "private-drop"
path = "/root/git/private-drop"
name = "Private Drop"
allow_patch = true
kind = "rust"
description = "Private Drop Runtime repository"
```

Agent config example:

```toml
server_url = "https://drop.example.com"
token = "REPLACE_WITH_DROP_TOKEN"
client_id = "workstation-1"
display_name = "Workstation"
owner = "you"
transport = "websocket"
poll_interval_ms = 1000
projects_dir = "/etc/private-drop-agent/projects.d"

[capabilities]
shell = true
file_read = true
file_write = true
git = true
jobs = true
async_jobs = true
async_shell_jobs = true

[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

Start the agent:

```bash
cargo run --bin private-drop-agent -- --config /etc/private-drop-agent/agent.toml
```

`websocket` is the preferred long-lived transport. `polling` remains available
as a fallback for restricted networks. Registered project ids are namespaced as:

```text
agent:<client_id>:<project_id>
```

For example:

```text
agent:workstation-1:private-drop
```

Never commit real `agent.toml`, env files, tokens, or machine-local
`projects.d/` entries. Use the examples under `deploy/` instead.

## GPT Actions

Import this URL in the GPT Builder Actions screen:

```text
https://<your-server>/openapi.json
```

Configure authentication as an HTTP API key in the `Authorization` header with
value:

```text
Bearer <DROP_TOKEN>
```

GPT Actions and MCP are peer surfaces over the same `ToolRuntime`. GPT
Actions expose a small typed OpenAPI surface with stable operation ids; MCP
exposes the runtime tool set directly. The underlying project/agent execution
path is the same.

Dedicated GPT Actions:

| operationId | Purpose |
| --- | --- |
| `getRuntimeStatus` | Runtime health, agents, project config state, and job counts. |
| `listProjects` | List agent-registered runtime project ids. |
| `readProjectFile` | Read a UTF-8 file from a project. |
| `getProjectGitStatus` | Run `git status --porcelain`. |
| `getProjectGitDiff` | Run `git diff` with optional args/path scoping. |
| `applyProjectPatch` | Apply a unified diff patch. Executable mutation. |
| `runProjectShellCommand` | Run a bounded shell command in a project. Executable. |
| `runCodexTask` | Optional Codex CLI task, returns a `job_id`. |
| `getRuntimeJobStatus` | Poll an async job. |
| `getRuntimeJobLog` | Read bounded job stdout/stderr. |
| `listRuntimeTools` | Advanced runtime tool discovery. |
| `callRuntimeTool` | Generic escape hatch; prefer typed actions above. |

Recommended tool-driven development flow, whether driven through GPT Actions
or MCP:

1. `getRuntimeStatus` / `runtime_status` — confirm the runtime is healthy and
   the agent is online.
2. `listProjects` / `list_projects` — select the runtime project id.
3. `getProjectGitStatus` / `git_status` and `getProjectGitDiff` / `git_diff` —
   inspect repository state.
4. `readProjectFile` / `read_file` — read focused source, config, and docs.
5. `runProjectShellCommand` / `run_shell` — run diagnostics such as
   `cargo check`, `cargo test`, or script syntax checks.
6. `validate_patch` — MCP/runtime dry-run patch preflight; it does not modify
   the worktree and is suitable for full-auto loops before `apply_patch`.
7. `apply_patch_checked` — validate, apply, and return the post-apply diff
   summary in one safer runtime/MCP step.
8. `delete_project_files`, `git_restore_paths`, or `discard_untracked` — use
   restricted cleanup tools instead of ad hoc `rm` when possible.
9. `applyProjectPatch` / `apply_patch` — apply small patches directly when the
   caller already performed preflight checks.
10. `runCodexTask` / `run_codex` — optional advanced path when Codex CLI is
    installed.

See [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) for examples, schema guarantees,
and executable-action risk notes.

## MCP

`/mcp` speaks JSON-RPC 2.0 over HTTP and is protected by the same Bearer token
as the REST API.

Supported methods:

- `initialize`
- `ping`
- `tools/list`
- `tools/call`
- `notifications/initialized`

GPT Actions and MCP share the same `ToolRuntime`; the MCP layer only frames and
translates JSON-RPC messages. See [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md),
[docs/AGENT_PROTOCOL.md](docs/AGENT_PROTOCOL.md), and
[docs/RUNTIME_STATUS.md](docs/RUNTIME_STATUS.md) for the detailed surfaces.

## Codex CLI jobs

Codex is optional. Only `runCodexTask` / `run_codex` require the Codex CLI on
the agent host. The rest of the runtime works through read, git, patch, shell,
and job actions.

Relevant environment variables:

| Variable | Default | Description |
| --- | --- | --- |
| `CODEX_BIN` | `codex` | Codex CLI binary name or path. |
| `CODEX_APPROVAL_MODE` | empty/disabled | Empty, blank, `none`, `off`, or `disabled` omit `--approval-mode`. Other values enable it. |
| `CODEX_DEFAULT_TIMEOUT_SECS` | `3600` | Default Codex job timeout. |
| `CODEX_MAX_PROMPT_BYTES` | `100000` | Maximum prompt size. |
| `CODEX_ALLOWED_EXTRA_ARGS` | empty | Comma-separated allowlist for optional extra CLI args. |

Use an empty `CODEX_APPROVAL_MODE` when the installed Codex CLI does not support
`--approval-mode`.

## Verify

Fast local checks:

```bash
cargo check
cargo test
```

Full local E2E smoke with a stub Codex CLI, server, WebSocket agent, GPT Actions
schema checks, and MCP checks:

```bash
bash scripts/e2e_zero_config_ws.sh
```

Polling transport smoke:

```bash
E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh
```

Deployment smoke against a live public instance:

```bash
DROP_PUBLIC_URL="https://drop.example.com" \
DROP_TOKEN="<token>" \
bash scripts/smoke_deployment.sh
```

## Deploy

Deployment samples live under [`deploy/`](deploy/):

- `private-drop.service.example`
- `private-drop.env.example`
- `private-drop-agent.service.example`
- `private-drop-agent.toml.example`
- `agent-project.toml.example`
- `projects.d/private-drop.toml.example`
- `nginx.private-drop.example.conf`

The deployment guide covers server env vars, agent config, reverse proxy / TLS,
GPT Actions import, MCP, smoke tests, and WebSocket online/stale troubleshooting:

- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)

## Documentation

Start here:

- [docs/INDEX.md](docs/INDEX.md) — current documentation map.
- [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) — GPT Actions import and usage.
- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) — production deployment.
- [docs/AGENT_PROTOCOL.md](docs/AGENT_PROTOCOL.md) — polling/WebSocket agent protocol.
- [docs/E2E_VALIDATION.md](docs/E2E_VALIDATION.md) — local E2E validation.
- [TODO.md](TODO.md) — current backlog.
