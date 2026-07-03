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

Runtime tools can be exposed directly as MCP tools, subject to the tool manifest
and MCP client constraints. GPT Actions are different: the dedicated operation
surface must stay below the 30-operation limit, so artifact upload remains
available there through `callRuntimeTool` rather than dedicated operations.

Runtime tool discovery includes annotations derived from `ToolMetadata`, a
lightweight precursor to ToolProvider. The metadata centralizes risk, OAuth
scope, read-only/destructive/open-world hints, project requirement, and path
hint facts without changing dispatch or tool behavior. Future external MCP
providers must generate equivalent metadata before their tools can be listed or
called.

MCP `tools/list` remains the full schema-oriented discovery surface. In GPT
Actions, full `listRuntimeTools` can be too large because schemas, metadata,
and descriptions expand the response; this is not a sign that the roughly
65-tool runtime surface has become unbounded. GPT Actions should prefer
`callRuntimeTool` with `tool_manifest` for daily discovery, or pass
`summary_only=true` with `category`, `features`, or `limit` to
`listRuntimeTools` for focused discovery. Use full `listRuntimeTools` only when
debugging runtime schemas.

For smoke project selection, call `list_projects` and prefer
`capabilities.recommended_for_smoke=true`. For git smoke, require
`capabilities.git_available=true`; `agent:special:test-mcp` can be safe for
basic smoke without being git-backed.

Typical MCP tools include:

- Discovery, health, and task tracking: `list_tools`, `start_session`,
  `start_coding_task`, `finish_coding_task`, `session_summary`,
  `session_handoff_summary`, `runtime_status`, `list_projects`, `list_agents`.
- Read-only project inspection: `show_changes`, `list_project_files`, `read_file`, `search_project_text`, `git_status`, `git_diff`, `git_diff_summary`, `git_diff_hunks`.
- Preferred structured edits: `replace_line_range`, `insert_at_line`, `delete_line_range`, `apply_text_edits`.
- Patch workflows: `validate_patch`, `apply_patch_checked`.
- Bounded artifact transfer: `save_project_artifact`, `read_project_artifact_metadata`, `read_project_artifact`, `artifact_upload_begin`, `artifact_upload_chunk`, `artifact_upload_finish`, `artifact_upload_abort`.
- Project commands and jobs: `run_shell`, `run_job`, `job_status`, `job_log`, `job_tail`.
- Structured Cargo helpers: `cargo_fmt`, `cargo_check`, `cargo_test`.

Codex delegation (`run_codex`) is currently hidden/disabled from MCP `tools/list` and model-facing runtime discovery. Run Codex outside WebCodex. The legacy `/api/codex/run` endpoint is not mounted unless `WEBCODEX_ENABLE_LEGACY_CODEX_RUN=1`, and that opt-in preserves only the old endpoint shape; it does not re-enable `run_codex`.

Use `start_coding_task` for the recommended coding-loop entry point, then
inspect with `read_file`, `search_project_text`, and `show_changes`. Use the
structured line edit tools when you already know the target line range, and
`apply_text_edits` for coordinated exact edits in one UTF-8 file. Use
`validate_patch` and `apply_patch_checked` for broader multi-file changes.
Validate with `cargo_fmt`, `cargo_check`, `cargo_test`, `validate_patch`, and
`apply_patch_checked` before falling back to bounded command/job tools. Treat
`run_shell` and `run_job` as diagnostics/build/test fallbacks, not as the first
source-editing path or the default validation source. Finish with
`finish_coding_task`, or use `session_handoff_summary` for multi-step handoff.

Artifact transfer is a bounded project artifact transfer primitive for binary
or external files associated with a project. It is not the source-editing path,
object storage, a gallery, or a large-file platform. Do not use
`save_project_artifact`, `artifact_upload_begin`, `artifact_upload_chunk`,
`artifact_upload_finish`, or `artifact_upload_abort` as replacements for
`replace_line_range`, `insert_at_line`, `delete_line_range`, `apply_text_edits`,
or `apply_patch_checked`.

The minimal chunked upload sequence is `artifact_upload_begin`,
`artifact_upload_chunk` until all bytes are sent, then
`artifact_upload_finish`. Use `artifact_upload_abort` when the upload fails, is
cancelled, or is no longer needed. Each chunk is base64, decoded chunks are
limited to 64 KiB, the current artifact total limit is 10 MiB, `offset` must be
contiguous, and `artifact_upload_chunk`, `artifact_upload_finish`, and
`artifact_upload_abort` must repeat the exact `path` from
`artifact_upload_begin`. Repeating `path` is intentional because it binds
`upload_id` to the requested target artifact path. `expected_bytes` /
`expected_sha256` are integrity guards checked before finish atomically commits
the target path. Session metadata records summary fields rather than raw
base64. For smoke artifacts, use `artifacts/smoke/<name>.artifact` or
`artifacts/smoke/<name>.txt`; do not use `.bin` with
`application/octet-stream`. `artifact_upload_abort` reports
`final_file_exists` after removing temporary upload state.

For download/readback, use `read_project_artifact_metadata` before
`read_project_artifact`. `read_project_artifact` returns a bounded base64
segment with `sha256`, `mime_type`, `offset`, `next_offset`, `truncated`, and
`eof`; continue from `next_offset` while needed. It is not an unlimited file
download tool. To verify an expected absence, call
`read_project_artifact_metadata` with `allow_missing=true`; missing then returns
`exists=false` and `missing=true` as a successful result.

Use `show_changes` near the end of a task to summarize the current worktree,
check for untracked smoke/tmp/test files, review `git diff --stat`, request
optional bounded hunks with `include_diff=true`, and optionally include session
activity with `session_id`. It is read-only, requires `project:read`, and never
cleans, stages, commits, or restores files.

`start_session`, `start_coding_task`, `finish_coding_task`, `session_summary`,
and `session_handoff_summary` are the current task tracking foundation. They
create, close out, and read bounded task-recorder metadata only; they do not
modify a workspace. `start_session` creates a session record but does not
automatically bind future calls. `start_coding_task` defaults
`bind_current=false`; subsequent MCP calls should pass the returned explicit
`session_id`. When session persistence is configured, session records, events,
and messages may be persisted and restored through the `sessions.json` ledger.
The ledger is for task continuity and handoff metadata, not a complete audit
log. Current-session bindings remain process-local in-memory state and may be
lost on restart, so pass the session id explicitly for deterministic MCP
handoff. To group MCP tool calls, pass the session id as reserved metadata in
`tools/call` arguments:

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
In session summaries, `policy_rejected` means a safety or policy check blocked
the request before a write. A missing artifact result from
`read_project_artifact_metadata` with `allow_missing=true` is a successful
negative assertion, not a failed tool call.

For `show_changes`, distinguish two session fields:

- `arguments._session_id` is MCP reserved tracking metadata for recording this
  `show_changes` call.
- `arguments.session_id` is the `show_changes` business parameter that asks it
  to include a session activity summary.

They can be the same id or different ids.

`session_handoff_summary` requires explicit `arguments.session_id`; it does not
implicitly use the current-session binding. Its `validation` section is
ledger-derived from validation-like tools (`cargo_fmt`, `cargo_check`,
`cargo_test`, `validate_patch`, and `apply_patch_checked`). It does not expose
raw stdout/stderr, excerpt fields, or `validation_output_summary`, and the
minimal parser extracts only stable facts from safe bounded metadata without
root-cause inference, fix suggestions, LSP/tree-sitter, or LLM summarization.

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
