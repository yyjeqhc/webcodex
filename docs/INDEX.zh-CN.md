# 文档索引

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

从 [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) 和
[CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) 开始。README 是产品概览；这个索引是 setup、operation、security 和 developer maintenance 文档的精简地图。

语言策略：英文文档是 canonical。主要 onboarding、部署、GPT Actions、MCP、
agent 和 troubleshooting 文档维护中文版本；深层 internals、release procedure
和 developer-maintenance 文档可以只保留英文版本。

## 从这里开始

1. [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) / [QUICK_START.md](QUICK_START.md) - onboarding、决策树、shared-key/open/managed 路径、service vs manual mode，以及第一次 runtime 检查。
2. [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md) / [CONCEPTS.md](CONCEPTS.md) - server、agent、project id、runtime tools、GPT Actions、MCP、credentials、sessions 和运行模式。
3. [../README.zh-CN.md](../README.zh-CN.md) / [../README.md](../README.md) - 产品概览、单机本地 demo、GPT/MCP 入口、凭据摘要和安全提示。

## 安装与运维

4. [BUILD_INSTALL.zh-CN.md](BUILD_INSTALL.zh-CN.md) / [BUILD_INSTALL.md](BUILD_INSTALL.md) - build/install 命令参考，以及 npm wrapper / artifact 细节。
5. [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) / [DEPLOYMENT.md](DEPLOYMENT.md) - 生产 server bootstrap、HTTPS、systemd、account onboarding、OAuth2、QUIC 和 smoke checks。
6. [OPERATIONS.md](OPERATIONS.md) - 日常 server 初始化、client enrollment、pairing、token 模型、项目注册、session workflow 和 smoke testing。
7. [TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) / [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - 常见部署和集成问题。

## GPT Actions 和 MCP

8. [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) / [GPT_ACTIONS.md](GPT_ACTIONS.md) - 创建 GPT Action、导入 OpenAPI、配置 Bearer/API-key auth，并使用 runtime tools。
9. [MCP.zh-CN.md](MCP.zh-CN.md) / [MCP.md](MCP.md) - remote MCP endpoint、认证、runtime surface、客户端配置示例和常见 MCP 错误。
10. [assets/README.zh-CN.md](assets/README.zh-CN.md) / [assets/README.md](assets/README.md) - GPT Actions 和 MCP 设置指南使用的截图。

## 安全与认证

11. [../SECURITY.md](../SECURITY.md) - security policy、漏洞报告、secret handling 和 threat-model 摘要。
12. [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) / [AUTH_MODEL.md](AUTH_MODEL.md) - shared key、bootstrap token、account credential、PAT、agent token、client id、runtime project id 和 hash storage。
13. [OAUTH2_INTERNALS.md](OAUTH2_INTERNALS.md) - OAuth2 storage、client management、authorize flow、token exchange、revocation 和 delegated scope enforcement。
14. [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) - 手动端到端 OAuth2 validation。
15. [OAUTH2_BRIDGE_THREAT_MODEL.md](OAUTH2_BRIDGE_THREAT_MODEL.md) - shared-key OAuth bridge threat model 和 subject model constraints。

## Agent 和 Runtime

16. [AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) / [AGENT_PROJECTS.md](AGENT_PROJECTS.md) - agent `projects.d/*.toml` 格式、顶层 `id/path`、项目管理工具，以及 server-side `projects.toml` 注意事项。
17. [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) / [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) - QUIC、WebSocket、polling、`auto` fallback 和 transport validation。
18. [AGENT_PROTOCOL.zh-CN.md](AGENT_PROTOCOL.zh-CN.md) / [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) - agent auth、identity、protocol 和 redacted policy summaries。
19. [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md) / [SHELL_PROFILES.md](SHELL_PROFILES.md) - prepared shell environment snapshots、profile 配置、解析规则和安全边界。

## Developer Maintenance

20. [ARCHITECTURE.md](ARCHITECTURE.md) - 当前 Rust module boundary map 和维护规则。
21. [TESTING.md](TESTING.md) - test lane definitions、默认测试原则、当前 test layout 和 ignored-test inventory。
22. [E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md) / [E2E_VALIDATION.md](E2E_VALIDATION.md) - 本地端到端验证脚本和文档扫描建议。

## Release Procedure

23. [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) - 最终 release readiness 和部署后 acceptance procedure。
24. [RELEASE_NOTES_v0.2.0.md](RELEASE_NOTES_v0.2.0.md) - v0.2.0 release notes。
