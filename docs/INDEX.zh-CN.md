# 文档索引

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

从 [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) 和
[CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) 开始。README 仍是产品概览；这个索引是 setup、auth、operations 和 validation 文档地图。

## 从这里开始

1. [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) / [QUICK_START.md](QUICK_START.md) — canonical onboarding、决策树、本地/shared-key/managed 路径、service vs manual mode、发布路径过渡，以及 model-facing workflow 状态说明。
2. [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) / [CONCEPTS.md](CONCEPTS.md) — server、agent、project、runtime tools、GPT Actions、MCP、credential vocabulary、sessions、handoff 和运行模式。
3. [../README.zh-CN.md](../README.zh-CN.md) / [../README.md](../README.md) — 产品概览、单机本地 demo、GPT/MCP 入口、凭据摘要和文档地图。

## Setup 和 surfaces

4. [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) / [GPT_ACTIONS.md](GPT_ACTIONS.md) — 创建 GPT Action、导入 OpenAPI、用 shared key 或 `wc_pat_xxx` 配置静态 Bearer/API-key auth，并使用 runtime tools。
5. [MCP.zh-CN.md](MCP.zh-CN.md) / [MCP.md](MCP.md) — MCP endpoint、用 shared key 或 `wc_pat_xxx` 配置静态 Bearer/API-key auth、客户端配置示例、sessions 和排障。
6. [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) / [DEPLOYMENT.md](DEPLOYMENT.md) — 生产部署、HTTPS、systemd service、account credential onboarding、OAuth2、QUIC 和 smoke checks。
7. [OPERATIONS.md](OPERATIONS.md) — 日常 server 初始化、client enrollment、pairing、项目注册、token 模型、session 工作流和 smoke testing。

## Security 和 auth

8. [../SECURITY.md](../SECURITY.md) — security policy 和报告入口。
9. [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) / [AUTH_MODEL.md](AUTH_MODEL.md) — shared key、`WEBCODEX_TOKEN`、`wc_acct_xxx`、`wc_pat_xxx`、`wc_agent_xxx`、`client_id`、runtime project id 和 hash storage。
10. [AUTH_ARCHITECTURE.md](AUTH_ARCHITECTURE.md) — auth middleware、verifier chain、route policy、OAuth scope enforcement 和 transport boundaries。
11. [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) — 手动端到端 OAuth2 validation path。
12. [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) — OAuth2 storage、verifier、token 和 implementation internals。
13. [OAUTH2_AUTHORIZE_DESIGN.md](OAUTH2_AUTHORIZE_DESIGN.md) — OAuth authorize endpoint、browser flow、shared-key OAuth bridge 和 scope constraints。
14. [OAUTH2_BRIDGE_THREAT_MODEL.md](OAUTH2_BRIDGE_THREAT_MODEL.md) — shared-key OAuth bridge threat model、subject model、endpoint contract 和 acceptance-test notes。

## Runtime 和 agent reference

15. [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) / [AGENT_PROJECTS.md](AGENT_PROJECTS.md) — agent `projects.d/*.toml` 注册格式、顶层 `id/path` 和项目管理工具。
16. [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) / [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) — QUIC、WebSocket、polling、`auto` fallback 和 transport validation。
17. [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) / [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) — agent auth、identity、protocol 和 redacted policy summaries。
18. [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) / [SHELL_PROFILES.md](SHELL_PROFILES.md) — prepared shell env snapshots、profile 配置、解析规则和安全边界。
19. [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) / [TROUBLESHOOTING.md](TROUBLESHOOTING.md) — 部署排障和运维检查清单。

## Testing 和 CI

20. [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) / [E2E_VALIDATION.md](E2E_VALIDATION.md) — 本地端到端验证脚本和文档扫描建议。
21. [TESTING.md](TESTING.md) — test lane definitions、默认测试原则和当前 slow/flaky test inventory notes。
22. [CI_LANES.md](CI_LANES.md) — static、contract、integration、security、manual 和 smoke validation 的 CI lane 结构提案。

## Install、assets 和 release reference

23. [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) / [BUILD_INSTALL.md](BUILD_INSTALL.md) — build/install 命令参考，以及 npm wrapper / artifact 细节。
24. [assets/README.zh-CN.md](assets/README.zh-CN.md) / [assets/README.md](assets/README.md) — GPT Actions 和 MCP 设置截图说明。
25. [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md) — v0.2.0 release notes、亮点、已知问题和验证清单。
26. [RELEASE_CHECKLIST_v0.2.0.md](RELEASE_CHECKLIST_v0.2.0.md) — v0.2.0 release readiness、artifact、发布和发布后 smoke checklist。

## Strategy 和 future work

27. [AGENT_RUNTIME_ARCHITECTURE.zh-CN.md](AGENT_RUNTIME_ARCHITECTURE.zh-CN.md) / [AGENT_RUNTIME_ARCHITECTURE.md](AGENT_RUNTIME_ARCHITECTURE.md) — WebCodex 长期 runtime 架构和 provider 方向。
28. [DESKTOP_SESSIONS.zh-CN.md](DESKTOP_SESSIONS.zh-CN.md) / [DESKTOP_SESSIONS.md](DESKTOP_SESSIONS.md) — 未来 Desktop Session strategy：面向工程工作流的可控、可审计、可回放 computer-use sessions。

## 已从公开文档集中移除

- `smoke-test-sg4.md` / `smoke-test-sg4.zh-CN.md`：已移除，因为包含环境特定私有命名，不适合作为公开 onboarding 文档。
- `AUDIT_API.md` / `AUDIT_API.zh-CN.md`：audit API 细节当前不需要放在用户文档中。
- `QUIC_E2E.md` / `QUIC_E2E.zh-CN.md`：QUIC 已正式支持，validation guidance 已合并进 [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md)。
