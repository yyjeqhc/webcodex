# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

Start with [QUICK_START.md](QUICK_START.md) and [CONCEPTS.md](CONCEPTS.md).
The README is the product overview; this index is the compact map for setup,
operation, security, and developer maintenance docs.

## Start Here

1. [QUICK_START.md](QUICK_START.md) / [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) - onboarding, decision tree, shared-key/open/managed paths, service vs manual mode, and first runtime check.
2. [CONCEPTS.md](CONCEPTS.md) / [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) - server, agent, project ids, runtime tools, GPT Actions, MCP, credentials, sessions, and operating modes.
3. [../README.md](../README.md) / [../README.zh-CN.md](../README.zh-CN.md) - product overview, local one-machine demo, GPT/MCP entry points, credential summary, and safety notes.

## Setup And Operation

4. [BUILD_INSTALL.md](BUILD_INSTALL.md) / [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) - build/install command reference and npm wrapper / artifact details.
5. [DEPLOYMENT.md](DEPLOYMENT.md) / [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) - production server bootstrap, HTTPS, systemd, account onboarding, OAuth2, QUIC, and smoke checks.
6. [OPERATIONS.md](OPERATIONS.md) - day-to-day server initialization, client enrollment, pairing, token model, project registration, session workflow, and smoke testing.
7. [TROUBLESHOOTING.md](TROUBLESHOOTING.md) / [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) - common deployment and integration failures.

## GPT Actions And MCP

8. [GPT_ACTIONS.md](GPT_ACTIONS.md) / [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) - create a GPT Action, import OpenAPI, configure Bearer/API-key auth, and use runtime tools.
9. [MCP.md](MCP.md) / [MCP.zh-CN.md](MCP.zh-CN.md) - remote MCP endpoint, authentication, runtime surface, example client config, and common MCP errors.
10. [assets/README.md](assets/README.md) / [assets/README.zh-CN.md](assets/README.zh-CN.md) - screenshots used by the GPT Actions and MCP setup guides.

## Security And Auth

11. [../SECURITY.md](../SECURITY.md) - security policy, vulnerability reporting, secret handling, and threat-model summary.
12. [AUTH_MODEL.md](AUTH_MODEL.md) / [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) - shared key, bootstrap token, account credential, PAT, agent token, client id, runtime project ids, and hash storage.
13. [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) - OAuth2 storage, client management, authorize flow, token exchange, revocation, and delegated scope enforcement.
14. [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) - manual end-to-end OAuth2 validation.
15. [OAUTH2_BRIDGE_THREAT_MODEL.md](OAUTH2_BRIDGE_THREAT_MODEL.md) - shared-key OAuth bridge threat model and subject model constraints.

## Agent And Runtime

16. [AGENT_PROJECTS.md](AGENT_PROJECTS.md) / [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) - agent `projects.d/*.toml` format, top-level `id/path`, project management tools, and server-side `projects.toml` caveats.
17. [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) / [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) - QUIC, WebSocket, polling, `auto` fallback, and transport validation.
18. [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) / [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) - agent auth, identity, protocol, and redacted policy summaries.
19. [SHELL_PROFILES.md](SHELL_PROFILES.md) / [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) - prepared shell environment snapshots, profile config, resolution rules, and safety boundaries.

## Developer Maintenance

20. [ARCHITECTURE.md](ARCHITECTURE.md) - current Rust module boundary map and maintenance rules.
21. [TESTING.md](TESTING.md) - test lane definitions, default test principles, current test layout, and ignored-test inventory.
22. [E2E_VALIDATION.md](E2E_VALIDATION.md) / [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) - local end-to-end validation scripts and documentation scan guidance.

## Release Procedure

23. [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) - final release readiness and post-deployment acceptance procedure.
24. [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md) - v0.2.0 release notes.
