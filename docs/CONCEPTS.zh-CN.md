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

runtime tools 是通过 `/api/tools/call`、GPT Actions 和 MCP 暴露的类型化操作。例如 `list_projects`、`read_file`、`git_status`、`replace_line_range`、`validate_patch`、`apply_patch_checked`、`run_shell`、`run_job`、`show_changes`、`start_session` 和 `session_handoff_summary`。

推荐的模型工作流是先检查，再在已知行号时使用结构化编辑工具；应用 patch 前先 validate；运行受限 shell/job 检查；最后用 `show_changes` 和 session tools 汇总。

Codex delegation（`run_codex`）当前已从模型可见 surface 隐藏/禁用：包括 GPT Actions、MCP `tools/list`、runtime tool discovery 和 generic model-facing dispatch。不要把它当成推荐路径。

### GPT Actions surface

GPT Actions 使用 WebCodex OpenAPI schema：

```text
https://your-domain.example/openapi.json
```

这个 surface 故意比 admin API 小。它面向 runtime、project、file、Git、patch、shell/job、artifact 和 session 工作流。它不暴露 user 创建、PAT 创建、agent-token 创建、pairing、enrollment、setup、server management 或 audit endpoints。

### MCP surface

MCP client 连接：

```text
https://your-domain.example/mcp
```

MCP 和 GPT Actions 共用同一套 `ToolRuntime`、agent registry、project id、基于 metadata 的 OAuth 检查和 session recording。MCP 是远程 WebCodex runtime endpoint；外部 MCP-server brokering 是后续扩展，不是当前 endpoint 的前置条件。

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

`start_session` 创建内存中的任务跟踪 session，并返回 `wc_sess_*` id。后续 REST 调用可以把它作为 tool metadata 传入，MCP 调用可以通过保留字段 `_session_id` 传入。session recording 是有界、redacted 的；它不是完整 audit log，server 重启后会丢失。

显式 `session_id` 总是优先于 current-session binding。未知的显式 session id 必须返回 `unknown_session_id`，不应静默 fallback 到另一个 session。

`bind_current_session`、`current_session` 和 `unbind_current_session` 可以把一个 project-scoped session 绑定到同一 principal、transport 和 project 后续的 project tool calls。这是便利状态，不是持久身份。

`session_handoff_summary` 是只读的结构化 handoff 工具。它汇总 session 信息、message-board state、recent progress/decisions、open todos/risks/questions/guidance、recent failed tools，以及可选的有界 workspace/checkpoint context。它不会调用 LLM。

`session_hint` 是 recorded tool output 上的轻量提示，表示 session 中有未解决的 guidance、question、todo 或 risk messages。它只包含计数和优先级，不包含 message text。

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
- Testing 和 CI lanes：[TESTING.md](TESTING.md)、[CI_LANES.md](CI_LANES.md)
- Security：[../SECURITY.md](../SECURITY.md)、[AUTH_ARCHITECTURE.md](AUTH_ARCHITECTURE.md)
