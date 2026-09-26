# WebPi CLI

The `webpi` command is the unified operator and developer interface. It
covers project setup, Server and Runner lifecycle, device enrollment, token
management, and read-only operator checks.

Remote operations (users, tokens, pairing, operator checks) go through the
Server HTTP API, and the CLI is a convenient client for them. Local operations
(project setup, service management, task review decisions) run directly on the
host machine and are not available through the Server API.

The CLI produces three binaries when built from source:

- `webpi` — the unified command documented here.
- `webpi-server` — the Server process (started and supervised with
  `webpi server ...`).
- `webpi-runner` — the Runner process that executes project work (started
  and supervised with `webpi runner ...`).

`webpi --help` lists the top-level namespaces. The sections below explain what each namespace is for. This is the complete CLI reference; ordinary users do not need to understand every command, credential, or internal configuration field before their first successful setup.

For everyday development, follow the [Full Setup guide](PERSONAL_SETUP.md) and use a regular Server + Runner. `webpi share` is the explicit **temporary one-project trial/share** entry on Linux, macOS, and Windows; it prepares the temporary project environment, Server, Runner, and optional Tunnel for that foreground run and ends when the process exits. Running bare `webpi` in an interactive Git checkout remains a Linux/macOS convenience shortcut into that same temporary `share` flow.

## Command map

### Project / local workflow

These commands work on the current Git project.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webpi` (no command) | Interactive temporary-share shortcut | Only auto-dispatches to `share` on Linux/macOS when stdin/stdout are terminals and the current directory is inside a Git checkout; otherwise normal help is shown. |
| `webpi share` | Temporarily share the current project with ChatGPT/MCP | Quick trial/short-lived sharing path on Linux/macOS/Windows; includes temporary setup, local Server + Runner, `cloudflare|openai|none`, and bounded foreground cleanup. Use `PERSONAL_SETUP` for full daily use. |
| `webpi connect <server>` | Connect the current project to an existing Server | Long-lived path when you already have a Server URL; defaults to hosted shared-key. |
| `webpi status` | Concise project coding readiness | Short summary; `doctor` is the full diagnostic check. |
| `webpi doctor` | Read-only readiness checks for the current project | Diagnostics/manual workflow; reports a stable `next action`. |
| `webpi setup` | Configure the current Git project without starting it | Local-only/manual workflow; creates private state and a Project Credential. |
| `webpi run` | Start the project-bound loopback Server and local Runner | Local-only/manual workflow; foreground, Ctrl-C stops both. |
| `webpi disconnect [--project PATH] [--profile NAME]` | Remove one hosted project registration | Exact inverse of `connect` for that repository; never removes the repository or `.git`. |

`webpi share --auth query-token` is an explicit temporary-share compatibility mode for MCP clients that cannot configure a Bearer header. It accepts the exact share Project Credential only on `/mcp?token=...`, prints a URL-encoded sensitive MCP URL, and tells the client to use No authentication. The mode is disabled for ordinary Server/runtime requests, does not accept PAT/OAuth/shared-key/Runner credentials through the query, and is rejected with `--tunnel openai`. Treat the complete URL as a credential because URL queries may be retained by clients, proxies, clipboards, or access logs. The default remains `--auth bearer`.

`webpi share --auth oauth --oauth-redirect-uri <exact-callback>` uses OAuth 2.0 Authorization Code with PKCE S256. The OAuth client ID/secret are persisted in protected project state for that project + callback, while the temporary OAuth grants are valid only for the current `share` run. Restarting `share` therefore invalidates old OAuth grants without changing the project. OAuth access tokens are never accepted on Runner transport.

Quick Tunnel origins remain temporary. For an operator-managed stable HTTPS origin, use `--tunnel none --public-url https://share.example` and route that origin to the loopback WebPi Server yourself; `--public-url` advertises the external origin/issuer and does not create a proxy or tunnel.

`webpi share --tunnel openai` is the explicit OpenAI Secure MCP Tunnel provider. It requires `CONTROL_PLANE_TUNNEL_ID` plus a Restricted `CONTROL_PLANE_API_KEY` with Tunnels Read + Use and currently supports only `--auth bearer`. WebPi resolves pinned OpenAI `tunnel-client` v0.0.14 from `WEBPI_TUNNEL_CLIENT_BIN`, `PATH`, or a verified managed download; it runs `doctor` before the daemon and waits for `/readyz`. The temporary WebPi Bearer is written only to the private share directory and referenced by `tunnel-client` through a file-backed MCP `Authorization` header. ChatGPT therefore uses Connection: Tunnel + No authentication. `OPENAI_ADMIN_KEY` and `OPENAI_API_KEY` are explicitly removed from the long-lived daemon environment; the Runtime API key remains the control-plane authority.

For public `share`, WebPi best-effort copies only the MCP URL to the clipboard and does not copy the temporary credential in the default Bearer/OAuth modes. The explicit `--auth query-token` mode instead copies the sensitive tokenized URL by design and says so in its status output. Interactive Linux/macOS terminals also offer an Enter shortcut to open ChatGPT App settings. Clipboard/browser integration is convenience-only and never gates runtime readiness. Use `--no-copy-url` to suppress clipboard access.

For supervised machine integration, `webpi share --json --stop-on-stdin-eof` keeps the same foreground lifecycle but also treats the supervising parent's closed stdin as a stop request. This lets Desktop or another structured process owner ask `share` to clean up its own temporary Server, Runner, and Tunnel without shell-command signaling. The flag is rejected outside `--json` mode.

`webpi connect <server> --auth oauth --oauth-redirect-uri <exact-callback>` is the ordinary hosted OAuth path. The Runner keeps its existing hosted credential while the MCP client uses OAuth. Add `--oauth-computer-permissions`, `--oauth-local-mcp`, or `--oauth-local-ssh` only when those optional capabilities are actually needed; they are explicit permission changes and can require reauthorization. `--oauth-local-ssh` grants the MCP client the optional `ssh:local` authority needed by the model-facing `ssh_resource` onboarding tool; it does not expose SSH credentials or make a registered resource active without the Runner restart reported by that tool. See [MCP](MCP.md#oauth2) for client setup and [Authentication](AUTH_MODEL.md#oauth2) for the security model.

The advanced managed identity flow remains available as `--auth managed-oauth --oauth-redirect-uri <exact-callback>` and requires `webpi login`; `--user` applies only to that mode.

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
| `webpi login <server-url> --code <wc_pair_...> [--project PATH]` | Log this device into a Server with a one-time code | Normal managed enrollment entry. `--project` selects the actual project, `--allowed-root` names a parent from which more projects may be added later, and `--print-mcp-config` explicitly prints sensitive ChatGPT MCP connection values. |
| `webpi project register --config PATH <PROJECT>` | Add another project to an existing Runner | Persists that Runner's project configuration without requiring the Server to be online; follow the command output if an already-running Runner needs a reload. |
| `webpi pairing create` | Server/admin side: create a short-lived pairing code | Needs server bootstrap/admin auth. |
| `webpi logout <server-url> [--user USER|--all]` | Remove this device's credentials for a Server | With one saved user, the user is selected automatically. With multiple saved users, choose one with `--user USER` or explicitly choose all with `--all`; deletion still uses the existing confirmation/`--yes` flow. |

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

The Runner executable is `webpi-runner`. Its canonical CLI lifecycle namespace
is `runner`: `webpi runner ...` manages the `webpi-runner` process and
service. `webpi` and `webpi-runner` remain separate executables.

| Command | Purpose |
| --- | --- |
| `webpi runner init` | Generate a `runner.toml` config manually |
| `webpi runner install` | Install, enable, and start the Runner service |
| `webpi runner run` | Run `webpi-runner` in the foreground |
| `webpi runner start` | Start a hosted background Runner or installed profile service |
| `webpi runner stop` | Stop it |
| `webpi runner restart` | Restart it |
| `webpi runner status` | Check Runner lifecycle, config, and connectivity |
| `webpi runner logs` | Read Runner logs (bounded) |
| `webpi runner uninstall` | Remove the service unit (requires `--confirm`) |

Service commands accept `--scope user|system`. Non-root users default to user
scope; root defaults to system scope. Profiles created by `webpi connect`
keep their detached-process behavior when `--scope` is omitted.

### Server

| Command | Purpose |
| --- | --- |
| `webpi server init` | Initialize/update the Server env file and selected data directory (creates the bootstrap token) |
| `webpi server install` | Install the Linux systemd `webpi.socket` + `webpi.service` pair; default WorkingDirectory follows selected env `WEBPI_DATA` |
| `webpi server run [--env-file PATH]` | Run `webpi-server` in the foreground (direct bind); `--env-file` passes the exact path via `WEBPI_ENV_FILE` |
| `webpi server start` / `stop` | Start or stop socket activation and the Server process coherently |
| `webpi server restart` | Restart only the Server process; keep the managed listener socket active |
| `webpi server status` | Check authoritative socket/service state, HTTP reachability, and build revisions |
| `webpi server logs` | Read the Server service journal |
| `webpi server uninstall` | Stop, disable, and remove the managed socket/service pair |

On Windows, `server init`, foreground `server run`, and explicit `share` are supported. The managed service lifecycle (`install`, `start`, `stop`, `restart`, `logs`, `uninstall`) remains Linux-only.

`webpi server install --service-file /path/name.service` derives the sibling
`/path/name.socket`. Use the same `--service-file` on `start`, `stop`, `restart`,
`status`, `logs`, and `uninstall` to manage or inspect that custom pair; omitting
it targets the default `webpi.service` / `webpi.socket` pair.

For Runner config terminology, `project_registry_dir` is the directory of Project registry TOML files, not a workspace root. `[policy].allowed_roots` bounds which filesystem paths may be registered; a Project record names the actual workspace.

### Operations (read-only operator checks)

| Command | Purpose |
| --- | --- |
| `webpi ops status` | Summarize runtime, tools, Jobs, Runners, and Projects |
| `webpi ops runners` | Compact Runner fleet status |
| `webpi ops runner --client-id <id>` | Exact read-only Runner registration/build status |
| `webpi ops projects` | Project inventory and smoke suitability |
| `webpi ops smoke-preflight --project <id>` | Preflight one project for a deploy smoke |

`ops` commands are read-only. They accept `--server-url`, `--token-file`,
`--env-file`, `--token`, `--json`, and `--strict`. Prefer `--token-file` for
operator use; `--token` can leak into shell history or process lists. `--strict`
makes a FAIL report exit with status 2.

### Review and runtime activity

The legacy `webpi task` namespace has been removed with the separate Connector Task/Result/Approval lifecycle. Local `webpi run` prints the Runtime Console URL (`/runtime`). Runtime review uses the canonical Workflow Session, Job, Git/diff, `show_changes`, and `finish_coding_task` paths rather than a host-side result accept/reject queue.

### Credentials and accounts

Admin user/token operations are Server-API-backed. `auth status` reads local
device connection state, while the `create-local` commands generate credentials
locally and register only their hashes with the Server.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webpi auth status` | Show which servers this device is logged in to | Read-only; supports `--dir` and `--json`. |
| `webpi users create` | Create a user; `--issue-credential` returns a one-time account credential | Server/admin side; uses `--server-url`. |
| `webpi users list` | List users | |
| `webpi tokens create-local` | Locally generate a `wc_pat_*` personal API token and register its hash | Uses `--server-url`, `--username`, and an account credential. |
| `webpi tokens create` | Admin: create a PAT server-side | Uses `--server-url`. |
| `webpi tokens generate` | Offline token material generation | Does **not** register with any Server. |
| `webpi tokens list` / `revoke` / `register-hash` | List or revoke PATs; register an externally computed hash | Admin side; uses `--server-url`. |
| `webpi runner-tokens create-local` | Locally generate a `wc_agent_*` Runner token and register its hash | Uses `--server-url` and binds to `--client-id`. |
| `webpi runner-tokens create` / `list` / `revoke` / `register-hash` | Admin variants | |

All Server-targeting credential commands use the canonical `--server-url` spelling.
Local `tokens create-local` / `runner-tokens create-local` use `--username` plus an
account credential; admin token management uses the same plural namespaces.

### Advanced and compatibility commands

These commands cover unusual setups; the recommended paths above are the
normal entry points.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webpi pairing create` | Server/admin side: create a short-lived pairing code | Needs server bootstrap/admin auth. |
| `webpi tokens generate` | Offline token material generation | Registers nothing; pair the output with `tokens register-hash` if the hash must be registered server-side. |
| `webpi tokens register-hash` | Admin: register an externally computed PAT hash | Uses `--server-url`; for offline-generated material. |
| `webpi runner-tokens register-hash` | Admin: register an externally computed Runner-token hash | Uses `--server-url`; for offline-generated material. |

## Terminology

- **Server** — authenticates callers, stores shared runtime state, and routes work.
- **Runner** — runs repository work on the machine that owns the code.
- **Project** — one repository/workspace registered by a Runner.
- **Job** — a command or validation that continues after the initiating call returns.
- **Workflow Session** — bounded coding evidence/continuity used by the runtime. Ordinary users normally do not manage its internal protocol fields.

Some compatibility-facing names still contain `agent`, notably `wc_agent_*` and `agent:<client_id>:<project_id>`. They refer to Runner-era compatibility, not the separate Durable Agent domain. Other process/protocol identifiers remain internal. New prose should say **Runner** unless it is quoting one of those public names.

## Credentials: which token do I need?

WebPi separates bootstrap administration, account onboarding, runtime API
access, and Runner connectivity. Do not reuse one credential across surfaces.
The full model is in [AUTH_MODEL.md](AUTH_MODEL.md); the table below is the
quick answer.

| Credential | Prefix | Created by | Used for | Do not use for |
| --- | --- | --- | --- | --- |
| Server bootstrap token | (env `WEBPI_TOKEN`) | `webpi server init` | server/admin setup, user creation, pairing | GPT Actions, MCP, Runner, daily use |
| Shared key | `wck_...` | `webpi connect` (generated once) | hosted shared-key MCP + Runner | production IAM |
| Project Credential | (private file) | `webpi setup` | one ProjectGrant's ordinary runtime API/MCP access | other ProjectGrants, admin, Runner transport |
| Account credential | `wc_acct_...` | `webpi users create --issue-credential` | local token creation | GPT Actions, MCP, Runner |
| Personal API token (PAT) | `wc_pat_...` | `webpi tokens create-local` | GPT Actions, MCP, REST API | Runner connectivity |
| Runner token | `wc_agent_...` | `webpi runner-tokens create-local` | `webpi-runner` transport only | MCP, REST, GPT Actions |
| OAuth access token | `wc_oat_...` | OAuth2 authorization flow | GPT Actions / MCP when OAuth is enabled | — |

### Practical credential rules

- Normal managed setup: `webpi login` creates the local user/API and Runner credentials; use the paths and MCP values it reports.
- Existing shared-key Server: use the operator-provided `wck_...` with `webpi connect`.
- Project-first/manual setup: keep the Project Credential in its protected project state; do not reuse it as a general user/admin token.
- Keep `WEBPI_TOKEN` on the Server. It is not an MCP or Runner credential.
- `wc_agent_*` is a Runner transport token only; `wc_pat_*` is the normal managed user API token.
- Prefer `--token-file` and never paste whole configuration files into chat.
- OAuth clients should follow the OAuth flow rather than manually copying access tokens. See [Authentication](AUTH_MODEL.md#oauth2) and [MCP](MCP.md#oauth2).

## Common examples

Full everyday use: first follow the [Full Setup guide](PERSONAL_SETUP.md) to start a regular Server, then enroll the project machine and start its Runner:

```bash
webpi login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git" \
  --project "$HOME/git/my-repo" \
  --print-mcp-config
webpi runner run --config <login-reported-runner-config>
```

To try one repository temporarily:

```bash
cd /path/to/your/repository
webpi share
```

Local/manual project-bound workflow (advanced/diagnostic):

```bash
webpi setup
webpi doctor
webpi run          # keep this terminal open; output points to /runtime
webpi status       # in another terminal
```

Existing hosted Server:

```bash
webpi connect https://your-server.example
webpi runner status --profile <profile>
webpi runner logs --profile <profile> --lines 100
```

Managed enrollment:

```bash
webpi login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
webpi runner install --scope user --config <login-reported-runner-config>
webpi runner status --scope user --config <login-reported-runner-config>
webpi ops status --server-url https://your-server.example \
  --token-file <login-reported-webpi-user-token> --strict
```

## Proxy and network

CLI requests follow the standard proxy environment by default
(`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`). Use
`--proxy http://HOST:PORT` to override for one invocation, or
`--no-system-proxy` to ignore proxy environment and connect directly. These
flags affect only the CLI's own HTTP requests; `webpi connect` does not
persist or inject them into the Runner configuration.
