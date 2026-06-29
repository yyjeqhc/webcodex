# Documentation Index

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

Start with the product README ([English](../README.md), [简体中文](../README.zh-CN.md)), then choose the guide for your path.

## Recommended primary docs

1. [../README.md](../README.md) / [../README.zh-CN.md](../README.zh-CN.md) — product overview, GPT/MCP entry points, quick start, credential summary, and doc map.
2. [QUICK_START.md](QUICK_START.md) / [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) — first server deployment, first client enrollment, service mode, no-service foreground/background modes, and PATH/shell-profile handling.
3. [DEPLOYMENT.md](DEPLOYMENT.md) / [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) — production server bootstrap, agent service installation, account credential onboarding, enrollment, and operations guide.
4. [GPT_ACTIONS.md](GPT_ACTIONS.md) / [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) — create a GPT Action, import OpenAPI, configure `wc_pat_xxx` auth, and use runtime tools.
5. [MCP.md](MCP.md) / [MCP.zh-CN.md](MCP.zh-CN.md) — MCP endpoint, `wc_pat_xxx` auth, example client config, and troubleshooting.
6. [AUTH_MODEL.md](AUTH_MODEL.md) / [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) — `WEBCODEX_TOKEN`, `wc_acct_xxx`, `wc_pat_xxx`, `wc_agent_xxx`, `client_id`, runtime project ids, and hash storage.
7. [AGENT_PROJECTS.md](AGENT_PROJECTS.md) / [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) — agent `projects.d/*.toml` registry format, top-level `id/path`, and project management tools.
8. [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) / [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) — QUIC, WebSocket, polling, `auto` fallback, and transport validation.
9. [SHELL_PROFILES.md](SHELL_PROFILES.md) / [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) — prepared shell env snapshots, profile config, resolution rules, and safety boundaries.
10. [TROUBLESHOOTING.md](TROUBLESHOOTING.md) / [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) — deployment troubleshooting and operational checklist.

## Reference docs worth keeping

11. [BUILD_INSTALL.md](BUILD_INSTALL.md) / [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) — short build/install command reference; overlaps with deployment but remains useful as a quick reference.
12. [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) / [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) — agent auth, identity, protocol, and redacted policy summaries.
13. [E2E_VALIDATION.md](E2E_VALIDATION.md) / [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) — local end-to-end validation scripts and documentation scan guidance.
14. [assets/README.md](assets/README.md) / [assets/README.zh-CN.md](assets/README.zh-CN.md) — documentation screenshots for GPT Actions and MCP setup.

## Environment-specific validation notes

15. [smoke-test-sg4.md](smoke-test-sg4.md) / [smoke-test-sg4.zh-CN.md](smoke-test-sg4.zh-CN.md) — sg4 manual smoke test record; environment-specific and not recommended as a general onboarding doc.

## Removed from the public docs set

- `AUDIT_API.md` / `AUDIT_API.zh-CN.md`: removed because audit API details are not needed in the current user-facing docs.
- `QUIC_E2E.md` / `QUIC_E2E.zh-CN.md`: removed because QUIC is now formally supported; validation guidance is folded into [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md).
