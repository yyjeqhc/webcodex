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
`ToolRuntime`, also exposed via MCP `tools/list`. Not GPT Actions —
`/openapi.json` stays at 12 ops):

- `POST /api/projects/list_files`
- `POST /api/projects/search_text`
- `POST /api/projects/git_diff_summary`
- `POST /api/jobs/list`
- `POST /api/jobs/tail`

Runtime/MCP-only patch and cleanup tools (not GPT Actions yet):

- `apply_patch_checked`: validates with `validate_patch`, applies only when
  `can_apply=true`, then returns `git_diff_summary`.
- `delete_project_files`: deletes selected project-relative files only.
- `git_restore_paths`: restores selected tracked paths with `git restore --`.
- `discard_untracked`: removes selected untracked files with `git clean -f --`.

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

Expected current result:

- `cargo check`: 0 warnings.
- `cargo check --tests`: 0 warnings.
- `cargo test`: main binary 430 tests passing, agent binary 23 tests passing.

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

- `bash scripts/e2e_zero_config_ws.sh`: 53 passed / 0 failed.
- `E2E_TRANSPORT=polling bash scripts/e2e_zero_config_ws.sh`: 53 passed / 0
  failed.

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
- Exposed via MCP `tools/list` (23 tools) and `POST /api/projects/validate_patch`.
- **Not** a GPT Action: `/openapi.json` stays at 12 ops. The route is in the
  `LEGACY_FORBIDDEN_PATHS` guard so it can never leak into the GPT Actions
  schema.
- Capability: requires the agent `shell` capability (same as `apply_patch`,
  since the dry-run runs `git apply --check` via the agent shell path). Owner
  boundary checks are reused from `authorize_agent_tool`.
- Server and agent should be upgraded together for stdin-backed
  `validate_patch` / `apply_patch` behavior.

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
3. Import `/openapi.json` into ChatGPT GPT Actions and verify the 12 operation
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
