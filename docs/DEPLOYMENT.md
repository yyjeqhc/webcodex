# Deployment

[English](DEPLOYMENT.md) | [简体中文](DEPLOYMENT.zh-CN.md)

This guide covers the current WebCodex production shape: server bootstrap, service installation, agent configuration, GPT Actions, MCP, and smoke checks.

## Components

- `webcodex`: public unified CLI for project workflows, server/agent lifecycle, enrollment, and operations.
- `webcodex-server`: server process exposing REST, GPT Actions OpenAPI, MCP, and agent endpoints.
- `webcodex-runner`: long-lived worker connected by `auto` transport (QUIC first, then WebSocket, then polling) or by an explicitly selected transport.

## First Deployment: Read This First

A first-time operator does not need OAuth, QUIC, or the account-credential
workflow. The minimum production path is:

1. Use a Linux x64 host with systemd, `sudo`, and a public HTTPS domain or
   trusted tunnel.
2. Install `@yyjeqhc/webcodex`, run `webcodex server init`, and install the
   `webcodex-server` service.
3. Configure the reverse proxy and set `WEBCODEX_PUBLIC_URL` to the exact public
   HTTPS origin.
4. Create a short-lived pairing code on the server and run
   `webcodex login <server-url> --code <code>` on the machine that owns the
   repositories (`webcodex client enroll` remains the advanced alternative).
5. Install the `webcodex-runner` service on that repository machine.
6. Run `webcodex ops status --strict`; only then import the GPT Actions
   schema or add the MCP connector.

For a same-machine evaluation without a permanent service or public ingress,
use the project-first flow in the main README instead. The rest of this guide
contains the exact production commands, optional OAuth, multi-user enrollment,
and transport details.

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
  -d '{"name":"ChatGPT MCP","redirect_uris":["https://chatgpt.com/connector/oauth/<callback-id>"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

Save the `client_secret` from the response — it is returned only once and only
its SHA-256 hash is stored. Omit `allowed_scopes` to grant the full delegable
OAuth scope set (`runtime:read project:read project:write job:run account:manage`).
For ChatGPT, copy the callback URL shown in the app's Advanced OAuth Settings
and register it exactly. Use `client_secret_post` in ChatGPT. Keep
`offline_access` enabled when offered, but do not add it to `allowed_scopes`:
it is advertised separately as a protocol-level refresh-token scope and grants
no WebCodex API permission.


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
JWKS/JWT ID tokens, `userinfo_endpoint`, `client_credentials` grant, and device
code flow are not implemented. OAuth `resource` handling is intentionally
limited to the configured WebCodex issuer and canonical `/mcp` resource; WebCodex
does not act as a general multi-resource authorization server. The default
client scope set can grant full delegable access, so prefer narrowed
`allowed_scopes` for clients that do not need account administration.

## Server-first setup

The documented distribution path uses the npm thin installer/wrapper:

```bash
npm install -g @yyjeqhc/webcodex
```
The npm wrapper currently supports `linux-x64`, `linux-arm64`, `darwin-arm64`, and `win32-x64`. Windows x64 supports the CLI + Runner workflow against a remote Linux Server; the long-running Windows Server/service path remains unsupported. Release checksums are generated on OE and embedded in the published npm package rather than committed to the source tree.

Initialize the env file:

```bash
sudo webcodex server init \
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
sudo webcodex server install \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex-server
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex server status --env-file /etc/webcodex/webcodex.env
```

The compatibility commands remain available:

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
webcodex setup single-user
```

Prefer `webcodex` in new docs and automation.

## Binary deployment checklist

Server:

1. Install the public `webcodex` CLI and the `webcodex-server` binary.
2. Run `webcodex server init`.
3. Run `webcodex server install --overwrite` only if replacing an old unit.
4. Run `sudo systemctl daemon-reload`.
5. Run `sudo systemctl enable --now webcodex`.
6. Run `webcodex server status`.

Server/admin:

7. Run `webcodex pairing create`.

Client:

8. Install the public `webcodex` CLI and the `webcodex-runner` binary.
9. As the ordinary account that will run project commands, run
   `webcodex login <server-url> --code <code>` (advanced:
   `webcodex client enroll`). Do not use `sudo`.
10. Run `webcodex agent install --scope user --config <login-reported-agent-config>`.
11. Run `webcodex agent status --scope user --config <login-reported-agent-config>`.
12. Run `webcodex ops status --strict`.

`agent install` reloads, enables, and starts the selected service manager. The
ordinary path therefore needs neither `sudo` nor separate `systemctl`
commands. `/etc/webcodex/webcodex.env` is server-side only. An ordinary user's
client files default to `$XDG_CONFIG_HOME/webcodex` (or
`$HOME/.config/webcodex`); system-scope profiles can instead live under
`/etc/webcodex` when an administrator deliberately provisions them.

## Account credential onboarding flow

For deployments that do not use pairing, use the account credential flow below. The commands in this section use `https://your-domain.example` placeholders.

1. Start the server with `WEBCODEX_TOKEN` in the server env file. This is the bootstrap/root/admin credential only.
2. Create a user with `webcodex users create --issue-credential` and give the returned `wc_acct_xxx` to that user once. The binary help for this path uses `users create` plus `--server-url`, while `token create-local` and `agent-token create-local` use `--server`.
3. The user runs `webcodex token create-local` with `wc_acct_xxx` to locally generate a `wc_pat_xxx` and register only its hash. Use this PAT for GPT Actions, MCP, and runtime API calls.
4. The user runs `webcodex agent-token create-local` with `wc_acct_xxx` and `--client-id <client_id>` to locally generate a `wc_agent_xxx` and register only its hash. Use this token only for `webcodex-runner`.
5. Initialize `webcodex-runner`, add top-level agent `projects.d/*.toml` files, start the agent, then verify `runtime_status`, `projects/list`, and a read-only `tools/call` such as `git_status`.

Do not use `wc_acct_xxx` as a GPT Action/MCP token and do not put it in `agent.toml`.

## Invite another user

When a server owner invites a friend or another operator, use a short-lived pairing code. Do not copy long-lived credentials between machines.

Server/admin side:

```bash
webcodex pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` is server/admin-side. This ordinary flow creates an unbound code, so the device running `login` claims its automatically generated id. `/etc/webcodex/webcodex.env` is server-side only. Send only the short-lived `wc_pair_*` code to the friend.

Client/friend side:

```bash
webcodex login https://your-domain.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"

webcodex agent install --scope user \
  --config "$HOME/.config/webcodex/https_your-domain.example/friendname/agent.toml"
webcodex agent status --scope user \
  --config "$HOME/.config/webcodex/https_your-domain.example/friendname/agent.toml"

webcodex ops status \
  --server-url https://your-domain.example \
  --token-file "$HOME/.config/webcodex/https_your-domain.example/friendname/webcodex-user-token" \
  --strict
```

`webcodex login` is the client/friend-side entry: it derives a unique device
name, redeems the pairing code, and writes the client-side `webcodex-user-token`
and an `agent.toml` below the user's WebCodex config directory. Login prints
the exact agent config path and a `webcodex agent install --scope user
--config <path>` command. GPT Actions, MCP, and ordinary REST/project APIs use
the client-side `webcodex-user-token`; the generated agent config uses the
client-side `webcodex-runner-token` only for Agent transport. Do not copy `WEBCODEX_TOKEN`,
`wc_pat_*`, `wc_agent_*`, complete env files, or complete `agent.toml` files
between machines. Each friend should use a unique `username`; the device name is
made unique automatically. The advanced `webcodex client enroll` flow is still
available for an explicit client id or custom output directory.

## Runtime console

WebCodex serves a read-only browser console at:

```text
https://your-domain.example/console
```

The static console bundle contains no secrets. It fetches the shared
project-readiness projection from the protected Connector API and shows only
Project, Connection, Agent/coding readiness, findings, and the next CLI action.
It does not expose the Agent registry or transport details. The console is not
part of the GPT Actions OpenAPI and is not a full admin UI.

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

Client enroll generates the agent config. For the normal non-root installation,
install a user unit with:

```bash
webcodex agent install \
  --scope user \
  --profile workstation \
  --bin /opt/webcodex/bin/webcodex-runner
webcodex agent status \
  --scope user \
  --profile workstation \
  --server-url https://your-domain.example
```

The npm install location and systemd scope are independent. User scope uses
`systemctl --user`, stores units under `$XDG_CONFIG_HOME/systemd/user` (or
`$HOME/.config/systemd/user`), uses `default.target`, omits `User=`/`Group=`,
and needs no administrator privileges. Non-root callers default to user scope.
Use the same scope for install, status, start, stop, restart, logs, and
uninstall.

An enabled user unit follows the lifetime of that account's user manager. It
does not by itself guarantee startup before the first login or continued
operation after the last logout. For unattended boot persistence, an
administrator may explicitly enable lingering with
`sudo loginctl enable-linger <runner-user>` after reviewing the resulting
long-lived service authority. WebCodex never changes lingering automatically.

For an administrator-managed service, system scope uses
`/etc/systemd/system`, `systemctl`, and `multi-user.target`. It normally
requires a named non-root user and a working directory that account can use:

```bash
sudo webcodex agent install \
  --scope system \
  --profile workstation \
  --user <runner-user> \
  --working-directory /home/<runner-user> \
  --config /etc/webcodex/clients/workstation/agent.toml
sudo webcodex agent status --scope system --profile workstation
```

`--group` is optional. WebCodex does not create a system user, change sudoers,
or migrate permissions. A root Runner is refused unless the administrator
also supplies `--allow-root-runner`; this discouraged exception prints and
embeds a prominent warning. Explicit config and working-directory overrides
remain available, but the selected service account must be able to read or use
them. An explicit service-file must match the selected manager scope: user
scope rejects system unit paths and system scope rejects user-unit paths.
Existing units are never silently overwritten.

For a foreground test, start the agent with:

```bash
webcodex-runner --profile workstation
```

Advanced manual initialization uses `webcodex agent init`; the duplicate
`webcodex-runner init` alias was removed.

Important agent settings:

| Setting | Notes |
| --- | --- |
| `server_url` | Public WebCodex URL. |
| `token` | Agent token. Do not commit or print it. |
| `client_id` | Stable id used in `agent:<client_id>:<project_id>`. |
| `owner` | Owner principal for this agent. |
| `transport` | Prefer `auto` with `[quic]` configured: QUIC first, then WebSocket, then polling. Use strict `quic`, `websocket`, or `polling` only when you want exactly one transport. |
| `projects_dir` | Directory of project registry files. |
| `temporary_projects_root` | Optional existing Runner-owned root for managed temporary projects; it is validated against the effective Runner path policy (and must be inside `allowed_roots` in narrowed deployments). |
| `[policy]` | Local execution boundary. |
| `[shell]` | Optional shell profile definitions and bounded persistent-shell limits. |
| `[ssh.resources.<name>]` | Optional named SSH target for Session-bound `run_shell` / `run_job`; contains only `host` (including a normal OpenSSH Host alias) and optional remote `default_cwd`. |

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

Persistent-shell settings are optional and preserve compatibility with older
configuration files. Defaults and accepted ranges are:

```toml
[shell]
# Live long-running shells owned by this Runner process: 1..64.
max_persistent_shells = 8
# Reclaim an idle shell after this many seconds: 1..86400.
persistent_shell_idle_timeout_secs = 1800
```

Idle collection never interrupts an executing command. A shell is also
released on explicit close, Workflow Session close, project
disable/unregister, process exit, Runner disconnect/shutdown, or loss of
command synchronization. These processes are not recovered across Runner or
Server restart.

Agent project files in `projects_dir` may set `shell_profile = "rust"` to bind a project to a configured profile.

SSH resources are Runner-local. For example:

```toml
[ssh.resources.tmp]
host = "tmp"
default_cwd = "/opt/webcodex-edge"
```

The `host` value is passed to the Runner machine's OpenSSH client, so normal
`~/.ssh/config`, `/etc/ssh/ssh_config`, keys, `ssh-agent`, and `ProxyJump`
configuration remain on that machine. Do not put credentials, private keys,
passwords, or complete SSH configuration into Session data, Server storage, or
tool input. A Session's `execution_context.resource = "tmp"` changes only
`run_shell` and `run_job`; other project tools remain local. The Runner
advertises `ssh_shell` only when its OpenSSH client is available.

Shell profiles prepare a one-time environment snapshot for one-shot commands.
An explicit Session persistent shell instead applies the selected profile once
when its long-lived process opens; later commands use that process state.
Neither path sources `.bashrc`/`.profile` by default. See
[SHELL_PROFILES.md](SHELL_PROFILES.md) for Rust/Cargo, Python venv, Conda
examples, resolution rules, and safety boundaries. After editing `agent.toml`,
reload the matching manager (`systemctl --user reload webcodex-runner` for
user scope, or `sudo systemctl reload webcodex-runner` for system scope). This
atomically applies policy, shell, SSH-resource, and tool-provider settings to
new requests. An already-open
persistent shell is not silently restarted or moved; current policy is still
rechecked before later exec/status operations, while close remains available
for cleanup; close/reopen applies new defaults. Identity, server/auth,
project source, concurrency, capabilities, and transport changes still require
a restart. Invalid reloads keep the active generation; `projects.d` continues
to refresh independently. Provider lifecycle and the exact field boundary are
documented in
[agent/claude-code-mcp-provider.md](agent/claude-code-mcp-provider.md#explicit-agent-config-reload).

`runtime_status` and `listAgents` expose a redacted policy summary plus a sanitized `shell_profiles` summary (profile names, `has_init_script`, `env_keys_count`, `program`, `args_count`). `listProjects` exposes `shell_profile`, `resolved_shell_profile`, and `shell_profile_status` (`configured` / `missing` / `not_configured` / `unknown`). They do not expose tokens, env values, `Authorization` headers, full `agent.toml`, the full env snapshot, or shell profile `init_script` bodies.

## Authentication and transport

Ordinary REST, polling, MCP, and GPT Actions calls must use the generated
`webcodex-user-token` (`wc_pat_*`):

```text
Authorization: Bearer <token>
```

`?token=` is allowed only for `/api/agents/ws` WebSocket handshake compatibility. Do not use query-string tokens for polling, REST, MCP, or GPT Actions.

The `webcodex-runner-token` (`wc_agent_*`) is accepted only by Agent transport
endpoints. Using it on a project/runtime endpoint remains a 403; do not work
around that boundary.

For agents, prefer `transport = "auto"` with QUIC configured. WebSocket and polling remain supported fallbacks for constrained networks.

## GPT Actions and MCP

Import GPT Actions from:

```text
https://your-domain.example/openapi.json
```

Configure GPT Actions authentication as HTTP Bearer/API key in the `Authorization` header.

This is an OpenAPI Custom GPT Action integration, not a claim that WebCodex is
published in the ChatGPT plugin directory. Plugins, apps, Custom GPTs, and GPT
Actions are distinct layers; see [GPT_ACTIONS.md](GPT_ACTIONS.md).

The OpenAPI GPT Actions management surface intentionally excludes users, API tokens, agent tokens, pairing/enrollment, setup, doctor, npm, server management, and audit endpoints. Use `webcodex` for those tasks.

MCP uses the same user API token and the same `ToolRuntime` as GPT Actions.

## Codex-specific workflows

WebCodex no longer exposes `run_codex` or legacy `/api/codex/*` routes. GPT Actions and MCP clients should use structured edit tools, patch validation, cargo validation, bounded `run_shell` / `run_job` escape hatches, `show_changes`, `workspace_hygiene_check`, and `finish_coding_task`. Operators who need Codex-specific workflows should run Codex outside WebCodex.

## Smoke checks

Recommended production smoke sequence:

1. `webcodex ops status --server-url https://your-domain.example --token-file PATH --strict` passes its read-only checks.
2. `POST /api/runtime/status` returns `service=webcodex` and the expected public URL.
3. `listAgents` shows at least one online agent.
4. `listProjects` shows `agent:<client_id>:<project_id>` ids.
5. Read-only project tools work on a known project.
6. Write/replace/validate tests are limited to disposable smoke projects.

For a repeatable process-level SIGHUP reload smoke:

```bash
WEBCODEX_E2E_AGENT_RELOAD=1 \
./scripts/test-agent-config-reload-e2e.sh
```

The smoke does not use systemd or require Claude Code. It creates a temporary
Server, Agent, project, config, and Git fixture; verifies valid, invalid, and
mixed reload semantics through real Agent requests; and checks process, port,
fixture, and temporary-directory cleanup.

## Troubleshooting

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for the operational checklist and common deployment fixes, including existing systemd services, `HTTP reachable: no`, missing client CLI on `PATH`, server-side pairing vs client-side enrollment, agent-only client warnings, and `client online: no`.
