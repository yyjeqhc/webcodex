# Agent Transports

[English](AGENT_TRANSPORTS.md) | [简体中文](AGENT_TRANSPORTS.zh-CN.md)

`webcodex-agent` supports QUIC, WebSocket, polling, and an `auto` selector.
For new deployments, prefer `transport = "auto"` with a configured `[quic]`
section. In that mode the agent tries QUIC first, falls back to WebSocket, and
then falls back to polling when needed.

| Transport | Config value | Recommended use | Status |
| --- | --- | --- | --- |
| Auto | `auto` | Default for new production agents when `[quic]` is configured. | recommended |
| QUIC | `quic` | Strict QUIC only; use when fallback is not desired. | stable |
| WebSocket | `websocket` | Compatibility fallback and simple deployments without UDP access. | stable fallback |
| Polling | `polling` | Last-resort fallback for constrained networks. | stable fallback |

## Production topology

GPT Actions and MCP continue to use HTTPS:

```text
ChatGPT / GPT Actions / MCP -> HTTPS TCP 443 -> reverse proxy -> WebCodex HTTP server
```

QUIC is a separate agent transport path:

```text
webcodex-agent -> QUIC UDP 8443 -> WebCodex QUIC listener
```

Important boundaries:

- QUIC is for `webcodex-agent` connectivity. It is not HTTP/3 and does not replace the GPT Actions or MCP HTTPS endpoint.
- Reverse proxies such as Nginx usually remain on TCP 443 for HTTPS. The QUIC listener is a separate UDP endpoint owned by WebCodex.
- WebSocket and polling remain supported fallback transports.

## Server requirements for QUIC

Enable the QUIC listener on the WebCodex server and open the chosen UDP port from agent hosts.
The default examples use UDP 8443:

```sh
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/etc/letsencrypt/live/<host>/fullchain.pem
WEBCODEX_QUIC_KEY=/etc/letsencrypt/live/<host>/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-agent/1
```

The certificate SAN must match the `server_name` configured on the agent. You may reuse the same Let's Encrypt certificate used by your HTTPS reverse proxy, or use a separate certificate.

Deployment preflight:

```sh
journalctl -u webcodex -n 100 --no-pager
ss -lunp | grep 8443
```

`runtime_status` exposes a non-secret `quic` object with `enabled`, `listen`, `alpn`, `listener_started`, and sanitized `last_error`. It never exposes cert/key paths, tokens, Authorization headers, or the full environment.

## Agent configuration

Recommended production config:

```toml
transport = "auto"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-agent/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

`auto` attempts QUIC first when `[quic]` is present. If QUIC cannot connect, it tries WebSocket, then polling.

Use strict QUIC when you want connection failures to stay failures instead of falling back:

```toml
transport = "quic"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-agent/1"
```

Notes:

- `server_addr` and `server_name` are required for QUIC.
- `server_name` must match the server certificate SAN.
- The agent token stays in the top-level `token` field. Do not put it in `[quic]`.
- TLS protects the transport; the agent token still authenticates the agent.

## Wire protocol

A single QUIC bidirectional stream carries length-prefixed JSON frames (`u32_be length || JSON bytes`) reusing the existing `AgentEnvelope`:

```text
agent -> server:  Register   { payload, auth_token }
server -> agent:  Registered { success, client, error }
server -> agent:  Request    { ...ShellAgentShellRequest }
agent -> server:  Result     { ...ShellAgentResultRequest }
agent -> server:  JobUpdate  { ...ShellAgentJobUpdateRequest }
either direction: Ping       { ts }
either direction: Pong       { ts }
```

- ALPN: `webcodex-agent/1`
- Transport label reported in `runtime_status` / `listAgents`: `quic`, `websocket`, or `polling`.
- QUIC agents report `agent_protocol_version=quic-v1`.

QUIC is an alternative transport for the existing agent envelope protocol. It uses a length-prefixed JSON `AgentEnvelope` stream over QUIC and is intended to mirror the WebSocket agent flow, not introduce a separate application protocol.

The current model is one bidirectional stream per agent connection with serialized frames. Stream multiplexing is not implemented yet.

## Capabilities over QUIC

With a `quic-v1` agent, QUIC supports the runtime request loop used by WebCodex tools, including:

- file read/write/list requests,
- git status/diff helpers,
- patch and structured line edit tools,
- project register/create operations,
- bounded shell commands,
- async shell jobs, job status, and job logs.

## Validation

Use built-in doctor diagnostics for repeatable QUIC checks.

Server-side listener and handshake check:

```sh
webcodex-cli doctor --quic --server-only \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/clients/workstation/webcodex-user-token \
  --agent-config /etc/webcodex/clients/workstation/agent.toml \
  --strict
```

Agent dispatch check:

```sh
webcodex-cli doctor --quic --agent-e2e \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/clients/workstation/webcodex-user-token \
  --agent-config /etc/webcodex/clients/workstation/agent.toml \
  --project agent:CLIENT_ID:PROJECT_ID \
  --strict
```

The server-only mode checks HTTPS reachability, `runtime_status.quic`, UDP resolution, ALPN, and certificate verification. The agent E2E mode confirms a `transport=quic` / `agent_protocol_version=quic-v1` agent, runs a marker command, starts an async job, polls `job_status`, and reads `job_log`.

## Fallback behavior

Strict transport values mean exactly one transport:

- `transport = "quic"`: strict QUIC; failures reconnect/error and do not downgrade.
- `transport = "websocket"`: WebSocket only.
- `transport = "polling"`: polling only.

`transport = "auto"` is the recommended production setting when QUIC is configured. It tries QUIC first, then WebSocket, then polling. If `[quic]` is missing, it starts at WebSocket.

Auto startup logs show the decision path, for example:

```text
webcodex-agent transport auto: trying quic
webcodex-agent transport auto: quic failed: <reason>; trying websocket
webcodex-agent transport auto: websocket failed: <reason>; falling back to polling
webcodex-agent registered client_id=... server=... preferred_transport=auto actual_transport=websocket transport=websocket
```

`runtime_status` and `listAgents` show the actual connected transport label, not merely the preferred setting.

## Failure table

| Symptom | Likely cause / next step |
| --- | --- |
| doctor says QUIC disabled | Server env is not set, the service was not restarted, or the running binary is old. |
| `listener_started=false` | Cert/key/listen/bind/crypto config is wrong; check `runtime_status.quic.last_error` and `journalctl`. |
| handshake timeout | UDP firewall, security group, NAT, or cloud provider network policy is blocking traffic. |
| certificate verify failed | `server_name` does not match certificate SAN, or the certificate chain is not trusted. |
| ALPN/handshake failed | Server/client ALPN differs, or the agent connected to the wrong UDP service. |
| no quic-v1 agent | Agent is still on fallback transport, `[quic]` is missing or wrong, or the agent binary is old. |
| `run_shell` succeeds but `run_job`/`job_log` fails | Async job/job_update/log path needs debugging. |

## Still not implemented

- HTTP/3 polling,
- reverse-proxy QUIC / HTTP/3 integration,
- UDP 443 defaulting,
- stream multiplexing.
