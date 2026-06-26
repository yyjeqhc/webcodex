# GPT Actions

Private Drop Runtime exposes a small, stable OpenAPI 3.1 schema for ChatGPT
GPT Actions. GPT Actions and the MCP endpoint (`/mcp`) share a single
`ToolRuntime` — there is no separate business logic for either surface.

## Import URL

In your ChatGPT GPT, under **Settings → Actions → Import from URL**, enter:

```
http(s)://<your-server>/openapi.json
```

`/openapi.json` is the only GPT-Actions entry point. It is a `GET` route and
is **not** listed inside the schema `paths` (which is POST-only).

## Authentication

- Scheme: HTTP Bearer (`Authorization: Bearer <token>`).
- Token: the value of the `DROP_TOKEN` environment variable on the server.
- Bearer auth is enabled globally on the schema (`security` + `bearerAuth`).
- When `DROP_TOKEN` is unset, the server runs in development mode and auth is
  bypassed — never do this in production.

Configure the GPT Action authentication as **API Key**, type **HTTP**,
header `Authorization`, value `Bearer <DROP_TOKEN>`.

## Server URL

The `servers[0].url` in the schema defaults to `http://localhost:8080`. Override
it for deployments by setting `DROP_PUBLIC_URL` on the server, for example:

```bash
DROP_PUBLIC_URL="https://drop.example.com" cargo run --bin private-drop
```

## Operations

The schema exposes a small, stable set of operation ids (23),
grouped by recommended call flow. GPT Actions and MCP are peer surfaces over
the same `ToolRuntime`: GPT Actions expose selected typed OpenAPI operations,
while MCP exposes the runtime tool set directly through `tools/list` and
`tools/call`. Codex is an **optional advanced capability**: the inspection,
mutation, and shell actions work without Codex installed — only `runCodexTask`
requires the Codex CLI on the agent host.

### Read-only actions

| operationId | Path | Purpose |
|-------------|------|---------|
| `listRuntimeTools` | `POST /api/tools/list` | List every runtime tool name plus `names`, `count`, `categories`, and `recommended_flows` (advanced). |
| `listProjects` | `POST /api/projects/list` | List agent-registered project ids. **Call this first.** |
| `getRuntimeStatus` | `POST /api/runtime/status` | Structured runtime health/observability summary. Read-only; never exposes tokens or secrets. |
| `getRuntimeJobStatus` | `POST /api/jobs/status` | Poll the `job_id` returned by `runCodexTask`. |
| `getRuntimeJobLog` | `POST /api/jobs/log` | Read bounded stdout/stderr for the `job_id`. |
| `readProjectFile` | `POST /api/projects/read_file` | Read a UTF-8 file from a project (paths confined to project root). |
| `getProjectGitStatus` | `POST /api/projects/git_status` | Run `git status --porcelain` in a project. |
| `getProjectGitDiff` | `POST /api/projects/git_diff` | Run `git diff` in a project (optional `args`). Read-only. |
| `getProjectGitDiffSummary` | `POST /api/projects/git_diff_summary` | Read-only diff summary: porcelain, diffstat, changed-file list. |
| `listProjectFiles` | `POST /api/projects/list_files` | Read-only bounded project file listing. |
| `searchProjectText` | `POST /api/projects/search_text` | Read-only bounded text search inside a project. |
| `validateProjectPatch` | `POST /api/projects/validate_patch` | Read-only dry-run patch preflight (`git apply --check`/`--stat`); never writes files. |
| `listRuntimeJobs` | `POST /api/jobs/list` | Read-only bounded job summaries (metadata only, no stdout/stderr). |
| `getRuntimeJobTail` | `POST /api/jobs/tail` | Read-only bounded stdout/stderr tail for a job. |

### Mutation / execution actions

| operationId | Path | Purpose |
|-------------|------|---------|
| `runCodexTask` | `POST /api/codex/run` | Start a Codex CLI task, returns `job_id`. **Optional advanced action; requires Codex CLI on the agent host.** |
| `applyProjectPatch` | `POST /api/projects/apply_patch` | Apply a unified diff patch. **Mutation with side effects; Bearer auth + agent shell capability required.** |
| `applyProjectPatchChecked` | `POST /api/projects/apply_patch_checked` | Validate then apply a patch; returns post-apply diff summary. **Mutation with side effects; Bearer auth + agent shell capability required.** |
| `runProjectShellCommand` | `POST /api/projects/run_shell` | Run a shell command in a project. **Mutation with side effects; Bearer auth + agent shell capability required.** |
| `deleteProjectFiles` | `POST /api/projects/delete_files` | Delete selected project-relative files. **Mutation with side effects; Bearer auth + agent shell capability required.** |
| `gitRestorePaths` | `POST /api/projects/git_restore_paths` | `git restore` selected tracked paths. **Mutation with side effects; Bearer auth + agent shell capability required.** |
| `discardUntrackedFiles` | `POST /api/projects/discard_untracked` | `git clean -f` selected untracked files. **Mutation with side effects; Bearer auth + agent shell capability required.** |
| `replaceProjectFileText` | `POST /api/projects/replace_in_file` | Replace a unique substring in a project file via the owning agent. **Mutation with side effects; Bearer auth + agent shell capability required. Fails without writing when `old` is missing or ambiguous; rejects sensitive paths.** |

### Advanced escape hatch

| operationId | Path | Purpose |
|-------------|------|---------|
| `callRuntimeTool` | `POST /api/tools/call` | Generic entry point for any runtime tool by name. Prefer the dedicated actions above. |

### Recommended call flow

1. `getRuntimeStatus` — is the runtime healthy? Are agents registered and
   online? (See [docs/RUNTIME_STATUS.md](RUNTIME_STATUS.md).)
2. `listProjects` — learn the available `project` ids.
3. `getProjectGitStatus` / `getProjectGitDiffSummary` — inspect repository
   state before making changes.
4. `readProjectFile` / `listProjectFiles` / `searchProjectText` — read the
   focused files needed for the task.
5. `validateProjectPatch` — dry-run a generated patch without modifying the
   worktree.
6. `applyProjectPatchChecked` — apply the patch only when the preflight passes
   and get the post-apply diff summary.
7. `runProjectShellCommand` — run bounded diagnostics such as `cargo check`,
   `cargo test`, or script syntax checks when needed.
8. `listRuntimeJobs` / `getRuntimeJobTail` — inspect async job summaries and
   bounded tails.
9. For cleanup, prefer `deleteProjectFiles`, `gitRestorePaths`, and
   `discardUntrackedFiles` over ad hoc `rm` or broad shell.
10. For simple text edits, prefer `replaceProjectFileText` / `replace_in_file`
    over `runProjectShellCommand` `sed`/`awk`/`python` one-liners — it is safer
    and refuses to write on a missing/ambiguous match. Use `write_project_file`
    (via `callRuntimeTool` / MCP `tools/call`) to create new files (or overwrite
    with an `expected_sha256` guard). `replace_in_file` is a dedicated GPT
    Action; `write_project_file` remains runtime-only.
11. Optional Codex path: `runCodexTask`, then `getRuntimeJobStatus` /
    `getRuntimeJobLog`, when Codex CLI is installed and a larger delegated
    task is desired.

The dedicated inspection and execution actions are the robust default path for
ChatGPT-assisted development. MCP clients can drive the same loop with the
snake_case runtime tool names (`read_file`, `git_diff`, `validate_patch`,
`apply_patch_checked`, `apply_patch`, `run_shell`). Codex is optional and should
not be required for basic read/diff/patch/test workflows.

## `callRuntimeTool` (advanced escape hatch)

`callRuntimeTool` remains available as the advanced generic escape hatch for any
runtime tool that does not yet have a dedicated GPT Action. A custom GPT can now
complete the full core coding loop using only dedicated typed actions; reach for
`callRuntimeTool` only when a tool is not exposed as a dedicated operation.

- `list_tools`, `list_projects`, `list_agents`, `runtime_status`
- `run_shell`, `run_job`, `run_codex`
- `job_status`, `job_log`, `list_jobs`, `job_tail`
- `read_file`, `git_status`, `git_diff`, `git_diff_summary`
- `list_project_files`, `search_project_text`
- `validate_patch`, `apply_patch_checked`, `apply_patch`
- `delete_project_files`, `git_restore_paths`, `discard_untracked`
- `replace_in_file` (Phase 4 structured-edit tool; now a dedicated GPT Action
  `replaceProjectFileText`, also reachable via `callRuntimeTool` / MCP
  `tools/call`). Prefer it over `run_shell` `sed`/`awk`/`python` one-liners for
  simple text edits: the command is a fixed helper, `old`/`new` travel over
  stdin (never interpolated into the shell command), sensitive paths are
  rejected, and the file is left untouched when `old` is missing or ambiguous.
- `write_project_file` (Phase 4 structured-edit tool; runtime-only — not a
  dedicated GPT Action, reachable via `callRuntimeTool` / MCP `tools/call`).
  Use it to create new files or overwrite with an `expected_sha256` /
  `expected_content_prefix` guard.

### Request shapes

`callRuntimeTool` accepts several equivalent request shapes so a custom GPT
can call it even when the OpenAI schema layer drops or renames fields:

- `{"tool":"list_tools"}` — params omitted (argument-less tools).
- `{"tool":"list_tools","params":null}` — explicit null params.
- `{"tool":"git_diff_summary","params":{"project":"agent:c:p"}}` — standard form.
- `{"tool":"git_diff_summary","arguments":{"project":"agent:c:p"}}` — MCP-style
  `arguments` alias.

`arguments` is a compatibility alias for `params`. When both `params` and
`arguments` are present, **`params` wins**; `arguments` is only used when
`params` is absent. Omit both (or send `null`) for argument-less tools like
`list_tools`, `list_projects`, `list_agents`, `runtime_status`.

### Error messages

Errors are field-aware and never echo the raw request body, token, env,
`agent.toml`, or the `Authorization` header:

- Unknown tool → lists every accepted tool name and points at `listRuntimeTools`.
- Missing required field → names both the tool and the missing field.
- Wrong field type → names the tool.

`validateProjectPatch` is a read-only dry-run patch preflight: it does not
modify the worktree and is suitable for full-auto coding loops before
`applyProjectPatch` / `applyProjectPatchChecked`. `applyProjectPatchChecked`
combines preflight, apply, and post-apply diff summary in one safer mutation
call. `deleteProjectFiles`, `gitRestorePaths`, and `discardUntrackedFiles` are
restricted cleanup tools intended to reduce ad hoc `rm` and broad shell usage.
All of these are now dedicated GPT Actions (Phase 3); they are also still
discoverable through `listRuntimeTools` and callable via `callRuntimeTool`.

`params` is an OpenAPI 3.1 object (`type: object`, `additionalProperties: true`)
that carries tool-specific arguments. **Prefer the dedicated actions when they
cover the task.** GPT Actions has historically mishandled free-form object
parameters (`UnrecognizedKwargsError: params` has been observed), so the typed
dedicated actions (`getProjectGitDiff`, `applyProjectPatch`,
`runProjectShellCommand`, `readProjectFile`, `getProjectGitStatus`) are the
robust path and should be used instead of `callRuntimeTool` wherever possible.
In particular, **do not** ask GPT to assemble raw shell to run Codex; use
`runCodexTask` instead.

## Executable actions

`applyProjectPatch` and `runProjectShellCommand` are **executable** actions with
side effects (they mutate files or run arbitrary commands on the agent host).
They are deliberately exposed as dedicated, typed operations — not via the
generic `callRuntimeTool` escape hatch — so GPT has clear, named operations.
They require:

- Bearer auth (`DROP_TOKEN`) on every call.
- The owning agent's capability (`shell` for `runProjectShellCommand`; patching
  allowed for `applyProjectPatch`).

Treat them as development-collaboration capabilities with execution risk.

## Examples

Start a Codex task (optional advanced; requires Codex CLI on the agent host):

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/codex/run \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","prompt":"Inspect the codebase and summarize the runtime architecture."}'
```

Poll status:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/jobs/status \
  -H "Content-Type: application/json" \
  -d '{"job_id":"<job-id>"}'
```

Read a project file:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/projects/read_file \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","path":"README.md"}'
```

Check git status / git diff:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/projects/git_status \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop"}'

curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/projects/git_diff \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","args":["--stat"]}'
```

Run a shell command (executable; Bearer auth required):

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/projects/run_shell \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","command":"cargo test"}'
```

Apply a patch (executable mutation; Bearer auth required):

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/projects/apply_patch \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop","patch":"--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\n# Private Drop\n+edited\n"}'
```

## Shared ToolRuntime

GPT Actions and MCP both call `ToolRuntime::dispatch`. The dedicated GPT Actions
are thin HTTP wrappers that dispatch to the same `ToolCall` variants used by MCP
`tools/call`: `listProjects` → `ListProjects`, `readProjectFile` → `ReadFile`,
`getProjectGitStatus` → `GitStatus`, `getProjectGitDiff` → `GitDiff`,
`applyProjectPatch` → `ApplyPatch`, `runProjectShellCommand` → `RunShell`. No
business logic is duplicated, and owner/capability checks stay centralized in
`ToolRuntime::authorize_agent_tool`.

## Schema guarantees

Tests in `src/openapi.rs` assert:

- The operation-id set matches the documented set exactly (23 operations).
- The operation count is exactly 23 and never exceeds 30. Phase 5 promotes
  `replace_in_file` to a dedicated GPT Action (`replaceProjectFileText`).
  `write_project_file` remains a **runtime-only** tool (reachable via
  `callRuntimeTool` / MCP `tools/call`) and is intentionally NOT promoted to a
  dedicated GPT Action; its endpoint `POST /api/projects/write_file` is listed
  in the forbidden-paths guard so it can never leak into the schema.
- Every `$ref` resolves to a defined schema.
- Every path is POST-only.
- Bearer auth is present and globally enabled.
- No legacy/non-GPT-Actions paths appear in the schema (file-drop, desktop,
  raw shell, codex command/context, agent protocol routes, `/mcp`,
  `/openapi.json`, `/console`, `/api/jobs/stop`, `/api/audit/*`).
- `callRuntimeTool` declares `params` as an OpenAPI 3.1 object accepting
  arbitrary tool arguments, plus an `arguments` compatibility alias (`params`
  wins when both are present).
- `listRuntimeTools` returns `tools` (back-compat), `names`, `count`,
  `categories`, and `recommended_flows`.
- Mutation actions describe their side effects, Bearer auth, and agent shell
  capability requirement.
- Read-only dedicated actions are marked read-only in their descriptions.
- Key actions ship request examples so ChatGPT has concrete templates.
