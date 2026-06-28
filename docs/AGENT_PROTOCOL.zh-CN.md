# Agent Protocol

[English](AGENT_PROTOCOL.md) | [简体中文](AGENT_PROTOCOL.zh-CN.md)

WebCodex agents 连接 server，并执行已注册项目上的 tools。新部署建议配置 QUIC 并使用 `transport = "auto"`；WebSocket 和 polling 继续作为 fallback transports。

## Authentication

Agents 应使用 client enrollment 期间创建的 agent tokens：

```bash
webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id CLIENT_ID
```

Server/admin 侧用 `webcodex-cli pairing create` 创建临时代码。Agent token 在 client enroll 期间返回给 client，并写入生成的 `agent.toml`；不要从 server 复制 agent token files。二进制部署时，使用 `webcodex-cli agent install-service` 安装 client-side service，并用 `webcodex-cli agent status` 检查。

Transport auth rules：

- QUIC：agent token 保留在顶层 agent config 中，并通过 QUIC stream 内的 agent registration envelope 发送。
- WebSocket：优先在 handshake headers 中使用 `Authorization: Bearer <agent-token>`。
- WebSocket compatibility：`/api/agents/ws?token=...` 只用于 handshake 兼容。
- Polling：每个 request 都必须使用 `Authorization: Bearer <agent-token>`。
- REST、MCP 和 GPT Actions ordinary APIs 必须使用 `Authorization: Bearer ...`。

不要在 `/api/agents/ws` 之外使用 query-string tokens。

## Registration and identity

Agents 注册时提交：

- `client_id`
- `owner`
- `transport`
- `agent_instance_id`
- capabilities
- registered projects
- redacted policy summary

`agent_instance_id` 标识一个正在运行的 agent instance，区别于稳定的 `client_id`。

## Policy summary

`runtime_status` 和 `listAgents` 为 operators 暴露 redacted summary：

- `allow_raw_shell`
- `allow_cwd_anywhere`
- `allowed_roots`
- `max_timeout_secs`
- `max_output_bytes`

它们不会暴露 tokens、full env、`Authorization` headers、完整 `agent.toml` 或 shell `init_script` values。

Policy 默认值：

- 如果 `allowed_roots` 缺失或为空，默认使用 `$HOME`。
- 显式 `allowed_roots` 会替换 `$HOME` 默认值。

## Project ids

Agent-backed project ids 报告为：

```text
agent:<client_id>:<project_id>
```

Server 会把 project tool calls 路由到拥有该项目的 connected agent。

## Optional Codex jobs

Codex jobs 是 project tool path，不是 agent lifecycle mechanism。`runCodexTask` 需要 agent host 上已有 Codex CLI，并且不会启动新的 `webcodex-agent`。
