# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This guide is the first deployable setup path after the no-sudo local demo in the README. It covers both long-running service mode and no-service agent mode, but stays shorter than [DEPLOYMENT.md](DEPLOYMENT.md).

| Need | Use |
| --- | --- |
| One-machine local evaluation, no `sudo`, no HTTPS, no service | README Quick start |
| **Quickest server + client (shared key)** | **Quick start below** |
| **Temporary demo (anonymous open mode)** | **Quick start below** |
| First server and long-running agent with systemd services | Sections 1-3 below |
| Temporary agent, container, or machine without systemd | Section 4 below |
| Production hardening, Nginx, QUIC, GPT Actions, MCP details | [DEPLOYMENT.md](DEPLOYMENT.md) |

The command shapes below were checked against the current binary help output for `webcodex-cli`, `webcodex-agent`, and `webcodex`.

## Quick start: three paths

### A. Shared key (recommended for early evaluation)

Start the server. `server up` keeps anonymous access off by default and enables the shared-key quick-start path:

```bash
webcodex-cli server up --public-url <URL>
```

Connect an agent from a project directory with the same shared key you will give GPT Actions or MCP:

```bash
webcodex-cli connect <URL> --key <KEY> --root <PROJECT>
```

Configure GPT Action / MCP with that key when the host supports static bearer/API-key authentication:

```text
Authorization: Bearer <KEY>
```

In ChatGPT custom connectors, choose **Access token / API key** for this path, not OAuth. The server groups shared-key callers by `shared_key_hash`: the agent and GPT/MCP caller using the same key can see each other, while different keys cannot. A shared key is non-admin quick-start auth; it is not a managed user and should not be treated as production IAM.

### B. Open demo mode (anonymous, temporary only)

```bash
webcodex-cli server up --open --public-url <URL>
webcodex-cli connect <URL> --open --root <PROJECT>
webcodex-agent --config <generated-agent.toml>
```

`--open` must be explicit on the server (`WEBCODEX_ALLOW_ANONYMOUS=true`) and on the client (`connect --open`). The generated agent config uses `token = ""`; `webcodex-agent` treats that as no Authorization. For GPT Actions / MCP hosts, use this path only when the host has an explicit **None**, **No authentication**, or no-auth setting. Leaving OAuth client fields blank is not the same as no auth.

Open demo mode is only for localhost, trusted LANs, and temporary demos. Do not use it as a long-running public internet mode. Anonymous open callers share one demo current-session principal, so session state is shared within the open group.

### C. Managed production mode

Use pairing, `setup single-user`, `wc_pat_*` user tokens, and `wc_agent_*` agent tokens for production. Managed mode supports multi-user deployment, revocable tokens, per-user scopes, and audit-friendly ownership. The full managed flow is preserved in Sections 1-4 below.

## GPT Action / MCP host authentication compatibility

Hosts do not present authentication settings the same way.

| Host UI option | WebCodex mode | Notes |
| --- | --- | --- |
| Access token / API key, static bearer, or custom `Authorization` header | Shared-key quick start or managed PAT | Use the shared key for quick start, or `wc_pat_*` for managed mode. The token is sent as `Authorization: Bearer ...`. |
| None, No authentication, or unauthenticated access | Open demo mode | Requires the server to be started with `--open`; do not expose this as a long-running public mode. |
| OAuth | Managed OAuth | Use only when WebCodex is configured for the OAuth flow expected by that host. |

Do not select OAuth and leave the client fields blank expecting shared-key or open behavior. Blank OAuth client fields usually mean the host will try OAuth metadata discovery, dynamic client registration, or client metadata discovery. If the host is OAuth-only, shared-key quick start cannot be configured directly through that UI. A future OAuth bridge can provide a bearer-like user experience by letting the OAuth authorization page map a user-entered shared key to an OAuth access token, but that is still an OAuth flow, not a static bearer header.

## 0. Install binaries

On every machine that will run the server or agent:

```bash
npm install -g @yyjeqhc/webcodex

webcodex -h
webcodex-cli -h
webcodex-agent -h
```

Planned v0.2.0 GitHub release artifacts are `linux-x64`, `linux-arm64`, and `darwin-arm64`. Windows and `darwin-x64` artifacts are not planned for v0.2.0. The npm wrapper currently installs v0.1.0 binaries; download GitHub release binaries directly for v0.2.0.

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

This creates `/etc/webcodex/webcodex.env` and writes the bootstrap/admin `WEBCODEX_TOKEN`, `WEBCODEX_ADDR`, `WEBCODEX_DATA`, and `WEBCODEX_PUBLIC_URL`. It does not create `wc_pat_xxx` user tokens or `wc_agent_xxx` agent tokens.

For admin commands in the same shell, use `--env-file /etc/webcodex/webcodex.env` when available, or load the env file first:

```bash
set -a
. /etc/webcodex/webcodex.env
set +a
```

### 1.2 Put the server behind HTTPS

Configure your reverse proxy so the public URL reaches the local HTTP server:

```text
https://your-domain.example  ->  http://127.0.0.1:8080
```

GPT Actions and MCP require a public HTTPS URL. WebCodex CLI does not automate DNS, TLS, reverse proxy, or tunnel setup. A minimal Nginx configuration is included in [DEPLOYMENT.md](DEPLOYMENT.md#public-https-url).

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

If QUIC is not ready, keep `--transport auto` without a `[quic]` section; the agent starts on the WebSocket fallback and can later use QUIC when the `[quic]` section is added.

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
  --output-dir /etc/webcodex/clients/alice-laptop \
  --agent-config /etc/webcodex/clients/alice-laptop/agent.toml \
  --projects-dir /etc/webcodex/clients/alice-laptop/projects.d \
  --allowed-root /home/alice/git
```

`client enroll` receives `wc_pat_xxx` and `wc_agent_xxx` over HTTPS and writes them locally with restrictive file permissions. By default the profile is the `client_id`, so root enrollment writes under `/etc/webcodex/clients/alice-laptop/`; an explicit `--output-dir` still wins and should point at the intended profile directory.

If your server QUIC listener is enabled, add a `[quic]` section to `/etc/webcodex/clients/alice-laptop/agent.toml`:

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
sudo mkdir -p /etc/webcodex/clients/alice-laptop/projects.d
sudo tee /etc/webcodex/clients/alice-laptop/projects.d/my-repo.toml >/dev/null <<'EOF'
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

Add or adjust this in `/etc/webcodex/clients/alice-laptop/agent.toml`:

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
  --profile alice-laptop \
  --bin "$(command -v webcodex-agent)"

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent-alice-laptop

webcodex-cli agent status \
  --profile alice-laptop \
  --server-url https://your-domain.example

webcodex-cli doctor --strict \
  --profile alice-laptop \
  --server-url https://your-domain.example
```

Use `--overwrite` with `agent install-service` only when replacing an existing unit.

## 4. First client deployment: no service, foreground or background

This is useful for quick tests, containers, temporary clients, or hosts where you do not want systemd. Service mode is preferred for long-running production agents; no-service mode is easier to inspect and stop manually.

### 4.1 Enroll into a user config directory

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id alice-laptop \
  --display-name "Alice Laptop" \
  --transport auto \
  --output-dir "$HOME/.config/webcodex/clients/alice-laptop" \
  --agent-config "$HOME/.config/webcodex/clients/alice-laptop/agent.toml" \
  --projects-dir "$HOME/.config/webcodex/clients/alice-laptop/projects.d" \
  --allowed-root "$HOME/git"
```

Create a project file:

```bash
mkdir -p "$HOME/.config/webcodex/clients/alice-laptop/projects.d"
cat > "$HOME/.config/webcodex/clients/alice-laptop/projects.d/my-repo.toml" <<'EOF'
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true
shell_profile = "rust"
EOF
```

Edit the `path` to match the actual user and repository.

### 4.2 Add a shell profile to agent.toml

Add or adjust this in `$HOME/.config/webcodex/clients/alice-laptop/agent.toml`:

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

Use absolute paths in `agent.toml`; do not rely on `$HOME` expansion inside TOML strings. Project command environments should be configured in `[shell.profiles.*]`, not in your interactive `.bashrc`.

### 4.3 Start in the foreground for inspection

Foreground mode is the simplest no-service mode. It prints logs directly and exits when you press `Ctrl-C`:

```bash
webcodex-agent --profile alice-laptop
```

In another terminal, check status:

```bash
webcodex-cli agent status \
  --profile alice-laptop \
  --server-url https://your-domain.example
```

### 4.4 Or start in the background with nohup

Use this after the foreground run works and you want the agent to keep running after the terminal closes:

```bash
mkdir -p "$HOME/.local/state/webcodex"
nohup webcodex-agent --profile alice-laptop \
  >> "$HOME/.local/state/webcodex/agent.log" 2>&1 &

echo $! > "$HOME/.local/state/webcodex/agent.pid"
```

Check logs and status:

```bash
tail -f "$HOME/.local/state/webcodex/agent.log"

webcodex-cli agent status \
  --profile alice-laptop \
  --server-url https://your-domain.example
```

Stop the background agent:

```bash
kill "$(cat "$HOME/.local/state/webcodex/agent.pid")"
```

## 5. Test from the server/API side

After the agent is online, use a user PAT, not `WEBCODEX_TOKEN`, for runtime calls:

```bash
export WEBCODEX_PAT="$(cat /etc/webcodex/clients/alice-laptop/webcodex-user-token 2>/dev/null || cat "$HOME/.config/webcodex/clients/alice-laptop/webcodex-user-token")"

curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  https://your-domain.example/api/tools/list \
  -d '{}'
```

Then test through GPT Actions or MCP using the same `wc_pat_xxx` token.

## 6. Runtime notes

- For project-scoped current sessions, call `start_session` with `project`. A session created with `project = null` cannot later be bound to a specific project; `session_project_mismatch` is the expected audit result, not a runtime outage.
- If `runtime_status` says the server project config is not configured, it only means server-side `projects.toml` is absent. Agent-registered projects can still be available through connected agents; use `list_projects` to see the active runtime surface.

## 7. Which mode should you choose?

| Mode | Use when | Notes |
| --- | --- | --- |
| Server systemd service | Production server | Recommended. Keeps server running after reboot. |
| Agent systemd service | Long-running trusted client or server-side worker | Recommended for stable machines. Configure shell profiles because systemd does not read `.bashrc`. |
| Agent no-service foreground/background | Temporary client, container, smoke test, or machine without systemd | Start in the foreground first for logs; use `nohup` when you want it to continue after the terminal closes. |

## 8. Next docs

- Full deployment details: [DEPLOYMENT.md](DEPLOYMENT.md)
- Agent projects: [AGENT_PROJECTS.md](AGENT_PROJECTS.md)
- Shell profiles and PATH handling: [SHELL_PROFILES.md](SHELL_PROFILES.md)
- Transport details and QUIC validation: [AGENT_TRANSPORTS.md](AGENT_TRANSPORTS.md)
- Troubleshooting: [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
