# WebCodex

Self-hosted runtime for ChatGPT GPT Actions and MCP clients to work with your private projects.

![WebCodex architecture](docs/assets/architecture.png)

## What is WebCodex?

WebCodex lets ChatGPT GPT Actions and MCP clients safely operate your private repositories through a self-hosted server and connected agent.

It is for developers and teams who want AI assistants to inspect, edit, test, and automate private code without handing project execution to a hosted black box. You run the server, you run the agent, and your repositories stay on your own machine.

## What it can do

- Expose controlled project tools to ChatGPT GPT Actions.
- Expose the same runtime through MCP.
- Let a connected agent execute file, git, shell, patch, cargo, and optional Codex CLI workflows.
- Keep project execution on your own machine.
- Separate credentials for admins, account onboarding, GPT/MCP tokens, and agents.

## Why WebCodex?

| Without WebCodex | With WebCodex |
| --- | --- |
| Custom ad-hoc scripts | Structured runtime tools |
| Long-lived root token reused everywhere | Separated `wc_acct` / `wc_pat` / `wc_agent` credentials |
| GPT cannot safely reach private repos | GPT/MCP use a scoped PAT against a controlled runtime |
| Agents are hard to enroll | `agent-token create-local` + `client_id` binding |

## Architecture

![Architecture](docs/assets/architecture.png)

```text
ChatGPT GPT Action / MCP client
        ↓
WebCodex server
        ↓
webcodex-agent
        ↓
registered project on your machine
```

The server exposes GPT Actions, MCP, and runtime APIs. The agent connects back to the server and performs allowed work inside registered project directories. GPT Actions and MCP use a personal API token; the agent uses a separate agent token bound to its `client_id`.

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

![Import OpenAPI](docs/assets/gpt-action-import-openapi.png)
![Configure GPT Action auth](docs/assets/gpt-action-auth.png)

See [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) for the full GPT Action setup guide and supported tool surface.

## Use with MCP

- MCP endpoint: `https://your-domain.example/mcp`
- Auth: Bearer `wc_pat_xxx`
- Runtime: the same `ToolRuntime` used by GPT Actions
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

## Screenshots

![Runtime tools](docs/assets/runtime-tools.png)
![GPT Action setup](docs/assets/gpt-action-auth.png)
![Agent project online](docs/assets/agent-project-online.png)

Screenshot placeholders are tracked in [docs/assets/README.md](docs/assets/README.md); maintainers can add the image files manually.

## Documentation

- Install and deploy: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- Create a GPT Action: [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md)
- Use with MCP: [docs/MCP.md](docs/MCP.md)
- Credential model: [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md)
- Agent projects: [docs/AGENT_PROJECTS.md](docs/AGENT_PROJECTS.md)
- Troubleshooting: [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- sg4 smoke test: [docs/smoke-test-sg4.md](docs/smoke-test-sg4.md)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
