# Deployment

This document covers deploying WebCodex Runtime from "runs locally" to "a
real ChatGPT can talk to it": server environment, agent configuration, reverse
proxy / HTTPS, ChatGPT GPT Actions import, MCP endpoint, smoke tests, and
troubleshooting.

The server is a **zero-project-config relay**. It authenticates callers and
agents, receives agent registrations, routes GPT Actions / MCP tool calls to the
correct registered agent, and records audit + runtime status. The server does
**not** need a server-side `projects.toml` as a runtime project source. Project
ids, paths, policies, and capabilities are registered by agents and exposed as
runtime ids of the form `agent:<client_id>:<project_id>`.

QUIC is a future transport and is **not** implemented in this phase.

## Components

```text
ChatGPT GPT Action      ChatGPT MCP client
        |                       |
        v                       v
   /openapi.json              /mcp            (public, behind reverse proxy + TLS)
        \                       /
         v                     v
              webcodex server             (WEBCODEX_ADDR, Bearer auth)
                      |
                      v
            GET /api/agents/ws (WebSocket, preferred)
            POST /api/shell/agent/* (polling, fallback)
                      |
                      v
            webcodex-agent (one or more)
                      |
                      v
            local project working tree + Codex CLI
```

Both transports feed one `ShellClientRegistry`, one per-client request queue,
one job state store, and one `ToolRuntime`. There is no second business-logic
path for WebSocket.

## 1. Server

### Build

```bash
cargo build --release
# Binaries:
#   target/release/webcodex        (server)
#   target/release/webcodex-agent  (agent)
```

Install them (example layout used by the systemd samples):

```bash
sudo install -d /opt/webcodex
sudo install -m 0755 target/release/webcodex        /opt/webcodex/
sudo install -m 0755 target/release/webcodex-agent  /opt/webcodex/
```

### Environment variables

| Variable | Default | Required | Description |
|----------|---------|----------|-------------|
| `WEBCODEX_TOKEN` | _(unset)_ | **Yes (production)** | Server bootstrap/admin token. Use it only for first setup and admin CLI calls, not as the day-to-day GPT Actions/MCP/agent credential. When unset the server runs in **development mode without authentication** — never do this in production. |
| `WEBCODEX_ADDR` | `0.0.0.0:8080` | No | Bind address for the HTTP server. Behind a reverse proxy, bind to `127.0.0.1:8080` and let the proxy terminate TLS. |
| `WEBCODEX_DATA` | `./data` | No | Runtime data directory: SQLite DB (`webcodex.db`), uploads, job metadata (`.codex/jobs/`). Use a persistent, backed-up path in production. |
| `WEBCODEX_PUBLIC_URL` | `http://localhost:8080` | **Yes (production)** | Public base URL used as `servers[0].url` in `/openapi.json`. Set to the externally reachable HTTPS URL (e.g. `https://webcodex.example.com`) so ChatGPT imports actions against the right host. |
| `WEBCODEX_ENV_FILE` | _(unset)_ | No | Optional path to an env file loaded at startup (KEY=value lines, `#` comments, optional `export ` prefix). If unset, the server also auto-loads `./webcodex.env`, `/opt/webcodex/webcodex.env`, and `/etc/webcodex/webcodex.env` if present. |
| `WEBCODEX_ENABLE_SSH` | `false` | No | Reserved SSH executor toggle. Not used by the zero-config runtime; leave unset. |
| `CODEX_BIN` | `codex` | No | Codex CLI binary name or path. Must be installed and on `PATH` on the **agent** host (the server only forwards requests; the agent runs Codex). **Codex is optional**: when not installed, the runtime still serves `read_file`, `git_status`, `git_diff`, `apply_patch`, and `run_shell` through the agent. Only `run_codex` requires the Codex CLI. |
| `CODEX_APPROVAL_MODE` | _(empty/disabled)_ | No | Approval mode passed via `--approval-mode`. Empty/blank/`none`/`off`/`disabled` omit the flag entirely — use this if the installed Codex CLI does not support `--approval-mode`. Other values (e.g. `full-auto`, `suggest`) enable it. A request `approval_mode` overrides this per call. |
| `CODEX_DEFAULT_TIMEOUT_SECS` | `3600` | No | Default job timeout when a request omits `timeout_secs`. |
| `CODEX_MAX_PROMPT_BYTES` | `100000` | No | Maximum prompt size in bytes. Larger prompts are rejected. |
| `CODEX_ALLOWED_EXTRA_ARGS` | _(empty)_ | No | Comma-separated allowlist of accepted Codex `extra_args`. Empty (default) means no extra args are allowed. |

> Codex runs on the **agent** host, not the server. `CODEX_*` env vars are read
> by the agent process that actually spawns Codex. The server validates prompts
> and forwards `run_codex` to the owning agent. Codex is an optional advanced
> capability, not a runtime dependency.

### Minimal production server invocation

```bash
WEBCODEX_TOKEN="<long-random-secret>" \
WEBCODEX_ADDR="127.0.0.1:8080" \
WEBCODEX_DATA="/var/lib/webcodex" \
WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
RUST_LOG="info" \
/opt/webcodex/webcodex
```

The server **does not** read a `projects.toml` for runtime project discovery. Do
not set `PROJECTS_CONFIG` expecting it to register projects on the runtime
surface — projects come from agent registration.

### systemd

See [`deploy/webcodex.service.example`](../deploy/webcodex.service.example)
and [`deploy/webcodex.env.example`](../deploy/webcodex.env.example). Copy
the service file to `/etc/systemd/system/webcodex.service`, copy the env
file to `/etc/webcodex/webcodex.env`, fill in `WEBCODEX_TOKEN` and
`WEBCODEX_PUBLIC_URL`, then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
sudo systemctl status webcodex
sudo journalctl -u webcodex -f
```

## 2. Agent

The agent owns local machine knowledge: project id, project path, project
policy, capabilities, and transport. It registers these with the server at
startup.

### Agent configuration

Full field reference (TOML, loaded via `--config <path>`):

| Field | Required | Description |
|-------|----------|-------------|
| `server_url` | **Yes** | Server base URL (e.g. `https://webcodex.example.com`). Must match the server's public URL for TLS to validate. |
| `token` | **Yes** | Bearer token for the agent. Prefer a Phase 3 `wc_agent_...` agent token bound to this `client_id`; bootstrap auth is for setup/admin only. Sent in the `Authorization: Bearer <token>` header, including the WebSocket handshake. |
| `client_id` | **Yes** | Stable unique id for this agent host (e.g. `workstation-1`). Used in runtime ids `agent:<client_id>:<project_id>`. |
| `owner` | No | Owner principal. A bootstrap `WEBCODEX_TOKEN` may register any `owner`; an agent token fills or verifies `owner == <username>` and may only use its bound `client_id`. |
| `transport` | No | `"websocket"` (preferred) or `"polling"` (fallback). Omitting it defaults to `"polling"`. **Prefer `"websocket"`** for deployments. |
| `projects_dir` | No | Directory of agent-side project files (one `*.toml` per project). Defaults to `~/.config/webcodex/projects.d`. |
| `poll_interval_ms` | No | Polling interval (only used by the polling transport). Default `1000`. |
| `display_name` | No | Human label shown in `list_agents` / `runtime_status`. |
| `hostname` | No | Override hostname reported during registration. |
| `max_concurrent_jobs` | No | Cap on concurrent in-flight jobs. Default applies a sane built-in cap. |
| `[capabilities]` | No | Capability flags (`shell`, `file_read`, `file_write`, `git`, `jobs`, `async_jobs`, `async_shell_jobs`). Enforced by `ToolRuntime::authorize_agent_tool` before any agent-backed tool runs. |
| `[policy]` | No | Local execution policy: `allow_raw_shell`, `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`, `max_output_bytes`. |

### Agent config example (WebSocket preferred)

```toml
server_url = "https://webcodex.example.com"
token = "REPLACE_WITH_WC_AGENT_TOKEN"
client_id = "workstation-1"
display_name = "Workstation"
owner = "you"
transport = "websocket"
poll_interval_ms = 1000
projects_dir = "/etc/webcodex/projects.d"

[capabilities]
shell = true
file_read = true
file_write = true
git = true
jobs = true
async_jobs = true
async_shell_jobs = true

[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

Omit `transport` (or set `transport = "polling"`) to use the polling fallback.

### Agent project file example

Each project is one TOML file under `projects_dir`:

```toml
# /etc/webcodex/projects.d/webcodex.toml
id = "webcodex"
path = "/root/git/webcodex"
name = "WebCodex"
allow_patch = true
kind = "rust"
description = "WebCodex Runtime repository"
```

At registration the agent reports this to the server, which exposes it as the
runtime id `agent:workstation-1:webcodex`. The server never needs the
matching server-side `[projects.webcodex]` block.

See
[`deploy/agent-project.toml.example`](../deploy/agent-project.toml.example)
and [`deploy/projects.d/webcodex.toml.example`](../deploy/projects.d/webcodex.toml.example).

### Agent systemd

See
[`deploy/webcodex-agent.service.example`](../deploy/webcodex-agent.service.example)
and
[`deploy/webcodex-agent.toml.example`](../deploy/webcodex-agent.toml.example).
Install to `/etc/systemd/system/webcodex-agent.service` and
`/etc/webcodex/agent.toml` respectively, then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent
sudo journalctl -u webcodex-agent -f
```

## 3. Transport selection: WebSocket preferred, polling fallback

- **WebSocket (preferred)**: one long-lived connection to
  `GET /api/agents/ws`. The server pushes requests; the agent executes them and
  streams `result` / `job_update` back. Bearer auth is sent in the handshake
  `Authorization` header. Set `transport = "websocket"` in the agent config.
- **Polling (fallback)**: the agent polls `POST /api/shell/agent/poll` on
  `poll_interval_ms`. Use this for restricted networks or older agents. Set
  `transport = "polling"` (or omit `transport`).

Both transports reuse the same execution path (`run_shell`,
`handle_file_request`, `JobManager`) and the same server-side registry, queue,
and job state. Reliability guarantees (Phase 14): per-client pending requests
are capped (`MAX_QUEUED_REQUESTS_PER_CLIENT = 256`); on disconnect the server
marks the agent's running jobs `lost` and removes its push notifier, so an agent
is never permanently `online` and a job is never permanently `running`.

## 4. Reverse proxy / HTTPS

ChatGPT GPT Actions and MCP require a public HTTPS endpoint. Terminate TLS at a
reverse proxy and forward to the server bound on `127.0.0.1:8080`.

Key requirements for the reverse proxy:

- **WebSocket upgrade** for `GET /api/agents/ws`: forward `Upgrade` /
  `Connection: upgrade` headers and use HTTP/1.1 upstream.
- **No buffering** for streaming/MCP responses where useful.
- **Larger body / longer timeout** for Codex job requests and long-running tool
  calls.
- Preserve `Host`, `X-Real-IP`, `X-Forwarded-For`, `X-Forwarded-Proto`.

See
[`deploy/nginx.webcodex.example.conf`](../deploy/nginx.webcodex.example.conf)
for a complete nginx sample (HTTPS server block, WebSocket upgrade headers,
`/mcp`, `/openapi.json`, `/api/agents/ws`, body size and timeout tuning). The
sample uses `webcodex.example.com` as a placeholder — replace it with your domain.

### TLS notes

- Obtain a certificate (e.g. via Let's Encrypt / certbot) for your domain.
- Redirect plain HTTP to HTTPS.
- Keep the server bound to `127.0.0.1:8080` so only the proxy is publicly
  reachable. `WEBCODEX_TOKEN` is the application-layer gate; TLS is the transport
  gate. Both are required for production.
- Set `WEBCODEX_PUBLIC_URL` to the `https://` URL so `/openapi.json` advertises the
  correct server URL to ChatGPT.

## 5. ChatGPT GPT Actions import

In your ChatGPT GPT, under **Settings → Actions → Import from URL**, enter:

```
https://webcodex.example.com/openapi.json
```

Then configure Action authentication as **API Key**, type **HTTP**, header
`Authorization`, value `Bearer <wc_pat_user_api_token>`.

Use a Phase 2 personal API token for GPT Actions. Keep the server
`WEBCODEX_TOKEN` as a bootstrap/admin credential for creating users and tokens.

`/openapi.json` is the only GPT-Actions entry point. It is a `GET` route and is
not listed inside the schema `paths` (which is POST-only). The schema exposes a
small, stable operation set; see [GPT_ACTIONS.md](GPT_ACTIONS.md) for the full
list and the recommended call flow.

## 6. MCP endpoint

ChatGPT MCP clients connect to:

```
https://webcodex.example.com/mcp
```

`/mcp` speaks JSON-RPC 2.0 over HTTP (streamable-http-jsonrpc transport),
protected by Bearer auth. Use a Phase 2 personal API token for MCP clients;
keep `WEBCODEX_TOKEN` for bootstrap/admin setup. Supported methods:
`initialize`, `ping`, `tools/list`, `tools/call`,
`notifications/initialized`. MCP and GPT Actions share a single `ToolRuntime` —
there is no separate business logic for either surface. See
[AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) and the MCP section of
[README.md](../README.md).

## 7. Smoke test

After deploying, run the deployment smoke script against the live instance:

```bash
WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
WEBCODEX_TOKEN="<your-secret>" \
bash scripts/smoke_deployment.sh
```

The script verifies, without starting a server or agent:

- `GET /openapi.json` returns a valid OpenAPI schema.
- `POST /api/runtime/status` returns `success: true`.
- `POST /api/projects/list` returns `success: true`.
- `POST /mcp` `initialize` returns a non-empty `protocolVersion`.
- `POST /mcp` `tools/list` returns a non-empty `tools` array.

It uses only `curl` + `python3` (no `jq` dependency) and never prints the token.
See [`scripts/smoke_deployment.sh`](../scripts/smoke_deployment.sh) for details.

For a full local end-to-end smoke (server + WebSocket agent + GPT Actions + MCP
on one host, with a stub Codex CLI — no ChatGPT or real Codex required):

```bash
bash scripts/e2e_zero_config_ws.sh
```

See [E2E_VALIDATION.md](E2E_VALIDATION.md) for what that harness covers.

## 8. Logs and troubleshooting order

### Log locations

- **Server (systemd)**: `journalctl -u webcodex -f` (or the file you
  configured if not using the journal).
- **Agent (systemd)**: `journalctl -u webcodex-agent -f`.
- **Runtime data**: under `WEBCODEX_DATA` (default `./data`): `webcodex.db` (SQLite),
  `uploads/`, `.codex/jobs/<job_id>/metadata.json` and per-job stdout/stderr.
- **Local E2E harness**: prints `server.log` and `agent.log` paths on failure.

### Troubleshooting order

1. **Server up?** `curl -sS https://webcodex.example.com/openapi.json | head` —
   should return JSON. If not, check `systemctl status webcodex` and the
   reverse proxy.
2. **Auth working?** `POST /api/runtime/status` with
   `Authorization: Bearer <wc_pat_user_api_token>` should return
   `success: true`. A `401` means the token header is missing or wrong.
3. **Agent registered?** `POST /api/runtime/status` → `output.agents.count`
   should be `>= 1` and the agent's `status` should be `online`. If `0`, check
   the agent service log (`journalctl -u webcodex-agent`) — common causes:
   wrong `server_url`, wrong `token`, TLS failure, or the agent cannot reach
   `/api/agents/ws` through the proxy (WebSocket upgrade not forwarded).
4. **Projects visible?** `POST /api/projects/list` should include
   `agent:<client_id>:<project_id>`. If empty, the agent's `projects_dir` has
   no `*.toml` files or they failed to parse — check the agent log.
5. **Tool round-trip?** `readProjectFile` or `getProjectGitStatus` against a
   registered project id. If it hangs or times out, the agent is registered but
   not executing — check `transport`, the agent process, and `allowed_roots`.
6. **Codex jobs?** `runCodexTask` then poll `getRuntimeJobStatus` /
   `getRuntimeJobLog`. If jobs go `lost`, the agent transport disconnected
   (reconnect marks running jobs `lost` — restart the job). If jobs `fail`,
   check `CODEX_BIN` exists on the agent host and the project path is correct.

### Common reverse-proxy pitfalls

- WebSocket `/api/agents/ws` returns `200` instead of `101`: the proxy is not
  forwarding the `Upgrade` / `Connection` headers, or it is using HTTP/2 to the
  upstream (use HTTP/1.1 upstream for WebSocket).
- `/openapi.json` works but Actions fail: `WEBCODEX_PUBLIC_URL` is wrong, so
  ChatGPT calls the wrong host; or the GPT Action auth is not set to
  `Authorization: Bearer <token>`.
- Long Codex jobs cut off: raise the proxy read/connect timeouts and body size
  limit (see the nginx sample's `proxy_read_timeout` / `client_max_body_size`).

### WebSocket agent online/stale troubleshooting

A WebSocket agent can flip between `online` and `stale`/`offline` if the
long-lived connection drops. The runtime never leaves an agent permanently
`online` or a job permanently `running` (on disconnect the server marks the
agent's running jobs `lost` and removes its push notifier). Use this checklist
when an agent won't stay `online`:

1. **Is the agent a long-lived systemd service?** A WebSocket agent must stay
   running to keep the connection. Confirm
   `systemctl status webcodex-agent` is `active (running)` and
   `Restart=always` is set in the unit. A cron-launched or one-shot agent will
   not hold the connection.
2. **Does nginx forward the WebSocket upgrade?** `/api/agents/ws` must receive
   `Upgrade: websocket` / `Connection: upgrade` and use an HTTP/1.1 upstream.
   A `200` (not `101 Switching Protocols`) response means the upgrade headers
   were stripped. See `deploy/nginx.webcodex.example.conf`.
3. **Reverse-proxy timeout.** Idle WebSocket connections are closed by proxies
   with short read timeouts. Raise `proxy_read_timeout` (e.g. `3600s`) and keep
   the agent's ping interval below it. A connection that silently drops every
   N seconds usually points here.
4. **Agent logs: handshake / auth / ping-pong / reconnect.** In
   `journalctl -u webcodex-agent -f`, look for the TLS/handshake step, the
   `Authorization: Bearer` handshake result, periodic `ping`/`pong` keepalives,
   and reconnect attempts. A repeating `handshake failed` or `401` means a
   wrong `token` or `server_url`; a repeating reconnect loop with no `pong`
   points at the proxy timeout.
5. **Temporarily switch to polling to isolate the link.** Set
   `transport = "polling"` in the agent config (or omit `transport`) and
   restart the agent. If the agent stays `online` over polling but not over
   WebSocket, the WebSocket-specific path (proxy upgrade / timeout) is the
   cause, not auth or project registration. If polling also fails, check
   `server_url`/`token`/TLS first.
6. **Interpreting `runtime_status`.** `POST /api/runtime/status` exposes, per
   agent: `transport` (`websocket` / `polling`), `status`
   (`online` / `stale` / `offline`), `last_seen` (timestamp of the last
   heartbeat/result), and `pending_requests` (depth of the per-client request
   queue, capped at 256). `online` = live connection and recent heartbeat;
   `stale` = registered but `last_seen` is older than the decay window;
   `offline` = no current registration. A non-zero `pending_requests` with
   `online` means the agent is not draining its queue (agent process stalled or
   a long-running command is blocking it).

## 9. What the server does NOT need

- The server does **not** need a server-side `projects.toml` as a runtime
  project source. Project ids, paths, and policies come from agent
  registration. Do not restore `PROJECTS_CONFIG` as the runtime project path.
- The server does **not** run Codex locally; it forwards `run_codex` to the
  owning agent.
- The server does **not** implement QUIC (future transport).
- There is no second `ToolRuntime` — GPT Actions, MCP, and both agent
  transports all share one execution layer.
