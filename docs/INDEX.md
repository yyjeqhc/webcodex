# Documentation Index

Recommended reading order for the current WebCodex Runtime. The authoritative
API surface is defined by `src/main.rs`, `src/openapi.rs`, and `README.md`.

1. [README.md](../README.md) — quick project overview, current endpoints,
   agent setup, and verification entry points.
2. [BUILD_INSTALL.md](BUILD_INSTALL.md) — short Rust build, install, server,
   agent, GPT Actions, and MCP setup guide.
3. [GPT_ACTIONS.md](GPT_ACTIONS.md) — ChatGPT GPT Actions import guide: import
   URL, Bearer auth, the operation ids, examples, and the recommended
   development flow.
4. [DEPLOYMENT.md](DEPLOYMENT.md) — production deployment: server env vars,
   agent config, reverse proxy / HTTPS, GPT Actions import, MCP endpoint,
   smoke tests, and WebSocket online/stale troubleshooting.
5. [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) — shared polling/WebSocket agent wire
   protocol: transport-neutral envelopes, register/request/result/job_update,
   ping-pong, backpressure, reconnect, capabilities, and owner boundary.
6. [AGENT_PROJECTS.md](AGENT_PROJECTS.md) — the live agent-local `projects.d`
   registry and runtime project id model.
7. [RUNTIME_STATUS.md](RUNTIME_STATUS.md) — `runtime_status` /
   `getRuntimeStatus` observability: response shape, field notes, and what is
   intentionally not exposed.
8. [AUDIT_API.md](AUDIT_API.md) — read-only admin/debug audit query API and
   secret-sanitization guarantees. Not a GPT Action.
9. [E2E_VALIDATION.md](E2E_VALIDATION.md) — local end-to-end validation harness
   for WebSocket/polling agents, GPT Actions schema smoke, and MCP smoke.
10. [ZERO_CONFIG_AGENT_RUNTIME.md](ZERO_CONFIG_AGENT_RUNTIME.md) — zero-server-
   project-config direction and agent-registered project model.
11. [MCP_APP_CONSOLE_PLAN.md](MCP_APP_CONSOLE_PLAN.md) — staged plan for the
    MCP App visual console, approval flow, and supporting typed runtime tools.
12. [ROADMAP.md](ROADMAP.md) — current forward-looking roadmap.
13. [AGENT_HANDOFF.md](AGENT_HANDOFF.md) — compact handoff notes for future
    coding agents after context compaction or a new window.

## Scope and backlog

- [V2_SCOPE.md](../V2_SCOPE.md) — vision, retained capabilities, and removed
  product surfaces.
- [TODO.md](../TODO.md) — current backlog and completed milestones.

Deprecated workflow, SSH, and legacy action-session dashboard documents were
removed from `docs/` to keep the documentation set focused on the active runtime
surface. Use git history if historical removed-feature notes are needed.
