# Build and Install Quick Reference

[English](BUILD_INSTALL.md) | [简体中文](BUILD_INSTALL.zh-CN.md)

This is the short install path. See [DEPLOYMENT.md](DEPLOYMENT.md) for production details.

## Build binaries

Build the three current binaries for your host:

```text
webcodex
webcodex-agent
webcodex-cli
```

Do not run unauthenticated production deployments.

## Help-verified command shape

The examples in this guide were checked against the current binary help output from `webcodex-cli -h`, `webcodex-agent -h`, and `webcodex -h`. Keep these flag differences in mind:

| Task | Preferred command shape |
| --- | --- |
| Server env bootstrap | `webcodex-cli server init --listen ... --data-dir ... --env-file ...` |
| Server systemd unit | `webcodex-cli server install-service --env-file ... --bin ...` |
| Server status | `webcodex-cli server status --env-file ...` |
| Admin-created account credential | `webcodex-cli users create --server-url ... --token ... --username ... --issue-credential` |
| User-created PAT | `webcodex-cli token create-local --server ... --user ... --credential ... --scopes ...` |
| User-created agent token | `webcodex-cli agent-token create-local --server ... --user ... --credential ... --client-id ...` |
| Pairing code | `webcodex-cli pairing create --server-url ... --username ... --client-id ...` |
| Client enrollment | `webcodex-cli client enroll --server-url ... --pairing-code ... --client-id ...` |
| Agent foreground run | `webcodex-agent --profile ...` |
| Agent service | `webcodex-cli agent install-service --profile ... --bin ...` |
| Doctor | `webcodex-cli doctor --server-url ... --user-token-file ... --strict` |

The account-management command uses `users create` and `--server-url`; local token creation commands use `--server`. That difference comes from the current CLI surface and is intentionally reflected in the examples.

## Install packages

The documented distribution path uses the npm thin installer/wrapper:

```bash
npm install -g @yyjeqhc/webcodex
```
Supported v0.1.0 release artifacts currently include `linux-x64`, `linux-arm64`, and `darwin-arm64`. `darwin-x64`, Windows, and other targets are not included in v0.1.0 unless a later release adds artifacts.


The npm package is a thin wrapper around native release artifacts. During install it downloads the matching GitHub Release artifact and verifies the SHA-256 checksum from the manifest.

## Example files

The `deploy/` directory contains short examples you can adapt:

- `deploy/webcodex.env.example`
- `deploy/webcodex.service.example`
- `deploy/webcodex-agent.toml.example`
- `deploy/webcodex-agent.service.example`
- `deploy/nginx.webcodex.example.conf`

The nginx file is only an example. WebCodex CLI does not automate reverse proxy setup.

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
  --username friendname \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` is a server/admin-side command. It needs server bootstrap/admin auth. Copy only the short-lived `wc_pair_*` code to the client; do not copy `WEBCODEX_TOKEN`, `wc_pat_*`, `wc_agent_*`, complete env files, or complete `agent.toml` files. Each friend should use a unique `username` and `client_id`.

Client:

6. Install `webcodex-agent` and `webcodex-cli` binaries.
7. Exchange the pairing code over HTTPS and write client-side credentials/config:

```bash
sudo webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id friend-laptop \
  --profile special \
  --allowed-root /home/friend/git
```

Client enroll creates the `wc_pat_*` user token, `wc_agent_*` agent token, and `/etc/webcodex/clients/special/agent.toml` locally with `0600` permissions on Unix. `/etc/webcodex/webcodex.env` is server-side only; isolate client-side token/config files under `/etc/webcodex/clients/<profile>/` when multiple users or clients share one machine.

8. Install and start the agent service, then validate:

```bash
sudo webcodex-cli agent install-service \
  --profile special \
  --bin /opt/webcodex/bin/webcodex-agent \
  --overwrite
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent-special
webcodex-cli agent status \
  --profile special \
  --server-url https://your-domain.example
webcodex-cli doctor --strict \
  --profile special \
  --server-url https://your-domain.example
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
webcodex-agent --profile special
```

`webcodex-agent init` remains available as a compatibility entry point.

## Doctor

Run non-destructive diagnostics:

```bash
webcodex-cli doctor --strict \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/clients/special/webcodex-user-token \
  --agent-token-file /etc/webcodex/clients/special/webcodex-agent-token
```

Add `--agent-config /etc/webcodex/clients/special/agent.toml` to run local shell-profile / project
diagnostics (parses `agent.toml`, checks `projects_dir`, project paths, and
`shell_profile` resolution) without contacting the server. Add `--project <id>`
to also run a remote `printf webcodex-doctor-ok` shell roundtrip against a
specific project:

```bash
webcodex-cli doctor --strict \
  --agent-config /etc/webcodex/clients/special/agent.toml \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/clients/special/webcodex-user-token \
  --project agent:oe:webcodex
```

Doctor never prints `init_script` bodies, env values, or tokens. See
[SHELL_PROFILES.md](SHELL_PROFILES.md) for profile config and troubleshooting.

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

systemd services do not read interactive shell startup files such as `~/.bashrc`. If commands need Rust/Cargo, Node, or Codex CLI, expose them through configured agent shell profiles or through the service manager's environment.

`runCodexTask` is optional and requires Codex CLI on the agent machine. It does not start a new `webcodex-agent`.
