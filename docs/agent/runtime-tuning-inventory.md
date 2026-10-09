# Runtime tuning inventory — #984 Phase 1

Source baseline: `main@6f8ed6d8` (2026-10-09). This is an ownership and semantics audit, **not** a proposed change in production defaults or authorization limits. Entries marked *candidate* need separate policy, bounds and measurement review before becoming operator overrides.

## Host MCP request timings: authoritative consumers

| Value / source | Effective defaults and bounds | Runtime consumer | Apply / observation | Classification |
| --- | --- | --- | --- | --- |
| `WEBCODEX_MCP_HOST_PROFILE`, `src/mcp_host.rs` | `direct` by default; `host_code_mode` opt-in | Chooses Host timing derivation and omitted model guidance | Server deployment/environment; selected effective profile in `get_runtime_status.effective_config.mcp_host` | Existing operator setting |
| `WEBCODEX_MCP_HOST_BUDGET_SECS` | direct 60s / host_code_mode 55s; positive integer; effective safe wait `max(1, budget - 5s)` | Request-local Host budget, return guard and readiness ceiling; **not** Job execution timeout | Deployment, request-local bounded selection; effective Server status and request-policy trace | Advanced candidate; current invalid env falls back to default |
| `initial_job_handoff_secs` | direct `min(10, safe_wait)`; Host Code Mode `min(5, safe_wait)` | **No independent runtime consumer** found in current source; only policy struct, status/schema, frontend and tests | Read-only legacy wire-compatible diagnostic field, not an operator control | Legacy projection; deprecated, leave key intact to avoid breaking strict clients |
| `max_sync_wait_secs` | direct `min(60, safe_wait)`; Host Code Mode `min(5, safe_wait)` | `mcp_timing::return_timing_policy` → `return_timing::normalize_structured_handoff` → durable Job handoff, intersected with internal orchestration caps | Derived per MCP request; status reports deployment value, which need not equal a request-local narrowed value | Advanced tuning *candidate*, not currently separately configurable |
| `continuation_wait_secs` | direct `min(100, safe_wait)`; Host Code Mode `min(5, safe_wait)` | `mcp_timing::normalize_observation_call_timing` for `observe_jobs`/`job_tail`; `normalize_result_timing` for generated `observe_jobs` continuation | Derived per MCP request; same deployment/request-local distinction | Advanced tuning *candidate*, not currently separately configurable |
| Host return guard `HOST_RETURN_GUARD_SECS` | fixed 5s | Reserve budget for returning the MCP response | Code-owned, not a user control | **Internal invariant** |
| `wait_for_job_readiness` explicit wait | Tool-specific wait capped by `max(1, host_budget - 5)` | Separate branch of `normalize_observation_call_timing`, **not** generic continuation 5s cap | Request-local, bounded by remaining Host wait allowance | **Keep separate** from ordinary Job polling |
| Canonical `sync_wait_secs` | default 10s, protocol ceiling 60s, intersected with execution timeout | `structured_execution::StructuredExecutionBudget` and `return_timing` | Per call, cannot increase Server-owned caps | Existing request preference, **not** Host execution-lifetime control |
| Job observation protocol limit | fixed 100s | `MAX_JOB_OBSERVATION_WAIT_SECS` in Core and tools | Protocol bounded | **Internal invariant** |

At present, the three derived Host Code Mode values all happen to be 5s, but their **consumers and authority are different**. Raising all three together is not a safe tuning experiment. Request-local narrowing must not be represented as an increase to deployment authority. API transport bypasses these MCP-only normalizers.

## Broader bounded inventory (Phase 1 discovery)

The following records identify representative knobs across the required runtime areas; they do not authorize a new global settings surface. Defaults here are sourced from the checked-in code. Values not independently established here are deliberately *not inferred*.

| Control | Owner / actual source | Default / bounds found | Apply / visibility | Classification / Phase 1 decision |
| --- | --- | --- | --- | --- |
| Runner Job capacity | Runner `runner.toml`, `webcodex_runner/config.rs` | default 4; 1–64 | Runner registration/capacity, Desktop Runner Job capacity panel; explicit save/apply | Existing **normal** operator tuning; do not duplicate |
| Runner polling interval | Runner `poll_interval_ms`, runner-config | default 1000ms; positive, at most 30000ms when polling may be used | Runner config, reload semantics owned by Runner | Existing transport tuning; do not couple to MCP Host budget |
| Runner WebSocket connect timeout | Runner `websocket_connect_timeout_secs` | default 5s; must be positive | Runner config; connection attempts | Existing transport tuning |
| Runner QUIC connect timeout | Runner `quic_connect_timeout_secs` | default 10s | Runner transport config | Existing transport tuning |
| Runner QUIC keepalive | Runner `quic_keepalive_interval_secs` | default 20s | Runner transport config | Transport setting; do not elevate to Desktop without demand |
| Runner command max timeout | Runner-config `max_timeout_secs` | default 3600s | Runner policy, bounded execution | Existing Runner setting, **not** MCP handoff wait |
| Runner max output | Runner-config `max_output_bytes` | default 256KiB | Runner response limit | Safety/resource bound; not automatically a performance knob |
| Coding Agent ACP run capacity | Runner `config.rs` | default 1; range 1–8 | Runner coding-agent configuration | Existing concurrency control; independent of Job capacity |
| ACP permission timeout | Runner `config.rs` | default 5s, range 1–60s | Permission/approval handshake | Authorization-related cap: **keep distinct**, not a generic speed knob |
| Persistent shell count | Runner `config.rs` | default 8, minimum 1 (full bound requires its validator) | Runner shell manager | Candidate only after capacity/resource audit |
| E2a/E2c consequential child handoff | Server `tool_runtime/code_mode.rs` | fixed 5s | Internal consequential child return timing | **Internal orchestration invariant**, not Host profile knob |
| Consequential started-child drain | Server `tool_runtime/code_mode.rs` | fixed 5000ms | Controlled post-start child drain | **Internal correctness/lifetime invariant**, not operator-exposed |
| Effective config diagnostics | Server `tool_runtime/runtime_info.rs` + Tool Contracts schema | safe allowlisted effective fields | `get_runtime_status` + Runtime UI | Read-only observability; never credential values |
| Desktop host deployment defaults | Desktop Tauri `state/readiness.rs` | profile/budget deployment environment | managed Server startup; applies at next managed launch | Deployment owner, not Runner tuning |

### Phase 1 decisions

1. **Preserve the JSON key** `initial_job_handoff_secs` for existing typed clients and mark it deprecated as a **legacy derived hint only**. In the Runtime UI, explicitly identify that it is unused for execution; do not drop a required field from the strict output schema without versioning.
2. Test the *actual* timing consumers independently: structured handoff, ordinary observation and generated continuations, readiness joins. A changed legacy hint must have no effect.
3. Keep 5s Host Code Mode defaults, 55s Host budget, Runner/Code Mode internal caps and protocol maxima unchanged.
4. Phase 2, if independently justified, should expose no more than *two* separately bounded Server-owned tuning concepts: structured synchronous handoff ceiling and ordinary continuation wait. Host budget stays a separate existing authority. Request-local headers can only narrow deployment bounds, not raise them.
5. Require before/after end-to-end timing, handoff rate, readiness/observe turns, Host duration and final Job outcomes before proposing different defaults. Successful HTTP/MCP return is not evidence of final Job success.

## Verification pointers

- `src/mcp_host.rs`, `src/mcp/request_policy.rs`: config/defaults and request-local policy
- `src/tool_runtime/mcp_timing.rs`, `src/tool_runtime/return_timing.rs`, `src/tool_runtime/structured_execution.rs`: actual consumers
- `src/tool_runtime/code_mode.rs`: separate inner orchestration caps
- `src/tool_runtime/runtime_info.rs`, `crates/webcodex-tool-contracts/src/registry/output_schemas/discovery.rs`: deployment diagnostics wire contract
- `crates/webcodex-runner/src/webcodex_runner/config.rs`, `crates/webcodex-runner-config/src/lib.rs`: Runner policy and defaults
- `apps/desktop/src-tauri/src/state/readiness.rs`: Desktop-managed Server deployment values

This is a **first-stage representative inventory**, not certification that every internal constant has been audited or that advanced tuning is ready to expose. Re-audit against the then-current main before Phase 2.

## Phase 2 addendum — independently bounded Server waits

This section supersedes the **not separately configurable** classification of the two timing candidates in the Phase 1 historical table above. It is not a reinterpretation of its `main@6f8ed6d8` source snapshot.

- `WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS` (optional integer, 1–60) controls the deployment ceiling for MCP structured-execution synchronous handoff.
- `WEBCODEX_MCP_HOST_CONTINUATION_WAIT_MAX_SECS` (optional integer, 1–100) controls ordinary MCP Job observation and generated `observe_jobs` continuation waiting.
- Both are parsed once on Server startup, require restart, reject invalid input without echoing values, and are limited by the request's effective Host budget minus the existing return guard.
- These explicit deployment caps are retained internally through request-local profile changes, so changing `X-WebCodex-MCP-Profile` cannot escape them. The request-local Host budget still can only narrow the deployment budget.
- When omitted, profile defaults and prior request-local profile switching behavior remain unchanged. `wait_for_job_readiness`, API transport, Runner configuration, Job execution lifetime and internal Code Mode child controls are unchanged.
- The legacy `initial_job_handoff_secs` projection remains present and does not become a control. Effective values appear in the existing allowlisted `get_runtime_status` deployment configuration, without disclosing environment names/contents or internal cap metadata.

These are controlled tuning **capabilities**, not evidence that an 8-second policy performs better. Collect before/after metrics and final Job outcomes separately; retain the original default values until tested.
