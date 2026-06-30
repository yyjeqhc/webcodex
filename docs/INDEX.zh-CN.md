# 文档索引

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

从产品 README 开始（[English](../README.md)，[简体中文](../README.zh-CN.md)），然后根据目标选择对应指南。

## Release notes

1. [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md) — v0.2.0 release notes、亮点、已知问题和验证清单。

## 推荐保留的主文档

2. [../README.md](../README.md) / [../README.zh-CN.md](../README.zh-CN.md) — 产品概览、GPT/MCP 入口、快速开始、凭据摘要和文档地图。
3. [OPERATIONS.md](OPERATIONS.md) — server 初始化、client enrollment、pairing、项目注册、token 模型、session 工作流和 smoke 测试。
4. [QUICK_START.md](QUICK_START.md) / [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) — 第一次 server 部署、第一次 client enrollment、service 模式、非 service 前台/后台模式，以及 PATH/shell-profile 处理。
5. [DEPLOYMENT.md](DEPLOYMENT.md) / [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) — 生产部署、server bootstrap、agent service、账户凭据、enrollment 和运维流程。
6. [GPT_ACTIONS.md](GPT_ACTIONS.md) / [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) — 创建 GPT Action、导入 OpenAPI、配置 `wc_pat_xxx` 认证并使用 runtime tools。
7. [MCP.md](MCP.md) / [MCP.zh-CN.md](MCP.zh-CN.md) — MCP endpoint、`wc_pat_xxx` 认证、客户端配置示例和排障。
8. [AUTH_MODEL.md](AUTH_MODEL.md) / [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) — `WEBCODEX_TOKEN`、`wc_acct_xxx`、`wc_pat_xxx`、`wc_agent_xxx`、`client_id`、runtime project id 和 hash storage。
9. [AGENT_PROJECTS.md](AGENT_PROJECTS.md) / [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) — agent `projects.d/*.toml` 注册格式、顶层 `id/path` 和项目管理工具。
10. [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) / [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) — QUIC、WebSocket、polling、`auto` fallback 和 transport validation。
11. [SHELL_PROFILES.md](SHELL_PROFILES.md) / [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) — prepared shell env snapshots、profile 配置、解析规则和安全边界。
12. [TROUBLESHOOTING.md](TROUBLESHOOTING.md) / [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) — 部署排障和运维检查清单。

## 建议保留的参考文档

13. [BUILD_INSTALL.md](BUILD_INSTALL.md) / [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) — 构建/安装快速参考；内容与部署文档有重叠，但适合作为命令速查。
14. [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) / [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) — agent auth、identity、protocol 和 policy 摘要。
15. [E2E_VALIDATION.md](E2E_VALIDATION.md) / [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) — 本地端到端验证脚本和文档扫描建议。
16. [assets/README.md](assets/README.md) / [assets/README.zh-CN.md](assets/README.zh-CN.md) — GPT Actions 和 MCP 设置截图说明。

## 已从公开文档集中移除

- `smoke-test-sg4.md` / `smoke-test-sg4.zh-CN.md`：已移除，因为包含环境特定私有命名，不适合作为公开入门文档。
- `AUDIT_API.md` / `AUDIT_API.zh-CN.md`：audit API 细节当前不需要放在用户文档中。
- `QUIC_E2E.md` / `QUIC_E2E.zh-CN.md`：QUIC 已正式支持，验证说明已合并进 [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md)。
