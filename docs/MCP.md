# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex exposes the same runtime tools used by GPT Actions through an MCP endpoint.

## Endpoint

```text
https://your-domain.example/mcp
```

## Deployment model

WebCodex currently exposes a remote MCP endpoint backed by WebCodex runtime tools. The connected `webcodex-agent` is a local execution agent, not the MCP client in the protocol sense.

In MCP terminology, the AI host creates the MCP client connection, WebCodex server acts as the MCP server, and the WebCodex agent executes project work behind that server. Local stdio MCP-server registration and external MCP-server brokering are separate future extensions, not required for the current endpoint.

Use your own WebCodex HTTPS domain in place of `your-domain.example`.

## Create a ChatGPT MCP app / connector

The screenshots in `docs/assets/mcp-*.png` show the ChatGPT app/connector flow:

![Open ChatGPT apps](assets/mcp-1.png)
![Choose webcodex](assets/mcp-2.png)
![Configure MCP URL and auth](assets/mcp-3.png)
![Connect webcodex](assets/mcp-4.png)

1. Open ChatGPT's apps/connectors area and choose to create or configure an MCP app.
2. Set the app name to something recognizable, for example `webcodex`.
3. Set the MCP server URL to:

   ```text
   https://your-domain.example/mcp
   ```

4. Configure authentication as HTTP/API key Bearer auth. Use the shared key for quick start, or a `wc_pat_xxx` personal API token for managed mode. Do not choose OAuth for the shared-key quick start.
5. Save the app, then connect it in ChatGPT when prompted.
6. Test with low-risk discovery tools first: list tools, check runtime status, list projects, then call a read-only project tool.

## Authentication

Use Bearer authentication with either a shared key for quick start or a `wc_pat_xxx` personal API token for managed mode. Static bearer/API-key host auth can carry either value as `Authorization: Bearer ...`.

OAuth is a separate flow. Blank OAuth client fields do not become no-auth or static bearer. Open demo mode is only for hosts with an explicit None / No authentication / no-auth setting and a WebCodex server started with `--open`.

Do not use these credentials for MCP:

- `WEBCODEX_TOKEN`: server bootstrap/root/admin credential.
- `wc_acct_xxx`: account credential used only by the user CLI to create local PATs and agent tokens.
- `wc_agent_xxx`: agent token used only by `webcodex-agent`.

For production, the recommended flow is to issue a user account credential once, then have the user run `webcodex-cli token create-local` locally. That command generates a `wc_pat_xxx` and registers only its hash with the server.

## Runtime surface

MCP and GPT Actions share the same `ToolRuntime`. A tool call made through MCP
reaches the same runtime, agent registry, project ids, and safety boundaries as
a GPT Action call. `tools/call` goes through the lightweight `ToolKernel`
facade, which centralizes metadata-backed OAuth checks, session event recording,
`ToolCall` parsing, and dispatch to the existing runtime handlers. This is
preparation for later provider work, not an external MCP host or provider
marketplace.

Runtime tool discovery includes annotations derived from `ToolMetadata`, a
lightweight precursor to ToolProvider. The metadata centralizes risk, OAuth
scope, read-only/destructive/open-world hints, project requirement, and path
hint facts without changing dispatch or tool behavior. Future external MCP
providers must generate equivalent metadata before their tools can be listed or
called.

Typical MCP tools include:

- Discovery, health, and task tracking: `list_tools`, `start_session`, `session_summary`, `runtime_status`, `list_projects`, `list_agents`.
- Read-only project inspection: `show_changes`, `list_project_files`, `read_file`, `search_project_text`, `git_status`, `git_diff`, `git_diff_summary`, `git_diff_hunks`.
- Preferred structured edits: `replace_line_range`, `insert_at_line`, `delete_line_range`.
- Patch workflows: `validate_patch`, `apply_patch_checked`.
- Project commands and jobs: `run_shell`, `run_job`, `job_status`, `job_log`, `job_tail`.
- Structured Cargo helpers: `cargo_fmt`, `cargo_check`, `cargo_test`.

Codex delegation (`run_codex`) is currently hidden from MCP `tools/list` and model-facing runtime discovery. Run Codex outside WebCodex, or wait for a future explicit opt-in feature flag.

Use the structured line edit tools when you already know the target line range. Use patch tools for broader multi-file changes. Treat `run_shell` and `run_job` as diagnostics/build/test fallbacks, not as the first source-editing path.

Use `show_changes` near the end of a task to summarize the current worktree,
check for untracked smoke/tmp/test files, review `git diff --stat`, request
optional bounded hunks with `include_diff=true`, and optionally include session
activity with `session_id`. It is read-only, requires `project:read`, and never
cleans, stages, commits, or restores files.

`start_session` and `session_summary` are the current task tracking foundation.
They create and read bounded task-recorder metadata only; they do not modify a
workspace. When session persistence is configured, session records, events, and
messages may be persisted and restored through the `sessions.json` ledger. The
ledger is for task continuity and handoff metadata, not a complete audit log.
Current-session bindings remain process-local in-memory state, so pass the
session id explicitly for deterministic MCP handoff. To group MCP tool calls,
pass the session id as reserved metadata in `tools/call` arguments:

```json
{
  "name": "read_file",
  "arguments": {
    "_session_id": "wc_sess_example",
    "project": "agent:workstation:my-repo",
    "path": "src/mcp.rs",
    "start_line": 1,
    "limit": 20
  }
}
```

WebCodex strips `_session_id` before dispatching the concrete tool, so it is
not forwarded to the tool parser, agent, or workspace files. The summary records
bounded/redacted start and finish events, including tool name, transport,
project id when supplied, risk class, status, duration, inferred write-like
paths, and returned `job_id` when available.

For `show_changes`, distinguish two session fields:

- `arguments._session_id` is MCP reserved tracking metadata for recording this
  `show_changes` call.
- `arguments.session_id` is the `show_changes` business parameter that asks it
  to include a session activity summary.

They can be the same id or different ids.

Use agent-backed project ids such as:

```text
agent:<client_id>:<project_id>
```

For example, `agent:workstation:my-repo`.

## Example client configuration

The exact shape depends on your MCP client. Use placeholders and environment variables for secrets; do not paste real tokens into committed config files.

```json
{
  "mcpServers": {
    "webcodex": {
      "url": "https://your-domain.example/mcp",
      "headers": {
        "<bearer-auth-header-name>": "Bearer ${WEBCODEX_PAT}"
      }
    }
  }
}
```

For quick start, `WEBCODEX_PAT` may contain the shared key. For managed mode, it contains a `wc_pat_xxx` value generated with `webcodex-cli token create-local`.

## Common errors

### 401 Unauthorized

The token is missing, malformed, expired, revoked, or not recognized by the server. Confirm the quick-start shared key matches the agent/server key; in managed mode, generate a fresh `wc_pat_xxx` and verify the MCP client is reading the intended environment variable.

### 403 Forbidden

The token is valid but lacks the scope needed for the requested tool or project operation. Create a PAT with the scopes needed for your workflow.

### Wrong token type

MCP static Bearer auth can use the quick-start shared key or a managed `wc_pat_xxx`. `WEBCODEX_TOKEN`, `wc_acct_xxx`, and `wc_agent_xxx` are intentionally for other surfaces.

### Agent offline

The server is up, but the selected `client_id` is offline or stale. Start `webcodex-agent` and check `runtime_status` or `list_agents`.

### Project not registered

The agent is online, but the requested `agent:<client_id>:<project_id>` does not exist. Add a top-level agent `projects.d/*.toml` file with `id` and `path`, then restart or refresh the agent.
