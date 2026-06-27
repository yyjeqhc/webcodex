# Deployment

This guide covers the current WebCodex production shape: server bootstrap, service installation, agent configuration, GPT Actions, MCP, and smoke checks.

## Components

- `webcodex`: server exposing REST, GPT Actions OpenAPI, MCP, and agent endpoints.
- `webcodex-agent`: long-lived worker connected by WebSocket or polling.
- `webcodex-cli`: recommended management CLI for server bootstrap, pairing/enrollment, status, and doctor checks.

## Server configuration

Required production settings usually include:

```text
WEBCODEX_TOKEN=<bootstrap-admin-token>
WEBCODEX_ADDR=127.0.0.1:8080
WEBCODEX_DATA=/var/lib/webcodex
```

`WEBCODEX_PUBLIC_URL=https://your-domain.example` is optional at server init time. Configure it when you have the public HTTPS URL you want runtime status/OpenAPI to report.

Use the bootstrap token only for initial setup/admin work. Day-to-day GPT Actions and MCP calls should use a user API token. Agents should use agent tokens.

## Server-first setup

The documented distribution path assumes the MVP npm wrapper:

```bash
npm install -g @webcodex/webcodex
```

Initialize the env file:

```bash
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
```

`server init` creates only `WEBCODEX_TOKEN`. It does not create `wc_pat_...` user API tokens or `wc_agent_...` agent tokens.

Install and start the systemd service:

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex-cli server status --env-file /etc/webcodex/webcodex.env
```

The compatibility commands remain available:

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
webcodex-cli setup single-user
```

Prefer `webcodex-cli` in new docs and automation.

## Binary deployment checklist

Server:

1. Install `webcodex` and `webcodex-cli` binaries.
2. Run `webcodex-cli server init`.
3. Run `webcodex-cli server install-service --overwrite` only if replacing an old unit.
4. Run `sudo systemctl daemon-reload`.
5. Run `sudo systemctl enable --now webcodex`.
6. Run `webcodex-cli server status`.

Server/admin:

7. Run `webcodex-cli pairing create`.

Client:

8. Install `webcodex-agent` and `webcodex-cli` binaries.
9. Run `webcodex-cli client enroll`.
10. Run `webcodex-cli agent install-service`.
11. Run `sudo systemctl daemon-reload`.
12. Run `sudo systemctl enable --now webcodex-agent`.
13. Run `webcodex-cli agent status`.
14. Run `webcodex-cli doctor --strict`.

`/etc/webcodex/webcodex.env` is server-side only. `/etc/webcodex/agent.toml`, `webcodex-user-token`, and `webcodex-agent-token` are client-side files when deploying a client agent as a service.

## Enrollment

On the server/admin side, create a short-lived one-time pairing code:

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username alice \
  --client-id alice-laptop
```

`pairing create` is a server/admin-side command and needs server bootstrap/admin auth. The default server bootstrap env file exists on the server, not the client. Copy only the short-lived `wc_pair_*` code to the client; do not copy `WEBCODEX_TOKEN`, `wc_pat_*`, or `wc_agent_*` values from the server.

On the client side, exchange the code over HTTPS and write local credentials:

```bash
sudo webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <temporary_pairing_code> \
  --client-id alice-laptop \
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml
```

Pairing creates no server-side `wc_pat_*` or `wc_agent_*` token files. Client enroll creates those tokens for the paired user/client and saves them locally with `0600` permissions on Unix:

```text
/etc/webcodex/webcodex-user-token
/etc/webcodex/webcodex-agent-token
/etc/webcodex/agent.toml
```

For non-root foreground agents, the default directory is `~/.config/webcodex`. GPT Actions should use the client-side user-token file.

## Public HTTPS URL

GPT Actions require a public HTTPS URL. WebCodex CLI does not automate reverse proxy or tunnel setup.

## Agent configuration

Client enroll generates the agent config. Install a systemd unit with:

```bash
sudo webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin /opt/webcodex/bin/webcodex-agent
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent
webcodex-cli agent status \
  --config /etc/webcodex/agent.toml \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token
```

For a foreground test, start the agent with:

```bash
webcodex-agent --config ~/.config/webcodex/agent.toml
```

`webcodex-agent init` remains available as a compatibility entry point.

Important agent settings:

| Setting | Notes |
| --- | --- |
| `server_url` | Public WebCodex URL. |
| `token` | Agent token. Do not commit or print it. |
| `client_id` | Stable id used in `agent:<client_id>:<project_id>`. |
| `owner` | Owner principal for this agent. |
| `transport` | Prefer `websocket`; polling is fallback. |
| `projects_dir` | Directory of project registry files. |
| `[policy]` | Local execution boundary. |
| `[shell]` | Optional shell program/PATH/env customization for commands. |

Policy behavior:

- Missing or empty `allowed_roots` defaults to `$HOME`.
- Explicit `allowed_roots` overrides the `$HOME` default.
- Use explicit roots when you want to narrow the agent, for example to one workspace tree.

Example narrow policy:

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

`runtime_status` and `listAgents` expose only a redacted policy summary: `allow_raw_shell`, `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`, and `max_output_bytes`. They do not expose tokens, env values, `Authorization` headers, full `agent.toml`, or shell `init_script` values.

## Authentication and transport

Ordinary REST, polling, MCP, and GPT Actions calls must use:

```text
Authorization: Bearer <token>
```

`?token=` is allowed only for `/api/agents/ws` WebSocket handshake compatibility. Do not use query-string tokens for polling, REST, MCP, or GPT Actions.

WebSocket is preferred for agents. Polling remains available for constrained networks.

## GPT Actions and MCP

Import GPT Actions from:

```text
https://your-domain.example/openapi.json
```

Configure GPT Actions authentication as HTTP Bearer/API key in the `Authorization` header.

The OpenAPI GPT Actions management surface intentionally excludes users, API tokens, agent tokens, pairing/enrollment, setup, doctor, npm, server management, and audit endpoints. Use `webcodex-cli` for those tasks.

MCP uses the same user API token and the same `ToolRuntime` as GPT Actions.

## Optional Codex CLI jobs

`runCodexTask` is optional. It requires the Codex CLI to be installed and configured on the agent machine. It does not start a new `webcodex-agent`; it delegates work to an already connected agent.

## Smoke checks

Recommended production smoke sequence:

1. `webcodex-cli doctor --server-url https://your-domain.example --user-token-file PATH` passes its non-destructive checks.
2. `POST /api/runtime/status` returns `service=webcodex` and the expected public URL.
3. `listAgents` shows at least one online agent.
4. `listProjects` shows `agent:<client_id>:<project_id>` ids.
5. Read-only project tools work on a known project.
6. Write/replace/validate tests are limited to disposable smoke projects.
