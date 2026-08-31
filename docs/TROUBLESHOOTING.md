# Troubleshooting

[English](TROUBLESHOOTING.md) | [简体中文](TROUBLESHOOTING.zh-CN.md)

Practical checks for common WebCodex deployment issues. Do not paste or share real tokens, env files, `Authorization` headers, or complete `agent.toml` files while debugging.

## Operational checklist

Server:

- `webcodex --version` prints a version.
- `webcodex server status --env-file /etc/webcodex/webcodex.env` reports the local server reachable.
- `curl http://127.0.0.1:8080/openapi.json` returns OpenAPI JSON on the server host.
- Public HTTPS is reachable through nginx or your chosen reverse proxy, if used.

Client:

- `webcodex-runner --version` prints a version.
- For hosted quick-start, `webcodex runner status --profile <profile-from-connect>`
  reports `runner mode: hosted local process`, `runner active: true`, and
  `client online: yes`.
- `webcodex runner status --profile workstation` can read the local Runner config (`agent.toml`).
- `webcodex doctor` passes for a canonical project, or advanced
  `webcodex ops status --strict --server-url https://your-domain.example`
  passes for a managed deployment.
- `listAgents` / `runtime_status` shows the agent online.

## Common issues

### `webcodex connect` cannot finish

`connect` waits up to 15 seconds for the complete path: Server reachable,
Runner visible, and target project visible to the same key. Its error includes
the profile Runner log path. Check:

```bash
webcodex runner status --profile <profile-from-connect>
webcodex runner logs --profile <profile-from-connect> --lines 100
```

Confirm that the Server URL points to the root origin, the Server enables
shared-key mode, and MCP and Runner use exactly the same trimmed key. A
different key intentionally sees no Runner or project. If this invocation
started a Runner that could not register, `connect` stops it; the local config
is retained so the command can be retried.

The hosted log is
`$XDG_STATE_HOME/webcodex/clients/<profile>/runner.log` (or the equivalent
default under `~/.local/state`). It rotates while the Runner is alive and keeps
only the current file plus `.1` and `.2`, at approximately 10 MiB each.
`runner logs --lines` reads only bounded file tails and can span those archives;
`--follow` follows the new current filename after rotation. Do not search or
edit internal Runner state to repair registration.

By default one shared-key group can register 16 Runners and the Server process
can hold 1,024 shared-key Runners in total. An offline shared-key Runner record
is pruned after 24 hours. Managed Agent Tokens are not charged against those
shared-key count or retention limits. All Runner registrations have a
64-project input safety limit.

### `connect` rejects a `wc_*` value

This is deliberate. `wc_pat_*`, `wc_agent_*`, `wc_acct_*`, and other `wc_*`
values are managed credentials and never fall back to shared-key auth. Use a
different random key for the hosted shared-key flow, or use `webcodex login`
for managed identity.

`webcodex token generate` is offline material generation only. It does not
register the generated credential with a remote Server, so do not use its
output as a hosted shared key.

### A hosted Runner stopped or its PID is stale

Re-run the same `webcodex connect` command. The profile lock prevents duplicate
starts; a live Runner with the same config is reused, while stale or
non-Runner PID state is discarded before one replacement Runner starts. To
stop it explicitly:

```bash
webcodex runner stop --profile <profile-from-connect>
```

The key is stored only in the protected profile config and is not printed by
status or written to the project checkout.

### `webcodex server install` says the service already exists

Use `--overwrite` only when you intentionally want to replace the existing unit:

```bash
sudo webcodex server install \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex-server \
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

### Trace one tool invocation end to end

For a self-hosted forensic capture, set `WEBCODEX_TOOL_REQUEST_TRACE=full` on
the Server and reproduce the call once. Find its `server_trace_id` in the Server
journal, then inspect:

```bash
TRACE_ROOT=/var/lib/webcodex/tool-request-traces
find "$TRACE_ROOT/<server_trace_id>" -maxdepth 2 -type f -print
cat "$TRACE_ROOT/<server_trace_id>/events.jsonl"
zstd -dc "$TRACE_ROOT/<server_trace_id>/payloads/<payload>.json.zst" | jq .
```

For `tools/call`, `raw_request_body` (API) or `raw_arguments` (MCP) shows what
reached the Server before WebCodex wrapper/session normalization;
`effective_arguments` shows the exact semantic argument object passed into the
runtime/connector dispatch. This is the first place to compare an optional field
that a client claims it sent, such as `ack_session_context_revision`.
`final_response` captures the bounded JSON response generated by the Server.

If the call reached a Runner, the trace `events.jsonl` contains
`runner_request_id`, Runner client/instance, transport, and advertised build
version/commit. The `runner_request` payload is the exact typed request the
Server enqueued for that Runner, so compare it with `effective_arguments` when a
field appears to vanish during dispatch construction. Search the Runner journal
for that `runner_request_id` when the Runner also has tool-request tracing
enabled. Subsequent raw Runner results and Job updates are captured back into the
original Server trace through the bounded correlation index.

The trace code does not read the WebCodex ingress HTTP `Authorization` header.
However, `full` is intentionally raw: a credential-like value placed inside a
tool argument, script/stdin, Runner request, or Runner response will be captured
as payload data.

No payload file means neither "empty" nor "truncated". Check the Server journal
for `tool_trace_capture_omitted` with `trace_writer_queue_full` or
`trace_disk_budget_exceeded`, or for `tool_trace_capture_failed`. Full-mode writes
use a bounded background queue, so saturation intentionally drops diagnostic
records instead of slowing the tool request. Those trace failures are diagnostic only; they do
not make the underlying tool fail. If WebCodex logged `tool_handler_returned`
but the client saw no reply, correlate the same request time with the reverse
proxy access log because handler return is not proof of client delivery.

### Client says `webcodex: command not found`

Install or symlink the CLI onto the client's `PATH`, for example:

```bash
sudo ln -s /opt/webcodex/bin/webcodex /usr/local/bin/webcodex
```

Use the actual install path for your host.

### Client accidentally runs `pairing create` and `/etc/webcodex/webcodex.env` is missing

`webcodex pairing create` is server/admin-side and uses the server bootstrap env file. A friend/client machine should run `webcodex login <server-url> --code <wc_pair_...>` (advanced: `webcodex client enroll`) with the short-lived `wc_pair_*` code from the server owner.

Copy only the `wc_pair_*` code between machines. Do not copy `WEBCODEX_TOKEN`, user API tokens, agent tokens, env files, or complete `agent.toml` files.

### Doctor warns `binary webcodex not found in PATH` on a client

That can be acceptable on agent-only client machines. Agent-only clients need the public `webcodex` CLI and `webcodex-runner`; `webcodex-server` is only required on server hosts.

### `client online: no`

For a hosted `connect` profile, use the profile-specific status and log path
shown above. For a systemd-managed deployment, check the agent service and its
connection details:

Use the same scope that installed the service:

```bash
# Ordinary user service
webcodex runner status --scope user
webcodex runner logs --scope user --lines 100

# Administrator-managed system service
sudo webcodex runner status --scope system
sudo webcodex runner logs --scope system --lines 100
```

Also verify the server URL, local token files, and agent `allowed_roots`. Missing or empty `allowed_roots` defaults to `$HOME`; explicit `allowed_roots` replaces that default.

### `listRuntimeTools` full response is too large

Full `listRuntimeTools` includes expanded schemas and metadata. For GPT Actions,
prefer `callRuntimeTool` with `tool="tool_manifest"` for daily discovery. For a
focused schema/debug view, call `listRuntimeTools` with `summary_only=true` plus
`category`, `features`, or `limit`.

### GPT Action still uses an old schema

Re-import the OpenAPI schema from the deployed `/openapi.json`, then check the
operation count. The current recommended count is 25 and the GPT Actions limit
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
webcodex runner status --scope user
webcodex runner logs --scope user --lines 100
# Use `sudo ... --scope system` for an administrator-managed system service.
```

Confirm the agent server URL, token file, service user, and `allowed_roots`.

### Wrong token type

In the hosted quick-start, MCP and Runner use the same non-`wc_` shared key.
In managed mode, GPT Actions, MCP, and ordinary REST/project APIs use
`webcodex-user-token` (`wc_pat_*`), while the Runner token (`wc_agent_*`) is
only for Runner transport — after `webcodex login` it lives inline in
`agent.toml`, with no separate `webcodex-runner-token` file (the advanced
`webcodex client enroll` flow writes one). A 403 after putting a `wc_agent_*`
value in `--token` or `--token-file` is the expected security boundary: select
the generated `webcodex-user-token` instead. Recent CLI commands also diagnose
this mismatch without printing the complete token. `WEBCODEX_TOKEN` is
bootstrap/admin-oriented and should not be copied into GPT Actions, MCP, or
agent config.

### Runner service is visible in one command but missing in another

Pass the same `--scope` to install, status, start, stop, restart, logs, and
uninstall. User scope invokes `systemctl --user` and `journalctl --user` and
uses `$XDG_CONFIG_HOME/systemd/user` (or `$HOME/.config/systemd/user`). System
scope invokes the system manager and uses `/etc/systemd/system`.

Non-root callers default to user scope. Root callers default to system scope,
but installation still requires a non-root `--user`; an intentional root
Runner additionally requires `--allow-root-runner` and is discouraged. If a
custom `--service-file` was used during install, pass that same absolute path
and scope to later commands. WebCodex does not silently migrate or overwrite a
unit in the other scope.

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
