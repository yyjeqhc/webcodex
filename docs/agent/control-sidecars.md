# Optional control-plane sidecars

Stateless MCP 2026 model calls carry this metadata as `_wc.control`. The MCP
adapter strips the outer `_wc` envelope before canonical business parsing; the
control payload is then parsed through the existing typed sidecar contract. Generic
`call_runtime_tool` accepts `_wc.control` only on the outer gateway call; nested
target `arguments` remain canonical business arguments and reject a second `_wc`.
Legacy MCP, HTTP Actions, internal callers without the explicit capability,
ModelHidden/App calls and specialized Plugin/SSH/Browser/Computer gateways do not
accept this Stateless 2026 envelope contract. Canonical standalone tools remain
fully available.

```json
{
  "project": "agent:runner:project",
  "executable": "cargo",
  "args": ["test", "focused_filter"],
  "_wc": {
    "control": {
      "before": {
        "attempt_heartbeat": {
          "task_id": "wc_task_abcdefghijklmnop",
          "attempt_id": "wc_attempt_abcdefghijklmnop",
          "assignee_agent_id": "wc_dagent_abcdefghijklmnop",
          "attempt_fence": "<exact existing attempt fence>",
          "attempt_controller_generation": 1
        }
      }
    }
  }
}
```

Use this only when an explicit control fact is already established and another
ordinary tool call is needed. Omit it otherwise. Operations requiring individual
Host review must use their canonical standalone callable. MCP rejects `before.goal_progress`,
`before.wake_consume` and `after_success.goal_completion` before any main or
sidecar effect; use `checkpoint_goal`, `consume_agent_wake` and `update_goal`
respectively. Their omission or refusal by the Host never permits a sidecar or
gateway fallback. After completing a plan phase, checkpoint its actual progress
with the standalone tool; do not pre-complete tests or postpone checkpoints.
Standalone tools are always valid, including when there is no suitable main call.
Traffic, validation results, Window activity and App polling never infer a
transition or renew a lease.

| Phase / field | Canonical invocation |
| --- | --- |
| `before.attempt_heartbeat` | `heartbeat_agent_task_attempt`, identical fences and Server lease policy |
| `before.session_context_update` | Reserved canonical `update_session_context` payload; currently rejected before main |
| `after_success.session_close` | `close_session`, exact Session id |
| `after_success.todo_completion` | `complete_session_message`, including the unchanged exact assignment fence |

The envelope and payloads are closed. Each `before` / `after_success` mutation
object contains exactly one operation; it is not an arbitrary operation list.
Existing `_wc.resolve` is also an effectful before mutation, so combining it with
`_wc.control.before` is rejected. ACK and context material requests do not count.
The separate `communication` field admits at most two messages total, as described
below. Session close is admitted only on `finish_coding_task`; it must succeed
with explicit `task_outcome.blocking=false`. Todo completion also requires a known
successful main result; asynchronous handoff/unknown results do not qualify.

## Replay-safe communication

`_wc.control.communication` can contain `before` and `after_success` arrays.
Each array entry is a closed, single-key `session_message` or `peer_message`
object using the corresponding `post_session_message` / `post_peer_message`
arguments. Every entry requires a nonempty `delivery_key`; the combined array
count is at most two. This is an optional way to carry already-authorized
communication on a needed call, not permission to send messages or select targets.
Standalone message tools remain valid.

The Server parses all message payloads and checks each message tool's scope before
main dispatch. Malformed payloads, excess messages, missing delivery keys or
insufficient scopes reject the envelope before effects. Actual message dispatch
still rechecks its own exact Session/Peer authority. The before mutation, if
present, runs first: a failed mutation prevents both before messages and main.
Once that gate succeeds, runtime failure of a communication message is recorded
independently and does not prevent main. After-success messages run only for a
known completed successful main result, with the same nonblocking-closeout gate
when main is `finish_coding_task`.

Receipts live under `output.control.communication.before` / `after_success` and
retain message identity, replay, state-change and uncertainty facts. Reconcile a
lost message response with the same canonical target, delivery key and payload;
changed reuse conflicts. The message's replay protection never makes the main
call safe to repeat. Sending, model ACK, Reply and Resolve remain distinct; see
[manual collaboration](manual-window-collaboration.md).

## Execution and authority

1. Strip/parse the wrapper, eagerly parse canonical sidecar requests, and reject
   operations requiring individual Host review before any effect.
2. Check adapter admission and each canonical sidecar scope independently.
3. Authorize recorder, main request scope and exact business targets.
4. Run the optional before mutation through the canonical kernel, including its
   own permissions, target/lifecycle checks, domain helper and replay fences.
5. Run admitted before messages, then dispatch main if the before mutation
   succeeded or was an exact canonical replay. Message execution failures are
   separate receipts; they do not grant or remove main authority.
6. Inspect main truth and run the eligible after-success mutation and messages
   through the same canonical kernel. Domain fences are checked at each operation.
7. Preserve the main result and append bounded `output.control` observations.

No domain transaction spans Goal, Agent and Workflow Session stores. A committed
before mutation remains committed when main later fails. A main call is never
made idempotent by its sidecar: replay keys protect only the associated canonical
mutation. After lost responses or effect uncertainty, reconcile main separately.
The nested canonical invocation retains authorized recorder provenance, but never
inherits main scope/permission or recursively propagates sidecars/capabilities.

## Result and recovery

Only Stateless MCP requests carrying `_wc.control` receive `output.control`. Top-level `success`
still reports **main** success; post failure never turns an already-successful
main effect into an overall failure. `control.main.execution_state` distinguishes
`succeeded`, `failed`, `started`, `outcome_unknown`, and
`definitely_not_started`. `state_changed=null` means unknown, never false by
assumption. Each requested phase reports kind, success, execution state, change
and replay truth, plus a bounded error code on failure. Skipped phases explicitly
report `definitely_not_started`.

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

## Source and regression map

[`ControlSidecars`](../../src/tool_runtime/control_sidecar.rs) owns closed payloads,
message bounds, canonical dispatch order and receipts. The
[MCP invocation adapter](../../src/mcp/tools/invocation.rs) filters individually
reviewed operations and rejects unsupported envelope fields before dispatch.
[`control_sidecars` tests](../../src/tool_runtime/tests/control_sidecars.rs) cover
message replay/standalone parity, independent authority, before failures,
after-success gates, bounds and uncertain outcomes. These are source-level
regressions; their presence is not evidence that a particular Host displayed or
acted on a sidecar.
