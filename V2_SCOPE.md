# Private Drop v2 — Remote Tool Runtime

## Vision

Private Drop v2 is a **self-hosted tool runtime for ChatGPT**: a server that
exposes local machine capabilities (shell, git, file, patch, jobs, Codex CLI)
as standardized tool endpoints, consumable by both **MCP clients** and
**GPT Actions**. Both access layers call the same `ToolRuntime` — there is no
separate business logic per surface.

## External Access Layers (parallel, both implemented)

| Layer | Endpoint | Protocol | Status |
|-------|----------|----------|--------|
| **MCP over HTTP** | `/mcp` | MCP (JSON-RPC 2.0 over streamable-http) | Implemented |
| **GPT Actions** | `/openapi.json` | OpenAPI 3.1 + Bearer auth | Implemented |

Both layers dispatch to the **same `ToolRuntime`** underneath. The MCP wrapper
only frames the JSON-RPC envelope; the GPT Actions wrapper only maps HTTP to
`ToolCall` variants. No business logic is duplicated.

## Core Architecture

```
┌──────────────┐   ┌──────────────┐
│  MCP Client  │   │ GPT Actions  │
│              │   │ (OpenAPI)    │
└──────┬───────┘   └──────┬───────┘
       │                  │
       ▼                  ▼
     /mcp            /openapi.json
       │                  │
       └────────┬─────────┘
                │
        ┌───────▼────────┐
        │  Tool Runtime   │  ← shared execution layer
        │  - list_tools   │
        │  - list_projects│
        │  - list_agents  │
        │  - runtime_status│
        │  - run_shell    │
        │  - run_job      │
        │  - run_codex    │
        │  - job_status   │
        │  - job_log      │
        │  - read_file    │
        │  - git_status   │
        │  - git_diff     │
        │  - apply_patch  │
        └───────┬────────┘
                │
        ┌───────▼────────┐
        │ Agent Transport │  ← local or remote execution
        │ - local (now)   │
        │ - polling-v1    │
        └────────────────┘
```

## Retained Capabilities

| Capability | Module | Notes |
|-----------|--------|-------|
| HTTP server | `main.rs`, Salvo | Lightweight, async |
| Token auth | `auth.rs`, `config.rs` | Bearer token for all API + `/mcp` |
| Config loading | `config.rs` | Env files, env vars |
| Project registry | `projects.rs` | Local + agent executors |
| Shared tool runtime | `tool_runtime.rs` | Single execution layer for GPT Actions + MCP |
| GPT Actions OpenAPI | `openapi.rs` | Minimal, POST-only, Bearer auth |
| MCP JSON-RPC | `mcp.rs` | `initialize`, `ping`, `tools/list`, `tools/call`, notifications |
| REST/GPT Actions wrappers | `runtime_http.rs` | Thin dispatch to `ToolRuntime` |
| Codex CLI jobs | `tool_runtime.rs`, `codex` | `run_codex` async jobs with bounded logs |
| Local job recovery | `tool_runtime.rs` | `.codex/jobs/<id>/metadata.json` recovery |
| Agent connection | `shell_client.rs`, `shell_protocol.rs` | Polling (`polling-v1`) |
| Agent binary | `bin/private-drop-agent.rs` | Polling execution client |
| Runtime observability | `tool_runtime.rs`, `runtime_http.rs` | `runtime_status` tool + `POST /api/runtime/status` |
| Action audit (internal) | `action_sessions.rs`, `action_audit.rs` | Metadata-only audit for legacy codex routes |

## Removed / Not part of v2

These are **not** retained capabilities and must not be reintroduced:

| Feature | Reason |
|---------|--------|
| File-drop / message / channel / Web UI | Old product direction, removed from active server surface |
| `drop_api.rs` / `web.rs` message/file API | Replaced by the tool runtime |
| Desktop task orchestration | Not part of v2 direction |
| SSH executor | Removed; use the polling agent instead |
| `command_request` / goal workflow | Old chat-approved flow, not needed |
| `project_workflow` / `project_doctor` / `project_hook` routes | Old orchestration, not mounted |
| Codex doctor / hooks / workflow runner | Old orchestration, not needed |
| Multiple OpenAPI variants (`/codex-openapi*.json`) | One clean GPT Actions endpoint (`/openapi.json`) only |

## GPT Actions — Required operationIds

These are the exact operation ids exposed by `/openapi.json` (see
`src/openapi.rs`). Tests assert this set matches the generated schema exactly:

- `listRuntimeTools` — `POST /api/tools/list`
- `listProjects` — `POST /api/projects/list`
- `getRuntimeStatus` — `POST /api/runtime/status`
- `runCodexTask` — `POST /api/codex/run`
- `getRuntimeJobStatus` — `POST /api/jobs/status`
- `getRuntimeJobLog` — `POST /api/jobs/log`
- `readProjectFile` — `POST /api/projects/read_file`
- `getProjectGitStatus` — `POST /api/projects/git_status`
- `callRuntimeTool` — `POST /api/tools/call` (advanced generic entry point)

All backed by the same `ToolRuntime` as MCP. Bearer token auth. Clean JSON
schemas. The GPT Actions surface is intentionally POST-only and does not expose
raw shell, file transfer, desktop, or the internal agent protocol routes.

## Agent Protocol

The server-to-agent transport is currently **polling-v1** (JSON over HTTP under
`/api/shell/agent/*`). It is an internal execution transport, not part of the
GPT Actions or MCP surface. See `docs/AGENT_PROTOCOL.md`. A WebSocket/SSE
transport is a possible future addition, not a current requirement.

## Non-Goals

- Model inference / LLM hosting
- Multi-tenant SaaS
- Complex file transfer / cloud storage replacement
- Browser automation / desktop task orchestration
- Real-time collaborative editing
- Restoring the old file-drop / Web UI product surface
