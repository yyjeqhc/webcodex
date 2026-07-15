# 概念

[English](CONCEPTS.md) | [简体中文](CONCEPTS.zh-CN.md)

WebCodex 让线上 AI client 通过自托管、可审计的工具 runtime 操作私有代码仓库。本文定义 setup、MCP、GPT Actions、安全和架构文档中的核心术语。

## 心智模型

```text
Online model / client
        |
        | MCP / GPT Actions / REST tool calls
        v
WebCodex server
        |
        | authenticated agent bridge
        v
WebCodex agent
        |
        v
Agent-registered project
```

项目在 agent 所在机器上。agent 只把被允许的目录注册给 server；server 不扫描你的文件系统。

## 核心术语

### Online Model / Client

online model 指 ChatGPT、Claude、Grok 或其他 hosted model。client 指向 WebCodex 发送 tool calls 的 host surface，例如 remote MCP、GPT Actions 或 REST integration。

模型不会获得直接文件系统访问权。它只能调用当前 client 暴露、并被 WebCodex 授权的 tools。

### WebCodex Server

`webcodex` 是自托管 server。它暴露 MCP endpoint、GPT Actions OpenAPI schema 和 runtime REST APIs。它负责认证 caller、应用 tool policy、记录有界 session evidence，并把项目工作路由给已连接 agent。

server 是稳定的线上入口。连接 hosted client 前，应把它放到 HTTPS 后面。

### WebCodex Agent

`webcodex-agent` 运行在拥有代码的机器上。它反向连接 server，注册允许的项目，并在项目边界内执行 file、Git、patch、validation、shell、job、artifact 和 checkpoint 请求。

agent 是离仓库最近的信任边界。应为它配置尽量窄的 allowed roots，以及适合项目的 shell profiles。

### Agent-Registered Project

agent-registered project 是 agent 暴露给 server 的目录。server 不会自行发明或发现项目路径。

runtime project id 使用这种形状：

```text
agent:<client_id>:<project_id>
```

`client_id` 标识 agent connection profile。`project_id` 是该 agent 注册的项目 id。prompt 和 tool call 中应写完整 runtime project id，避免模型选错仓库。

### Tool Runtime

ToolRuntime 是与协议无关的执行层。MCP、GPT Actions 和 REST wrapper 会把 client request 转成同一套 runtime tool calls。

常见工具组：

- Discovery：`runtime_status`、`list_projects`、`list_agents`、`tool_manifest`。
- Inspect：`list_project_files`、`search_project_text`、`read_file`、`git_status`、`git_diff_hunks`。
- Edit：`apply_text_edits`（带 guard 的事务式文件变更）、`apply_patch_checked`（复杂 checked unified diff）、`write_project_file`（有意的整文件重写）。行/模式类工具仍为兼容路径。
- Validate：`validate_patch`、`cargo_fmt`、`cargo_check`、`cargo_test`。
- Review：`show_changes`、`workspace_hygiene_check`。
- Finish：`finish_coding_task`、`session_handoff_summary`。
- Escape hatch：`run_shell`、`run_job`。

### MCP

MCP client 连接：

```text
https://your-domain.example/mcp
```

如果 client 支持 remote MCP，使用 MCP。MCP 通过 MCP framing 暴露 WebCodex runtime tools，同时保留与 GPT Actions 相同的 server、agent、project id 和安全边界。

### GPT Actions

GPT Actions 导入 WebCodex OpenAPI schema：

```text
https://your-domain.example/openapi.json
```

如果你在构建 Custom GPT，使用 GPT Actions。GPT Actions 暴露精简 REST operation surface，并通过 generic `callRuntimeTool` 调用 runtime tools。它和 MCP 共享同一个 WebCodex ToolRuntime。

### Session

session 是有界任务记录。`start_coding_task` 创建推荐的 coding session，并返回显式 `session_id`。保存该 id，并在后续 review、validation、handoff 或 finish tool 支持时传入。

脏工作区是预期中的开发状态，**不会**阻止启动 coding task。已有的 worktree 改动（tracked modified、staged、untracked、renamed、deleted 或 conflicted）必须先检查并保留；不得自动 revert、stash、clean 或覆盖。Startup blocking 仅保留给项目不可访问、或请求的工作不安全/不可能的情况（路径不存在、解析失败、所需 agent 离线、权限拒绝、路径安全失败等）。review/finish 工具在收口时仍可将 dirty closeout 作为非 pass 证据。

session 是 task-continuity evidence，不是完整监控日志。它记录有界、redacted 的事实，例如 tool name、status、project id、validation summaries、permission decisions 和 closeout state。

### Handoff / Finish

`finish_coding_task` 是常规收口工具。它可以包含 review evidence、workspace hygiene、validation summary、job state、warnings 和规范的 task/evidence outcomes。

`session_handoff_summary` 是只读 handoff 工具。当另一个 operator、client 或后续 session 需要接手时使用。

### Validation

validation 是变更已被检查的证据。WebCodex 提供 `validate_patch`、`cargo_fmt`、`cargo_check` 和 `cargo_test` 等结构化 helper。

应选择与变更匹配的验证。docs-only edit 可能只需要 WebCodex 外部的 `git diff --check` 或定向 review；Rust 行为变更应运行 Cargo check 或 tests。

### Review / Hygiene

review tools 在用户接受结果前展示变更。`show_changes` 用于查看文件列表、状态、diff stat 和可选有界 hunks。`workspace_hygiene_check` 用于发现 untracked smoke files、临时文件、blocking jobs 和其他收口风险。

### `run_shell` 作为 Escape Hatch

`run_shell` 可以通过 agent 运行受限项目命令。它适合尚无结构化 helper 的项目特定检查。

它不是默认编辑路径，不是第一验证选择，也不是绕过项目策略的方式。shell/job tools 很强，需要可信配置和人工 review。

## 默认 Coding Loop

1. `start_coding_task`
2. 用 `list_project_files`、`search_project_text` 和 `read_file` inspect。
3. 用结构化 edit 或 patch tools 修改。
4. 用结构化 validation tools 验证。
5. 用 `show_changes`、`git_diff_hunks` 和 `workspace_hygiene_check` review。
6. 用 `finish_coding_task` 收口，或用 `session_handoff_summary` 交接。

## 下一步

- 第一次设置：[QUICK_START.zh-CN.md](QUICK_START.zh-CN.md)
- Demo 工作流：[DEMO.zh-CN.md](DEMO.zh-CN.md)
- 架构：[ARCHITECTURE.md](ARCHITECTURE.md)
- MCP：[MCP.zh-CN.md](MCP.zh-CN.md)
- GPT Actions：[GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)
- 安全：[../SECURITY.md](../SECURITY.md)
