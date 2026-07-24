# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Custom GPT 需要调用 project-bound WebCodex Connector 时使用 GPT Actions；
client 直接支持 MCP 时优先 MCP。

## Schema

导入：

```text
https://your-domain.example/openapi.json
```

ChatGPT 要求公网 HTTPS。`webcodex setup` 有意只创建 loopback project runtime；
ingress 和 production authentication 由 operator 负责，见
[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)。

Bearer/API-key authentication 使用 scoped runtime credential。不要把
bootstrap/admin、account 或 Agent credential 粘贴进 GPT。

## Canonical hosted operations

project-bound Connector 的 OpenAPI 与 MCP 来自同一份九项 capability registry：

```text
task_start
files_read
files_search
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
```

operation count 由 generation/tests 验证；setup、pairing、token management、Agent
management、audit endpoint 和 legacy `/api/codex` route 不进入 Action schema。

Connector 已拥有确定性 project binding。Custom GPT 普通 coding 前不得调用
`listProjects`、`runtime_status`、`tool_manifest`、`start_session` 或 Agent listing，
prompt 也不包含 Agent client ID 或 runtime project ID。

## 建议 GPT Instructions

```text
Use the configured WebCodex project.
Start each bounded request with task_start.
Use files_read/files_search before edits_apply.
Use a stable operation_id for exact retry.
Run checks_run before task_finish.
Use task_review for execution progress and result review.
Use commands_run only when structured capabilities are insufficient and
approval is available.
Never ask the user for internal routing, Agent, transport, queue, or workflow
session identifiers.
```

## 人类决定

`task_finish` 创建 stable result，不会静默应用到 target checkout。host 用户执行：

```bash
webcodex task show <task-id>
webcodex task accept <task-id>
# 或：webcodex task reject <task-id>
```

即使模型托管在远端，accept authority 仍留在本机。

## 常见错误

- `project_not_configured`：运行 `webcodex setup`。
- `project_registration_invalid` / `project_credential_invalid`：解决报告的
  private-state 问题；setup 不会覆盖或静默轮换它。
- `project_credential_rejected`：恢复与可达 server 匹配的 credential；这不是
  `agent_offline`。
- `server_unreachable` / `agent_offline`：运行 `webcodex doctor`，再执行其 next
  action。
- `required_capability_unavailable` /
  `structured_validation_unavailable`：升级全部 WebCodex binaries。
- `task_not_active`：开始新 task。
- `execution_not_terminal`：review、wait 或 cancel execution。
- `checks_required`：调用 `checks_run`。
- `checks_stale`：使用新 operation ID 运行 fresh check。

每个错误都携带 stable code、human message、retryability、
`user_action_required` 和 suggested next action。控制流必须使用 code，不得匹配任意
英文 message。

## 相关文档

- [QUICK_START.zh-CN.md](QUICK_START.zh-CN.md)
- [MCP.zh-CN.md](MCP.zh-CN.md)
- [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)
- [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)
- [../SECURITY.md](../SECURITY.md)
