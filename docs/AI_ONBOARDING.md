# AI Coding Agent Onboarding

[English](AI_ONBOARDING.md) | [简体中文](AI_ONBOARDING.zh-CN.md)

This guide is for an AI coding agent helping a **user** connect a local
repository to WebCodex or deploy a WebCodex Server. It is not the same as
[`AGENTS.md`](../AGENTS.md), which is for an AI coding agent **developing
WebCodex itself**.

Choose one path before running commands, and verify the current machine and
existing configuration before changing anything.

## Decision tree

1. Does the user want the fastest connection to one or more repositories,
   without operating a Server?
   - Yes: use **Hosted shared key** and `webcodex connect`.
2. Does the user need an individual account, device-level authorization,
   independent token revocation, identity audit, or organization management?
   - Yes: use the **Managed flow** and `webcodex login`.
3. Does the user need full infrastructure control, an internal network, their
   own HTTPS or identity system, or no dependency on the official Server?
   - Yes: use **Full self-hosting** and read
     [Deployment](DEPLOYMENT.md).

Do not deploy a WebCodex Server for the hosted path.

## Fastest connection: hosted shared key

Run on the machine that owns the repository:

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://your-server.example
```

The current directory is the default project. The command generates the shared
key unless the user explicitly supplies `--key-file` or `--key`. Configure the
MCP client from the values printed after the connection check succeeds:

```text
MCP URL: https://your-server.example/mcp
Authentication: Bearer token
Bearer token: the generated key
```

`connect` performs the complete local setup: it creates or reuses a profile
scoped to that origin and key, generates a unique client ID, registers the
local project, writes a `0600` Runner config, starts one detached Runner, and
waits until the same key can see the Runner and target project. Running the
same command again reuses the profile and live Runner.

For automation, prefer `--key-file <path>` over passing a key in shell
history. Do not pass `--key` and `--key-file` together.

## Automatic key

When `--key`/`--key-file` is omitted, `connect` generates a `wck_...` key with
more than 256 bits of randomness, stores it in the protected profile, and
prints the complete value only when first created. Tell the user to copy it
immediately into the MCP client. A repeat connection recovers the local
profile and does not print the key again.

The key is stored as the top-level `token` field in
`~/.config/webcodex/clients/<profile>/agent.toml` (or
`$XDG_CONFIG_HOME/webcodex/clients/<profile>/agent.toml`). If the user lost
the printed value, have the human copy it from that field; as an AI agent,
locate the file and point the user at it — do not echo the value into chat.

The detached Runner survives terminal closure but not a machine reboot. After
reboot, rerun the same `connect` or use `webcodex agent start --profile
<profile>`.

The inverse operation is `webcodex disconnect [--project PATH] [--profile NAME]`.
Use it when the user wants to unregister one hosted repository without deleting or modifying the
repository, `.git`, the profile credential, `agent.toml`, or sibling projects. It resolves the
canonical repository path exactly; if several hosted profiles match, let the command fail closed
and ask the user to choose one with `--profile` rather than guessing. A live Runner is unregistered
through the structured Server/Runner lifecycle before the local registration is removed.

## Managed flow

Use the managed flow when the user needs a separate user identity, token
revocation, device-level authorization, identity audit, or organization
administration:

```bash
webcodex login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
```

The managed flow uses a pairing/account credential, a PAT for MCP/API, and a
separately bound Runner token. Do not replace that split with a shared key.

## Full self-hosting

Use [Deployment](DEPLOYMENT.md) when the user needs complete Server and data
control, an internal-network deployment, their own HTTPS endpoint, their own
identity system, or no dependency on the official Server. That path includes
Server, database/state, reverse proxy, TLS, service, and credential operations;
none of those are prerequisites for hosted `webcodex connect`.

## What the AI agent may inspect

You may safely locate and read **non-secret** configuration:

- profile path: `~/.config/webcodex/clients/<profile>/` (or
  `$XDG_CONFIG_HOME/webcodex/clients/<profile>/`)
- Runner state and log path:
  `~/.local/state/webcodex/clients/<profile>/` (or
  `$XDG_STATE_HOME/webcodex/clients/<profile>/`)
- token **file** path (not its contents): e.g. `webcodex-user-token`
- env file **path** and variable **name** (e.g. `WEBCODEX_TOKEN` in the server
  env file) — not the value
- server URL
- service status via `webcodex agent status` / `webcodex server status`
- `webcodex doctor` output

Useful non-secret commands:

```bash
webcodex agent status --profile <profile>
webcodex agent logs --profile <profile> --lines 100
webcodex status
webcodex doctor
webcodex ops status --server-url <url> --token-file <path> --strict
```

## What the human must copy or paste

Do **not** read back, print, log, commit, or echo into chat:

- full token values (shared key, PAT, Runner token, account credential,
  bootstrap token, OAuth secrets)
- full `agent.toml` contents
- server env files
- `Authorization` headers
- OAuth client secrets

When a secret must be entered into ChatGPT/Claude or another client, tell the
**human** exactly which local file/value to copy. For example: "Copy the value
from `~/.config/webcodex/<server>/<user>/webcodex-user-token` into the Bearer
field." Do not read the file yourself and paste its contents into the chat.

### Credential rules for AI agents

- Never run `webcodex token generate` and assume a remote Server will accept
  its output. It creates offline material only; it does not register it.
- Never use a `wc_*` value as a hosted shared key. Unknown or revoked managed
  credentials do not fall back to shared-key auth.
- Never substitute a `wc_agent_*` for an MCP token.
- Never paste a bootstrap `WEBCODEX_TOKEN` into MCP or a local hosted profile.
- Never print, log, commit, or copy a full `agent.toml`.
- Run `connect` before configuring MCP, so the full Runner/project path is
  verified first.

## Local state and troubleshooting

For a normal non-root user, profile configuration defaults below
`~/.config/webcodex/clients/<profile>/`; Runner state and logs below
`~/.local/state/webcodex/clients/<profile>/`. The hosted Runner writes
`runner.log` in that profile state directory and rotates it while running at
approximately 10 MiB, keeping `runner.log`, `runner.log.1`, and
`runner.log.2` (all `0600` on Unix). `agent logs --lines` reads bounded tails;
`--follow` reopens `runner.log` after rotation.

On connection failure, use the profile and log path printed by `connect`.
Check Server reachability, shared-key enablement, exact key equality, client ID
collision, and project-path validity. Status and logs do not print the key. See
[Troubleshooting](TROUBLESHOOTING.md) for stable failure guidance.
