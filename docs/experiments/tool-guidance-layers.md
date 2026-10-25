# Tool contract and guidance layers — 2026-10-25

## Scope and evidence

Base: `2371ec6730f16cee137a94835f7ac3a71364925b` (main after #1288–#1291).
This change is independent of [the response measurement baseline](https://github.com/yyjeqhc/webcodex/pull/1293).
Measurements below are serialized JSON or UTF-8 bytes from repository tests on mini,
not Host Tokens, real-client latency, or client error rates.

Two concrete problems remained after those merges:

- `src/mcp/discovery.rs` copied six execution/Job descriptions independently of
  canonical ToolDefinitions. In particular, the compact `observe_jobs` description
  made a readiness wait sound mandatory before diagnostic observation, although
  the canonical tool permits immediate reads.
- The default workflow repeated detailed Goal continuation/checkpoint guidance
  already available in the explicit `webcodex.goal_workflow` context chapter.

## Ownership and projections

| Layer | Existing owner | Change |
| --- | --- | --- |
| L0: normative contract | ToolDefinition, typed input/output schemas, execution metadata | Eight concise execution/Job descriptions; six compact overrides removed. |
| L1: invocation strategy | Guidance Contributors and selected Direct/Host Code Mode/Code Mode profile | Retained existing batching, pending fallback, dependency joins, wait budgeting and semantic stop rules. |
| L2: selected workflow | Context Projection and ModelWorkflowPolicy | Goal details remain in the existing explicit chapter; default context retains exact-identity selection, chapter discovery and no incidental Goal creation/closeout. |

No selection-description registry, parallel prompt framework, client-specific
ToolDefinition copy, additional execution tool, or configuration option was added.
The existing compact adapter retains overrides for actual presentation/admission
differences. Tests prevent the migrated tools from drifting or being truncated.

## Execution distinctions retained

| Form | Ownership and selection boundary |
| --- | --- |
| `run_process` | Literal native argv, Runner-owned; interactive mode is a keyed-input pipe, not PTY/SSH or structured validation. |
| `run_shell` | Shell grammar/related chain. Local Jobs are Runner-owned; named Session SSH has **no durable Job handoff**. |
| `run_script` | Typed script data, never interpolated into a command string; supported languages still come from the input schema. |
| `run_job` | Intentional immediate asynchronous shell start; queued identity is stable. Ordinary synchronous work already hands off the same execution. |
| `run_detached_process` | Supervisor ownership accepted before payload start; recovery needs matching identity/lifetime fence, and expired replay keys do not authorize retry. |
| Session persistent shell | Existing exact-Session, shared-process/state contract retained without edits. |

Pending retains a returned Job and continuation; `observe_jobs` recovers known
Jobs. An uncertain execution without a Job requires inspection of the actual
process/service/target state, not an invented Job identity or redispatch.
The SSH exception was verified in `src/tool_runtime/shell.rs`, not inferred from
a common tool name. Exact tool-name discovery tests also prevent cross-reference
prose from needlessly adding `run_job` to a `run_shell` query.

`list_jobs` remains identity recovery/inventory. `observe_jobs` can immediately
read logs/details. `wait_for_job_readiness` remains a bounded, authorized current-turn
join: terminal is not success, deadline may leave pending targets, and readiness
grants no execution or retry authority. No Server-side replay or automatic Host
turn was introduced. Timeout parameter limits and runtime execution code are unchanged.

## Measured costs

| Canonical description | Before UTF-8 bytes | After UTF-8 bytes |
| --- | ---: | ---: |
| run_process | 1,007 | 412 |
| run_shell | 882 | 410 |
| run_script | 971 | 403 |
| run_job | 695 | 384 |
| run_detached_process | 849 | 415 |
| observe_jobs | 977 | 347 |
| list_jobs | 744 | 316 |
| wait_for_job_readiness | 1,021 | 351 |
| **Total** | **7,146** | **3,038** |

The eight descriptions shrink by 4,108 bytes (57.5%). This is relevant to exact
manifests and full canonical descriptions; it is not a claimed saving for every
`tools/list` projection. Complete compact catalogs and workflow measurements:

| Projection (same existing fixture) | Before JSON bytes | After JSON bytes | Change |
| --- | ---: | ---: | ---: |
| CLI, 17 tools | 34,800 | 35,129 | +329 |
| Adaptive without Apps, 41 tools | 69,189 | 69,559 | +370 |
| Adaptive with Apps, 74 tools | 105,055 | 105,425 | +370 |
| Default workflow, Direct, on-demand/unknown | 12,832 | 12,163 | −669 |
| Default workflow, Host Code Mode, on-demand/unknown | 14,683 | 14,014 | −669 |
| Optional Goal chapter, on-demand/unknown | 3,485 | 3,577 | +92 |

The compact catalogs grow by 0.95%, 0.53% and 0.35%, respectively. They now carry
the concise canonical lifecycle distinctions; the MCP observer still appends
its real `summary_only=true` default override. Its schema/default and exact
bounded suffix are explicitly tested rather than treated as duplicate L0 prose.
Default workflow reductions are 669–773 bytes depending on the previous Host
resume mode; both default profiles retain the same shared orchestration rules.

The complete default workflow contains more than the two removed fields. The
optional chapter grows by 92 bytes to keep the explicit no-Goal checkpoint guard
at the workflow's own entry point. All policy combinations remain under its
existing 4,096-byte regression target. No budget was increased to accommodate
the change.

## Compatibility and validation

Business input schemas, tool admission, routing, permissions, execution metadata,
Job persistence and file revision rules are unchanged. Built-in workflow version
advances **30 → 31** because its model-guidance output drops two required fields;
the matching output schema changes with it. Retained workflow context must be
refreshed under the existing version/fingerprint rules after upgrade. There is
no Job or durable-state migration. Host continuation availability still derives
from the existing policy plus the exact live binding; optional prose does not
grant it.

Validation reuses existing facilities:

- Guidance preflight and its Python unit tests.
- Tool-contract registry/schema tests, including exact-discovery, SSH, lifecycle
  and observation-policy boundaries.
- Full MCP tests, including legacy/modern published input equivalence, annotations,
  CLI and compact/full canonical-description equality.
- Workflow policy tests across two preferences, three Host resume modes and two
  default profiles; explicit context request retains the complete Goal chapter.
- Existing built-in workflow budget/contract tests; formatting, diff and Markdown checks.

Local results on mini: **335 tool-contract tests passed** (one existing ignored),
**429 MCP tests passed** (one existing ignored), **7 model-workflow tests passed**,
**13 built-in-workflow tests passed**, and **11 guidance-verifier Python tests
passed**. Guidance preflight, formatting and diff checks passed. The exact
measurement commands are:

```sh
python3 scripts/check_builtin_guidance_contract.py
python3 -m unittest scripts.tests.test_check_builtin_guidance_contract
cargo test --locked -p webcodex-tool-contracts --lib
cargo test --locked -p webcodex --lib mcp::tests -- --nocapture
cargo test --locked -p webcodex --lib model_workflow -- --nocapture
cargo test --locked -p webcodex --lib builtin_coding_workflow
```

This PR removes an incorrect mandatory-wait implication and redundant default
context. It does not claim measured Host call-count, latency or completion-rate
improvements. Actual ChatGPT/Claude Code/Codex trials and the telemetry baseline
can measure those separately without treating byte counts as Tokens.
