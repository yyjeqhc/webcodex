# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

Start with the README, [QUICK_START.md](QUICK_START.md), and [DEMO.md](DEMO.md). This index is the compact map for setup, client integration, security, release, and maintenance docs.

## Start Here

1. [../README.md](../README.md) / [../README.zh-CN.md](../README.zh-CN.md) - product overview and positioning.
2. [QUICK_START.md](QUICK_START.md) / [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) - local-first setup for one server, one agent, one project, and one client.
3. [DEMO.md](DEMO.md) / [DEMO.zh-CN.md](DEMO.zh-CN.md) - expected safe coding workflow.
4. [CONCEPTS.md](CONCEPTS.md) / [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) - vocabulary and mental model.
5. [ARCHITECTURE.md](ARCHITECTURE.md) - product architecture and Rust module map.

## Client Integration

6. [MCP.md](MCP.md) / [MCP.zh-CN.md](MCP.zh-CN.md) - remote MCP endpoint, authentication, default coding loop, read-only LSP and validation intelligence, and common MCP errors.
7. [GPT_ACTIONS.md](GPT_ACTIONS.md) / [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) - Custom GPT setup, OpenAPI import, authentication, generic validation-summary access, and workflow guidance.
8. [assets/README.md](assets/README.md) / [assets/README.zh-CN.md](assets/README.zh-CN.md) - screenshots used by MCP and GPT Actions guides.

## Setup And Operation

9. [BUILD_INSTALL.md](BUILD_INSTALL.md) / [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) - build/install command reference and artifact details.
10. [DEPLOYMENT.md](DEPLOYMENT.md) / [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) - production server bootstrap, HTTPS, systemd, OAuth, QUIC, and smoke checks.
11. [OPERATIONS.md](OPERATIONS.md) - day-to-day operations, token model, project registration, session workflow, and smoke testing.
12. [TROUBLESHOOTING.md](TROUBLESHOOTING.md) / [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) - common deployment and integration failures.

## Security And Auth

13. [../SECURITY.md](../SECURITY.md) - security model, model capabilities, project boundary, shell/job risk, token handling, audit evidence, revocation, and vulnerability reporting.
14. [AUTH_MODEL.md](AUTH_MODEL.md) / [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) - shared key, bootstrap token, account credential, PAT, OAuth token, agent token, and hash storage.
15. [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) - OAuth2 storage, client management, authorize flow, token exchange, revocation, and delegated scope enforcement.
16. [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) - manual end-to-end OAuth2 validation.
17. [OAUTH2_BRIDGE_THREAT_MODEL.md](OAUTH2_BRIDGE_THREAT_MODEL.md) - shared-key OAuth bridge threat model and subject model constraints.

## Agent And Runtime

18. [AGENT_PROJECTS.md](AGENT_PROJECTS.md) / [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) - agent project registry format and runtime project ids.
19. [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) / [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) - QUIC, WebSocket, polling, fallback, and transport validation.
20. [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) / [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) - agent auth, identity, typed read-only LSP bridge, protocol, and redacted policy summaries.
21. [SHELL_PROFILES.md](SHELL_PROFILES.md) / [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) - prepared shell environment snapshots and safety boundaries.
22. [LSP_NAVIGATION.md](LSP_NAVIGATION.md) - read-only Rust LSP navigation tools, startup capability summary, boundaries, limits, and error codes.

## Release And Roadmap

23. [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md) - external-facing 0.2.0 release notes.
24. [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) - release readiness and acceptance procedure.
25. [ROADMAP.md](ROADMAP.md) / [ROADMAP.zh-CN.md](ROADMAP.zh-CN.md) - short roadmap.

## Developer Maintenance

26. [TESTING.md](TESTING.md) - test lanes, test layout, and ignored-test inventory.
27. [E2E_VALIDATION.md](E2E_VALIDATION.md) / [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) - local end-to-end validation scripts and documentation scan guidance.
28. [../AGENTS.md](../AGENTS.md) - agent execution contract (safety, editing, git, validation, architecture musts).
29. [agent/architecture-decisions.md](agent/architecture-decisions.md), [agent/oauth-bridge-plan.md](agent/oauth-bridge-plan.md), [agent/openapi-guidelines.md](agent/openapi-guidelines.md), [agent/release-process.md](agent/release-process.md) - long-form design moved out of AGENTS.md.
