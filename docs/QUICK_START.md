# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This guide gets one local Git repository into ChatGPT through MCP with the fewest
concepts possible. On Linux/macOS the normal first-run command is `webcodex share`.
Windows does not support the local Server runtime used by `share`; Windows users
need an existing remote Linux Server and should use `webcodex connect <server-url>`
instead. If no Server exists yet, see [Deployment](DEPLOYMENT.md).

## Prerequisites

- Node.js 18+ for the npm installer.
- Git on `PATH` and a Git repository you can safely inspect/edit.
- On Linux/macOS, [`cloudflared`](https://developers.cloudflare.com/tunnel/downloads/)
  on `PATH` for the default temporary public HTTPS share.

Install WebCodex:

```bash
npm install -g @yyjeqhc/webcodex
```

On Linux/macOS, local MCP debugging with `webcodex share --tunnel none` does not
need `cloudflared`.

## 1. Share the repository

```bash
cd /path/to/your/repository
webcodex share
```

That single command performs the project setup, starts the local Server + Runner,
creates a temporary Connector credential, opens a Cloudflare Quick Tunnel, and
waits for the MCP endpoint to become usable. Do not run `setup`, `doctor`, or
`run` first unless you specifically want the manual/local workflow described
later.

The default share is temporary. Keep the terminal open; Ctrl-C stops the local
runtime, tunnel, URL, and temporary credential.

If `cloudflared` is missing, WebCodex fails before project setup/state creation
and tells you where to install it. Install it and retry, or use `--tunnel none`
for local-only debugging.

## 2. Add WebCodex to ChatGPT

When the terminal reports **WebCodex ready**, use the values printed under
**What to do next**:

1. In ChatGPT, enable **Developer Mode** and create a **custom app** using MCP.
2. Set **MCP URL** to the printed `https://.../mcp` value.
3. For the default share, choose **Access token / API key** (Bearer token).
4. Paste the printed **Credential (this share only)**.
5. Run **Scan Tools**.

The Console intentionally does not display that credential. If you later open
`/console`, get connection credentials from the successful CLI output, not from
the browser page.

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

If you already have a WebCodex Server URL, use `connect` instead of temporary
`share`:

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example
```

`connect` creates/reuses a local profile, starts a detached Runner, waits until
the Server can see the Runner and project, then prints the MCP URL,
authentication type, credential source, ChatGPT hint, and diagnostic details.
When it generates a shared key, the full value is shown only on the permitted
first disclosure; status/log commands do not reveal it.

To remove only this repository from a hosted profile later:

```bash
webcodex disconnect
```

Self-hosting and managed identity are separate operator/advanced workflows; see
[Deployment](DEPLOYMENT.md).

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
| `cloudflared` missing | Install it from the official Cloudflare downloads and retry, or use `share --tunnel none` locally |
| loopback port already in use | Stop the conflicting process and retry |
| local/manual runtime stopped | `webcodex run` |
| Runner unavailable on an existing hosted profile | rerun `connect` or inspect `webcodex agent status --profile <profile>` |
| workspace unavailable | restore the Git project/path |

See [Troubleshooting](TROUBLESHOOTING.md) for the full operational checklist.
