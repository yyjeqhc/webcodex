# WebCodex

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
| Release artifacts | v0.1.0 includes `linux-x64`, `linux-arm64`, and `darwin-arm64`. |
| Windows and `darwin-x64` binaries | Not included in v0.1.0 release artifacts. |

## Quick start

This is the shortest path from zero to a working private project runtime. For production deployment details, service files, reverse proxy setup, and the full sg4 smoke record, see [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) and [docs/smoke-test-sg4.md](docs/smoke-test-sg4.md).

### 1. Install

```bash
npm install -g @yyjeqhc/webcodex
```

Or download platform binaries from the project release artifacts.

### 2. Start a server

```bash
webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env

WEBCODEX_ENV_FILE=/etc/webcodex/webcodex.env webcodex
```

Put the server behind your own HTTPS domain before connecting GPT Actions or remote agents.

### 3. Create a user and account credential

```bash
webcodex-cli user create \
  --server https://your-domain.example \
  --admin-token "$WEBCODEX_TOKEN" \
  --username alice \
  --display-name "Alice" \
  --role user \
  --issue-credential
```

This issues a one-time `wc_acct_xxx` account credential for local token creation. It is not a GPT/MCP token and it is not an agent token.

### 4. User creates a PAT for GPT Actions, MCP, and runtime APIs

```bash
webcodex-cli token create-local \
  --server https://your-domain.example \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

Use the generated `wc_pat_xxx` as the bearer/API-key value in GPT Actions and MCP clients.

### 5. User creates an agent token

```bash
webcodex-cli agent-token create-local \
  --server https://your-domain.example \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --client-id alice-laptop \
  --name alice-laptop
```

Use the generated `wc_agent_xxx` only for `webcodex-agent`.

### 6. Initialize the agent

```bash
webcodex-agent init \
  --server-url https://your-domain.example \
  --token "$WEBCODEX_AGENT_TOKEN" \
  --client-id alice-laptop \
  --owner alice \
  --display-name "Alice Laptop" \
  --transport websocket \
  --projects-dir ~/.config/webcodex/projects.d \
  --allowed-root ~/git \
  --output ~/.config/webcodex/agent.toml \
  --overwrite
```

### 7. Register a project

Create `~/.config/webcodex/projects.d/my-repo.toml` on the agent machine:

```toml
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true

[hooks]
status = ["git status --short"]
check = ["cargo check --all-targets"]
```

Then start the agent:

```bash
webcodex-agent --config ~/.config/webcodex/agent.toml
```

Runtime project ids use this form:

```text
agent:<client_id>:<project_id>
```

For example: `agent:alice-laptop:my-repo`.

### 8. Test the runtime tool list

```bash
curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  https://your-domain.example/api/tools/list \
  -d '{}'
```

## Create your own GPT

GPT Actions are one of the main reasons to use WebCodex: your GPT gets a structured, scoped runtime instead of a pile of custom scripts.

1. Create a GPT in ChatGPT.
2. Add an Action.
3. Import the OpenAPI schema from `https://your-domain.example/openapi.json`.
4. Configure authentication as Bearer/API key in the GPT Action settings.
5. Use a `wc_pat_xxx` personal API token. Do not use `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx`.
6. Test `listTools` and `callRuntimeTool` against a registered project such as `agent:alice-laptop:my-repo`.

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

- Install and deploy: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- Create a GPT Action: [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md)
- Use with MCP: [docs/MCP.md](docs/MCP.md)
- Credential model: [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md)
- Agent projects: [docs/AGENT_PROJECTS.md](docs/AGENT_PROJECTS.md)
- Build/install reference: [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md)
- Troubleshooting: [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- Full documentation index: [docs/INDEX.md](docs/INDEX.md)
- sg4 smoke test: [docs/smoke-test-sg4.md](docs/smoke-test-sg4.md)

## Security notes

- Put the server behind HTTPS before connecting GPT Actions, MCP clients, or remote agents.
- Keep `WEBCODEX_TOKEN` server-side. It is a bootstrap/admin credential, not an integration token.
- Prefer one `wc_pat_xxx` per GPT Action, MCP client, or automation surface.
- Prefer one `wc_agent_xxx` per agent `client_id`.
- Use structured file edit tools before falling back to shell-based edits.
- Review [SECURITY.md](SECURITY.md) before exposing a server on the public internet.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
