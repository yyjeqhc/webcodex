# Agent Transports

`webcodex-agent` can connect to the server over three transports:

| Transport   | Config value     | Default | Status        |
| ----------- | ---------------- | ------- | ------------- |
| WebSocket   | `websocket`      | yes     | stable        |
| Polling     | `polling`        | no      | stable (fallback) |
| QUIC        | `quic`           | no      | **experimental** (Phase 5B) |

## QUIC (experimental custom transport)

> **This is a custom QUIC *stream* transport, NOT HTTP/3.**

### What changes vs. the existing setup

The production topology is unchanged for GPT Actions:

```
ChatGPT / GPT Actions  -> HTTPS TCP 443 -> Nginx -> WebCodex server HTTP 8080
```

QUIC is a **separate, parallel** path used only by the agent:

```
webcodex-agent  -> QUIC UDP 8443 -> WebCodex server quinn endpoint
```

- **Nginx is NOT involved in QUIC.** Nginx still terminates HTTPS on TCP 443 and
  proxies to the server on TCP 8080 as before. Do not enable QUIC/HTTP/3 in
  Nginx for this.
- **The HTTP server still listens on its original TCP port** (default 8080).
  WebSocket and polling agents keep working unchanged.
- **TCP 443 HTTPS / GPT Actions are completely unaffected.**

### Server requirements

- Open **UDP 8443** to the server (the agent dials this directly). UDP 443 is
  **not** used in this phase.
- Provide a TLS **certificate and key** on the server. These are read from paths
  you configure (env vars below); they are **not** hardcoded to a production
  Let's Encrypt path. You can reuse the same Let's Encrypt cert/key that Nginx
  uses, or a separate cert — the cert's SAN must match the `server_name` the
  agent uses.
- Enable the listener (it is **off by default**).

Server env vars:

```sh
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/etc/letsencrypt/live/<host>/fullchain.pem
WEBCODEX_QUIC_KEY=/etc/letsencrypt/live/<host>/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-agent/1
```

If `WEBCODEX_QUIC_ENABLED` is unset/false, the QUIC listener never starts and
behavior is identical to before.

QUIC is an optional experimental listener. If `WEBCODEX_QUIC_ENABLED=true` but
the cert/key/listen config is invalid, the server logs `QUIC listener disabled
due to config error` and continues serving HTTP/WebSocket/polling. Check the
server logs for `Agent QUIC listener (experimental) on UDP ...` before assuming
QUIC is actually accepting connections.

### Agent requirements

Set `transport = "quic"` in `agent.toml` and add a `[quic]` section:

```toml
transport = "quic"

[quic]
server_addr = "v4.example.com:8443"
server_name = "v4.example.com"
alpn = "webcodex-agent/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

- `server_addr` / `server_name` are **required** when `transport = "quic"`.
- `server_name` must match the server certificate's SAN (TLS verification).
- The agent token is **not** stored in the `[quic]` section; it stays in the
  top-level `token` field and is carried inside the `Register` envelope. TLS is
  transport security only — the agent token still authenticates the agent, exactly
  like the WebSocket/polling paths.

### Wire protocol

A single QUIC bidirectional stream carries length-prefixed JSON frames
(`u32_be length || JSON bytes`) reusing the existing `AgentEnvelope`:

```
agent -> server:  Register   { payload, auth_token }
server -> agent:  Registered { success, client, error }
server -> agent:  Request    { ...ShellAgentShellRequest }
agent -> server:  Result     { ...ShellAgentResultRequest }
agent -> server:  JobUpdate  { ...ShellAgentJobUpdateRequest }
either direction: Ping       { ts }
either direction: Pong       { ts }
```

- ALPN: `webcodex-agent/1`
- Transport label reported in `runtime_status` / `listAgents`: `quic`
- Agent protocol versions:
  - `quic-v1`: Phase 5A register/ack/ping/pong only. Server downgrades
    capabilities to false and rejects runtime enqueue with the explicit 5A
    dispatch error so work never silently sits in a dead queue.
  - `quic-v2`: Phase 5B dispatch-capable QUIC. Server keeps the agent's real
    capabilities and can deliver requests over the QUIC stream.

The current model is **one bidirectional stream** per agent connection with
serialized frames. Stream multiplexing is not implemented yet.

### Phase 5B capabilities

With a `quic-v2` agent, QUIC can execute the basic agent transport loop:

- `run_shell`
- file read/write/list requests
- agent-side project register/create operations
- async shell job start/status/log/update basics
- stop job delivery through the same queue, when the active agent connection is
  still present

`quic-v1` agents remain visible as online QUIC agents, but are intentionally
register-only and must be upgraded to `quic-v2` before they can execute runtime
requests.

### Fallback

Fallback is **manual** for now. To revert an agent to a working transport, set
`transport` back to `websocket` (or `polling`) in `agent.toml` and restart the
agent. WebSocket and polling remain fully supported and are unaffected by the
QUIC listener.

Production guidance: keep WebSocket as the default transport and retain a
manual WebSocket/polling fallback path for agents. Do not switch fleets to QUIC
by default until later phases add more operational validation.

### Still not implemented

- auto fallback: `quic -> websocket -> polling`,
- HTTP/3 polling,
- Nginx QUIC / HTTP/3 integration,
- UDP 443 defaulting,
- stream multiplexing,
- `doctor` QUIC connectivity check,
- deployment validation on UDP 8443.
