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

The schema exposes exactly 9 operation ids, grouped by recommended call flow:

| Flow step | operationId | Path | Purpose |
|-----------|-------------|------|---------|
| Discovery | `listRuntimeTools` | `POST /api/tools/list` | List every runtime tool name (advanced). |
| Discovery | `listProjects` | `POST /api/projects/list` | List configured project ids. **Call this first.** |
| Discovery | `getRuntimeStatus` | `POST /api/runtime/status` | Structured runtime health/observability summary (projects config status, agent client summaries, job counts). Read-only; never exposes tokens or secrets. |
| Code task | `runCodexTask` | `POST /api/codex/run` | Start a Codex CLI task, returns `job_id`. **Recommended primary action.** |
| Code task | `getRuntimeJobStatus` | `POST /api/jobs/status` | Poll the `job_id` returned by `runCodexTask`. |
| Code task | `getRuntimeJobLog` | `POST /api/jobs/log` | Read bounded stdout/stderr for the `job_id`. |
| Inspect | `readProjectFile` | `POST /api/projects/read_file` | Read a UTF-8 file from a project (paths confined to project root). |
| Inspect | `getProjectGitStatus` | `POST /api/projects/git_status` | Run `git status --porcelain` in a project. |
| Advanced | `callRuntimeTool` | `POST /api/tools/call` | Generic entry point for any runtime tool by name. Prefer the dedicated actions above. |

### Recommended call flow

1. `getRuntimeStatus` — is the runtime healthy? Are projects configured? Are
   agents online? (See [docs/RUNTIME_STATUS.md](RUNTIME_STATUS.md).)
2. `listProjects` — learn the available `project` ids.
3. `runCodexTask` — start a Codex task in a project; capture `job_id` from the
   response (`output.job_id`).
4. `getRuntimeJobStatus` — poll `job_id` until `status` is `completed`,
   `failed`, `stopped`, or `lost`.
5. `getRuntimeJobLog` — read the Codex output, using `tail_lines` / `offset`
   (`next_stdout_line`) for pagination.

For inspection before/after a task, use `readProjectFile` and
`getProjectGitStatus`. These are safe, read-only, and confined to configured
project roots.

## `callRuntimeTool` (advanced)

`callRuntimeTool` is the generic escape hatch. It takes a `tool` name and a
`params` object and dispatches to `ToolRuntime`. Accepted `tool` values:

- `list_tools`, `list_projects`, `list_agents`, `runtime_status`
- `run_shell`, `run_job`, `run_codex`
- `job_status`, `job_log`
- `read_file`, `git_status`, `git_diff`
- `apply_patch`

Prefer the dedicated actions when they cover the task — they give GPT clearer
named operations and better examples. In particular, **do not** ask GPT to
assemble raw shell to run Codex; use `runCodexTask` instead.

## Examples

Start a Codex task:

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

Check git status:

```bash
curl -H "Authorization: Bearer change-me" \
  -X POST http://127.0.0.1:8080/api/projects/git_status \
  -H "Content-Type: application/json" \
  -d '{"project":"private-drop"}'
```

## Shared ToolRuntime

GPT Actions and MCP both call `ToolRuntime::dispatch`. The dedicated GPT
Actions (`listProjects`, `readProjectFile`, `getProjectGitStatus`) are thin HTTP
wrappers that dispatch to the same `ToolCall::ListProjects`, `ToolCall::ReadFile`,
and `ToolCall::GitStatus` variants used by MCP `tools/call`. No business logic is
duplicated.

## Schema guarantees

Tests in `src/openapi.rs` assert:

- The operation-id set matches the documented 9 ids exactly (no more, no less).
- Every `$ref` resolves to a defined schema.
- Every path is POST-only.
- Bearer auth is present and globally enabled.
- No legacy/non-GPT-Actions paths appear in the schema (file-drop, desktop,
  raw shell, codex command/context, agent protocol routes, `/mcp`,
  `/openapi.json`).
- Key actions ship request examples so ChatGPT has concrete templates.
