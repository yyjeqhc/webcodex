# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

WebCodex exposes a focused OpenAPI schema for ChatGPT GPT Actions at:

```text
GET /openapi.json
```

GPT Actions and MCP share the same `ToolRuntime`; GPT Actions provides typed REST operations while MCP provides MCP framing.

## Create a GPT Action in ChatGPT

The existing `docs/assets/gpt-action-*.png` screenshots are suitable for the current deployment guide because they cover the full ChatGPT GPT builder path: open the editor, configure the GPT, add an Action, set Bearer authentication, and import the WebCodex OpenAPI schema. Treat them as UI landmarks rather than exact button-position requirements; ChatGPT may rename or move controls over time.

Use the screenshots with the checklist below:

1. **Open or create a GPT.**

   ![Open GPT editor](assets/gpt-action-1.png)

   Start from ChatGPT's GPT creation or edit flow.

2. **Enter the GPT configuration screen.**

   ![Configure GPT](assets/gpt-action-2.png)

   Confirm you are editing the GPT's configuration, not an ordinary chat.

3. **Open Actions and add an Action.**

   ![Add an Action](assets/gpt-action-3.png)

   Use the Actions section of the GPT builder; do not paste the OpenAPI schema into the GPT instructions.

4. **Configure Action authentication.**

   ![Set Action authentication](assets/gpt-action-4.png)

   Choose API key / HTTP authentication, set the auth type to **Bearer**, and paste either the shared key for quick start or a `wc_pat_xxx` personal API token for managed mode. Do not choose OAuth for the shared-key quick start. Do not use `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx`.

5. **Import the OpenAPI schema and required metadata.**

   ![Import OpenAPI schema](assets/gpt-action-5.png)

   Import or paste the schema URL:

   ```text
   https://your-domain.example/openapi.json
   ```

   Set the GPT privacy policy URL if the ChatGPT UI requires it. Use your own product or deployment privacy URL; do not put secrets in that URL.

6. Save the Action, then test a harmless discovery call such as `getRuntimeStatus`, followed by `listProjects` and a read-only project call such as `getProjectGitStatus`.
7. Use mutation tools only against a known disposable project until the GPT has been validated.

## Authentication

Configure the GPT Action with Bearer/API-key authentication in the GPT Action settings. Static bearer/API-key host auth can be used with either a shared key for quick start or a `wc_pat_xxx` token for managed mode.

For production, use a `wc_pat_xxx` personal API token for GPT Actions and MCP. The recommended explicit flow is: an administrator issues a one-time `wc_acct_xxx` account credential, then the user runs `webcodex-cli token create-local` locally to generate a `wc_pat_xxx` and register only its hash with the server.

OAuth is a separate flow. Blank OAuth client fields usually mean the host may attempt OAuth metadata discovery, dynamic client registration, or client metadata discovery; they do not become no-auth or static bearer.

Do not paste or store `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx` as a GPT Actions or MCP credential. `WEBCODEX_TOKEN` is only for server bootstrap/root/admin work, `wc_acct_xxx` is only for local token self-registration, and `wc_agent_xxx` is only for `webcodex-agent` WebSocket connectivity. Pairing/enrollment remains available as a shortcut: `webcodex-cli pairing create` creates a short-lived `wc_pair_*` code on the server/admin side, and `webcodex-cli client enroll` exchanges that code on the client side.

`?token=` is not a GPT Actions auth mechanism. It is accepted only by `/api/agents/ws` for WebSocket handshake compatibility.

GPT Actions require a public HTTPS URL for the WebCodex server.


## Token selection

Credential purpose summary:

- GPT Actions / MCP / `/api/tools/list` / `/api/tools/call`: use the shared key for quick start, or `wc_pat_xxx` for managed mode.
- Server bootstrap and emergency admin: use `WEBCODEX_TOKEN`.
- Local self-registration of PATs and agent tokens: use `wc_acct_xxx` only with `webcodex-cli token create-local` or `webcodex-cli agent-token create-local`.
- Agent connection: use `wc_agent_xxx` only in `webcodex-agent` config.

A GPT Action configured with `wc_acct_xxx` will not be able to call runtime tools and leaks the wrong secret into the wrong surface. For managed mode, generate a PAT instead:

```bash
webcodex-cli token create-local \
  --server https://your-domain.example \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

## Tool surface

The GPT Actions surface is intentionally smaller than the full admin API. It includes runtime, project, git, patch, file, shell/job, artifact, and session operations.

GPT Actions can expose at most 30 operations/tools. The current WebCodex OpenAPI
surface is intentionally held at 27 operations. New runtime tools should usually
remain reachable through `callRuntimeTool` instead of becoming dedicated
Actions. Chunked artifact upload tools (`artifact_upload_begin`,
`artifact_upload_chunk`, `artifact_upload_finish`, `artifact_upload_abort`) are
not dedicated GPT Action operations; call them through `callRuntimeTool`.

It does not expose user, API-token, agent-token, pairing/enrollment, setup, doctor, npm, server management, or audit endpoints such as:

```text
/api/users/create
/api/tokens/create
/api/agent-tokens/create
/api/pairing/create
/api/pairing/enroll
/api/audit/sessions
```

Use `webcodex-cli` for those management tasks.

## Recommended flow

1. Optional: call `callRuntimeTool` with `start_session` and keep the returned `wc_sess_*` id for this task.
2. `getRuntimeStatus` — verify runtime health and redacted agent policy summaries.
3. `getRuntimeStatus`, or `callRuntimeTool` with `list_agents` — confirm an online agent and its redacted policy summary or `agent_instance_id`.
4. `listProjects` — choose an `agent:<client_id>:<project_id>`.
5. `callRuntimeTool` with `show_changes`, plus `getProjectGitStatus`, `listProjectFiles`, `readProjectFile`, and `searchProjectText` — inspect before editing.
6. For scoped source edits with known line numbers, use `callRuntimeTool` with the structured edit tools: `replace_line_range`, `insert_at_line`, `delete_line_range`, and `apply_text_edits`.
7. For broader multi-file edits, use `validateProjectPatch` first, then `applyProjectPatchChecked` only when the patch is intentional.
8. Use `writeProjectFile` only for new files or deliberate small whole-file overwrites; use `replaceProjectFileText` only for short exact substring changes.
9. `runProjectShellCommand` or `startProjectShellJob` — execute only bounded commands in registered projects after file edits are complete.
10. Call `callRuntimeTool` with `session_summary` to inspect recorded tool calls, then use `show_changes` for the current worktree state.
11. Prefer structured edit tools and the controlled `runProjectShellCommand` / `startProjectShellJob` validation flow for coding tasks.

Do not use `save_project_artifact`, `artifact_upload_begin`,
`artifact_upload_chunk`, `artifact_upload_finish`, or `artifact_upload_abort` as
source-writing tools. They are for bounded project artifact transfer, not for
editing UTF-8 source files.

Codex delegation is currently hidden from GPT Actions and model-facing runtime tool discovery. Operators who want Codex should run it outside WebCodex, or wait for a future explicit opt-in feature flag.

`show_changes` is a read-only project inspection tool available through
`callRuntimeTool`. It summarizes branch/head, modified/added/deleted/renamed/
untracked files, `git diff --stat`, optional bounded hunks, simple warnings for
untracked smoke/tmp/test/anchor files, optional session activity, and suggested
next actions. Use it before summarizing a task, reviewing, or committing. It
requires `project:read` and never modifies, cleans, stages, commits, or restores
the worktree.

Tool risk, OAuth scope, session risk class, MCP annotations, and path hints now
begin from `ToolMetadata`. This is the metadata-only foundation for a later
ToolKernel/ToolProvider design; it does not change runtime dispatch, OAuth grant
management, or the existing tool API.

`callRuntimeTool` now enters the same lightweight `ToolKernel` facade used by
MCP `tools/call`. The facade performs metadata-backed OAuth scope checks,
session event recording, `ToolCall` parsing, and dispatch to the existing
`ToolRuntime` handlers. It is not a provider system; concrete tool handlers and
schemas remain unchanged.

## Session tracking

`start_session` and `session_summary` are runtime tools for task tracking
foundation work. They let a caller group later `/api/tools/call` invocations
under an opaque `wc_sess_*` id and ask which tools ran, which succeeded or
failed, which project id was supplied, which write-like paths were inferred,
and which job-like calls returned a `job_id`.

Start a session through the generic Action:

```json
{
  "tool": "start_session",
  "params": {
    "project": "agent:workstation:my-repo",
    "title": "implement show_changes follow-up"
  }
}
```

Pass the returned id as `recording_session_id` metadata on later generic calls
when using `params` or `arguments`:

```json
{
  "tool": "read_file",
  "recording_session_id": "wc_sess_example",
  "params": {
    "project": "agent:workstation:my-repo",
    "path": "src/mcp.rs",
    "start_line": 1,
    "limit": 20
  }
}
```

Then summarize it directly, or pass the same id to `show_changes` so the git
state and session activity are returned together:

```json
{
  "tool": "show_changes",
  "params": {
    "project": "agent:workstation:my-repo",
    "session_id": "wc_sess_example",
    "include_diff": false,
    "session_event_limit": 30
  }
}
```

For `/api/tools/call`, top-level `recording_session_id` is recorder metadata
for the current generic wrapper call and is stripped before concrete tool
dispatch. Top-level `session_id` is ordinary flattened tool input when
`params`/`arguments` are absent. `params.session_id` is the `show_changes`
business argument that selects which session to summarize; those ids may be the
same or different.

The recorder is bounded. Session records, events, and messages may be persisted
and restored through the configured `sessions.json` ledger, but the ledger is
task continuity and handoff metadata rather than a complete audit log. The
recorder does not automatically modify a workspace and does not scan diffs.
Inputs and errors are redacted and bounded before storage. Current-session
bindings remain process-local in-memory state, not durable ledger state, and
may be lost on restart. For reliable long-running or cross-client workflows,
keep the explicit `session_id` and pass it as tool input or
`recording_session_id` metadata instead of relying only on current binding.

## Observability

`getRuntimeStatus` and `callRuntimeTool` with `list_agents` may show a redacted policy summary:

- `allow_raw_shell`
- `allow_cwd_anywhere`
- `allowed_roots`
- `max_timeout_secs`
- `max_output_bytes`

They must not expose tokens, env values, `Authorization` headers, full `agent.toml`, or shell `init_script` values.

## Compatibility notes

The management CLI compatibility commands `webcodex users`, `webcodex tokens`, and `webcodex agent-tokens` still work, but `webcodex-cli` is the recommended CLI for current setup and operations.

## Artifact transfer and conversation file import

Artifact transfer is a bounded project artifact transfer primitive. It is for
importing and exporting binary or external files associated with a project. It
is not the source-editing path, object storage, a gallery, or a large-file
platform.

GPT Action OpenAPI operations and MCP/runtime tools are related but not
identical. The runtime side exposes more tools, and `callRuntimeTool` is the
generic entry point for runtime-only tools. To stay under the GPT Actions
30-operation limit, WebCodex exposes exactly one dedicated conversation-file
import Action: `importConversationFilesToProject` at
`POST /api/artifacts/import`.

Use this single Action for generated images, user-uploaded files, Code Interpreter outputs, PDFs, zip archives, CSV/JSON/text files, and other supported bounded binary artifacts. The recommended path remains `importConversationFilesToProject` plus `openaiFileIdRefs`. Do not create separate dedicated GPT Actions for images, zip files, or PDFs.

Recommended generated-image flow:

1. The GPT uses built-in image generation in the current ChatGPT conversation.
2. The GPT calls `importConversationFilesToProject` with `openaiFileIdRefs`, `project`, and optionally `output_dir` such as `docs/assets` or `artifacts/imports`. If the model already has a generated image, user upload, or Code Interpreter file reference from the current conversation, it must pass that file reference as `openaiFileIdRefs`; do not call the import Action with an empty array.
3. WebCodex immediately downloads each `download_link`, validates MIME type and project-relative output paths, and saves the file under the selected agent/project directory.
4. The response returns each saved file's `source_name`, `project`, `path`, `bytes_written`, `mime_type`, and `sha256`.


Do not use shell/base64 as a fallback for large files. Calling
`save_project_artifact` through `callRuntimeTool` is only appropriate for small
binary payloads or cases where a trusted base64 string already exists; the
import Action with `openaiFileIdRefs` is the preferred path for ChatGPT
conversation files. `save_project_artifact` is not a replacement for
`writeProjectFile` or the structured source-editing tools.

Artifact runtime tools form the project-local read/write loop:

- `save_project_artifact` saves a bounded one-shot base64 payload into a project artifact path.
- `artifact_upload_begin` starts a bounded upload with optional `expected_bytes` and `expected_sha256` guards.
- `artifact_upload_chunk` appends one base64 chunk at the next contiguous `offset`.
- `artifact_upload_finish` verifies guards and atomically commits the temporary upload to the target path.
- `artifact_upload_abort` cleans temporary upload state when the upload fails, is cancelled, or is no longer needed.
- `read_project_artifact_metadata` inspects artifact metadata such as bytes, MIME type, sha256, image dimensions, and zip entry count without returning file content.
- `read_project_artifact` is a bounded chunked read from a non-sensitive project path and returns one base64 segment plus full-file metadata.

Do not use `read_project_artifact` for large files. Prefer metadata-only inspection, targeted source reads, or external artifact transfer flows instead of returning large base64 payloads through `callRuntimeTool`.

This flow does not call the OpenAI Images API from WebCodex and therefore does not consume `gpt-image-2` API image-generation charges. The image generation happens in ChatGPT; WebCodex only imports the resulting conversation file through the GPT Actions file-passing mechanism.

Security constraints: imports are limited to at most 10 files per request and 10 MiB per file. Paths must stay inside the project root; `..`, absolute paths, `.git`, `.env*`, `*.pem`, `secrets`, `tokens`, `node_modules`, and `target` paths are rejected. `overwrite` defaults to `false`. Zip files are saved as zip files and are not automatically extracted.


## Chunked artifact uploads

Use chunked upload through the generic `callRuntimeTool` Action:

1. `artifact_upload_begin`
2. `artifact_upload_chunk` until all bytes are sent
3. `artifact_upload_finish`

Call `artifact_upload_abort` when an upload fails, is cancelled, or is no longer
needed.

Each `artifact_upload_chunk` payload is base64 and the decoded chunk must be at
most 64 KiB. The artifact total limit is currently 10 MiB. `offset` must be
contiguous with the bytes already received. `expected_bytes` and
`expected_sha256` are optional integrity guards captured at begin time and
checked before finish commits the upload. `artifact_upload_finish` succeeds only
after the guard checks pass, then atomically commits the temporary upload to the
target project-relative path. `artifact_upload_abort` removes the temporary
upload state. Session logs do not record raw base64; they keep bounded summary
fields such as path, upload id, offsets, byte counts, and sha256 guard metadata.

## Artifact metadata and chunked content reads

For existing project artifacts, prefer `read_project_artifact_metadata` first. It returns size, sha256, MIME type, and image dimensions where available without embedding file content in the GPT Action response.

Do not read large files as one base64 response. If content is needed, call
`read_project_artifact` as a bounded chunked read: use `offset` and `length`
(default 32768 bytes, maximum 65536 bytes) and continue from `next_offset` while
`truncated` is true. The returned `content_base64` contains only the current
segment; `sha256`, `mime_type`, `file_bytes`, `offset`, `bytes_returned`,
`next_offset`, `truncated`, and `eof` describe the segment and full artifact
file. This is not an unlimited download tool.
