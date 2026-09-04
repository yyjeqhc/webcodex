# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Custom GPT 需要通过 Server 的 OpenAPI surface 调用 WebCodex 时使用 GPT Actions；客户端直接支持 MCP 时请用 [MCP](MCP.zh-CN.md)。导入的 schema 由当前 Server 配置决定：project-first 部署暴露 project-bound actions，普通 Server 暴露其 runtime actions。普通用户不需要理解内部 surface type 名称。

## 什么是 GPT Action

WebCodex 提供基于 OpenAPI 的 **Custom GPT Action** 集成。它不是已发布的
ChatGPT plugin。在当前的 OpenAI 术语里，app、Custom GPT 与 Action 是不同层；
plugin 是 ChatGPT/Codex plugin 目录中可安装的包。参见 OpenAI 的
[GPT Actions 介绍](https://developers.openai.com/api/docs/actions/introduction)。

## 导入 schema

把 OpenAPI schema 导入你的 Custom GPT：

```text
https://your-domain.example/openapi.json
```

不要预设导入后的 operation 集合；直接检查当前 Server 返回的 operation names。Project-first 与普通 runtime 部署会有意暴露不同的 operation set。

ChatGPT 需要公网 HTTPS。把 API-key 认证配置为 HTTP Bearer。使用生成的
`webcodex-user-token`（`wc_pat_*`）——它用于 GPT Actions、MCP 与普通 REST/项目
API。Runner token（`wc_agent_*`）只被 Runner 传输 endpoint 接受；不要把
bootstrap/admin token 或 account credential 粘贴到 GPT 中。

OpenAPI 管理 surface 有意排除 users、API token、Runner token、pairing/enrollment、
setup、doctor、npm、server 管理与 audit endpoint。这些请用 `webcodex` CLI 完成。

## Connector surface

Server 以 project-bound Connector 配置运行时，OpenAPI 从与 canonical MCP
Connector 相同的十四个 capability 生成：

```text
task_start
task_list
task_resume
files_list
files_read
files_search
code_navigate
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
code_impact
```

Connector 已经绑定项目。普通 coding 直接从 Connector actions 开始，不要先做 broader
runtime/project discovery，也不要在 prompt 中放 Runner client ID 或 runtime project ID。

`task_start` 只接受 `normal`（默认）和 `read_only`。`normal` 在受管理的隔离 Git
worktree 中执行可写工作；无法安全准备 workspace 时会 fail closed，模型不会直接写
目标 checkout，也不能接受自己的结果。`read_only` 允许分析，但拒绝 edit、command 与
check。

## 建议的 GPT 指令

```text
使用配置好的 WebCodex 项目。
每次用户指令用 task_start 开始或延续。
让 task_start 复用当前项目上下文；不要向用户询问 ID。
只有 WebCodex 明确要求恢复或继续已有 task 时才使用 task_list 与 task_resume。
猜测路径前先用 files_list 查看项目内容。
在 edits_apply 前使用 files_read/files_search。
使用 code_navigate 进行只读的语义状态、symbols、definition、references、
diagnostics 与 hover；只提供项目相对路径。
使用 code_impact 做有界 incoming/outgoing call hierarchy 与变更影响检查；只提供
项目相对路径和源码位置。
在 task_finish 前运行 checks_run。
用 task_review 查看执行进度与结果审查。
仅当结构化能力不足且有人工审批时使用 commands_run。
永远不要向用户询问 WebCodex 内部 ID；后续调用需要时直接使用工具返回的值。
```

## 校验

`checks_run` 是唯一的结构化校验 Action。它接受可选 `recipe` 枚举（`rust`、
`node`、`python`、`go`）；省略时做确定性的最近 manifest 解析。Recipe 不安装
依赖、不修改 lockfile、不使用网络。缺少工具是 executor 失败；已启动 validator
的非零判定是断言失败。recipe 表格见 [MCP](MCP.zh-CN.md#校验-recipe)。

## 人工决策

`task_finish` 生成稳定结果；它不会静默地把变更应用到目标 checkout。由宿主用户
在本地审查并决策：

```bash
webcodex task show <task-id>
webcodex task accept <task-id>
# 或：webcodex task reject <task-id>
```

即使模型是 hosted 的，接受权也保留在本地。

## 常见错误

- 复制 `wc_agent_*` 后出现认证错误，说明选错了凭据类型。请改用生成的
  `webcodex-user-token`；不要把完整令牌值粘贴到日志或 bug 报告。
- `project_not_configured`：运行 `webcodex setup`。
- `project_credential_invalid` / `project_credential_rejected`：解决报告的
  私有状态问题，然后恢复匹配的凭据。
- `server_unreachable` / `agent_offline`：运行 `webcodex doctor`，再执行报告的
  next action。
- `required_capability_unavailable` / `structured_validation_unavailable`：
  升级所有 WebCodex 二进制。
- `checks_required`：调用 `checks_run`。
- `checks_stale`：针对当前 task state 重新运行要求的检查。

每个错误都带稳定 code、人类可读消息、可重试性与建议的下一步。控制流应使用
code，而不是匹配任意英文消息。

## 相关文档

- [完整使用指南](PERSONAL_SETUP.zh-CN.md)
- [快速试用](QUICK_START.zh-CN.md)
- [MCP](MCP.zh-CN.md)
- [认证模型](AUTH_MODEL.zh-CN.md)
- [部署指南](DEPLOYMENT.zh-CN.md)
- [SECURITY.md](../SECURITY.md)
