# Agent Transports

[English](AGENT_TRANSPORTS.md) | [简体中文](AGENT_TRANSPORTS.zh-CN.md)

`webcodex-runner` supports QUIC, WebSocket, polling, and an `auto` selector.
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
webcodex-runner -> QUIC UDP 8443 -> WebCodex QUIC listener
```

Important boundaries:

- QUIC is for `webcodex-runner` connectivity. It is not HTTP/3 and does not replace the GPT Actions or MCP HTTPS endpoint.
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
WEBCODEX_QUIC_ALPN=webcodex-runner/1
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
websocket_connect_timeout_secs = 5

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-runner/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

`auto` attempts QUIC first when `[quic]` is present. If QUIC cannot connect, it tries WebSocket, then polling. When `[quic]` is absent, the agent logs that QUIC is not configured and starts with WebSocket. `websocket_connect_timeout_secs` bounds only the WebSocket connect attempt; timeout fallback to polling is normal in networks that block WebSocket.

WebSocket connections honor proxy environment variables. `wss://` uses `HTTPS_PROXY` / `https_proxy` and then `ALL_PROXY` / `all_proxy`; `ws://` uses `HTTP_PROXY` / `http_proxy` and then `ALL_PROXY` / `all_proxy`. `NO_PROXY` / `no_proxy` bypasses the proxy for matching localhost, IP, hostname/domain suffix, host:port, or `*` entries. The current Runner supports `http://host:port` proxies through HTTP `CONNECT`; HTTPS proxies, SOCKS, and proxy authentication are not supported and fail closed rather than silently connecting directly. The WebSocket connect timeout covers proxy TCP connect, CONNECT, target TLS, and the WebSocket handshake. QUIC does not use these proxy settings.

Use strict QUIC when you want connection failures to stay failures instead of falling back:

```toml
transport = "quic"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-runner/1"
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

- ALPN: `webcodex-runner/1`
- Transport label reported in `runtime_status` / `listAgents`: `quic`, `websocket`, or `polling`.
- QUIC agents report `agent_protocol_version=quic-v1`.

QUIC is an alternative transport for the existing agent envelope protocol. It uses a length-prefixed JSON `AgentEnvelope` stream over QUIC and is intended to mirror the WebSocket agent flow, not introduce a separate application protocol.

The current model is one bidirectional stream per agent connection with serialized frames. Stream multiplexing is not implemented yet.

## Job reconciliation across transports

Polling registration, the WebSocket `Register` envelope, and the QUIC
`Register` envelope all serialize the same `job_inventory` model when the
runner advertises `job_state_reconciliation`. Reconnect state is implemented
once in the shared registry and runner JobManager; transports do not have
separate job state machines.

WebSocket and QUIC registrations also receive an internal connection lease.
This fences delayed cleanup from an older socket after the same runner instance
has already completed a newer registration.

After authentication and active-instance lease validation, registration merges
the complete inventory atomically with the client lease. A same-instance
reconnect resolves `recovering` jobs by monotonic `update_seq`. A new instance
loses the old instance's active jobs, and a complete inventory missing a known
active job loses that job. A malformed, duplicate, inconsistent, or oversized
inventory is rejected before registry mutation.

During a short WebSocket, QUIC, or polling-session interruption, the runner
continues updating its in-memory records even when best-effort JobUpdate
delivery fails. After registration succeeds and the new sink is installed, it
replays the latest bounded authoritative snapshots; equal sequences are
idempotent. `job_status`, bounded logs, and `stop_job(original_job_id)` resume
after reconciliation. While the job is still `recovering`, `stop_job` reports
`runner_unavailable_recovering` rather than fabricating success.

A new `agent_instance_id` does not migrate or inherit the old instance's
jobs; they are terminated to `lost` (`runner_instance_replaced`) at
registration. A delayed disconnect from the already-replaced instance is a
no-op with respect to the current instance — it does not clear the current
notifier or affect the current instance's jobs — and it does not change the
old instance's already-terminal `lost` job (first `ended_at`/reason retained).

`recovering` is bounded across all transports. A non-request-triggered
in-process sweep transitions a `recovering` job whose grace window has elapsed
to `lost` (`runner_recovery_deadline_exceeded`) regardless of transport, with
no per-job task and no disk/network work under the registry lock. The deadline
is not extended by stale-connection Ping/Pong/metadata, repeated disconnect,
or late inventory, and is a per-Server-process property (not persisted across a
Server restart). Older runners without `job_state_reconciliation` keep the
immediate-`lost` disconnect behavior (`legacy_runner_disconnected`) and never
enter `recovering`.

A malformed structured validation progress update is an executor protocol
violation: it moves the job to terminal `failed` with a bounded, stable
`validation_progress_invalid`-class error, retains the last accepted valid
progress, sets `ended_at` once, releases the pending request, and never
re-executes; an already terminal job is not revived by a late update or by
register inventory.

This continuity applies only while the runner process and its
`agent_instance_id` survive. A runner process restart cannot reacquire old
child handles. It is not a command exactly-once or `run_job` request
idempotency guarantee.

## Capabilities over QUIC

With a `quic-v1` agent, QUIC supports the runtime request loop used by WebCodex tools, including:

- file read/write/list requests,
- git status/diff helpers,
- patch and structured line edit tools,
- project register/create operations,
- bounded shell commands,
- async shell jobs, job status, and job logs.

## Validation

Use operator status projections for bounded QUIC checks:

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

The projections confirm server reachability, listener state, and the negotiated
agent transport/protocol without making transport discovery part of the
ordinary coding path. Certificate/UDP handshake validation remains a deploy
gate owned by the ingress/transport operator.

## Fallback behavior

Strict transport values mean exactly one transport:

- `transport = "quic"`: strict QUIC; failures reconnect/error and do not downgrade.
- `transport = "websocket"`: WebSocket only.
- `transport = "polling"`: polling only.

`transport = "auto"` is the recommended production setting when QUIC is configured. It tries QUIC first, then WebSocket, then polling. If `[quic]` is missing, it logs that QUIC is not configured and starts at WebSocket. After a WebSocket connection has been established, ordinary socket close/EOF/read errors are treated as transport disconnects and the agent reconnects instead of exiting.

Auto startup logs show the decision path, for example:

```text
webcodex-runner transport auto: quic trying
webcodex-runner transport auto: quic unavailable: <reason>; trying websocket
webcodex-runner transport auto: websocket trying
webcodex-runner transport auto: websocket failed: <reason>; falling back to polling
webcodex-runner transport auto: polling trying
webcodex-runner registered client_id=... server=https://your-domain.example preferred_transport=auto actual_transport=polling projects=11
```

The registered line prints the final `actual_transport` and a server label with only scheme, host, and port. It does not print tokens, headers, query strings, or the full agent config.

`runtime_status` and `listAgents` show the actual connected transport label, not merely the preferred setting. Both include compact agent health fields for quick checks: online/stale/offline counts plus per-client `client_id`, `status`, `transport`, `last_seen_age_secs`, `projects_count`, `pending_requests`, and `active_jobs`.

Projects are registered by agents. Use `runtime_status.projects.effective` and
`listProjects` to confirm that online agent projects are visible to the runtime.

### Foreground polling failures

When `webcodex-runner` is run in the foreground with polling as the active
transport, server-unavailable poll failures are terminal. HTTP 502, 503, and
504 responses from `/api/shell/agent/poll`, including proxy HTML error pages,
cause the agent to print a concise status summary and exit with a non-zero
status instead of retrying forever. The proxy HTML body is not dumped to the
terminal.

HTTP 401 and 403 poll responses are also terminal; check the agent token and
config without printing token values. Service deployments should rely on the
service manager restart policy, such as systemd `Restart=...`, to bring the
agent back after the server or upstream is restored.

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
