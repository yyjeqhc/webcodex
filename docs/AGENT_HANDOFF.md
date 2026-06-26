# Agent Handoff Notes

This file exists for future coding agents after context compaction or a fresh
window. Read it before making changes.

## Project Identity

Private Drop Runtime is now a self-hosted tool runtime for ChatGPT. ChatGPT can
connect through:

- GPT Actions: `GET /openapi.json`
- MCP: `POST /mcp`

Both surfaces must use the same backend execution layer: `ToolRuntime`.

Current direction: the server should be zero project config. Runtime projects
are registered by agents and exposed as ids like
`agent:<client_id>:<project_id>`. Do not make `projects.toml` the normal
runtime project source again.

## Current Baseline

Latest known baseline when this file was written:

- Branch: `v2-mcp-runtime`
- Commit: `3a0352e Harden local job lifecycle`
- Main binary: `private-drop`
- Agent binary: `private-drop-agent`

Always run `git status --short --untracked-files=all` first. The user often has
uncommitted work from another agent window; do not overwrite or revert it.

## Core Files

- `src/tool_runtime.rs`: shared tool execution layer. Put business logic here
  or in modules it calls.
- `src/runtime_http.rs`: thin REST/GPT Actions wrappers.
- `src/mcp.rs`: thin MCP JSON-RPC wrapper.
- `src/openapi.rs`: minimal GPT Actions OpenAPI schema.
- `src/main.rs`: route wiring and shared state injection.
- `src/projects.rs`: legacy server-side project config parser. Treat this as
  non-primary for runtime project discovery unless a task explicitly says
  otherwise.
- `src/shell_client.rs`: polling agent registry and in-memory agent job queue.
  Now also carries the `transport` label (`polling`/`websocket`) and a push
  notifier map used by the WebSocket handler.
- `src/agent_ws.rs`: WebSocket agent endpoint (`GET /api/agents/ws`). Thin: only
  translates between `AgentEnvelope` and `ShellClientRegistry` calls.
- `src/shell_protocol.rs`: shared protocol types including the transport-neutral
  `AgentEnvelope`.
- `src/bin/private-drop-agent.rs`: agent process. Selects `polling` (default) or
  `websocket` transport via config; both reuse `dispatch_request` / `JobManager`
  through an `AgentSink` abstraction.
- `src/action_sessions.rs`, `src/action_audit.rs`, `src/audit_http.rs`: audit
  storage and read-only admin/debug API.

## Live Public/Integration Surfaces

GPT Actions schema exposes a small stable set of POST operations. Check
`src/openapi.rs` tests for the exact operation ID set.

Important runtime endpoints:

- `POST /api/runtime/status`
- `GET /api/agents/ws` (WebSocket, preferred agent transport; Bearer auth in
  handshake header)
- `POST /api/shell/agent/register` / `/poll` / `/result` / `/job_update`
  (polling agent fallback)
- `POST /api/projects/list`
- `POST /api/projects/read_file`
- `POST /api/projects/git_status`
- `POST /api/codex/run`
- `POST /api/jobs/status`
- `POST /api/jobs/log`
- `POST /api/tools/list`
- `POST /api/tools/call`
- `POST /mcp`

Admin/debug only:

- `POST /api/audit/sessions`
- `POST /api/audit/session`
- `POST /api/audit/stats`
- `POST /api/jobs/stop`

Do not expose admin/debug write-like operations as GPT Actions without a
specific safety review.

## Architectural Invariants

- `ToolRuntime` is the single execution layer.
- MCP and REST wrappers stay thin: auth/protocol envelope/deserialization only.
- Runtime project discovery comes from agent registration, not server-side
  project config.
- Do not create a second Codex runner, shell runner, or MCP-specific business
  path.
- Project file access is routed to the owning registered agent; server-side
  project path config is not the runtime source of truth.
- Agent tools must keep owner and capability checks centralized.
- Do not log or return tokens, API keys, full secrets, full prompts containing
  credentials, or unbounded stdout/stderr.
- Keep GPT Actions small and stable. Prefer dedicated safe read-only actions
  plus `runCodexTask`; keep `callRuntimeTool` advanced.

## Job Lifecycle Notes

Local jobs are written under `.codex/jobs/<job_id>/`.

Current behavior:

- Local job metadata includes `kind`, `max_runtime_secs`, and current jobs write
  `process_group_id`.
- `job_status` and `job_log` can recover local jobs from disk after restart.
- Over-time running local jobs are marked `lost` and the recorded process group
  is terminated when possible.
- `stop_job` exists for local job lifecycle control and is wired to
  `POST /api/jobs/stop`, but it is not a GPT Action.
- Old metadata without `pid` / `process_group_id` is never guessed; it can only
  be marked terminal.

## Testing Commands

Run these before reporting completion unless the user explicitly says not to:

```bash
cargo fmt
cargo fmt --check
cargo check
cargo check --tests
cargo test
```

Expected current result:

- `cargo check`: 0 warnings.
- `cargo check --tests`: 0 warnings.
- `cargo test`: main binary 327 tests passing, agent binary 21 tests passing.

If `cargo test` hangs, do not assume the test suite is too large. Use:

```bash
timeout 120 cargo test -- --test-threads=1 --nocapture
```

The previous hang was caused by holding `local_jobs` across recovery in
`stop_job`; that is fixed. Env-var tests use a shared test lock to avoid
parallel pollution.

## Documentation Map

Start with:

- `docs/INDEX.md`
- `docs/ROADMAP.md`
- `docs/ZERO_CONFIG_AGENT_RUNTIME.md`
- `TODO.md`
- `README.md`

Historical phase plan:

- `docs/GLM52_DEVELOPMENT_PLAN.md`

Deprecated docs are retained only for history. Do not use them as integration
guides.

## Next Likely Work

Prefer this order:

1. Finish zero-config agent runtime cleanup.
2. WebSocket agent transport is implemented (Phase 13) and hardened
   (Phase 14): per-client pending-queue cap (`MAX_QUEUED_REQUESTS_PER_CLIENT`),
   conservative `reconcile_disconnect` (marks running jobs `lost` on
   disconnect), and `enforce_register_owner` (binds `owner` to the authed
   principal at registration on both transports). Polling fallback is
   unchanged and green. Not yet committed — confirm before committing.
3. Keep the message envelope QUIC-compatible for a later QUIC transport
   (Phase 15, design only; not implemented).
4. Real ChatGPT GPT Actions and MCP connection validation.
5. Deployment hardening docs and examples.

Avoid broad refactors until real ChatGPT integration has been exercised.
