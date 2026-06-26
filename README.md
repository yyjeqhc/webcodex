# Private Drop Runtime

Private Drop is now a self-hosted tool runtime for ChatGPT:

- GPT Actions import `/openapi.json`.
- ChatGPT MCP clients connect to `/mcp`.
- Both surfaces call the same `ToolRuntime`.
- Project execution is handled by registered `private-drop-agent` clients; the server does not need project path/client mappings.

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

## Projects

Runtime projects are registered by agents. The server does not need a
server-side `projects.toml` block that maps project ids to local paths or
`client_id`s.

Agent-side project files describe local projects and are reported during agent
registration:

```toml
id = "private-drop"
path = "/root/git/private-drop"
name = "Private Drop"
allow_patch = true
kind = "rust"
```

`listProjects` returns runtime ids in the form
`agent:<client_id>:<project_id>`, for example
`agent:workstation-1:private-drop`.

Polling is the fallback agent transport. WebSocket is the preferred long-lived
transport (Phase 13); QUIC is a later transport target. Both transports feed
the same `ShellClientRegistry` / job queue / `ToolRuntime`, so there is no
second business-logic path.

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
- `POST /api/projects/list` → `listProjects`: list agent-registered project ids.
- `POST /api/projects/read_file` → `readProjectFile`: read a UTF-8 file from a
  project through the owning agent.
- `POST /api/projects/git_status` → `getProjectGitStatus`: run
  `git status --porcelain` in a project.

All are thin HTTP wrappers that dispatch to the same `ToolRuntime`
variants used by MCP. See [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) for the
full import guide and recommended call flow, and
[docs/RUNTIME_STATUS.md](docs/RUNTIME_STATUS.md) for the observability guide.

### Recommended troubleshooting flow

1. `getRuntimeStatus` — is the runtime healthy? Are agents registered? Are
   agents online?
2. `listProjects` — which project ids are available?
3. `listRuntimeTools` — which tools are exposed?
4. `runCodexTask` — start a Codex task in a project.
5. `getRuntimeJobStatus` / `getRuntimeJobLog` — poll the returned `job_id`.

## Audit API (read-only)

A small admin/debug surface for inspecting the action-session audit trail. It is
**not** a GPT Action and is not included in `/openapi.json`. All endpoints are
`POST` + Bearer auth and perform no write operations.

- `POST /api/audit/sessions` — list recent sessions (default 50, max 200;
  optional `status` filter).
- `POST /api/audit/session` — fetch one session summary + decoded events
  (`session_id` required; `events_limit` default 50, max 200).
- `POST /api/audit/stats` — aggregate `ActionSessionStats`, optionally scoped to
  a `session_id`.

Event payloads are passed through a strict read-time sanitizer that drops
secret-like keys entirely (token/api_key/password/etc.) and strips raw
stdout/stderr/diff; no full prompts or secrets are ever returned. See
[docs/AUDIT_API.md](docs/AUDIT_API.md) for full details.

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

### Local job lifecycle & timeout

Local executor jobs (started by `run_job` / `run_codex`) run in their own
process group via `setsid`, and the leader pid is recorded as
`process_group_id` in `.codex/jobs/<job_id>/metadata.json`. This lets the
runtime reclaim a whole subtree on timeout or stop rather than orphaning child
processes.

- **Timeout.** When `job_status` or `job_log` detect a `running` local job
  whose `started_at + max_runtime_secs` has passed, the runtime sends `SIGTERM`
  to the whole process group (`kill -TERM -<pgid>`), escalates to `SIGKILL` if
  the group is still alive after a short grace window, and persists a terminal
  `lost` status plus a `note` describing what happened. The process group is
  never left running in the background.
- **Stop.** An internal `POST /api/jobs/stop` endpoint (with `{"job_id": ...}`)
  stops a running local job by terminating its process group and marking it
  `stopped`. This is a thin REST wrapper over `ToolRuntime::stop_job` and is
  **not** exposed as a GPT Action (it is deliberately absent from
  `openapi.json`) so remote ChatGPT callers cannot drive an explicit kill.
  Already-terminal jobs are left untouched.
- **Old metadata without pid/pgid.** Local job metadata written before
  pid/process-group tracking existed has no `pid` file or
  `process_group_id`. On timeout/stop the runtime only marks such jobs
  `lost`/`stopped` — it never guesses a pid to kill.

Security: only jobs the runtime itself created and recorded (in memory or
recoverable on disk) can be stopped. The pid/pgid come exclusively from the
job's own on-disk files, never from caller input, and `job_id` is validated by
`is_safe_job_id` against path traversal.

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

The agent supports two transports, selected by the `transport` config field:

- `websocket` (preferred): one long-lived WebSocket connection to
  `GET /api/agents/ws`. The server pushes requests; the agent executes them
  and streams `result` / `job_update` back. Bearer auth is sent in the
  handshake `Authorization` header.
- `polling` (fallback, default): the agent polls `POST /api/shell/agent/poll`
  on an interval. Use this for restricted networks or older agents.

Both transports reuse the same execution path (`run_shell`, `handle_file_request`,
`JobManager`) and the same server-side registry, queue, and job state.

Reliability guarantees (Phase 14):

- **Backpressure**: per-client pending requests are capped
  (`MAX_QUEUED_REQUESTS_PER_CLIENT = 256`); overflow is rejected with a
  structured error rather than growing unbounded. Outbound `request` messages
  are never silently dropped; `pong` keepalives are best-effort.
- **Reconnect**: on disconnect the server marks the agent's running jobs `lost`
  and removes its push notifier, so an agent is never permanently `online` and a
  job is never permanently `running`. A reconnecting agent should treat `lost`
  jobs as needing a restart.
- **Owner/auth**: at registration, a bootstrap token may register any `owner`;
  a normal API key may only register `owner == <username>`. Applied identically
  to polling and WebSocket.

QUIC is a future transport target; it is not implemented yet. The
`AgentEnvelope` wire format is already transport-neutral.

```bash
cargo run --bin private-drop-agent -- --config /etc/private-drop-agent/agent.toml
```

Example config (WebSocket preferred):

```toml
server_url = "https://your-server.example"
token = "change-me"
client_id = "workstation-1"
display_name = "Workstation"
owner = "you"
transport = "websocket"
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

Omit `transport` (or set `transport = "polling"`) to use the polling fallback.
The agent announces `websocket-v1` / `polling-v1` as its protocol version; the
transport label is also visible in `runtime_status` and `list_agents`.

## Verification

```bash
cargo check
cargo test
```

For a real end-to-end smoke (server + WebSocket agent + GPT Actions + MCP on
one host, with a stub Codex CLI — no ChatGPT or real Codex required):

```bash
bash scripts/e2e_zero_config_ws.sh
```

See [docs/E2E_VALIDATION.md](docs/E2E_VALIDATION.md) for what it covers, env
vars, and how to map the same flow to a real ChatGPT GPT Action import.

## Deployment

To deploy the runtime so a real ChatGPT can reach it, see
[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md). It covers server environment variables
(`DROP_ADDR`, `DROP_TOKEN`, `DROP_DATA`, `DROP_PUBLIC_URL`, `CODEX_*`), agent
configuration (`server_url`, `token`, `client_id`, `owner`,
`transport = "websocket"`, `projects_dir`), reverse proxy / HTTPS, ChatGPT GPT
Actions import URL (`/openapi.json`), MCP endpoint URL (`/mcp`), smoke tests,
and the troubleshooting order.

Deployment helpers in this repo:

- systemd + env samples: [`deploy/`](deploy/) —
  `private-drop.service.example`, `private-drop.env.example`,
  `private-drop-agent.service.example`, `private-drop-agent.toml.example`,
  `agent-project.toml.example`, `projects.d/private-drop.toml.example`.
- nginx reverse proxy sample: `deploy/nginx.private-drop.example.conf`.
- Deployment smoke (against a live instance):
  `bash scripts/smoke_deployment.sh` (uses `DROP_PUBLIC_URL` + `DROP_TOKEN`).
- Local full E2E smoke: `bash scripts/e2e_zero_config_ws.sh`.

The server is a **zero-project-config relay**: it does not need a `projects.toml`
as a runtime project source. Projects are registered by agents as
`agent:<client_id>:<project_id>`. QUIC is a future transport and is not
implemented.
