# Agent Transports

[English](AGENT_TRANSPORTS.md) | [简体中文](AGENT_TRANSPORTS.zh-CN.md)

`webcodex-runner` 支持 QUIC、WebSocket、polling，以及 `auto` selector。
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
webcodex-runner -> QUIC UDP 8443 -> WebCodex QUIC listener
```

关键边界：

- QUIC 用于 `webcodex-runner` 连接。它不是 HTTP/3，也不会替代 GPT Actions 或 MCP 的 HTTPS endpoint。
- Nginx 等 reverse proxy 通常继续在 TCP 443 上处理 HTTPS。QUIC listener 是 WebCodex 自己的独立 UDP endpoint。
- WebSocket 和 polling 继续作为 fallback transports 支持。

## QUIC server requirements

在 WebCodex server 上启用 QUIC listener，并从 agent hosts 打开对应 UDP port。示例默认使用 UDP 8443：

```sh
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/etc/letsencrypt/live/<host>/fullchain.pem
WEBCODEX_QUIC_KEY=/etc/letsencrypt/live/<host>/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-runner/1
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
alpn = "webcodex-runner/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

当 `[quic]` 存在时，`auto` 会先尝试 QUIC。如果 QUIC 连接失败，会尝试 WebSocket，然后 polling。

WebSocket 连接会遵循代理环境变量。`wss://` 依次使用 `HTTPS_PROXY` / `https_proxy`、`ALL_PROXY` / `all_proxy`；`ws://` 依次使用 `HTTP_PROXY` / `http_proxy`、`ALL_PROXY` / `all_proxy`。`NO_PROXY` / `no_proxy` 可按 localhost、IP、hostname/domain suffix、host:port 或 `*` 绕过代理。当前 Runner 仅支持通过 HTTP `CONNECT` 使用 `http://host:port` proxy；不支持 HTTPS proxy、SOCKS 或 proxy authentication，遇到这些配置会 fail closed，不会偷偷 direct-connect。`websocket_connect_timeout_secs` 同时覆盖 proxy TCP connect、CONNECT、目标站 TLS 和 WebSocket handshake。QUIC 不使用这些 proxy 设置。

如果希望连接失败保持失败、不要自动 fallback，可以使用 strict QUIC：

```toml
transport = "quic"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-runner/1"
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

- ALPN：`webcodex-runner/1`
- `runtime_status` / `listAgents` 报告的 transport label：`quic`、`websocket` 或 `polling`。
- QUIC agents 报告 `agent_protocol_version=quic-v1`。

QUIC 是现有 agent envelope protocol 的另一种 transport。它在 QUIC 上使用 length-prefixed JSON `AgentEnvelope` stream，目标是镜像 WebSocket agent flow，而不是引入单独的 application protocol。

当前模型是每个 agent connection 一个 bidirectional stream，frames 串行化。尚未实现 stream multiplexing。

## 跨 transport 的 Job reconciliation

Runner 声明 `job_state_reconciliation` 后，polling registration、WebSocket
`Register` envelope 和 QUIC `Register` envelope 都序列化同一个
`job_inventory` 模型。重连状态机只在共享 Registry 和 Runner JobManager 中实现
一次，不为三种 transport 分别维护。

WebSocket 与 QUIC 注册还会获得内部 connection lease。同一 Runner instance
已经完成新注册后，旧 socket 的延迟清理不能再断开新连接。

完成认证和 active-instance lease 校验后，Server 会在同一个 Registry 临界区内把
完整 inventory 与 client lease 一起合并。同实例重连按单调 `update_seq` 解除
`recovering`；新实例会使旧实例活动 Job 进入 `lost`；完整 inventory 缺少已知活动
Job 时，该 Job 也会进入 `lost`。Malformed、重复、不一致或超限 inventory 会在
Registry mutation 前被拒绝。

短暂 WebSocket、QUIC 或 polling session 中断期间，即使 best-effort JobUpdate
发送失败，Runner 仍继续更新进程内 record。注册成功并安装新 sink 后，它会重放
最新的有界权威 snapshot；相同序号重放幂等。完成 reconciliation 后，
`job_status`、有界日志以及 `stop_job(original_job_id)` 会继续工作。Job 仍处于
`recovering` 时，`stop_job` 返回 `runner_unavailable_recovering`，不会伪造成功。

新的 `agent_instance_id` 不会迁移或继承旧实例的 Job；旧实例活动 Job 在注册时
以 `runner_instance_replaced` 终结为 `lost`。旧实例迟到 disconnect 相对当前
实例是 no-op —— 不清除当前 notifier，也不影响当前实例的 Job —— 也不改变旧实例
已 terminal 的 `lost` Job（首次 `ended_at`/reason 保留）。

跨所有 transport，`recovering` 都是有界的。进程内 sweep（不依赖请求流量）会将
grace window 已到期的 `recovering` Job 转为 `lost`（`runner_recovery_deadline_exceeded`），
不创建 per-job task，锁内不做磁盘/网络操作。deadline 不会被 stale connection 的
Ping/Pong/metadata、重复 disconnect 或迟到 inventory 延长，且是单个 Server 进程的属性
（不跨 Server 重启持久化）。不声明 `job_state_reconciliation` 的旧 Runner 保持立即 `lost`
断线语义（`legacy_runner_disconnected`），永不进入 `recovering`。

非法的 structured validation progress update 属于 executor protocol violation：
Job 进入 terminal `failed`，错误为有界、稳定的 `validation_progress_invalid` 类，
保留最后一次已接受的合法 progress，`ended_at` 只设置一次，释放 pending request，
不重新执行；已 terminal 的 Job 不会被迟到 update 或 register inventory 复活。

该连续性只适用于 Runner 进程和 `agent_instance_id` 均未变化的情况。Runner
进程重启无法重新获得旧 child handle。本机制不是 command exactly-once 或
`run_job` request idempotency 保证。

## QUIC capabilities

使用 `quic-v1` agent 时，QUIC 支持 WebCodex tools 使用的 runtime request loop，包括：

- file read/write/list requests；
- git status/diff helpers；
- patch 和 structured line edit tools；
- project register/create operations；
- bounded shell commands；
- async shell jobs、job status 和 job logs。

## Validation

使用 operator status projection 做有界 QUIC 检查：

```sh
webcodex ops status \
  --server-url https://your-domain.example \
  --token-file /etc/webcodex/clients/workstation/webcodex-user-token \
  --strict
webcodex ops agents \
  --server-url https://your-domain.example \
  --token-file /etc/webcodex/clients/workstation/webcodex-user-token \
  --strict
```

这些 projection 确认 server reachability、listener state 和协商后的 Agent
transport/protocol，不让 transport discovery 进入普通 coding path。
certificate/UDP handshake validation 仍是 ingress/transport operator 的 deploy gate。

## Fallback behavior

Strict transport values 表示只使用一个 transport：

- `transport = "quic"`：strict QUIC；失败时 reconnect/error，不降级。
- `transport = "websocket"`：只使用 WebSocket。
- `transport = "polling"`：只使用 polling。

配置了 QUIC 时，`transport = "auto"` 是推荐生产设置。它先尝试 QUIC，再 WebSocket，再 polling。如果缺少 `[quic]`，会从 WebSocket 开始。WebSocket 已经建立后，普通 socket close、EOF、read error 会被当作 transport disconnect，agent 会重连而不是退出进程。

Auto startup logs 会显示决策路径，例如：

```text
webcodex-runner transport auto: trying quic
webcodex-runner transport auto: quic failed: <reason>; trying websocket
webcodex-runner transport auto: websocket failed: <reason>; falling back to polling
webcodex-runner registered client_id=... server=... preferred_transport=auto actual_transport=websocket transport=websocket
webcodex-runner websocket connection closed; reconnecting
webcodex-runner reconnect attempt scheduled transport=websocket delay=1s
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
