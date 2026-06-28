# Troubleshooting

[English](TROUBLESHOOTING.md) | [简体中文](TROUBLESHOOTING.zh-CN.md)

Practical checks for common WebCodex deployment issues. Do not paste or share real tokens, env files, `Authorization` headers, or complete `agent.toml` files while debugging.

## Operational checklist

Server:

- `webcodex --version` prints a version.
- `webcodex-cli server status --env-file /etc/webcodex/webcodex.env` reports the local server reachable.
- `curl http://127.0.0.1:8080/openapi.json` returns OpenAPI JSON on the server host.
- Public HTTPS is reachable through nginx or your chosen reverse proxy, if used.

Client:

- `webcodex-agent --version` prints a version.
- `webcodex-cli agent status --config /etc/webcodex/agent.toml` can read the local agent config.
- `webcodex-cli doctor --strict --server-url https://your-domain.example --user-token-file /etc/webcodex/webcodex-user-token --agent-token-file /etc/webcodex/webcodex-agent-token` passes.
- `listAgents` / `runtime_status` shows the agent online.

## Common issues

### `webcodex-cli server install-service` says the service already exists

Use `--overwrite` only when you intentionally want to replace the existing unit:

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex \
  --overwrite
sudo systemctl daemon-reload
```

Then restart or start the service according to your normal deployment process.

### `server status` says `HTTP reachable: no`

Check the local service first, then the reverse proxy:

```bash
systemctl status webcodex
journalctl -u webcodex
curl http://127.0.0.1:8080/openapi.json
```

If local HTTP works but public HTTPS does not, check the nginx upstream host/port and TLS configuration. WebCodex CLI does not automate reverse proxy setup.

### Client says `webcodex-cli: command not found`

Install or symlink the CLI onto the client's `PATH`, for example:

```bash
sudo ln -s /opt/webcodex/bin/webcodex-cli /usr/local/bin/webcodex-cli
```

Use the actual install path for your host.

### Client accidentally runs `pairing create` and `/etc/webcodex/webcodex.env` is missing

`webcodex-cli pairing create` is server/admin-side and uses the server bootstrap env file. A friend/client machine should run `webcodex-cli client enroll` with the short-lived `wc_pair_*` code from the server owner.

Copy only the `wc_pair_*` code between machines. Do not copy `WEBCODEX_TOKEN`, user API tokens, agent tokens, env files, or complete `agent.toml` files.

### Doctor warns `binary webcodex not found in PATH` on a client

That can be acceptable on agent-only client machines. Agent-only clients need `webcodex-agent` and `webcodex-cli`; the server binary `webcodex` is only required on server hosts.

### `client online: no`

Check the agent service and its connection details:

```bash
systemctl status webcodex-agent
journalctl -u webcodex-agent
```

Also verify the server URL, local token files, and agent `allowed_roots`. Missing or empty `allowed_roots` defaults to `$HOME`; explicit `allowed_roots` replaces that default.
