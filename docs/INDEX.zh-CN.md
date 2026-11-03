# 文档索引

[English](INDEX.md) | [简体中文](INDEX.zh-CN.md)

按你现在想完成的事情选择文档即可。

## 我想正常使用 WebCodex

- [README](../README.zh-CN.md) —— WebCodex 能做什么，以及如何选择配置方式
- [统一安装指南](unified-installation.zh-CN.md) —— 长期使用的主节点与多机配置；选择安装包前先查看[部署验收清单](unified-deployment-validation.md)
- [Runtime 安装与 CLI 加入](runtime-installation.zh-CN.md) —— 无 Desktop 的 Linux Runner 配置与安装包更新/恢复
- [Desktop 安装与连接](desktop-install.zh-CN.md) —— 对应 Release 的 Windows/macOS 安装和 OpenAI Secure Tunnel 细节
- [Desktop 日常使用](desktop-guide.zh-CN.md) —— 项目、设备邀请、连接、活动与服务控制
- [Desktop Runtime 兼容说明](DESKTOP_RUNTIME_COMPATIBILITY.zh-CN.md) —— Runtime 选择、诊断、更新与回滚
- [Runtime Console](runtime-console.md) —— 浏览器中的活动、项目、Session 与 Agent 通信
- [完整使用指南](PERSONAL_SETUP.zh-CN.md) —— CLI、已有 Server、Linux 与高级普通 Server + Runner 配置
- [AI 辅助接入](AI_ONBOARDING.zh-CN.md) —— 让 AI 帮你按普通用户语言完成配置
- [MCP](MCP.zh-CN.md) —— ChatGPT、Claude 与其他 MCP 客户端
- [任务接续](SESSION_CONTINUITY.zh-CN.md) —— 保存上下文、跨账号或窗口继续，以及委派 API 模型评审

## 我只想先试几分钟

- [快速试用](QUICK_START.zh-CN.md) —— 一条 `webcodex share` 临时体验一个仓库

## 我需要生产部署或深入排障

- [部署指南](DEPLOYMENT.zh-CN.md) —— systemd/Docker、多用户、自托管和长期运维
- [Windows + OpenAI Secure MCP Tunnel 深入实操](WINDOWS_OPENAI_TUNNEL.zh-CN.md) —— 独立 Windows Server/Runner、Tunnel 与真实故障排查
- [Runner](RUNNER.zh-CN.md) —— Runner 运维参考
- [CLI](CLI.zh-CN.md) —— 完整命令、配置和凭据参考

## 我需要认证或网络配置

- [MCP](MCP.zh-CN.md) —— Bearer、query-token 兼容方式、OAuth、私有隧道和 MCP 参考
- [认证模型](AUTH_MODEL.zh-CN.md) —— 详细凭据和权限边界
- [部署指南](DEPLOYMENT.zh-CN.md) —— 稳定 HTTPS、自托管和生产网络配置

## 我遇到了问题

- [故障排查](TROUBLESHOOTING.zh-CN.md) —— ChatGPT/MCP Host、安装、连接、运行和 Runner 问题
- [安全说明](../SECURITY.md) —— 安全模型和使用建议

## 我想理解或扩展 WebCodex

- [架构](ARCHITECTURE.md) —— 主要组件如何协同
- [Coding 工作流](CODING_WORKFLOW.zh-CN.md) —— 任务启动、指导、验证和收尾
- [Native Tool Plugins](PLUGINS.zh-CN.md) —— 用任意可执行语言为 Runner 增加本地工具，不需要 MCP SDK
- [Computer Use](COMPUTER_USE.md) —— 当前桌面原语、权限与平台边界，以及明确尚未实现的方向
- [Browser/CDP 运行时架构](architecture/browser-cdp-runtime.md) —— 当前 Browser 归属、托管会话、人工接管、权限与陈旧请求隔离

## 架构阅读路径

先读每一行的当前合同，再看实现或实验记录。学习 Agent Infra 或应用设计时，
围绕“谁拥有身份、权限、状态、重试和下一次观察”追踪文档中的源码与测试，
不要把阅读笔记变成第二套规范。

| 步骤与问题 | 接着阅读 |
| --- | --- |
| 1. 请求在哪执行，设备实例替换后什么能恢复？ | [架构与请求链](ARCHITECTURE.md#module-map-and-one-request)、[Runner](RUNNER.zh-CN.md)、[Server 拥有的 Tunnel](architecture/server-owned-tunnels.md) |
| 2. 工具如何被发现，同时不扩大权限？ | [MCP](MCP.zh-CN.md)、[工具合同](agent/tool-contract-guidelines.md)、[Context/指导](architecture/builtin-context-guidance.md)、[Control sidecar](agent/control-sidecars.md) |
| 3. 什么是证据，什么是执行，什么能接续？ | [Session 模型](agent/session-model.md)、[SQLite 与 Hot/Cold 驻留](implementation/session-active-residency.md)、[Job 恢复](agent/job-reliability-and-concurrency.md)、[手动协作](agent/manual-window-collaboration.md) |
| 4. Goal、Agent Task/TaskAttempt、Conversation 和 Wake 如何区分？ | [Durable Agent 架构](architecture/durable-agent-runtime.md)、[当前通信合同](architecture/durable-agent-conversation.md) |
| 5. UI 操作为何需要精确目标与新观察？ | [Browser/CDP](architecture/browser-cdp-runtime.md)、[浏览器连续性](architecture/browser-session-continuity.md)、[Computer Use](COMPUTER_USE.md) |
| 6. 当前到底有哪些上下文与扩展资源？ | [Native Tool Plugins](PLUGINS.zh-CN.md)、[资源归属复核](architecture/resource-model-and-quality-review.md)、[Project Knowledge 插件](../plugins/project-knowledge/README.zh-CN.md)。核心 Memory、词法工具发现与可选 FTS5 插件相互独立，都不是内置的向量 RAG 流水线。 |
| 7. 哪些编排结论仍属实验？ | [Host 与 V8 Code Mode](experiments/code-mode.md)、[Code Mode 测量条件](experiments/code-mode-performance.md)、[编排研究](architecture/tool-composition-research.md) |

### 文档状态与归属

| 状态 | 如何使用 |
| --- | --- |
| 当前有效合同 | 以拥有该行为的 Schema、源码与回归测试为准；当前 `main` 的行为不意味着所有已发布版本或 Host 都可用。 |
| 历史实验或实现记录 | 保留源码版本、环境、原始测量和验证限制；通过数量属于当次实验，不是当前验收证据。 |
| 尚未实现的设计方向 | 视为有明确条件的提案，不是已支持工具或执行要求。 |
| 已废弃或被替代的设计 | 跟随明确的新入口；先将有用证据并入所属文档、修正引用，再删除重复页面。 |

`docs/agent/` 保存维护者合同及明确标注的部分历史审计；
`docs/architecture/` 保存设计与源码导航；
`docs/implementation/` 同时包含当前实现合同和特定阶段验收；
`docs/experiments/`、`docs/design/`、`docs/contribution-reports/` 保留有范围的研究与阶段记录。
Integration 和 Plugin 的消费者指南保留在相应代码旁。
不能仅凭目录、日期、标题或 TODO 数量认定文档状态；混合文档应分别说明各部分状态。

## 我想参与开发或发布

- [参与贡献](../CONTRIBUTING.zh-CN.md) —— Issue 证据、focused fix、验证与 pull request
- [Desktop 开发与打包](DESKTOP_DEVELOPMENT.zh-CN.md) —— 从源码运行 Desktop，并本地构建/验证各平台安装包
- [赞助与项目支持](SPONSORSHIP.zh-CN.md) —— 社区赞助、基础设施支持、项目合作与展示原则

以下参考文档涵盖仓库政策与 maintainer/internal contract。契约页面会有意保留
protocol field、兼容名称和实现 invariant；普通用户不需要为了使用 WebCodex 而学习
这些内容。

- [AGENTS.md](../AGENTS.md) —— 面向 coding/AI agent 的仓库开发指引
- [仓库维护](MAINTENANCE.md) —— 维护队列、依赖更新节奏、PR/CI 约定和双语文档规则
- [测试策略](TESTING.md)
- [发布清单](RELEASE_CHECKLIST.md)
- [兼容策略](compatibility-policy.md) —— 支持的已发布格式、升级桥接与明确的协议使用方
- [架构决策](agent/architecture-decisions.md)
- [工具契约指南](agent/tool-contract-guidelines.md) —— 模型可见的工具选择、权限、效果、接续与恢复
- [Durable Agent 运行时](architecture/durable-agent-runtime.md) —— 持久身份与异步工作架构
- [Durable Agent/Conversation/Wake 契约](architecture/durable-agent-conversation.md) —— 当前通信实现
- [Runtime host context](agent/runtime-host-context.md) —— Runner 配置的 planning context 与 runtime diagnostics
- [Job 可靠性与 Runner 并发](agent/job-reliability-and-concurrency.md) —— 重启恢复、观察语义、共享 Job 容量与工具描述要求
- [Tool request tracing](agent/tool-request-tracing.md) —— maintainer forensic payload/correlation contract
- [权限模型](agent/permission-model.md)
- [会话模型](agent/session-model.md)
- [手动多窗口协作](agent/manual-window-collaboration.md)
- [Runtime API 指南](agent/runtime-api-guidelines.md)
- [发布流程](agent/release-process.md)
