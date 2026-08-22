# AI Coding Agent Onboarding

[English](AI_ONBOARDING.md) | [简体中文](AI_ONBOARDING.zh-CN.md)

This guide is for an AI coding agent helping a **user** connect a repository to
WebCodex or operate an existing deployment. It is not [`AGENTS.md`](../AGENTS.md),
which governs development of WebCodex itself.

The default user goal is: **"let ChatGPT work with this repository."** Do not
introduce Server deployment, client ids, runtime project ids, PATs, Runner tokens,
or OAuth scope ceilings unless the user's chosen path actually requires them.

## Choose the smallest path

Check the platform before choosing the path. Windows does not support the local
Server runtime used by `webcodex share`; on Windows, use `connect` when a remote
Linux Server already exists, or guide the user through Linux Server deployment
first. Do not recommend `share` on Windows.

1. On Linux/macOS, the user has one local repository and wants to try ChatGPT/MCP
   now, with no existing WebCodex Server URL: use **`webcodex share`**.
2. The user already has a WebCodex Server URL and wants a persistent repository
   connection: use **`webcodex connect <server>`**.
3. The user needs separate identity, independent revocation, audit, or managed
   users: use the managed identity path (`webcodex login` / managed OAuth).
4. The user needs infrastructure control, private networking, stable HTTPS, or
   their own identity system: use [Deployment](DEPLOYMENT.md).

Do not invent `https://your-server.example` as a prerequisite for a new user.

## First-time ChatGPT path: `share`

Verify Git and the target repository, then verify that `cloudflared` is installed
for the default public share. If it is absent, point the user to Cloudflare's
official downloads rather than installing third-party executables silently.

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

`share` performs project setup itself. Do not teach `setup → doctor → run → stop
run → share` as the onboarding sequence.

When the CLI reports **WebCodex ready**, tell the human to keep that terminal open
and follow the printed **What to do next** section. Before promising write access,
remember that ChatGPT Developer Mode, custom MCP apps, and write/modify actions
are controlled by the user's ChatGPT plan, workspace, and admin settings; these
client-side permissions are separate from WebCodex authorization:

- ChatGPT Developer Mode → create an MCP custom app.
- paste the printed MCP URL.
- select the authentication type printed by the CLI.
- the **human** pastes the printed credential into ChatGPT.
- Scan Tools.
- first prompt: `Inspect this repository and summarize its structure. Do not make changes.`

For local-only debugging, `webcodex share --tunnel none` avoids the Cloudflare
Quick Tunnel and does not require `cloudflared`.

## Existing Server path: `connect`

Only use this path when a real Server URL is already available:

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example
```

`connect` creates/reuses a profile, starts a detached Runner, waits for the same
identity to see the Runner and project, then prints the MCP setup values before
diagnostic details. If the command generates a shared key, it is printed in full
only on its permitted first disclosure. A repeat connection reuses the protected
profile without redisclosing the key.

The inverse operation is:

```bash
webcodex disconnect
```

It unregisters only the exact canonical repository; do not delete the checkout,
`.git`, profile credential, or sibling project registrations.

## OAuth and managed identity are opt-in complexity

Use `share --auth oauth` or `connect --auth oauth` only when the client requires
OAuth and the exact callback URL is known. Do not collapse OAuth client secrets,
shared keys, Project Credentials, PATs, or Runner tokens into one concept.

Use the managed flow when the user explicitly needs user identity, revocation,
device authorization, audit, or organization administration:

```bash
webcodex login https://webcodex.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
```

The managed path intentionally keeps user/API authority separate from Runner
transport authority. See [Authentication](AUTH_MODEL.md) and [MCP](MCP.md) for
the reference model.

## What the AI may inspect

Safe non-secret observations include:

- repository path and Git status;
- Server URL;
- profile/state/log **paths**;
- service status via `webcodex agent status` / `webcodex server status`;
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
- full `agent.toml` contents or Server env files;
- `Authorization` headers.

When a credential must be entered into ChatGPT/Claude, identify the source
precisely and ask the **human** to copy it. The successful `share`/`connect` output
is the preferred first-disclosure source. If a stored value must be recovered,
point the user to the exact protected file/field without echoing it yourself.
Status/log commands intentionally do not reveal secrets.

Never substitute a `wc_agent_*` Runner token for an MCP token, never use a
bootstrap `WEBCODEX_TOKEN` as an MCP credential, and never assume offline
`webcodex token generate` material is registered on a remote Server.

## Troubleshooting

Use the CLI's actionable error first. For established state, inspect only the
minimum non-secret diagnostics needed:

```bash
webcodex status
webcodex doctor
webcodex agent status --profile <profile>
webcodex agent logs --profile <profile> --lines 100
```

See [Troubleshooting](TROUBLESHOOTING.md) for stable failure codes and operator
checks.
