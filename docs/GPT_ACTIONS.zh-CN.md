# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Custom GPT 需要通过 Server 的 OpenAPI surface 调用 WebCodex 时使用 GPT Actions；
客户端直接支持 MCP 时请用 [MCP](MCP.zh-CN.md)。两者都适配同一个 runtime，但
实际暴露的 schema 取决于 Server mode：带 Connector 配置的 project-first Server
暴露 project-bound Connector schema；普通 hosted/self-hosted Server 暴露标准
runtime OpenAPI schema。

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

不要预设导入后的 operation 集合。带 project-bound Connector 配置的 Server 返回
下文十二个 capability；普通 Server 则返回标准 runtime OpenAPI projection。

ChatGPT 需要公网 HTTPS。把 API-key 认证配置为 HTTP Bearer。使用生成的
`webcodex-user-token`（`wc_pat_*`）——它用于 GPT Actions、MCP 与普通 REST/项目
API。Runner token（`wc_agent_*`）只被 Runner 传输 endpoint 接受；不要把
bootstrap/admin token 或 account credential 粘贴到 GPT 中。

OpenAPI 管理 surface 有意排除 users、API token、agent token、pairing/enrollment、
setup、doctor、npm、server 管理与 audit endpoint。这些请用 `webcodex` CLI 完成。

## Connector surface

Server 以 project-bound Connector 配置运行时，OpenAPI 从与 canonical MCP
Connector 相同的十二个 capability 生成：

```text
task_start
task_list
task_resume
files_list
files_read
files_search
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
```

Connector 已经拥有确定性的项目绑定。Custom GPT 在普通 coding 前不得调用
`listProjects`、`runtime_status`、`tool_manifest`、`start_session` 或 Agent
listing，prompt 中也不得包含 Agent client ID 或 runtime project ID。

## 建议的 GPT 指令

```text
使用配置好的 WebCodex 项目。
每次用户指令用 task_start 开始或延续。
让 task_start 复用当前项目上下文；不要向用户询问 ID。
只有在 WebCodex 报告自动传输窗口恢复不可用时才使用 task_list 与 task_resume。
猜测路径前先用 files_list 查看项目内容。
在 edits_apply 前使用 files_read/files_search。
用稳定的 operation_id 做精确重试。
在 task_finish 前运行 checks_run。
用 task_review 查看执行进度与结果审查。
仅当结构化能力不足且有人工审批时使用 commands_run。
永远不要向用户询问 task、session、current-binding、Agent、transport、queue
或 workflow 标识符。
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
- `checks_stale`：用新的 operation ID 运行一次新检查。

每个错误都带稳定 code、人类可读消息、可重试性与建议的下一步。控制流应使用
code，而不是匹配任意英文消息。

## 相关文档

- [快速开始](QUICK_START.zh-CN.md)
- [MCP](MCP.zh-CN.md)
- [认证模型](AUTH_MODEL.zh-CN.md)
- [部署指南](DEPLOYMENT.zh-CN.md)
- [SECURITY.md](../SECURITY.md)
