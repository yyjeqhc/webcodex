# Host execution control and pending boundaries

Status: opt-in MCP 2026 presentation; no Runner workflow engine or deployment change.

## Observed problem and scope

The user reported a deployment chain in which `npm ci` or `scp` handed off to a
Job and interrupted a Host cell's dependent commands. That deployment was not
replayed during this change. Source inspection confirms two separate costs:

1. A dependent operation must not start merely because the preceding tool returned
   `success=true`: the effect may still be pending or uncertain.
2. Ordinary synchronous success deliberately removes redundant lifecycle fields,
   whereas Job observation uses another, richer shape. A Host has to reconstruct
   control decisions across these shapes.

Pending does not intrinsically require a new model turn. A Host with a remaining
activation budget and an explicitly selected blocked dependency can wait and
observe the same Job in its current cell. But generic `fallback_recovery` calls
must not be promoted to `mechanically_followable`, and a Host cannot infer the
authority for later deployment steps from successful completion of an earlier one.

## Opt-in execution view

For MCP 2026 `run_process`, `run_script`, `run_shell`, `project_build`,
`run_skill_resource`, and `observe_jobs`, request:

```json
{"_wc":{"compact_execution":true}}
```

For `call_runtime_tool`, put `_wc` on the gateway envelope beside `tool` and
`arguments`. The resolved target must support this view. Ordinary calls, legacy
MCP, Runtime HTTP, Runner wire messages, persisted records, and execution limits
retain their existing contracts. Refresh Host tool schemas before using the flag.
The exact `read_tool_manifest` remains business-input-only; `_wc` is advertised
on MCP `tools/list`, including the outer gateway envelope for long-tail tools.

The ordinary ToolResult envelope remains. A command result's `output` becomes:

```json
{
  "execution": {"state":"completed","outcome":"passed","exit_code":0},
  "details": {"stdout_tail":"requested command output"}
}
```

`observe_jobs` puts the same `execution` object on each `output.items[]` member.
Batch counts, partiality, shared wait outcome and recovery guidance remain outside
those items. A successful batch or one completed Job does not prove every Job passed.

| Field | Meaning |
| --- | --- |
| `state` | Producer lifecycle vocabulary; `unknown` when absent. |
| `outcome` | `passed`, `failed`, `pending`, or `unknown`. Only `passed` proves command completion with exit code zero (and successful validation metadata when present). It is not an application health check or task oracle. |
| `exit_code` | Observed exit code, if available; never substitute zero for absence. |
| `job_id` | Exact original Job, when one exists. |
| `observation_ref` / `observation_token` | Exact next observation selector. Prefer the issued ref; no inferred identity or retry permission. |
| `next` | Existing adapter-admitted continuation, preserving its execution posture, Job and cursor. Pending continuations keep `fallback_recovery`; the presentation opt-in carries forward to `observe_jobs`. |

These facts are captured from canonical results before success compaction and after
execution/recording. A missing field, contradictory completion evidence, failed
observation, recovering/lost Job, or unknown process effect cannot become `passed`.
Protocol errors before a result exists retain their normal error response. Missing
`execution` is never success and must stop a dependent chain.
If the separated view would exceed the existing model-result byte budget, the
adapter returns the ordinary view without `execution`, preserving all diagnostics.

`details` keeps logs, validation information, truncation/reset evidence and failure
recovery. Synchronous output is not discarded: many successful commands have no
Job from which it could be recovered. This first version separates control from
data; it does not promise smaller total wire responses. Hosts can keep details
inside their cell and emit only decision-relevant evidence to the model.

Collaboration messages, ACK obligations, explicitly requested context, control
sidecar results and passive Job attention remain visible at the outer level.
`execution` is deliberately separate from the already-existing `control` sidecar.
A Host must surface these obligations even when it only needs command control facts.

## Host consumption

```js
const result = await tools.run_process({
  project, executable: "npm", args: ["ci"],
  _wc: { compact_execution: true }
});
const reply = result.structuredContent;
const execution = reply?.output?.execution;
// Surface any operator/peer/session messages and Job attention independently.
// Keep reply.output.details for output interpretation and diagnostics.
if (reply?.success === true && execution?.outcome === "passed") {
  // A previously selected and independently authorized dependent step may run.
} else if (execution?.outcome === "pending") {
  // Finish ready independent work. For this explicit blocked dependency,
  // join wait_for_job_readiness with a bounded remaining Host budget, then
  // observe the exact Job and require its execution.outcome === "passed".
  // At the activation deadline, retain identity and return; never redispatch.
} else {
  // Stop dependent effects. Diagnose failure/uncertainty; do not guess or retry.
}
```

The example is a consumption pattern, not an unattended scheduler or a claim that
the Host can survive a new activation. Shell success is also not enough for a
deployment: keep explicitly selected health, tunnel, configuration, and public
smoke checks as their own predicates.

## Deferred dependency-chain design

No new Runner batch/DAG tool is introduced. Before considering a 4–8 step chain,
resolve and test all of these together:

- **Target and authority:** per-step authorization and fences; `sf` and another
  Runner are separate targets. Admission of the plan cannot reserve future rights.
- **Conditions:** exit-zero, validator postcondition, and application-health
  predicates are distinct. Unknown, partial, denied, timed-out and cancelled
  predecessors block descendants; dependencies cannot be inferred from shell text.
- **Lifetime and cancellation:** one total deadline, bounded steps/output, clear
  ownership of the current child process, and cancellation of unstarted descendants.
- **Recovery:** a durable plan/step identity, checkpoint ordering and receipt
  reconciliation; a Server or Runner restart must not replay a consequential step.
- **Partial effects:** no implied transaction or automatic rollback for an already
  installed service or reloaded proxy. Recovery is an explicit subsequent decision.
- **Host boundary:** a bounded Runner plan cannot itself resume an expired Host
  activation, and a transport timeout is not a cancelled plan.

First compare ordinary calls and the control view on the same fixed workflows:
short all-success chains, mid-chain pending, nonzero exit, lost observation,
cancel/timeout and output truncation. Record required Host branching, schema or
interpretation repairs, model-visible bytes, completed task oracles and duplicate
side effects. Do not claim reduced model turns from tool-call counts alone.
Production deployment, a current-model A/B, and durable dependency execution are
outside this change.
