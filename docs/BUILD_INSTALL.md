# Build and Install

WebCodex is a Rust project. A normal deployment needs the server binary
(`webcodex`) and at least one agent binary (`webcodex-agent`).

## Requirements

Install a Rust toolchain with `cargo`:

```bash
rustup install stable
rustup default stable
```

A Linux host with `git` available is recommended for the agent, because runtime
tools use git for status, diff, patch validation, and patch application.

## Build

From the repository root:

```bash
cargo build --release
```

The release binaries are:

```text
target/release/webcodex
target/release/webcodex-agent
```

## Run the server

```bash
WEBCODEX_TOKEN="change-me" \
WEBCODEX_ADDR="0.0.0.0:8080" \
WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
./target/release/webcodex
```

Expose the server over HTTPS before connecting ChatGPT GPT Actions or MCP Apps.
A reverse proxy such as nginx or Caddy is fine.

Useful server endpoints:

```text
GET  /openapi.json   GPT Actions schema
POST /mcp            MCP JSON-RPC endpoint
GET  /console        Read-only runtime console
```

## Configure an agent project

Create one project file per local repository:

```toml
# /etc/webcodex/projects.d/webcodex.toml
id = "webcodex"
name = "WebCodex"
path = "/root/git/webcodex"
allow_patch = true
kind = "rust"
```

Create the agent config:

```toml
# /etc/webcodex/agent.toml
server_url = "https://webcodex.example.com"
token = "change-me"
client_id = "workstation-1"
display_name = "Workstation"
owner = "you"
transport = "websocket"
projects_dir = "/etc/webcodex/projects.d"

[capabilities]
shell = true
file_read = true
file_write = true
git = true
jobs = true
async_jobs = true
async_shell_jobs = true

[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

Run the agent:

```bash
./target/release/webcodex-agent --config /etc/webcodex/agent.toml
```

Registered project ids use this form:

```text
agent:<client_id>:<project_id>
```

Example:

```text
agent:workstation-1:webcodex
```

## Agent shell PATH under systemd

A `webcodex-agent` service launched by systemd does not read interactive shell
startup files such as `~/.bashrc`. Tools installed through rustup, including
`cargo`, may therefore be absent from `PATH` even when they work in an SSH
session.

Prefer an explicit agent shell configuration:

```toml
[shell]
program = "/bin/bash"
args = ["-lc"]
path_prepend = ["/root/.cargo/bin"]
env = { CARGO_HOME = "/root/.cargo", RUSTUP_HOME = "/root/.rustup" }
# init_script = "/root/.config/webcodex/shell-env.sh"
```

The default remains `program = "sh"` and `args = ["-c"]` with no extra
environment or `PATH` changes. `init_script` is never used unless configured
explicitly. As an alternative, set `Environment=PATH=...` in the systemd unit.

## Connect ChatGPT

For GPT Actions, import:

```text
https://webcodex.example.com/openapi.json
```

Use HTTP API key authentication in the `Authorization` header:

```text
Bearer <wc_pat_user_api_token>
```

Use the server `WEBCODEX_TOKEN` only as a bootstrap/admin credential for setup.
Use personal API tokens for GPT Actions/MCP and agent tokens for
`webcodex-agent`.

For MCP / Apps, connect to:

```text
https://webcodex.example.com/mcp
```

The GPT Actions and MCP surfaces share the same `ToolRuntime`; they differ only
in the protocol used to reach the runtime tools.

## Verify

Local checks:

```bash
cargo fmt --check
cargo check
cargo check --tests
cargo test
```

Release readiness gate (runs the above plus both WebSocket and polling E2E
transports, and a static check that no sensitive files are tracked/staged):

```bash
bash scripts/release_check.sh
```

Deployment smoke, when a public endpoint and token are available:

```bash
WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
WEBCODEX_TOKEN="change-me" \
bash scripts/smoke_deployment.sh
```

## GPT Actions import checklist

Run through this short checklist before importing `/openapi.json` into ChatGPT
GPT Actions:

- [ ] Public HTTPS URL is reachable (e.g. `https://webcodex.example.com`).
- [ ] `GET /openapi.json` returns a valid schema.
- [ ] Schema exposes 25 operations (`scripts/e2e_zero_config_ws.sh` asserts
      this against the live schema).
- [ ] Every operation is POST-only (asserted by the E2E schema check).
- [ ] `WEBCODEX_TOKEN` is set on the server as the bootstrap/admin credential;
      GPT Action auth is configured as an HTTP API key in the `Authorization`
      header with a Phase 2 personal API token.
- [ ] At least one agent is `online` (`POST /api/runtime/status`).
- [ ] `POST /api/projects/list` shows `agent:<client_id>:<project_id>`.
- [ ] Local full-auto loop E2E passes:
      `bash scripts/e2e_zero_config_ws.sh`
      (and `E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh`).
