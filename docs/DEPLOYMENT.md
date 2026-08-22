# Deployment

[English](DEPLOYMENT.md) | [简体中文](DEPLOYMENT.zh-CN.md)

This guide covers self-hosting WebCodex in production: building and installing
the binaries, bootstrapping the Server, enrolling Runner machines, connecting
MCP/GPT clients, and running smoke checks. For the shortest first ChatGPT/MCP
connection, see [Quick Start](QUICK_START.md).

## Components

- `webcodex` — the unified CLI for project workflows, Server/Runner lifecycle,
  enrollment, and operations.
- `webcodex-server` — the Server process exposing REST, GPT Actions OpenAPI,
  MCP, and Runner endpoints.
- `webcodex-runner` — the long-lived worker on the machine that owns the
  repositories.

## Build and install

The documented distribution path is the npm thin installer/wrapper:

```bash
npm install -g @yyjeqhc/webcodex
```

Supported package platforms are Linux x64, Linux arm64, macOS arm64, Windows
x64, and Windows arm64. Windows x64/arm64 support the CLI + Runner workflow
against a remote Linux Server; a long-running Windows Server is not supported.
The npm wrapper
requires Node.js 18 or newer. Starting with v0.3.5, the native Linux x64
artifact targets glibc 2.17 or newer.

Build from source:

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

This produces `webcodex`, `webcodex-server`, and `webcodex-runner`.

## Connect a repository to an existing Server

The hosted shared-key path needs no local Server, database, reverse proxy, or
systemd unit:

```bash
cd /path/to/your/repository
webcodex connect https://your-server.example
```

`connect` uses the current directory as the project, generates a shared key
(`wck_...`, printed once), writes an owner-only profile, starts a detached
Runner, and waits until the Server sees both the Runner and the project. Use
the printed `/mcp` URL and key in your MCP client. After a machine reboot,
rerun the same `connect` or use `webcodex agent start --profile <profile>`.

For automation, prefer `--key-file <path>` over `--key`. Do not pass `--key`
and `--key-file` together.

## First production deployment

A first-time operator does not need OAuth, QUIC, or account credentials. The
minimum production path:

1. Use a Linux x64 host with systemd, `sudo`, and a public HTTPS domain or
   trusted tunnel.
2. Install `@yyjeqhc/webcodex`, run `webcodex server init`, and install the
   `webcodex-server` service.
3. Configure the reverse proxy and set `WEBCODEX_PUBLIC_URL` to the exact
   public HTTPS origin.
4. Create a short-lived pairing code on the server and run
   `webcodex login <server-url> --code <code>` on the machine that owns the
   repositories.
5. Install the `webcodex-runner` service on that repository machine.
6. Run `webcodex ops status --strict`; only then import the GPT Actions schema
   or add the MCP connector.

### Server setup

Initialize the Server env file (creates the bootstrap `WEBCODEX_TOKEN` and the
listen/data settings):

```bash
sudo webcodex server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env \
  --public-url https://your-domain.example
```

`server init` creates only the server-side bootstrap/admin token. It does not
create user API tokens or agent tokens.

Install and start the systemd service:

```bash
sudo webcodex server install \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex-server
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex server status --env-file /etc/webcodex/webcodex.env
```

Use `--overwrite` on `server install` only when replacing an existing unit.

### Public HTTPS

Hosted MCP clients and GPT Actions require a public HTTPS URL. Set
`WEBCODEX_PUBLIC_URL` in the Server env file and put a reverse proxy in front
of `127.0.0.1:8080`. Nginx is supported; a named Cloudflare Tunnel is also a
valid front door. The same hostname must carry ordinary HTTPS requests and
`/api/agents/ws` (Cloudflare supports WebSocket upgrades). WebCodex CLI does
not automate reverse proxy or tunnel setup.

### Enroll a repository machine

On the machine that owns the repositories, as the ordinary user who will run
project commands (do not use `sudo`):

```bash
webcodex login https://your-domain.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
webcodex agent install --scope user \
  --config <login-reported-agent-config>
webcodex agent status --scope user \
  --config <login-reported-agent-config>
webcodex ops status --server-url https://your-domain.example \
  --token-file <login-reported-webcodex-user-token> --strict
```

`webcodex login` is the primary client entry: it derives a unique device name,
redeems the pairing code, and writes the client-side `webcodex-user-token` and
an `agent.toml`. `webcodex client enroll` remains the advanced alternative for
an explicit client id or custom output directory.

The pairing code is created server/admin-side:

```bash
webcodex pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --display-name "Friend Name" \
  --ttl-secs 600
```

Copy only the short-lived `wc_pair_*` code to the client. Do not copy
`WEBCODEX_TOKEN`, user API tokens, agent tokens, env files, or complete
`agent.toml` files between machines. Each friend should use a unique
`username`.

## Runner service scopes

`webcodex agent install` supports user or system scope. Non-root users default
to user scope; root defaults to system scope.

**User scope** uses `systemctl --user`, writes the unit under
`$XDG_CONFIG_HOME/systemd/user`, stores config under `$XDG_CONFIG_HOME/webcodex`,
and needs no `sudo`:

```bash
webcodex agent install --scope user --profile workstation
webcodex agent status --scope user --profile workstation
webcodex agent logs --scope user --profile workstation --lines 100
```

An enabled user unit follows that account's user manager. For unattended boot
persistence, an administrator may explicitly run
`sudo loginctl enable-linger <runner-user>`; WebCodex never changes lingering
automatically.

**System scope** uses `/etc/systemd/system` and requires a named non-root
`--user`:

```bash
sudo webcodex agent install \
  --scope system \
  --profile workstation \
  --user <runner-user> \
  --working-directory /home/<runner-user> \
  --config /etc/webcodex/clients/workstation/agent.toml
sudo webcodex agent status --scope system --profile workstation
```

A root Runner is refused unless `--allow-root-runner` is explicit (discouraged).
Use the same `--scope` for every lifecycle command. Example files live in
`deploy/` (`webcodex.env.example`, `webcodex.service.example`,
`webcodex-runner.toml.example`, `webcodex-runner.service.example`,
`nginx.webcodex.example.conf`).

## Docker (server-only)

The repository includes a server-only Dockerfile and Compose deployment that
runs `webcodex-server` plus the admin CLI; it intentionally excludes the
Runner, project repositories, and toolchains.

```bash
git clone https://github.com/yyjeqhc/webcodex.git
cd webcodex
./deploy/docker/bootstrap.sh https://webcodex.example.com
docker compose ps
```

The default binding is `127.0.0.1:8080`. Put an HTTPS reverse proxy in front,
then create a pairing code and enroll the machines that hold your
repositories. The Compose file builds the image from the checked-out source.

## Agent configuration

Client enrollment generates the agent config. Important settings in
`agent.toml`:

| Setting | Notes |
| --- | --- |
| `server_url` | Public WebCodex URL. |
| `token` | Agent token. Do not commit or print it. |
| `client_id` | Stable id used in `agent:<client_id>:<project_id>`. |
| `owner` | Owner principal for this agent. |
| `transport` | Prefer `auto` with `[quic]` configured. |
| `projects_dir` | Directory of project registry files. |
| `[policy]` | Local execution boundary (`allowed_roots`, etc.). |
| `[shell]` | Optional shell profile definitions and bounded persistent-shell limits. |
| `[ssh.resources.<name>]` | Optional named SSH target for Session-bound `run_shell` / `run_job`. |

Policy defaults: missing or empty `allowed_roots` defaults to `$HOME`; an
explicit `allowed_roots` overrides it. Use explicit roots to narrow an agent,
for example to one workspace tree:

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

After editing `agent.toml`, reload the matching service
(`systemctl --user reload webcodex-runner` for user scope, or
`sudo systemctl reload webcodex-runner` for system scope) to apply policy,
shell, and SSH-resource settings. Identity, server/auth, project source,
concurrency, capabilities, and transport changes require a restart.

For a foreground test, run `webcodex-runner --profile workstation`. Advanced
manual config generation uses `webcodex agent init`.

## OAuth2

OAuth2 remains disabled by default when a Server has no public origin. `webcodex server init --public-url https://your-domain.example` writes the public URL, enables OAuth with that exact issuer, and enables the shared-key OAuth bridge for ordinary hosted connect. For a hand-managed env file, the equivalent settings are:

```text
WEBCODEX_PUBLIC_URL=https://your-domain.example
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=https://your-domain.example
WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=true
```

For ordinary repository machines, no managed login is required. Connect with the MCP client's exact callback:

```bash
cd /path/to/your/repository
webcodex connect https://your-domain.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback --project .
```

To let this ordinary shared-key OAuth client offer the fixed optional Computer consent set, explicitly opt in:

```bash
webcodex connect https://your-domain.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback \
  --oauth-computer-permissions --project .
```

The Runner continues to authenticate with the hosted shared key, whose model-facing authority remains the fixed baseline (`runtime/project/job` plus `computer:read` and `computer:control`). `connect` provisions a separate OAuth client bound to that shared-key hash. A fresh client starts with the full baseline; a protected historical client may legitimately retain a narrower baseline subset. `--oauth-computer-permissions` appends only launch, full-display, pointer, clipboard-read, and clipboard-write scopes to that existing subset and never restores baseline scopes that were previously absent; the flag itself grants nothing. The WebCodex authorize page leaves those permissions unchecked and maps browser selections to the fixed scope bundles, constrained by the OAuth request. Launch is selectable only when the request already contains both `computer:read` and `computer:launch`; the Server never fills a missing prerequisite. Existing baseline clients are never widened by ordinary reconnect, and revoked/missing-client rotation preserves the same protected baseline subset. A real ceiling change atomically revokes prior access/refresh/code grants so reauthorization is required. The picker never contains account/admin/Agent, `job:detach`, or future scopes. It reports only safe per-same-Runner capability availability and performs no hidden Computer observation/effect; native/OS permission and current capability are still checked by the runtime call. ChatGPT never receives the shared key, and OAuth access tokens remain invalid on Agent transport.

If a managed-user OAuth identity is specifically required, use the advanced `webcodex login` flow followed by `webcodex connect ... --auth managed-oauth --oauth-redirect-uri ...`; `--user` applies only there.

Create an OAuth client (the `client_secret` is returned only once; only its
hash is stored):

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"ChatGPT MCP","redirect_uris":["https://chatgpt.com/connector/oauth/<callback-id>"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

`allowed_scopes` is the client's persistent delegation ceiling. Computer observation
requires `computer:read`; effectful Computer tools additionally require
`computer:control`. Existing clients are never silently widened when a new scope is
introduced. To explicitly add Computer control to an existing client, send the
**complete** desired non-empty allow-list to the first-party management endpoint:

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/update_scopes \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"client_id":"wc_client_<server-generated-id>","allowed_scopes":["runtime:read","project:read","project:write","job:run","computer:read","computer:control"]}'
```

A changed allow-list atomically revokes that client's existing access tokens, refresh
tokens, and outstanding authorization codes. The OAuth host must then authorize
again and obtain a new token; this is intentional for any permission expansion or
reduction. Re-sending the same canonical allow-list is a no-op and does not revoke
existing grants.

ChatGPT MCP host-file imports use a separate operator trust anchor because their
host-provided temporary download URLs are not restricted to the GPT Action
`files.oaiusercontent.com` hostname policy. After creating the ChatGPT OAuth
client above, take the returned server-generated `wc_client_*` ID and configure
that exact ID (comma-separated for multiple trusted registrations):

```text
WEBCODEX_OAUTH2_TRUSTED_MCP_FILE_CLIENT_IDS=wc_client_<server-generated-id>
```

The server requires the request's OAuth access token to resolve through
`allowed_client_id` to that exact configured, still-active OAuth client record.
Redirect URIs and client display names are not trust identities. Reprovisioning
a ChatGPT OAuth client generates a new client ID and is therefore an explicit
trust rotation: update this setting to the new ID. Ordinary API-token/raw MCP
callers remain ineligible.

List and revoke clients with `POST /api/oauth/clients/list` and
`POST /api/oauth/clients/revoke`. OAuth uses the authorization-code flow;
dynamic client registration, OIDC, and the device-code flow are not
implemented. Keep `offline_access` enabled when a host offers it — it is a
protocol-level refresh-token scope and grants no extra WebCodex permission.

## GPT Actions and MCP

- **MCP:** connect a client to `https://your-domain.example/mcp` with a user
  API token (`wc_pat_*`) or, when OAuth is enabled, the OAuth flow.
- **GPT Actions:** import `https://your-domain.example/openapi.json` into a
  Custom GPT with HTTP Bearer authentication.

Both use the same user API token and the same ToolRuntime. The OpenAPI schema
intentionally excludes users, token, pairing/enrollment, setup, doctor, npm,
server-management, and audit endpoints. Use `webcodex` for those tasks.

MCP and GPT Actions are documented in [MCP.md](MCP.md) and the client-specific
setup in [AI Onboarding](AI_ONBOARDING.md).

## Operations

### Authority mode

`WEBCODEX_AUTHORITY_MODE` controls whether consequential runtime tools
auto-execute or require human approval:

| Value | Behavior |
| --- | --- |
| unset / empty | `trusted_agent` (default for self-hosted single-operator deployments). |
| `trusted_agent` | Project work, shell, jobs, git, and validation auto-execute after hard safety checks, with no approval interruptions. Push/tag/publish/release/deploy still require an explicit user task action. |
| `restricted` | Consequential tools are denied unless a human approves them (`webcodex task approve/deny`). |

Hard safety boundaries (project roots, read-only sessions, path policy,
credential redaction, job cancel semantics) are never relaxed by
`trusted_agent`. `WEBCODEX_PERMISSION_MODE` is removed; if set, configuration
is invalid.

### Operator checks

```bash
webcodex ops status --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE" --strict
webcodex ops agents --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webcodex ops projects --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webcodex ops smoke-preflight --server-url "$SERVER_URL" \
  --token-file "$USER_TOKEN_FILE" --project agent:workstation:my-repo
```

`ops` commands are read-only and never print token or env values. `--strict`
makes a FAIL report exit with status 2. `WARN` means worth reviewing but not a
deploy blocker.

### Smoke checks

Recommended production smoke sequence:

1. `webcodex ops status ... --strict` passes.
2. `POST /api/runtime/status` returns `service=webcodex` and the expected
   public URL.
3. `listAgents` shows at least one online agent.
4. `listProjects` shows `agent:<client_id>:<project_id>` ids.
5. Read-only project tools work on a known project.
6. Write/replace/validate tests are limited to disposable smoke projects.

### Runtime console

The Server serves a host-local browser console at `/console`. It shows project
readiness, the work queue, Workflow Session activity, visible Runners, and recent
mutating activity. For Connector tasks, the same host-local human can send task
guidance, decide pending approvals, cancel work, and Accept or Reject a stable
result. These actions use the same authority boundaries as the CLI; the online
model still cannot accept its own work. The console also shows non-secret client
connection targets, with ChatGPT Developer Mode MCP custom apps as the primary
ChatGPT path. Credentials are deliberately never returned by the console API.

### Runtime job API trust model

`job_status`, `job_log`, `list_jobs`, and `job_tail` are intended for trusted
single-operator deployments. They are not a tenant boundary between mutually
untrusted users. Do not expose one runtime to multiple untrusted users without
adding job-owner isolation; use separate server/runtime instances instead.

## Troubleshooting

See [Troubleshooting](TROUBLESHOOTING.md) for the operational checklist and
common fixes, including existing systemd services, `HTTP reachable: no`,
missing client CLI on `PATH`, server-side pairing vs client-side enrollment,
and `client online: no`.
