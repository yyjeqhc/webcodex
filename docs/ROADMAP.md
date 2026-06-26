# Private Drop Runtime Roadmap

This document is the forward-looking development plan after the initial
ChatGPT runtime build-out. The historical phase plan is kept in
`docs/GLM52_DEVELOPMENT_PLAN.md`; this file is the current planning source.

## Current State

The runtime MVP is implemented:

- GPT Actions schema at `GET /openapi.json`.
- MCP HTTP JSON-RPC endpoint at `POST /mcp`.
- Shared `ToolRuntime` execution layer for GPT Actions, MCP, and REST wrappers.
- Codex CLI async jobs through `run_codex`.
- Local job recovery, bounded logs, timeout detection, and process-group
  termination for over-time or stopped local jobs.
- Polling agent protocol `polling-v1` with version, capabilities, owner checks,
  and structured errors.
- Runtime observability through `runtime_status`.
- Read-only Audit API for admin/debug.
- Documentation cleanup for removed legacy product surfaces.

The main engineering rule remains: new capabilities go through `ToolRuntime`;
MCP and GPT Actions wrappers stay thin.

## Near-Term Priorities

### Phase 12: Real ChatGPT Connection Validation

Goal: prove the runtime works from real ChatGPT surfaces, not only tests.

Tasks:

- Start a local or deployed server with a real `DROP_TOKEN`.
- Import `GET /openapi.json` into a GPT Action.
- Exercise the recommended GPT Actions flow:
  `getRuntimeStatus` -> `listProjects` -> `readProjectFile` /
  `getProjectGitStatus` -> `runCodexTask` -> `getRuntimeJobStatus` ->
  `getRuntimeJobLog`.
- Connect a ChatGPT MCP client to `/mcp`, then run `initialize`,
  `tools/list`, and a low-risk `tools/call`.
- Document the exact setup, screenshots or request/response snippets, failure
  modes, and any schema/MCP compatibility changes needed.

Acceptance:

- A real GPT Action can call at least one read-only project operation.
- A real MCP client can list tools and call a read-only tool.
- Any manual setup steps are written in docs.

### Phase 13: Deployment Hardening

Goal: make the server safe and repeatable to run outside a dev shell.

Tasks:

- Write a deployment guide covering `DROP_TOKEN`, `DROP_PUBLIC_URL`,
  `PROJECTS_CONFIG`, `CODEX_*`, reverse proxy, HTTPS, and systemd.
- Add a sample systemd unit and environment file template.
- Add production health/smoke commands for `/api/runtime/status`,
  `/openapi.json`, and `/mcp` discovery.
- Review startup logs for deploy usefulness.

Acceptance:

- A new operator can deploy the runtime from docs without reading source code.
- Secrets are passed through env files or secret stores, not committed config.

### Phase 14: Agent Queue Durability

Goal: decide whether in-flight agent jobs should survive server restart.

Tasks:

- Document current in-memory behavior and operational consequences.
- If needed, persist queued/running agent job metadata.
- On restart, mark ambiguous in-flight agent jobs as `lost` or reattach when the
  polling agent can prove ownership.
- Keep owner/capability checks centralized.

Acceptance:

- Server restart behavior for agent jobs is explicit, tested, and documented.

### Phase 15: Operational Controls

Goal: reduce abuse and long-term maintenance risk.

Tasks:

- Add rate limiting for expensive endpoints.
- Add audit retention or export policy.
- Consider admin-only cleanup for old audit rows and job directories.
- Keep destructive admin operations out of GPT Actions unless there is a clear
  safety model.

Acceptance:

- Operators have a documented way to bound storage and request load.

## Deferred Ideas

- WebSocket or SSE agent transport. Only add this if polling latency or network
  behavior becomes a real limitation.
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
