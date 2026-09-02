# AI Coding Agent Onboarding

[English](AI_ONBOARDING.md) | [简体中文](AI_ONBOARDING.zh-CN.md)

This guide is for an AI coding agent helping a **user** connect a repository to
WebCodex or operate an existing deployment. It is not [`AGENTS.md`](../AGENTS.md),
which governs development of WebCodex itself.

Treat the default user goal as: **"let ChatGPT use my development environment normally."** Unless the user explicitly asks only for a temporary trial, prefer the full regular Server + Runner coding experience instead of treating `share` as WebCodex itself.

Before the first successful setup, keep the user-facing vocabulary to: **WebCodex service, Runner (the machine that runs code), Project, one-time login code, and ChatGPT connection**. Do not front-load `client_id`, runtime project ids, `project_registry_dir`, PATs, Runner tokens, scope ceilings, or Connector surfaces.

## First choose the experience

1. The user wants normal/daily/long-lived WebCodex use, or did not explicitly ask for a temporary trial: use the [Full Setup guide](PERSONAL_SETUP.md) with a regular Server + Runner.
2. The user explicitly wants a few-minute, zero-commitment trial of one repository: use **`webcodex share`**. It is temporary, single-project, and more restricted.
3. The user already has a WebCodex Server URL: use the enrollment method that Server actually provides. Fresh managed self-hosting uses pairing + `webcodex login`; use `webcodex connect <server>` only when an operator explicitly supplied a shared-key credential.
4. Move to [Deployment](DEPLOYMENT.md) only for production hosting, multiple users, OAuth, systemd/Docker, private CAs, and similar operator concerns.

Windows can run a foreground Server and Runner directly. Do not redirect Windows users to a remote Linux host by default, and do not switch them to `share` merely because they need a Tunnel. **A Tunnel solves reachability; it does not choose the WebCodex coding permission model.**

## Default: full setup

Use the [Full Setup guide](PERSONAL_SETUP.md) as the user-facing source of truth. Help the user complete these observable steps:

- install WebCodex;
- start a regular Server;
- choose one reachability path to that Server (public HTTPS, Cloudflare, OpenAI Tunnel, etc.);
- create a one-time login code;
- run `webcodex login ... --project <repo>`;
- start the Runner;
- use `--print-mcp-config` or the selected Tunnel guide to add WebCodex to ChatGPT;
- verify one read and one small edit using the full coding experience.

Do not copy systemd socket details, OAuth scopes, token taxonomy, registry paths, or similar Deployment reference material into this ordinary user path before it is needed.

## Temporary trial: `share`

Only make `share` the main path after the user explicitly chooses a quick temporary trial:

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

`share` performs temporary project setup and owns the Server/Runner/Tunnel for that run. When the CLI reports **WebCodex ready**, have the human follow its ChatGPT connection output. Developer Mode, custom MCP apps, and write/modify actions still depend on the ChatGPT plan, workspace, and admin policy.

Use `webcodex share --tunnel none` only for a local-only temporary trial, and `webcodex share --tunnel openai` for an explicitly selected OpenAI Tunnel temporary share. Do not infer from those temporary share modes that a regular Server + Runner has the same restrictions.

## Existing shared-key Server path: `connect`

Use this path only when a hosted Server is already configured for shared-key
clients and the operator supplied that client credential (or the machine already
has the protected profile):

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example --key-file /private/path/shared-key
```

`connect` creates/reuses a profile, starts a detached Runner, waits for the same
identity to see the Runner and project, then prints the MCP setup values before
diagnostic details. Never obtain this shared key by copying a self-hosted Docker
Server's `.env` bootstrap administrator token to the client.

The inverse operation is:

```bash
webcodex disconnect
```

It unregisters only the exact canonical repository; do not delete the checkout,
`.git`, profile credential, or sibling project registrations.

## Self-hosted enrollment and optional OAuth

For a freshly bootstrapped self-hosted Server, managed pairing is the normal
repository-machine enrollment path, not an optional reuse of the Server admin
token. Create a short-lived pairing code on the Server, then redeem it as the
ordinary user that will run project commands:

```bash
webcodex login https://webcodex.example --code <wc_pair_...> \
  --allowed-root "$HOME/git" \
  --project "$HOME/git/my-repo"
webcodex runner install --scope user \
  --config <login-reported-runner-config>
```

For ordinary users, explain only that `--project` is the actual project and `--allowed-root` is a parent directory from which more projects may be added later. Do not require manual edits to `runner.toml` or `project-registry`; reserve registry/authority internals for troubleshooting and reference material. If login intentionally omitted `--project`, add it later with `webcodex project register --config <login-reported-runner-config> /path/to/repo`.

Use `share --auth oauth` or `connect --auth oauth` only when the client requires
OAuth and the exact callback URL is known. Do not collapse OAuth client secrets,
shared keys, Project Credentials, PATs, Runner tokens, or bootstrap administrator
tokens into one concept.

The same managed flow also supplies separate user/device identity, revocation,
audit, and organization administration when those controls are needed. It keeps
user/API authority separate from Runner transport authority. See
[Authentication](AUTH_MODEL.md) and [MCP](MCP.md) for the reference model.

## What the AI may inspect

Safe non-secret observations include:

- repository path and Git status;
- Server URL;
- profile/state/log **paths**;
- service status via `webcodex runner status` / `webcodex server status`;
- `webcodex status` and `webcodex doctor` output;
- the location of a token file, but not its contents.

`doctor` is a local/manual runtime diagnostic. If it recommends `webcodex run`,
do not reinterpret that as a prerequisite for hosted ChatGPT; use `share` for the
first hosted-chat path.

## Secrets remain human-controlled

Do **not** read back, print, log, commit, or echo into chat:

- shared keys, PATs, Runner tokens, account credentials, bootstrap tokens;
- Project Credentials;
- OAuth client secrets;
- full `runner.toml` contents or Server env files;
- `Authorization` headers.

When a credential must be entered into ChatGPT/Claude, identify the source precisely and ask the **human** to copy it. Prefer connection details explicitly produced by the current flow, such as `login --print-mcp-config` for full setup or the successful disclosure from a temporary `share` / existing-Server `connect`. If a stored value must be recovered, point the user to the exact protected file/field without echoing it yourself. Status/log commands intentionally do not reveal secrets.

Never substitute a `wc_agent_*` Runner token for an MCP token, never use a
bootstrap `WEBCODEX_TOKEN` as an MCP credential, and never assume offline
`webcodex tokens generate` material is registered on a remote Server.

## Troubleshooting

Use the CLI's actionable error first. For established state, inspect only the
minimum non-secret diagnostics needed:

```bash
webcodex status
webcodex doctor
webcodex runner status --profile <profile>
webcodex runner logs --profile <profile> --lines 100
```

See [Troubleshooting](TROUBLESHOOTING.md) for stable failure codes and operator
checks.
