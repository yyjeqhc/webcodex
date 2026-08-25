# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This guide gets one local Git repository into ChatGPT through MCP with the fewest
concepts possible. On Linux/macOS, bare `webcodex` in an interactive Git checkout
is the shortest first-run entry and dispatches to the normal `webcodex share`
workflow. Windows does not support the local Server runtime used by `share`;
Windows users need an existing remote Linux Server and should use
`webcodex connect <server-url>`
instead. If no Server exists yet, see [Deployment](DEPLOYMENT.md).

## Prerequisites

- Node.js 18+ for the npm installer.
- Git on `PATH` and a Git repository you can safely inspect/edit.
- No separate `cloudflared` installation is required for the default Linux/macOS
  temporary public share. WebCodex reuses an explicit/PATH binary or downloads a
  pinned, verified managed copy when needed.
  Through the npm wrapper, managed acquisition also inherits npm proxy,
  `noproxy`, CA, and `strict-ssl` settings; standard proxy/system trust remains
  the fallback outside npm-specific configuration.

For a one-shot trial without a global install:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

Or install the CLI once:

```bash
npm install -g @yyjeqhc/webcodex
```

The npm wrapper can lazily bootstrap the verified native binaries on first
execution, so the npx path does not depend on npm preserving postinstall output.
On Linux/macOS, local MCP debugging with `webcodex share --tunnel none` does not
need `cloudflared`.

## 1. Share the repository

The `npx --yes @yyjeqhc/webcodex` command above already enters this first-run
path. If you installed the CLI globally instead, run:

```bash
cd /path/to/your/repository
webcodex
# explicit/script-friendly equivalent:
# webcodex share
```

Bare `webcodex` only auto-dispatches this way for an interactive Linux/macOS
terminal inside a Git checkout. Non-interactive invocations and non-repository
directories still show the normal CLI help. The explicit `share` command remains
the deterministic entry for scripts.

That single first-run path performs the project setup, starts the local Server +
Runner, creates a temporary Connector credential, opens a Cloudflare Quick Tunnel, and
waits for the MCP endpoint to become usable. Do not run `setup`, `doctor`, or
`run` first unless you specifically want the manual/local workflow described
later.

The default share is temporary. Keep the terminal open; Ctrl-C stops the local
runtime, tunnel, URL, and temporary credential.

If `cloudflared` is missing, WebCodex acquires a pinned Cloudflare release into
private user state before project setup/state creation, verifies the artifact and
binary, and then continues. Set `WEBCODEX_CLOUDFLARED_BIN` to force a trusted
binary, or use `--tunnel none` for local-only debugging.

## 2. Add WebCodex to ChatGPT

When the terminal reports **WebCodex ready**, the printed values remain the
source of truth. For a public share, WebCodex best-effort copies the MCP URL to
the clipboard but never copies the credential automatically. In an interactive
terminal, press Enter to open ChatGPT App settings. Then:

1. In ChatGPT, enable **Developer Mode** and go to **Settings -> Apps -> Create**.
2. Paste the copied **MCP URL**, or use the printed `https://.../mcp` fallback.
3. For the default share, choose **Access token / API key** (Bearer token).
4. Paste the printed **Credential (this share only)**.
5. Run **Scan Tools**.

Use `webcodex share --no-copy-url` to suppress the clipboard attempt.

The Console intentionally does not display that credential. If you later open
`/console`, get connection credentials from the successful CLI output, not from
the browser page. ChatGPT Developer Mode, custom MCP apps, and write/modify
actions are separately controlled by the ChatGPT plan, workspace, and admin
settings; WebCodex cannot enable an action that the client workspace does not
permit.

## 3. Send a safe first prompt

Start by confirming that the client can see the intended repository without
making changes:

```text
Inspect this repository and summarize its structure. Do not make changes.
```

After that succeeds, ask for a small, reversible change. WebCodex's project-bound
coding surface handles project identity internally; ordinary users do not need
to provide runtime project ids or operation ids in prompts.

## 4. Review work

The browser `/console` shows readiness, work queue, task guidance, approvals, and
review actions. Where a stable result is ready, a human can Accept or Reject it
from the Console or CLI:

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
# or: webcodex task reject <task-id>
```

The online model cannot accept its own result.

## Existing Server: long-lived connection

If an existing hosted Server is intentionally configured for shared-key clients,
use `connect` with the shared key supplied by its operator instead of temporary
`share`:

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example --key-file /private/path/shared-key
```

`connect` creates/reuses a local profile, starts a detached Runner, waits until
the Server can see the Runner and project, then prints the MCP URL,
authentication type, credential source, ChatGPT hint, and diagnostic details.
The shared key is a hosted-client credential; it is not the bootstrap
administrator token of a self-hosted Server.

To remove only this repository from a hosted profile later:

```bash
webcodex disconnect
```

If you need a Server first, the recommended Docker Server path is clone-free and
uses three shell commands; see [Deployment](DEPLOYMENT.md#docker-server-only).
After that bootstrap, enroll repository machines with a short-lived pairing code
and `webcodex login`, then explicitly install the reported Runner config. Keep
the Server `.env` and its bootstrap administrator token on the Server.

## Optional OAuth

Bearer authentication is the simplest trial path. If the MCP client requires
OAuth, provide its exact callback URL.

Temporary project-bound share:

```bash
webcodex share --auth oauth \
  --oauth-redirect-uri https://client.example/callback
```

Existing hosted Server:

```bash
webcodex connect https://webcodex.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback
```

The CLI prints which values belong in the MCP client and which temporary project
credential belongs only on the WebCodex authorization page. Advanced OAuth
scope ceilings, optional Computer permissions, managed-user OAuth, and protocol
contracts are documented in [MCP](MCP.md) and [Authentication](AUTH_MODEL.md).

## Local-only / manual workflow

These commands remain useful for development and diagnostics, but they are not
prerequisites for a hosted ChatGPT connection:

```bash
cd /path/to/your/repository
webcodex setup     # configure private project state only
webcodex doctor    # read-only local readiness diagnostics
webcodex run       # foreground loopback Server + Runner
# in another terminal:
webcodex status
```

`doctor` describes the local/manual runtime and may recommend `webcodex run` when
that loopback runtime is stopped. Hosted clients cannot reach a loopback-only
runtime, which is why the normal ChatGPT onboarding starts with `share` instead.

## Troubleshooting

Start with the exact error from `share` or `connect`. For established local
state, these remain useful:

```bash
webcodex status
webcodex doctor
```

Common examples:

| Symptom | Next action |
| --- | --- |
| managed `cloudflared` acquisition fails | Check network/proxy access and retry; alternatively set `WEBCODEX_CLOUDFLARED_BIN` to a trusted binary or use `share --tunnel none` locally |
| loopback port already in use | Stop the conflicting process and retry |
| local/manual runtime stopped | `webcodex run` |
| Runner unavailable on an existing hosted profile | rerun `connect` or inspect `webcodex agent status --profile <profile>` |
| workspace unavailable | restore the Git project/path |

See [Troubleshooting](TROUBLESHOOTING.md) for the full operational checklist.
