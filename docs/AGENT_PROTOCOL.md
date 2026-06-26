# Agent Protocol

This document describes the wire protocol between `webcodex-agent` and the
WebCodex server. There are two transports, but they share the same
business semantics: the server authenticates and routes; the agent owns local
execution; the server never executes local repository commands as the normal
path.

## Transports

| Transport   | Endpoint                      | Direction        | Status                          |
| ----------- | ----------------------------- | ---------------- | ------------------------------- |
| `websocket` | `GET /api/agents/ws`          | Long-lived, push | Preferred (Phase 13)            |
| `polling`   | `POST /api/shell/agent/*`     | Poll-based        | Fallback (default)              |
| `quic`      | (future)                      | Long-lived        | Future; envelope is compatible  |

Both implemented transports feed the same `ShellClientRegistry`, the same
per-client request queue, the same job state, and the same `ToolRuntime`.
There is no second business-logic path for WebSocket.

## Authentication

Both transports require Bearer auth (`WEBCODEX_TOKEN` or an API key):

- Polling: `Authorization: Bearer <token>` on every request (or `?token=`).
- WebSocket: `Authorization: Bearer <token>` in the handshake request headers.

Auth is enforced by the shared `AuthMiddleware`. The WebSocket endpoint is
mounted under the same authenticated `/api` router as the polling endpoints.

### Owner binding at registration

The `owner` field in `register` is bound to the authenticated principal, on
both transports, by `enforce_register_owner` (which reuses the existing
`assert_shell_client_owner` rule):

- A bootstrap token (or auth disabled) may register any `owner`.
- A normal API key may only register `owner == <api key username>`.
- A normal API key with a missing/empty `owner` is rejected.

This closes the gap where any valid token could register a client under an
arbitrary owner. Polling and WebSocket register follow the same rule.

## Registration

The agent registers its `client_id`, capabilities, projects (id / path /
`allow_patch` / kind / git state), and a protocol version:

- Polling agent announces `agent_protocol_version = "polling-v1"`.
- WebSocket agent announces `agent_protocol_version = "websocket-v1"`.

Runtime project ids are namespaced as `agent:<client_id>:<project_id>`. The
server does not need a server-side `projects.toml` to resolve runtime project
ids; project paths and policies come from agent registration.

## Polling protocol (fallback)

- `POST /api/shell/agent/register` — register client + projects.
- `POST /api/shell/agent/poll` — long-poll the next pending request for this
  client. Returns at most one `ShellAgentShellRequest` or `None`.
- `POST /api/shell/agent/result` — submit the result of a synchronous
  shell/file request.
- `POST /api/shell/agent/job_update` — push an incremental or final update for
  an async job.

The agent polls on `poll_interval_ms`. The server records `last_seen` on every
poll/register/result/job_update; a client whose `last_seen` ages past the
online window is reported `stale`/`offline`.

## WebSocket protocol (preferred)

### Transport-neutral envelope

All WebSocket traffic is JSON text messages using a single internally-tagged
envelope (`AgentEnvelope`, defined in `src/shell_protocol.rs`). The same
envelope is intended to carry over QUIC later without changes.

```jsonc
// agent -> server (first message after handshake)
{"type":"register","client_id":"...","projects":[...],"agent_protocol_version":"websocket-v1",...}

// server -> agent (ack)
{"type":"registered","success":true,"client":{...}}

// server -> agent (push a pending request)
{"type":"request","request_id":"...","client_id":"...","kind":"run_shell","command":"...",...}

// agent -> server (synchronous result)
{"type":"result","client_id":"...","request_id":"...","exit_code":0,"stdout":"...",...}

// agent -> server (incremental/final job state)
{"type":"job_update","client_id":"...","job_id":"...","status":"running","stdout_chunk":"...",...}

// either direction (keepalive)
{"type":"ping","ts":1700000000}
{"type":"pong","ts":1700000000}

// server -> agent (fatal protocol error; agent should reconnect)
{"type":"error","code":"...","message":"..."}
```

### Lifecycle

1. Agent opens `GET /api/agents/ws` with `Authorization: Bearer <token>`.
2. Agent sends `register`. Server calls `ShellClientRegistry::register`, flips
   the client `transport` to `"websocket"`, installs a push notifier, and
   replies `registered`.
3. Server runs a request pump: it pops pending requests from the client's
   shared queue (the same queue polling serves) and pushes each as a `request`
   envelope. When idle it waits on the notifier that the registry fires on
   enqueue.
4. Agent executes each `request` via the shared dispatch path
   (`run_shell` / `handle_file_request` / `JobManager`) and sends `result`
   (synchronous) or `job_update` (async job) back. Server routes these to
   `ShellClientRegistry::complete` / `update_job`.
5. Either side may send `ping`; the other replies `pong`.
6. On disconnect the server runs `reconcile_disconnect`: it removes the push
   notifier and marks every non-final running-like job owned by the client as
   `lost` (with its pending request dropped). The client record is retained and
   `last_seen` is left untouched, so the client decays to `stale`/`offline`
   through the normal 60s online window — it is never left permanently
   `online`, and a job is never left permanently `running`.

### Request kinds

The `request.kind` field selects execution (identical to polling):

- `run_shell` — synchronous shell command; agent returns `result`.
- `file_read` / `file_write` / `file_list` — synchronous file op; agent returns
  `result`.
- `start_job` — async job; agent streams `job_update` until `finished: true`.
- `stop_job` — stop a running/queued local job.

## Backpressure

Memory and liveness are bounded conservatively in both directions:

- **Outbound (server → agent) `request`**: critical, never silently dropped. The
  request pump uses a bounded mpsc (`OUTGOING_CHANNEL_CAPACITY = 64`) and blocks
  on send until the agent reads; if the writer task detects a closed socket it
  tears down the pump.
- **Outbound `pong`**: best-effort keepalive. Sent with `try_send`; a saturated
  outbound channel drops the pong rather than stalling inbound processing. The
  agent treats a missing pong as a soft liveness signal, not a fatal error.
- **Per-client pending queue cap** (`MAX_QUEUED_REQUESTS_PER_CLIENT = 256`): the
  hard memory ceiling. Once a client's shared queue reaches this depth, new
  enqueues (`enqueue_run`, `enqueue_file_op`, `start_job`, `stop_job`) are
  rejected with a structured `too many pending requests` error instead of
  growing unboundedly. This protects the registry when an agent is slow or dead
  and the pump cannot drain.
- **Inbound (agent → server) `job_update`**: processed sequentially under the
  registry lock; job stdout/stderr are capped by `append_limited` /
  `replace_limited` (`MAX_OUTPUT_BYTES`). The agent side uses `blocking_send`
  for `result` and `job_update`, which naturally throttles the update rate to
  what the server can ingest and fails fast when the channel closes.

A slow consumer therefore never deadlocks the server's enqueue path (enqueue
never blocks on the transport), and total in-flight requests per client are
bounded by the outbound channel plus the queue cap.

## Reconnect and job reconciliation

Each transport session reconciles conservatively on disconnect
(`ShellClientRegistry::reconcile_disconnect`):

- The push notifier is removed so the request pump is not re-armed.
- Every non-final job in a running-like state (`queued` / `agent_queued` /
  `running` / `stop_requested`) owned by the client is marked `lost` with error
  `agent transport disconnected`, and its pending request/waiter is dropped.
- The client record is retained (late results/updates are logged), and
  `last_seen` is left untouched so the client decays to `stale`/offline within
  the 60s online window.

Trade-off (intentional, conservative): a reconnecting agent that keeps running
the same job will see the server-side job as `lost` (final); its late
`job_update`/`result` is ignored by `update_job`/`complete`. Operators should
treat `lost` as "the server no longer tracks this job; restart it if needed".
A future phase may lift `JobManager` to agent-level so reconnects can resume
in-flight jobs.

## Observability

`runtime_status` and `list_agents` expose, per agent:

- `client_id`, `status` (`online` / `stale`), `connected`
- `agent_protocol_version` (`polling-v1` / `websocket-v1` / `unknown`)
- `transport` (`polling` / `websocket`)
- `pending_requests` (depth of the shared per-client request queue)
- `capabilities`, `projects_count`

No tokens, API keys, secrets, full environment, or Authorization headers are
exposed.

## Constraints

- The server never executes local repository commands as the normal path.
- WebSocket does not introduce a second `ToolRuntime`, a second agent job
  queue, or transport-specific execution logic.
- Polling remains fully supported and is the default when `transport` is
  omitted.
- The server does not need a server-side `projects.toml` to resolve runtime
  project ids; project paths and policies come from agent registration, as
  `agent:<client_id>:<project_id>`.
- Per-client pending requests are bounded (`MAX_QUEUED_REQUESTS_PER_CLIENT`);
  a disconnect never leaves an agent permanently `online` or a job permanently
  `running`.
- `owner` at registration is bound to the authenticated principal on both
  transports; QUIC is not implemented yet (future transport; envelope is
  already transport-neutral).
