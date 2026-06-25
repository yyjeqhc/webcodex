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

GPT Actions and MCP share a single `ToolRuntime`. See
[docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) for the ChatGPT import guide
(import URL, Bearer auth, recommended call flow, and operation list), and
[docs/INDEX.md](docs/INDEX.md) for the full documentation reading order.

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
- `runtime_status`
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

## GPT Actions (dedicated endpoints)

`/openapi.json` exposes a small, stable set of GPT Actions. Beyond the generic
`callRuntimeTool`, dedicated read-only actions give ChatGPT named
operations without guessing tool names:

- `POST /api/runtime/status` → `getRuntimeStatus`: structured runtime
  health/observability summary (service metadata, projects config status, agent
  client summaries, job counts). Read-only; never exposes tokens or secrets.
- `POST /api/projects/list` → `listProjects`: list configured project ids.
- `POST /api/projects/read_file` → `readProjectFile`: read a UTF-8 file from a
  project (paths confined to the project root).
- `POST /api/projects/git_status` → `getProjectGitStatus`: run
  `git status --porcelain` in a project.

All are thin HTTP wrappers that dispatch to the same `ToolRuntime`
variants used by MCP. See [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) for the
full import guide and recommended call flow, and
[docs/RUNTIME_STATUS.md](docs/RUNTIME_STATUS.md) for the observability guide.

### Recommended troubleshooting flow

1. `getRuntimeStatus` — is the runtime healthy? Are projects configured? Are
   agents online?
2. `listProjects` — which project ids are available?
3. `listRuntimeTools` — which tools are exposed?
4. `runCodexTask` — start a Codex task in a project.
5. `getRuntimeJobStatus` / `getRuntimeJobLog` — poll the returned `job_id`.

## Codex CLI Jobs

`run_codex` starts Codex CLI asynchronously and returns a `job_id`. It is the
recommended high-level action for ChatGPT-driven Codex tasks. It constructs the
Codex command for the caller — GPT should not assemble raw shell to run Codex.

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/codex/run \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","prompt":"Inspect the codebase and summarize the runtime architecture."}'
```

The response includes:

- `job_id` — use this to poll status and logs.
- `kind` — always `"codex"`.
- `project` — the project the job runs in.
- `status_endpoint` — `"/api/jobs/status"`.
- `log_endpoint` — `"/api/jobs/log"`.

### Codex configuration

Codex behavior is controlled by `CODEX_*` environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `CODEX_BIN` | `codex` | Codex CLI binary name or path. |
| `CODEX_APPROVAL_MODE` | `full-auto` | Approval mode passed via `--approval-mode`. |
| `CODEX_DEFAULT_TIMEOUT_SECS` | `3600` | Default job timeout when the request omits `timeout_secs`. |
| `CODEX_MAX_PROMPT_BYTES` | `100000` | Maximum prompt size in bytes. Larger prompts are rejected. |
| `CODEX_ALLOWED_EXTRA_ARGS` | _(empty)_ | Comma-separated allowlist of accepted `extra_args`. |

`extra_args` defaults to **not allowed**. To permit additional Codex CLI flags,
set `CODEX_ALLOWED_EXTRA_ARGS` to a comma-separated list, for example:

```bash
CODEX_ALLOWED_EXTRA_ARGS="--verbose,--json,--no-color"
```

Any `extra_args` element not present in the allowlist is rejected. Empty or
whitespace-only entries in the list are ignored.

Prompt validation:

- Empty prompts are rejected.
- Prompts containing NUL bytes are rejected.
- Prompts exceeding `CODEX_MAX_PROMPT_BYTES` are rejected.

Poll status:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/jobs/status \
  -H "Content-Type: application/json" \
  -d '{"job_id":"<job-id>"}'
```

Read logs (bounded; supports `offset` and `tail_lines`):

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/jobs/log \
  -H "Content-Type: application/json" \
  -d '{"job_id":"<job-id>"}'
```

## MCP

`/mcp` speaks JSON-RPC 2.0 over HTTP (streamable-http-jsonrpc transport). It is
protected by the same Bearer token as the REST API (`DROP_TOKEN`). When
`DROP_TOKEN` is set, every request to `/mcp` must carry
`Authorization: Bearer <token>`; requests without a valid token are rejected
with `401 Unauthorized`.

MCP and GPT Actions share a single `ToolRuntime` — there is no separate
business logic for either surface. The MCP wrapper only frames the JSON-RPC
envelope and translates tool results into MCP content blocks.

Supported methods:

- `initialize` — returns `protocolVersion`, `serverInfo`, and capabilities.
- `ping` — returns an empty result.
- `tools/list` — returns the same tool list as `POST /api/tools/list`.
- `tools/call` — calls a tool by `name` with `arguments`.
- `notifications/initialized` — a notification (no `id`). The server replies
  with `HTTP 202 Accepted` and an empty body, per the JSON-RPC notification
  convention. Any request without an `id` member is treated as a notification.

`GET /mcp` returns discovery metadata: server name, version, protocol version,
endpoint, supported methods, and an auth hint.

### Tool result shape

`tools/call` always returns a normal JSON-RPC `result` (never a protocol
`error`) containing:

- `content` — array of `{ "type": "text", "text": "..." }` blocks.
- `structuredContent` — `{ "success", "output", "error" }`.
- `isError` — `true` when the tool failed, `false` otherwise.

A business failure (e.g. unknown project, command exit code != 0) is reported
as `isError: true` inside a normal result. JSON-RPC protocol errors are only
used for:

- invalid `jsonrpc` version (`-32600`),
- unknown method (`-32601`),
- invalid params shape (`-32602`),
- unknown tool name / deserialization failure (`-32602`).

### Examples

Initialize:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
```

List tools:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
```

Call a tool:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_projects","arguments":{}}}'
```

Send `notifications/initialized` (no `id`, server replies `202` with empty body):

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}'
```

Discovery (`GET /mcp`):

```bash
curl -H "Authorization: Bearer change-me" \
  http://127.0.0.1:8080/mcp
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
