# QUIC E2E Validation

This guide validates the experimental custom QUIC agent transport. It is not
HTTP/3, does not involve Nginx, and is opt-in only.

## Server Test Config

Keep the normal HTTPS/API path unchanged. Add a separate UDP listener for the
agent transport:

```sh
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/path/to/fullchain.pem
WEBCODEX_QUIC_KEY=/path/to/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-agent/1
```

QUIC is still disabled unless `WEBCODEX_QUIC_ENABLED=true`. Do not use UDP 443
as the default in this phase.

Open firewall/security-group access for UDP 8443 from the agent host. TCP 443,
Nginx, and GPT Actions remain unchanged.

Check server logs for:

```text
Agent QUIC listener (experimental) on UDP ... with ALPN webcodex-agent/1
```

If startup fails, the server continues serving HTTP/WebSocket/polling and logs
a config or bind error.

Deployment preflight:

```sh
journalctl -u webcodex -n 100 --no-pager
ss -lunp | grep 8443
```

Confirm the systemd unit or EnvironmentFile contains the `WEBCODEX_QUIC_*`
settings, then restart the WebCodex server. The `runtime_status` tool reports a
non-secret `quic` object:

```json
{
  "enabled": true,
  "listen": "0.0.0.0:8443",
  "alpn": "webcodex-agent/1",
  "listener_started": true,
  "last_error": null
}
```

It does not expose cert/key paths, tokens, Authorization headers, or the full
environment.

## Agent Config

Set only the test agent to QUIC:

```toml
transport = "quic"

[quic]
server_addr = "v4.example.com:8443"
server_name = "v4.example.com"
alpn = "webcodex-agent/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

`server_name` must match the certificate SAN. The agent token stays in the
top-level token field; do not put it in `[quic]`.

## Server-Only Harness

This checks the HTTPS API when `--server-url` is provided, parses local
`agent.toml` if provided, resolves the UDP address, then performs a real QUIC
TLS handshake with the configured ALPN and certificate verification.

```sh
webcodex-cli doctor --quic --server-only \
  --server-url https://v4.example.com \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-config /etc/webcodex/agent.toml \
  --strict
```

Equivalent without an agent config:

```sh
webcodex-cli doctor --quic --server-only \
  --server-url https://v4.example.com \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --quic-server-addr v4.example.com:8443 \
  --quic-server-name v4.example.com \
  --quic-alpn webcodex-agent/1 \
  --strict
```

Expected PASS checks include `runtime status`, `quic resolve`, and
`quic runtime config`, `quic resolve`, and `quic handshake`. A handshake pass
means UDP reached the listener, ALPN matched, and rustls verified the
certificate chain/SAN for `server_name`.

## Agent E2E Harness

Start a real agent first:

```sh
webcodex-agent --config /etc/webcodex/agent.toml
```

Then run the dispatch E2E check against a project id visible in
`runtime_status`/`listProjects`:

```sh
webcodex-cli doctor --quic --agent-e2e \
  --server-url https://v4.example.com \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-config /etc/webcodex/agent.toml \
  --project agent:CLIENT_ID:PROJECT_ID \
  --strict
```

The E2E path verifies:

- `runtime_status` is reachable.
- A matching agent is visible with `transport=quic` and
  `agent_protocol_version=quic-v2`.
- Capabilities, `pending_requests`, `connected/status`, and `last_seen` are
  observable.
- `run_shell` returns `webcodex-quic-ok`.
- `run_job` starts an async job.
- `job_status` reaches `completed`.
- `job_log` returns `webcodex-quic-job-ok`.

## Disconnect/Reconcile Check

Stop the foreground agent or its systemd unit:

```sh
sudo systemctl stop webcodex-agent
```

Then observe status:

```sh
webcodex-cli doctor --server-url https://v4.example.com \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-config /etc/webcodex/agent.toml
```

or call `runtime_status` through your existing API tooling. The agent should
become stale/offline according to the normal registry window, and jobs tied to
the disconnected transport should reconcile as lost where applicable.

## Manual WebSocket Fallback

The default remains WebSocket. Strict QUIC stays strict: edit `agent.toml` to
return a strict QUIC agent to WebSocket:

```toml
transport = "websocket"
```

Restart the agent and verify:

```sh
webcodex-cli doctor --server-url https://v4.example.com \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-config /etc/webcodex/agent.toml \
  --project agent:CLIENT_ID:PROJECT_ID \
  --strict
```

For explicit automatic fallback, set:

```toml
transport = "auto"

[quic]
server_addr = "v4.example.com:8443"
server_name = "v4.example.com"
alpn = "webcodex-agent/1"
```

`auto` tries QUIC first when `[quic]` exists, then WebSocket, then polling. If
`[quic]` is missing, it skips QUIC and starts at WebSocket. `runtime_status` and
`listAgents` show the actual connected transport (`quic`, `websocket`, or
`polling`).

## Failure Hints

- doctor says QUIC disabled: server env is not set, the service was not
  restarted, or the running binary is old.
- `listener_started=false`: cert/key/listen/bind/crypto config is wrong; check
  `runtime_status.quic.last_error` and `journalctl`.
- `quic resolve` fails: fix DNS or use an explicit `host:port`.
- `handshake timeout` / `connect timeout`: check UDP 8443 firewall/security
  group/NAT/cloud network policy and listener startup.
- `certificate verify failed`: check `server_name`, certificate SAN, and issuer
  trust chain.
- `ALPN/handshake failed`: check server/client ALPN and ensure the address is
  the WebCodex QUIC UDP service.
- `register rejected by server`: check the agent token and client_id/owner
  binding.
- `agent-e2e no quic-v2 agent`: confirm the running agent uses
  `transport="quic"` or explicit `transport="auto"` with QUIC success, and was
  built from the dispatch-capable agent.
- `run_shell` succeeds but `run_job`/`job_log` fails: debug the async
  job/job_update/log path.
