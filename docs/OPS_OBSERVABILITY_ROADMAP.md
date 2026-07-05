# Ops Observability Roadmap

Status: docs-only RFC. This document proposes operator workflows, CLI/API
shapes, and engineering slices for deployment and agent fleet observability. It
does not change runtime behavior, agent transport, OpenAPI, or dependencies.

## Scope

This roadmap focuses on self-hosted operations:

- deployment health and smoke validation,
- agent fleet status across QUIC, WebSocket, and polling,
- runtime/project/job health summaries,
- safe troubleshooting output for GPT Actions, MCP, and CLI operators.

The intended operator should be able to answer four questions quickly:

1. Is the deployed server reachable and on the expected build?
2. Which agents are online, stale, offline, or on fallback transport?
3. Which projects are actually usable now?
4. Are there active, stuck, or terminal-pending jobs that block a clean handoff?

## Current Operator Surface

WebCodex already has useful observability building blocks. The gap is mostly
packaging them into repeatable, bounded operator workflows.

### Runtime Status

`runtime_status` is the central read-only health source. Current docs describe
runtime summaries that can expose:

- service identity such as `service=webcodex`,
- public URL and deployment-facing runtime metadata,
- build version, build commit, and dirty state in compact startup summaries,
- `tools.count`,
- `jobs.active_count`,
- `agents.summary`,
- `projects.effective`, `projects.agent_registered`, and
  `projects.server_static`.

`start_coding_task(include_runtime_status=true, compact_startup=true)` already
offers a bounded model-facing summary. It intentionally omits tool names, full
agent policy, allowed roots, shell profile internals, command text,
stdout/stderr, env values, tokens, secrets, and full config values.

### Agent Inventory

`list_agents` / `listAgents` already provide the base agent view. Current
transport docs describe compact fields suitable for operators:

- fleet counts for online, stale, and offline agents,
- per-client `client_id`,
- `status`,
- actual `transport` label: `quic`, `websocket`, or `polling`,
- `last_seen_age_secs`,
- `projects_count`,
- `pending_requests`,
- `active_jobs`,
- redacted policy and shell profile summaries.

Foreground agent logs also expose the `auto` transport decision path, including
QUIC attempts, WebSocket fallback, polling fallback, and final
`actual_transport`. Those logs are useful for one-agent setup debugging, but
they are not yet packaged as a multi-agent fleet view.

### Project Inventory

`list_projects` and `runtime_status.projects` already separate project sources:

- `projects.server_static`,
- `projects.agent_registered`,
- `projects.effective`.

This matters for agent-only deployments. A missing server-side `projects.toml`
is not a runtime failure when online agents register projects and
`projects.effective.status = "ok"`. Operators should treat
`projects.effective` as the health source and explain
`projects.server_static.status = "not_configured"` as informational in that
shape.

`list_projects` also reports `recommended_for_smoke`, project capabilities, and
shell profile resolution fields that can drive safe smoke target selection.

### Jobs And Sessions

Current workflow docs already separate broad active job counts from closeout
decision fields:

- `jobs.active_count`,
- `blocking_active_count`,
- `nonblocking_active_count`,
- `running_count`,
- `stop_requested_count`,
- `terminal_pending_count`,
- `jobs_terminal_pending`.

`finish_coding_task` and `session_handoff_summary` return bounded job metadata
without raw stdout/stderr, tails, excerpts, or command text. This is a good
foundation for operator-safe job inventory and stuck-job diagnostics.

### Transport And Deployment Docs

Existing docs cover:

- systemd server and agent services,
- manual foreground/background startup,
- no-service agent mode for temporary clients, containers, smoke tests, or
  hosts without systemd,
- public HTTPS and reverse proxy setup,
- QUIC listener setup and validation,
- QUIC/WebSocket/polling fallback behavior,
- GPT Actions and MCP setup,
- smoke checks after deployment.

The docs already state that `runtime_status.quic` is non-secret and that
`runtime_status` / `listAgents` report the actual connected transport, not only
the preferred setting.

## Current Gaps

The current surface is powerful but still too low-level for common operator
questions.

### Version Skew

Operators need a direct way to compare:

- server binary version and commit,
- agent binary version and commit,
- dirty state,
- protocol version,
- schema/tool registry version if it becomes explicit later.

Today compact startup can expose build commit for the runtime summary, and QUIC
agents report `agent_protocol_version=quic-v1`, but there is no first-class
fleet skew verdict such as "server commit differs from 3 of 8 agents".

### Agent Stale Or Offline Diagnosis

`listAgents` exposes online/stale/offline status and last-seen age, but an
operator still has to connect causes manually:

- service not running,
- wrong token,
- owner mismatch,
- clock/network/proxy issue,
- QUIC blocked with WebSocket or polling fallback,
- agent registered but no projects usable.

The future operator view should provide cause-oriented next checks without
printing config files, tokens, or raw logs.

### Transport Status Troubleshooting

QUIC, WebSocket, and polling each have distinct failure modes:

- QUIC listener disabled or bind failure,
- UDP firewall or security group block,
- certificate SAN mismatch,
- ALPN mismatch,
- WebSocket proxy upgrade missing,
- polling fallback due to constrained networks,
- foreground polling terminal HTTP failures.

The docs contain the pieces, but there is no unified transport health table per
agent and no CLI command that summarizes "preferred, actual, fallback reason,
last error class".

### Multi-Agent Fleet View

The current APIs can list agents, but operators need a compact fleet summary
designed for scanning:

- total agents by status,
- agents by transport,
- agents by version/commit,
- agents with zero projects,
- agents with active jobs,
- agents stale beyond a threshold,
- agents whose effective projects are not usable.

This should be available in plain CLI output and bounded JSON.

### Job Inventory And Stuck Jobs

Current session closeout fields are good for a single workflow. Operators still
need a deployment-level job inventory:

- running jobs by project,
- jobs older than a threshold,
- stop requested but still `terminal_pending`,
- terminal jobs that need final status collection,
- jobs with no owning session or missing project when that can happen.

Raw logs should not be displayed by default. A job inventory can show ids,
project, age, status, kind, and safe timestamps first, then require an explicit
job log command for bounded excerpts.

### Smoke Workspace Health

Docs recommend disposable smoke projects and `recommended_for_smoke`, but the
operator workflow should become explicit:

- choose a project intentionally,
- prove workspace clean before write/smoke checks,
- verify no active blocking jobs,
- run a minimal read-only check first,
- run write/replace/validate only on an explicit smoke project,
- leave the workspace clean after smoke.

### Release And Deployment Checklist

Existing deployment and release docs include smoke checks, but a focused
operator checklist should tie together:

- binary version/commit expectations,
- server status,
- agent status,
- runtime status,
- GPT Action schema refresh only when needed,
- OpenAPI schema drift checks,
- smoke workspace checks,
- rollback decision points.

### GPT Action Schema Refresh

Docs already say to refresh `/openapi.json` after deploying a build that changes
tool schemas. The missing piece is a checklist that helps operators decide when
refresh is needed:

- OpenAPI operation count changed,
- dedicated action path changed,
- accepted flattened args changed for model-facing tools,
- runtime-only tool changed but remains behind `callRuntimeTool`,
- deployment changed without schema changes.

The checklist should avoid unnecessary GPT builder churn while still catching
schema drift after deploy.

### Log Redaction And Output Bounds

The current runtime summaries intentionally avoid tokens, env values, raw
command text, stdout/stderr, and full config values. Future ops commands need to
preserve that by default:

- no token, secret, credential, or env value display,
- no raw logs unless explicitly requested,
- bounded output with counts and truncation markers,
- redacted policy summaries only,
- safe error classes instead of dumping upstream HTML or full response bodies.

### `projects.toml` Versus Agent-Registered-Only Deployments

Operators can misread `server_static.status = "not_configured"` as a failure.
The correct interpretation is:

- if `projects.effective.status = "ok"` and agent-registered projects are
  present, the deployment can be healthy without server-side `projects.toml`,
- if `projects.effective.count = 0`, the operator should inspect agent
  registration, ownership, project policy, and agent online status.

This distinction should be repeated in ops status output and docs.

## Recommended Roadmap

### P1: Operator Basics

P1 should make the existing observability surface easier to use without adding
new deployment complexity.

- `webcodex ops status`: read-only deployment health command that summarizes
  runtime status, project source health, agent summary, job counts, build
  metadata, and transport listener status.
- Deployment smoke checklist: a short, repeatable sequence for post-deploy and
  post-agent-enrollment validation.
- Agent/server version skew field design: specify the stable fields before
  implementation, including server commit, agent commit, dirty state, protocol
  version, and display rules for unknown older agents.
- Runtime status and list agents operator view: define bounded CLI and JSON
  output sourced from existing `runtime_status` and `list_agents`.
- Safe smoke workspace check: document and later implement a read-only first
  pass, explicit smoke project requirement for writes, clean workspace check,
  and no active blocking jobs.

### P2: Fleet And Release Workflow

P2 should improve multi-agent operations and release validation after the P1
read-only view is stable.

- Lightweight web ops dashboard: read-only browser view for status, agents,
  projects, jobs, and deployment checklist state.
- Multi-agent fleet status: group agents by status, transport, version/commit,
  owner, project count, and active job count.
- Job inventory and stale jobs page: show bounded job metadata, age buckets,
  `terminal_pending`, and stop-requested jobs without raw logs by default.
- Release bundle validation: compare expected artifact versions, server binary,
  agent binary, CLI binary, OpenAPI operation count, and docs checklist before
  release or deploy.

### P3: Long-Range Ops Platform

P3 is useful after the CLI and dashboard have stable semantics.

- Alerting: stale/offline agent alerts, transport fallback alerts, stuck job
  alerts, schema drift alerts, and smoke failure alerts.
- Metrics export: low-cardinality counters and gauges for agents, transports,
  projects, jobs, requests, and smoke results.
- Multi-tenant admin console: owner-scoped fleet operations, tenant isolation
  views, and admin-only actions once product boundaries require them.
- Audit visualization: session, permission, job, and deployment event timelines
  with redaction-preserving drilldowns.

## Proposed CLI Shape

These commands are proposals only. They should initially be implemented as
read-only or explicitly bounded checks.

```text
webcodex ops status
webcodex ops status --json
webcodex ops status --server-url https://your-domain.example --user-token-file PATH

webcodex ops agents
webcodex ops agents --status stale
webcodex ops agents --transport quic
webcodex ops agents --json

webcodex ops smoke --project agent:special:webcodex-smoke
webcodex ops smoke --project agent:special:webcodex-smoke --read-only
webcodex ops smoke --project agent:special:webcodex-smoke --write --confirm

webcodex ops doctor
webcodex ops doctor --quic
webcodex ops doctor --gpt-action-schema
webcodex ops doctor --project agent:special:webcodex-smoke
```

Recommended behavior:

- `ops status` is read-only and should be safe for routine use.
- `ops agents` is read-only and should default to compact fleet output.
- `ops smoke` requires `--project`; any write smoke requires an explicit smoke
  project and confirmation.
- `ops doctor` can run targeted diagnostics but must avoid destructive cleanup.
- JSON output should be stable enough for scripts and should include explicit
  `status`, `severity`, `checks`, and `suggested_next_actions` fields.

## Proposed API Shape

Prefer composing existing APIs first. Add dedicated endpoints only when CLI and
dashboard needs cannot be met with bounded calls to existing runtime tools.

Possible future shapes:

```text
GET /api/ops/status
GET /api/ops/agents
GET /api/ops/jobs
POST /api/ops/smoke
GET /api/ops/openapi-drift
```

Suggested response conventions:

- top-level `status`: `ok`, `warn`, or `fail`,
- top-level `service`: `webcodex`,
- `generated_at` timestamp,
- bounded `summary` object,
- bounded `checks` array with `id`, `status`, `severity`, `reason`, and
  `suggested_next_actions`,
- no raw logs, tokens, env values, full config files, command text,
  stdout/stderr, or `Authorization` headers,
- explicit truncation fields when output is bounded.

The first implementation should probably avoid new OpenAPI GPT Action
operations. Keep operator-only APIs out of GPT Actions unless there is a strong
reason to expose a dedicated action. Runtime-only access through CLI or
authenticated REST is enough for the initial ops workflow.

## Safety Principles

All ops observability work should preserve these rules:

- Do not output tokens, secrets, credentials, env values, or authorization
  headers.
- Do not output raw logs by default.
- Keep output bounded and mark truncation explicitly.
- Require an explicit `project` for write or smoke checks.
- Prefer read-only checks first.
- Do not perform destructive cleanup without explicit confirmation.
- Treat `runtime_status` as read-only.
- Show redacted policy summaries, never full `agent.toml`, full env snapshots,
  shell profile init scripts, or agent token fields.
- Avoid printing command text, stdout/stderr, and log excerpts in summary
  commands.
- Use safe error classes and suggested next actions instead of dumping upstream
  proxy pages or full response bodies.

## Recommended Smoke Matrix

| Smoke | Purpose | Default safety |
| --- | --- | --- |
| Local MCP direct smoke | Validate the MCP endpoint and runtime dispatch without ChatGPT UI. | Use a PAT or OAuth token; no secrets printed. |
| GPT Action generic `callRuntimeTool` smoke | Validate deployed `/openapi.json`, auth, and runtime-only tool access. | Start with `getRuntimeStatus` and `tool_manifest`; mutate only a smoke project. |
| Agent foreground smoke | Inspect one agent's registration and transport fallback logs. | Run with redacted logs; do not print token/config values. |
| Safe smoke workspace clean | Prove the target smoke project starts and ends clean. | Require explicit `--project`; fail if workspace is dirty. |
| No active jobs | Ensure handoff or deploy smoke is not blocked by running jobs. | Check counts first; no raw job logs by default. |
| Schema refreshed only when needed | Avoid stale GPT Action schemas without unnecessary refreshes. | Refresh only after model-facing OpenAPI/schema changes. |

Minimum deploy smoke sequence:

1. Confirm server service is running and HTTPS is reachable.
2. Call `runtime_status` and verify `service=webcodex`, expected public URL,
   build metadata, and project health.
3. Call `list_agents` and verify at least one required agent is online.
4. Call `list_projects` and choose a project with `recommended_for_smoke=true`
   when available.
5. Verify no blocking active jobs before write smoke.
6. Run a read-only project check.
7. Run write/validate smoke only on an explicit disposable smoke project.
8. Confirm the smoke workspace is clean.
9. Refresh GPT Action schema only if the deployed OpenAPI or model-facing tool
   schema changed.

## GPT Action And OpenAPI Checklist

Use this checklist after deploy:

- Fetch deployed `/openapi.json` from the public URL.
- Confirm the operation count remains within the GPT Actions limit.
- Confirm expected dedicated operations are present.
- Confirm runtime-only tools remain behind `callRuntimeTool` unless they were
  deliberately promoted.
- Re-import the GPT Action schema only when model-facing OpenAPI shape changed.
- Test `getRuntimeStatus`.
- Test `callRuntimeTool` with `tool_manifest`.
- Test a read-only project call against a safe project.
- Defer mutation until agent, project, and job health checks pass.

OpenAPI drift after deploy should be treated as an operator warning when the
server build changed but the GPT Action still uses an older schema. It should be
a failure only when the old schema blocks required calls or exposes an
operation shape that no longer matches the deployed server.

## First Engineering Slices

These are intentionally small, testable, and low risk.

1. Document the exact `ops status` output contract.
   - Input: server URL and user token source.
   - Output: read-only compact summary from `runtime_status`.
   - Test: fixture JSON renders `ok`, `warn`, and `fail` statuses without
     secrets.

2. Add a CLI help-only skeleton for `webcodex ops status`.
   - No network behavior in the first slice.
   - Test: CLI help smoke asserts command appears and describes read-only
     behavior.

3. Add an `OpsStatusSummary` mapper around existing `runtime_status` output.
   - No new server fields required.
   - Test: unit fixtures for missing `projects.toml`, agent-only healthy
     deployment, no agents online, active jobs present, and QUIC listener error.

4. Add `webcodex ops agents` as a read-only wrapper over `list_agents`.
   - Output compact table plus JSON mode.
   - Test: stale/offline/online grouping and no secret-like values in rendered
     output.

5. Specify version skew fields before implementing them.
   - Fields: `server.version`, `server.commit`, `server.dirty`,
     `agent.version`, `agent.commit`, `agent.dirty`,
     `agent_protocol_version`.
   - Test: older agents with unknown fields render as `unknown` and produce
     `warn`, not parser failure.

6. Add a deployment smoke checklist doc block to the CLI output or docs.
   - No runtime changes.
   - Test: markdown/link validation and manual review against deployment docs.

7. Add safe smoke workspace preflight design.
   - Require explicit project.
   - Read-only default.
   - Write smoke requires `--write --confirm`.
   - Test: argument parser rejects write smoke without project and confirm.

8. Add OpenAPI drift check design using deployed `/openapi.json`.
   - Start as docs or a local helper that reports operation count and expected
     operation ids.
   - Test: fixture schemas for unchanged, missing operation, and over-limit
     operation count.

9. Add job inventory response design.
   - Summaries only: job id, project, status, kind, age, timestamps.
   - No stdout/stderr, command text, or log excerpts.
   - Test: redaction and terminal-pending classification.

10. Add release bundle validation design.
    - Compare expected server/agent/CLI artifact metadata.
    - Keep it local and read-only.
    - Test: fixture manifest mismatch produces a bounded warning.

## Non-Goals For This RFC

- No runtime implementation.
- No OpenAPI changes.
- No agent transport changes.
- No dashboard implementation.
- No metrics exporter implementation.
- No alerting implementation.
- No destructive cleanup workflow.
- No broad test-suite run requirement.
