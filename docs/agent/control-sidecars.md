# Optional control-plane sidecars

Stateless MCP 2026 model calls may carry `_control` wrapper metadata. The
adapter strips it before canonical business parsing and payload tracing. Generic
`call_runtime_tool` accepts it either outside or inside `arguments`, never both.
Legacy MCP, HTTP Actions, internal callers without the explicit capability,
ModelHidden/App calls and specialized Plugin/SSH/Browser/Computer gateways do not
accept this v1 contract. Canonical standalone tools remain fully available.

```json
{
  "project": "agent:runner:project",
  "executable": "cargo",
  "args": ["test", "focused_filter"],
  "_control": {
    "before": {
      "goal_progress": {
        "goal_id": "wc_goal_abcdefghijklmnop",
        "expected_revision": 3,
        "idempotency_key": "implementation-complete",
        "completed_step_ids": ["implement"],
        "current_step_id": "tests",
        "summary": "Implementation complete; begin focused tests"
      }
    }
  }
}
```

Use this only when an explicit control fact is already established and another
ordinary tool call is needed. Omit it otherwise. After completing a plan phase,
attach the previous phase's completion and next current phase to the next ordinary
call; do not pre-complete tests or postpone all checkpoints to task closeout.
Standalone tools are always valid, including when there is no suitable main call.
Traffic, validation results, Window activity and App polling never infer a
transition or renew a lease.

| Phase / field | Canonical invocation |
| --- | --- |
| `before.goal_progress` | `checkpoint_goal`, identical fields |
| `before.wake_consume` | `consume_agent_wake`, identical exact continuation proof |
| `before.attempt_heartbeat` | `heartbeat_agent_task_attempt`, identical fences and Server lease policy |
| `before.session_context_update` | Reserved canonical `update_session_context` payload; currently rejected before main |
| `after_success.goal_completion` | `update_goal` with explicit `lifecycle=completed`; only Goal id, revision, key and optional terminal reason |
| `after_success.session_close` | `close_session`, exact Session id |
| `after_success.todo_completion` | `complete_session_message`, including the unchanged exact assignment fence |

The envelope and payloads are closed. A present phase has exactly one mutation;
there is no operation list. Existing `session_message_resolution` is also an
effectful before mutation, so combining it with `_control.before` is rejected.
ACK and context material requests do not count. Goal completion and Session close
are admitted only on `finish_coding_task`; it must succeed with explicit
`task_outcome.blocking=false`. Todo completion also requires a known successful
main result; asynchronous handoff/unknown results do not qualify.

## Execution and authority

1. Strip/parse the wrapper and eagerly parse both canonical sidecar requests.
2. Check adapter admission and each canonical sidecar scope independently.
3. Authorize recorder, main request scope and exact business targets.
4. Run the optional before mutation through the canonical kernel, including its
   own permissions, target/lifecycle checks, domain helper and replay fences.
5. Dispatch main only if before succeeded or was an exact canonical replay.
6. Inspect main truth and run the eligible after-success mutation through the
   same canonical kernel. Domain fences are checked at mutation time.
7. Preserve the main result and append bounded `output.control` observations.

No domain transaction spans Goal, Agent and Workflow Session stores. A committed
before mutation remains committed when main later fails. A main call is never
made idempotent by its sidecar: replay keys protect only the associated canonical
mutation. After lost responses or effect uncertainty, reconcile main separately.
The nested canonical invocation retains authorized recorder provenance, but never
inherits main scope/permission or recursively propagates sidecars/capabilities.

## Result and recovery

Only requests carrying `_control` receive `output.control`. Top-level `success`
still reports **main** success; post failure never turns an already-successful
main effect into an overall failure. `control.main.execution_state` distinguishes
`succeeded`, `failed`, `started`, `outcome_unknown`, and
`definitely_not_started`. `state_changed=null` means unknown, never false by
assumption. Each requested phase reports kind, success, execution state, change
and replay truth, plus a bounded error code on failure. Goal results include the
current revision. Skipped phases explicitly report `definitely_not_started`.

A before rejection proves main was not dispatched. After main succeeds but the
post mutation fails, recover just that mutation with its standalone canonical
tool and exact fence/replay contract; do not repeat main blindly. Unknown post
outcomes are labeled `outcome_unknown`. Tokens, Attempt fences, assignment fences,
answers, Goal summaries and arbitrary business bodies are absent from control
projections.

`update_session_context` currently replaces defaults without an exact CAS/replay
identity. Its sidecar therefore returns
`session_context_replay_contract_required` with main definitely not started.
The standalone tool is unchanged. Enabling this sidecar later requires a shared
canonical CAS/replay contract plus exact active Runner resource validation; v1
never silently broadens context, environment or credential authority.
