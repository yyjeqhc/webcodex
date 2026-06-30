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

   Choose API key / HTTP authentication, set the auth type to **Bearer**, and paste a `wc_pat_xxx` personal API token. Do not use `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx`.

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

Configure the GPT Action with Bearer/API-key authentication in the GPT Action settings. The secret value must be a `wc_pat_xxx` personal API token.

Use a `wc_pat_xxx` personal API token for GPT Actions and MCP. The recommended explicit flow is: an administrator issues a one-time `wc_acct_xxx` account credential, then the user runs `webcodex-cli token create-local` locally to generate a `wc_pat_xxx` and register only its hash with the server.

Do not paste or store `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx` as a GPT Actions or MCP credential. `WEBCODEX_TOKEN` is only for server bootstrap/root/admin work, `wc_acct_xxx` is only for local token self-registration, and `wc_agent_xxx` is only for `webcodex-agent` WebSocket connectivity. Pairing/enrollment remains available as a shortcut: `webcodex-cli pairing create` creates a short-lived `wc_pair_*` code on the server/admin side, and `webcodex-cli client enroll` exchanges that code on the client side.

`?token=` is not a GPT Actions auth mechanism. It is accepted only by `/api/agents/ws` for WebSocket handshake compatibility.

GPT Actions require a public HTTPS URL for the WebCodex server.


## Token selection

Credential purpose summary:

- GPT Actions / MCP / `/api/tools/list` / `/api/tools/call`: use `wc_pat_xxx`.
- Server bootstrap and emergency admin: use `WEBCODEX_TOKEN`.
- Local self-registration of PATs and agent tokens: use `wc_acct_xxx` only with `webcodex-cli token create-local` or `webcodex-cli agent-token create-local`.
- Agent connection: use `wc_agent_xxx` only in `webcodex-agent` config.

A GPT Action configured with `wc_acct_xxx` will not be able to call runtime tools and leaks the wrong secret into the wrong surface. Generate a PAT instead:

```bash
webcodex-cli token create-local \
  --server https://your-domain.example \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

## Tool surface

The GPT Actions surface is intentionally smaller than the full admin API. It includes runtime, project, git, patch, file, shell/job, and optional Codex task operations.

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

1. `getRuntimeStatus` — verify runtime health and redacted agent policy summaries.
2. `getRuntimeStatus`, or `callRuntimeTool` with `list_agents` — confirm an online agent and its redacted policy summary or `agent_instance_id`.
3. `listProjects` — choose an `agent:<client_id>:<project_id>`.
4. `callRuntimeTool` with `show_changes`, plus `getProjectGitStatus`, `listProjectFiles`, `readProjectFile`, and `searchProjectText` — inspect before editing.
5. For scoped source edits with known line numbers, use `callRuntimeTool` with the structured line edit tools: `replace_line_range`, `insert_at_line`, and `delete_line_range`.
6. For broader multi-file edits, use `validateProjectPatch` first, then `applyProjectPatchChecked` only when the patch is intentional.
7. Use `writeProjectFile` only for new files or deliberate small whole-file overwrites; use `replaceProjectFileText` only for short exact substring changes.
8. `runProjectShellCommand` or `startProjectShellJob` — execute only bounded commands in registered projects after file edits are complete.
9. `runCodexTask` — optional advanced path when Codex CLI is installed and configured on the agent machine.

`runCodexTask` does not launch a new agent. It asks the already connected agent to run the Codex CLI in a project.

`show_changes` is a read-only project inspection tool available through
`callRuntimeTool`. It summarizes branch/head, modified/added/deleted/renamed/
untracked files, `git diff --stat`, optional bounded hunks, simple warnings for
untracked smoke/tmp/test/anchor files, and suggested next actions. Use it before
summarizing a task or committing. It requires `project:read` and never modifies,
cleans, stages, commits, or restores the worktree.

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

## Conversation file import / generated image saving

GPT Action OpenAPI operations and MCP/runtime tools are related but not identical. The runtime side exposes more tools, and `callRuntimeTool` is the generic entry point for runtime-only tools. To avoid approaching the GPT Actions operation limit, WebCodex exposes exactly one dedicated conversation-file import Action: `importConversationFilesToProject` at `POST /api/artifacts/import`.

Use this single Action for generated images, user-uploaded files, Code Interpreter outputs, PDFs, zip archives, CSV/JSON/text files, and other supported bounded binary artifacts. The recommended path remains `importConversationFilesToProject` plus `openaiFileIdRefs`. Do not create separate dedicated GPT Actions for images, zip files, or PDFs.

Recommended generated-image flow:

1. The GPT uses built-in image generation in the current ChatGPT conversation.
2. The GPT calls `importConversationFilesToProject` with `openaiFileIdRefs`, `project`, and optionally `output_dir` such as `docs/assets` or `artifacts/imports`. If the model already has a generated image, user upload, or Code Interpreter file reference from the current conversation, it must pass that file reference as `openaiFileIdRefs`; do not call the import Action with an empty array.
3. WebCodex immediately downloads each `download_link`, validates MIME type and project-relative output paths, and saves the file under the selected agent/project directory.
4. The response returns each saved file's `source_name`, `project`, `path`, `bytes_written`, `mime_type`, and `sha256`.


Do not use shell/base64 as a fallback for large files. Calling `save_project_artifact` through `callRuntimeTool` is only appropriate for small binary payloads or cases where a trusted base64 string already exists; the import Action with `openaiFileIdRefs` is the preferred path for ChatGPT conversation files.

Artifact runtime tools form the project-local read/write loop:

- `save_project_artifact` saves a bounded base64 payload into a project artifact path.
- `read_project_artifact_metadata` inspects artifact metadata such as bytes, MIME type, sha256, image dimensions, and zip entry count without returning file content.
- `read_project_artifact` reads small artifact content from a non-sensitive project path and returns `content_base64` plus `bytes`, `mime_type`, and `sha256`. It defaults to a small 1 MiB `max_bytes` cap and is intended for thumbnails, small JSON/zip test fixtures, and other small binary artifacts.

Do not use `read_project_artifact` for large files. Prefer metadata-only inspection, targeted source reads, or external artifact transfer flows instead of returning large base64 payloads through `callRuntimeTool`.

This flow does not call the OpenAI Images API from WebCodex and therefore does not consume `gpt-image-2` API image-generation charges. The image generation happens in ChatGPT; WebCodex only imports the resulting conversation file through the GPT Actions file-passing mechanism.

Security constraints: imports are limited to at most 10 files per request and 10 MiB per file. Paths must stay inside the project root; `..`, absolute paths, `.git`, `.env*`, `*.pem`, `secrets`, `tokens`, `node_modules`, and `target` paths are rejected. `overwrite` defaults to `false`. Zip files are saved as zip files and are not automatically extracted.


## Artifact metadata and chunked content reads

For existing project artifacts, prefer `read_project_artifact_metadata` first. It returns size, sha256, MIME type, and image dimensions where available without embedding file content in the GPT Action response.

Do not read large files as one base64 response. If content is needed, call `read_project_artifact` as a chunked content read: use `offset` and `length` (default 32768 bytes, maximum 65536 bytes) and continue from `next_offset` while `truncated` is true. The returned `content_base64` contains only the current chunk; `sha256` and `file_bytes` describe the full artifact file. This tool is for targeted inspection or small binary transfer, not large-file transfer.
