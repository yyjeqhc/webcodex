# Recent Changes Audit

Date: 2026-06-27

Scope: final MVP review of project management, agent instance leases, Phase 2
user API tokens, Phase 3 owner-bound agent tokens, CLI/admin tooling,
OpenAPI/MCP exposure, and deployment/security documentation.

## Summary

- Project management remains agent-side: `register_project` and
  `create_project` route through the owning agent, validate request shape on
  the server, validate paths against agent policy on the agent, write project
  TOML atomically, clean up newly-created paths on failure, and refresh the
  server-side project cache after success.
- Agent instance leases are enforced by `agent_instance_id`: duplicate
  online instances are rejected, stale/offline replacement is allowed, stale
  poll/result/job_update/notifier/keepalive paths are rejected, and WebSocket
  disconnect reconciliation is instance-aware.
- User API tokens are stored hash-only, return plaintext only at creation,
  honor revoked/expired/disabled checks, update `last_used_at`, and enforce
  admin-or-self owner boundaries.
- Agent tokens use the `wc_agent_` format, are stored as `kind="agent"`, are
  bound to `allowed_client_id`, can only carry `agent:*` scopes, and are
  centrally gated to exact agent transport paths.
- GPT Actions OpenAPI remains at 27 operations. User/token/agent-token
  management endpoints are absent from `/openapi.json`; MCP exposes runtime
  tools only and does not expose token creation.
- CLI/admin tooling was added for user, personal token, and agent-token
  management, plus `webcodex-agent init`.

## Issue Fixed

- Deployment/auth documentation still described `WEBCODEX_TOKEN` as the normal
  GPT Actions/MCP/agent credential. Updated the docs and public metadata to
  state the intended credential split:
  - server uses `WEBCODEX_TOKEN` as bootstrap/admin credential;
  - GPT Actions and MCP should use a Phase 2 personal API token;
  - `webcodex-agent` should use a Phase 3 agent token;
  - server and agent must be upgraded together because current agent transport
    requires `agent_instance_id`.

## Residual Notes

- `webcodex-agent init` intentionally requires at least one `--allowed-root`
  unless `--allow-cwd-anywhere true` is passed. This is stricter than the old
  example-only flow, but it matches the safer policy posture.
- Bootstrap auth remains supported for initial setup and admin operations.
