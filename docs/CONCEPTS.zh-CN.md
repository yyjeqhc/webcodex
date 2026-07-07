# 概念

[English](CONCEPTS.md) | [简体中文](CONCEPTS.zh-CN.md)

这是 WebCodex onboarding 的术语地图。配合
[QUICK_START.zh-CN.md](QUICK_START.zh-CN.md) 阅读；具体命令仍以各专题文档为准。

## 心智模型

```text
GPT Actions / MCP / REST client
        |
        | HTTPS + shared key, wc_pat_* 或 wc_oat_*
        v
WebCodex server
        |
        | agent transport + wc_agent_*
        v
webcodex-agent
        |
        v
registered project directory
```

server 提供稳定 API 和认证边界。agent 反向连接 server，并在已注册的项目根目录内执行允许的工作。WebCodex 是自托管 runtime；它不是 hosted SaaS、租户隔离层、OIDC、JWKS、JWT ID token 或 userinfo。

## 核心组件

### Server

`webcodex` 是 HTTP server。它暴露 REST API、给 GPT Actions 使用的 `/openapi.json`、给 MCP client 使用的 `/mcp`，以及 agent 连接 endpoint。server 保存 runtime state 和凭据 hash，但项目执行通常路由给已连接的 agent。

### Agent

`webcodex-agent` 是执行 worker。它用 `wc_agent_*` token 和 `client_id` 连接 server，读取 `projects.d/*.toml`，执行本地 allowed roots 策略，并在已注册项目目录中处理 file、Git、patch、shell、job 和 Cargo 请求。

### Project

project 是已注册 workspace。agent-backed project id 格式是：

```text
agent:<client_id>:<project_id>
```

`<project_id>` 来自 agent `projects.d/*.toml` 文件中的顶层 `id` 字段。项目路径保留在 agent 主机上。

### Runtime tool

runtime tools 是通过 `/api/tools/call`、GPT Actions 和 MCP 暴露的类型化操作。例如 `list_projects`、`read_file`、`search_project_text`、`git_status`、`replace_line_range`、`insert_at_line`、`delete_line_range`、`apply_text_edits`、`validate_patch`、`apply_patch_checked`、`cargo_fmt`、`cargo_check`、`cargo_test`、`run_shell`、`run_job`、`show_changes`、`start_session`、`start_coding_task`、`finish_coding_task` 和 `session_handoff_summary`。

推荐的 coding workflow 是：

1. 用 `start_coding_task` 开始，并保存显式 `session_id`。
2. 用 `read_file`、`search_project_text` 和 `show_changes` 检查。
3. 用 `replace_line_range`、`insert_at_line`、`delete_line_range`、`apply_text_edits` 或 `apply_patch_checked` 编辑。
4. 用 `cargo_fmt`、`cargo_check`、`cargo_test`、`validate_patch` 或 `apply_patch_checked` 验证。
5. 用 `show_changes`、`git_diff_hunks` 和 `workspace_hygiene_check` review。
6. 用 `finish_coding_task` 收尾；多步交接时用 `session_handoff_summary`。

`run_shell` 和 `run_job` 是受限 command/job escape hatch，不是默认 validation source，也不是主要源码编辑路径。

Codex delegation（`run_codex`）当前已从模型可见 surface 隐藏/禁用：包括 GPT Actions、MCP `tools/list`、runtime tool discovery 和 generic model-facing dispatch。legacy `/api/codex/run` endpoint 默认关闭，只有设置 `WEBCODEX_ENABLE_LEGACY_CODEX_RUN=1` 才挂载；该 opt-in 不会重新启用 `run_codex`。不要把它当成推荐路径。

### Artifact transfer

artifact transfer 是受限的项目 artifact 传输基础能力，用于把二进制或外部输入文件安全地导入/导出项目上下文。它使用 project-relative path、字节上限、chunk 上限和 sha256 guard；它不是源码编辑路径，不是对象存储，不是文件管理平台，也不是大文件系统。

源码编辑仍然应优先使用 `replace_line_range`、`insert_at_line`、`delete_line_range`、`apply_text_edits` 和 `apply_patch_checked`。不要把 `save_project_artifact`、`artifact_upload_begin`、`artifact_upload_chunk`、`artifact_upload_finish` 或 `artifact_upload_abort` 当成源码写入工具。`write_project_file` 和 `replace_in_file` 这类兼容编辑工具仍可通过 `callRuntimeTool` 使用。

### GPT Actions surface

GPT Actions 使用 WebCodex OpenAPI schema：

```text
https://your-domain.example/openapi.json
```

这个 surface 故意比 admin API 小。它面向 runtime、project、file、Git、patch、shell/job、artifact 和 session 工作流。它不暴露 user 创建、PAT 创建、agent-token 创建、pairing、enrollment、setup、server management 或 audit endpoints。

GPT Actions 必须保持在 30 个 operation/tool 上限以下。当前 WebCodex OpenAPI surface 是 25 个 operations，因此 chunked artifact upload 和兼容编辑工具通过 `callRuntimeTool` 使用，而不是 promoted 为 dedicated GPT Action operations。

旧 `/api/codex/*` REST API 已进入 lifecycle-deprecated 状态，并且不暴露在 GPT Actions OpenAPI schema 中。新客户端应使用 `/api/tools/call`、`/api/projects/*` 或 MCP；`/api/codex/*` 仅保留给历史调用方和 audit 连续性。

### MCP surface

MCP client 连接：

```text
https://your-domain.example/mcp
```

MCP 和 GPT Actions 共用同一套 `ToolRuntime`、agent registry、project id、基于 metadata 的 OAuth 检查和 session recording。MCP 是远程 WebCodex runtime endpoint；外部 MCP-server brokering 是后续扩展，不是当前 endpoint 的前置条件。

runtime tools 可以作为 MCP tools 直接暴露，但仍受 tool manifest 和 client 约束。这不同于 GPT Actions：GPT Actions 的 dedicated operation surface 必须保持在 30 个 operation/tool 上限以下。

## 认证词汇

| Credential | 用途 | 不要用于 |
| --- | --- | --- |
| `WEBCODEX_TOKEN` | Server bootstrap/admin setup | GPT Actions、MCP、agents、日常 runtime 调用 |
| Shared key | Host 支持静态 Bearer/API-key auth 时的快速 agent + GPT/MCP onboarding | 生产 IAM、admin、managed-user identity |
| `wc_acct_*` | 一次性本地创建 PAT 和 agent token | GPT Actions、MCP、runtime API、agent transport |
| `wc_pat_*` | Managed runtime API、GPT Actions、MCP、REST tools | Agent transport |
| `wc_oat_*` | OAuth2 delegated runtime access | Agent transport，默认也不是 admin |
| `wc_agent_*` | 仅用于 `webcodex-agent` 连接 | GPT Actions、MCP、runtime API |

静态 Bearer/API-key Host auth 可以使用 quick start 的 shared key，也可以使用 managed mode 的 `wc_pat_*`：

```text
Authorization: Bearer <token-or-shared-key>
```

OAuth 是单独流程。OAuth client 字段留空不会变成 no-auth、shared-key fallback 或静态 Bearer auth。OAuth2 access token 仍然不能用于 agent transport endpoints。

shared-key OAuth bridge 适合 OAuth-only Host，但 operator 仍希望保持低配置 shared-key onboarding 的场景。它默认关闭，必须用 `WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=true` 显式启用。用户在 WebCodex OAuth 页面输入 shared key；WebCodex 只保存 shared-key hash，并通过 authorization-code flow 签发 OAuth token。bridge-issued token 的 scope 限制在 runtime/project/job 范围，不会获得 `admin`、`account:manage` 或 `agent:*` scope。

## Sessions、handoff 和 hints

`start_session` 创建有界的任务跟踪 session record，并返回 `wc_sess_*` id。它不会自动把这个 session bind 成 future calls 的 current session。后续 generic `/api/tools/call` 调用可以把 id 作为 `recording_session_id` recorder metadata 传入，tool-specific 调用可以传显式 `session_id` input，MCP 调用可以通过保留字段 `_session_id` 传入。session records、events 和 messages 是有界、redacted 的 task-recorder metadata；配置 session persistence 时，WebCodex 可以通过 `sessions.json` ledger 持久化并恢复它们。这个 ledger 是 durable task-continuity 和 handoff record，不是完整 audit log。

显式 `session_id` 总是优先于 current-session binding。未知的显式 session id 必须返回 `unknown_session_id`，不应静默 fallback 到另一个 session。

`bind_current_session`、`current_session` 和 `unbind_current_session` 可以把一个 project-scoped session 绑定到同一 principal、transport 和 project 后续的 project tool calls。这是 process-local in-memory 便利状态，不是 durable session ledger。不要假设它会在 process restart 后保留；需要确定性 handoff 时传显式 `session_id` 或 `recording_session_id`。

`session_handoff_summary` 是只读的结构化 handoff 工具。它汇总 session 信息、message-board state、recent progress/decisions、open todos/risks/questions/guidance、recent failed tools，以及可选的有界 workspace/checkpoint context。它必须传显式 `session_id`，不会 fallback 到 current-session binding，也不会调用 LLM。

`start_coding_task` 是推荐的 coding-loop 入口。它创建 session、返回显式 `session_id`、收集确定性的 project/runtime context，并且默认 `bind_current=false`。`finish_coding_task` 是对应的显式 `session_id` 收尾 aggregate；它可以包含 `show_changes`、workspace hygiene、handoff 和 validation summary。

`session_hint` 是 recorded tool output 上的轻量提示，表示 session 中有未解决的 guidance、question、todo 或 risk messages。它只包含计数和优先级，不包含 message text。

## Validation summaries

validation summaries 来自 session ledger events。validation-like tools 是 `cargo_fmt`、`cargo_check`、`cargo_test`、`validate_patch` 和 `apply_patch_checked`。`run_shell` 默认不会被归类为 validation。

session ledger 可以为 Cargo validation helpers 保存小型、sanitized、有界的 `validation_output_summary`，它来自已经有界的 output tails，并在持久化前过滤。`finish_coding_task` 和 `session_handoff_summary` 的 validation 输出不会暴露 raw stdout/stderr、excerpt fields 或 `validation_output_summary`。

minimal parser 只从安全有界 metadata 中提取稳定事实，例如 Cargo severity/code/span 和 test summary counts。它不会推断 root cause，不会提供 fix suggestion，不会调用 LLM，也不会使用 LSP 或 tree-sitter。

## 运行模式

Service mode 使用 systemd units 管理 server 和 agent。长期自托管 server 和稳定 agent 主机推荐使用 service mode，因为它提供重启和开机恢复能力。命令环境应通过 agent shell profiles 配置，因为 systemd 不读取 `.bashrc` 等交互式 shell 文件。

Manual/no-service mode 用前台进程或 `nohup` 这类简单后台方式运行 agent。它适合本地评估、容器、smoke test，或无法使用 systemd 的主机。它更容易观察和手动停止，但不提供和 service mode 相同的生命周期管理。

agent transport 使用 `transport = "auto"` 时，只有配置了 `[quic]` section 才会先尝试 QUIC，然后 fallback 到 WebSocket，再 fallback 到 polling。没有 `[quic]` 时，`auto` 从 WebSocket 开始。GPT Actions 和 MCP 仍然走 HTTPS；QUIC 只用于 `webcodex-agent` 连接。

## 下一步

- 第一次设置和决策树：[QUICK_START.zh-CN.md](QUICK_START.zh-CN.md)
- GPT Actions：[GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)
- MCP：[MCP.zh-CN.md](MCP.zh-CN.md)
- Deployment 和 systemd：[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)
- 认证模型：[AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)
- OAuth2 smoke test：[OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md)
- Testing：[TESTING.md](TESTING.md)、[E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md)
- Security：[../SECURITY.md](../SECURITY.md)、[AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)
