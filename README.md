# Private Drop Runtime

Private Drop is now a self-hosted tool runtime for ChatGPT:

- GPT Actions import `/openapi.json`.
- ChatGPT MCP clients connect to `/mcp`.
- Both surfaces call the same `ToolRuntime`.
- Local or remote execution is handled by configured projects and optional `private-drop-agent` clients.

The old file-drop/web UI direction has been removed from the active server surface.

## Current Shape

```text
ChatGPT GPT Action      ChatGPT MCP client
        |                       |
        v                       v
   /openapi.json              /mcp
        \                       /
         v                     v
              ToolRuntime
        shell | git | patch | jobs | codex
              |
    local project or private-drop-agent
```

## Build

```bash
cargo build --release
```

## Run

```bash
DROP_TOKEN="change-me" \
PROJECTS_CONFIG="./projects.toml" \
DROP_ADDR="127.0.0.1:8080" \
cargo run --bin private-drop
```

OpenAPI for GPT Actions:

```text
http://127.0.0.1:8080/openapi.json
```

MCP endpoint:

```text
http://127.0.0.1:8080/mcp
```

Use Bearer auth for protected endpoints:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/tools/list \
  -H "Content-Type: application/json" \
  -d '{}'
```

## Project Config

`PROJECTS_CONFIG` points to a TOML file:

```toml
[projects.private-drop]
path = "/root/git/private-drop"
executor = "local"
allow_patch = true

[projects.remote-demo]
path = "/root/git/remote-demo"
executor = "agent"
client_id = "workstation-1"
allow_patch = true
```

`executor = "local"` runs on the server host. `executor = "agent"` queues work for a registered `private-drop-agent`.

## Runtime Tools

The active tool list is returned by:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/tools/list \
  -H "Content-Type: application/json" \
  -d '{}'
```

Current tools:

- `list_tools`
- `list_projects`
- `list_agents`
- `run_shell`
- `run_job`
- `run_codex`
- `job_status`
- `job_log`
- `read_file`
- `git_status`
- `git_diff`
- `apply_patch`

Generic tool call:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/tools/call \
  -H "Content-Type: application/json" \
  -d '{"tool":"git_status","params":{"project":"private-drop"}}'
```

## Codex CLI Jobs

`run_codex` starts Codex CLI asynchronously and returns a `job_id`.

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/codex/run \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","prompt":"Inspect the codebase and summarize the runtime architecture."}'
```

Defaults:

- `CODEX_BIN=codex`
- `CODEX_APPROVAL_MODE=full-auto`

Poll status:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/jobs/status \
  -H "Content-Type: application/json" \
  -d '{"job_id":"<job-id>"}'
```

Read logs:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/jobs/log \
  -H "Content-Type: application/json" \
  -d '{"job_id":"<job-id>"}'
```

## MCP

Minimal JSON-RPC methods:

- `initialize`
- `ping`
- `tools/list`
- `tools/call`

Example:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Tool call:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_projects","arguments":{}}}'
```

## Agent

The agent is still a polling execution client:

```bash
cargo run --bin private-drop-agent -- --config /etc/private-drop-agent/agent.toml
```

Example config:

```toml
server_url = "https://your-server.example"
token = "change-me"
client_id = "workstation-1"
display_name = "Workstation"
owner = "you"
poll_interval_ms = 1000

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

## Verification

```bash
cargo check
cargo test
```
