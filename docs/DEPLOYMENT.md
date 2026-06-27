# Deployment

This guide covers the current WebCodex production shape: server, HTTPS reverse proxy, `webcodex-cli` setup, agent configuration, GPT Actions, MCP, and smoke checks.

## Components

- `webcodex`: server exposing REST, GPT Actions OpenAPI, MCP, and agent endpoints.
- `webcodex-agent`: long-lived worker connected by WebSocket or polling.
- `webcodex-cli`: recommended management CLI for users, tokens, setup, and agent config generation.

## Server configuration

Required production settings usually include:

```text
WEBCODEX_TOKEN=<bootstrap-admin-token>
WEBCODEX_PUBLIC_URL=https://your-domain.example
WEBCODEX_ADDR=127.0.0.1:8080
WEBCODEX_DATA=/var/lib/webcodex
```

Use the bootstrap token only for initial setup/admin work. Day-to-day GPT Actions and MCP calls should use a user API token. Agents should use agent tokens.

## Recommended setup CLI

Create the first user token and agent token with:

```bash
webcodex-cli setup single-user
```

This is the recommended one-shot initialization entry point. The compatibility commands remain available:

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
```

Prefer `webcodex-cli` in new docs and automation.

## Agent configuration

Generate the agent config with:

```bash
webcodex-cli agent init
```

`webcodex-agent init` remains available as a compatibility entry point.

Important agent settings:

| Setting | Notes |
| --- | --- |
| `server_url` | Public WebCodex URL. |
| `token` | Agent token. Do not commit or print it. |
| `client_id` | Stable id used in `agent:<client_id>:<project_id>`. |
| `owner` | Owner principal for this agent. |
| `transport` | Prefer `websocket`; polling is fallback. |
| `projects_dir` | Directory of project registry files. |
| `[policy]` | Local execution boundary. |
| `[shell]` | Optional shell program/PATH/env customization for commands. |

Policy behavior:

- Missing or empty `allowed_roots` defaults to `$HOME`.
- Explicit `allowed_roots` overrides the `$HOME` default.
- Use explicit roots when you want to narrow the agent, for example to one workspace tree.

Example narrow policy:

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

`runtime_status` and `listAgents` expose only a redacted policy summary: `allow_raw_shell`, `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`, and `max_output_bytes`. They do not expose tokens, env values, `Authorization` headers, full `agent.toml`, or shell `init_script` values.

## Authentication and transport

Ordinary REST, polling, MCP, and GPT Actions calls must use:

```text
Authorization: Bearer <token>
```

`?token=` is allowed only for `/api/agents/ws` WebSocket handshake compatibility. Do not use query-string tokens for polling, REST, MCP, or GPT Actions.

WebSocket is preferred for agents. Polling remains available for constrained networks.

## GPT Actions and MCP

Import GPT Actions from:

```text
https://your-domain.example/openapi.json
```

Configure GPT Actions authentication as HTTP Bearer/API key in the `Authorization` header.

The OpenAPI GPT Actions management surface intentionally excludes users, API tokens, agent tokens, setup, and audit management endpoints. Use `webcodex-cli` for those tasks.

MCP uses the same user API token and the same `ToolRuntime` as GPT Actions.

## Optional Codex CLI jobs

`runCodexTask` is optional. It requires the Codex CLI to be installed and configured on the agent machine. It does not start a new `webcodex-agent`; it delegates work to an already connected agent.

## Smoke checks

Recommended production smoke sequence:

1. `POST /api/runtime/status` returns `service=webcodex` and the expected public URL.
2. `listAgents` shows at least one online agent.
3. `listProjects` shows `agent:<client_id>:<project_id>` ids.
4. Read-only project tools work on a known project.
5. Write/replace/validate tests are limited to disposable smoke projects.
