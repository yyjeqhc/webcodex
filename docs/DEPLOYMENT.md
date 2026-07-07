# Deployment

[English](DEPLOYMENT.md) | [简体中文](DEPLOYMENT.zh-CN.md)

This guide covers the current WebCodex production shape: server bootstrap, service installation, agent configuration, GPT Actions, MCP, and smoke checks.

## Components

- `webcodex`: server exposing REST, GPT Actions OpenAPI, MCP, and agent endpoints.
- `webcodex-agent`: long-lived worker connected by `auto` transport (QUIC first, then WebSocket, then polling) or by an explicitly selected transport.
- `webcodex-cli`: recommended management CLI for server bootstrap, pairing/enrollment, status, and doctor checks.

## Server configuration

Required production settings usually include:

```text
WEBCODEX_TOKEN=<bootstrap-admin-token>
WEBCODEX_ADDR=127.0.0.1:8080
WEBCODEX_DATA=/var/lib/webcodex
```

`WEBCODEX_PUBLIC_URL=https://your-domain.example` is optional at `server init` time because you may not know the final HTTPS domain yet. Configure it before connecting GPT Actions, MCP clients, remote agents, or any user-facing OpenAPI flow; otherwise runtime status and OpenAPI server URLs may point at the wrong address.

Use the bootstrap token only for initial setup/admin work. Day-to-day GPT Actions and MCP calls should use a user API token. Agents should use agent tokens.

## OAuth2

OAuth2 is disabled by default. Enable it to let GPT Actions / MCP clients obtain delegated `wc_oat_*` access tokens via the authorization-code flow:

```text
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=https://your-domain.example
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

`WEBCODEX_OAUTH2_ISSUER` takes precedence over `WEBCODEX_PUBLIC_URL` for the
`/.well-known/*` metadata endpoint URLs. Set both to your public HTTPS domain
in production so the authorize/token/revocation endpoints advertised by
discovery are reachable by clients and the authorize session cookie is marked
`Secure`.

### Create an OAuth client

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"ChatGPT Action","redirect_uris":["https://example.com/oauth/callback"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

Save the `client_secret` from the response — it is returned only once and only
its SHA-256 hash is stored. Omit `allowed_scopes` to grant the full delegable
OAuth scope set (`runtime:read project:read project:write job:run account:manage`).

List and revoke clients with `POST /api/oauth/clients/list` and
`POST /api/oauth/clients/revoke` (body `{"client_id":"wc_client_..."}`).
Revoking a client also revokes all of its active access tokens, refresh tokens,
and authorization codes.

### Browser authorize flow

Point the client at `https://your-domain.example/oauth/authorize?...`. With no
Bearer token and no session cookie, WebCodex renders a minimal login page; enter
a WebCodex PAT to get a 10-minute `HttpOnly` session cookie, then approve the
consent page. `Allow` redirects back to the registered `redirect_uri` with a
`wc_oac_*` code; exchange it at `POST /oauth/token` for a `wc_oat_*` access
token. A first-party Bearer PAT can still use the direct authorization-code
issuance path on `/oauth/authorize` for non-browser clients. The bootstrap token
can create OAuth clients but cannot authorize one because it has no user id.

A full end-to-end smoke test walkthrough (enable, create client, authorize,
token exchange, revoke) is in [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md).

### Not yet supported

Dynamic client registration, OIDC / `/.well-known/openid-configuration`,
JWKS/JWT ID tokens, `userinfo_endpoint`, `client_credentials` grant, device
code flow, and MCP resource/audience binding are not implemented. The default
client scope set can grant full delegable access, which is convenient for
self-hosted GPT Action / MCP use; use narrowed `allowed_scopes` for
untrusted clients.

## Server-first setup

The documented distribution path uses the npm thin installer/wrapper:

```bash
npm install -g @yyjeqhc/webcodex
```
Planned v0.2.0 GitHub release artifacts are `linux-x64`, `linux-arm64`, and `darwin-arm64`. `darwin-x64`, Windows, and other targets are not planned for v0.2.0 unless a later release adds artifacts. The npm wrapper currently installs v0.1.0 binaries; use matching GitHub release artifacts for v0.2.0 only after those artifacts are published, and do not expect npm to install v0.2.0 until the wrapper and manifest are updated.


Initialize the env file:

```bash
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
```

`server init` creates the env file and writes the bootstrap admin token into `WEBCODEX_TOKEN`. It also writes the server listen address and data directory settings. It does not create `wc_pat_...` user API tokens or `wc_agent_...` agent tokens.

For one-off admin CLI commands, either pass `--env-file /etc/webcodex/webcodex.env` when the command supports it, pass `--token "$WEBCODEX_TOKEN"` explicitly, or load the env file into the current shell:

```bash
set -a
. /etc/webcodex/webcodex.env
set +a
```

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

`/etc/webcodex/webcodex.env` is server-side only. Client-side files live under a profile directory such as `/etc/webcodex/clients/workstation/agent.toml`, `/etc/webcodex/clients/workstation/webcodex-user-token`, `/etc/webcodex/clients/workstation/webcodex-agent-token`, and `/etc/webcodex/clients/workstation/projects.d` when multiple users or clients share one machine.

## Account credential onboarding flow

For deployments that do not use pairing, use the account credential flow below. The commands in this section use `https://your-domain.example` placeholders.

1. Start the server with `WEBCODEX_TOKEN` in the server env file. This is the bootstrap/root/admin credential only.
2. Create a user with `webcodex-cli users create --issue-credential` and give the returned `wc_acct_xxx` to that user once. The binary help for this path uses `users create` plus `--server-url`, while `token create-local` and `agent-token create-local` use `--server`.
3. The user runs `webcodex-cli token create-local` with `wc_acct_xxx` to locally generate a `wc_pat_xxx` and register only its hash. Use this PAT for GPT Actions, MCP, and runtime API calls.
4. The user runs `webcodex-cli agent-token create-local` with `wc_acct_xxx` and `--client-id <client_id>` to locally generate a `wc_agent_xxx` and register only its hash. Use this token only for `webcodex-agent`.
5. Initialize `webcodex-agent`, add top-level agent `projects.d/*.toml` files, start the agent, then verify `runtime_status`, `projects/list`, and a read-only `tools/call` such as `git_status`.

Do not use `wc_acct_xxx` as a GPT Action/MCP token and do not put it in `agent.toml`.

## Invite another user

When a server owner invites a friend or another operator, use a short-lived pairing code. Do not copy long-lived credentials between machines.

Server/admin side:

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` is server/admin-side. `/etc/webcodex/webcodex.env` is server-side only. Send only the short-lived `wc_pair_*` code to the friend.

Client/friend side:

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --profile workstation \
  --allowed-root /home/friend/git

webcodex-cli agent install-service \
  --profile workstation \
  --bin /opt/webcodex/bin/webcodex-agent \
  --overwrite

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent-workstation

webcodex-cli doctor \
  --profile workstation \
  --server-url https://your-domain.example \
  --strict
```

`client enroll` is client/friend-side. GPT Actions should use the client-side `webcodex-user-token`; the generated agent config uses the client-side agent token for `webcodex-agent`. Do not copy `WEBCODEX_TOKEN`, `wc_pat_*`, `wc_agent_*`, complete env files, or complete `agent.toml` files between machines. Each friend should use a unique `username` and `client_id`.

## Runtime console

WebCodex serves a read-only browser console at:

```text
https://your-domain.example/console
```

The static console bundle contains no secrets. Runtime data is fetched by the browser from protected APIs using the user's credentials, session, or token as applicable. The console is not part of the GPT Actions OpenAPI and is not a full admin UI.

### Runtime job API trust model

WebCodex runtime job APIs are intended for trusted single-operator deployments.
`job_status`, `job_log`, `list_jobs`, and `job_tail` are not a tenant boundary
between mutually untrusted users. Do not expose one WebCodex runtime to multiple
untrusted users without adding job owner isolation for project-less job APIs.
Use separate server/runtime instances for untrusted users.

## Public HTTPS URL

GPT Actions require a public HTTPS URL. WebCodex CLI does not automate reverse proxy or tunnel setup, so configure one before importing `/openapi.json` into ChatGPT.

Set the same public URL in the server env file:

```text
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

Minimal Nginx example:

```nginx
server {
    listen 80;
    server_name your-domain.example;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name your-domain.example;

    ssl_certificate /etc/letsencrypt/live/your-domain.example/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/your-domain.example/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
    }

    location /api/agents/ws {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_buffering off;
    }
}
```

Keep WebCodex listening on `127.0.0.1:8080` behind the proxy. The QUIC agent transport is separate from this HTTPS path; see [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md) before opening UDP 8443.

## Agent configuration

Client enroll generates the agent config. Install a systemd unit with:

```bash
sudo webcodex-cli agent install-service \
  --profile workstation \
  --bin /opt/webcodex/bin/webcodex-agent
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent-workstation
webcodex-cli agent status \
  --profile workstation \
  --server-url https://your-domain.example
```

For a foreground test, start the agent with:

```bash
webcodex-agent --profile workstation
```

`webcodex-agent init` remains available as a compatibility entry point.

Important agent settings:

| Setting | Notes |
| --- | --- |
| `server_url` | Public WebCodex URL. |
| `token` | Agent token. Do not commit or print it. |
| `client_id` | Stable id used in `agent:<client_id>:<project_id>`. |
| `owner` | Owner principal for this agent. |
| `transport` | Prefer `auto` with `[quic]` configured: QUIC first, then WebSocket, then polling. Use strict `quic`, `websocket`, or `polling` only when you want exactly one transport. |
| `projects_dir` | Directory of project registry files. |
| `[policy]` | Local execution boundary. |
| `[shell]` | Optional shell profile definitions for project development environments. |

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

Agent project files in `projects_dir` may set `shell_profile = "rust"` to bind a project to a configured profile.

Shell profiles prepare a one-time environment snapshot per project/profile (no persistent shell, no `.bashrc`/`.profile` sourced by default). See [SHELL_PROFILES.md](SHELL_PROFILES.md) for Rust/Cargo, Python venv, and Conda examples, resolution rules, and safety boundaries. Changing a profile requires restarting `webcodex-agent` (no reload API).

`runtime_status` and `listAgents` expose a redacted policy summary plus a sanitized `shell_profiles` summary (profile names, `has_init_script`, `env_keys_count`, `program`, `args_count`). `listProjects` exposes `shell_profile`, `resolved_shell_profile`, and `shell_profile_status` (`configured` / `missing` / `not_configured` / `unknown`). They do not expose tokens, env values, `Authorization` headers, full `agent.toml`, the full env snapshot, or shell profile `init_script` bodies.

## Authentication and transport

Ordinary REST, polling, MCP, and GPT Actions calls must use:

```text
Authorization: Bearer <token>
```

`?token=` is allowed only for `/api/agents/ws` WebSocket handshake compatibility. Do not use query-string tokens for polling, REST, MCP, or GPT Actions.

For agents, prefer `transport = "auto"` with QUIC configured. WebSocket and polling remain supported fallbacks for constrained networks.

## GPT Actions and MCP

Import GPT Actions from:

```text
https://your-domain.example/openapi.json
```

Configure GPT Actions authentication as HTTP Bearer/API key in the `Authorization` header.

The OpenAPI GPT Actions management surface intentionally excludes users, API tokens, agent tokens, pairing/enrollment, setup, doctor, npm, server management, and audit endpoints. Use `webcodex-cli` for those tasks.

MCP uses the same user API token and the same `ToolRuntime` as GPT Actions.

## Codex CLI delegation

Codex delegation is currently hidden from GPT Actions, MCP `tools/list`, runtime tool discovery, and generic model-facing dispatch. Operators who want Codex should run it outside WebCodex, or wait for a future explicit opt-in feature flag.

## Smoke checks

Recommended production smoke sequence:

1. `webcodex-cli doctor --server-url https://your-domain.example --user-token-file PATH` passes its non-destructive checks.
2. `POST /api/runtime/status` returns `service=webcodex` and the expected public URL.
3. `listAgents` shows at least one online agent.
4. `listProjects` shows `agent:<client_id>:<project_id>` ids.
5. Read-only project tools work on a known project.
6. Write/replace/validate tests are limited to disposable smoke projects.

## Troubleshooting

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for the operational checklist and common deployment fixes, including existing systemd services, `HTTP reachable: no`, missing client CLI on `PATH`, server-side pairing vs client-side enrollment, agent-only client warnings, and `client online: no`.
