# Build and Install

Private Drop is a Rust project. A normal deployment needs the server binary
(`private-drop`) and at least one agent binary (`private-drop-agent`).

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
target/release/private-drop
target/release/private-drop-agent
```

## Run the server

```bash
DROP_TOKEN="change-me" \
DROP_ADDR="0.0.0.0:8080" \
DROP_PUBLIC_URL="https://drop.example.com" \
./target/release/private-drop
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
# /etc/private-drop-agent/projects.d/private-drop.toml
id = "private-drop"
name = "Private Drop"
path = "/root/git/private-drop"
allow_patch = true
kind = "rust"
```

Create the agent config:

```toml
# /etc/private-drop-agent/agent.toml
server_url = "https://drop.example.com"
token = "change-me"
client_id = "workstation-1"
display_name = "Workstation"
owner = "you"
transport = "websocket"
projects_dir = "/etc/private-drop-agent/projects.d"

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
./target/release/private-drop-agent --config /etc/private-drop-agent/agent.toml
```

Registered project ids use this form:

```text
agent:<client_id>:<project_id>
```

Example:

```text
agent:workstation-1:private-drop
```

## Connect ChatGPT

For GPT Actions, import:

```text
https://drop.example.com/openapi.json
```

Use HTTP API key authentication in the `Authorization` header:

```text
Bearer <DROP_TOKEN>
```

For MCP / Apps, connect to:

```text
https://drop.example.com/mcp
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
DROP_PUBLIC_URL="https://drop.example.com" \
DROP_TOKEN="change-me" \
bash scripts/smoke_deployment.sh
```

## GPT Actions import checklist

Run through this short checklist before importing `/openapi.json` into ChatGPT
GPT Actions:

- [ ] Public HTTPS URL is reachable (e.g. `https://drop.example.com`).
- [ ] `GET /openapi.json` returns a valid schema.
- [ ] Schema exposes 25 operations (`scripts/e2e_zero_config_ws.sh` asserts
      this against the live schema).
- [ ] Every operation is POST-only (asserted by the E2E schema check).
- [ ] `DROP_TOKEN` is set on the server; GPT Action auth is configured as an
      HTTP API key in the `Authorization` header with value `Bearer <DROP_TOKEN>`.
- [ ] At least one agent is `online` (`POST /api/runtime/status`).
- [ ] `POST /api/projects/list` shows `agent:<client_id>:<project_id>`.
- [ ] Local full-auto loop E2E passes:
      `bash scripts/e2e_zero_config_ws.sh`
      (and `E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh`).
