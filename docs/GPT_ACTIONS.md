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

The schema exposes a small, stable set of operation ids, grouped by recommended
call flow. Codex is an **optional advanced capability**: the inspection,
mutation, and shell actions work without Codex installed — only `runCodexTask`
requires the Codex CLI on the agent host.

| Flow step | operationId | Path | Purpose |
|-----------|-------------|------|---------|
| Discovery | `listRuntimeTools` | `POST /api/tools/list` | List every runtime tool name (advanced). |
| Discovery | `listProjects` | `POST /api/projects/list` | List agent-registered project ids. **Call this first.** |
| Discovery | `getRuntimeStatus` | `POST /api/runtime/status` | Structured runtime health/observability summary (agent client summaries, project counts, job counts). Read-only; never exposes tokens or secrets. |
| Code task | `runCodexTask` | `POST /api/codex/run` | Start a Codex CLI task, returns `job_id`. **Optional advanced action; requires Codex CLI on the agent host.** |
| Code task | `getRuntimeJobStatus` | `POST /api/jobs/status` | Poll the `job_id` returned by `runCodexTask`. |
| Code task | `getRuntimeJobLog` | `POST /api/jobs/log` | Read bounded stdout/stderr for the `job_id`. |
| Inspect | `readProjectFile` | `POST /api/projects/read_file` | Read a UTF-8 file from a project (paths confined to project root). |
| Inspect | `getProjectGitStatus` | `POST /api/projects/git_status` | Run `git status --porcelain` in a project. |
| Inspect | `getProjectGitDiff` | `POST /api/projects/git_diff` | Run `git diff` in a project (optional `args`). Read-only. |
| Execute | `applyProjectPatch` | `POST /api/projects/apply_patch` | Apply a unified diff patch. **Executable mutation; Bearer auth + agent patch capability required.** |
| Execute | `runProjectShellCommand` | `POST /api/projects/run_shell` | Run a shell command in a project. **Executable with side effects; Bearer auth + agent shell capability required.** |
| Advanced | `callRuntimeTool` | `POST /api/tools/call` | Generic entry point for any runtime tool by name. Prefer the dedicated actions above. |

### Recommended call flow

1. `getRuntimeStatus` — is the runtime healthy? Are agents registered and
   online? (See [docs/RUNTIME_STATUS.md](RUNTIME_STATUS.md).)
2. `listProjects` — learn the available `project` ids.
3. `getProjectGitStatus` / `getProjectGitDiff` — inspect repository state before
   making changes.
4. `readProjectFile` — read the focused files needed for the task.
5. `runProjectShellCommand` — run bounded diagnostics such as `cargo check`,
   `cargo test`, or script syntax checks when needed.
6. `applyProjectPatch` — apply small reviewed patches through the owning agent.
7. Optional Codex path: `runCodexTask`, then `getRuntimeJobStatus` /
   `getRuntimeJobLog`, when Codex CLI is installed and a larger delegated task
   is desired.

The dedicated inspection and execution actions are the robust default path for
ChatGPT-assisted development. Codex is optional and should not be required for
basic read/diff/patch/test workflows.

## `callRuntimeTool` (advanced)

`callRuntimeTool` is the generic escape hatch. It takes a `tool` name and a
`params` object and dispatches to `ToolRuntime`. Accepted `tool` values:

- `list_tools`, `list_projects`, `list_agents`, `runtime_status`
- `run_shell`, `run_job`, `run_codex`
- `job_status`, `job_log`
- `read_file`, `git_status`, `git_diff`
- `apply_patch`

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

- The operation-id set matches the documented set exactly (no more, no less).
- Every `$ref` resolves to a defined schema.
- Every path is POST-only.
- Bearer auth is present and globally enabled.
- No legacy/non-GPT-Actions paths appear in the schema (file-drop, desktop,
  raw shell, codex command/context, agent protocol routes, `/mcp`,
  `/openapi.json`).
- `callRuntimeTool` declares `params` as an OpenAPI 3.1 object accepting
  arbitrary tool arguments.
- Executable actions (`applyProjectPatch`, `runProjectShellCommand`) describe
  their execution risk and auth requirement.
- Key actions ship request examples so ChatGPT has concrete templates.
