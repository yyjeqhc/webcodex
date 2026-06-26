# WebCodex Runtime Roadmap

This document is the forward-looking development plan after the initial
ChatGPT runtime build-out. Historical phase-by-phase planning notes were removed
from the active docs set; this file is the current planning source.

## Current State

The runtime MVP is implemented:

- GPT Actions schema at `GET /openapi.json`.
- MCP HTTP JSON-RPC endpoint at `POST /mcp`.
- Shared `ToolRuntime` execution layer for GPT Actions, MCP, and REST wrappers.
- Codex CLI async jobs through `run_codex`.
- Polling agent protocol `polling-v1` with version, capabilities, owner checks,
  and structured errors.
- Initial zero-config runtime project discovery: agent-registered projects are
  listed as `agent:<client_id>:<project_id>` and server-side project config is
  no longer the runtime project source.
- Runtime observability through `runtime_status`.
- Read-only Audit API for admin/debug.
- Documentation cleanup for removed legacy product surfaces.

The main engineering rule remains: new capabilities go through `ToolRuntime`;
MCP and GPT Actions wrappers stay thin.

## Near-Term Priorities

### Phase 12: Finish Zero-Config Agent Runtime

Goal: complete the transition from server-configured projects to
agent-registered projects.

Tasks:

- Remove current-doc references that present `PROJECTS_CONFIG` as the runtime
  project source.
- Update runtime status wording around projects to focus on agent-registered
  projects.
- Add tests for agent-registered `readProjectFile`, `getProjectGitStatus`, and
  `runCodexTask` routing.
- Decide whether remaining legacy `ProjectsState` code should be deleted now
  or left only for deprecated `/api/codex/*` internals.

Acceptance:

- The server can start with no `projects.toml`.
- `listProjects` works when an authenticated agent registers projects.
- Static server-side project config is not required for normal GPT/MCP use.

### Phase 13: WebSocket Agent Transport

Goal: make WebSocket the primary long-lived agent transport while keeping
polling as fallback.

Status: implemented.

- Transport-neutral `AgentEnvelope` (`src/shell_protocol.rs`) wraps the
  existing register/request/result/job_update payloads; no business types
  duplicated.
- Server endpoint `GET /api/agents/ws` (`src/agent_ws.rs`): Bearer auth via the
  shared `AuthMiddleware`, registers into the existing `ShellClientRegistry`,
  installs a push notifier, and pumps pending requests from the shared
  per-client queue. Result / job_update / ping are routed back to the same
  registry methods the polling endpoints use.
- Agent WebSocket mode in `src/bin/webcodex-agent.rs` selected by
  `transport = "websocket"`. It reuses the shared `dispatch_request` /
  `JobManager` execution path through an `AgentSink` abstraction; polling mode
  is unchanged and remains the default/fallback.
- `runtime_status` and `list_agents` expose `transport`
  (`polling` / `websocket`); disconnect removes the notifier so the agent
  decays to stale/offline.
- Polling endpoints remain fully functional.

Acceptance:

- One agent can stay connected over WebSocket and execute a low-risk runtime
  tool call. (Covered by `agent_ws::tests::ws_register_then_request_result_roundtrip`.)
- Polling agents continue to work. (Polling tests unchanged.)

### Phase 14: WebSocket Hardening And Phase 13 Consolidation

Goal: converge the Phase 13 WebSocket foundation into committable quality and
close the most critical long-connection risks. QUIC is explicitly out of scope
for this phase.

Status: implemented.

- Consolidated the mixed Phase 13 workspace (staged + unstaged + untracked
  `src/agent_ws.rs`) into a single coherent change set; no commit was made
  (pending human confirmation).
- Backpressure: per-client pending queue cap
  (`MAX_QUEUED_REQUESTS_PER_CLIENT = 256`) rejects overflow with a structured
  error; outbound `request` is never silently dropped; `pong` is best-effort
  (`try_send`); inbound `job_update` stdout/stderr stay capped. A slow consumer
  never deadlocks the enqueue path.
- Reconnect / job reconciliation: `reconcile_disconnect` removes the notifier
  and marks running-like jobs `lost` on transport disconnect, so an agent is
  never permanently `online` and a job is never permanently `running`.
- Owner / auth boundary: `enforce_register_owner` binds `owner` to the
  authenticated principal at registration on both polling and WebSocket
  (bootstrap = any owner; API key = `owner == username`).
- Observability: `runtime_status` / `list_agents` expose `transport`,
  `agent_protocol_version`, `connected`/`status`, and `pending_requests`; no
  secrets are exposed.
- Tests: `cargo fmt`/`fmt --check`, `cargo check`, `cargo check --tests` all
  clean (0 warnings); `cargo test` = main 327 passed, agent 21 passed; polling
  fallback tests still pass.

Acceptance:

- WebSocket and polling share one registry, one queue, one job state, one
  `ToolRuntime` (no second execution path).
- Polling fallback unchanged and green.
- Long-connection risks (backpressure, reconnect, owner boundary) covered by
  tests.

### Phase 15: QUIC Transport Design

Goal: prepare QUIC support without coupling runtime logic to a transport.

Tasks:

- Keep the WebSocket message envelope compatible with QUIC.
- Document stream/session/auth expectations.
- Defer implementation until WebSocket behavior is stable.

Acceptance:

- QUIC can be added as another transport without changing ToolRuntime.

### Phase 16: Real ChatGPT Connection Validation

Goal: prove the zero-config runtime works from real ChatGPT surfaces, not only
tests.

Tasks:

- Start a local or deployed server with a real `WEBCODEX_TOKEN`.
- Connect an agent and confirm `listProjects` sees agent projects.
- Import `GET /openapi.json` into a GPT Action.
- Exercise the recommended GPT Actions flow.
- Connect a ChatGPT MCP client to `/mcp`, then run `initialize`,
  `tools/list`, and a low-risk `tools/call`.

Acceptance:

- A real GPT Action can call at least one read-only agent project operation.
- A real MCP client can list tools and call a read-only agent project tool.

### Phase 17: Deployment Hardening

Goal: reduce abuse and long-term maintenance risk.

Tasks:

- Add rate limiting for expensive endpoints.
- Add audit retention or export policy.
- Consider admin-only cleanup for old audit rows and job directories.
- Keep destructive admin operations out of GPT Actions unless there is a clear
  safety model.

Acceptance:

- Operators have a documented way to bound storage and request load.

### Phase 18: MCP App Console And Approval Surface

Goal: keep GPT Actions as the model-callable development interface while adding
an MCP App console for human observation, diff review, and approval.

Tasks:

- Add structured read-only console tools for file listing/search, file ranges,
  diff summaries, job listing, and job tails.
- Build runtime/agent status panels that make WebSocket stale transitions,
  transport, last heartbeat, pending requests, and project counts obvious.
- Build project selection, file browser, and read-only file preview around
  agent-registered runtime project ids.
- Add patch validation and an explicit approval flow before
  `applyProjectPatch`.
- Add command and job/log panels with high-risk execution warnings and bounded
  output.
- Keep all business logic in `ToolRuntime`; MCP, GPT Actions, REST wrappers,
  and the app remain protocol/UI layers.

Acceptance:

- A user can see runtime health, current project state, diffs, patch previews,
  command output, and job tails without reading raw JSON.
- Patch application and shell execution require clear human confirmation in the
  app.
- GPT Actions and MCP continue to share one runtime layer.
- Removed legacy workflow, SSH, file-drop, and old dashboard surfaces are not
  reintroduced.

See [MCP_APP_CONSOLE_PLAN.md](MCP_APP_CONSOLE_PLAN.md) for the staged plan and
copy/paste implementation prompts.

## Deferred Ideas

- SSE agent transport. WebSocket is now the preferred long-lived transport;
  SSE is not planned.
- Exposing audit summaries to ChatGPT. Prefer aggregated `runtime_status`
  signals first; raw audit query APIs should remain admin/debug by default.
- More dedicated GPT Actions. The current small surface is intentional.

## Do Not Reintroduce

- File-drop / message / channel product surface.
- Web UI as the main product surface.
- Desktop task orchestration.
- SSH executor.
- `command_request` / goal workflow.
- Legacy OpenAPI variants.
