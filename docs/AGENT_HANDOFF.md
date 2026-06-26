# Agent Handoff Notes

This file exists for future coding agents after context compaction or a fresh
window. Read it before making changes.

## Project Identity

Private Drop Runtime is now a self-hosted tool runtime for ChatGPT. ChatGPT can
connect through:

- GPT Actions: `GET /openapi.json`
- MCP: `POST /mcp`

Both surfaces must use the same backend execution layer: `ToolRuntime`.

Current direction: the server should be zero project config. Runtime projects
are registered by agents and exposed as ids like
`agent:<client_id>:<project_id>`. Do not make `projects.toml` the normal
runtime project source again.

## Current Baseline

Latest known baseline when this file was written:

- Branch: `v2-mcp-runtime`
- Commit: current `HEAD` (`Harden generic runtime tool calls`)
- Main binary: `private-drop`
- Agent binary: `private-drop-agent`

Always run `git status --short --untracked-files=all` first. The user often has
uncommitted work from another agent window; do not overwrite or revert it.

## Core Files

- `src/tool_runtime.rs`: shared tool execution layer. Put business logic here
  or in modules it calls.
- `src/runtime_http.rs`: thin REST/GPT Actions wrappers.
- `src/mcp.rs`: thin MCP JSON-RPC wrapper.
- `src/openapi.rs`: minimal GPT Actions OpenAPI schema.
- `src/main.rs`: route wiring and shared state injection.
- `src/projects.rs`: legacy server-side project config parser. Treat this as
  non-primary for runtime project discovery unless a task explicitly says
  otherwise.
- `src/shell_client.rs`: polling agent registry and in-memory agent job queue.
  Now also carries the `transport` label (`polling`/`websocket`) and a push
  notifier map used by the WebSocket handler.
- `src/agent_ws.rs`: WebSocket agent endpoint (`GET /api/agents/ws`). Thin: only
  translates between `AgentEnvelope` and `ShellClientRegistry` calls.
- `src/shell_protocol.rs`: shared protocol types including the transport-neutral
  `AgentEnvelope`.
- `src/bin/private-drop-agent.rs`: agent process. Selects `polling` (default) or
  `websocket` transport via config; both reuse `dispatch_request` / `JobManager`
  through an `AgentSink` abstraction.
- `src/action_sessions.rs`, `src/action_audit.rs`, `src/audit_http.rs`: audit
  storage and read-only admin/debug API.

## Live Public/Integration Surfaces

GPT Actions schema exposes a small stable set of POST operations. Check
`src/openapi.rs` tests for the exact operation ID set.

Important runtime endpoints:

- `POST /api/runtime/status`
- `GET /api/agents/ws` (WebSocket, preferred agent transport; Bearer auth in
  handshake header)
- `POST /api/shell/agent/register` / `/poll` / `/result` / `/job_update`
  (polling agent fallback)
- `POST /api/projects/list`
- `POST /api/projects/read_file`
- `POST /api/projects/git_status`
- `POST /api/projects/git_diff`
- `POST /api/projects/apply_patch`
- `POST /api/projects/validate_patch`
- `POST /api/projects/run_shell`
- `POST /api/codex/run`
- `POST /api/jobs/status`
- `POST /api/jobs/log`
- `POST /api/tools/list`
- `POST /api/tools/call`
- `POST /mcp`

GPT Actions `callRuntimeTool` (`POST /api/tools/call`) is the advanced generic
entry point. It accepts `params` omitted, `params: null`, or `arguments` as a
compatibility alias. If both `params` and `arguments` are present, `params`
wins. `listRuntimeTools` (`POST /api/tools/list`) returns `tools`, `names`,
`count`, `categories`, and `recommended_flows`.

MCP App console read-only tools (Phase A; thin REST wrappers over
`ToolRuntime`, also exposed via MCP `tools/list`. Now also dedicated GPT Actions
as of Phase 3 — `/openapi.json` grew from 12 to 22 ops, then to 23 in Phase 5,
then to 25 this phase):

- `POST /api/projects/list_files` → `listProjectFiles` (read-only GPT Action)
- `POST /api/projects/search_text` → `searchProjectText` (read-only GPT Action)
- `POST /api/projects/git_diff_summary` → `getProjectGitDiffSummary` (read-only GPT Action)
- `POST /api/jobs/list` → `listRuntimeJobs` (read-only GPT Action)
- `POST /api/jobs/tail` → `getRuntimeJobTail` (read-only GPT Action)

Runtime/MCP-only patch and cleanup tools (now also dedicated GPT Actions as of
Phase 3):

- `POST /api/projects/validate_patch` → `validateProjectPatch` (read-only dry-run GPT Action)
- `POST /api/projects/apply_patch_checked` → `applyProjectPatchChecked` (mutation GPT Action)
- `POST /api/projects/delete_files` → `deleteProjectFiles` (mutation GPT Action)
- `POST /api/projects/git_restore_paths` → `gitRestorePaths` (mutation GPT Action)
- `POST /api/projects/discard_untracked` → `discardUntrackedFiles` (mutation GPT Action)

Phase 4/5 structured-edit tools (and this phase's promotions):

- `POST /api/projects/replace_in_file` → `replaceProjectFileText` (mutation;
  dedicated GPT Action as of Phase 5; thin REST wrapper over
  `ToolCall::ReplaceInFile`). Fixed python3 helper on the owning agent;
  `old`/`new` travel over stdin, never interpolated into the command. Also
  reachable via `callRuntimeTool` / MCP `tools/call`.
- `POST /api/projects/write_file` → `writeProjectFile` (mutation; dedicated
  GPT Action as of this phase; thin REST wrapper over
  `ToolCall::WriteProjectFile`). Create/overwrite a UTF-8 file via the owning
  agent with optional `expected_sha256` / `expected_content_prefix` guards.
  Rejects sensitive paths. Also reachable via `callRuntimeTool` / MCP
  `tools/call`.
- `POST /api/projects/run_job` → `startProjectShellJob` (execution; dedicated
  GPT Action as of this phase; thin REST wrapper over `ToolCall::RunJob`).
  Starts an async background shell job and returns a `job_id`; poll with
  `getRuntimeJobStatus` and read output with `getRuntimeJobTail` /
  `getRuntimeJobLog`. Requires the agent async shell job capability. Also
  reachable via `callRuntimeTool` / MCP `tools/call`.

MCP App console (Phase B; read-only status console. Public static HTML/JS/CSS
entry, NOT behind Bearer auth — like `/openapi.json`. Data is fetched by the
browser from the protected `POST /api/runtime/status`. Not a GPT Action; the
route is explicitly excluded from `/openapi.json`):

- `GET /console` (HTML shell)
- `GET /console/app.js`
- `GET /console/styles.css`

Admin/debug only:

- `POST /api/audit/sessions`
- `POST /api/audit/session`
- `POST /api/audit/stats`
- `POST /api/jobs/stop`

Do not expose admin/debug write-like operations as GPT Actions without a
specific safety review.

## Architectural Invariants

- `ToolRuntime` is the single execution layer.
- MCP and REST wrappers stay thin: auth/protocol envelope/deserialization only.
- Runtime project discovery comes from agent registration, not server-side
  project config.
- **Codex is an optional advanced capability, not a runtime dependency.** When
  the Codex CLI is not installed, the runtime still serves `read_file`,
  `git_status`, `git_diff`, `apply_patch`, and `run_shell` through the agent.
  Only `run_codex` requires the Codex CLI on the agent host.
- `CODEX_APPROVAL_MODE` defaults to empty (disabled): `--approval-mode` is not
  emitted unless a non-disabled value (`full-auto`, `suggest`, ...) is set in
  config or the request. This keeps the runtime compatible with Codex CLI
  builds that lack the flag.
- **Never commit local deployment config.** `agent.toml` and `projects.d/*.toml`
  contain real server URLs, tokens, and host paths. They are git-ignored
  (`/agent.toml`, `/projects.d/`, `/*.local.toml`, `/private-drop.env`). Do not
  `git add` them. If a token was ever committed or exposed, rotate `DROP_TOKEN`.
- Do not create a second Codex runner, shell runner, or MCP-specific business
  path.
- Project file access is routed to the owning registered agent; server-side
  project path config is not the runtime source of truth.
- Agent tools must keep owner and capability checks centralized.
- Do not log or return tokens, API keys, full secrets, full prompts containing
  credentials, or unbounded stdout/stderr.
- WebSocket liveness is based on keepalive traffic. The agent sends application
  `Ping` envelopes every 30s; the server replies `Pong` and refreshes
  `last_seen` via `ShellClientRegistry::touch_client`. `Pong` is normal
  keepalive traffic on both sides and must never be treated as an unexpected
  envelope. This prevents idle WebSocket agents from decaying to `stale` while
  the socket is healthy.
- Keep GPT Actions small and stable. Prefer dedicated typed actions
  (`readProjectFile`, `getProjectGitStatus`, `getProjectGitDiff`,
  `applyProjectPatch`, `runProjectShellCommand`) plus optional `runCodexTask`;
  keep `callRuntimeTool` advanced. Executable actions require Bearer auth and
  the relevant agent capability.

## Job Lifecycle Notes

Local jobs are written under `.codex/jobs/<job_id>/`.

Current behavior:

- Local job metadata includes `kind`, `max_runtime_secs`, and current jobs write
  `process_group_id`.
- `job_status` and `job_log` can recover local jobs from disk after restart.
- Over-time running local jobs are marked `lost` and the recorded process group
  is terminated when possible.
- `stop_job` exists for local job lifecycle control and is wired to
  `POST /api/jobs/stop`, but it is not a GPT Action.
- Old metadata without `pid` / `process_group_id` is never guessed; it can only
  be marked terminal.

## Testing Commands

Run these before reporting completion unless the user explicitly says not to:

```bash
cargo fmt
cargo fmt --check
cargo check
cargo check --tests
cargo test
```

A single pre-release gate runs all of the above plus both E2E transports and a
static check that no sensitive files are tracked/staged:

```bash
bash scripts/release_check.sh
```

The release gate does not re-count operations statically; the E2E harness
asserts the invariants: `/openapi.json` has exactly 25 operations and MCP
`tools/list` has exactly 25 tools.

Expected current result:

- `cargo check`: 0 warnings.
- `cargo check --tests`: 0 warnings.
- `cargo test`: main binary 475 tests passing, agent binary 23 tests passing.
  (Phase 4 added tests covering `replace_in_file` / `write_project_file`
  parsing, path validation, agent routing, capability checks, helper-script
  semantics, and the runtime-only REST wrappers. Phase 6 hardening added
  tests pinning the agent-backed patch chain: patch content travels over
  `ShellRunRequest.stdin` (never in the command string), `cwd` is supplied via
  the shell request field (no `cd` prefix), `apply_patch_checked` skips the
  apply step when the preflight fails, `validate_patch` never enqueues a
  mutating `git apply -`, large patches over the command limit still
  validate/apply, and server-configured projects are rejected by every patch
  tool. Phase 7 added the `additionalProperties=false` requestBody contract
  guard and expanded the read-only / mutation description guards to cover
  every operation.)

If `cargo test` hangs, do not assume the test suite is too large. Use:

```bash
timeout 120 cargo test -- --test-threads=1 --nocapture
```

The previous hang was caused by holding `local_jobs` across recovery in
`stop_job`; that is fixed. Env-var tests use a shared test lock to avoid
parallel pollution.

Shell scripts should be syntax-checked with `bash -n` (and Python snippets
with `python3 -m py_compile` if changed):

```bash
bash -n scripts/e2e_zero_config_ws.sh
bash -n scripts/smoke_deployment.sh
```

Current E2E smoke result:

- `bash scripts/e2e_zero_config_ws.sh`: 108 passed / 0 failed.
- `E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh`: 108 passed / 0
  failed.

The E2E smoke now includes a "full-auto coding loop smoke" stage (section 7h)
that simulates a GPT Actions auto-coding loop using ONLY dedicated endpoints
(no `callRuntimeTool`): `listProjects` → `readProjectFile` →
`searchProjectText` → `getProjectGitDiffSummary` → `replaceProjectFileText`
→ `getProjectGitDiffSummary` → `runProjectShellCommand` → `gitRestorePaths`
→ `getProjectGitDiffSummary`, plus a patch sub-loop
(`validateProjectPatch` → `applyProjectPatchChecked` →
`getProjectGitDiffSummary` → `deleteProjectFiles` →
`getProjectGitDiffSummary`). The worktree returns to its clean baseline at
the end of both sub-loops. A dedicated `writeProjectFile` +
`startProjectShellJob` smoke (section 7i) exercises the two newly promoted
actions: create → read → overwrite-with-guard → read → delete, then start an
async `printf job-ok` job, poll `getRuntimeJobStatus` to completion, and
confirm output via `getRuntimeJobTail`.

The `/openapi.json` schema check in the E2E smoke now also asserts:
`additionalProperties=false` on every requestBody schema, unique operationIds,
mutation descriptions mention side effects + Bearer auth, and read-only
descriptions mention read-only / never writes.

## validate_patch (patch preflight / dry-run)

`validate_patch` is a read-only patch preflight tool for full-auto coding
agent loops. It is **not** a human approval UI. The intended loop is:

```text
read/search -> generate patch -> validate_patch -> applyProjectPatch
            -> git status/diff -> run tests -> fix failures -> repeat
```

Behavior:

- Dry-run only: runs `git apply --check -` and `git apply --stat -` through the
  owning `private-drop-agent`, passing the patch as `ShellRunRequest.stdin`.
  Never invokes the real `git apply` application mode and never falls back to
  `apply_patch`.
- Do not embed patch text in the shell command. `ShellRunRequest.command` is
  capped at 8 KiB; stdin is the protocol field intended for patch payloads.
- The server never reads the agent project filesystem directly — all checks
  are routed to the owning agent via the existing WebSocket/polling execution
  path.
- Input validation rejects empty patches, NUL bytes, and patches over
  `MAX_VALIDATE_PATCH_BYTES` (256 KiB) before project resolution.
- Absolute paths and `..` traversal are hard-rejected; sensitive filenames
  (`agent.toml`, `private-drop.env`, `.env`, `projects.d`, `.git`, `target`,
  `node_modules`) produce `warnings` rather than blocking the preflight.
- `deny_sensitive_paths=true` turns sensitive-path warnings into a structured
  policy block (`can_apply=false`, `policy_blocked=true`) without running git.
- Output: `can_apply` (bool), `affected_files` (array), `stat`, `stdout`,
  `stderr`, `warnings` (array).
- Exposed via MCP `tools/list` (25 tools as of Phase 4) and `POST /api/projects/validate_patch`.
- As of Phase 3, **also** a dedicated read-only GPT Action
  (`validateProjectPatch`); `/openapi.json` grew from 12 to 22 ops. Phase 4 adds
  `replace_in_file` / `write_project_file` as runtime-only tools. Phase 5
  promotes `replace_in_file` to a dedicated GPT Action
  (`replaceProjectFileText`), so the OpenAPI op count became 23 and MCP
  `tools/list` is 25. Phase 7 hardens the GPT Actions contract guard
  (every requestBody schema must have `additionalProperties=false`; every
  read-only action description must say "read-only" or "never writes"; every
  mutation action description must mention side effects + Bearer auth) and
  adds the full-auto coding loop E2E smoke — but did not change the op count
  (23) or MCP tool count (25). This phase promotes `write_project_file`
  (`writeProjectFile`) and `run_job` (`startProjectShellJob`) to dedicated
  GPT Actions, bringing the OpenAPI op count to 25; MCP `tools/list` stays
  25 (no new runtime tool).
- Capability: requires the agent `shell` capability (same as `apply_patch`,
  since the dry-run runs `git apply --check` via the agent shell path). Owner
  boundary checks are reused from `authorize_agent_tool`.
- Server and agent should be upgraded together for stdin-backed
  `validate_patch` / `apply_patch` behavior.

## Patch Application Agent Chain Hardening

The agent-backed patch application chain (`ApplyPatch` / `ApplyPatchChecked` /
`ValidatePatch`) was hardened so the patch payload and working directory are
never passed through the shell command string:

- The patch body always travels over `ShellRunRequest.stdin`. The `command`
  string is a fixed `git apply` invocation (`git apply --check -`,
  `git apply --check - && echo OK`, `git apply --stat -`, `git apply -`) and
  never contains patch content, an `echo <patch>` / `cat` splice, a heredoc,
  or a `cd <path> && ...` prefix.
- The working directory is supplied via the shell request `cwd` field (set to
  the owning agent's project path), never via `cd` in the command. The
  historical `echo '<patch>' | cd <path> && git apply --check -` shape is
  structurally impossible now.
- `validate_patch` stays read-only: it only ever enqueues `git apply --check`
  and `git apply --stat` (both dry-run) and never a bare mutating
  `git apply -`. It never modifies the worktree.
- `apply_patch` / `apply_patch_checked` route to the owning agent only.
  Server-configured (non-agent) projects are rejected by every patch tool, so
  the server never reads or writes the agent filesystem directly. The legacy
  server-local `apply_patch_local` path was removed for this reason.
- `apply_patch_checked` runs the preflight first and skips the apply step when
  the check fails (`can_apply=false`), so a non-applicable patch never mutates
  the worktree. `git apply` (without `--reject`) is itself atomic, so a failed
  apply does not partially apply.
- Patches larger than the agent shell command limit (`MAX_COMMAND_LEN` = 8000
  bytes) still validate and apply because they travel over stdin, not the
  command string. `validate_patch` caps preflight input at
  `MAX_VALIDATE_PATCH_BYTES` (256 KiB).
- `deny_sensitive_paths` semantics are unchanged: sensitive filenames warn by
  default and become a structured policy block (`can_apply=false`,
  `policy_blocked=true`) when `deny_sensitive_paths=true`.
- External API is unchanged: `/api/projects/apply_patch`,
  `/api/projects/apply_patch_checked`, `/api/projects/validate_patch` keep
  their schemas; OpenAPI operation count stays 23 and MCP `tools/list` stays
  25.

Unit tests pin these invariants (command never contains the patch marker;
patch equals `stdin`; `cwd` is the project path; check-failure skips apply;
`validate_patch` enqueues no mutating command; large patch over the limit
still validates/applies; server-configured projects rejected). E2E smoke adds
a large-patch `applyProjectPatchChecked` check and a check-failed worktree
immutability check over both transports.

## Documentation Map

Start with:

- `README.md`
- `docs/INDEX.md`
- `docs/GPT_ACTIONS.md`
- `docs/DEPLOYMENT.md`
- `docs/ROADMAP.md`
- `TODO.md`

Deprecated workflow, SSH, and legacy dashboard notes have been removed from the
active docs set. Use git history if historical removed-feature notes are needed.

## Next Likely Work

Prefer this order:

1. Deploy the current build and verify the real public endpoint with
   `scripts/smoke_deployment.sh`.
2. Run the real WebSocket agent under systemd/supervisor and confirm
   `runtime_status` keeps the agent `online` across idle periods. If it flips
   to `stale`, inspect reverse-proxy WebSocket upgrade/timeouts and agent logs
   before changing code.
3. Import `/openapi.json` into ChatGPT GPT Actions and verify the 25 operation
   schema works against the public deployment.
4. Test the optional real Codex CLI path with a low-risk prompt. Codex remains
   optional; most development work should still function through typed
   actions (`readProjectFile`, `getProjectGitStatus`, `getProjectGitDiff`,
   `applyProjectPatch`, `runProjectShellCommand`).
5. Keep the message envelope QUIC-compatible for a later QUIC transport
   (design only for now; do not implement QUIC unless explicitly requested).

## Priority verification after handoff

After reading this file, run the E2E smoke harness first to confirm the
runtime still runs end-to-end on a single host before making changes:

```bash
bash scripts/e2e_zero_config_ws.sh
```

This boots a real server + WebSocket agent with a stub `CODEX_BIN` (no real
Codex CLI, no ChatGPT) and exercises the GPT Actions + MCP surface. See
[docs/E2E_VALIDATION.md](E2E_VALIDATION.md) for details. If it fails, inspect
the printed server/agent log paths before touching code — the failure is often
a local environment issue, not a regression.

For an already-deployed instance, run the deployment smoke (no server/agent
started; verifies the public surface of a live deployment):

```bash
DROP_PUBLIC_URL="https://drop.example.com" \
DROP_TOKEN="<your-secret>" \
bash scripts/smoke_deployment.sh
```

It checks `GET /openapi.json`, `POST /api/runtime/status`,
`POST /api/projects/list`, `POST /mcp initialize`, and `POST /mcp tools/list`
using only `curl` + `python3` and never prints the token. See
[docs/DEPLOYMENT.md](DEPLOYMENT.md) for the full deployment guide (env vars,
agent config, reverse proxy, ChatGPT import, troubleshooting order).

Avoid broad refactors until real ChatGPT integration has been exercised.
