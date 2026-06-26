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

### Deprecated (not active features)

The old file-drop / message / channel / Web UI product surface, desktop task
orchestration, SSH executor, `command_request` / goal workflow, and
`project_workflow` / `project_doctor` / `project_hook` routes have been
removed. They are intentionally not tracked as future work.

## Backlog

- [ ] Finish zero-config agent runtime cleanup: remove remaining docs/tests that
      present `PROJECTS_CONFIG` as the normal runtime project source, and add
      agent-registered happy-path tests for read/git/Codex routing.
- [ ] WebSocket agent transport as the primary long-lived transport, reusing the
      existing registry/queue/job_update semantics.
- [ ] Keep polling as fallback transport for restricted networks and old agents.
- [ ] QUIC transport design after the WebSocket message envelope is stable.
- [ ] Real ChatGPT GPT Actions import test (import `/openapi.json` and run the
      recommended call flow against a live deployment with an agent-registered
      project)
- [ ] Real ChatGPT MCP connection test (connect a ChatGPT MCP client to `/mcp`)
- [ ] Deployment hardening: reverse proxy / HTTPS guide, systemd notes
- [ ] Rate limiting
- [ ] Audit retention / cleanup policy for long-running deployments
- [ ] Persistent agent job queue (survive server restart for in-flight agent
      jobs; currently in-memory)
- [ ] Docs cleanup ongoing (keep README + docs aligned with `src/main.rs` and
      `src/openapi.rs` as the runtime evolves)
