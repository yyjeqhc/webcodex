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

4. Configure authentication as HTTP/API key Bearer auth and use a `wc_pat_xxx` personal API token.
5. Save the app, then connect it in ChatGPT when prompted.
6. Test with low-risk discovery tools first: list tools, check runtime status, list projects, then call a read-only project tool.

## Authentication

Use Bearer authentication with a `wc_pat_xxx` personal API token.

Do not use these credentials for MCP:

- `WEBCODEX_TOKEN`: server bootstrap/root/admin credential.
- `wc_acct_xxx`: account credential used only by the user CLI to create local PATs and agent tokens.
- `wc_agent_xxx`: agent token used only by `webcodex-agent`.

The recommended flow is to issue a user account credential once, then have the user run `webcodex-cli token create-local` locally. That command generates a `wc_pat_xxx` and registers only its hash with the server.

## Runtime surface

MCP and GPT Actions share the same `ToolRuntime`. A tool call made through MCP reaches the same runtime, agent registry, project ids, and safety boundaries as a GPT Action call.

Typical MCP tools include:

- Discovery, health, and task tracking: `list_tools`, `start_session`, `session_summary`, `runtime_status`, `list_projects`, `list_agents`.
- Read-only project inspection: `show_changes`, `list_project_files`, `read_file`, `search_project_text`, `git_status`, `git_diff`, `git_diff_summary`, `git_diff_hunks`.
- Preferred structured edits: `replace_line_range`, `insert_at_line`, `delete_line_range`.
- Patch workflows: `validate_patch`, `apply_patch_checked`.
- Project commands and jobs: `run_shell`, `run_job`, `job_status`, `job_log`, `job_tail`.
- Structured Cargo helpers: `cargo_fmt`, `cargo_check`, `cargo_test`.
- Optional Codex CLI launcher: `run_codex`.

Use the structured line edit tools when you already know the target line range. Use patch tools for broader multi-file changes. Treat `run_shell` as a diagnostics/build/test fallback, not as the first source-editing path.

Use `show_changes` near the end of a task to summarize the current worktree,
check for untracked smoke/tmp/test files, review `git diff --stat`, request
optional bounded hunks with `include_diff=true`, and optionally include session
activity with `session_id`. It is read-only, requires `project:read`, and never
cleans, stages, commits, or restores files.

`start_session` and `session_summary` are the current task tracking foundation.
They create and read bounded in-memory recorder state only; they do not modify a
workspace and are not a complete audit log. Sessions are lost when the server
restarts. To group MCP tool calls, pass the session id as reserved metadata in
`tools/call` arguments:

```json
{
  "name": "read_file",
  "arguments": {
    "_session_id": "wc_sess_example",
    "project": "agent:special-container:webcodex",
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

For example, the sg4 smoke test used `agent:ubuntu-client:webcodex`.

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

Where `WEBCODEX_PAT` contains a `wc_pat_xxx` value generated with `webcodex-cli token create-local`.

## Common errors

### 401 Unauthorized

The token is missing, malformed, expired, revoked, or not recognized by the server. Generate a fresh `wc_pat_xxx` and verify the MCP client is reading the intended environment variable.

### 403 Forbidden

The token is valid but lacks the scope needed for the requested tool or project operation. Create a PAT with the scopes needed for your workflow.

### Wrong token type

MCP requires `wc_pat_xxx`. `WEBCODEX_TOKEN`, `wc_acct_xxx`, and `wc_agent_xxx` are intentionally for other surfaces.

### Agent offline

The server is up, but the selected `client_id` is offline or stale. Start `webcodex-agent` and check `runtime_status` or `list_agents`.

### Project not registered

The agent is online, but the requested `agent:<client_id>:<project_id>` does not exist. Add a top-level agent `projects.d/*.toml` file with `id` and `path`, then restart or refresh the agent.
