# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

Self-hosted runtime for letting ChatGPT GPT Actions and MCP clients work on private code through a controlled server and a local execution agent.

WebCodex is for developers and teams who want an AI assistant to inspect repositories, edit files, run Git/test/build commands, and optionally launch Codex CLI workflows without handing project execution to a hosted black box.

## Why it exists

Most AI coding integrations force a trade-off:

| Common approach | Problem |
| --- | --- |
| One-off scripts behind an HTTP endpoint | Hard to discover, audit, scope, or reuse safely. |
| Local-only MCP servers | Good for desktop clients, but not enough for ChatGPT GPT Actions or remote workflows. |
| Temporary tunnels to a laptop | URL churn, weak lifecycle control, and awkward client reconfiguration. |
| Hosted coding agents | Convenient, but project execution leaves your machine or trusted host. |

WebCodex provides a stable remote entry point while keeping the actual repository and command execution on a machine you control.

## How it works

```text
ChatGPT GPT Action / MCP client
        |
        | HTTPS + wc_pat_xxx
        v
WebCodex server
        |
        | agent transport + wc_agent_xxx
        v
webcodex-agent
        |
        v
registered project directory
```

The server exposes GPT Actions, MCP, and runtime APIs. The agent connects back to the server and performs allowed work inside registered project directories. GPT Actions and MCP use a personal API token; the agent uses a separate agent token bound to its `client_id`.

## What it can do

- Expose controlled project tools to ChatGPT GPT Actions.
- Expose the same runtime through an MCP endpoint.
- Route tool calls to connected agents instead of reading private project paths directly from the server.
- Read files, list files, search text, inspect Git status/diffs, validate/apply patches, and run bounded project commands.
- Prefer structured source edits with `replace_line_range`, `insert_at_line`, and `delete_line_range` when line numbers are known.
- Run Rust-oriented checks through structured Cargo helpers when configured.
- Optionally start Codex CLI jobs on an agent machine that already has Codex CLI installed and authenticated.
- Track task sessions with `start_session`, `session_summary`, and session-aware `show_changes`.
- Use `ToolMetadata` and `ToolKernel` foundation for consistent OAuth scope checks and session recording across REST and MCP.
- Separate credentials for admins, account onboarding, GPT/MCP tokens, and agents.

## What WebCodex is not

- It is not a hosted code runner. The agent performs project execution on your own machine or server.
- It is not a raw tunnel replacement. The server keeps a stable GPT/MCP-facing API and applies its own auth/tool boundaries.
- It is not a reason to put root/admin credentials into GPT Actions. GPT Actions and MCP should use `wc_pat_xxx` only.
- It is not a complete external MCP marketplace. The current runtime exposes WebCodex tools; broker-style registration of arbitrary external MCP servers is future work.

## Current status

| Capability | Status |
| --- | --- |
| GPT Actions runtime tools | Working; use `/openapi.json` with Bearer/API-key auth. |
| MCP endpoint | Working; uses the same `ToolRuntime` as GPT Actions. |
| Agent-backed project registry | Working; project ids use `agent:<client_id>:<project_id>`. |
| Structured line edits | Working; preferred for scoped source edits with known line numbers. |
| Git/file/patch/shell/Cargo tools | Working; shell execution should remain bounded and project-scoped. |
| Codex CLI job launcher | Optional; requires Codex CLI on the agent machine. |
| Release artifacts | Planned v0.2.0 GitHub release will publish `linux-x64`, `linux-arm64`, and `darwin-arm64` artifacts. |
| Windows and `darwin-x64` binaries | Not planned for v0.2.0 release artifacts. |

## Quick start

This local demo runs on one machine without `sudo`, `/etc`, systemd, HTTPS, Nginx, or QUIC. It is meant for evaluation. For a real deployment with services, HTTPS, remote agents, and GPT Actions, use [docs/QUICK_START.md](docs/QUICK_START.md) and [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

### 1. Install

```bash
npm install -g @yyjeqhc/webcodex
```

Or download platform binaries from the GitHub release. The npm wrapper currently installs v0.1.0 binaries; v0.2.0 users should download GitHub release binaries directly until a later npm release updates the wrapper.

### 2. Start a local server

```bash
mkdir -p .webcodex/data .webcodex/projects.d

webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir "$PWD/.webcodex/data" \
  --env-file "$PWD/.webcodex/server.env" \
  --public-url http://127.0.0.1:8080

set -a
. "$PWD/.webcodex/server.env"
set +a

WEBCODEX_ENV_FILE="$PWD/.webcodex/server.env" webcodex
```

Keep that server process running. `server init` created `.webcodex/server.env`, including the bootstrap/admin `WEBCODEX_TOKEN`. Do not use this token for GPT Actions, MCP, or agents.

### 3. Create local user, PAT, and agent token

In another terminal from the same directory:

```bash
set -a
. "$PWD/.webcodex/server.env"
set +a

webcodex-cli users create \
  --server-url http://127.0.0.1:8080 \
  --token "$WEBCODEX_TOKEN" \
  --username alice \
  --display-name "Alice" \
  --role user \
  --issue-credential
```

Copy the returned `wc_acct_xxx` account credential, then create a PAT and an agent token:

```bash
export WEBCODEX_ACCOUNT_CREDENTIAL=<wc_acct_xxx from the previous command>

webcodex-cli token create-local \
  --server http://127.0.0.1:8080 \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name local-demo \
  --scopes runtime:read,project:read,project:write,job:run

webcodex-cli agent-token create-local \
  --server http://127.0.0.1:8080 \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --client-id local-dev \
  --name local-dev
```

Save the returned `wc_pat_xxx` as `WEBCODEX_PAT` and the returned `wc_agent_xxx` as `WEBCODEX_AGENT_TOKEN`.

### 4. Register this repo and start a local agent

```bash
export WEBCODEX_AGENT_TOKEN=<wc_agent_xxx from the previous step>

webcodex-agent init \
  --server-url http://127.0.0.1:8080 \
  --token "$WEBCODEX_AGENT_TOKEN" \
  --client-id local-dev \
  --owner alice \
  --display-name "Local Dev" \
  --transport auto \
  --projects-dir "$PWD/.webcodex/projects.d" \
  --allowed-root "$PWD" \
  --output "$PWD/.webcodex/agent.toml" \
  --overwrite

cat > "$PWD/.webcodex/projects.d/webcodex.toml" <<EOF
id = "webcodex"
path = "$PWD"
name = "WebCodex"
kind = "repo"
allow_patch = true

[hooks]
status = ["git status --short"]
EOF

webcodex-agent --config "$PWD/.webcodex/agent.toml"
```

`auto` tries QUIC only when `[quic]` is configured. This local demo has no `[quic]` section, so the agent starts on the WebSocket fallback.

### 5. Test the runtime API

In a third terminal:

```bash
export WEBCODEX_PAT=<wc_pat_xxx from step 3>

curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/list \
  -d '{}'
```

The demo project id is `agent:local-dev:webcodex`. For service mode, no-service background mode, HTTPS, GPT Actions, MCP, and QUIC, continue with [docs/QUICK_START.md](docs/QUICK_START.md) and [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## Create your own GPT

GPT Actions are one of the main reasons to use WebCodex: your GPT gets a structured, scoped runtime instead of a pile of custom scripts.

1. Create a GPT in ChatGPT.
2. Add an Action.
3. Import the OpenAPI schema from `https://your-domain.example/openapi.json`.
4. Configure authentication as Bearer/API key in the GPT Action settings.
5. Use a `wc_pat_xxx` personal API token. Do not use `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx`.
6. Test `listRuntimeTools` and `callRuntimeTool` against a registered project such as `agent:workstation:my-repo`.

See [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) for the full GPT Action setup guide and supported tool surface.

## Use with MCP

WebCodex exposes a remote MCP endpoint backed by the same runtime used by GPT Actions.

- Endpoint: `https://your-domain.example/mcp`
- Auth: Bearer `wc_pat_xxx`
- Runtime: the same `ToolRuntime` used by GPT Actions
- Project ids: `agent:<client_id>:<project_id>`
- Token boundary: do not use `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx` for MCP

See [docs/MCP.md](docs/MCP.md) for client configuration examples and troubleshooting.

## Credential model

| Credential | Used by | Purpose | Do not use for |
| --- | --- | --- | --- |
| `WEBCODEX_TOKEN` | server admin | bootstrap/root admin | GPT/MCP/agent daily use |
| `wc_acct_xxx` | user CLI | create local PAT/agent token | GPT/MCP/agent |
| `wc_pat_xxx` | GPT Action/MCP/API | runtime tools | agent connection |
| `wc_agent_xxx` | `webcodex-agent` | connect agent to server | GPT/MCP/runtime API |

The server stores only hashes for user-created PATs and agent tokens. See [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md) for the full credential model.

## Documentation

- Release notes: [docs/RELEASE_NOTES_v0.2.0.md](docs/RELEASE_NOTES_v0.2.0.md)
- Operations guide: [docs/OPERATIONS.md](docs/OPERATIONS.md)
- Install and deploy: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) / [简体中文](docs/DEPLOYMENT.zh-CN.md)
- Create a GPT Action: [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) / [简体中文](docs/GPT_ACTIONS.zh-CN.md)
- Quick start: [docs/QUICK_START.md](docs/QUICK_START.md) / [简体中文](docs/QUICK_START.zh-CN.md)
- Use with MCP: [docs/MCP.md](docs/MCP.md) / [简体中文](docs/MCP.zh-CN.md)
- Credential model: [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md) / [简体中文](docs/AUTH_MODEL.zh-CN.md)
- Agent projects: [docs/AGENT_PROJECTS.md](docs/AGENT_PROJECTS.md) / [简体中文](docs/AGENT_PROJECTS.zh-CN.md)
- Agent transports: [docs/AGENT_TRANSPORTS.md](docs/AGENT_TRANSPORTS.md) / [简体中文](docs/AGENT_TRANSPORTS.zh-CN.md)
- Shell profiles: [docs/SHELL_PROFILES.md](docs/SHELL_PROFILES.md) / [简体中文](docs/SHELL_PROFILES.zh-CN.md)
- Troubleshooting: [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) / [简体中文](docs/TROUBLESHOOTING.zh-CN.md)
- Full documentation index: [docs/INDEX.md](docs/INDEX.md) / [简体中文](docs/INDEX.zh-CN.md)

## Security notes

- Put the server behind HTTPS before connecting GPT Actions, MCP clients, or remote agents.
- Keep `WEBCODEX_TOKEN` server-side. It is a bootstrap/admin credential, not an integration token.
- Prefer one `wc_pat_xxx` per GPT Action, MCP client, or automation surface.
- Prefer one `wc_agent_xxx` per agent `client_id`.
- Use structured file edit tools before falling back to shell-based edits.
- Review [SECURITY.md](SECURITY.md) before exposing a server on the public internet.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
