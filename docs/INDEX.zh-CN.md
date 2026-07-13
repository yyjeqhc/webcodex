# 文档索引

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

先看 README、[QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) 和 [DEMO.zh-CN.md](DEMO.zh-CN.md)。这个索引是 setup、client integration、security、release 和 maintenance 文档的精简地图。

## 从这里开始

1. [../README.zh-CN.md](../README.zh-CN.md) / [../README.md](../README.md) - 产品概览和定位。
2. [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) / [QUICK_START.md](QUICK_START.md) - local-first setup，跑通一个 server、一个 agent、一个 project 和一个 client。
3. [DEMO.zh-CN.md](DEMO.zh-CN.md) / [DEMO.md](DEMO.md) - 预期的安全 coding workflow。
4. [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) / [CONCEPTS.md](CONCEPTS.md) - 术语和心智模型。
5. [ARCHITECTURE.md](ARCHITECTURE.md) - 产品架构和 Rust module map。

## Client Integration

6. [MCP.zh-CN.md](MCP.zh-CN.md) / [MCP.md](MCP.md) - remote MCP endpoint、认证、默认 coding loop、read-only LSP 与 validation intelligence，以及常见 MCP 错误。
7. [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) / [GPT_ACTIONS.md](GPT_ACTIONS.md) - Custom GPT 设置、OpenAPI 导入、认证、generic validation-summary access 和 workflow guidance。
8. [assets/README.zh-CN.md](assets/README.zh-CN.md) / [assets/README.md](assets/README.md) - MCP 和 GPT Actions 指南使用的截图。

## 安装与运维

9. [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) / [BUILD_INSTALL.md](BUILD_INSTALL.md) - build/install 命令参考和 artifact 细节。
10. [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) / [DEPLOYMENT.md](DEPLOYMENT.md) - 生产 server bootstrap、HTTPS、systemd、OAuth、QUIC 和 smoke checks。
11. [OPERATIONS.md](OPERATIONS.md) - 日常运维、token model、project registration、session workflow 和 smoke testing。
12. [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) / [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - 常见部署和集成问题。

## 安全与认证

13. [../SECURITY.md](../SECURITY.md) - security model、模型能力、project boundary、shell/job risk、token handling、audit evidence、revocation 和 vulnerability reporting。
14. [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) / [AUTH_MODEL.md](AUTH_MODEL.md) - shared key、bootstrap token、account credential、PAT、OAuth token、agent token 和 hash storage。
15. [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) - OAuth2 storage、client management、authorize flow、token exchange、revocation 和 delegated scope enforcement。
16. [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) - 手动端到端 OAuth2 validation。
17. [OAUTH2_BRIDGE_THREAT_MODEL.md](OAUTH2_BRIDGE_THREAT_MODEL.md) - shared-key OAuth bridge threat model 和 subject model constraints。

## Agent 和 Runtime

18. [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) / [AGENT_PROJECTS.md](AGENT_PROJECTS.md) - agent project registry 格式和 runtime project ids。
19. [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) / [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) - QUIC、WebSocket、polling、fallback 和 transport validation。
20. [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) / [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) - agent auth、identity、typed read-only LSP bridge、protocol 和 redacted policy summaries。
21. [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) / [SHELL_PROFILES.md](SHELL_PROFILES.md) - prepared shell environment snapshots 和 safety boundaries。
22. [LSP_NAVIGATION.md](LSP_NAVIGATION.md) - 只读 Rust LSP 导航工具、startup capability summary、边界、限制和 error codes。

## Release And Roadmap

23. [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md) - 面向外部用户的 0.2.0 release notes。
24. [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) - release readiness 和 acceptance procedure。
25. [ROADMAP.zh-CN.md](ROADMAP.zh-CN.md) / [ROADMAP.md](ROADMAP.md) - 短 roadmap。

## Developer Maintenance

26. [TESTING.md](TESTING.md) - test lanes、test layout 和 ignored-test inventory。
27. [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) / [E2E_VALIDATION.md](E2E_VALIDATION.md) - 本地端到端验证脚本和文档扫描建议。
28. [../AGENTS.md](../AGENTS.md) - Agent 执行契约（安全、编辑、Git、验证、架构必须项）。
29. [agent/architecture-decisions.md](agent/architecture-decisions.md)、[agent/session-model.md](agent/session-model.md)、[agent/oauth-bridge-plan.md](agent/oauth-bridge-plan.md)、[agent/openapi-guidelines.md](agent/openapi-guidelines.md)、[agent/release-process.md](agent/release-process.md) - 从 AGENTS.md 迁出的长文设计说明（Session 双模型、OAuth bridge、OpenAPI、release）。
