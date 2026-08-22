# 文档索引

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

## 入门

- [README](../README.zh-CN.md) —— WebCodex 是什么，以及 `webcodex share` 首次体验
- [快速开始](QUICK_START.zh-CN.md) —— 从本地仓库接入 ChatGPT MCP 的最短路径
- [MCP](MCP.zh-CN.md) —— ChatGPT Developer Mode 与其他 MCP 客户端配置
- [AI 接入指南](AI_ONBOARDING.zh-CN.md) —— 让 AI agent 帮你配置 WebCodex

## 日常使用

- [CLI](CLI.zh-CN.md) —— 命令、兼容性术语与凭据
- [Coding 工作流](CODING_WORKFLOW.zh-CN.md) —— canonical task bootstrap、behavioral guidance、validation 与 closeout
- [Runner](RUNNER.zh-CN.md) —— 在持有仓库的机器上运维 Runner

## 高级部署与集成

- [部署指南](DEPLOYMENT.zh-CN.md) —— 自托管、Server bootstrap 与长期 Runner 接入
- [认证模型](AUTH_MODEL.zh-CN.md) —— 详细凭据与令牌边界
- [GPT Actions](GPT_ACTIONS.zh-CN.md) —— 可选的 OpenAPI Custom GPT 集成

## 理解 WebCodex

- [架构](ARCHITECTURE.md) —— 各组件如何协同
- [安全](../SECURITY.md) —— 安全模型与策略

## 帮助

- [故障排查](TROUBLESHOOTING.zh-CN.md)

## 贡献

- [AGENTS.md](../AGENTS.md) —— 面向 coding/AI agent 的仓库指引
- [仓库维护](MAINTENANCE.md) —— canonical maintenance queue、依赖更新节奏、PR/CI 约定与双语文档规则
- [测试策略](TESTING.md) —— 测试策略
- [发布清单](RELEASE_CHECKLIST.md) —— 发布就绪
- [架构决策](agent/architecture-decisions.md)
- [Job 可靠性与 Runner 并发](agent/job-reliability-and-concurrency.md) —— Control 重启恢复、observation 语义、共享 Job 容量与工具描述要求
- [权限模型](agent/permission-model.md)
- [会话模型](agent/session-model.md)
- [手动多窗口协作](agent/manual-window-collaboration.md) —— 使用现有 Workflow Session message primitive 做 coordinator/worker handoff
- [OpenAPI 指南](agent/openapi-guidelines.md)
- [发布流程](agent/release-process.md)
