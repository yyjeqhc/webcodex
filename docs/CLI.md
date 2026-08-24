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
  and supervised with `webcodex agent ...`).

`webcodex --help` lists the top-level namespaces. The sections below explain
what each namespace is for.

For a first ChatGPT connection on Linux/macOS, the normal entry is simply
`webcodex share` in the repository. It performs project setup, starts the local
Server and Runner, and exposes a temporary HTTPS MCP endpoint. The default Quick
Tunnel requires `cloudflared` on `PATH`; the command checks that prerequisite
before creating project/share state and reports an installation link if it is
missing. Windows does not support this local Server/share path; use
`webcodex connect <server-url>` against a remote Linux Server there.

## Command map

### Project / local workflow

These commands work on the current Git project.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex share` | Share the current project for ChatGPT/MCP over HTTPS | Linux/macOS first-run path; includes setup and starts the local Server + Runner. Unavailable on Windows. Default Quick Tunnel requires `cloudflared`. |
| `webcodex connect <server>` | Connect the current project to an existing Server | Long-lived path when you already have a Server URL; defaults to hosted shared-key. |
| `webcodex status` | Concise project coding readiness | Short summary; `doctor` is the full diagnostic check. |
| `webcodex doctor` | Read-only readiness checks for the current project | Diagnostics/manual workflow; reports a stable `next action`. |
| `webcodex setup` | Configure the current Git project without starting it | Local-only/manual workflow; creates private state and a Project Credential. |
| `webcodex run` | Start the project-bound loopback Server and local Runner | Local-only/manual workflow; foreground, Ctrl-C stops both. |
| `webcodex disconnect [--project PATH] [--profile NAME]` | Remove one hosted project registration | Exact inverse of `connect` for that repository; never removes the repository or `.git`. |

`webcodex share --auth oauth --oauth-redirect-uri <exact-callback>` uses OAuth 2.0 Authorization Code with PKCE S256. The OAuth client ID/secret are persisted in protected project state for that project + callback, while authorization codes, access tokens, refresh tokens, and the temporary Project Credential are fenced to the current `share` process. Restarting `share` therefore invalidates old OAuth grants without changing the Connector's stable project identity. OAuth access tokens are never accepted on Runner transport.

Quick Tunnel origins remain temporary. For an operator-managed stable HTTPS origin, use `--tunnel none --public-url https://share.example` and route that origin to the loopback WebCodex Server yourself; `--public-url` advertises the external origin/issuer and does not create a proxy or tunnel.

`webcodex connect <server> --auth oauth --oauth-redirect-uri <exact-callback>` is the ordinary ChatGPT OAuth path for hosted connect. It uses the same `wck_*` shared-key identity as the Runner and keeps the direct shared-key baseline unchanged: `runtime:read`, `project:read`, `project:write`, `job:run`, `computer:read`, and `computer:control`. A fresh OAuth client starts with that full baseline, while an existing protected client may carry a valid narrower baseline subset. Adding `--oauth-computer-permissions` is an explicit client-ceiling opt-in that appends only `computer:launch`, `computer:display_read`, `computer:pointer_control`, `computer:clipboard_read`, and `computer:clipboard_write` to the existing baseline subset; it never restores baseline scopes that were previously absent and does not grant the optional scopes by itself. The WebCodex authorize page presents eligible additional Computer permissions unchecked, and only the selected permissions enter the authorization code/access/refresh grant. Launch consent requires the OAuth request to contain both `computer:read` and `computer:launch`; missing prerequisites are disabled rather than filled in by WebCodex. A real ceiling expansion revokes existing grants and requires reauthorization; ordinary reconnect never widens a baseline client. `account:manage`, `admin`, `job:detach`, every `agent:*` scope, and future scopes are never part of this picker. Runner capability shown on the consent page is current backend availability, not a guarantee that native/OS permission will succeed; runtime calls recheck current capability and native preflight. OAuth access tokens remain invalid on Agent transport.
To let the same Connector access Runner-owned local MCP providers, add the separate explicit `--oauth-local-mcp` opt-in. It adds only `mcp:local` to that shared-key-owned OAuth client's ceiling; old clients/credentials do not gain it on upgrade. A real ceiling change revokes old grants and requires reauthorization.

The advanced managed identity flow remains available as `--auth managed-oauth --oauth-redirect-uri <exact-callback>` and requires `webcodex login`; `--user` applies only to that mode.

`disconnect` matches the canonical repository path, not a basename or project id. If the same
repository is registered in more than one hosted profile, specify `--profile`. With a live
managed Runner it performs a fenced structured unregister before removing the local registration;
with a stopped Runner it removes only the exact local project registration. Other projects,
profile credentials, and `agent.toml` are preserved.

After connecting an MCP coding client, see the [Coding Workflow](CODING_WORKFLOW.md) for the
canonical `work_on_project` model bootstrap, behavioral guidance, validation, and closeout
evidence. `start_coding_task` remains available only to explicit advanced/direct callers and is
not advertised by ordinary MCP/GPT Actions model discovery.

### Enrollment

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex login <server-url> --code <wc_pair_...>` | Log this device into a Server with a pairing code | Primary client entry. Writes the user token and `agent.toml`. |
| `webcodex pairing create` | Server/admin side: create a short-lived pairing code | Needs server bootstrap/admin auth. |
| `webcodex client enroll` | Advanced client enrollment with explicit `--client-id` | Advanced; ordinary users should use `login`, which derives the client id and publishes the canonical per-server/user connection layout in one step. |
| `webcodex logout <server-url>` | Remove this device's credentials for a Server | |

### Runner (the `agent` namespace)

The Runner executable is `webcodex-runner`. The CLI namespace for it is called
`agent` (for historical reasons): `webcodex agent ...` manages the
`webcodex-runner` process and service. "Agent" and "Runner" refer to the same
execution component, but they are not the same program: `webcodex` (which
contains the `agent` namespace) and `webcodex-runner` are separate
executables.

| Command | Purpose |
| --- | --- |
| `webcodex agent init` | Generate an `agent.toml` config manually |
| `webcodex agent install` | Install, enable, and start the Runner service |
| `webcodex agent run` | Run `webcodex-runner` in the foreground |
| `webcodex agent start` | Start a hosted background Runner or installed profile service |
| `webcodex agent stop` | Stop it |
| `webcodex agent restart` | Restart it |
| `webcodex agent status` | Check Runner lifecycle, config, and connectivity |
| `webcodex agent logs` | Read Runner logs (bounded) |
| `webcodex agent uninstall` | Remove the service unit (requires `--confirm`) |

Service commands accept `--scope user|system`. Non-root users default to user
scope; root defaults to system scope. Profiles created by `webcodex connect`
keep their detached-process behavior when `--scope` is omitted.

### Server

| Command | Purpose |
| --- | --- |
| `webcodex server init` | Initialize or update the Server env file (creates the bootstrap token) |
| `webcodex server install` | Install the `webcodex-server` systemd service |
| `webcodex server run` | Run `webcodex-server` in the foreground |
| `webcodex server start` / `stop` / `restart` | Control the installed service |
| `webcodex server status` | Check systemd, HTTP reachability, and build revisions |
| `webcodex server logs` | Read the service journal |
| `webcodex server uninstall` | Remove the service unit |

### Operations (read-only operator checks)

| Command | Purpose |
| --- | --- |
| `webcodex ops status` | Summarize runtime, tools, jobs, agents, and projects |
| `webcodex ops agents` | Compact agent fleet status |
| `webcodex ops runner --client-id <id>` | Exact read-only Runner registration/build status |
| `webcodex ops projects` | Project inventory and smoke suitability |
| `webcodex ops smoke-preflight --project <id>` | Preflight one project for a deploy smoke |

`ops` commands are read-only. They accept `--server-url`, `--token-file`,
`--env-file`, `--token`, `--json`, and `--strict`. Prefer `--token-file` for
operator use; `--token` can leak into shell history or process lists. `--strict`
makes a FAIL report exit with status 2.

### Review (host-local decisions)

| Command | Purpose |
| --- | --- |
| `webcodex task list` | List recent tasks for this project |
| `webcodex task show <id>` | Show a task's result, approvals, and timeline |
| `webcodex task accept <id>` | Apply a reviewed result to the checkout |
| `webcodex task reject <id> [reason]` | Reject the stable result; the reason reaches the model |
| `webcodex task resume <id>` | Resume a preserved run after a runtime restart |
| `webcodex task guide <id> <message>` | Send course-correcting guidance to a running task |
| `webcodex task approve <id> <approval> [reason]` | Approve one exact raw command for one use |
| `webcodex task deny <id> <approval> [reason]` | Deny it; the reason is shown to the model |
| `webcodex task activity` | Show recent mutating tool executions (workspace ledger) |

`task` commands operate on the current project by default; use `--root PATH`,
`--profile NAME`, or `--state-dir PATH` to point at another project. Accept and
Reject are the two ways a human applies or discards a coding result locally —
the online model can never accept its own work.

### Credentials and accounts

Admin user/token operations are Server-API-backed. `auth status` reads local
device connection state, while the `create-local` commands generate credentials
locally and register only their hashes with the Server.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex auth status` | Show which servers this device is logged in to | Read-only; supports `--dir` and `--json`. |
| `webcodex users create` | Create a user; `--issue-credential` returns a one-time account credential | Server/admin side; uses `--server-url`. |
| `webcodex users list` | List users | |
| `webcodex token create-local` | Locally generate a `wc_pat_*` personal API token and register its hash | Uses `--server` and an account credential. |
| `webcodex tokens create` | Admin: create a PAT server-side | Uses `--server-url`. |
| `webcodex token generate` | Offline token material generation | Does **not** register with any Server. |
| `webcodex tokens list` / `revoke` / `register-hash` | List or revoke PATs; register an externally computed hash | Admin side; uses `--server-url`. |
| `webcodex agent-token create-local` | Locally generate a `wc_agent_*` Runner token and register its hash | Binds to `--client-id`. |
| `webcodex agent-tokens create` / `list` / `revoke` / `register-hash` | Admin variants | |

Note the flag difference: `users create` and the admin `tokens`/`agent-tokens`
commands use `--server-url`; the local `token create-local` and
`agent-token create-local` commands use `--server`.

### Advanced and compatibility commands

These commands cover unusual setups; the recommended paths above are the
normal entry points.

| Command | Purpose | Notes |
| --- | --- | --- |
| `webcodex client enroll` | Advanced enrollment with explicit `--client-id` | Its help says: advanced; prefer `webcodex login`, which derives the client id and publishes the canonical per-server/user connection layout in one step. |
| `webcodex pairing create` | Server/admin side: create a short-lived pairing code | Needs server bootstrap/admin auth. |
| `webcodex token generate` | Offline token material generation | Registers nothing; pair the output with `tokens register-hash` if the hash must be registered server-side. |
| `webcodex tokens register-hash` | Admin: register an externally computed PAT hash | Uses `--server-url`; for offline-generated material. |
| `webcodex agent-tokens register-hash` | Admin: register an externally computed Runner-token hash | Uses `--server-url`; for offline-generated material. |
| `webcodex setup single-user` | Legacy single-user bootstrap flow | Not the normal path. |

## Terminology

### People and machines

- **Server** — the `webcodex-server` process that authenticates callers and
  routes tool requests.
- **CLI** — the `webcodex` command described here.
- **Runner** — the `webcodex-runner` process on the machine that owns the
  repositories. It executes the actual work.
- **profile** — a named local client configuration (paths, `agent.toml`,
  tokens) under the user's WebCodex config directory. `webcodex connect`
  creates one; `webcodex agent ... --profile <name>` targets it.
- **client_id** — a stable logical identifier for one Runner/device (for
  example `workstation` or `alice-macbook`). A Runner's `client_id` is part of
  its runtime project ids and is what Runner tokens are bound to.
- **agent_instance_id** — a per-process identity generated by
  `webcodex-runner` at startup and reused for the whole process lifetime
  (including WebSocket reconnects). The Server treats it as the active lease
  identity: a second process with the same `client_id` but a different
  `agent_instance_id` is rejected while the first is online, and a
  stale/replaced instance can no longer poll or submit results. It is not a
  secret.
- **Connector** — the project-bound coding surface exposed by a configured
  local project. A Connector binds one logical project to its registered
  executor, so the model does not manage project ids.

### Projects and work

- **project_id** — a project id registered by an agent in its `projects.d`
  registry.
- **runtime project id** — the full identifier `agent:<client_id>:<project_id>`
  that addresses a registered project. A project-bound Connector resolves this
  internally; ordinary users do not type it.
- **Task** — one bounded unit of project work created by the model and
  reviewed by a human. Tasks have stable ids (`task_...`) and a review result.
- **Job** — a long-running command or validation that continues after the
  initiating call returns. Jobs have ids and bounded logs, and can be stopped.
- **Workflow Session** — the operator runtime's bounded evidence ledger for a
  long-lived coding session. Connector users do not manage session ids; their
  continuity comes from the project-bound task context.
- **request / operation ids** — correlation and retry identifiers used by the
  model (`operation_id`, `request_id`, `execution_id`, `result_id`). They are
  plumbing: ordinary users do not need to manage them.

## Credentials: which token do I need?

WebCodex separates bootstrap administration, account onboarding, runtime API
access, and Runner connectivity. Do not reuse one credential across surfaces.
The full model is in [AUTH_MODEL.md](AUTH_MODEL.md); the table below is the
quick answer.

| Credential | Prefix | Created by | Used for | Do not use for |
| --- | --- | --- | --- | --- |
| Server bootstrap token | (env `WEBCODEX_TOKEN`) | `webcodex server init` | server/admin setup, user creation, pairing | GPT Actions, MCP, Runner, daily use |
| Shared key | `wck_...` | `webcodex connect` (generated once) | hosted shared-key MCP + Runner | production IAM |
| Project Credential | (private file) | `webcodex setup` | the one project's Connector + Runner | other projects, admin |
| Account credential | `wc_acct_...` | `webcodex users create --issue-credential` | local token creation | GPT Actions, MCP, Runner |
| Personal API token (PAT) | `wc_pat_...` | `webcodex token create-local` | GPT Actions, MCP, REST API | Runner connectivity |
| Runner token | `wc_agent_...` | `webcodex agent-token create-local` | `webcodex-runner` transport only | MCP, REST, GPT Actions |
| OAuth access token | `wc_oat_...` | OAuth2 authorization flow | GPT Actions / MCP when OAuth is enabled | — |

### Hosted shared key (`wck_...`)

- Generated by `webcodex connect` when no `--key` or `--key-file` is supplied.
- Printed in full only when first created; the profile stores it so a repeat
  `connect` reuses it without printing it again.
- Stored in the owner-only profile config at
  `~/.config/webcodex/clients/<profile>/agent.toml` (or
  `$XDG_CONFIG_HOME/webcodex/clients/<profile>/agent.toml`) as the top-level
  `token = "wck_..."` field.
- To recover the value as a human, copy that `token` field. Status and log
  commands deliberately do not print it. There is no `show-token` command. An
  AI agent should locate the profile and point the human at the file rather
  than echoing the value.
- A repeat `connect` reuses the profile and does not print the key again.
- Never put `wck_` into a managed `wc_*` context; shared-key auth never falls
  back to managed identity.

### Project Credential

- Created by `webcodex setup` for the selected Git root and profile; stored in
  owner-only private files (a Connector credential file and the generated
  Agent configuration).
- If lost, restore both matching private files. There is no in-place rotate
  command; if unrecoverable, stop the runtime and explicitly recreate the
  private project-state profile (this also retires that profile's local task
  history).

### `WEBCODEX_TOKEN`

- The Server bootstrap/admin credential, created by `webcodex server init`
  and stored in the server env file (normally `/etc/webcodex/webcodex.env`)
  under the variable name `WEBCODEX_TOKEN`.
- It is not an MCP, Runner, or daily-use token. Use it only for initial setup,
  user creation, pairing, and emergency administration.
- To identify the env file and variable name: look at the Server service unit
  or the operator-managed env file that the service loads. Reading the token
  value is a secret-revealing, human-only action; do not paste it into a
  client or commit it.

### `wc_pat_*` (personal API token)

- A managed user token generated locally by `webcodex token create-local`;
  the Server stores only its hash.
- `webcodex login` writes it to a file named `webcodex-user-token` under the
  login directory for that server/user.
- Supply it to commands with `--token-file <path>` rather than `--token`.
  `--token-file` keeps the value out of shell history and process lists.
- To paste it into an MCP client, read that one specific `webcodex-user-token`
  file. Do not echo whole config files.

### `wc_agent_*` (Runner token)

- A Runner transport token generated locally by
  `webcodex agent-token create-local` and bound to a `client_id`.
- `webcodex login` stores it **only** inline in the generated `agent.toml`
  under `~/.config/webcodex/<server-slug>/<user>/` — no separate
  `webcodex-runner-token` file is created. The advanced `webcodex client
  enroll` flow (and the legacy `webcodex setup single-user` flow) additionally writes a
  `webcodex-runner-token` file next to `webcodex-user-token`.
- It is accepted only by Runner transport endpoints; using it on MCP/REST
  returns 403. Never use it as an MCP/API token.

### `wc_pair_*` (pairing code)

- A short-lived, one-time code created server-side by
  `webcodex pairing create`.
- Transfer only this code to the enrolling client; the client redeems it with
  `webcodex login <server-url> --code <code>`.
- It is not a long-lived API token and cannot be used after it expires.

### OAuth

When OAuth is enabled on the Server, MCP/GPT clients can use the
authorization-code flow instead of a static PAT. The client id, client secret,
and `wc_oat_*` access tokens are delegated credentials; see
[AUTH_MODEL.md](AUTH_MODEL.md#oauth2) and [MCP.md](MCP.md#oauth2).

## Common examples

First ChatGPT/MCP connection:

```bash
cd /path/to/your/repository
webcodex share
```

Local/manual workflow:

```bash
webcodex setup
webcodex doctor
webcodex run          # keep this terminal open
webcodex status       # in another terminal
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

Existing hosted Server:

```bash
webcodex connect https://your-server.example
webcodex agent status --profile <profile>
webcodex agent logs --profile <profile> --lines 100
```

Managed enrollment:

```bash
webcodex login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
webcodex agent install --scope user --config <login-reported-agent-config>
webcodex agent status --scope user --config <login-reported-agent-config>
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
