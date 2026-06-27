# WebCodex Runtime

WebCodex is a self-hosted runtime that exposes controlled project tools to ChatGPT GPT Actions and MCP clients. A server hosts the API surface, and one or more agents execute filesystem, git, shell, and optional Codex CLI work inside registered projects.

## Current entry points

- `webcodex` — server binary.
- `webcodex-agent` — project execution agent.
- `webcodex-cli` — recommended management and initialization CLI.

Recommended binary deployment flow:

```bash
# Server: install webcodex and webcodex-cli binaries first.
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex-cli server status --env-file /etc/webcodex/webcodex.env
```

Use `webcodex-cli server install-service --overwrite` only when intentionally replacing an old unit. `server init` creates only the server bootstrap/admin `WEBCODEX_TOKEN` in `/etc/webcodex/webcodex.env`. That file is server-side only; it is not a client credential and does not contain `wc_pat_...` user API tokens or `wc_agent_...` agent tokens.

Recommended enrollment flow:

```bash
# Server/admin side: creates a short-lived wc_pair_* code, not token files.
webcodex-cli pairing create \
  --server-url https://example.com \
  --env-file /etc/webcodex/webcodex.env \
  --username alice \
  --client-id alice-laptop

# Client: install webcodex-agent and webcodex-cli binaries first.
sudo webcodex-cli client enroll \
  --server-url https://example.com \
  --pairing-code <temporary_pairing_code> \
  --client-id alice-laptop \
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml
sudo webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin /opt/webcodex/bin/webcodex-agent
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent
webcodex-cli agent status --config /etc/webcodex/agent.toml
webcodex-cli doctor --strict \
  --server-url https://example.com \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token
```

`pairing create` runs on the server/admin side. `client enroll` runs on the client side and writes `webcodex-user-token`, `webcodex-agent-token`, and `agent.toml` under the client config directory with `0600` permissions on Unix. Copy only the short-lived `wc_pair_*` code between machines; do not copy `WEBCODEX_TOKEN`, `wc_pat_*`, or `wc_agent_*` from server to client. GPT Actions should use the client-side user-token file.

Run non-destructive diagnostics with:

```bash
webcodex-cli doctor --server-url https://example.com --user-token-file ~/.config/webcodex/webcodex-user-token
```

The older `webcodex users`, `webcodex tokens`, `webcodex agent-tokens`, `webcodex-cli setup single-user`, and `webcodex-agent init` commands still work as compatibility entry points.

## Runtime surfaces

- GPT Actions import: `GET /openapi.json`.
- MCP endpoint: `POST /mcp`.
- Runtime health: `POST /api/runtime/status`.
- Agent WebSocket: `GET /api/agents/ws`.

GPT Actions and MCP share the same `ToolRuntime`. The GPT Actions OpenAPI surface is intentionally limited to project/runtime/job tools and does not expose user, API-token, agent-token, pairing/enrollment, setup, doctor, npm, server management, or audit endpoints.

GPT Actions need a public HTTPS URL. WebCodex CLI does not automate reverse proxy or tunnel setup.

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
