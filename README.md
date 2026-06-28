# WebCodex Runtime

WebCodex is a self-hosted runtime that exposes controlled project tools to ChatGPT GPT Actions and MCP clients. A server hosts the API surface, and one or more agents execute filesystem, git, shell, and optional Codex CLI work inside registered projects.

## Install

Install the npm thin installer/wrapper:

```bash
npm install -g @yyjeqhc/webcodex
```

The npm package downloads native binaries from the GitHub Release for the current platform. You can also download release artifacts directly from:

```text
https://github.com/yyjeqhc/webcodex/releases/tag/v0.1.0
```

## Supported platforms

v0.1.0 release artifacts currently include:

- `linux-x64`
- `linux-arm64`
- `darwin-arm64`

`darwin-x64`, Windows, and other targets are not included in v0.1.0; treat them as future targets unless a later release adds artifacts.

## Quick start

Server/admin side:

```bash
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

webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --ttl-secs 600
```

Client/friend side:

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml \
  --projects-dir /etc/webcodex/projects.d \
  --allowed-root /home/friend/git

webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin /opt/webcodex/bin/webcodex-agent \
  --overwrite
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent

webcodex-cli doctor \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token \
  --strict
```

## Security model

- The server bootstrap token in `/etc/webcodex/webcodex.env` is for server/admin setup only.
- User API tokens are for human/API clients such as GPT Actions and MCP.
- Agent tokens are only for `webcodex-agent`.
- GPT Actions should use the client-side `webcodex-user-token`, not the server bootstrap token.
- Copy only short-lived `wc_pair_*` pairing codes between machines; do not copy env files, token files, or complete `agent.toml` files.

## Invite another user

Use pairing when the server owner wants to add a friend or another machine without copying long-lived credentials.

Server/admin side:

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` is server/admin-side. `/etc/webcodex/webcodex.env` is server-side only. Send only the short-lived `wc_pair_*` code to the friend.

Client/friend side:

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml \
  --projects-dir /etc/webcodex/projects.d \
  --allowed-root /home/friend/git

webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin /opt/webcodex/bin/webcodex-agent \
  --overwrite

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent

webcodex-cli doctor \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token \
  --strict
```

`client enroll` is client/friend-side. GPT Actions should use the client-side `webcodex-user-token`; `webcodex-agent` should use the client-side agent token through the generated agent config. Do not copy `WEBCODEX_TOKEN`, `wc_pat_*`, `wc_agent_*`, complete env files, or complete `agent.toml` files between machines. Each friend should use a unique `username` and `client_id`.

Run non-destructive diagnostics with:

```bash
webcodex-cli doctor --server-url https://example.com --user-token-file ~/.config/webcodex/webcodex-user-token
```

The older `webcodex users`, `webcodex tokens`, `webcodex agent-tokens`, and `webcodex-agent init` commands still work as compatibility entry points. `webcodex-cli setup single-user` remains a recommended shortcut for single-user setup; pairing/enroll is preferred when inviting another user or machine.

## Runtime surfaces

- GPT Actions import: `GET /openapi.json`.
- MCP endpoint: `POST /mcp`.
- Runtime health: `POST /api/runtime/status`.
- Agent WebSocket: `GET /api/agents/ws`.

GPT Actions and MCP share the same `ToolRuntime`. The GPT Actions OpenAPI surface is intentionally limited to project/runtime/job tools and does not expose user, API-token, agent-token, pairing/enrollment, setup, doctor, npm, server management, or audit endpoints.

GPT Actions need a public HTTPS URL. WebCodex CLI does not automate reverse proxy or tunnel setup.

## Runtime console

WebCodex serves a read-only browser console at:

```text
https://your-domain.example/console
```

The static console bundle contains no secrets. Runtime data is fetched by the browser from protected APIs using the user's credentials, session, or token as applicable. The console is not part of the GPT Actions OpenAPI and is not a full admin UI.

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

Agent project files can bind a project to a prepared shell profile. WebCodex prepares a one-time environment snapshot per project/profile (no persistent shell, no `.bashrc`/`.profile` sourced by default); see [docs/SHELL_PROFILES.md](docs/SHELL_PROFILES.md) for Rust/Cargo, Python venv, and Conda examples. Changing a profile requires restarting `webcodex-agent`.

```toml
shell_profile = "rust"
```

Example: to deliberately narrow an agent to one workspace tree, configure an explicit root:

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
```

`runtime_status`, `listAgents`, and `listProjects` expose a redacted policy summary plus a sanitized `shell_profiles` summary (profile names, `has_init_script`, `env_keys_count`, `program`, `args_count`). `listProjects` also shows each project's `resolved_shell_profile` and `shell_profile_status`. They do not expose tokens, env values, `Authorization` headers, the full `agent.toml`, the full env snapshot, or shell profile `init_script` bodies.

## Optional Codex CLI jobs

`runCodexTask` is an optional advanced feature. It requires the Codex CLI to be installed and configured on the agent machine. Calling `runCodexTask` does not start a new `webcodex-agent`; it asks the already connected agent to run Codex inside a registered project.

All non-Codex project tools, including read, git, patch validation, patch application, file write, and shell tools, can work without the Codex CLI.

## Troubleshooting

For common deployment issues and a short operational checklist, see [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md). It covers existing systemd services, local HTTP reachability, client `PATH` setup, server-side pairing vs client-side enrollment, agent-only clients, and `client online: no` checks.

## Documentation

Start here:

- [docs/INDEX.md](docs/INDEX.md) — documentation map.
- [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md) — installation quick reference.
- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) — production deployment guide.
- [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md) — GPT Actions import and tool usage.
- [docs/AGENT_PROTOCOL.md](docs/AGENT_PROTOCOL.md) — agent auth, transports, and observability.
- [docs/AGENT_PROJECTS.md](docs/AGENT_PROJECTS.md) — project registry and project management tools.
- [docs/E2E_VALIDATION.md](docs/E2E_VALIDATION.md) — local end-to-end validation.
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) — deployment troubleshooting and operational checklist.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
