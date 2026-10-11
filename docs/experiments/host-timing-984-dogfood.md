# Host timing dogfood — controlled comparison for #984 Phase 3

**Status:** reproducible offline reporting, not a performance result, recommendation, or new Server telemetry. The operator must separately establish real Host configuration, exact task correctness and final Job outcome. The new report reads existing ActionAudit; it does not inspect request/response payloads, logs, model content or credentials.

## Hypothesis and fixed variables

Compare only the **structured MCP synchronous handoff ceiling**, with other Host settings fixed:

| Cohort | Host profile | Host budget | Structured sync ceiling | Ordinary continuation |
| --- | --- | ---: | ---: | ---: |
| baseline | `host_code_mode` | 55s | 5s (existing default) | 5s |
| candidate | `host_code_mode` | 55s | 8s (explicit experimental override) | 5s |

All actual task timeouts, Runner configuration, Server Git build, protocol limits, Model/Host version, machine, project revision, authorization and task instructions must remain unchanged. Restart the managed Server when switching deployment env; verify the **deployment-effective** `get_runtime_status.effective_config.mcp_host` projection before the first task of each cohort. Observe whether the per-request policy was narrowed by an MCP header using existing metadata traces (`WEBCODEX_TOOL_REQUEST_TRACE=metadata`, where allowed). A request-local profile change or narrowing makes a sample ineligible unless separately stratified.

Run each workload at least in an alternated A/B/B/A schedule, warming caches before measurement and never mixing cold Rust build against warm tests. Do **not** silently include other users/Windows/tasks in one cohort. Exact tracked tasks should include short commands, 5–10s commands, long validation, several concurrent Jobs, intentional nonzero exit, Runner saturation and a controlled reconnect/recovery example. Use separate fixed time windows and explicit project filter. Save build revision, profile, budget, effective sync/continuation, Runner load, scenario and final correctness as **run-side operator evidence**, never in raw exported ActionAudit.

## Aggregate-only CLI

The offline reporter consumes existing `action_events` from a *read-only local copy* of the Server SQLite database. It opens SQLite in `mode=ro`, requires UTC half-open time bounds and exact project filtering, and caps any analysis at 100,000 selected rows over at most seven days. **Do not share your private database**, any raw trace file or credentials.

Example cohort manifests (the Git SHA must be the same exact known source revision for both):

```json
{
  "schema_version": 1,
  "cohort": "baseline",
  "case_id": "long_validation",
  "base_revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "host_profile": "host_code_mode",
  "host_budget_secs": 55,
  "sync_wait_secs": 5,
  "continuation_wait_secs": 5
}
```

Replace the placeholder Git SHA with the **real 40-character common revision**. Save as `baseline.json`; `candidate.json` uses `cohort: candidate` and `sync_wait_secs: 8`, with all other fields unchanged. These files are **operator declarations**, *not proof* the deployment actually ran those settings. Capture the real Server status and Host trace separately to validate them.

```bash
python3 scripts/host_timing_dogfood.py summarize \
  --audit-db /path/to/read-only/baseline.db \
  --policy baseline.json \
  --project agent:special:webcodex \
  --since-utc 2026-10-10T00:00:00Z \
  --until-utc 2026-10-10T01:00:00Z \
  --output baseline-summary.json

python3 scripts/host_timing_dogfood.py summarize \
  --audit-db /path/to/read-only/candidate.db \
  --policy candidate.json \
  --project agent:special:webcodex \
  --since-utc 2026-10-10T02:00:00Z \
  --until-utc 2026-10-10T03:00:00Z \
  --output candidate-summary.json

python3 scripts/host_timing_dogfood.py compare \
  --baseline baseline-summary.json \
  --candidate candidate-summary.json \
  --output descriptive-comparison.json
```

Times are **examples**, not a claim these runs occurred. Run this only on self-hosted data that the operator is authorized to inspect.

## Interpretation and guardrails

The report uses only ActionAudit `action_events` and `summary_json.model_ergonomics` (existing schema). Exact `toolsCall` records must have matching canonical `tool_name`, schema version and boolean `success`; anything else is counted as unavailable. `pending`/`running` are **observed handoff states**; `completed` is a separately observed in-call completion state; other or missing structured states are explicitly unclassified. Never replace missing states with `status=success`. No Job state is inferred from the HTTP status alone.

The fields named `structured_request_p50/p90` and `observation_request_p50/p90` are retained for report compatibility, but their actual source is **`action_events.duration_ms`, the legacy ActionAudit pre-render interval**. It may end *before* MCP response handoff. This is explicitly asserted in `src/mcp_tests/http_transport.rs`; the ActionAudit duration is **not** full Host-visible request duration, transport latency, end-to-end task wall time or consumed Host budget. Reports now carry `duration_basis=legacy_action_audit_pre_response_handoff_not_host_latency`. For a real latency evaluation, correlate the existing transport `request_observed_at_ms` → `response_handed_at_ms` timestamps (and Host-side arrival/completion timing if available); only then make Host-budget/latency statements.

Structured/observation request p50/p90 and readiness `waited_ms` use known nonnegative values only, with denominators. `outcome_unknown` and `dispatch_hard_timeout` have an explicit count. `meaningful_outer_calls_proxy` is **not** the number of model turns. The report intentionally emits `model_turns=null`, `task_wall_time_ms=null`, `final_job_outcomes=null`, `correctness=null`: ActionAudit alone cannot prove these. Use exact task-level annotations, durable Job terminal evidence, validation/CI/review outcomes and real Host telemetry for those fields **outside** this aggregation.

The comparison only checks that the **declared policy dimensions** are aligned and the sync ceiling differs. Its raw deltas are descriptive, not a causal difference-in-means: runs can differ in workload, scheduling, Server activity and missing telemetry. `causal_claim_supported=false` and `final_job_outcomes_comparable=false` remain fixed.

Do not recommend a new default unless correctness and terminal reliability do not regress and paired end-to-end task wall time, Host model-turn evidence and total Host budget use support the change across a representative repeated workload. If not available, the correct result is **insufficient evidence**.

No changes to production defaults, active Server/Runner configuration, schema, auth, request-policy, Tool Contract or Desktop UX are part of this Phase 3 offline utility.
