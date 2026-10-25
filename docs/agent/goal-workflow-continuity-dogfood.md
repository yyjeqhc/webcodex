# Goal workflow and single-window continuity — G4/G5/G6/G7

This is the current WebCodex-owned Goal workflow contract, not repository
`AGENTS.md` policy. Architecture is defined in
[`durable-agent-runtime.md`](../architecture/durable-agent-runtime.md) and
[`durable-agent-conversation.md`](../architecture/durable-agent-conversation.md).

## Optional single-window flow

Workflow selection is now [deployment guidance](model-workflow-policy.md), default
`on_demand`, not a requirement for every substantial task. Load `webcodex.workflow`
for current selection and `webcodex.goal_workflow` only for selected Goal work.
Tool inventories and schemas stay fixed when this preference changes.

For explicitly selected **new** durable Goal work, first establish the
exact Workflow Session with `work_on_project`, then call `prepare_goal_workflow` with
that exact `session_id`, bounded completion conditions/stable plan steps, and an
optional explicit controller Agent. The Store admits the new Goal and initial Session
correlation atomically at revision 1. Present the Goal Plan card when requested or
useful. Ordinary coding/review and tiny lookups need no Goal setup. Exact known Goals can still use the
lower-level `create_goal` / `associate_goal_workflow_session` primitives for explicit
advanced composition; `prepare_goal_workflow` never guesses or reuses a Goal by title,
Window, Session, or recency.

For ordinary continuation of an exact existing Workflow Session, `work_on_project`
also projects sparse `goal_context` when caller-owned active Goals are explicitly
correlated to that Session. One returned Goal may be explicitly reused with
`get_goal` / `present_goal_plan`; multiple returned Goals remain a bounded choice
and are never auto-selected. No Goal is inferred from Project, Window, title, or
recency, and the Session correlation grants no Goal authority. An unavailable
projection is not evidence that the Session has no active Goal. This normal re-entry
path prevents a new model turn from creating a duplicate Goal merely because it lost
the earlier Goal identity.

Explicit `read_session_handoff` recovery follows the same identity boundary but may
return a richer bounded sibling `goal_context` for an already-correlated active Goal:
Goal identity/lifecycle/revision, objective, current plan and checkpoint context. Zero
active correlations omit it; multiple active correlations remain `selection_required`
and expose only candidate identities until a caller chooses explicitly. This recovery
read never establishes a new correlation, refreshes Goal liveness, records Goal
progress, schedules continuation, or grants Goal/Session/Project authority.

Use `checkpoint_goal` at recoverable milestones: exact Goal revision and idempotency
key, atomic completed-step ids and optional current step, and a bounded recovery
summary. Complete all steps and freshly verify/review before explicitly completing
the Goal. Session closeout returns owned correlated active Goal follow-up; it does
not complete the Goal.

Automatic continuation is optional and requires an exact durable controller Agent
plus separately verified Host behavior. An MCP App message channel alone does not
prove confirmation-free continuation; unknown/user-confirmed deployment modes
retain manual follow-up without advertising unattended readiness.
An Agent already made callable through explicit identity/Endpoint/presentation setup
should be reused as that controller. If its carrier is not ready, use the separate
`agent_continuation_setup` flow. Endpoint rotation, App mount/bind, Host readiness,
Goal Plan presentation and Wake creation are never part of `prepare_goal_workflow`
success. The Agent may remain another Coordinator's Worker/Task assignee at the same
time. Do not infer identity from Window co-location or create a second Goal-only Agent.

The Goal Plan resource is solely `ui://webcodex/goal-plan/v10`, wire version 3. It
renders step counts, current step, bounded milestones, last checkpoint, activity,
and a bounded read-only continuity projection. Continuity keeps production Host
carrier readiness and the exact current Goal-stall Wake lifecycle separate from
bounded Host delivery and exact-consume fresh-turn proof. After newer meaningful work,
the current epoch returns to ready/stalled with no current Wake while the most recent
confirmed resume keeps its bounded Host outcome, fresh-turn proof, and timeline.
Old pre-production Goal resource aliases are not supported. Agent Continuation
remains a separate card and the only Host turn-dispatch carrier.

## Deterministic regression scenarios

The Store tests cover plan bounds, retained stable ids, one current step, atomic validation
before mutation, revision competition, exact/changed keyed replay, reopen,
malformed persisted plans and completion/terminal gates. G6 additionally covers one
transactional Goal + exact Session admission at revision 1, explicit/omitted owned
controller, composition replay conflict, invalid Session identity, foreign controller
existence hiding, and injected correlation/idempotency failures with complete rollback.
Attention migration tests preserve existing Task-terminal facts and Wake identities in
the one current schema.

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

The Goal Plan JavaScript contract tests verify one exact `sync_goal_plan` App RPC,
serial adaptive polling, hidden/visible cadence, teardown, and no conditional second
RPC. Server idempotency remains the durable duplicate-prevention boundary. Sync
calls carry `goal_id` plus an optional adapter-stripped diagnostic `app_call_id`; no browser timestamp, Session, controller, Window
selector or claimed coverage is trusted. There is no Goal Plan `ui/message` path.
Terminal state stops polling.

For the hidden-cadence change, reuse the existing lifecycle tests at their
own boundaries; faster browser polling must not change their authority rules:

| Boundary | Deterministic regression evidence |
| --- | --- |
| Browser callback delayed beyond 75s | `hidden callback after lease expiry sends one sync without catch-up bursts`; `goal_workflow_delayed_hidden_card_requires_new_observation_without_new_epoch` covers 75,001ms and 180,000ms gaps and exact fresh observation before one Wake |
| Agent heartbeat/Endpoint expiry | `input-only background carrier keeps bounded heartbeat cadence`; `agent_listing_projects_only_current_production_continuation_readiness` rejects an expired Endpoint even with a retained local binding |
| Unload/remount | Goal teardown tests; `mcp_app_restart_refresh_recovers_only_same_client_window_without_attachment_shortcut`; old View/binding cannot retarget |
| Server restart | `mcp_app_restart_recovery_preserves_prepared_delivery_unknown_without_redispatch`; exact same-Window restart recovery tests |
| Terminal Goal or Session | `goal_workflow_absent_controller_and_terminal_goals_do_not_create_attention`; `goal_workflow_requires_current_unambiguous_active_session_not_historical_window_link` |
| Controller replacement | `goal_continuity_does_not_retarget_an_existing_stall_wake_after_controller_replacement`; App identity-epoch and replacement tests |
| New meaningful work and incomplete coverage | `goal_workflow_new_meaningful_work_is_required_for_a_new_inactivity_epoch`; `goal_workflow_partial_window_coverage_never_commits_attention` |
| Duplicate/unknown dispatch | Accepted/error/timeout/teardown App tests; `goal_workflow_same_durable_agent_is_worker_and_controller_and_stall_uses_existing_host_fences`; exact-consume and no-redispatch regressions |

These are automated local fixtures, not real ChatGPT observations. The separate
[Host comparison and capture procedure](../implementation/mcp-app-auto-resume-restoration.md#goal-hidden-cadence-baseline-and-candidate-comparison)
requires three 300s trials per build, actual modal/fresh-turn observation and
exact Wake consume before the named real-Host gate can pass.

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
the boundary or during a Wake transition, and 30s while hidden/backgrounded,
shortened by elapsed observation age when necessary to reserve the unchanged 10s RPC timeout within the 75s lease.
An expired attempt backs off to 30s; it does not create a retry burst. Browser
throttling can still exceed the lease. Agent hidden heartbeat remains 15s.
The [baseline/candidate procedure](../implementation/mcp-app-auto-resume-restoration.md#goal-hidden-cadence-baseline-and-candidate-comparison)
requires actual Host observations before claiming improved background continuation.
The Server still checks current Goal/controller/Session/Project authority, latest
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
the exact correlated Session with `read_session_handoff`, and use current
Job/Project truth only as needed. Continue from the latest checkpoint/current step.
A disappeared turn does not establish failure: never replay an uncertain previous
effect merely because it disappeared. Stalled is not offline, heartbeat is not
authority, and continuation is a fresh reasoning opportunity rather than a retry.

## Plan revision acceptance through real ChatGPT MCP

Use an authorized development Runtime build exposing `revise_goal_plan` in its
current manifest and a real ChatGPT chat connected through MCP. This acceptance
uses the chat's own model allowance; do not configure model API/ACP providers or
an agent/model loop. Local deterministic tests and loopback MCP fixtures cannot
prove that the live ChatGPT model follows this workflow. Record the exact build,
chat evidence and tool results; do not report this lane as passed before it ran.

Give the chat this bounded task:

> In a disposable authorized project, explicitly use one durable Goal to write a
> tiny text report from sample data, validate its calculations, and review the
> output. Start with inspect, implement, validate and review milestones. Checkpoint
> inspection before continuing. After inspecting, introduce one extra malformed
> sample and revise the unfinished plan to add a separate input-validation step;
> explain the revision reason and preserve completed inspection. Continue using
> the same Goal and Session. Show the refreshed Goal Plan card. Stop before final
> completion and save enough evidence to recover in another chat.

Observe one exact Goal/Session admission, completed inspection retaining id/title,
a full ordered `revise_goal_plan` at the current revision, new pending validation,
retained status for existing ids, exactly one revision increment, and the prior
snapshot/reason in `get_goal.plan_history`. Repeat the exact same revision request
and key to verify inert replay; changed payload with that key and a fresh-key stale
revision must fail without changing current truth. An attempted completed-step
removal must fail atomically. A second chat with the same canonical authority must
explicitly select/recover the same Session and Goal, see the revised plan through
handoff/card, and continue from current durable state. Fresh validation and review
must precede explicit checkpoints and Goal completion. New revision attempts after
completion must fail; historical exact replay remains read-only. Verify no duplicate
Goal, ACP run, paid model request, task spawn or inferred carrier setup occurred.

For a reproducible real-process transport lane without a model, extend the existing
generic-Agent fixture only through its bounded Goal case:

```sh
python3 scripts/e2e_generic_agent_ws.py --goal-plan-only --profile upgraded \
  --bin-dir /path/to/dogfood --source-version ACTUAL_BUILD_ID \
  --report /tmp/goal-plan-revision-report.json
```

That lane uses disposable real Server/Runner and stateless MCP gateway calls. It
checks replay/conflict/stale/invalid atomicity, history and card/handoff convergence.
After those real protocol observations pass, it checkpoints the revised remaining
steps, explicitly completes the same Goal, verifies the terminal card/current plan,
and rejects a fresh late revision while historical exact replay remains inert. Its
verification/review consists of the fixture's actual protocol/card/handoff assertions;
it makes no source-code validation claim. Four negative tool results are expected:
changed replay, stale revision, completed-step removal and new revision after
terminal completion. It does not establish real ChatGPT model acceptance or
authorize deployment.

Optional bounded timing diagnosis and the ChatGPT acceptance scenario are described in [MCP diagnostics](../MCP.md#goal-and-agent-continuation-timing-diagnostics).
