# Zero-Config Agent Runtime

This document defines the deployment direction for WebCodex Runtime before
WebSocket/QUIC transport work.

## Goal

The server should be a zero-project-config relay:

- It authenticates callers and agents.
- It receives agent registrations.
- It routes GPT Actions / MCP tool calls to the correct registered agent.
- It records audit and runtime status.
- It does not need a server-side `projects.toml` to know project ids, local
  paths, or agent client mappings.

The agent owns local machine knowledge:

- project id
- project path
- project policy such as `allow_patch`
- capabilities
- transport support

## Current Initial Implementation

The runtime project surface has started moving to agent registration:

- `ShellAgentProjectSummary` includes `allow_patch`.
- `webcodex-agent` reads `allow_patch` from project files and reports it
  during registration.
- `listProjects` returns agent-registered projects using ids:
  `agent:<client_id>:<project_id>`.
- Runtime tool project resolution accepts agent-registered project ids.
- Server-side configured project ids are no longer resolved by the runtime
  surface.

The old server-side project config code may still exist in legacy/internal
modules during the transition. It is not the target runtime model.

## Project Id Format

Runtime project ids are namespaced:

```text
agent:<client_id>:<project_id>
```

Example:

```text
agent:workstation-1:webcodex
```

This makes routing explicit and avoids collisions between agents.

## Agent Project File

Agent-side project files should describe local projects. Example:

```toml
id = "webcodex"
path = "/root/git/webcodex"
name = "WebCodex"
allow_patch = true
kind = "rust"
description = "WebCodex Runtime repository"
```

The server should not need the matching server-side block:

```toml
[projects.webcodex]
executor = "agent"
client_id = "workstation-1"
path = "/root/git/webcodex"
```

## Transport Direction

WebSocket is the preferred long-lived transport and is implemented (Phase 13).
Polling remains a fallback. The target order is:

1. WebSocket agent connection as the primary long-lived transport
   (`GET /api/agents/ws`). Implemented.
2. Polling fallback for restricted networks and old agents
   (`POST /api/shell/agent/poll`). Implemented, unchanged semantics.
3. QUIC transport later, after the message envelope is stable. Envelope is
   designed to be transport-neutral so QUIC can reuse it.

The transport never duplicates business logic. All transports feed the same
`ShellClientRegistry`, the same per-client request queue, the same job state,
and the same `ToolRuntime`. The server's WebSocket handler only translates
between the `AgentEnvelope` wire format and registry method calls.

### Transport-neutral envelope

A single `AgentEnvelope` (defined in `src/shell_protocol.rs`) is used by the
WebSocket transport and is intended for QUIC later. It wraps the existing
polling payloads so no business types are duplicated:

- `register` / `registered`
- `request` (server pushes a pending shell/file/job request)
- `result` (agent returns a synchronous result)
- `job_update` (agent streams incremental/final job state)
- `ping` / `pong`
- `error`

### Observability

`runtime_status` and `list_agents` expose each agent's `transport`
(`polling` / `websocket`), `agent_protocol_version`, `connected`/`status`, and
`pending_requests` (depth of the shared per-client request queue). No tokens,
API keys, or secrets are exposed.

When a WebSocket agent disconnects, `reconcile_disconnect` removes its push
notifier and marks its running-like jobs `lost`; the client then decays to
`stale`/`offline` based on `last_seen`, exactly like a polling agent that
stops polling. An agent is never left permanently `online` and a job is never
left permanently `running`.

## Next Engineering Steps

1. Finish removing server-side project config from current runtime docs and
   tests.
2. WebSocket reconnect and backpressure hardening is implemented (Phase 14):
   per-client pending queue cap, conservative `lost` reconciliation on
   disconnect, and `owner`/auth binding at registration. A future phase may
   lift `JobManager` to agent-level so reconnects can resume in-flight jobs
   instead of marking them `lost`.
3. Add QUIC transport as another `AgentEnvelope` carrier without changing
   `ToolRuntime` (design only for now; not implemented).
4. Keep polling as fallback with the same request/result/job_update semantics.

## Non-Goals

- Do not bring back Web UI / file-drop / SSH executor / command_request.
- Do not make the server execute local repository commands as the normal path.
- Do not create WebSocket-specific tool execution logic.
- Do not expose agent stop/kill/admin APIs as GPT Actions by default.
