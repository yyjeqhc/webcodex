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

The documented distribution path assumes the MVP npm wrapper:

```bash
npm install -g @webcodex/webcodex
```

The npm package is a wrapper around native release artifacts. Publishing and real artifact URLs/checksums are a separate release step.

## Binary deployment flow

Server:

1. Install `webcodex` and `webcodex-cli` binaries.
2. Initialize the server env file:

```bash
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
```

This creates only the server bootstrap/admin `WEBCODEX_TOKEN` in `/etc/webcodex/webcodex.env`. That file is server-side only; it does not create user API tokens or agent tokens.

3. Install the server service. Use `--overwrite` only when replacing an old unit.

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex
```

4. Reload systemd, start the service, and check status:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex-cli server status --env-file /etc/webcodex/webcodex.env
```

Server/admin:

5. Create a temporary one-time pairing code:

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username alice \
  --client-id alice-laptop
```

`pairing create` is a server/admin-side command. It needs server bootstrap/admin auth. Copy only the short-lived `wc_pair_*` code to the client; do not copy `WEBCODEX_TOKEN`, `wc_pat_*`, or `wc_agent_*` values.

Client:

6. Install `webcodex-agent` and `webcodex-cli` binaries.
7. Exchange the pairing code over HTTPS and write client-side credentials/config:

```bash
sudo webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <temporary_pairing_code> \
  --client-id alice-laptop \
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml
```

Client enroll creates the `wc_pat_*` user token, `wc_agent_*` agent token, and `/etc/webcodex/agent.toml` locally with `0600` permissions on Unix.

8. Install and start the agent service, then validate:

```bash
sudo webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin /opt/webcodex/bin/webcodex-agent
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent
webcodex-cli agent status \
  --config /etc/webcodex/agent.toml \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token
webcodex-cli doctor --strict \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token
```

GPT Actions should use the generated client-side user-token file. GPT Actions require a public HTTPS URL; WebCodex CLI does not automate reverse proxies or tunnels.

Compatibility commands still work, but should not be the first choice in new docs:

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
webcodex-cli setup single-user
```

## Agent config

Client enroll writes `agent.toml`. For a systemd service, use `webcodex-cli agent install-service`; for a foreground test, run:

```bash
webcodex-agent --config ~/.config/webcodex/agent.toml
```

`webcodex-agent init` remains available as a compatibility entry point.

## Doctor

Run non-destructive diagnostics:

```bash
webcodex-cli doctor --strict \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token
```

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
