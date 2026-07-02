# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

Start with [QUICK_START.md](QUICK_START.md) and
[CONCEPTS.md](CONCEPTS.md). The README remains the product overview; this index
is the map for setup, auth, operations, and validation docs.

## Start here

1. [QUICK_START.md](QUICK_START.md) / [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) — canonical onboarding, decision tree, local/shared-key/managed paths, service vs manual mode, release-path transition, and model-facing workflow notes.
2. [CONCEPTS.md](CONCEPTS.md) / [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) — server, agent, project, runtime tools, GPT Actions, MCP, credential vocabulary, sessions, handoff, and operating modes.
3. [../README.md](../README.md) / [../README.zh-CN.md](../README.zh-CN.md) — product overview, local one-machine demo, GPT/MCP entry points, credential summary, and documentation map.

## Setup and surfaces

4. [GPT_ACTIONS.md](GPT_ACTIONS.md) / [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) — create a GPT Action, import OpenAPI, configure static Bearer/API-key auth with a shared key or `wc_pat_xxx`, and use runtime tools.
5. [MCP.md](MCP.md) / [MCP.zh-CN.md](MCP.zh-CN.md) — MCP endpoint, static Bearer/API-key auth with a shared key or `wc_pat_xxx`, example client config, sessions, and troubleshooting.
6. [DEPLOYMENT.md](DEPLOYMENT.md) / [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) — production server bootstrap, HTTPS, systemd service installation, account credential onboarding, OAuth2, QUIC, and smoke checks.
7. [OPERATIONS.md](OPERATIONS.md) — day-to-day server initialization, client enrollment, pairing, project registration, token model, session workflow, and smoke testing.

## Security and auth

8. [../SECURITY.md](../SECURITY.md) — security policy and reporting entry point.
9. [AUTH_MODEL.md](AUTH_MODEL.md) / [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) — shared key, `WEBCODEX_TOKEN`, `wc_acct_xxx`, `wc_pat_xxx`, `wc_agent_xxx`, `client_id`, runtime project ids, and hash storage.
10. [AUTH_ARCHITECTURE.md](AUTH_ARCHITECTURE.md) — auth middleware, verifier chain, route policy, OAuth scope enforcement, and transport boundaries.
11. [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) — manual end-to-end OAuth2 validation path.
12. [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) — OAuth2 storage, verifier, token, and implementation internals.
13. [OAUTH2_AUTHORIZE_DESIGN.md](OAUTH2_AUTHORIZE_DESIGN.md) — OAuth authorize endpoint, browser flow, shared-key OAuth bridge, and scope constraints.
14. [OAUTH2_BRIDGE_THREAT_MODEL.md](OAUTH2_BRIDGE_THREAT_MODEL.md) — shared-key OAuth bridge threat model, subject model, endpoint contract, and acceptance-test notes.

## Runtime and agent references

15. [AGENT_PROJECTS.md](AGENT_PROJECTS.md) / [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) — agent `projects.d/*.toml` registry format, top-level `id/path`, and project management tools.
16. [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) / [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) — QUIC, WebSocket, polling, `auto` fallback, and transport validation.
17. [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) / [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) — agent auth, identity, protocol, and redacted policy summaries.
18. [SHELL_PROFILES.md](SHELL_PROFILES.md) / [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) — prepared shell env snapshots, profile config, resolution rules, and safety boundaries.
19. [TROUBLESHOOTING.md](TROUBLESHOOTING.md) / [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) — deployment troubleshooting and operational checklist.

## Testing and CI

20. [E2E_VALIDATION.md](E2E_VALIDATION.md) / [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) — local end-to-end validation scripts and documentation scan guidance.
21. [TESTING.md](TESTING.md) — test lane definitions, default test principles, and current slow/flaky test inventory notes.
22. [CI_LANES.md](CI_LANES.md) — proposed CI lane structure for static, contract, integration, security, manual, and smoke validation.

## Install, assets, and release reference

23. [BUILD_INSTALL.md](BUILD_INSTALL.md) / [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) — build/install command reference and npm wrapper / artifact details.
24. [assets/README.md](assets/README.md) / [assets/README.zh-CN.md](assets/README.zh-CN.md) — documentation screenshots for GPT Actions and MCP setup.
25. [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md) — v0.2.0 release notes, highlights, known issues, and validation checklist.
26. [RELEASE_CHECKLIST_v0.2.0.md](RELEASE_CHECKLIST_v0.2.0.md) — v0.2.0 release readiness, artifact, publishing, and post-release smoke checklist.

## Strategy and future work

27. [AGENT_RUNTIME_ARCHITECTURE.md](AGENT_RUNTIME_ARCHITECTURE.md) / [AGENT_RUNTIME_ARCHITECTURE.zh-CN.md](AGENT_RUNTIME_ARCHITECTURE.zh-CN.md) — long-term WebCodex runtime architecture and provider direction.
28. [DESKTOP_SESSIONS.md](DESKTOP_SESSIONS.md) / [DESKTOP_SESSIONS.zh-CN.md](DESKTOP_SESSIONS.zh-CN.md) — future Desktop Session strategy for controlled, auditable, replayable computer-use sessions.

## Removed from the public docs set

- `smoke-test-sg4.md` / `smoke-test-sg4.zh-CN.md`: removed because they contained environment-specific private naming and are not suitable as public onboarding docs.
- `AUDIT_API.md` / `AUDIT_API.zh-CN.md`: removed because audit API details are not needed in the current user-facing docs.
- `QUIC_E2E.md` / `QUIC_E2E.zh-CN.md`: removed because QUIC is now formally supported; validation guidance is folded into [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md).
