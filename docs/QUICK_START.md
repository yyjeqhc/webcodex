# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This guide is the shortest deployable path for a first WebCodex server and a first client agent. It is intentionally more operational than the README and less exhaustive than [DEPLOYMENT.md](DEPLOYMENT.md).

The command shapes below were checked against the current binary help output for `webcodex-cli`, `webcodex-agent`, and `webcodex`.

## 0. Install binaries

On every machine that will run the server or agent:

```bash
npm install -g @yyjeqhc/webcodex

webcodex -h
webcodex-cli -h
webcodex-agent -h
```

v0.1.0 release artifacts currently include `linux-x64`, `linux-arm64`, and `darwin-arm64`. Windows and `darwin-x64` artifacts are not included in v0.1.0.

## 1. First server deployment

Run this on the server host.

### 1.1 Initialize server env

```bash
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env \
  --public-url https://your-domain.example
```

This writes only the bootstrap/admin `WEBCODEX_TOKEN`. It does not create `wc_pat_xxx` user tokens or `wc_agent_xxx` agent tokens.

### 1.2 Put the server behind HTTPS

Configure your reverse proxy so the public URL reaches the local HTTP server:

```text
https://your-domain.example  ->  http://127.0.0.1:8080
```

GPT Actions and MCP require a public HTTPS URL. WebCodex CLI does not automate DNS, TLS, reverse proxy, or tunnel setup.

### 1.3 Install and start the server service

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin "$(command -v webcodex)"

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex

webcodex-cli server status \
  --env-file /etc/webcodex/webcodex.env \
  --url http://127.0.0.1:8080
```

Use `--overwrite` with `server install-service` only when replacing an existing unit.

### 1.4 Optional: enable QUIC for agents

QUIC is only for `webcodex-agent` connectivity. GPT Actions and MCP still use HTTPS.

Add these values to `/etc/webcodex/webcodex.env` when you want QUIC enabled:

```bash
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/etc/letsencrypt/live/your-domain.example/fullchain.pem
WEBCODEX_QUIC_KEY=/etc/letsencrypt/live/your-domain.example/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-agent/1
```

Then restart the server and open UDP 8443 from agent hosts:

```bash
sudo systemctl restart webcodex
webcodex-cli doctor --quic --server-only \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --strict
```

If QUIC is not ready, use WebSocket first. Agents can still use `--transport websocket` or `transport = "websocket"`.

## 2. Invite or enroll a first client

The easiest first-client flow is pairing. Run this on the server/admin side:

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username alice \
  --client-id alice-laptop \
  --display-name "Alice" \
  --ttl-secs 600
```

Copy only the short-lived `wc_pair_xxx` code to the client. Do not copy `WEBCODEX_TOKEN`, `wc_pat_xxx`, `wc_agent_xxx`, `/etc/webcodex/webcodex.env`, or a complete `agent.toml` to another machine.

## 3. First client deployment: system service mode

Run this on the client/agent machine.

### 3.1 Enroll the client

```bash
sudo webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id alice-laptop \
  --display-name "Alice Laptop" \
  --transport auto \
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml \
  --projects-dir /etc/webcodex/projects.d \
  --allowed-root /home/alice/git
```

`client enroll` receives `wc_pat_xxx` and `wc_agent_xxx` over HTTPS and writes them locally with restrictive file permissions.

If your server QUIC listener is enabled, add a `[quic]` section to `/etc/webcodex/agent.toml`:

```toml
transport = "auto"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-agent/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

With `transport = "auto"`, the agent tries QUIC first when `[quic]` is configured, then WebSocket, then polling.

### 3.2 Register a project

Create a project registry file under the configured `projects_dir`:

```bash
sudo mkdir -p /etc/webcodex/projects.d
sudo tee /etc/webcodex/projects.d/my-repo.toml >/dev/null <<'EOF'
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true
shell_profile = "rust"

[hooks]
status = ["git status --short"]
check = ["cargo check --all-targets"]
EOF
```

Runtime project ids use this form:

```text
agent:<client_id>:<project_id>
```

For the example above: `agent:alice-laptop:my-repo`.

### 3.3 Configure project command environment

systemd services do not read interactive shell files such as `~/.bashrc`. Configure project command environment through agent shell profiles instead of relying on login-shell state.

Add or adjust this in `/etc/webcodex/agent.toml`:

```toml
[shell]
default_profile = "rust"

[shell.profiles.rust]
description = "Rust development tools"
program = "bash"
args = ["-lc"]

[shell.profiles.rust.env]
PATH = "/home/alice/.cargo/bin:/home/alice/.local/bin:/usr/local/bin:/usr/bin:/bin"
CARGO_HOME = "/home/alice/.cargo"
RUSTUP_HOME = "/home/alice/.rustup"
```

If you use Python, Conda, Node, or Codex CLI, put their required `PATH` entries and environment variables in the selected shell profile. Restart the agent after changing `agent.toml`.

### 3.4 Install and start the agent service

```bash
sudo webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin "$(command -v webcodex-agent)"

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
  --agent-token-file /etc/webcodex/webcodex-agent-token \
  --agent-config /etc/webcodex/agent.toml
```

Use `--overwrite` with `agent install-service` only when replacing an existing unit.

## 4. First client deployment: no service, background process

This is useful for quick tests, containers, temporary clients, or hosts where you do not want systemd. Service mode is preferred for long-running production agents.

### 4.1 Enroll into a user config directory

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id alice-laptop \
  --display-name "Alice Laptop" \
  --transport auto \
  --output-dir "$HOME/.config/webcodex" \
  --agent-config "$HOME/.config/webcodex/agent.toml" \
  --projects-dir "$HOME/.config/webcodex/projects.d" \
  --allowed-root "$HOME/git"
```

Create a project file:

```bash
mkdir -p "$HOME/.config/webcodex/projects.d"
cat > "$HOME/.config/webcodex/projects.d/my-repo.toml" <<'EOF'
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true
shell_profile = "rust"
EOF
```

Edit the `path` to match the actual user and repository.

### 4.2 Configure environment for background startup

Create a small environment file for the process launch:

```bash
mkdir -p "$HOME/.config/webcodex" "$HOME/.local/state/webcodex"
cat > "$HOME/.config/webcodex/agent.env" <<'EOF'
WEBCODEX_AGENT_CONFIG=$HOME/.config/webcodex/agent.toml
PATH=$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin
EOF
chmod 600 "$HOME/.config/webcodex/agent.env"
```

This environment controls how the background `webcodex-agent` process is launched. Project command environments should still be configured in `[shell.profiles.*]` inside `agent.toml`.

### 4.3 Add a shell profile to agent.toml

Add or adjust this in `$HOME/.config/webcodex/agent.toml`:

```toml
[shell]
default_profile = "rust"

[shell.profiles.rust]
description = "Rust development tools"
program = "bash"
args = ["-lc"]

[shell.profiles.rust.env]
PATH = "/home/alice/.cargo/bin:/home/alice/.local/bin:/usr/local/bin:/usr/bin:/bin"
CARGO_HOME = "/home/alice/.cargo"
RUSTUP_HOME = "/home/alice/.rustup"
```

Use absolute paths in `agent.toml`; do not rely on `$HOME` expansion inside TOML strings.

### 4.4 Start in the background

```bash
set -a
. "$HOME/.config/webcodex/agent.env"
set +a

nohup webcodex-agent --config "$HOME/.config/webcodex/agent.toml" \
  >> "$HOME/.local/state/webcodex/agent.log" 2>&1 &

echo $! > "$HOME/.local/state/webcodex/agent.pid"
```

Check logs and status:

```bash
tail -f "$HOME/.local/state/webcodex/agent.log"

webcodex-cli agent status \
  --config "$HOME/.config/webcodex/agent.toml" \
  --server-url https://your-domain.example \
  --user-token-file "$HOME/.config/webcodex/webcodex-user-token" \
  --agent-token-file "$HOME/.config/webcodex/webcodex-agent-token"
```

Stop the background agent:

```bash
kill "$(cat "$HOME/.local/state/webcodex/agent.pid")"
```

## 5. Test from the server/API side

After the agent is online, use a user PAT, not `WEBCODEX_TOKEN`, for runtime calls:

```bash
export WEBCODEX_PAT="$(cat /etc/webcodex/webcodex-user-token 2>/dev/null || cat "$HOME/.config/webcodex/webcodex-user-token")"

curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  https://your-domain.example/api/tools/list \
  -d '{}'
```

Then test through GPT Actions or MCP using the same `wc_pat_xxx` token.

## 6. Which mode should you choose?

| Mode | Use when | Notes |
| --- | --- | --- |
| Server systemd service | Production server | Recommended. Keeps server running after reboot. |
| Agent systemd service | Long-running trusted client or server-side worker | Recommended for stable machines. Configure shell profiles because systemd does not read `.bashrc`. |
| Agent no-service background process | Temporary client, container, smoke test, or machine without systemd | Start with `nohup`; keep a log and pid file; restart manually after reboot. |

## 7. Next docs

- Full deployment details: [DEPLOYMENT.md](DEPLOYMENT.md)
- Agent projects: [AGENT_PROJECTS.md](AGENT_PROJECTS.md)
- Shell profiles and PATH handling: [SHELL_PROFILES.md](SHELL_PROFILES.md)
- Transport details and QUIC validation: [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md)
- Troubleshooting: [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
