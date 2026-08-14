# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex 可以暴露不止一种 MCP model surface。project-first 的
`webcodex run` / `webcodex share` 会启动带 project-bound
`canonical_connector` surface 的 Server。`webcodex connect <server>` 则连接
已有 hosted Server，并使用该 Server 选择的 MCP surface；没有 Connector 配置时，
默认是更宽的 `local_coding` surface。project-first 流程先看
[快速开始](QUICK_START.zh-CN.md)。

## Endpoint 与认证

本地客户端可用：

```text
http://127.0.0.1:<configured-port>/mcp
```

Hosted 客户端需要 HTTPS。有三种路径：

- **Hosted：** `webcodex connect <server>` 使用已有 hosted Server；本地只运行
  Runner。MCP URL 为 `https://your-server.example/mcp`，bearer 凭据为生成的共享
  key。暴露的工具集合由该 Server 配置的 MCP model surface 决定；`connect` 不会
  把远端 Server 变成 project-bound Connector。
- **本地分享：** `webcodex share` 启动本地 Server + Agent 与 Cloudflare Quick
  Tunnel，并输出临时 HTTPS `/mcp` URL 与独立的临时 Bearer credential。Ctrl-C
  通过停止 runtime/tunnel 并删除临时分享状态来撤销访问。`--tunnel none` 只用于
  本地测试。
- **自托管：** 使用稳定 HTTPS 域名/tunnel、持久服务管理与 OAuth 或受限凭据进行
  长期运行。

对于 managed 或自托管 Server，用 user API token（`wc_pat_*`）作为 bearer
凭据；启用 OAuth 时用 OAuth。不要把 bootstrap/admin token、account credential、
Runner token 或持久 project-first Connector credential 当作公开分享 secret。
`share` 会为该会话创建并打印自己的临时 credential。

在 ChatGPT Developer Mode 中，用输出的 `/mcp` URL 创建自定义 app。如果认证菜单
提供 **访问令牌/API 密钥**，选择它并粘贴 bearer credential，然后执行 **Scan
Tools / 扫描工具**。ChatGPT 的 UI 文案与可用范围可能随 workspace 与 rollout
变化。

### OAuth2

当 managed 或自托管 Server 启用 OAuth 时，MCP 客户端可以使用 authorization-code
流程而非静态 token。把精确的 ChatGPT callback URL 注册为 OAuth client redirect
URI；宿主提供 `offline_access` 时保持勾选（它是协议级 refresh-token scope，不授予
额外权限）。服务端 OAuth 设置见[部署指南](DEPLOYMENT.zh-CN.md#oauth2)。

## project-bound surface

Server 以 project-first Connector 配置（`canonical_connector`）启动时，MCP
`tools/list` 恰好包含以下十四个操作。这是 `webcodex run` 与 `webcodex share`
使用的 surface；没有 Connector context 的普通 hosted/self-hosted Server 默认暴露
`local_coding`（或显式的 `full_operator_runtime`）而不是这十四个操作：

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

Connector context 已绑定配置的仓库。用 `task_start` 开始；不要调用项目发现、
session 或 runtime 工具，也不要在 prompt 里放 runtime project id。同一个聊天窗口
会自动延续当前仓库的工作。`task_list` 与 `task_resume` 是客户端无法再提供传输
窗口身份时的显式恢复工具。

## 黄金 coding 循环

```text
task_start
→ files_list
→ files_read / files_search / code_navigate / code_impact
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

- `files_list` 从 Git index 回答"项目里有什么"，因此被忽略的目录不会出现。猜测
  路径前先调用它。
- `code_navigate` 提供只读的语言服务器状态、document/workspace symbols、
  definition、references、diagnostics 与 hover。它只接受项目相对路径和从 1 开始的
  Unicode scalar 位置；Connector 负责选择已绑定的 executor project。参数按
  operation 严格区分：`status` 不带额外字段；document symbols 与 diagnostics
  使用 `path`；workspace symbols 使用 `query`；definition、references 与 hover
  使用 `path` + `line` + `column`。无意义的字段会被拒绝。normal、inspect 和
  read-only task 均可调用。
- `code_impact` 从项目相对源码位置执行一次有界 call hierarchy 操作。它支持
  `incoming`、`outgoing`、`both`，广度优先深度为 1 或 2，全局 edge 上限为
  1..100；只返回规范化的项目内 root、edge 和有界 call-site range。语言服务器
  不支持时会显式失败，不回退到 grep 或 AST。normal、inspect、read-only task
  均可调用。
- `edits_apply` 是受保护的编辑工具；`commands_run` 是需要 shell 的命令的有界
  逃生口。
- `checks_run` 做校验。使用稳定 `operation_id`，让精确重试复用同一操作。
- `task_finish` 生成稳定结果；由人工在本地用 `webcodex task accept <id>` /
  `webcodex task reject <id>` 接受或拒绝。模型永远不能接受自己的工作。

### 校验 recipe

`checks_run` 支持 `format`、`check`、`test` 与可选 `recipe` 枚举（`rust`、
`node`、`python`、`go`）。省略 `recipe` 时，从任务 `cwd` 相对位置最近的
`Cargo.toml`、`package.json`、`pyproject.toml` 或 `go.mod` 自动解析。Recipe 不
安装依赖、不修改 lockfile、不使用网络。缺少工具是 executor 失败；已启动的
validator 返回非零是断言失败。

| Recipe | 标记 | `format` | `check` | `test` |
| --- | --- | --- | --- | --- |
| Rust | `Cargo.toml` | `cargo fmt -- --check` | `cargo check --all-targets` | `cargo test` |
| Node | `package.json` | `format:check`/`format-check`/`check:format` 第一个 | `check`/`typecheck`/`lint` 第一个 | 精确 `test` |
| Python | `pyproject.toml` | 配置的 Ruff/Black | 配置的 Ruff/Mypy | 配置的 pytest |
| Go | `go.mod` | 不可用 | `go vet ./...` | `go test -json ./...` |

### 长校验会持久继续

`checks_run` 与 `commands_run` 使用 durable execution；工作仍在继续时，调用大约
8 秒后可能 quick-yield。在十四工具 Connector surface 上，用 `task_review` 的
`after_cursor` / `wait_ms`（需要输出时再加 `include_output_tail=true`）观察进度，
直到 execution 进入 terminal；需要停止时调用 `task_cancel`。不要为了轮询而重新
执行同一个操作。

更宽的 `local_coding` 与 `full_operator_runtime` MCP surface 才暴露
`job_status`、`job_log`、`validation_summary`、`stop_job` 等原始 Job 工具；这些
工具不属于十四个 Connector capability。

## 第一个安全 prompt

```text
Use the configured WebCodex project. Start a read-only task, read README.md,
summarize the project, review the result, and finish. Do not edit files.
```

这个 prompt 里不需要项目发现或 runtime 标识符。

## 读取与搜索边界

- `read_file` 是有界流式范围读取：`start_line`（默认 1）、`limit`（默认 2000，
  最大 2000），返回范围加上完整文件 SHA-256 与行元数据，以及用于继续的
  `next_start_line`。
- `read_files` 批量执行最多 8 次单文件读取，条目结果相互独立。
- `search_project_text` 是默认搜索工具（优先 ripgrep，工作量与字节均有界）；
  `search_project_texts` 批量执行最多 8 个查询。

失败返回只含项目相对路径的小型结构化错误——绝不包含绝对路径、命令或 Runner
stderr。

## 常见错误

| 错误码 | 含义 | 处理 |
| --- | --- | --- |
| `project_not_configured` | 没有 canonical setup | 运行 `webcodex setup` |
| `project_credential_invalid` | 私有 Project Credential 缺失或不匹配 | 恢复两个匹配的私有文件或重建 profile |
| `project_credential_rejected` | 可达 server 拒绝了该凭据 | 恢复与 server 匹配的凭据 |
| `workspace_unavailable` | 配置的 Git 工作区不可用 | 恢复工作区，再运行 doctor |
| `server_unreachable` / `agent_offline` | 项目 runtime 或 Agent 不可用 | 运行 `webcodex run` / `webcodex doctor` |
| `required_capability_unavailable` | Agent 缺少 coding 能力 | 升级所有二进制 |
| `task_not_active` | 任务无法再变更或执行 | 开始新任务 |
| `execution_not_terminal` | Finish 被活跃/未知工作阻塞 | 审查/等待/取消 |
| `checks_required` | 普通任务尚未运行检查 | 调用 `checks_run` |
| `checks_stale` | 上次检查后工作区已变化 | 运行一次新检查 |

## 高级 runtime surface

在 project-bound Connector 之外，WebCodex 还可以作为多项目管理 ToolRuntime 运行，
提供 discovery、session、LSP、raw job 与 artifact 工具。那是面向运维者的高级
surface，不是 canonical project Connector，也不是普通 coding 的前提。

更宽的 coding surface 暴露 `work_on_project` 或 `start_coding_task` 时，请阅读
[Coding 工作流](CODING_WORKFLOW.zh-CN.md)，按 canonical mental model 区分 bootstrap
与 behavioral role，并遵循其中的 validation/closeout guidance。运维工具见
[架构](ARCHITECTURE.md)与 `webcodex` CLI。
