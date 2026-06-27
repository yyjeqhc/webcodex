# Build and Install Quick Reference

This is the short install path. See [DEPLOYMENT.md](DEPLOYMENT.md) for production details.

## Build binaries

Build the three current binaries for your host:

```text
webcodex
webcodex-agent
webcodex-cli
```

Do not run unauthenticated production deployments.

## Install packages

The documented distribution path assumes:

```bash
npm install -g @webcodex/server @webcodex/cli
```

## Server bootstrap

Initialize the server env file:

```bash
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
```

This creates only the server bootstrap/admin `WEBCODEX_TOKEN`. It does not create user API tokens or agent tokens.

Install and start the service:

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex-cli server status --env-file /etc/webcodex/webcodex.env
```

User and agent token setup is a separate client-side setup/enroll flow.

GPT Actions require a public HTTPS URL. WebCodex CLI does not automate reverse proxies or tunnels; configure nginx, Caddy, Cloudflare Tunnel, ngrok, or similar infrastructure separately if needed.

Compatibility commands still work, but should not be the first choice in new docs:

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
webcodex-cli setup single-user
```

## Agent config

Recommended agent config generation:

```bash
webcodex-cli agent init
```

`webcodex-agent init` remains available as a compatibility entry point.

Agent policy defaults:

- Missing or empty `allowed_roots` defaults to `$HOME`.
- Explicit `allowed_roots` replaces the `$HOME` default.
- To narrow an agent, set an explicit workspace root such as:

```toml
[policy]
allowed_roots = ["/root/git"]
```

The example above is a narrowing example, not the default.

## Auth reminders

Use:

```text
Authorization: Bearer <token>
```

for REST, polling, MCP, and GPT Actions.

`?token=` is allowed only for `/api/agents/ws` WebSocket handshake compatibility.

## systemd PATH reminder

systemd services do not read interactive shell startup files such as `~/.bashrc`. If commands need Rust/Cargo, Node, or Codex CLI, expose them through the agent `[shell].path_prepend` / `[shell].env` config or through the service manager's environment.

`runCodexTask` is optional and requires Codex CLI on the agent machine. It does not start a new `webcodex-agent`.
