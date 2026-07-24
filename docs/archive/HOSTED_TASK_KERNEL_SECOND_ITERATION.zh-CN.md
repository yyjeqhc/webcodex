# Hosted Task Kernel：第二轮实现与验收边界

> Archive：历史实现记录，不是当前使用指南。当前入口见 `../QUICK_START.zh-CN.md`。
>
> 历史说明：本文件记录第二轮当时的单文件编辑面；第五轮已将同名 `edits_apply` 原位升级为事务式多文件 edit/create/delete/rename，当前语义见 `HOSTED_EDIT_RELIABILITY_FIFTH_ITERATION.zh-CN.md`。

> 状态：第二轮核心纵向切片，2026-07-15。本文记录已经进入代码的行为，不把后续隔离 workspace、人工审批或真实公网平台验收写成已完成。

## 1. 本轮的体验变化

第一轮解决的是“怎样用一个命令把当前项目接到线上窗口”。第二轮解决的是“连接成功以后，模型第一次看到什么，以及怎样开始一项工作”。

由 `webcodex connect` 启动的 hosted profile 不再把完整 ToolRuntime 暴露给模型。MCP `tools/list` 和 GPT Actions `/openapi.json` 都从同一份 registry 生成下面 8 项能力：

| 能力 | 单一职责 |
|---|---|
| `task_start` | 在已连接项目中创建 Task 和 Run |
| `files_read` | 读取一个同意图的小批量文件 |
| `files_search` | 在已连接项目中做有界文本搜索 |
| `edits_apply` | 对一个文件原子应用 structured text edits |
| `checks_run` | 运行 format/check/test 标准检查 |
| `commands_run` | 显式执行一个受控 raw command |
| `task_review` | 一次返回 change summary 和 task timeline |
| `task_finish` | 捕获最终 changes，并原子进入 `ready_for_review` |

项目不是一个需要模型先调用的 discovery tool。`task_start` 只接收 `goal` 和可选 `mode`；后续调用只携带 `task_id`。模型不再填写 `client_id`、`agent:<client>:<project>`、`session_id` 或 `recording_session_id`。

Hosted PAT/OAuth/shared-key 直接请求旧 `/api/tools/call`、项目列表或 session/status 路由会被认证层拒绝。bootstrap 仍可完成本机初始化；agent token 仍只能访问精确的 executor transport 路由。普通 `webcodex serve` 未启用 project-bound connector context 时，旧运维/诊断 surface 保持原状，便于继续迁移而不影响管理入口。

## 2. Task/Run 已成为独立事实源

SQLite 新增以下 vNext authority：

- `wc_projects`：逻辑项目；
- `wc_workspaces`：逻辑项目到当前 executor checkout 的内部绑定；
- `wc_connector_grants`：认证 subject 对项目与 8 项能力的 grant；
- `wc_tasks`：目标、owner subject、mode 和 task state；
- `wc_runs`：一次 workspace executor attempt；
- `wc_task_events`：每个 task 单调递增的事实事件。

ID 使用 `wc_proj_*`、`wc_ws_*`、`wc_cgr_*`、`wc_task_*`、`wc_run_*` 和 `wc_evt_*`。它们不复用 `wc_sess_*`，也不向旧 workflow session ledger 双写。

当前已实现的最小状态机是：

    Task: active -> ready_for_review
    Run:  running -> completed

`task_start` 在一个 transaction 中创建 Task、Run 和 sequence=1 的 `task_started` event。读/改/检查/命令等执行型能力成功、失败或被 read-only guard 拒绝后，都会追加有界 metadata event；`task_review` 只是这些事实与当前 changes 的只读投影，不再为“查看状态”制造新事实。Event payload 不保存源码、命令 stdout 或凭据。`task_finish` 只有在最终 change capture 成功后，才在一个 transaction 中追加结束 event 并同时迁移 Task/Run 状态。

模型不能 accept/reject、commit、push 或 deploy。`ready_for_review` 表示等待人类判断，不表示工作已经被接受。

## 3. Project-bound connector context

`connect` 根据当前 worktree 和 profile 生成稳定的逻辑 Project/Workspace id，并只向它监督的 loopback runtime 注入内部 context：

- 模型可见：逻辑 `wc_proj_*`、`wc_task_*`、`wc_run_*`；
- adapter 内部：executor runtime project id 和本地绝对路径；
- protocol adapter：MCP 或 REST/OpenAPI framing；
- ingress adapter：OpenAI Tunnel 或 Cloudflare。

Canonical capability 仍通过现有 ToolRuntime kernel 执行，因此继续使用原有 OAuth scope、agent owner boundary、permission gate、sensitive-path policy 和超时。adapter 会注入 executor project，并在响应中递归替换内部 runtime id/绝对 root，同时删除 `client_id`、`request_id` 和 executor transport metadata。

同一 Task 的跨设备请求按 task lock 串行，防止 `task_finish` 与仍在执行的 edit/command 竞争；同一 Workspace 的读操作共享 read lease，edit/check/command/finish 使用独占 lease。不同 Task 可以并发进入应用层，但在当前同一 checkout 上的 consequential 操作不会同时执行。

## 4. 多用户与多设备边界

Task owner 使用认证后的稳定 subject，而不是 token id：

- 同一 managed user 的多个 PAT/OAuth token 使用同一个 `user:<id>` subject，因此可以携带同一个 `task_id` 在多设备/多窗口继续；
- shared-key subject 按不可复用的 key hash 分组；
- 不同 user subject 查询同一个 `task_id` 统一返回 `task_not_found`，不泄露 task 是否存在；
- ConnectorGrant 绑定 `subject + logical project + capabilities`，不把 tunnel credential 当用户身份。

这只是正确的本地 authority 边界，不等于已经实现跨机器同步。个人 `connect` profile 的 SQLite 仍在当前机器；机器离线后，远端窗口不能继续调用，另一台机器也不会自动获得这份 Task/event 数据。真正的多设备共享需要后续 shared control plane 和 Device/Workspace registration。

当前多个用户若获准进入同一个 connector，会拥有彼此隔离的 Task timeline，但底层仍指向同一个 checkout。Workspace lease 可以避免并发写竞态，却不能提供每个 Task 的内容隔离；这是第三轮必须解决的主要问题。

## 5. 统一响应与错误

成功响应固定包含：

    ok
    task_id
    run_id
    event_cursor
    data
    warnings
    blocking

失败额外包含：

    error.code
    error.message
    error.retryable
    error.user_action_required
    error.suggested_action

Malformed schema/未知 capability 在 MCP 中是 JSON-RPC `-32602`；scope denial 是 HTTP 403；Task state、read-only guard 和 executor failure 是正常的 tool error，并保留 task/run/cursor，避免模型在不确定副作用后盲目重试。

## 6. 本轮验证证据

本轮新增或固定了以下回归边界：

- fresh SQLite authority、单调 event sequence、原子 finish；
- 同 project 下跨 subject 的 Task 隔离；
- read-only Task 在 executor dispatch 前拒绝 edit/check/command；
- canonical `files_read` 经过真实 ShellClientRegistry poll/result harness 到达唯一绑定 executor；
- adapter 响应不出现 runtime project id、executor root、client id 或 transport request id；
- hosted MCP `tools/list` 精确等于 8 项 registry；
- hosted OpenAPI operation set 从同一 registry 生成，精确等于 8 且输入 schema 为 strict object；
- OAuth route scope 与 hosted capability risk 对齐；
- hosted user credential 无法回退调用旧 runtime/session surface。

最终的命令和数量应以本次提交的验证报告为准；真实 OpenAI/Cloudflare 账号路径仍需在有 provider credential 的机器上补验。

## 7. 明确延期到第三轮

第三轮应继续只做最提升体验的主路径：

1. 为写 Task 创建隔离 execution worktree，并捕获可比较 baseline；
2. 让 `task_review`/`task_finish` 生成稳定 Task Result（diff、changed files、validation evidence）；
3. 增加真正的人类 accept/reject/approval authority，模型工具不得代替人类决定；
4. 处理 runtime 异常退出，把仍为 running 的 Run 恢复为可判定的 interrupted；
5. 再用真实 MCP Connector 与 GPT Actions 跑一次 start -> inspect -> edit -> check -> review -> finish acceptance。

本轮没有实现内置 LLM、模型 loop、任务自动继续、通用多语言 check planner、浏览器 Task UI、跨机器同步或 shared control plane。也没有为了兼容旧 surface 给 8 项能力添加 aliases。
