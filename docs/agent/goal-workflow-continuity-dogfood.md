# Goal workflow and single-window continuity — G4/G5

This is the current WebCodex-owned Goal workflow contract, not repository
`AGENTS.md` policy. Architecture is defined in
[`durable-agent-runtime.md`](../architecture/durable-agent-runtime.md) and
[`durable-agent-conversation.md`](../architecture/durable-agent-conversation.md).

## Ordinary single-window flow

Use a new or existing durable Goal for substantial multi-step/cross-turn work.
Create bounded completion conditions and stable plan steps, explicitly associate
the current Workflow Session, and present the Goal Plan card. Tiny one-step
lookups/trivial edits do not require setup. Use `checkpoint_goal` at recoverable
milestones: exact Goal revision and idempotency key, atomic completed-step ids and
optional current step, and a bounded recovery summary. Complete all steps and
freshly verify/review before explicitly completing the Goal. Session closeout
returns owned correlated active Goal follow-up; it does not complete the Goal.

Automatic continuation is optional and requires an exact durable controller Agent.
An Agent already made callable through explicit identity/Endpoint/presentation
setup should be reused as that controller, with the same Agent Continuation card.
The Agent may remain another Coordinator's Worker/Task assignee at the same time.
Do not infer identity from Window co-location or create a second Goal-only Agent.

The Goal Plan resource is solely `ui://webcodex/goal-plan/v6`, wire version 3. It
renders step counts, current step, bounded milestones, last checkpoint, activity,
and a bounded read-only continuity projection. Continuity keeps production Host
carrier readiness and the exact current Goal-stall Wake lifecycle separate from
bounded Host delivery and exact-consume fresh-turn proof. After newer meaningful work,
the current epoch returns to ready/stalled with no current Wake while the most recent
confirmed resume keeps its bounded Host outcome, fresh-turn proof, and timeline.
Old pre-production Goal resource aliases are not supported. Agent Continuation
remains a separate card and the only Host turn-dispatch carrier.

## Deterministic regression scenarios

The Store tests cover plan bounds, fixed ids, one current step, atomic validation
before mutation, revision competition, exact/changed keyed replay, reopen,
malformed persisted plans and completion/terminal gates. Attention migration tests
preserve existing Task-terminal facts and Wake identities in the one current schema.

Runtime Goal tests use controlled timestamps rather than sleeping five minutes.
They cover recent activity; continued live exact-Goal polling; one Event/Wake under
repeated/concurrent rechecks; a new epoch only after new meaningful work; running
requests; completion-record gaps; absent/stale/wrong-Goal cards; partial visibility;
foreign principals and missing scopes; revoked historical correlated Projects even
when their work is older than the current anchor; malformed/missing card timing;
stale/closed/ambiguous Sessions; absent
controllers; terminal Goals; transactional Wake insertion failure; malformed
persistence; and Goal closeout privacy. They also exercise the same Agent as Worker
and controller, the existing dispatch fence, accepted versus unknown delivery,
exact consume, compact source-specific recovery messages, and no Task spawning.

The Goal Plan JavaScript contract tests verify one exact `goal_plan_sync` App RPC,
serial adaptive polling, hidden/visible cadence, teardown, and no conditional second
RPC. Server idempotency remains the durable duplicate-prevention boundary. Sync
calls carry only `goal_id`; no browser timestamp, Session, controller, Window
selector or claimed coverage is trusted. There is no Goal Plan `ui/message` path.
Terminal state stops polling.

Representative focused commands:

```sh
cargo test -p webcodex-store --lib
cargo test -p webcodex-tool-contracts --lib
cargo test -p webcodex-tool-runtime-contracts --lib
cargo test -p webcodex-workflow-session --lib
cargo test -p webcodex --lib goal
cargo test -p webcodex --lib agent_continuation
cargo test -p webcodex --lib agent_wake
cargo test -p webcodex --lib window_
cargo test -p webcodex --lib workflow_session
cargo test -p webcodex --lib finish_coding_task
cargo test -p webcodex --lib startup_brief
node --test src/mcp_tests/goal_plan_app.test.mjs src/mcp_tests/goal_workflow_app.test.mjs
node --test src/mcp_tests/agent_continuation_app.test.mjs
```

## Separate production-Host verification boundary

Deterministic Store/runtime/App tests do not establish that a particular live Host
will schedule a background turn. This change does not itself deploy, restart or
issue real automatic Host messages. Manual production dogfood requires a separately
authorized deployment and an actually bound Agent Continuation card.

In that environment, explicitly set up/reuse the controller, correlate the current
active Session, then keep the exact Goal Plan card observed while meaningful work
is quiet. After 300,000 ms, a successful exact-Goal sync within the Server-owned
75,000 ms observation lease and at least 1,000 ms later than the last meaningful
completion is only a candidate. The App targets 12s while visibly stable, 5s near
the boundary or during a Wake transition, and 60s while hidden/backgrounded; the
lease covers that hidden cadence plus bounded Host scheduling slack. The Server
still checks current Goal/controller/Session/Project authority, latest
Window-to-Session relation, complete evidence and no active meaningful request.
Closing or losing the card must not generate a new automatic turn. A different
Goal's polling cannot keep this Goal's card alive.

Inspect only authorized sparse facts: one `goal_workflow_stalled` Event, one logical
Wake, and existing carrier dispatch state. Pending means no delivery proof;
`dispatch_accepted` is not model resumption; `delivery_unknown` must not cause a
same-epoch resend. Only exact `consume_agent_wake` proves the new turn ran. A later
independent inactivity epoch requires newer meaningful work after the earlier
attention, not a metadata revision or imported older Session history.

The resumed turn must bootstrap/consume that exact Wake, re-read Goal/plan, recover
the exact correlated Session with `session_handoff_summary`, and use current
Job/Project truth only as needed. Continue from the latest checkpoint/current step.
A disappeared turn does not establish failure: never replay an uncertain previous
effect merely because it disappeared. Stalled is not offline, heartbeat is not
authority, and continuation is a fresh reasoning opportunity rather than a retry.
