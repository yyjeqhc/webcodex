# MCP App Console Development Plan

This document turns the current product direction into staged implementation
work. The goal is not to replace GPT Actions. GPT Actions remain the stable,
typed, model-callable development interface; the MCP App should become a
visual runtime console for observation, review, and human approval.

## Product Shape

```text
GPT Actions = stable typed machine interface
MCP App     = visual console, approval panel, and observability UI
ToolRuntime = the single business logic layer shared by both
```

The app should focus on the active runtime surface:

- Shared `ToolRuntime`
- Agent-registered projects
- WebSocket preferred, polling fallback
- GPT Actions and MCP as first-class clients
- Optional Codex jobs

Do not restore removed product surfaces:

- file-drop / message / channel UI
- workflow / command_request UI
- SSH executor UI
- old Codex dashboard behavior
- server-side `projects.toml` as the normal runtime project source

## Current Baseline

At the time this plan was written:

- Branch: `v2-mcp-runtime`
- Latest observed commit: `f8fd16c docs: streamline runtime documentation`
- Local E2E: `bash scripts/e2e_zero_config_ws.sh` passed with 22 passed / 0
  failed over WebSocket.
- `/openapi.json` exposes the current 12 GPT Action operation ids.
- MCP `tools/list` currently returns the same runtime tool set through
  `ToolRuntime`.

Always re-check `git status --short --untracked-files=all` and run the E2E
smoke before starting implementation in a new agent window.

## Design Principles

- Keep MCP and GPT Actions thin. Add core behavior to `ToolRuntime` or modules
  called by it, then expose it through REST wrappers, MCP tools, and the app.
- Add typed tools only when they improve structure, safety, or UI behavior.
  Avoid a large duplicate generic shell surface.
- Prefer read-only backend additions before mutation or execution additions.
- Treat patch application and shell execution as high-risk UI actions.
- Never display tokens, API keys, Authorization headers, full env vars, or real
  `agent.toml` / `private-drop.env` content.
- Keep app state project-centered: select a current runtime project id, then
  default file, diff, command, and job panels to that project.

## Phase A: Backend Read-Only Console Tools

Goal: add structured read-only tools that make the app safer and reduce ad hoc
shell usage.

Suggested tools / endpoints:

```text
listProjectFiles
searchProjectText
readProjectFileRange
getProjectGitDiffSummary
getRuntimeJobs
getRuntimeJobTail
```

Runtime tool names can stay snake_case:

```text
list_project_files
search_project_text
read_file_range
git_diff_summary
list_jobs
job_tail
```

Implementation notes:

- Put tool variants and business behavior in `src/tool_runtime.rs` or a focused
  helper module called by it.
- Route agent-backed file and git operations through the owning registered
  agent. Do not read agent project paths on the server.
- Keep REST wrappers in `src/runtime_http.rs` thin.
- Add MCP exposure through `tool_specs` and `ToolCall::from_tool_name`.
- Only add GPT Actions for stable typed tools that are useful to the model.
  It is reasonable for the app to call REST endpoints that are not all imported
  into GPT Actions, but they must still share `ToolRuntime`.

Acceptance:

- `list_project_files` returns bounded, project-relative paths plus basic file
  metadata where available.
- `search_project_text` returns bounded matches with path, line number, and
  preview text.
- `read_file_range` is an alias-compatible or clearer wrapper around current
  `read_file` pagination behavior.
- `git_diff_summary` returns `git status --porcelain`, `git diff --stat`, and a
  changed-file list without requiring the UI to parse a full diff.
- `list_jobs` returns bounded job summaries without stdout/stderr bodies.
- `job_tail` returns bounded stdout/stderr tails and cursor metadata.
- Existing `scripts/e2e_zero_config_ws.sh`, `cargo fmt --check`,
  `cargo check`, `cargo check --tests`, and `cargo test` pass.

> Status: Phase A backend tools are implemented (`list_project_files`,
> `search_project_text`, `git_diff_summary`, `list_jobs`, `job_tail`). They are
> exposed via MCP `tools/list` (18 tools) and thin REST wrappers, and are
> bounded + agent-backed. `/openapi.json` intentionally stays at 12 GPT Actions
> (the console tools are app-facing, not model-facing typed actions).
> `read_file_range` is deferred — the existing `read_file` pagination already
> covers that behavior and will be revisited when the file browser (Phase C)
> needs a distinct range tool. Local E2E passes 32/32 over both transports;
> `cargo test` passes 370 main + 22 agent tests. Phases B–G remain.

## Phase B: Runtime / Agent Status Console

Goal: build the first read-only console screen.

UI sections:

```text
server public_url
auth_enabled
active jobs
agents online/stale/offline
agent transport: websocket / polling
pending_requests
projects_count
```

Agent rows should show:

```text
client_id
display_name
owner
status
transport
connected
agent_protocol_version
last_seen
pending_requests
projects_count
```

UX requirements:

- WebSocket agents that move from `online` to `stale` must be visually obvious.
- Show transport and last heartbeat time near the status badge.
- Do not display tokens or raw config files.
- Refresh status on a timer, but avoid aggressive polling.

Acceptance:

- A user can tell whether the public URL, auth, agents, and projects are
  healthy without reading raw JSON.
- A stale WebSocket agent is clearly distinguishable from a polling fallback
  agent and from an offline agent.

## Phase C: Project Panel And File Browser

Goal: make the selected project the center of the app.

UI behavior:

- Show `listProjects` as structured project rows.
- Let the user select one current project.
- Default file, diff, command, and job panels to the current project.
- Provide file list/search/open flows.
- Show line numbers in file preview.
- Keep previews read-only in this phase.

Project card fields:

```text
runtime id
project id
client_id
path
allow_patch
executor
kind / description if present
```

Acceptance:

- A user can find and read a file without asking the model to run `find`,
  `grep`, or `sed`.
- File reads are bounded and project-relative.
- The UI does not reveal secret files by default. Sensitive filenames such as
  `agent.toml`, `private-drop.env`, `.env`, and local `projects.d` entries
  should carry a warning or be hidden unless explicitly opened by a trusted
  user flow.

## Phase D: Git Diff And Patch Approval

Goal: make review and approval the MCP App's strongest value.

UI behavior:

- Show `git status --porcelain`.
- Show `git diff --stat`.
- Show changed files grouped as added / modified / deleted / renamed.
- Allow per-file diff expansion.
- Show patch previews before mutation.
- Require explicit human approval before calling `applyProjectPatch`.
- After apply, automatically refresh git status and diff summary.

Suggested backend addition:

```text
validateProjectPatch
```

Runtime tool name:

```text
validate_patch
```

Validation behavior:

- Check that the patch can apply cleanly.
- Return affected project-relative files.
- Return whether the patch appears to create, modify, delete, or rename files.
- Return warnings for large patches, suspicious paths, binary patches, or
  sensitive filenames.
- Do not modify the worktree.

Implementation notes:

- Prefer `git apply --check --stat` or equivalent through the owning agent.
- Keep validation and application separate.
- Do not make validation silently fall back to applying a patch.

Acceptance:

- `validate_patch` does not write files.
- The app can preview the patch and require approval before mutation.
- After approval and application, the app shows the resulting git status/diff.

## Phase E: Command And Job Panels

Goal: expose execution clearly without making it feel like a generic dangerous
terminal.

Command panel fields:

```text
command
cwd
timeout
exit_code
stdout
stderr
duration
```

Job panel fields:

```text
job_id
kind
project
status
duration
exit_code
stdout tail
stderr tail
```

UX requirements:

- Mark `runProjectShellCommand` and async shell/Codex jobs as high-risk.
- Warn on dangerous commands, very long commands, secret-reading patterns, and
  broad filesystem operations.
- Support collapsed output, tail view, and copy command/output actions.
- Make Codex jobs optional advanced capability, not a required runtime path.

Acceptance:

- A user can inspect what command ran, where it ran, how long it took, and what
  it returned.
- The app can list recent jobs and inspect bounded tails without reading full
  logs by default.

## Phase F: Security, Retention, And Deployment Hardening

Goal: make the console acceptable for long-running deployments.

Tasks:

- Add rate limits for expensive read/search/diff endpoints.
- Add audit retention or cleanup policy.
- Add UI warnings for sensitive file names and high-risk commands.
- Ensure app responses and audit summaries use the existing sanitizer model.
- Keep admin/debug write operations out of GPT Actions unless a separate safety
  review approves them.

Acceptance:

- Operators can bound storage and request load.
- No UI panel displays tokens, Authorization headers, API keys, or full env.
- Docs explain what is safe to commit and what must stay local.

## Phase G: App Packaging And Real ChatGPT Validation

Goal: validate the full product shape with real ChatGPT surfaces.

Tasks:

- Deploy the current server and agent behind HTTPS.
- Import `/openapi.json` into GPT Actions and exercise the recommended flow.
- Connect a ChatGPT MCP client to `/mcp`.
- Open the MCP App console and verify status, project, diff, patch preview,
  command, and job flows.
- Run `scripts/smoke_deployment.sh` against the public endpoint.
- Confirm a WebSocket agent stays `online` across idle periods.

Acceptance:

- GPT Actions can still drive development directly.
- The MCP App can observe and approve high-risk steps.
- Both surfaces continue to share `ToolRuntime`.

## Suggested GLM Prompts

Use one prompt per implementation stage. Before each prompt, tell GLM to read
`docs/AGENT_HANDOFF.md`, `docs/INDEX.md`, `docs/DEPLOYMENT.md`, and this file.

### Prompt 1: Backend Read-Only Tools

```text
You are working in /root/git/private-drop on branch v2-mcp-runtime.
First read docs/AGENT_HANDOFF.md, docs/INDEX.md, docs/DEPLOYMENT.md, and
docs/MCP_APP_CONSOLE_PLAN.md. Run `git status --short --untracked-files=all`
and `bash scripts/e2e_zero_config_ws.sh` before editing.

Implement Phase A read-only console backend tools for Private Drop Runtime.
Keep ToolRuntime as the single business layer and keep REST/MCP wrappers thin.
Add bounded agent-backed tools for list_project_files, search_project_text,
git_diff_summary, list_jobs, and job_tail. Reuse existing read_file pagination
where possible instead of duplicating behavior. Do not read agent project paths
on the server; route through the owning registered agent. Do not expose tokens,
full env, or unbounded stdout/stderr.

Update tests and docs for the new tools. Keep /openapi.json intentionally small:
only add dedicated GPT Actions if the tool is stable and model-useful; otherwise
make the app call the REST wrapper while MCP sees the tool through tools/list.
Verify with cargo fmt, cargo fmt --check, cargo check, cargo check --tests,
cargo test, and bash scripts/e2e_zero_config_ws.sh.
```

### Prompt 2: Runtime / Agent Status Console

```text
Continue from the clean Phase A baseline. Read docs/MCP_APP_CONSOLE_PLAN.md and
inspect the existing frontend directory before editing. Build Phase B: a
read-only Private Drop console screen for runtime and agent status. It should
show public URL, auth_enabled, active jobs, project count, and agent rows with
status, transport, last_seen, connected, pending_requests, protocol version,
owner, and projects_count.

Do not revive the old file-drop/message/workflow dashboard. Use the active
runtime APIs only. Never display DROP_TOKEN, Authorization headers, API keys,
full env, agent.toml token values, private-drop.env values, or real projects.d
secret contents. Make stale WebSocket agents visually obvious and distinguish
websocket from polling.

Update docs and frontend checks. Run npm checks/builds if the frontend has
them, plus cargo fmt --check, cargo check, cargo check --tests, cargo test, and
bash scripts/e2e_zero_config_ws.sh.
```

### Prompt 3: Project And File Browser

```text
Continue from the clean console baseline. Implement Phase C: project selection,
project detail panel, file list/search/open, and read-only file preview with
line numbers. Use agent-registered runtime project ids from listProjects and
default later panels to the selected current project.

Use structured backend tools from Phase A. Avoid shelling out to find/grep/sed
from the UI. Keep file reads bounded and project-relative. Warn or hide
sensitive filenames such as agent.toml, private-drop.env, .env, and local
projects.d files unless the user explicitly opens them in a trusted flow.

Update tests/docs. Verify frontend checks/builds and the normal Rust/E2E checks.
```

### Prompt 4: Diff And Patch Approval

```text
Continue from the clean project/file browser baseline. Implement Phase D:
structured git status/diff UI and patch approval. Add a validate_patch runtime
tool and REST wrapper that checks whether a unified diff can apply, reports
affected files and warnings, and does not modify the worktree. Prefer git apply
--check/--stat through the owning registered agent.

The UI must preview patch content and validation results before calling
applyProjectPatch. Require explicit human approval. After apply, refresh git
status and diff summary. Keep mutation paths audited and do not expose secrets.

Add tests proving validate_patch is read-only and agent-backed. Verify with the
full Rust checks, frontend checks, and local E2E.
```

### Prompt 5: Command And Job Panels

```text
Continue from the clean approval baseline. Implement Phase E: command execution
and job/log panels. Show command, cwd, timeout, duration, exit_code, stdout,
stderr, and copy/collapse controls. Show job list, job status, bounded stdout
tail, bounded stderr tail, duration, and terminal state.

Clearly mark shell execution and async jobs as high-risk. Add UI warnings for
dangerous commands, very long commands, broad filesystem operations, and
secret-reading patterns. Codex jobs should appear as optional advanced
capability, not a baseline requirement.

Keep outputs bounded. Do not display Authorization headers, tokens, full env,
or secret config contents. Update docs/tests and run all checks.
```

### Prompt 6: Hardening And Real Deployment Validation

```text
Continue from the clean command/job baseline. Implement Phase F/G hardening and
validation work: rate limits for expensive console endpoints, audit/job
retention or cleanup policy docs, deployment docs for the console, and real
deployment smoke instructions.

Run scripts/smoke_deployment.sh against the live public endpoint when credentials
are provided by the user. Confirm WebSocket agents remain online across idle
periods and that GPT Actions plus MCP still share ToolRuntime. Keep admin/debug
write operations out of GPT Actions unless the safety model is explicitly
documented and reviewed.

Update docs/AGENT_HANDOFF.md with the final commit, tests, E2E result, and next
handoff notes.
```

## Verification Checklist For Every Phase

```bash
git status --short --untracked-files=all
bash scripts/e2e_zero_config_ws.sh
cargo fmt
cargo fmt --check
cargo check
cargo check --tests
cargo test
```

If frontend files changed:

```bash
cd frontend
npm run typecheck
npm run build
npm run check:dist
```

For deployment validation when a live endpoint is available:

```bash
DROP_PUBLIC_URL="https://drop.example.com" \
DROP_TOKEN="<your-secret>" \
bash scripts/smoke_deployment.sh
```
