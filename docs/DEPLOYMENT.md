# Deployment

[English](DEPLOYMENT.md) | [简体中文](DEPLOYMENT.zh-CN.md)

This guide is for **production and advanced self-hosting**: long-lived Servers, multiple machines/users, system services, reverse proxies, Docker, and operator-managed networking. WebPi Desktop is not a supported installation path in the current tree. For a normal workstation or an existing Server, start with the [Full Setup guide](PERSONAL_SETUP.md); for a few-minute one-repository trial, use the [Quick Trial](QUICK_START.md).

## Components

- `webpi` — the unified CLI for project workflows, Server/Runner lifecycle,
  enrollment, and operations.
- `webpi-server` — the Server process exposing REST, GPT Actions OpenAPI,
  MCP, and Runner endpoints.
- `webpi-runner` — the long-lived worker on the machine that owns the
  repositories.

Execution configuration belongs to the Runner that performs the work. The
retired Server `CODEX_*` settings do not select a coding-agent executable,
approval mode, timeout, or argument allowlist. Configure coding-agent providers
in the Runner's `[acp]` / `[[acp.agents]]` settings instead; see the
[ACP coding-agent guide](agent/acp-coding-agent-run.md). The Server needs its
writable data directory, not a separate legacy `uploads` directory.

## Build and install

The current WebPi tree is source-first. The retired upstream npm downloader/wrapper is not a WebPi installation path and must not be used to fetch another product's binaries. Build the native WebPi programs from the reviewed checkout:

```bash
cargo build --release --locked -p webcodex -p webcodex-cli -p webcodex-runner --bin webpi --bin webpi-server --bin webpi-runner
export PATH="$PWD/target/release:$PATH"
```

This produces `webpi`, `webpi-server`, and `webpi-runner`. Internal Cargo package names intentionally retain `webcodex*` compatibility identifiers; those package names are not the product runtime identity.

On Windows, the repository's dogfood build uses the same three WebPi program names with `.exe`. WebPi-managed Windows Server and Runner services are not supported; run them explicitly in the foreground or use the repository's `webpi.cmd` lifecycle launcher.

## Windows foreground Server and Runner

Windows can run both the Server and Runner directly without Linux, WSL, or a Windows Service. Keep the foreground terminals open while the processes are in use.

On the Windows Server machine, initialize an explicit env file and start the Server from PowerShell:

```powershell
$envFile = Join-Path $HOME ".config\webpi\webpi.env"
$dataDir = Join-Path $HOME ".local\share\webpi"
webpi server init --listen 127.0.0.1:8080 --data-dir $dataDir --env-file $envFile
webpi server run --env-file $envFile
```

Pass the same `--env-file` explicitly when starting the foreground Server; do not rely on a managed-service default. For a local same-machine enrollment, open another PowerShell window and create a short-lived pairing code:

```powershell
webpi pairing create --server-url http://127.0.0.1:8080 --env-file $envFile --username workstation --display-name "Windows Workstation" --ttl-secs 600
```

Then, on the Windows machine that owns the repositories, redeem only that short-lived code and run the generated Runner config in the foreground:

```powershell
webpi login http://127.0.0.1:8080 --code <wc_pair_...> --allowed-root C:\src --project C:\src\my-repo
webpi runner run --config <login-reported-runner-config>
```

When Server and Runner are on different machines, replace the loopback URL with the Server's reachable HTTPS URL and configure the Server listener/public URL plus a trusted reverse proxy or tunnel as described below. Do not copy the Server bootstrap token or env file to the Runner machine. `webpi server install/start/stop/restart/logs/uninstall` and `webpi runner install` remain unsupported on Windows; Ctrl-C or Ctrl-Break ends the foreground runtime.

To keep a Windows Server loopback-only while exposing MCP privately through an OpenAI Secure MCP Tunnel and operating an independent Runner like a normal long-lived Runner, see the [Windows + OpenAI Secure MCP Tunnel deep dive](WINDOWS_OPENAI_TUNNEL.md). It is advanced setup/troubleshooting material; ordinary users do not need it before understanding the full setup path.

## Connect a repository to an existing shared-key Server

The hosted shared-key path needs no local Server, database, reverse proxy, or
systemd unit, but it does require a shared key accepted by that Server (normally
supplied by its operator or recovered from an existing protected profile):

```bash
cd /path/to/your/repository
webpi connect https://your-server.example --key-file /private/path/shared-key
```

`connect` uses the current directory as the project, writes an owner-only
profile, starts a detached Runner, and waits until the Server sees both the
Runner and the project. Use the printed `/mcp` URL and credential in your MCP
client. After a machine reboot, rerun the same `connect` or use
`webpi runner start --profile <profile>`.

This is not first enrollment for a freshly self-hosted Server. For that case,
keep the bootstrap administrator token on the Server and follow the pairing /
`webpi login` flow below. For automation of a shared-key deployment, prefer
`--key-file <path>` over `--key`. Do not pass both.

## First production deployment

A first-time operator does not need OAuth, QUIC, or account credentials. The
minimum production path:

1. Use a Linux x64 host with systemd, `sudo`, and a public HTTPS domain or
   trusted tunnel.
2. Build or stage the reviewed `webpi`, `webpi-server`, and `webpi-runner` binaries, run `webpi server init`, and install the `webpi-server` service.
3. Configure the reverse proxy and set `WEBPI_PUBLIC_URL` to the exact
   public HTTPS origin.
4. Create a short-lived pairing code on the server and run
   `webpi login <server-url> --code <code>` on the machine that owns the
   repositories.
5. Install the `webpi-runner` service on that repository machine.
6. Run `webpi ops status --strict`; only then import the GPT Actions schema
   or add the MCP connector.

### Server setup

Initialize the Server env file (creates the bootstrap `WEBPI_TOKEN` and the
listen/data settings):

```bash
sudo webpi server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webpi \
  --env-file /etc/webpi/webpi.env \
  --public-url https://your-domain.example
```

`server init` creates the selected data directory and the server-side bootstrap/admin token. It does not create user API tokens or Runner tokens. Its next-step guidance preserves the exact env/data paths supplied above.

Install and start the managed systemd socket/service pair:

```bash
sudo webpi server install \
  --env-file /etc/webpi/webpi.env \
  --working-directory /var/lib/webpi \
  --bin /usr/local/bin/webpi-server
webpi server status --env-file /etc/webpi/webpi.env
```

When `--working-directory` is omitted, `server install` uses `WEBPI_DATA` from the selected env file before falling back to the platform default. Install preflight rejects a missing/non-executable binary, missing working directory, unreadable env file, or unknown explicit User/Group before mutating systemd. `server status` likewise derives its default local HTTP probe from that env file's `WEBPI_ADDR`; an explicit `--url` still wins.

The managed Linux layout is intentionally split: `webpi.socket` owns the
fixed `WEBPI_ADDR` listener, while `webpi.service` owns only the Server
process. For ordinary binary replacement after this socket-activated layout is
established, stage the replacement and run:

```bash
sudo systemctl restart webpi.service
```

Do **not** restart `webpi.socket` during the normal Server replacement path.
The socket remains bound while the old Server drains and the new process later
inherits the same listener, eliminating the listener/`ECONNREFUSED` ownership
gap under normal bounded-backlog conditions. On SIGTERM (managed restart/stop)
or Ctrl-C/SIGINT (foreground), WebPi first makes a process-local drain fence
authoritative, then asks Salvo to stop accepting connections and gracefully close
existing HTTP connections. A request admitted before that fence may run for up to
315 seconds and flush its response; a request that still reaches the old process
after the fence receives a retriable HTTP 503 without entering its handler. The
315-second bound is derived from the 300-second ordinary HTTP hard timeout plus a
15-second response/teardown margin. The generated systemd service uses
`TimeoutStopSec=330s`, leaving another 15-second margin so systemd does not
SIGKILL the process before the application-owned bound.

This is an availability-preserving graceful restart, not overlapping generations.
During a long drain, new TCP connections can remain queued in the systemd socket
backlog until the old process exits and the new Server inherits the listener, so
restart latency may approach the finite-request bound. Existing WebSocket,
HTTP keep-alive, and streaming connections may still disconnect/reconnect; there
is no WebSocket continuity or literal zero-interruption guarantee.

Use `--overwrite` on `server install` only when replacing an existing managed
pair. Migrating an already-active legacy direct-bind `webpi.service` is a
one-time migration boundary: the installer fails closed rather than competing
for the live address. Stop the legacy Server first, then rerun the install with
`--overwrite`. This boundary does not provide a gap-free first migration.

### Tool invocation tracing

Tool-request tracing is an **operator diagnostic** and is disabled by default. Use metadata mode for lightweight lifecycle diagnostics; use `full` only when you explicitly need request/response payload capture:

```text
WEBPI_TOOL_REQUEST_TRACE=full
WEBPI_TOOL_REQUEST_TRACE_DIR=/var/lib/webpi/tool-request-traces
WEBPI_TOOL_REQUEST_TRACE_RETENTION_HOURS=168
WEBPI_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES=2147483648
```

`full` tracing can contain source text, patches, command/script input and output, user messages, and secrets that were themselves present inside captured tool payloads. Protect the trace directory like other sensitive diagnostic data and keep retention/budget bounded. Trace capture is diagnostic only; trace-write failure does not change tool execution.

The exact trace file layout, correlation ids, model-facing forensic reader, queueing, compression, and payload-validation rules are implementation/maintainer details and are intentionally omitted here.

### Public HTTPS

Hosted MCP clients and GPT Actions require a public HTTPS URL. Set
`WEBPI_PUBLIC_URL` in the Server env file and put a reverse proxy in front
of `127.0.0.1:8080`. Nginx is supported; a named Cloudflare Tunnel is also a
valid front door. The same hostname must carry ordinary HTTPS requests and
`/api/agents/ws` (Cloudflare supports WebSocket upgrades). WebPi also uses
this configured origin as the MCP App `ui.domain` for Computer and Result
resources; it never substitutes a WebPi-operated domain for self-hosted
servers. When no public URL is configured the optional field is omitted.
WebPi CLI does not automate reverse proxy or tunnel setup.

### Enroll a repository machine

On the machine that owns the repositories, as the ordinary user who will run
project commands (do not use `sudo`):

```bash
webpi login https://your-domain.example --code <wc_pair_...> \
  --allowed-root "$HOME/git" \
  --project "$HOME/git/my-repo"
webpi runner install --scope user \
  --config <login-reported-runner-config>
webpi runner status --scope user \
  --config <login-reported-runner-config>
webpi ops status --server-url https://your-domain.example \
  --token-file <login-reported-webpi-user-token> --strict
```

`webpi login` is the canonical client entry: it derives a unique device name, redeems the pairing code, and writes the client-side `webpi-user-token` and a `runner.toml`. `--allowed-root` grants registration authority only; `--project` names the actual existing workspace to register. The generated `project_registry_dir` is a registry directory, not the workspace root. If login is performed without `--project`, use `webpi project register --config <login-reported-runner-config> /path/to/repo` before project-bound work. Use the documented `login --device` and `--dir` options when an explicit device identity or alternate local base directory is required; there is no separate compatibility enrollment command.

The pairing code is created server/admin-side:

```bash
webpi pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webpi/webcodex.env \
  --username friendname \
  --display-name "Friend Name" \
  --ttl-secs 600
```

Copy only the short-lived `wc_pair_*` code to the client. Do not copy
`WEBPI_TOKEN`, user API tokens, Runner tokens, env files, or complete
`runner.toml` files between machines. Each friend should use a unique
`username`.

## Runner service scopes

`webpi runner install` supports user or system scope. Non-root users default
to user scope; root defaults to system scope.

**User scope** uses `systemctl --user`, writes the unit under
`$XDG_CONFIG_HOME/systemd/user`, stores config under `$XDG_CONFIG_HOME/webpi`,
and needs no `sudo`:

```bash
webpi runner install --scope user --profile workstation
webpi runner status --scope user --profile workstation
webpi runner logs --scope user --profile workstation --lines 100
```

An enabled user unit follows that account's user manager. For unattended boot
persistence, an administrator may explicitly run
`sudo loginctl enable-linger <runner-user>`; WebPi never changes lingering
automatically.

**System scope** uses `/etc/systemd/system` and requires a named non-root
`--user`:

```bash
sudo webpi runner install \
  --scope system \
  --profile workstation \
  --user <runner-user> \
  --working-directory /home/<runner-user> \
  --config /etc/webpi/clients/workstation/runner.toml
sudo webpi runner status --scope system --profile workstation
```

A root Runner is refused unless `--allow-root-runner` is explicit (discouraged).
Use the same `--scope` for every lifecycle command. Current non-secret examples in `deploy/` include `webpi.service.example`, `webpi-runner.toml.example`, `webpi-runner.service.example`, and `nginx.webpi.example.conf`. Generate the real `/etc/webpi/webpi.env` with `webpi server init`; do not copy a credential-bearing env file from another installation.

## Docker (server-only)

The repository includes a server-only Dockerfile and Compose deployment that runs `webpi-server` plus the WebPi admin CLI; it intentionally excludes the Runner, project repositories, and toolchains. The current release workflows are fail-closed, so this tree does **not** promise an official public WebPi registry image or downloadable release bootstrap.

`compose.yaml` requires an explicit reviewed `WEBPI_SERVER_IMAGE`; there is no implicit upstream or `latest` fallback. From a reviewed source checkout, the supported source-build path is:

```bash
./deploy/docker/bootstrap.sh https://webpi.example.com --build-from-source
```

For later source rebuilds in the same checkout, keep the image choice explicit:

```bash
WEBPI_SERVER_IMAGE=webpi-server-local:reviewed \
  docker compose -f compose.yaml -f compose.build.yaml up -d --build
```

The bootstrap is recoverable rather than all-or-nothing. It validates the strict HTTPS origin, Compose/source assets, host port, Docker/Compose availability, and chosen image before committing an administrator secret. It advances the private `.webpi-bootstrap.receipt` through durable stages; the receipt stores hashes and state, not the administrator token. Its private `.env` is written through a restricted temporary file and atomically renamed. Success is reported only after the Compose healthcheck and `/openapi.json` verification succeed.

If source-build bootstrap is interrupted or health validation fails, keep its private state and use the same checked-out script:

```bash
./deploy/docker/bootstrap.sh status
./deploy/docker/bootstrap.sh resume
# Remove runtime effects while preserving committed recovery state:
./deploy/docker/bootstrap.sh rollback
```

The container binds the host side to loopback by default. Put a reviewed HTTPS reverse proxy or tunnel in front, verify the local Server first, and only then expose the public origin. Public image publication remains a separate release task with its own provenance and anonymous-pull gates.

## Runner configuration

Client enrollment generates the Runner config. Important settings in
`runner.toml`:

| Setting | Notes |
| --- | --- |
| `server_url` | Public WebPi URL. |
| `token` | Runner credential. Do not commit or print it. |
| `client_id` | Stable id used in `agent:<client_id>:<project_id>`. |
| `owner` | Owner principal for this Runner. |
| `transport` | Prefer `auto` with `[quic]` configured. |
| `project_registry_dir` | Directory of project registry files. |
| `[policy]` | Local execution boundary (`allowed_roots`, etc.). |
| `[skills].roots` | Optional absolute Runner-local live Skill roots. WebPi does not modify them; supported scripts may execute via `run_skill_resource`; content is not copied into the managed Skill Store. |
| `[instructions].files` | Optional absolute Runner-local instruction files applied to every Project on this Runner. No implicit default path; the list is hot-reloadable and file contents are live. |
| `[shell]` | Optional shell profile definitions and bounded persistent-shell limits. |
| `[ssh.resources.<name>]` | Optional named SSH target for Session-bound `run_shell` / `run_job`. |

Policy defaults: missing or empty `allowed_roots` defaults to `$HOME`; an
explicit `allowed_roots` overrides it. Use explicit roots to narrow a Runner,
for example to one workspace tree:

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

After editing the already-running Runner's startup-bound `runner.toml`, use
`runner_config_check(client_id=...)`, then pass its `current_generation` to
`runner_config_reload(client_id=..., expected_generation=...)`, then inspect
`runtime_status(client_id=...)`. Check never activates the candidate; reload never
writes the file. Invalid candidates preserve the active snapshot/generation, and
`restart_required_fields` names startup-only changes that are not claimed live
until restart. Unix service reload/SIGHUP remains a compatibility trigger for the
same reload primitive, but is not required for first-class config control. Identity,
server/auth, project source, concurrency, capabilities, and transport changes
remain restart-only where reported.

`[instructions].files` is explicitly hot-reloadable: after check/reload, new
Project bootstraps use the new list without Runner restart. Changing the contents
of an already-configured instruction file needs no config reload at all; the next
bootstrap re-reads it. These configured instruction paths do not widen
`[policy].allowed_roots` or ordinary Project filesystem authority, and native
absolute paths are not exposed in startup projection. The current manual
`runner.toml` configuration is Runner-level and applies to every Project on that
Runner; Desktop selection/upload UI is future work.

`[plugins]` is live-reloadable: generic Runner config reload and `plugin_tool reload`
share the same Plugin candidate admission/atomic-commit primitive. Plugin provider
tools remain Runner-local capabilities behind `plugin_tool`; they are never promoted
into outer MCP `tools/list` and do not require a Runner restart for discovery.

For a foreground test, run `webpi-runner --profile workstation`. Advanced
manual config generation uses `webpi runner init`.

## OAuth2

OAuth2 remains disabled by default when a Server has no public origin. `webpi server init --public-url https://your-domain.example` writes the public URL, enables OAuth with that exact issuer, and enables the shared-key OAuth bridge for ordinary hosted connect. For a hand-managed env file, the equivalent settings are:

```text
WEBPI_PUBLIC_URL=https://your-domain.example
WEBPI_OAUTH2_ENABLED=true
WEBPI_OAUTH2_ISSUER=https://your-domain.example
WEBPI_OAUTH2_SHARED_KEY_BRIDGE=true
```

For ordinary repository machines, no managed login is required. Connect with the MCP client's exact callback:

```bash
cd /path/to/your/repository
webpi connect https://your-domain.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback --project .
```

To let this ordinary shared-key OAuth client offer the fixed optional Computer consent set, explicitly opt in:

```bash
webpi connect https://your-domain.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback \
  --oauth-computer-permissions --project .
```

The Runner continues using its hosted credential while the MCP client receives a separate OAuth credential. `--oauth-computer-permissions` and `--oauth-local-mcp` are explicit opt-ins for optional capabilities; ordinary reconnect never silently adds them. A real OAuth permission change requires the client to authorize again. ChatGPT never receives the Runner/shared-key credential, and OAuth tokens are not valid on Runner transport.

If a managed-user OAuth identity is specifically required, use the advanced `webpi login` flow followed by `webpi connect ... --auth managed-oauth --oauth-redirect-uri ...`; `--user` applies only there.

Create an OAuth client (the `client_secret` is returned only once; only its
hash is stored):

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBPI_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"ChatGPT MCP","redirect_uris":["https://chatgpt.com/connector/oauth/<callback-id>"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

`allowed_scopes` limits what an OAuth client may request. Existing clients are not silently widened when WebPi adds new permissions. To change an existing client, submit the complete desired non-empty allow-list to `POST /api/oauth/clients/update_scopes`. A real change invalidates the client's old OAuth grants and requires reauthorization; submitting the same canonical list is a no-op. See [Authentication](AUTH_MODEL.md#oauth2) for the security model.

If ChatGPT MCP host-file import is enabled, configure the exact server-generated OAuth client id in `WEBPI_OAUTH2_TRUSTED_MCP_FILE_CLIENT_IDS`. Reprovisioning the client creates a new id, so update this setting as part of that explicit trust rotation. Client display names and redirect URIs are not substitutes for the configured client id.

A separate local-only exception exists for an operator-controlled Server that is
bound to loopback and reached through OpenAI Secure Tunnel with a locally
injected user API token. Set
`WEBPI_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=true` to trust ChatGPT
host-file rewrites on that path. The flag is ignored for non-loopback binds and
for non-user API credentials; leave it unset on network-accessible Servers.

List and revoke clients with `POST /api/oauth/clients/list` and
`POST /api/oauth/clients/revoke`. OAuth uses the authorization-code flow;
dynamic client registration, OIDC, and the device-code flow are not
implemented. Keep `offline_access` enabled when a host offers it — it is a
protocol-level refresh-token scope and grants no extra WebPi permission.

## GPT Actions and MCP

- **MCP:** connect a client to `https://your-domain.example/mcp` with a user
  API token (`wc_pat_*`) or, when OAuth is enabled, the OAuth flow. MCP remains
  the primary ChatGPT integration.
- **GPT Actions:** import `https://your-domain.example/openapi.json` into a
  Custom GPT with HTTP Bearer authentication. On a generic runtime Server this
  projects the same canonical Adaptive Runtime model surface: current Adaptive
  Direct tools become direct snake_case Action operations and supported long-tail
  tools use `call_runtime_tool`. MCP-only protocol presentation is excluded.

After upgrading from the older generic Action facade, re-import `/openapi.json`
to pick up the canonical operation names. Existing legacy REST routes may remain
for compatibility but are not part of the new model-facing schema.

Both integrations enter the same ToolRuntime authority path. GPT Actions does not
introduce a separate scope, Project-authority, permission, Runner-capability, or
retry policy. Project-scoped `share`/`run` deployments expose the same ordinary
Adaptive Runtime while ProjectGrant visibility keeps them bound to their Project.

See [GPT Actions](GPT_ACTIONS.md), [MCP](MCP.md), and [AI Onboarding](AI_ONBOARDING.md).

## Operations

### Authority mode

`WEBPI_AUTHORITY_MODE` controls whether consequential runtime tools
auto-execute or require human approval:

| Value | Behavior |
| --- | --- |
| unset / empty | `trusted_agent` (default for self-hosted single-operator deployments). |
| `trusted_agent` | Project work, shell, jobs, git, and validation auto-execute after hard safety checks, with no approval interruptions. Push/tag/publish/release/deploy still require an explicit user task action. |
| `restricted` | Consequential runtime tools are denied by permission policy. There is no separate Connector command-approval queue. |

Hard safety boundaries (project roots, read-only sessions, path policy,
credential redaction, job cancel semantics) are never relaxed by
`trusted_agent`. Legacy `WEBPI_PERMISSION_MODE` supports the unambiguous
`dev_auto_approve` → `trusted_agent` and `require_approval` → `restricted` mappings.
Unknown or conflicting old/new settings remain invalid.

### Operator checks

```bash
webpi ops status --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE" --strict
webpi ops runners --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webpi ops projects --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webpi ops smoke-preflight --server-url "$SERVER_URL" \
  --token-file "$USER_TOKEN_FILE" --project agent:workstation:my-repo
```

`ops` commands are read-only and never print token or env values. `--strict`
makes a FAIL report exit with status 2. `WARN` means worth reviewing but not a
deploy blocker.

### Smoke checks

Recommended production smoke sequence:

1. `webpi ops status ... --strict` passes.
2. `POST /api/runtime/status` returns `service=webcodex` and the expected
   public URL.
3. `list_runners` shows at least one online Runner.
4. `listProjects` shows `agent:<client_id>:<project_id>` ids.
5. Read-only project tools work on a known project.
6. Write/replace/validate tests are limited to disposable smoke projects.

### Runtime console

The Server serves the Runtime Console at `/runtime`. It projects ordinary runtime,
Project, Runner, Job, Workflow Session, collaboration, and recent activity state
through the same authorization path used by ToolRuntime. Project-scoped credentials
see only their ProjectGrant-visible Runner/Project set; knowing another Project or
Runner id does not widen visibility. The old Connector Project Review Console at
`/console` and its task/result/approval APIs are removed. Credentials are never
returned by the Runtime Console API.

### Runtime job API trust model

`observe_jobs`, `list_jobs`, and `job_tail` are intended for trusted
single-operator deployments. They are not a tenant boundary between mutually
untrusted users. Do not expose one runtime to multiple untrusted users without
adding job-owner isolation; use separate server/runtime instances instead.

## Troubleshooting

See [Troubleshooting](TROUBLESHOOTING.md) for the operational checklist and
common fixes, including existing systemd services, `HTTP reachable: no`,
missing client CLI on `PATH`, server-side pairing vs client-side enrollment,
and `client online: no`.
