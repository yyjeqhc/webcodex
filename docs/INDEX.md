# Documentation Index

Recommended reading order for the Private Drop Runtime. The authoritative API
surface is defined by `src/main.rs`, `src/openapi.rs`, and `README.md`.

1. [README.md](../README.md) — runtime overview, build/run, current endpoints,
   project config, MCP, and agent setup.
2. [GPT_ACTIONS.md](GPT_ACTIONS.md) — ChatGPT GPT Actions import guide: import
   URL, Bearer auth, the 9 operation ids, and the recommended call flow.
3. [RUNTIME_STATUS.md](RUNTIME_STATUS.md) — `runtime_status` /
   `getRuntimeStatus` observability: response shape, field notes, and what is
   intentionally not exposed.
4. [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) — the polling agent wire protocol
   (`polling-v1`): register / poll / result / job_update, capabilities, owner
   boundary, and known limitations.
5. [AUDIT_API.md](AUDIT_API.md) — the read-only admin/debug audit query API
   (`/api/audit/sessions`, `/api/audit/session`, `/api/audit/stats`): endpoints,
   limit bounds, and secret-sanitization guarantees. Not a GPT Action.
6. [GLM52_DEVELOPMENT_PLAN.md](GLM52_DEVELOPMENT_PLAN.md) — the historical
   phase-by-phase development plan (Phases 1–7 complete; Phase 8 in progress).

## Scope and architecture

- [V2_SCOPE.md](../V2_SCOPE.md) — vision, retained capabilities, removed
  features, and the required GPT Actions operation ids.
- [TODO.md](../TODO.md) — current backlog and deprecated items.

## Agent project registry

- [AGENT_PROJECTS.md](AGENT_PROJECTS.md) — the agent-local `projects.d`
  registry (still live) and the removed workflow / doctor / hook execution
  routes.

## Deprecated docs (kept for historical reference only)

These describe removed endpoints and must not be used as integration guides:

- [GPT_WORKFLOW.md](GPT_WORKFLOW.md) — removed v4 GPT workflow
  (`/codex-openapi-*.json`, desktop tasks, goal workflow).
- [WORKFLOWS.md](WORKFLOWS.md) — removed `project_workflow` / `project_doctor`
  / `project_hook` routes and SSH executor.
- [CODEX_USAGE.md](CODEX_USAGE.md) — removed `pdctl.py` workflow/doctor/hook
  commands.
- [action_sessions.md](action_sessions.md) — removed action session dashboard;
  documents the internal audit storage layer and links to the new read-only
  Audit API.
- [action_session_coverage.md](action_session_coverage.md) — audits removed
  routes.
- [job_recovery_notes.md](job_recovery_notes.md) — removed `runJobOp` recovery.
- [trusted_raw_commands.md](trusted_raw_commands.md) — removed
  `command_request` / goal / trusted raw command workflow.
- [TAILSCALE_SSH_DIAGNOSTICS.md](TAILSCALE_SSH_DIAGNOSTICS.md) — removed SSH
  executor context.
