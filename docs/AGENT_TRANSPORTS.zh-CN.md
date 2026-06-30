# Agent Transports

[English](AGENT_TRANSPORTS.md) | [简体中文](AGENT_TRANSPORTS.zh-CN.md)

`webcodex-agent` 支持 QUIC、WebSocket、polling，以及 `auto` selector。
新的生产部署建议使用 `transport = "auto"`，并配置 `[quic]` section。在该模式下，agent 会优先尝试 QUIC；失败时 fallback 到 WebSocket，再 fallback 到 polling。

| Transport | Config value | 推荐用途 | Status |
| --- | --- | --- | --- |
| Auto | `auto` | 新生产 agent 的推荐默认值，前提是配置了 `[quic]`。 | recommended |
| QUIC | `quic` | strict QUIC only；不希望 fallback 时使用。 | stable |
| WebSocket | `websocket` | compatibility fallback，以及没有 UDP access 的简单部署。 | stable fallback |
| Polling | `polling` | 受限网络下的最后 fallback。 | stable fallback |

## 生产拓扑

GPT Actions 和 MCP 继续使用 HTTPS：

```text
ChatGPT / GPT Actions / MCP -> HTTPS TCP 443 -> reverse proxy -> WebCodex HTTP server
```

QUIC 是独立的 agent transport path：

```text
webcodex-agent -> QUIC UDP 8443 -> WebCodex QUIC listener
```

关键边界：

- QUIC 用于 `webcodex-agent` 连接。它不是 HTTP/3，也不会替代 GPT Actions 或 MCP 的 HTTPS endpoint。
- Nginx 等 reverse proxy 通常继续在 TCP 443 上处理 HTTPS。QUIC listener 是 WebCodex 自己的独立 UDP endpoint。
- WebSocket 和 polling 继续作为 fallback transports 支持。

## QUIC server requirements

在 WebCodex server 上启用 QUIC listener，并从 agent hosts 打开对应 UDP port。示例默认使用 UDP 8443：

```sh
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/etc/letsencrypt/live/<host>/fullchain.pem
WEBCODEX_QUIC_KEY=/etc/letsencrypt/live/<host>/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-agent/1
```

Certificate SAN 必须匹配 agent 配置的 `server_name`。可以复用 HTTPS reverse proxy 使用的 Let's Encrypt certificate，也可以使用单独 certificate。

Deployment preflight：

```sh
journalctl -u webcodex -n 100 --no-pager
ss -lunp | grep 8443
```

`runtime_status` 会暴露 non-secret `quic` object，包括 `enabled`、`listen`、`alpn`、`listener_started` 和 sanitized `last_error`。它不会暴露 cert/key paths、tokens、Authorization headers 或完整 environment。

## Agent configuration

推荐生产配置：

```toml
transport = "auto"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-agent/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

当 `[quic]` 存在时，`auto` 会先尝试 QUIC。如果 QUIC 连接失败，会尝试 WebSocket，然后 polling。

如果希望连接失败保持失败、不要自动 fallback，可以使用 strict QUIC：

```toml
transport = "quic"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-agent/1"
```

注意：

- QUIC 需要 `server_addr` 和 `server_name`。
- `server_name` 必须匹配 server certificate SAN。
- Agent token 仍保留在顶层 `token` 字段。不要把它放进 `[quic]`。
- TLS 保护 transport；agent token 仍负责 agent authentication。

## Wire protocol

单个 QUIC bidirectional stream 承载 length-prefixed JSON frames（`u32_be length || JSON bytes`），并复用现有 `AgentEnvelope`：

```text
agent -> server:  Register   { payload, auth_token }
server -> agent:  Registered { success, client, error }
server -> agent:  Request    { ...ShellAgentShellRequest }
agent -> server:  Result     { ...ShellAgentResultRequest }
agent -> server:  JobUpdate  { ...ShellAgentJobUpdateRequest }
either direction: Ping       { ts }
either direction: Pong       { ts }
```

- ALPN：`webcodex-agent/1`
- `runtime_status` / `listAgents` 报告的 transport label：`quic`、`websocket` 或 `polling`。
- QUIC agents 报告 `agent_protocol_version=quic-v1`。

QUIC 是现有 agent envelope protocol 的另一种 transport。它在 QUIC 上使用 length-prefixed JSON `AgentEnvelope` stream，目标是镜像 WebSocket agent flow，而不是引入单独的 application protocol。

当前模型是每个 agent connection 一个 bidirectional stream，frames 串行化。尚未实现 stream multiplexing。

## QUIC capabilities

使用 `quic-v1` agent 时，QUIC 支持 WebCodex tools 使用的 runtime request loop，包括：

- file read/write/list requests；
- git status/diff helpers；
- patch 和 structured line edit tools；
- project register/create operations；
- bounded shell commands；
- async shell jobs、job status 和 job logs。

## Validation

使用内置 doctor diagnostics 做可重复的 QUIC 检查。

Server listener 和 handshake check：

```sh
webcodex-cli doctor --quic --server-only \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/clients/workstation/webcodex-user-token \
  --agent-config /etc/webcodex/clients/workstation/agent.toml \
  --strict
```

Agent dispatch check：

```sh
webcodex-cli doctor --quic --agent-e2e \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/clients/workstation/webcodex-user-token \
  --agent-config /etc/webcodex/clients/workstation/agent.toml \
  --project agent:CLIENT_ID:PROJECT_ID \
  --strict
```

Server-only mode 会检查 HTTPS reachability、`runtime_status.quic`、UDP resolution、ALPN 和 certificate verification。Agent E2E mode 会确认 `transport=quic` / `agent_protocol_version=quic-v1` agent，运行 marker command，启动 async job，poll `job_status`，并读取 `job_log`。

## Fallback behavior

Strict transport values 表示只使用一个 transport：

- `transport = "quic"`：strict QUIC；失败时 reconnect/error，不降级。
- `transport = "websocket"`：只使用 WebSocket。
- `transport = "polling"`：只使用 polling。

配置了 QUIC 时，`transport = "auto"` 是推荐生产设置。它先尝试 QUIC，再 WebSocket，再 polling。如果缺少 `[quic]`，会从 WebSocket 开始。

Auto startup logs 会显示决策路径，例如：

```text
webcodex-agent transport auto: trying quic
webcodex-agent transport auto: quic failed: <reason>; trying websocket
webcodex-agent transport auto: websocket failed: <reason>; falling back to polling
webcodex-agent registered client_id=... server=... preferred_transport=auto actual_transport=websocket transport=websocket
```

`runtime_status` 和 `listAgents` 显示实际连接的 transport label，而不只是 preferred setting。

## Failure table

| Symptom | Likely cause / next step |
| --- | --- |
| doctor says QUIC disabled | Server env 未设置、service 未重启，或 running binary 太旧。 |
| `listener_started=false` | Cert/key/listen/bind/crypto 配置错误；检查 `runtime_status.quic.last_error` 和 `journalctl`。 |
| handshake timeout | UDP firewall、security group、NAT 或 cloud provider network policy 阻断。 |
| certificate verify failed | `server_name` 不匹配 certificate SAN，或 certificate chain 不受信任。 |
| ALPN/handshake failed | Server/client ALPN 不一致，或 agent 连到了错误 UDP service。 |
| no quic-v1 agent | Agent 仍在 fallback transport，`[quic]` 缺失或错误，或 agent binary 太旧。 |
| `run_shell` succeeds but `run_job`/`job_log` fails | Async job/job_update/log path 需要排查。 |

## 尚未实现

- HTTP/3 polling；
- reverse-proxy QUIC / HTTP/3 integration；
- UDP 443 defaulting；
- stream multiplexing。
