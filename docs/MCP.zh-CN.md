# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

client 能连接 project-bound WebCodex endpoint 时使用 MCP。先完成
[QUICK_START.zh-CN.md](QUICK_START.zh-CN.md)。

## Endpoint 与认证

本地 client 可以使用：

```text
http://127.0.0.1:<configured-port>/mcp
```

Hosted client 需要 operator 管理的 HTTPS endpoint：

```text
https://your-domain.example/mcp
```

Bearer credential 必须用于 runtime/project access。不要使用或暴露
bootstrap/admin、account 或 Agent credential。优先使用 client secret store，
不要提交 token。

Canonical setup 不打印 credential value 或 secret path，不创建 tunnel，也不开放
公网端口。production enrollment、scoped user token 和 OAuth 见
[AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md) 与
[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)。

## Project-bound surface

runtime 由 `webcodex setup` 配置时，MCP `tools/list` 恰好包含：

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

Connector context 已绑定项目。直接从 `task_start` 开始；不要调用
`list_projects`、`runtime_status`、`tool_manifest`、`start_session` 或
`current_session`，也不要向用户索取 Agent client ID、runtime project ID、
executor reference 或 workflow session。

保留的 durable ID 都有明确产品用途：

- `task_id`：继续/review 一个 bounded task；
- `operation_id`：mutation/execution 的 exact retry identity；
- `execution_id`：查看、等待或取消一个 durable execution；
- `result_id`：review 并决定一个 stable result。

Agent transport、executor routing 和 pending request ID 保持内部实现。

## Golden coding loop

```text
task_start
→ files_read / files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

`commands_run` 只作为需要 approval 的 escape hatch。queued/running execution
需要停止时使用 `task_cancel`。

普通可写 task 必须在 finish 前运行 structured checks。成功 check 带 trusted
workspace provenance；之后任何 mutation 都使其 stale，并要求新的 operation ID。
command 无法 spawn 属于 executor failure，不是 assertion evidence。

### `checks_run` recipes

`checks_run` schema 仍只暴露 `format`、`check`、`test`，以及可选 enum
`recipe`（`rust`、`node`、`python`、`go`）。省略时，从 Task workspace 的相对
`cwd` 选择最近的 `Cargo.toml`、`package.json`、`pyproject.toml` 或 `go.mod`。
只有同 root 歧义需要显式 recipe，且对应 marker 必须存在；resolver 不扫描 sibling
project，也不允许 path/symlink escape。

Rust 支持三项 check，并且是唯一支持单 argv `test_filter` 的 recipe。Node 使用有
证据的 package manager 和固定 script allowlist；Python 只选择已配置的
Ruff/Black、Ruff/Mypy 或 pytest；Go 支持 `check/test`，`format` unavailable。
所有 recipe 都不安装依赖、不修改 lockfile、不联网。tool 缺失属于 executor
failure；validator 启动后 non-zero 才属于 assertion failure。recipe version、相对
root 和 invocation/manifest evidence 绑定 operation exact-retry identity。这不会
增加第十项 MCP tool；MCP、OpenAPI 与 capability registry 仍共享同一九项 source。

review 后由人类在 host 上接受或拒绝：

```bash
webcodex task show <task-id>
webcodex task accept <task-id>
# 或：webcodex task reject <task-id>
```

## 第一个安全 Prompt

```text
Use the configured WebCodex project. Start a read-only task, read README.md,
summarize the project, review the result, and finish. Do not edit files.
```

prompt 中不需要 project discovery 或 runtime identifier。

## 常见错误

| Code | 含义 | Action |
|---|---|---|
| `project_not_configured` | canonical setup 不存在 | `webcodex setup` |
| `project_registration_invalid` | 本地 project state malformed、incomplete 或冲突 | 解决报告的 private-state conflict |
| `project_credential_invalid` | private Project Credential 缺失、权限不安全、格式错误或两份不匹配 | 恢复两份匹配 private file，或显式重建 profile |
| `project_credential_rejected` | 可达 server 拒绝已配置 Project Credential | 恢复与 server 匹配的 credential；不得折叠为 Agent offline |
| `workspace_unavailable` | 配置的 Git workspace 不可用 | 恢复 workspace 后运行 doctor |
| `server_unreachable` | project runtime 不可用 | `webcodex agent start` |
| `agent_offline` | 本地 Agent 未 ready | `webcodex doctor` |
| `required_capability_unavailable` | Agent 缺少 coding capability | 升级全部 binaries |
| `structured_validation_unavailable` | Agent 不能运行 structured checks | 升级全部 binaries |
| `task_not_active` | task 已不能 mutation/execute | 新建 task |
| `execution_not_terminal` | active/unknown work 阻止 finish | review/wait/cancel |
| `validation_recipe_not_found` / `validation_recipe_ambiguous` | auto resolution 没有 recipe 或最近 root 多 recipe | 修改 `cwd` 或提供匹配 `recipe` |
| `validation_recipe_mismatch` / `validation_manifest_invalid` | 显式 recipe、路径、marker 或 manifest evidence 无效 | 修复报告的公开 evidence |
| `validation_check_unavailable` / `test_filter_unsupported` | 请求的 semantic input 没有安全映射 | 修改 check/filter |
| `package_manager_ambiguous` | Node package-manager evidence 缺失或冲突 | 修正 `packageManager` 或 lockfile |
| `validation_tool_unavailable` | 所选 executable/module 缺失 | 提供项目已有工具并使用新 operation ID |
| `checks_required` | 普通 task 尚未运行 checks | 调用 `checks_run` |
| `checks_stale` | 上次 check 后 workspace 改变 | 运行新的 check |

短答案使用 `webcodex status`，完整只读 findings 使用 `webcodex doctor`。

## Advanced runtime surface

WebCodex 也可以作为 multi-project management ToolRuntime 运行。其 discovery、
session、LSP、raw job、artifact 和 operator tools 继续记录在
[OPERATIONS.md](OPERATIONS.md)。那是高级 surface，不是 canonical project
Connector，也不是普通 coding 的前置步骤。
