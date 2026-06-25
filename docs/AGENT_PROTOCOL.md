# Agent Protocol (Polling)

Private Drop Runtime drives remote execution through a **polling** agent
protocol. There is no WebSocket or SSE transport in this phase; the agent
binary (`private-drop-agent`) periodically polls the server for work.

This document describes the wire protocol as implemented by
`src/shell_protocol.rs`, `src/shell_client.rs`, and
`src/bin/private-drop-agent.rs`. GPT Actions (`/openapi.json`) and MCP
(`/mcp`) do **not** speak this protocol directly — they call `ToolRuntime`,
which enqueues work onto the same in-memory queues described here.

## Transport

- Wire format: JSON over HTTP.
- Auth: HTTP Bearer (`Authorization: Bearer <DROP_TOKEN>`), the same token
  used by the runtime API. When `DROP_TOKEN` is unset the server runs in
  development mode and auth is bypassed.
- All endpoints are `POST` with a JSON body and a JSON response.
- The agent polls on a fixed interval (`poll_interval_ms`, default 1000ms).

## Endpoints

All four endpoints live under `/api/shell/agent/*` and require Bearer auth
(when auth is enabled).

### 1. Register — `POST /api/shell/agent/register`

The agent announces itself and its capabilities. Called once on startup and
again whenever a poll fails (the agent re-registers to refresh `last_seen`).

Request (`ShellClientRegisterRequest`):

| field                    | type     | required | notes |
|--------------------------|----------|----------|-------|
| `client_id`              | string   | yes      | Stable agent id. ASCII letters/digits/`-`/`_`/`.`, 1–80 chars. |
| `display_name`           | string   | no       | Human label. |
| `owner`                  | string   | no       | Username that owns this agent. Drives the owner boundary (see below). |
| `hostname`               | string   | no       | Agent host. |
| `capabilities`           | object   | no       | Capability flags (see below). Defaults to `shell=true`, everything else false. |
| `projects`               | array    | no       | Project summaries the agent can execute against. |
| `agent_protocol_version` | string   | no       | Protocol version announced by the agent. Current agent sends `"polling-v1"`. Missing → `"unknown"`. |

Response (`ShellClientRegisterResponse`): `{ success, client?, error? }` where
`client` is a `ShellClientView`.

### 2. Poll — `POST /api/shell/agent/poll`

The agent asks for the next pending request. The server also uses this call to
refresh `last_seen` and optionally update the agent's project list.

Request (`ShellAgentPollRequest`):

| field       | type   | required | notes |
|-------------|--------|----------|-------|
| `client_id` | string | yes      | The agent's `client_id`. |
| `projects`  | array  | no       | If present, replaces the agent's registered project summaries. |

Response (`ShellAgentPollResponse`): `{ success, request?, error? }`. `request`
is a `ShellAgentShellRequest` when there is pending work, otherwise `null`.

### 3. Result — `POST /api/shell/agent/result`

For **synchronous** shell/file requests, the agent posts the final result
exactly once.

Request (`ShellAgentResultRequest`): `client_id`, `request_id`, and the
optional `exit_code`, `stdout`, `stderr`, `duration_ms`, `error`.

Response (`ShellAgentResultResponse`): `{ success, error? }`.

### 4. Job Update — `POST /api/shell/agent/job_update`

For **asynchronous jobs**, the agent streams progress and the final status.

Request (`ShellAgentJobUpdateRequest`): `client_id`, `job_id`, `status`,
optional `stdout_chunk`/`stderr_chunk` (append), optional
`stdout_tail`/`stderr_tail` (replace), `exit_code`, `duration_ms`, `error`,
and `finished` (boolean).

Response (`ShellAgentJobUpdateResponse`): `{ success, job?, error? }`.

## Request kinds (`ShellAgentShellRequest.kind`)

The server enqueues a `ShellAgentShellRequest` and the agent dispatches on
`kind`:

| kind          | sync/async | agent action |
|---------------|------------|--------------|
| `run_shell`   | sync       | Run `command` via `sh -c`, return result on `/result`. |
| `file_read`   | sync       | Read `path` (bounded by `max_bytes`), return on `/result`. |
| `file_write`  | sync       | Write `content` to `path` (optional `expected_sha256`), return on `/result`. |
| `file_list`   | sync       | List `path` directory, return on `/result`. |
| `start_job`   | async      | Spawn `command` (setsid), stream updates via `/job_update`. |
| `stop_job`    | async      | Stop a running job by `job_id`. |

## Sync shell vs async job

- **Sync shell** (`run_shell`, `file_*`): the runtime holds a `oneshot` waiter
  while the agent runs the command. The HTTP request blocks up to
  `wait_timeout_secs` (≤ 120s) for the agent's `/result`. If the agent does not
  respond in time, the request is cancelled and the caller gets a timeout
  error.
- **Async job** (`start_job`): the runtime creates a `ShellJobRecord` with
  `status=queued`, enqueues a `start_job` request, and returns a `job_id`
  immediately. The agent streams `running` → terminal status via `/job_update`.
  Callers poll `job_status`/`job_log` (runtime tools) or
  `/api/shell/jobs/status` (legacy).

## How the server enqueues work

1. A caller invokes a runtime tool (`run_shell`, `read_file`, `git_status`,
   `run_job`, `run_codex`, …) for an **agent** project.
2. `ToolRuntime` resolves the project, checks the caller is allowed to drive
   the agent (owner boundary + capability), then calls
   `ShellClientRegistry::enqueue_run` / `enqueue_file_op` / `start_job`.
3. The registry pushes a `request_id` onto `queues_by_client[client_id]` and
   stores a `PendingShellRequest` (with a `oneshot` waiter for sync calls).
4. The agent's next `/poll` pops the request and runs it.

## How the agent polls

`private-drop-agent` runs a loop:

1. `register` on startup.
2. `poll` every `poll_interval_ms`.
3. If a request is returned:
   - `start_job` → enqueue locally (bounded concurrency) and stream
     `/job_update`.
   - `stop_job` → stop the local job.
   - `file_*` → run the file op and `result`.
   - other → run `sh -c` and `result`.
4. On poll error, sleep and re-`register`.

## Capabilities

`ShellClientCapabilities` flags (registered with the agent):

| flag               | default | meaning |
|--------------------|---------|---------|
| `shell`            | `true`  | Agent can run `sh -c` commands (sync `run_shell`, `apply_patch` via `git apply`, `git status`/`git diff`). |
| `file_read`        | `false` | Agent can read files (`read_file` runtime tool). |
| `file_write`       | `false` | Agent can write files. |
| `git`              | `false` | Agent can run git commands. `git_status`/`git_diff` accept `shell` **or** `git`. |
| `jobs`             | `false` | Agent tracks jobs. |
| `async_jobs`       | `false` | Agent can run async jobs (`run_job`/`run_codex`). |
| `async_shell_jobs` | `false` | Agent can run async shell jobs. `run_job`/`run_codex` require `async_jobs` **or** `async_shell_jobs`. |

The current `private-drop-agent` registers with `shell=true` (default),
`file_read=true`, `file_write=true`, `jobs=true`, `async_jobs=true`,
`async_shell_jobs=true`.

`ToolRuntime::authorize_agent_tool` enforces these before dispatching any
agent-backed tool. Missing capability → structured error:
`agent client <id> does not support <shell|file_read|shell or git|async shell jobs>`.

## `agent_protocol_version`

Declared during register. Current values:

- `"polling-v1"` — current `private-drop-agent` builds.
- `"unknown"` — agents that registered before the field existed (backwards
  compatibility; old agents keep working).

The version is stored on the client record and returned by `list_agents` /
`ShellClientView`. The server does not yet reject unknown versions; it is
intended for observability and future capability negotiation.

## Owner field and permission boundary

Each agent registers an optional `owner` username. The owner boundary is
enforced in two places, both using `assert_shell_client_owner`:

1. **Legacy `/api/shell/*` handlers** — check the caller's API key username
   matches the agent's `owner` (bootstrap tokens always pass).
2. **Runtime paths** (`/api/tools/call`, `/api/codex/run`,
   `/api/projects/{list,read_file,git_status}`, `/mcp` `tools/call`) —
   `ToolRuntime::dispatch_with_auth` calls `authorize_agent_tool`, which runs
   the same owner check before any agent-backed tool runs. This closes the gap
   where runtime paths previously bypassed owner enforcement.

Rules:

- **Bootstrap token** (the `DROP_TOKEN`, or dev mode with auth disabled):
  allowed to drive any agent.
- **API key** (non-bootstrap): allowed only if the key's username equals the
  agent's `owner`.
- **No auth context** (`dispatch` without auth, e.g. internal calls): agent
  tools are rejected because no owner can be proven. Local-executor projects
  are unaffected.

This means a non-owner API key cannot use `run_shell`, `read_file`,
`git_status`, `run_job`, `run_codex`, `apply_patch`, or `git_diff` against an
agent project through any runtime path.

## Structured agent errors

Agent-backed runtime tools return explicit error strings:

| situation | error string |
|-----------|--------------|
| Agent client_id not registered | `unknown shell client: <id>` |
| Caller is not the agent owner | `agent client <id> is owned by <owner>; current api key belongs to <user>` |
| Missing capability | `agent client <id> does not support <capability>` |
| Sync wait timed out | `timed out waiting <n> seconds for agent shell result` |
| Waiter dropped | `shell request waiter was dropped` |
| Agent policy deny | forwarded verbatim from the agent's `error` field |

## Known limitations

- **In-memory queues.** Pending requests, job records, and the agent registry
  live in process memory. A server restart loses in-flight agent jobs (local
  on-disk jobs can be recovered via `.codex/jobs/<id>/metadata.json`).
- **Polling latency.** Work starts only on the agent's next poll
  (`poll_interval_ms`). There is no push notification.
- **Single-server.** The registry is not shared across server instances; each
  agent polls one server.
- **No flow control.** A slow agent backs up its per-client queue; there is no
  global backpressure across clients beyond the sync wait timeout.
- **Owner boundary is username-based.** Agents without an `owner` cannot be
  driven by non-bootstrap API keys.
