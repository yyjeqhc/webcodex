# WebCodex CLI

The `webcodex` command is the unified operator and developer interface. It
covers project setup, Server and Runner lifecycle, device enrollment, token
management, and read-only operator checks.

Remote operations (users, tokens, pairing, operator checks) go through the
Server HTTP API, and the CLI is a convenient client for them. Local operations
(project setup, service management, task review decisions) run directly on the
host machine and are not available through the Server API.

The CLI produces three binaries when built from source:

- `webcodex` — the unified command documented here.
- `webcodex-server` — the Server process (started and supervised with
  `webcodex server ...`).
- `webcodex-runner` — the Runner process that executes project work (started
  and supervised with `webcodex runner ...`).

`webcodex --help` lists the top-level namespaces. The sections below explain what each namespace is for. This is the complete CLI reference; ordinary users do not need to understand every command, credential, or internal configuration field before their first successful setup.

For everyday development, follow the [Full Setup guide](PERSONAL_SETUP.md) and use a regular Server + Runner. `webcodex share` is the explicit **temporary one-project trial/share** entry on Linux, macOS, and Windows; it prepares the temporary project environment, Server, Runner, and optional Tunnel for that foreground run and ends when the process exits. Running bare `webcodex` in an interactive Git checkout remains a Linux/macOS convenience shortcut into that same temporary `share` flow.

## Command map

### Environment configuration

`webcodex environment` and Desktop call the same setup core. This namespace configures the machine's persistent environment; the existing project-level `webcodex setup` command keeps its original meaning. Installer availability and native acceptance are tracked in [Unified installation](unified-installation.md) and [Deployment validation](unified-deployment-validation.md).

| Command | Purpose |
| --- | --- |
| `webcodex environment configure` | Interactively choose create/join and project/skip, then collect the required address, authentication, and system authorization. |
| `configure --create --project PATH` / `configure --create --no-project` | Create or resume local Server + Runner / Server-only setup. |
| `configure --join URL --project PATH --code-stdin` | Join with a project; read one Runner pairing code from stdin, then save credentials, register the project, install services, and verify readiness. |
| `configure --join URL --no-project --token-file PATH` | Join as a viewer with a protected user API credential; creates no local Runner identity or service. Omit the file option for hidden terminal input. |
| `resume` | Reconcile saved progress and complete missing steps without silently rebinding the environment. |
| `invite` | Create a short-lived Runner invitation on the environment's local Server; the displayed code is sensitive. |
| `add-project PATH` | Reuse the existing Runner identity; a viewer must complete Runner enrollment first. |
| `status --json` / `doctor --json` | Inspect saved configuration, Server reachability, Runner/project readiness, and structured diagnostics. |
| `start COMPONENT` / `stop COMPONENT` / `restart COMPONENT` | Explicitly manage an environment-owned `server`, `runner`, or `tunnel`. |
| `repair-user-credential [--token-file PATH]` | Verify and replace the saved user credential without pairing or changing service state. |
| `repair-credential runner` | Repair Windows SCM account credentials through hidden input. |

Use `webcodex environment --help` for the complete namespace, including Tunnel profiles, explicit legacy migrations, and installer upgrade/recovery commands. Public environment commands support `--json` and `--environment-dir PATH`. Do not put tokens, pairing codes, or service passwords in command arguments. When pairing redemption is uncertain, read the recovery diagnostic before explicitly supplying a replacement through `resume --new-pairing-code --code-stdin`; do not replay the old code automatically.

### Project / local workflow

These commands work on the current Git project.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex` (no command) | Interactive temporary-share shortcut | Only auto-dispatches to `share` on Linux/macOS when stdin/stdout are terminals and the current directory is inside a Git checkout; otherwise normal help is shown. |
| `webcodex share` | Temporarily share the current project with ChatGPT/MCP | Quick trial/short-lived sharing path on Linux/macOS/Windows; includes temporary setup, local Server + Runner, `cloudflare|openai|none`, and bounded foreground cleanup. Use `PERSONAL_SETUP` for full daily use. |
| `webcodex connect <server>` | Connect the current project to an existing Server | Long-lived path when you already have a Server URL; defaults to hosted shared-key. |
| `webcodex status` | Concise project coding readiness | Short summary; `doctor` is the full diagnostic check. |
| `webcodex doctor` | Read-only readiness checks for the current project | Diagnostics/manual workflow; reports a stable `next action`. |
| `webcodex setup` | Configure the current Git project without starting it | Local-only/manual workflow; creates private state and a Project Credential. |
| `webcodex run` | Start the project-bound loopback Server and local Runner | Local-only/manual workflow; foreground, Ctrl-C stops both. |
| `webcodex disconnect [--project PATH] [--profile NAME]` | Remove one hosted project registration | Exact inverse of `connect` for that repository; never removes the repository or `.git`. |

`webcodex share --auth query-token` is an explicit temporary-share compatibility mode for MCP clients that cannot configure a Bearer header. It accepts the exact share Project Credential only on `/mcp?token=...`, prints a URL-encoded sensitive MCP URL, and tells the client to use No authentication. The mode is disabled for ordinary Server/runtime requests, does not accept PAT/OAuth/shared-key/Runner credentials through the query, and is rejected with `--tunnel openai`. Treat the complete URL as a credential because URL queries may be retained by clients, proxies, clipboards, or access logs. The default remains `--auth bearer`.

`webcodex share --auth oauth --oauth-redirect-uri <exact-callback>` uses OAuth 2.0 Authorization Code with PKCE S256. The OAuth client ID/secret are persisted in protected project state for that project + callback, while the temporary OAuth grants are valid only for the current `share` run. Restarting `share` therefore invalidates old OAuth grants without changing the project. OAuth access tokens are never accepted on Runner transport.

Quick Tunnel origins remain temporary. For an operator-managed stable HTTPS origin, use `--tunnel none --public-url https://share.example` and route that origin to the loopback WebCodex Server yourself; `--public-url` advertises the external origin/issuer and does not create a proxy or tunnel.

`webcodex share --tunnel openai` is the explicit OpenAI Secure MCP Tunnel provider. It requires `CONTROL_PLANE_TUNNEL_ID` plus a Restricted `CONTROL_PLANE_API_KEY` with Tunnels Read + Use and currently supports only `--auth bearer`. WebCodex resolves pinned OpenAI `tunnel-client` v0.0.12 from `WEBCODEX_TUNNEL_CLIENT_BIN`, `PATH`, or a verified managed download; it runs `doctor` before the daemon and waits for `/readyz`. The temporary WebCodex Bearer is written only to the private share directory and referenced by `tunnel-client` through a file-backed MCP `Authorization` header. ChatGPT therefore uses Connection: Tunnel + No authentication. `OPENAI_ADMIN_KEY` and `OPENAI_API_KEY` are explicitly removed from the long-lived daemon environment; the Runtime API key remains the control-plane authority.

For public `share`, WebCodex best-effort copies only the MCP URL to the clipboard and does not copy the temporary credential in the default Bearer/OAuth modes. The explicit `--auth query-token` mode instead copies the sensitive tokenized URL by design and says so in its status output. Interactive Linux/macOS terminals also offer an Enter shortcut to open ChatGPT App settings. Clipboard/browser integration is convenience-only and never gates runtime readiness. Use `--no-copy-url` to suppress clipboard access.

For supervised machine integration, `webcodex share --json --stop-on-stdin-eof` keeps the same foreground lifecycle but also treats the supervising parent's closed stdin as a stop request. This lets Desktop or another structured process owner ask `share` to clean up its own temporary Server, Runner, and Tunnel without shell-command signaling. The flag is rejected outside `--json` mode.

`webcodex connect <server> --auth oauth --oauth-redirect-uri <exact-callback>` is the ordinary hosted OAuth path. The Runner keeps its existing hosted credential while the MCP client uses OAuth. Add `--oauth-computer-permissions`, `--oauth-local-mcp`, or `--oauth-local-ssh` only when those optional capabilities are actually needed; they are explicit permission changes and can require reauthorization. `--oauth-local-ssh` grants the MCP client the optional `ssh:local` authority needed by the model-facing `ssh_resource` onboarding tool; it does not expose SSH credentials or make a registered resource active without the Runner restart reported by that tool. See [MCP](MCP.md#oauth2) for client setup and [Authentication](AUTH_MODEL.md#oauth2) for the security model.

The advanced managed identity flow remains available as `--auth managed-oauth --oauth-redirect-uri <exact-callback>` and requires `webcodex login`; `--user` applies only to that mode.

`disconnect` matches the canonical repository path, not a basename or project id. If the same
repository is registered in more than one hosted profile, specify `--profile`. With a live
managed Runner it performs a structured unregister before removing the local registration;
with a stopped Runner it removes only the exact local project registration. Other projects,
profile credentials, and `runner.toml` are preserved.

After connecting an MCP coding client, see the [Coding Workflow](CODING_WORKFLOW.md) for the
canonical `work_on_project` model bootstrap, behavioral guidance, validation, and closeout
evidence.

### Enrollment

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex login <server-url> --code <wc_pair_...> [--project PATH]` | Log this device into a Server with a one-time code | Normal managed enrollment entry. `--project` selects the actual project, `--allowed-root` names a parent from which more projects may be added later, and `--print-mcp-config` explicitly prints sensitive ChatGPT MCP connection values. |
| `webcodex project register --config PATH <PROJECT>` | Add another project to an existing Runner | Persists that Runner's project configuration without requiring the Server to be online; follow the command output if an already-running Runner needs a reload. |
| `webcodex pairing create` | Server/admin side: create a short-lived pairing code | Needs server bootstrap/admin auth. |
| `webcodex logout <server-url> [--user USER|--all]` | Remove this device's credentials for a Server | With one saved user, the user is selected automatically. With multiple saved users, choose one with `--user USER` or explicitly choose all with `--all`; deletion still uses the existing confirmation/`--yes` flow. |

Local `login --project`, `project register`, and Desktop project selection grant
the exact canonical directory of an existing project to the saved Runner `allowed_roots`
when needed; they do not grant its parent directory or entire share. Symlinks
that require new authority must be selected by their canonical target path.
Windows UNC directories are supported; dangerous device namespaces are rejected. Restart an
already-running Runner when the command reports that a reload is required.
Model-facing registration requires preconfigured network authority even with
`allow_cwd_anywhere = true`; use a direct path without `..` components.
Creating new network projects with `create_project` remains unsupported.

Root login also provides a foreground Runner command, with a warning that project
commands run as root. On Linux its system-service installation command includes
`--allow-root-runner`. The same login credentials are reused.


### Runner lifecycle

The Runner executable is `webcodex-runner`. Its canonical CLI lifecycle namespace
is `runner`: `webcodex runner ...` manages the `webcodex-runner` process and
service. `webcodex` and `webcodex-runner` remain separate executables.

| Command | Purpose |
| --- | --- |
| `webcodex runner init` | Generate a `runner.toml` config manually |
| `webcodex runner install` | Install, enable, and start the Runner service |
| `webcodex runner run` | Run `webcodex-runner` in the foreground |
| `webcodex runner start` | Start a hosted background Runner or installed profile service |
| `webcodex runner stop` | Stop it |
| `webcodex runner restart` | Restart it |
| `webcodex runner status` | Check Runner lifecycle, config, and connectivity |
| `webcodex runner logs` | Read Runner logs (bounded) |
| `webcodex runner uninstall` | Remove the service unit (requires `--confirm`) |

Service commands accept `--scope user|system`. Non-root users default to user
scope; root defaults to system scope. Profiles created by `webcodex connect`
keep their detached-process behavior when `--scope` is omitted.

### Server

| Command | Purpose |
| --- | --- |
| `webcodex server init` | Initialize/update the Server env file and selected data directory (creates the bootstrap token) |
| `webcodex server install` | Install the Linux systemd `webcodex.socket` + `webcodex.service` pair; default WorkingDirectory follows selected env `WEBCODEX_DATA` |
| `webcodex server run [--env-file PATH]` | Run `webcodex-server` in the foreground (direct bind); `--env-file` passes the exact path via `WEBCODEX_ENV_FILE` |
| `webcodex server start` / `stop` | Start or stop socket activation and the Server process coherently |
| `webcodex server restart` | Restart only the Server process; keep the managed listener socket active |
| `webcodex server status` | Check authoritative socket/service state, HTTP reachability, and build revisions |
| `webcodex server logs` | Read the Server service journal |
| `webcodex server uninstall` | Stop, disable, and remove the managed socket/service pair |

## Controller (WSL/Linux V0)

webcodex controller is the local terminal control plane for WSL/Linux. V0 does not modify Desktop and does not change the lower-level Server, Runner, or OpenAI Tunnel process contracts; the Controller owns and supervises those existing processes as their parent.

    webcodex controller init
    webcodex controller doctor
    webcodex controller install

The default configuration is ~/.config/webcodex/controller.toml. A running Controller exposes a local Unix Socket at $XDG_RUNTIME_DIR/webcodex/controller.sock, falling back to a per-user /tmp runtime directory when XDG_RUNTIME_DIR is unavailable.

Common operations:

    webcodex controller start
    webcodex controller status
    webcodex controller restart
    webcodex controller restart server
    webcodex controller restart runner
    webcodex controller restart tunnel
    webcodex controller logs --lines 100
    webcodex controller stop

V0 uses the local Server as the dependency root, so enabling Runner or Tunnel also requires Server to be enabled. The configured Runner `server_url` must match that Controller-managed loopback Server; remote Server topology is rejected before the Runner starts or any local Server credential is used. The Controller refuses to take ownership when the existing webcodex.service, webcodex.socket, or webcodex-runner.service is already active, avoiding duplicate process ownership.

`controller install` installs a user service at `~/.config/systemd/user/webcodex-controller.service` by default and manages the Controller lifecycle through `systemctl --user`. The optional service environment file defaults to `~/.config/webcodex/controller.env`; when OpenAI Tunnel is enabled for background startup, `CONTROL_PLANE_TUNNEL_ID` and `CONTROL_PLANE_API_KEY` can be placed there. `controller doctor` checks those same two credentials from the process environment or the selected `--environment-file`. Tunnel control-plane credentials are not inherited by the managed Server or Runner children. The Controller service does not install separate Server/Runner services; those lower-level processes remain children owned by the Controller.

On Windows, `server init`, foreground `server run`, and explicit `share` are supported. The managed service lifecycle (`install`, `start`, `stop`, `restart`, `logs`, `uninstall`) remains Linux-only.

`webcodex server install --service-file /path/name.service` derives the sibling
`/path/name.socket`. Use the same `--service-file` on `start`, `stop`, `restart`,
`status`, `logs`, and `uninstall` to manage or inspect that custom pair; omitting
it targets the default `webcodex.service` / `webcodex.socket` pair.

For Runner config terminology, `project_registry_dir` is the directory of Project registry TOML files, not a workspace root. `[policy].allowed_roots` bounds which filesystem paths may be registered; a Project record names the actual workspace.

### Operations (read-only operator checks)

| Command | Purpose |
| --- | --- |
| `webcodex ops status` | Summarize runtime, tools, Jobs, Runners, and Projects |
| `webcodex ops runners` | Compact Runner fleet status |
| `webcodex ops runner --client-id <id>` | Exact read-only Runner registration/build status |
| `webcodex ops projects` | Project inventory and smoke suitability |
| `webcodex ops smoke-preflight --project <id>` | Preflight one project for a deploy smoke |

`ops` commands are read-only. They accept `--server-url`, `--token-file`,
`--env-file`, `--token`, `--json`, and `--strict`. Prefer `--token-file` for
operator use; `--token` can leak into shell history or process lists. `--strict`
makes a FAIL report exit with status 2.

### Review and runtime activity

The legacy `webcodex task` namespace has been removed with the separate Connector Task/Result/Approval lifecycle. Local `webcodex run` prints the Runtime Console URL (`/runtime`). Runtime review uses the canonical Workflow Session, Job, Git/diff, `show_changes`, and `finish_coding_task` paths rather than a host-side result accept/reject queue.

### Credentials and accounts

Admin user/token operations are Server-API-backed. `auth status` reads local
device connection state, while the `create-local` commands generate credentials
locally and register only their hashes with the Server.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex auth status` | Show which servers this device is logged in to | Read-only; supports `--dir` and `--json`. |
| `webcodex users create` | Create a user; `--issue-credential` returns a one-time account credential | Server/admin side; uses `--server-url`. |
| `webcodex users list` | List users | |
| `webcodex tokens create-local` | Locally generate a `wc_pat_*` personal API token and register its hash | Uses `--server-url`, `--username`, and an account credential. |
| `webcodex tokens create` | Admin: create a PAT server-side | Uses `--server-url`. |
| `webcodex tokens generate` | Offline token material generation | Does **not** register with any Server. |
| `webcodex tokens list` / `revoke` / `register-hash` | List or revoke PATs; register an externally computed hash | Admin side; uses `--server-url`. |
| `webcodex runner-tokens create-local` | Locally generate a `wc_agent_*` Runner token and register its hash | Uses `--server-url` and binds to `--client-id`. |
| `webcodex runner-tokens create` / `list` / `revoke` / `register-hash` | Admin variants | |

All Server-targeting credential commands use the canonical `--server-url` spelling.
Local `tokens create-local` / `runner-tokens create-local` use `--username` plus an
account credential; admin token management uses the same plural namespaces.

### Advanced and compatibility commands

These commands cover unusual setups; the recommended paths above are the
normal entry points.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex pairing create` | Server/admin side: create a short-lived pairing code | Needs server bootstrap/admin auth. |
| `webcodex tokens generate` | Offline token material generation | Registers nothing; pair the output with `tokens register-hash` if the hash must be registered server-side. |
| `webcodex tokens register-hash` | Admin: register an externally computed PAT hash | Uses `--server-url`; for offline-generated material. |
| `webcodex runner-tokens register-hash` | Admin: register an externally computed Runner-token hash | Uses `--server-url`; for offline-generated material. |

## Terminology

- **Server** — authenticates callers, stores shared runtime state, and routes work.
- **Runner** — runs repository work on the machine that owns the code.
- **Project** — one repository/workspace registered by a Runner.
- **Job** — a command or validation that continues after the initiating call returns.
- **Workflow Session** — bounded coding evidence/continuity used by the runtime. Ordinary users normally do not manage its internal protocol fields.

Some compatibility-facing names still contain `agent`, notably `wc_agent_*` and `agent:<client_id>:<project_id>`. They refer to Runner-era compatibility, not the separate Durable Agent domain. Other process/protocol identifiers remain internal. New prose should say **Runner** unless it is quoting one of those public names.

## Credentials: which token do I need?

WebCodex separates bootstrap administration, account onboarding, runtime API
access, and Runner connectivity. Do not reuse one credential across surfaces.
The full model is in [AUTH_MODEL.md](AUTH_MODEL.md); the table below is the
quick answer.

| Credential | Prefix | Created by | Used for | Do not use for |
| --- | --- | --- | --- | --- |
| Server bootstrap token | (env `WEBCODEX_TOKEN`) | `webcodex server init` | server/admin setup, user creation, pairing | GPT Actions, MCP, Runner, daily use |
| Shared key | `wck_...` | `webcodex connect` (generated once) | hosted shared-key MCP + Runner | production IAM |
| Project Credential | (private file) | `webcodex setup` | one ProjectGrant's ordinary runtime API/MCP access | other ProjectGrants, admin, Runner transport |
| Account credential | `wc_acct_...` | `webcodex users create --issue-credential` | local token creation | GPT Actions, MCP, Runner |
| Personal API token (PAT) | `wc_pat_...` | `webcodex tokens create-local` | GPT Actions, MCP, REST API | Runner connectivity |
| Runner token | `wc_agent_...` | `webcodex runner-tokens create-local` | `webcodex-runner` transport only | MCP, REST, GPT Actions |
| OAuth access token | `wc_oat_...` | OAuth2 authorization flow | GPT Actions / MCP when OAuth is enabled | — |

### Practical credential rules

- Normal managed setup: `webcodex login` creates the local user/API and Runner credentials; use the paths and MCP values it reports.
- Existing shared-key Server: use the operator-provided `wck_...` with `webcodex connect`.
- Project-first/manual setup: keep the Project Credential in its protected project state; do not reuse it as a general user/admin token.
- Keep `WEBCODEX_TOKEN` on the Server. It is not an MCP or Runner credential.
- `wc_agent_*` is a Runner transport token only; `wc_pat_*` is the normal managed user API token.
- Prefer `--token-file` and never paste whole configuration files into chat.
- OAuth clients should follow the OAuth flow rather than manually copying access tokens. See [Authentication](AUTH_MODEL.md#oauth2) and [MCP](MCP.md#oauth2).

## Common examples

Full everyday use: first follow the [Full Setup guide](PERSONAL_SETUP.md) to start a regular Server, then enroll the project machine and start its Runner:

```bash
webcodex login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git" \
  --project "$HOME/git/my-repo" \
  --print-mcp-config
webcodex runner run --config <login-reported-runner-config>
```

To try one repository temporarily:

```bash
cd /path/to/your/repository
webcodex share
```

Local/manual project-bound workflow (advanced/diagnostic):

```bash
webcodex setup
webcodex doctor
webcodex run          # keep this terminal open; output points to /runtime
webcodex status       # in another terminal
```

Existing hosted Server:

```bash
webcodex connect https://your-server.example
webcodex runner status --profile <profile>
webcodex runner logs --profile <profile> --lines 100
```

Managed enrollment:

```bash
webcodex login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
webcodex runner install --scope user --config <login-reported-runner-config>
webcodex runner status --scope user --config <login-reported-runner-config>
webcodex ops status --server-url https://your-server.example \
  --token-file <login-reported-webcodex-user-token> --strict
```

## Proxy and network

CLI requests follow the standard proxy environment by default
(`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`). Use
`--proxy http://HOST:PORT` to override for one invocation, or
`--no-system-proxy` to ignore proxy environment and connect directly. These
flags affect only the CLI's own HTTP requests; `webcodex connect` does not
persist or inject them into the Runner configuration.
