# Troubleshooting

[English](TROUBLESHOOTING.md) | [简体中文](TROUBLESHOOTING.zh-CN.md)

Practical checks for common WebCodex deployment issues. Do not paste or share real tokens, env files, `Authorization` headers, or complete `runner.toml` files while debugging.

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
- `webcodex runner status --profile workstation` can read the local Runner config (`runner.toml`).
- `webcodex doctor` passes for a canonical project, or advanced
  `webcodex ops status --strict --server-url https://your-domain.example`
  passes for a managed deployment.
- `list_runners` / `runtime_status` shows the Runner online.

## Identify the failing layer first

When ChatGPT says an app, plugin, or tool is blocked, do not start by restarting the
Runner. First determine whether the request reached WebCodex at all.

| Signal | Likely layer | Next check |
| --- | --- | --- |
| ChatGPT reports `FORBIDDEN: This conversation does not support developer MCPs` or says the current conversation disabled the developer MCP server | ChatGPT Host / conversation MCP admission, when no matching request reaches WebCodex | Verify WebCodex independently from the operator/Runner host; then test the Host connection separately |
| WebCodex returns HTTP 401/403, an MCP authentication error, or a normal structured ToolResult failure | Server authentication / authorization / ToolRuntime | Check the user/API credential, OAuth scopes, Server logs, and the exact WebCodex error |
| `runtime_status` succeeds but shows the Runner offline or the project missing | Runner / project registration | Use `webcodex runner status` and bounded Runner logs on the Runner host |
| `plugin_tool` reaches WebCodex and returns `ready=false`, `plugin_check_busy`, `plugin_reload_busy`, or another Plugin diagnostic | WebCodex Native Tool Plugin runtime | Use `webcodex plugin check/list/describe/reload` and the [Native Tool Plugin guide](PLUGINS.md) |

The first row is important: if the ChatGPT Host refuses to dispatch
`runtime_status`, the displayed `FORBIDDEN` text is **not** a WebCodex
`runtime_status` result. Restarting or reconfiguring the Runner cannot repair a
request that never reached the Server.

### ChatGPT says developer MCP is disabled or unsupported

Reports in [Issue #500](https://github.com/yyjeqhc/webcodex/issues/500) include a
conversation that had already used WebCodex successfully, then repeatedly received:

```text
FORBIDDEN: This conversation does not support developer MCPs
```

The same Server, Runner, project, and local workspace remained usable through
independent paths, and the same ChatGPT conversation later recovered without a
WebCodex configuration change. This is consistent with a Host/conversation-level
developer-MCP routing or permission state, not with a durable Runner failure.

Use this order:

1. Record the exact error text, timestamp, timezone, and ChatGPT surface
   (web/desktop/mobile if relevant).
2. Verify WebCodex independently of that conversation. For a hosted profile:

   ```bash
   webcodex --version
   webcodex-runner --version
   webcodex runner status --profile <profile-from-connect>
   webcodex runner logs --profile <profile-from-connect> --lines 100
   ```

   For a managed deployment, the read-only operator checks are also useful:

   ```bash
   webcodex ops status --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE" --strict
   webcodex ops runners --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
   ```

   For a systemd Runner, use the same `--scope user|system` that installed it.
3. Determine whether the failing ChatGPT attempt reached the Server. If ordinary
   logs are insufficient, use the bounded one-call trace procedure in
   [Capture one failing tool call](#capture-one-failing-tool-call). A healthy
   Server/Runner plus no matching inbound request at the exact reproduction time
   is strong evidence that the failure is before WebCodex. Check trace-capture
   warnings before treating an absent file as definitive.
4. In ChatGPT, verify that Developer Mode / the developer MCP app is still
   available for that conversation/workspace. Reconnect or re-enable the same MCP
   if the Host UI offers that control. Testing the same endpoint in a fresh
   conversation is a useful isolation step: if one conversation works and another
   does not, do not change Runner/project configuration just to make the failing
   conversation look healthy.
5. Change WebCodex configuration only when the independent checks show a WebCodex
   problem (Server unreachable/auth failure, Runner offline, project missing, or a
   real `plugin_tool` diagnostic).

Creating another MCP entry with a different display name may cause the Host to
mount a fresh connection, but it is not a reliable WebCodex fix and does not
explain the underlying Host state. Likewise, do not rotate tokens, rewrite
`runner.toml`, re-register projects, or repeatedly restart a healthy Runner
solely because of the exact Host-level developer-MCP error above.

When reporting this class of issue, include only safe evidence:

- exact Host error text plus reproduction/recovery timestamps and timezone;
- ChatGPT surface and whether the same MCP works in a fresh conversation;
- `webcodex --version` and `webcodex-runner --version`;
- sanitized `webcodex runner status` / `webcodex ops status` output;
- if another conversation/client can still call `runtime_status`, its sanitized build/connection-layer summary;
- whether a matching Server request/trace was observed at the failure time.

Do **not** publish access tokens, OAuth secrets, `Authorization` headers,
complete env files, complete `runner.toml`, or an unreviewed raw full trace.

### ChatGPT reports `Thinking stopped` / `Thinking failed` during long-running work

A long-running WebCodex Job does not depend on one ChatGPT/model turn remaining
open. When a command or validation outlives the synchronous grace period, it
continues as the same Job with a stable `job_id`. Therefore, `Thinking stopped`
or `Thinking failed` in the ChatGPT UI during long-running work does **not by
itself** mean that the local process or WebCodex Job failed, and it does not
establish that a fixed Host/server timeout was reached.

When this happens:

1. **Do not immediately run the same task again.** If you still have the
   `job_id`, observe that Job first. Use the Job list only when its identity was
   genuinely lost.
2. If the existing Job is still running, queued, or recovering, keep observing
   it or continue independent work. Do not start a second copy merely because
   the ChatGPT turn ended.
3. If the same ChatGPT conversation can continue, send “continue” and ask it to
   re-observe the existing Job before resuming from the previous progress. A new
   model turn does not require restarting the underlying Job.
4. Start a replacement only after the original Job is confirmed terminal or
   lost and retrying is safe. If its state is uncertain, re-observe/reconcile
   the existing Job first to avoid duplicate processes, duplicate side effects,
   or resource conflicts.

See [Coding workflow: Long-running work](CODING_WORKFLOW.md#long-running-work)
and [Runner: Jobs and concurrency](RUNNER.md#jobs-and-concurrency) for the Job
lifecycle details.

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
is pruned after 24 hours. Managed Runner Tokens are not charged against those
shared-key count or retention limits. All Runner registrations have a
64-project input safety limit.

### `connect` rejects a `wc_*` value

This is deliberate. `wc_pat_*`, `wc_agent_*`, `wc_acct_*`, and other `wc_*`
values are managed credentials and never fall back to shared-key auth. Use a
different random key for the hosted shared-key flow, or use `webcodex login`
for managed identity.

`webcodex tokens generate` is offline material generation only. It does not
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

### Capture one failing tool call

When status/log output is not enough on a self-hosted Server, temporarily enable
full tool-request tracing and reproduce the failing call **once**:

```text
WEBCODEX_TOOL_REQUEST_TRACE=full
WEBCODEX_TOOL_REQUEST_TRACE_DIR=/var/lib/webcodex/tool-request-traces
```

Apply the Server environment change using your normal deployment lifecycle, note
the exact reproduction time/tool/error, then inspect the newest entry in the
configured trace directory. Disable `full` tracing again after the capture.

Full traces may contain source text, patches, script/stdin data, command output,
user messages, or secrets that were themselves present inside tool payloads. Do
not publish or paste a raw trace without reviewing/redacting it. If no new capture
appears, check the Server journal for trace-capture warnings rather than assuming
the request was empty.

For the internal trace layout, request/Runner correlation fields, payload layers,
and capture-omission semantics, see the maintainer-only
[Tool Request Tracing](agent/tool-request-tracing.md) contract.

### Client says `webcodex: command not found`

Install or symlink the CLI onto the client's `PATH`, for example:

```bash
sudo ln -s /opt/webcodex/bin/webcodex /usr/local/bin/webcodex
```

Use the actual install path for your host.

### Client accidentally runs `pairing create` and `/etc/webcodex/webcodex.env` is missing

`webcodex pairing create` is server/admin-side and uses the server bootstrap env file. A friend/client machine should run `webcodex login <server-url> --code <wc_pair_...>` with the short-lived `wc_pair_*` code from the server owner.

Copy only the `wc_pair_*` code between machines. Do not copy `WEBCODEX_TOKEN`, user API tokens, Runner tokens, env files, or complete `runner.toml` files.

### Doctor warns `binary webcodex not found in PATH` on a client

That can be acceptable on Runner-only client machines. Runner-only clients need the public `webcodex` CLI and `webcodex-runner`; `webcodex-server` is only required on server hosts.

### `client online: no`

For a hosted `connect` profile, use the profile-specific status and log path
shown above. For a systemd-managed deployment, check the Runner service and its
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

Also verify the server URL, local token files, and Runner `allowed_roots`. Missing or empty `allowed_roots` defaults to `$HOME`; explicit `allowed_roots` replaces that default.

### `tool_manifest` discovery is too broad

For GPT Actions, call the canonical `tool_manifest` operation directly and prefer
an exact `tool_name` or a `category` / `intent` filter for compact discovery. The
generic Actions surface no longer exposes the retired `listRuntimeTools` facade.

### GPT Action still uses an old schema

Re-import the OpenAPI schema from the deployed `/openapi.json`, then check the
operation count. It is derived from the current Adaptive Direct projection plus
`call_runtime_tool`, so do not compare it with a fixed recommended count. The
generated surface must remain below the GPT Actions 30-operation ceiling; if it
reaches that ceiling, change the canonical Adaptive projection or a real protocol
exception rather than silently truncating the schema.

### MCP tool list looks stale

Reconnect or restart the MCP client so it runs a fresh `initialize` and
`tools/list`. If the server was just upgraded, verify public HTTPS reaches the
new service and check `journalctl -u webcodex` for startup or auth errors.

### Runner is offline

Run `runtime_status` or `list_runners`, then check the Runner host:

```bash
webcodex runner status --scope user
webcodex runner logs --scope user --lines 100
# Use `sudo ... --scope system` for an administrator-managed system service.
```

Confirm the Runner server URL, token file, service user, and `allowed_roots`.

### Wrong token type

In the hosted quick-start, MCP and Runner use the same non-`wc_` shared key.
In managed mode, GPT Actions, MCP, and ordinary REST/project APIs use
`webcodex-user-token` (`wc_pat_*`), while the Runner token (`wc_agent_*`) is
only for Runner transport — after `webcodex login` it lives inline in
`runner.toml`, with no separate `webcodex-runner-token` file. A 403 after putting a `wc_agent_*`
value in `--token` or `--token-file` is the expected security boundary: select
the generated `webcodex-user-token` instead. Recent CLI commands also diagnose
this mismatch without printing the complete token. `WEBCODEX_TOKEN` is
bootstrap/admin-oriented and should not be copied into GPT Actions, MCP, or
Runner config.

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
the smoke at another safe Runner-backed git project.

### `operation_count` exceeds 30

The generated GPT Actions surface must stay below 30 operations. Long-tail
runtime tools, including chunked artifact upload tools, remain behind
`call_runtime_tool`; direct operations are derived from the canonical Adaptive
Direct surface rather than a separate Actions allowlist.

### `artifact_upload_chunk` says `path` is missing

`artifact_upload_chunk`, `artifact_upload_finish`, and `artifact_upload_abort`
must repeat the exact `path` used by `artifact_upload_begin`. This binds the
opaque `upload_id` to the requested target artifact path.

### `application/octet-stream` is rejected for an unsafe extension

Use a safe project-relative artifact path and a MIME type that matches the file
extension. For smoke tests, prefer a simple `.txt` path with `text/plain`. Avoid
secret-like paths, absolute paths, `.env*`, `.git`, token/credential paths, and
unsafe binary extensions.

## Desktop Diagnostics Center

**Settings → Troubleshooting** provides safe report/support-bundle export, Off/Metadata/Full request tracing, local diagnostic locations and a credential-free Runtime Console URL. Full tracing and copying a managed user credential require explicit confirmation. See [Desktop Runtime compatibility](DESKTOP_RUNTIME_COMPATIBILITY.md) for ownership-aware recovery and how to interpret response handoff/continuation evidence.
