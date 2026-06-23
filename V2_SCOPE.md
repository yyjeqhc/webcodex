# Private Drop v2 — Remote MCP Tool Runtime

## Vision

Private Drop v2 is a **Remote MCP Tool Runtime**: a self-hosted server that exposes
local machine capabilities (shell, git, file, patch, jobs) as standardized tool
endpoints, consumable by both MCP clients and GPT Actions.

## External Access Layers (parallel)

| Layer | Endpoint | Protocol | Status |
|-------|----------|----------|--------|
| **MCP over HTTP** | `/mcp` | MCP (Model Context Protocol) | TODO — future |
| **GPT Actions** | `/openapi.json` | OpenAPI 3.0 + Bearer auth | Preserved |

Both layers call the **same tool runtime** underneath. No separate business logic
per access layer.

## Core Architecture

```
┌──────────────┐   ┌──────────────┐
│  MCP Client  │   │ GPT Actions  │
│  (future)    │   │ (OpenAPI)    │
└──────┬───────┘   └──────┬───────┘
       │                  │
       ▼                  ▼
  /mcp (TODO)       /openapi.json
       │                  │
       └────────┬─────────┘
                │
        ┌───────▼────────┐
        │  Tool Runtime   │  ← shared execution layer
        │  - shell        │
        │  - apply_patch  │
        │  - git status   │
        │  - git diff     │
        │  - read_file    │
        │  - run_job      │
        │  - job_status   │
        │  - job_log      │
        └───────┬────────┘
                │
        ┌───────▼────────┐
        │ Agent Transport │  ← local or remote execution
        │ - local (now)   │
        │ - WebSocket (v2)│
        └────────────────┘
```

## Retained Capabilities

| Capability | Module | Notes |
|-----------|--------|-------|
| HTTP server | `main.rs`, Salvo | Lightweight, async |
| Token auth | `auth.rs`, `config.rs` | Bearer token for all API |
| Config loading | `config.rs` | Env files, env vars |
| Project registry | `projects.rs` | Simplified — local + agent only |
| Agent connection | `shell_client.rs` | Will migrate to WebSocket |
| Shell tool | `codex/shell.rs`, `shell_client.rs` | `run_shell` |
| Apply patch | `codex/patch.rs` | `apply_patch` / `applyProjectEdit` |
| Git status/diff | `codex/git.rs` | `gitStatus`, `gitDiff` |
| Run job / job log | `codex/jobs.rs`, `shell_client.rs` | Async job execution |
| Action sessions | `action_sessions.rs` | Audit trail for tool calls |
| GPT Actions OpenAPI | `openapi.rs` (rebuild) | Minimal, clean schema |
| Web UI (minimal) | `web.rs` | Debug/status pages only |
| Core message/file API | `drop_api.rs` | Retained as utility |

## Deleted / Deprecated

| Feature | Reason |
|---------|--------|
| Desktop tasks | Not part of v2 direction |
| Multiple OpenAPI variants | One clean GPT Actions endpoint only |
| SSH executor | Agent + future WebSocket replaces it |
| command_request / goal workflow | Old chat-approved flow, not needed |
| Codex doctor / hooks / workflow runner | Old orchestration, not needed |
| Complex file-transfer Web UI | Not part of v2 |
| Old polling agent as primary path | Will be replaced by WebSocket transport |

## GPT Actions — Required operationIds

These must exist in the OpenAPI spec and map to working endpoints:

- `listProjects` — list configured projects
- `getProjectContext` / `getProjectContextBatch` — read project context
- `readFile` — read a file from a project
- `applyPatch` / `applyProjectEdit` — apply unified diff
- `gitStatus` — git status of a project
- `gitDiff` — git diff of a project
- `runShell` — execute a shell command
- `runJob` — start an async job
- `jobStatus` — check job status
- `jobLog` — get job stdout/stderr

All backed by the same tool runtime as MCP. Bearer token auth. Clean JSON schemas.

## Non-Goals

- Model inference / LLM hosting
- Multi-tenant SaaS
- Complex file transfer / cloud storage replacement
- Browser automation / desktop task orchestration
- Real-time collaborative editing
