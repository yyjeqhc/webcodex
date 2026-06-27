# WebCodex Runtime

WebCodex is a self-hosted runtime that exposes controlled project tools to ChatGPT GPT Actions and MCP clients. A server hosts the API surface, and one or more agents execute filesystem, git, shell, and optional Codex CLI work inside registered projects.

## Current entry points

- `webcodex` — server binary.
- `webcodex-agent` — project execution agent.
- `webcodex-cli` — recommended management and initialization CLI.

Recommended first setup:

```bash
webcodex-cli setup single-user
```

That command is the preferred one-shot entry point for creating a user API token for GPT Actions/MCP and an agent token for `webcodex-agent`.

Recommended agent config generation:

```bash
webcodex-cli agent init
```

The older `webcodex users`, `webcodex tokens`, `webcodex agent-tokens`, and `webcodex-agent init` commands still work as compatibility entry points, but new documentation should prefer `webcodex-cli`.

## Runtime surfaces

- GPT Actions import: `GET /openapi.json`.
- MCP endpoint: `POST /mcp`.
- Runtime health: `POST /api/runtime/status`.
- Agent WebSocket: `GET /api/agents/ws`.

GPT Actions and MCP share the same `ToolRuntime`. The GPT Actions OpenAPI surface is intentionally limited to project/runtime/job tools and does not expose user, API-token, agent-token, setup, or audit management endpoints.

## Authentication

Production APIs use HTTP Bearer authentication:

```bash
curl -X POST \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{}' \
  https://example.com/api/runtime/status
```

`?token=` is accepted only for `/api/agents/ws` WebSocket handshake compatibility. Polling, REST, MCP, and GPT Actions ordinary API calls must use `Authorization: Bearer ...`.

Never commit real tokens, env files, `Authorization` headers, or complete `agent.toml` files.

## Agent projects and policy

Agents report project files from their configured `projects_dir`. Project ids surfaced to GPT Actions use this form:

```text
agent:<client_id>:<project_id>
```

Agent policy controls execution boundaries. When `allowed_roots` is omitted or empty, the agent defaults to `$HOME`. If `allowed_roots` is configured explicitly, that list replaces the `$HOME` default.

Example: to deliberately narrow an agent to one workspace tree, configure an explicit root:

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
```

`runtime_status` and `listAgents` expose a redacted policy summary for observability: `allow_raw_shell`, `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`, and `max_output_bytes`. They do not expose tokens, env values, `Authorization` headers, the full `agent.toml`, or shell `init_script` values.

## Optional Codex CLI jobs

`runCodexTask` is an optional advanced feature. It requires the Codex CLI to be installed and configured on the agent machine. Calling `runCodexTask` does not start a new `webcodex-agent`; it asks the already connected agent to run Codex inside a registered project.

All non-Codex project tools, including read, git, patch validation, patch application, file write, and shell tools, can work without the Codex CLI.

## Documentation

Start here:

- [docs/INDEX.md](docs/INDEX.md) — documentation map.
- [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md) — installation quick reference.
- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) — production deployment guide.
- [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) — GPT Actions import and tool usage.
- [docs/AGENT_PROTOCOL.md](docs/AGENT_PROTOCOL.md) — agent auth, transports, and observability.
- [docs/AGENT_PROJECTS.md](docs/AGENT_PROJECTS.md) — project registry and project management tools.
- [docs/E2E_VALIDATION.md](docs/E2E_VALIDATION.md) — local end-to-end validation.
