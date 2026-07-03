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
- `webcodex-cli agent status --profile workstation` can read the local agent config.
- `webcodex-cli doctor --strict --profile workstation --server-url https://your-domain.example` passes.
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

### `listRuntimeTools` full response is too large

Full `listRuntimeTools` includes expanded schemas and metadata. For GPT Actions,
prefer `callRuntimeTool` with `tool="tool_manifest"` for daily discovery. For a
focused schema/debug view, call `listRuntimeTools` with `summary_only=true` plus
`category`, `features`, or `limit`.

### GPT Action still uses an old schema

Re-import the OpenAPI schema from the deployed `/openapi.json`, then check the
operation count. The current recommended count is 27 and the GPT Actions limit
is 30. If the count exceeds 30, do not deploy the schema as-is; artifact upload
tools should remain runtime-only behind `callRuntimeTool`, not promoted to new
dedicated Actions.

### MCP tool list looks stale

Reconnect or restart the MCP client so it runs a fresh `initialize` and
`tools/list`. If the server was just upgraded, verify public HTTPS reaches the
new service and check `journalctl -u webcodex` for startup or auth errors.

### Agent is offline

Run `runtime_status` or `listAgents`, then check the agent host:

```bash
systemctl status webcodex-agent
journalctl -u webcodex-agent
```

Confirm the agent server URL, token file, service user, and `allowed_roots`.

### Wrong token type

GPT Actions and MCP should use a managed `wc_pat_*` token or a
deployment-allowed shared key. `wc_agent_*` is only for `webcodex-agent`.
`WEBCODEX_TOKEN` is bootstrap/admin-oriented and should not be copied into GPT
Actions, MCP, or agent config.

### Non-git smoke workspace cannot run `git_status`

`git_status` requires a git repository for a clean deployment smoke result.
Initialize the disposable smoke project with git and an initial commit, or point
the smoke at another safe agent-backed git project.

### `operation_count` exceeds 30

The GPT Actions surface must stay at or below 30 operations. Keep runtime-only
tools, including chunked artifact upload tools, behind `callRuntimeTool` unless
there is an explicit product decision and operation budget for a dedicated
Action.

### `artifact_upload_chunk` says `path` is missing

`artifact_upload_chunk`, `artifact_upload_finish`, and `artifact_upload_abort`
must repeat the exact `path` used by `artifact_upload_begin`. This binds the
opaque `upload_id` to the requested target artifact path.

### `application/octet-stream` is rejected for an unsafe extension

Use a safe project-relative artifact path and a MIME type that matches the file
extension. For smoke tests, prefer a simple `.txt` path with `text/plain`. Avoid
secret-like paths, absolute paths, `.env*`, `.git`, token/credential paths, and
unsafe binary extensions.
