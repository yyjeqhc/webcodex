# TODO

## Direction: Self-hosted Tool Runtime for ChatGPT

See [V2_SCOPE.md](V2_SCOPE.md) for the full scope. The runtime exposes local
capabilities through a single `ToolRuntime` consumed by both GPT Actions
(`/openapi.json`) and MCP (`/mcp`).

## Done

- [x] Shared `ToolRuntime` as the single execution layer for GPT Actions + MCP
- [x] GPT Actions OpenAPI schema (`/openapi.json`, POST-only, Bearer auth)
- [x] MCP over HTTP (`/mcp`): `initialize`, `ping`, `tools/list`, `tools/call`,
      `notifications/initialized`, GET discovery
- [x] Codex CLI async jobs (`run_codex`) with prompt validation and
      `CODEX_ALLOWED_EXTRA_ARGS` allowlist
- [x] Local job recovery from `.codex/jobs/<id>/metadata.json` after restart
- [x] Bounded job logs with `offset` / `tail_lines`
- [x] Agent protocol cleanup: `agent_protocol_version`, capability checks,
      owner boundary, structured agent errors
- [x] `runtime_status` observability tool + `POST /api/runtime/status` GPT Action
- [x] Dedicated GPT Actions: `listProjects`, `readProjectFile`,
      `getProjectGitStatus`
- [x] Documentation cleanup (Phase 8): deprecated legacy docs, aligned
      V2_SCOPE / TODO / README with the real API surface
- [x] Read-only Audit API (Phase 10): `POST /api/audit/sessions`,
      `/api/audit/session`, `/api/audit/stats` — admin/debug only, not a GPT
      Action, with strict read-time secret sanitization. See
      [docs/AUDIT_API.md](docs/AUDIT_API.md).
- [x] Local job process termination on timeout/stop (Phase 11): current jobs
      record `process_group_id`; over-time running jobs are marked `lost` and
      their process group is terminated when possible; explicit stop is wired to
      `POST /api/jobs/stop` but not exposed as a GPT Action.
- [x] Initial zero-config runtime project switch: runtime project discovery now
      starts from agent-registered project summaries using ids like
      `agent:<client_id>:<project_id>`; server-side project config is no longer
      the intended runtime project source.
- [x] WebSocket agent transport as the primary long-lived transport (Phase 13),
      reusing the existing registry/queue/job_update semantics. Hardened in
      Phase 14 (per-client pending queue cap, conservative `lost`
      reconciliation on disconnect, `owner`/auth binding at registration).
      Polling remains the fallback transport.
- [x] Local E2E validation harness (Phase 15): `scripts/e2e_zero_config_ws.sh`
      boots a real server + WebSocket agent with a stub Codex CLI and exercises
      the GPT Actions + MCP surface. See [docs/E2E_VALIDATION.md](docs/E2E_VALIDATION.md).
- [x] Deployment hardening (Phase 16): deployment guide (`docs/DEPLOYMENT.md`),
      systemd + env samples, nginx reverse proxy sample, and a deployment smoke
      script (`scripts/smoke_deployment.sh`). The server is a zero-project-
      config relay; WebSocket is preferred, polling is fallback, QUIC is not
      implemented. See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).
- [x] MCP App console Phase A — read-only backend tools (Phase A):
      `list_project_files`, `search_project_text`, `git_diff_summary`,
      `list_jobs`, and `job_tail` added to `ToolRuntime` and exposed via MCP
      `tools/list` + thin REST wrappers (`/api/projects/list_files`,
      `/api/projects/search_text`, `/api/projects/git_diff_summary`,
      `/api/jobs/list`, `/api/jobs/tail`). All are bounded and agent-backed;
      `/openapi.json` stays at 12 GPT Actions. See
      [docs/MCP_APP_CONSOLE_PLAN.md](docs/MCP_APP_CONSOLE_PLAN.md).
- [x] MCP App console Phase B — read-only runtime/agent status console: a
      self-contained `/console` page (public HTML/JS/CSS embedded in the
      binary) that renders `POST /api/runtime/status` — runtime stats plus a
      per-agent table (client_id, owner, status, transport, connected,
      protocol, last_seen, pending_requests, projects_count). WebSocket agents
      that flip `online` -> `stale` are visually obvious; websocket/polling are
      distinguishable at a glance. `runtime_status` gained `agents.stale_count`
      and per-agent `last_seen` (minimal, tested). Bearer token lives only in
      `localStorage`; no tokens/secrets rendered in the DOM. `/openapi.json`
      stays at 12 ops; the console route is explicitly excluded from the GPT
      Actions schema. See [docs/MCP_APP_CONSOLE_PLAN.md](docs/MCP_APP_CONSOLE_PLAN.md).
- [x] `validate_patch` patch preflight / dry-run (Phase D backend): a
      read-only `ToolCall::ValidatePatch` that runs `git apply --check` and
      `git apply --stat` through the owning agent without modifying the
      worktree. Returns `can_apply`, `affected_files`, `stat`, `stdout`,
      `stderr`, `warnings`. Input validation rejects empty/NUL/oversized
      patches (256 KiB cap); sensitive filenames warn; absolute paths and `..`
      traversal are hard-rejected. Exposed via MCP `tools/list` (23 tools) and
      `POST /api/projects/validate_patch`; **not** a GPT Action
      (`/openapi.json` stays at 12 ops). Designed for full-auto coding agent
      loops, not human approval. Patch payloads use `ShellRunRequest.stdin`,
      not shell command embedding. Local E2E passes 44/44 over both transports;
      `cargo test` passes 401 main + 23 agent tests. `apply_patch_checked`,
      `delete_project_files`, `git_restore_paths`, and `discard_untracked`
      are runtime/MCP-only cleanup tools. See
      [docs/MCP_APP_CONSOLE_PLAN.md](docs/MCP_APP_CONSOLE_PLAN.md).
- [x] Harden generic `callRuntimeTool` / `/api/tools/call` (Phase 2):
      supports omitted/null `params`, `arguments` alias with `params`
      precedence, richer unknown-tool and field errors, and enhanced
      `/api/tools/list` output (`tools`, `names`, `count`, `categories`,
      `recommended_flows`). `/openapi.json` remains 12 ops; MCP tools remain
      23. Local E2E passes 53/53 over both transports; `cargo test` passes
      430 main + 23 agent tests.
- [x] Promote core runtime tools to dedicated GPT Actions (Phase 3): the
      OpenAPI surface grew from 12 to 22 operations so a custom GPT can drive
      the full core coding loop without `callRuntimeTool`. New dedicated
      actions: `validateProjectPatch`, `applyProjectPatchChecked`,
      `getProjectGitDiffSummary`, `listProjectFiles`, `searchProjectText`,
      `listRuntimeJobs`, `getRuntimeJobTail`, `deleteProjectFiles`,
      `gitRestorePaths`, `discardUntrackedFiles`. New REST wrappers added for
      `apply_patch_checked`, `delete_files`, `git_restore_paths`, and
      `discard_untracked`; the other six reused existing REST endpoints and
      only gained OpenAPI schemas. All remain thin wrappers dispatching to
      `ToolRuntime`; no business logic duplicated. MCP tools remain 23.
      `callRuntimeTool` retained as advanced escape hatch. Mutation actions
      describe side effects + Bearer auth + agent shell capability;
      read-only actions marked read-only. Local E2E passes 61/61 over both
      transports; `cargo test` passes 433 main + 23 agent tests.
- [x] Structured file-edit runtime tools (Phase 4): added `replace_in_file`
      and `write_project_file` as runtime/MCP tools. Both run a FIXED python3
      helper on the owning agent (`old`/`new`/`content` travel over stdin as
      JSON; the command string is a compile-time constant — no caller content
      is interpolated, so there is no shell-injection surface). The server
      never reads/writes the agent fs directly. `replace_in_file` replaces a
      unique substring and refuses to write when `old` is missing or ambiguous
      (`allow_multiple` + `expected_replacements` for multi-replace);
      `write_project_file` creates files and overwrites only with an
      `expected_sha256` / `expected_content_prefix` guard. Sensitive paths
      (`agent.toml`, `private-drop.env`, `.env`, `projects.d`, `.git`,
      `target`, `node_modules`) are hard-rejected; absolute/`..`/NUL rejected.
      New runtime-only REST wrappers `POST /api/projects/replace_in_file` and
      `POST /api/projects/write_file` (NOT dedicated GPT Actions; listed in the
      OpenAPI forbidden-paths guard). OpenAPI op count stays 22; MCP
      `tools/list` grows from 23 to 25. Capability: requires agent `shell`.
      Local E2E adds a Phase 4 probe smoke (write → replace → read → delete)
      via `callRuntimeTool`; `cargo test` passes 466 main + 23 agent tests.
- [x] Promote `replace_in_file` to a dedicated GPT Action (Phase 5): the safer
      structured text-replace primitive is now a dedicated GPT Action
      (`replaceProjectFileText`, `POST /api/projects/replace_in_file`) so GPT
      can use it at high frequency without `callRuntimeTool`. It reuses the
      existing REST wrapper `projects_replace_in_file` and
      `ToolCall::ReplaceInFile` — no business logic duplicated. `write_file`
      stays runtime/MCP-only (whole-file overwrite is riskier). OpenAPI op
      count grew from 22 to 23 (still <= 30); `/api/projects/replace_in_file`
      was removed from the forbidden-paths guard while
      `/api/projects/write_file` remains forbidden. MCP `tools/list` stays 25.
      Tests updated (op set, count 23, mutation-description coverage,
      forbidden-path guard, write_file stays out). E2E adds a dedicated-action
      smoke (write probe → replaceProjectFileText → read → missing-old fail →
      cleanup). Local E2E passes 78/78 over both transports; `cargo test`
      passes 466 main + 23 agent tests.

### Deprecated (not active features)

The old file-drop / message / channel / Web UI product surface, desktop task
orchestration, SSH executor, `command_request` / goal workflow, and
`project_workflow` / `project_doctor` / `project_hook` routes have been
removed. They are intentionally not tracked as future work.

## Backlog

- [ ] MCP App console and approval surface: visual runtime/agent status,
      project browser, git diff viewer, patch validation/approval, command
      panel, and job/log viewer. GPT Actions remain the stable machine
      interface; the app is for human observation and approval. See
      [docs/MCP_APP_CONSOLE_PLAN.md](docs/MCP_APP_CONSOLE_PLAN.md).
- [ ] Finish zero-config agent runtime cleanup: remove remaining docs/tests that
      present `PROJECTS_CONFIG` as the normal runtime project source, and add
      agent-registered happy-path tests for read/git/Codex routing.
- [ ] QUIC transport design after the WebSocket message envelope is stable.
- [ ] Real ChatGPT GPT Actions import test (import `/openapi.json` and run the
      recommended call flow against a live deployment with an agent-registered
      project)
- [ ] Real ChatGPT MCP connection test (connect a ChatGPT MCP client to `/mcp`)
- [ ] Rate limiting
- [ ] Audit retention / cleanup policy for long-running deployments
- [ ] Persistent agent job queue (survive server restart for in-flight agent
      jobs; currently in-memory)
- [ ] Docs cleanup ongoing (keep README + docs aligned with `src/main.rs` and
      `src/openapi.rs` as the runtime evolves)
