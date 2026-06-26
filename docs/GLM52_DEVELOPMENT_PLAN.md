# GLM 5.2 Development Plan

This document is the handoff plan for continuing Private Drop Runtime development with GLM 5.2.

## Current Baseline

Branch baseline:

- Current architecture commit when the initial runtime MVP closed:
  `3a0352e Harden local job lifecycle`
- Server binary: `private-drop`
- Agent binary: `private-drop-agent`
- Main integration surfaces:
  - GPT Actions: `GET /openapi.json`
  - MCP: `POST /mcp`
  - Runtime HTTP tools:
    - `POST /api/tools/list`
    - `POST /api/tools/call`
    - `POST /api/codex/run`
    - `POST /api/jobs/status`
    - `POST /api/jobs/log`
    - `POST /api/projects/list`
    - `POST /api/projects/read_file`
    - `POST /api/projects/git_status`
    - `POST /api/runtime/status`

Core files:

- `src/main.rs`: route wiring and shared state injection.
- `src/tool_runtime.rs`: shared execution layer for GPT Actions and MCP.
- `src/runtime_http.rs`: GPT Actions / REST wrapper around `ToolRuntime`.
- `src/mcp.rs`: minimal MCP JSON-RPC wrapper around `ToolRuntime`.
- `src/openapi.rs`: generated minimal GPT Actions OpenAPI schema.
- `src/shell_client.rs`: polling agent registry and job queue.
- `src/bin/private-drop-agent.rs`: polling execution agent.
- `src/action_sessions.rs` and `src/action_audit.rs`: action audit/event storage.

Verification already passing at this baseline:

```bash
cargo check
cargo test
```

Manual smoke already verified:

- `GET /openapi.json`
- `POST /api/tools/list`
- `POST /mcp` with `initialize`
- `POST /mcp` with `tools/list`

## Product Goal

Private Drop should become a self-hosted runtime that ChatGPT can use through both:

1. GPT Actions, via OpenAPI and Bearer auth.
2. MCP tools, via `/mcp`.

Both access layers must call the same backend tool runtime. Do not create separate business logic for GPT Actions and MCP.

The server-to-agent protocol is not fixed. It can remain polling for now, or later move to WebSocket/SSE if that meaningfully improves behavior.

## Non-Goals

Do not bring back the old file-drop product surface.

Avoid reintroducing:

- Message/channel/file-transfer API as a product feature.
- Web UI as the primary product.
- Desktop task orchestration.
- SSH executor.
- Old command request / goal workflow.
- Multiple legacy OpenAPI variants.

## Engineering Rules

- Keep the codebase centered on `ToolRuntime`.
- Every new capability must be exposed through `ToolRuntime` first.
- GPT Actions and MCP wrappers must stay thin.
- Prefer small, testable changes.
- Run `cargo fmt`, `cargo check`, and `cargo test` before reporting completion.
- Do not silently ignore security boundaries. Project paths must stay within configured roots.
- Do not expose broad arbitrary local filesystem access by default.
- Do not log secrets, tokens, prompts containing credentials, or full stdout/stderr in action audit summaries.

## Phase Status

Phases 1–11 are complete. This plan is retained as the historical handoff for
the GLM 5.2 implementation sequence. The authoritative current state lives in
`README.md`, `V2_SCOPE.md`, `TODO.md`, `docs/ROADMAP.md`, and the kept docs
under `docs/` (see `docs/INDEX.md`).

## Phase 1: Stabilize Runtime Tool Contracts

Objective: make the current runtime API reliable enough for real ChatGPT use.

Tasks:

1. Add focused unit tests for `ToolCall::from_tool_name`.
2. Add tests for `tool_specs()` shape:
   - all names are unique;
   - all schemas are objects;
   - required fields match deserialization expectations.
3. Add tests for OpenAPI operation IDs and schema references.
4. Add tests for MCP `tools/list` result shape without starting a real HTTP server if practical.
5. Normalize tool naming:
   - runtime names stay snake_case: `run_codex`, `job_status`;
   - GPT operation IDs stay camelCase: `runCodexTask`, `getRuntimeJobStatus`.
6. Add an explicit runtime version field in `tools/list` output or server info.

Acceptance criteria:

- `cargo test` passes.
- `POST /api/tools/list` returns a stable, documented schema.
- `/openapi.json` does not expose legacy routes.
- MCP `tools/list` and REST `tools/list` expose the same tool names.

## Phase 2: Make Local Jobs Robust

Objective: local async jobs should survive normal polling and be understandable after server restart.

Current issue:

- Agent jobs are tracked in memory by `ShellClientRegistry`.
- Local jobs write metadata under `.codex/jobs/<job_id>`, but the runtime in-memory map is lost after restart.

Tasks:

1. Implement local job recovery:
   - when `job_status` or `job_log` receives an unknown local-looking `job_id`, search configured projects for `.codex/jobs/<job_id>/metadata.json`;
   - verify the recovered job belongs to a configured project;
   - reject paths outside project roots.
2. Add a `kind` field to local metadata:
   - `shell`
   - `codex`
3. Add status normalization:
   - `queued`
   - `running`
   - `completed`
   - `failed`
   - `stopped`
   - `lost`
4. Add timeout enforcement for local jobs:
   - write `max_runtime_secs` into metadata;
   - detect over-time running jobs in `job_status`;
   - attempt process group termination when possible.
5. Add bounded log reads:
   - support `offset`;
   - support `tail_lines`;
   - never return unbounded logs.

Acceptance criteria:

- A local job can be created, queried, and logged.
- A local job can be queried after server restart.
- Large logs are bounded.
- Tests cover recovery and path safety.

## Phase 3: Harden `run_codex`

Objective: `run_codex` should be the safe default for ChatGPT-driven Codex CLI tasks.

Tasks:

1. Add a `CodexConfig` section in `Config`:
   - `CODEX_BIN`, default `codex`;
   - `CODEX_APPROVAL_MODE`, default `full-auto`;
   - `CODEX_DEFAULT_TIMEOUT_SECS`, default `3600`;
   - `CODEX_MAX_PROMPT_BYTES`, default `100000`;
   - `CODEX_ALLOWED_EXTRA_ARGS`, optional allowlist.
2. Stop accepting arbitrary `extra_args` unless explicitly allowed.
3. Add `run_codex` tests:
   - empty prompt rejected;
   - huge prompt rejected;
   - NUL bytes rejected;
   - command string is shell-escaped correctly;
   - metadata marks job kind as `codex`.
4. Add structured output fields for Codex jobs:
   - `job_id`;
   - `kind`;
   - `project`;
   - `status_url` or status endpoint hint;
   - `log_url` or log endpoint hint.
5. Decide whether `run_codex` should support sync mode. Default should remain async.

Acceptance criteria:

- GPT Actions can start Codex tasks without raw shell.
- Arbitrary unexpected CLI flags are not accepted by default.
- Tests cover command construction and rejection paths.

## Phase 4: Improve MCP Compatibility

Objective: `/mcp` should work with ChatGPT MCP tooling and common MCP clients.

Tasks:

1. Verify the expected MCP transport behavior for ChatGPT.
2. Add support for `notifications/initialized` as a no-response notification if required by clients.
3. Return MCP-compatible tool result content:
   - `content` array with text;
   - `structuredContent` for JSON-capable clients;
   - `isError` for failures.
4. Add optional GET metadata route for discovery if needed.
5. Add integration tests for:
   - `initialize`;
   - `tools/list`;
   - `tools/call` success;
   - `tools/call` failure;
   - Bearer auth failure.

Acceptance criteria:

- A basic MCP client can initialize and call `list_projects`.
- Tool errors are represented as MCP tool errors, not malformed HTTP errors.
- Auth behavior is clear and tested.

## Phase 5: Improve GPT Actions Schema

Objective: GPT Actions should see a small, stable, useful API surface.

Tasks:

1. Keep `/openapi.json` minimal:
   - `listRuntimeTools`;
   - `callRuntimeTool`;
   - `runCodexTask`;
   - `getRuntimeJobStatus`;
   - `getRuntimeJobLog`.
2. Consider adding dedicated GPT Actions for common tools:
   - `listProjects`;
   - `readProjectFile`;
   - `getProjectGitStatus`;
   - only if ChatGPT performs poorly through generic `callRuntimeTool`.
3. Add OpenAPI tests:
   - all operation IDs match expected set;
   - all `$ref` targets exist;
   - no legacy paths exist;
   - Bearer auth exists.
4. Make server URL configurable through `DROP_PUBLIC_URL`.
5. Add example GPT instructions in docs.

Acceptance criteria:

- The schema imports cleanly into GPT Actions.
- The schema does not expose dangerous raw shell as the primary path unless intentionally kept.
- `runCodexTask` is the recommended high-level action.

## Phase 6: Agent Protocol Cleanup

Objective: make remote execution reliable without deciding prematurely between polling and WebSocket.

Tasks:

1. Document the current polling protocol:
   - register;
   - poll;
   - result;
   - job update.
2. Add an `agent_protocol_version` field during registration.
3. Add server-side capability checks for every agent-backed tool:
   - shell;
   - file read/write;
   - async jobs;
   - git.
4. Add owner and permission checks to all new runtime paths.
5. Add structured agent errors:
   - client offline;
   - unsupported capability;
   - request timeout;
   - policy denied.
6. Later option: add WebSocket transport as a second transport, not a rewrite of runtime tools.

Acceptance criteria:

- Agent-backed `run_shell`, `run_job`, and `read_file` fail clearly when capability is missing.
- The protocol is documented.
- Polling agent remains usable.

## Phase 7: Observability And Admin

Objective: make real deployments debuggable.

Tasks:

1. Add `GET /api/runtime/status` or `POST /api/tools/call` tool `runtime_status`.
2. Include:
   - server version;
   - configured project count;
   - connected agent count;
   - active job count;
   - auth mode;
   - public URL.
3. Add action audit coverage for new runtime HTTP endpoints.
4. Add docs for querying recent action sessions if an API exists. If not, either implement a minimal read-only endpoint or remove stale docs.
5. Add deployment notes for reverse proxy and HTTPS.

Acceptance criteria:

- Operator can quickly see whether GPT/MCP should work.
- Audit does not leak secrets.
- Deployment docs match actual routes.

## Phase 8: Documentation Cleanup

Objective: remove confusing stale docs.

Status: in progress.

Tasks:

1. Update or delete docs that still describe legacy endpoints:
   - old file/message APIs;
   - old `/codex-openapi-compact.json`;
   - old desktop tasks;
   - old command request workflow.
2. Keep docs focused on:
   - runtime tools;
   - GPT Actions;
   - MCP;
   - agent setup;
   - job lifecycle;
   - security model.
3. Add a short "How to connect ChatGPT" guide:
   - GPT Actions import `/openapi.json`;
   - MCP connect to `/mcp`;
   - Bearer token setup;
   - recommended first calls.

Acceptance criteria:

- README and docs do not contradict actual routes.
- A new developer can run and test the server from docs alone.

## Suggested Implementation Order

Recommended order for GLM:

1. Phase 1: runtime contract tests.
2. Phase 2: local job recovery.
3. Phase 3: `run_codex` hardening.
4. Phase 5: GPT Actions schema tests and docs.
5. Phase 4: MCP compatibility tests.
6. Phase 6: agent capability hardening.
7. Phase 7 and 8: observability and documentation cleanup.

Do not start with a large refactor. The architecture already has the correct center: `ToolRuntime`.
