# Durable Agent runtime and asynchronous work

This document defines the standing product direction for durable Agent identity and
asynchronous work in WebCodex. It builds on the implemented Agent/Conversation/Wake
foundation without turning WebCodex into a generic swarm framework or workflow
scheduler.

Implementation details for the current communication and wake substrate live in
[`durable-agent-conversation.md`](durable-agent-conversation.md). Workflow Session
semantics remain defined by [`../agent/session-model.md`](../agent/session-model.md).

## Product direction

The product goal is a durable Agent entity that can outlive any one model turn,
browser window, host connection, Project, or execution carrier.

A user should eventually be able to:

1. create or select a durable Agent;
2. attach a current window/host to that Agent;
3. communicate through durable Conversations and Inbox Deliveries;
4. leave work for the Agent that remains durable while no model is running;
5. later attach another Endpoint and continue as the same Agent;
6. execute accepted work through whatever authorized carrier is appropriate;
7. inspect durable status instead of inferring progress from browser/UI state.

The model process is not the Agent. A model turn executes **on behalf of** a durable
Agent. Closing a window therefore means that an Endpoint disappeared; it does not
mean that the Agent, its Conversations, or its accepted work disappeared.

This direction may eventually make multi-Agent scheduling possible, but scheduling
is not the product definition. WebCodex should first make Agent identity and work
independent from windows and synchronous model turns. Runnable-frontier scheduling,
capacity management, graph execution, and autonomous delegation remain optional
capabilities that require separate product evidence.

## Names that must not collapse

WebCodex already contains several concepts that use similar words. They are separate
domains and must remain explicit in code, schemas, documentation, and reviews.

| Concept | Meaning | Not interchangeable with |
| --- | --- | --- |
| Runtime Project | Runner-registered execution target addressed as `agent:<client_id>:<project_id>` | durable Agent identity |
| Durable Agent | Server-minted `wc_dagent_*` identity representing who acts/communicates | Runner, browser window, credential, Workflow Session |
| Agent Endpoint | Current Host/Client attachment for one Agent, with lifecycle/generation | Agent identity or work ownership |
| Conversation | Durable communication space | Workflow Session, task queue, execution context |
| Agent Delivery | Recipient-specific Inbox state for one Message | model invocation or accepted work |
| Wake Intent | Durable logical processing opportunity for an Agent | Message, Delivery, Agent Task, execution lease |
| Agent Task | Durable unit of explicitly created/accepted asynchronous Agent work | Workflow Session todo, Conversation Message, Job |
| Agent TaskAttempt | Durable exact execution-ownership attempt for one Agent Task | Endpoint, Workflow Session, CodingAgentRun |
| Workflow Session | Existing execution/provenance/validation/handoff evidence context | Agent, Conversation, Agent Task |
| Job | Concrete long-running process/validation execution | Agent Task or TaskAttempt |
| CodingAgentRun | Existing ACP delegated coding execution | Agent Task itself |
| Goal | Server-owned high-level durable intent/control state (`wc_goal_*`) | Agent Task, Workflow Session, Job, Project authority, scheduler |

The `agent:` prefix in a runtime Project id is historical Runner-address syntax. It
is unrelated to the durable `wc_dagent_*` Agent identity domain.

In implementation names, use `AgentTask` / `AgentTaskAttempt` for the asynchronous Agent work domain. Ordinary coding work uses Workflow Session and Job terminology; do not introduce an unqualified new `Task` type where ownership would be ambiguous.

## Implemented durable Agent foundation

The current implementation already establishes these boundaries, the A2.5 natural-conversation path, and the A3 durable AgentTask/TaskAttempt ownership substrate:

```text
Durable Agent
    |
    +-- Agent Card / profile
    +-- Endpoint attachments + controller generations
    |     +-- process-local exact Host adapter binding
    +-- Conversations
    |     +-- append-only Messages
    |     +-- recipient Deliveries / Inbox
    |
    +-- Wake Intents
    |     +-- Endpoint/generation-bound Wake Delivery Attempts
    |     +-- bounded event-driven continuation scheduling
    |
    +-- Agent Tasks
          +-- explicit durable assignee
          +-- fenced, leased TaskAttempts + Attempt-local controller generation
```

Important current invariants:

- `agent_id` is Server-minted stable identity; Agent Card metadata grants no
  execution authority.
- Endpoint identity is replaceable and principal-bound; lease validity and
  controller generation fence stale attachments.
- Conversation participation grants communication authority only.
- Message, Delivery, Wake Intent, and Wake Delivery Attempt are independent durable
  facts.
- Message/Delivery creation and required Wake coalescing are transactional.
- communication-resource authorization hides existence from unauthorized exact-id
  probes.
- generic database open is storage-only; standalone Server takeover recovery runs
  only after explicit Server instance ownership is acquired.
- dispatch uncertainty after the Wake dispatch fence is preserved rather than
  blindly retried.
- one authenticated communication principal can create/select/update multiple
  durable Agents, explicitly attach different Host windows, and communicate through
  exact-recipient Conversations without making the window the Agent.
- exact Agent/Endpoint/generation bootstrap exposes only bounded IDs, counts,
  high-watermarks, Wake hints, and truthful current adapter capability; authoritative
  Inbox and Conversation reads remain separate. An already-active explicit turn can
  idempotently accept one pending Wake through an `explicit_activation` Attempt and
  recover the same consume token without pretending it requested a new model turn.
- bounded Agent listing exposes `production_auto_resume_available` only as the
  conjunction of the listed current durable Endpoint/generation state and a production
  Host carrier in this Server process. Together with `active_endpoint_count`, this lets
  a coordinator distinguish a current Endpoint without a carrier from no current
  Endpoint. It is not idle/model liveness, presence, capacity reservation, execution
  authority, or guaranteed/immediate Host scheduling; restart clears the process-local
  side until an ordinary fenced rebind/recovery succeeds.
- automatically resumed replies can derive stable replay identity from exact Wake
  plus a bounded operation index, closing the reply-committed/response-lost window
  without merging Wake and Delivery consumption.
- Agent Tasks are independent durable work truth with explicit assignment; Conversation
  Message/source references and Project references are correlation only.
- each execution-ownership retry creates a durable TaskAttempt with a distinct opaque
  Attempt fence; lease expiry never transfers assignment and cannot be revived by the
  same Agent.
- carrier replacement within one TaskAttempt increments an Attempt-local controller
  generation without creating another Attempt; older generations are stale.
- AgentTask creation, Attempt start, and terminal completion use durable keyed replay
  in the same authoritative SQLite transaction as their effects, and Task/Attempt,
  fence, generation, lease, terminal result, and accepted replay identity survive
  database reopen/Server restart.
- A3 does not bind or dispatch an execution backend: starting a TaskAttempt does not
  create a CodingAgentRun, Job, Workflow Session, Wake, or Host callback.
- process-local Host bindings are empty after restart; offline Messages, Deliveries,
  and the same logical Wake remain durable. A fresh/replacement exact Endpoint may
  register a carrier. An MCP App may recreate only its lost local binding when the
  exact v11 binding fingerprint still matches, or when the authenticated principal,
  current Endpoint/generation, and durable canonical ClientWindow all match. The
  latter permits a refreshed iframe to use a new process-instance binding fence
  without granting continuity to another Host window.
- project-scoped Memory is unchanged; Agent-scoped Memory is only a future boundary.
- durable Goals are independent high-level intent/control truth; Goal identity, ownership, lifecycle, revision, and correlations grant no Project, Runner, filesystem, Workflow Session, AgentTaskAttempt, CodingAgentRun, or Job authority.

G3 adds an optional production ChatGPT MCP App Host carrier on top of this substrate. It is deliberately a pull bridge rather than a fake Server callback: one explicit `present_agent_continuation(agent_id, endpoint_id, expected_controller_generation)` card binds the current `ui://webcodex/agent-continuation/v16` resource, while ModelHidden app-only operations establish one exact process-local View binding, renew the exact Endpoint, acquire through the existing Wake claim state machine, cross the existing durable dispatch fence, and record Host dispatch acceptance or uncertainty. The View itself performs `ui/message` only after prepare succeeds. The process-local `binding_id` fences the current iframe instance but grants no authority and is not durable execution truth. The Store retains its identity-bound SHA-256 fingerprint as the no-Window restart fallback plus an optional canonical hashed ClientWindow key as Host-window continuity; raw Host session metadata is never persisted. Natural Endpoint expiry is a separate bounded path: only the same authenticated principal + durable ClientWindow may ask the App-only recovery operation to atomically replace the exact expired generation with its immediate successor. v16 retains v15's independently idempotent predecessor recovery edges and at-most-eight exact `generation+1` successor hops; an old selector never skips directly to a later generation.

Every bridge operation re-runs ordinary communication authorization and exact Agent/Endpoint/controller-generation validation. Ordinary push bindings still require an Endpoint freshly attached in the current Server process. For MCP Apps, successful bind persists the current identity-bound recovery fingerprint and the optional canonical ClientWindow key already derived by the protocol adapter. Server takeover clears process-local bindings and `wake_capable` but preserves both. With no local binding, the exact fingerprint remains sufficient; when a canonical Window is present, the same principal + exact current Endpoint/generation + same Window may also receive the normal `success=true` recovery projection and create a new iframe fence after refresh. Exact unbind clears the current fingerprint but preserves a matching Window key; natural expiry also preserves only that Window key for the dedicated expired-Endpoint replacement operation. Explicit detach (including after expiry), ordinary Endpoint replacement, and push transition clear both recovery values. Replacement replay re-checks the successor's retained Window key, so a historical replay record cannot undo that revocation. Only a newly committed replacement populates `attached_endpoints`; replay and restart recovery never recreate fresh push-attachment authority. Missing or malformed Window metadata grants nothing beyond the exact fingerprint fallback, and another Window, stale generation, expiry, detach, foreign principal, malformed result, or generic bridge failure remains fail-closed. The strict published continuation projection schema still requires `recovery` on every result (`null` normally, the sole fixed restart-loss object when recoverable), so Host schema projection cannot discard the observation. Replacing or withdrawing a View reuses existing Wake reconciliation: a pre-fence claim is revoked and the logical Wake returns to `pending`, while a prepared/delivered Attempt becomes `delivery_unknown`. The App never blindly resends after the dispatch fence. Host `ui/message` success means only `dispatch_accepted`; only later exact `consume_agent_wake` proves that a continuation model turn actually ran. A consume-before-ACK race is valid and late ACK is idempotent. v16 keeps the slower bounded heartbeat cadence while hidden but allows the same acquire -> prepare -> `ui/message` -> finish path in background. Visibility transitions are scheduling observations only: they do not by themselves create `delivery_unknown`. Host scheduling remains best effort/non-immediate, and correctness still depends on the durable Wake and exact consume rather than timer liveness.

For a new ChatGPT durable-Agent window, the canonical setup sequence is `create_agent_identity -> rotate_agent_continuation_endpoint -> present_agent_continuation -> yield/end the current model turn`. Presentation can return before the MCP App has established its process-local Host binding, and the presenting model turn has no authoritative “card fully mounted” signal. Do not present the card and then continue a long coordinator/business turn while assuming wake readiness. On a later turn, `list_agent_identities.production_auto_resume_available` is the bounded readiness observation for the exact current generation. ClientWindow continuity remains recovery input, not a correctness or model-liveness signal.

Runtime Console, explicit activation, and push `ContinuationAdapter` behavior retain their existing contracts. The MCP App is an optional carrier, not a scheduler or a source of Agent, Task, Goal, Project, Workflow Session, or execution authority. Ordinary WebCodex Jobs are not materialized as MCP Tasks. See [`../agent/mcp-app-continuation-experiments.md`](../agent/mcp-app-continuation-experiments.md) for the Host evidence and production mapping.

These invariants, the natural-conversation slice, and the durable A3 ownership
substrate support asynchronous Agent work without introducing a scheduler.

## Durable Goal workflow — G4/G6/G7

Goal is high-level **durable workflow/progress truth**: fixed bounded completion
intent and mechanical milestones, an explicit controller routing identity, and an
`active | completed | cancelled` lifecycle. Workflow Session is the concrete
current-work context: Project, validation, Jobs, provenance, and recovery. Task /
TaskAttempt remains the owner of concrete execution. None replaces another.

The workflow is WebCodex-owned, not a repository convention. The startup brief's
`webcodex.workflow.model_protocol`, canonical tool descriptions, and
`single_window_goal_workflow` recommended flow direct ordinary substantial
multi-step or cross-turn development, fixes, refactoring, troubleshooting,
deployment and migration through one canonical Goal lifecycle. On explicit exact
Workflow Session re-entry, `work_on_project` may return owner-scoped sparse
`goal_context` for active Goals already correlated to that Session. One candidate
can be explicitly re-read/reused; multiple candidates remain a bounded explicit
choice. The projection never auto-selects, binds, or completes a Goal and never
infers identity from Project, Window, title, recency, or Session correlation itself.
Unavailable evidence is not proof of zero active Goals. With no reusable active Goal
context, ordinary new substantial work uses the canonical atomic Goal admission.
Tiny one-step lookups and trivial edits do not mechanically create one. Repository
`AGENTS.md` continues to express repository-specific rules; it does not decide
whether Goal workflow exists. Existing exact Goals can still be composed explicitly
through the lower-level Goal primitives; admission never heuristically selects a
recent Goal.

```text
ordinary new substantial work
  -> work_on_project -> exact Workflow Session
  -> prepare_goal_workflow(session_id, completion_conditions, steps, controller_agent_id?)
       # one durable admission: new Goal + exact Session correlation
  -> present_goal_plan
  -> work; checkpoint at recovery-worthy boundaries
  -> fresh verification/review
  -> complete remaining plan steps; explicitly update_goal(completed)

normal exact-Session continuation
  -> work_on_project(session_id=exact)
  -> optional sparse goal_context
       # 1 active Goal: explicitly get_goal / present_goal_plan and reuse it
       # >1 active Goals: explicitly choose; no latest/Window/Project inference

optional automatic continuation
  -> reuse one exact explicit durable controller Agent
  -> if its carrier is not ready, use the separate agent_continuation_setup flow
```

`prepare_goal_workflow` is deliberately Host-neutral. ToolRuntime first calls the
existing exact Session authorization path, which rechecks the immutable Session
creation-authority fingerprint and any bound Project authorization. Only after that
succeeds does the Store enter one `BEGIN IMMEDIATE` transaction. The transaction
validates the bounded Goal input and optional exact owned controller, checks the
composition-specific keyed replay identity, allocates and inserts the Goal at
`revision = 1`, inserts exactly one `workflow_session` correlation without a synthetic
revision bump, records the prepare idempotency fact, then commits. Any failure rolls
back all three durable writes. Exact same-key replay returns that same admitted Goal
without mutation; changed reuse fails closed.

That transaction contains no MCP, MCP Apps, `ClientWindow`, iframe, Endpoint, Host
binding, `ui/message`, Wake, or presentation state. `present_goal_plan` and Agent
Continuation are separate adapters after durable admission; neither App mount nor
`production_auto_resume_available` is part of prepare success.

A single `wc_dagent_*` can simultaneously be a callable Task assignee and controller
of one or more Goals. There is no Worker/Goal Agent type, role enum, second identity,
or Window role. Reuse an existing Agent only when its identity is established by
explicit durable setup or exact Wake context, never because it appears in the same
Window. Goal controller routes attention/next reasoning; Task/Attempt determines
execution ownership. Agent identity, Goal correlation and Window continuity grant
no Project, Runner, Session, Job, TaskAttempt or Host authority.

### Mechanical plan and checkpoint

The existing Server SQLite database holds `wc_goals`, explicit
`wc_goal_correlations`, and operation-scoped `wc_goal_idempotency`. One bounded
`plan_json` column has one current strict encoding; an additive migration supplies
an empty plan to existing Goals. There are no dual writes, legacy request-hash
branches, alternate plan mutations or schema adapters.

```text
Goal
  goal_id, owner principal
  title <= 200 characters; objective <= 8192 UTF-8 bytes
  controller_agent_id?                  # explicit independently owned Agent
  lifecycle, revision, created_at, updated_at, terminal_at?, terminal_reason?
  correlations[] <= 64                 # exact AgentTask / Workflow Session
  plan
    completion_conditions[] <= 8       # each 1..512 UTF-8 bytes
    steps[] <= 32
      id                              # stable 1..32 ASCII [A-Za-z0-9_-]
      title                           # 1..120 characters
      status                          # pending | in_progress | completed
      updated_at_unix_ms
    progress_summary?                 # 1..2048 UTF-8 bytes
    checkpoint_at_unix_ms?
```

The full encoded plan is at most 32 KiB. Conditions and step structure are fixed at
creation; all steps initially are pending. Natural-language conditions are durable
completion intent, not predicates the Server claims to evaluate.

`checkpoint_goal(goal_id, expected_revision, idempotency_key,
completed_step_ids?, current_step_id?, summary)` is the sole progress mutation.
It validates the whole batch before changing anything and commits plan, summary,
checkpoint time, exactly one revision increment and replay identity in one
IMMEDIATE transaction. Pending steps may become in-progress or completed;
in-progress steps may become completed; completed steps never regress. At most
one step is in-progress, so advancing to another requires completing the previous
one in the same batch. Unknown, duplicate, invalid or contradictory selectors fail
closed. Stale revisions reject the write. Exact keyed replay is read-only (including
historical replay after terminalization); changed reuse conflicts. Every new-key
checkpoint is one new recovery point, even when its selected steps are unchanged.

Persisted plans are bounded before decoding and validate the sole field shape,
unique ids, closed status values, timestamp/checkpoint consistency and single
current-step invariant. Corruption fails closed for read/list/mutation and attention
source validation. A Goal with incomplete steps cannot become completed. Terminal
Goals cannot accept new checkpoints, metadata changes or correlations. Fresh
verification/review and judgment of completion intent remain explicit model work.
`session_handoff_summary` is for genuine missing-context/cross-turn recovery, not
routine checkpoint bookkeeping.

Goal tools do not gain Project execution selectors. Explicit Session association
independently checks immutable creation authority and the Session's Project;
Task association independently authorizes its exact Task. Later dereferences must
repeat their own domain authorization. Goal/Session correlation is never guessed
from Project, Window, credentials, Conversation or recent activity.

`finish_coding_task` does not complete a Goal. Its compact and detailed closeout
return a bounded `goal_follow_up` when the independently authorized exact Session
has active Goals explicitly correlated under the caller's Goal owner. At most eight
entries contain only exact Goal id/revision, incomplete count, current step id/title
and canonical `checkpoint_goal` or `update_goal` next action. No match omits the
field; unavailable durable evidence is explicit and is not presented as no active
Goal. Additional Goals are marked truncated. No Goal id is added to ordinary
coding tools, and no Goal lifecycle is inferred from Session closeout.

### Goal Plan progress projection and inactivity detector

The sole current Goal Plan resource is `ui://webcodex/goal-plan/v6`; its sole current
wire version is **3**. Older pre-production Goal resource aliases are not served.
Goal Plan deliberately advances its canonical resource URI whenever its shipped View
or App-tool wire changes: production Hosts may retain a same-URI View across Server
deploys, so resource identity—not an assumed refresh—is the cache/version fence.
`present_goal_plan` is the only model-visible App-bound presentation tool.
`goal_plan_sync` is the single App-only effectful observation/synchronization primitive.
It accepts only `goal_id`, re-authorizes on every call, may commit one authoritative
stall Attention/Wake under the existing fences, and returns the final post-reconciliation
projection in the same RPC.
The card projects title, lifecycle/revision, controller identity, step counts,
bounded steps/current step, checkpoint summary/time, derived activity, and bounded
continuity observation. Continuity separates current production carrier readiness
and the exact current Goal-stall Wake lifecycle from bounded continuation outcome
evidence. While a current Wake exists, Host delivery and fresh-turn proof describe
that Wake. After exact consume followed by newer meaningful work, the current state
returns to ready/stalled with no current Wake, while Host delivery, fresh-turn proof,
and the bounded timeline retain the most recent confirmed resume. It omits
objective/conditions, Task bodies/results, private Project paths, correlation
identities, Session ledger, Endpoint bindings, tokens/fences, stdout/stderr and full
history. The presentation envelope is bounded to 40 KiB including metadata.

`goal_plan_sync` is globally ModelHidden and App-only on an eligible Stateless
MCP 2026 surface. Its public contract is intentionally **effectful**
(`Mutate / WorkflowManage / DesiredState`), never disguised as a pure read; a
kernel capability gate remains the final backstop for non-App transports. It is still
**Transport / NonMeaningful**: synchronization records that the Window was seen,
never that business work advanced. A successful sync records only its exact Goal
selector in the existing ActionAudit identity projection; it cannot make another
Goal's card appear alive. Window context is trusted Host/runtime sideband; client
timestamps, Session/controller selections, claimed active-request counts and claimed
coverage are not accepted.

The existing Window activity registry and ActionAudit ledger remain the only
observation machinery. Meaningful completion rows and the newest observation are
read separately with bounded result pages, so thousands of transport polls cannot
evict the last meaningful-work anchor. Goal-to-Session discovery (16), correlated
Windows (16), meaningful rows (64 per Window), visibility checks and active-request
coverage stay bounded. Missing/partial/ambiguous or currently unauthorized evidence
fails closed. The soft UI states remain `active`, `attention_needed`, `unobserved`,
and terminal `not_applicable`. Running meaningful requests keep activity active.
Five minutes of known quiet can warrant attention, but **stalled is not offline**:
Host scheduling, inference, user work or other connectors may explain the interval.

`attention_needed` alone is not auto-resume eligibility. During the same
`goal_plan_sync` request the Server independently rechecks all of the following:

- owned active Goal, exact explicit owned controller Agent, and current
  communication read/manage, runtime read, Session collaborate and Project read;
- explicit Goal/Session correlation and normal Session/Project authorization;
  the current Window's latest explicit durable Session link must be that exact
  Session, without equal-time ambiguity;
- complete authoritative observation; no active meaningful request under the
  principal, including a request that started while asynchronous checks ran;
- last meaningful completion at least **300,000 ms** ago; a successful exact-Goal
  card observation at least **1,000 ms** later than that completion, no future
  timestamp, and within the Server-owned **75,000 ms observation lease**. The lease
  covers the App's 60s hidden/background cadence plus 15s scheduling slack; browser
  timestamps are never accepted;
- still-active exact Session and creation-authority fingerprint, matching Goal
  revision/controller/correlation and unchanged durable meaningful-work anchor.

Request completion stays registered as active until its ActionAudit completion is
recorded. A missing/failed completion record creates a bounded coverage gap in the
existing registry, not a second timer. A later fully recorded meaningful request
in that same principal/Window can clear it. The existing registry's monotonic
process-local synchronization revision and lock linearize the attention transaction
against request start/finish. The existing Session lock fences active lifecycle and
creation authority through that synchronous SQLite transaction; no async dispatch
runs under either lock. The transaction recomputes persisted evidence rather than
trusting a browser clock or a stale async snapshot. A closed/disappeared card or
unobserved Window never triggers an automatic turn.

A missing outer `recording_session_id` remains durable forensic truth in ActionAudit.
It is not, however, a Goal-liveness coverage hole when that exact successful MCP
action already carries canonical business-Session evidence for the same Goal Session
and Project: the typed ToolCall has independently passed business Session authority
and lifecycle checks, and MCP persists only that exact `business_session_id` as a
bounded internal audit id. Different/missing Session evidence, Project mismatch,
malformed audit ids, and unrecorded calls remain fail-closed. ClientWindow/_meta is
used only to correlate the observation; it never selects or authorizes a Session,
and this does not create a sticky recorder or backfill the Session ledger.

### Narrow Goal workflow stall attention and fresh-turn recovery

An eligible sync atomically creates one `goal_workflow_stalled` Event and one
ordinary pending `attention_event` Wake for the exact controller. The Event binds
Goal, selected Agent, exact correlated Workflow Session, canonical hashed Window,
observation principal, Goal revision and observed timing. It contains no copied
Goal objective, Task instruction/result, transcript or credentials. The Event table
has exactly two closed source kinds: this and existing `agent_task_terminal`.
There is no generic EventBus, periodic scanner, Goal clock, cron, scheduler or Task
spawner.

The inactivity epoch is `(Goal, last meaningful completion)`, not Goal revision,
View identity, selected controller or repeated poll time. A unique durable index and
one transaction enforce one fact/one logical Wake, across retries and database
reopen. Replays return original identities and do not schedule again. A subsequent
epoch requires strictly newer meaningful work **after the previous attention fact**;
adding older historical evidence through another Session cannot mint an epoch.
Event/Wake corruption fails closed. No sliding deadline or consume/dispatch action
resets the epoch.

```text
Goal Plan sync -> authoritative observation + fenced reconciliation in one RPC
 -> atomic goal_workflow_stalled Event + controller Wake
 -> existing AgentContinuationController / Agent Continuation MCP App
 -> existing claim + prepare + dispatch fence -> ui/message
 -> new model turn: bootstrap exact Wake -> immediately consume exact Wake
 -> get_goal -> exact session_handoff_summary -> continue latest checkpoint
```

Goal Plan never calls `ui/message`; the cards remain separate. If the Goal controller
has no production Host carrier, the Wake remains durable/pending. The existing
`production_auto_resume_available` means exact Agent/generation Host carrier
readiness only, never worker idleness, model-turn liveness, progress or a lease.
`dispatch_accepted` does not prove resume; `delivery_unknown` cannot cause a duplicate
same-epoch resend. Only exact Wake consume proves that the fresh turn ran.

The deterministic stall message stays under 1,000 characters and directs bootstrap,
immediate exact consume, exact Goal/plan and Session recovery, current authorized
Job/Project reads as needed, then checkpointed continuation. A vanished previous
turn does not prove failure: **never repeat an uncertain effect merely because the
turn disappeared**. Events remain immutable historical facts if work resumes,
controller metadata changes or a Goal later becomes terminal; routing history is
not new execution authority. The resumed model re-reads current truth and stops for
a terminal Goal or changed controller rather than reopening or retargeting it. TaskAttempt leases, existing Goal-scoped
ANY/ALL Wait precedence and Host delivery uncertainty remain independent.

See [Goal workflow continuity dogfood](../agent/goal-workflow-continuity-dogfood.md)
for deterministic coverage and the distinct manual production-Host boundary.

**G3 — production Host continuation adapter.** G3 is now implemented for explicit Durable Agent Endpoints, independently of Goal. The public `present_agent_continuation` entry requires exact `agent_id`, `endpoint_id`, and `expected_controller_generation`; it does not infer a target from Goal, Workflow Session, Project, ClientWindow, credential, recent Agent, recent Task, or any other ambient state. The associated App bridge is available only on an App-enabled Stateless MCP 2026 request, where its hidden tools are projected with `ui.visibility = ["app"]`; they remain absent from the ordinary model universe, legacy MCP, REST/generic runtime, and Adaptive gateway targets, with a kernel protocol-capability gate as the final backstop.

The durable lifecycle remains exactly the pre-existing Agent Endpoint / Wake / Wake Delivery Attempt lifecycle. The App View is only a live Host controller/carrier: `bind` establishes one current process-local View fence, `state` heartbeats and renews the exact Endpoint, `wake_acquire` delegates to the existing claim path, `wake_prepare` crosses the existing durable dispatch fence and revalidates exact binding, and `wake_finish` records only accepted/unknown Host dispatch outcome. Claim fence and binding secret are not durable/model-visible truth; the automatic message carries the exact consume token only through App-private MCP result metadata, not ordinary structured model content or audit/trace payloads. `dispatch_prepared`, `dispatch_accepted`, and `dispatch_unknown` are bounded Host observations, not new authoritative Wake states. Only exact durable `consume_agent_wake` establishes `continuation_consumed`.

G3 by itself remains independent of Goal identity: presenting/binding a continuation card never selects a Goal. Explicit Goal-to-carrier bridges are the narrow workflow-stall attention above and Task-terminal attention below. Goal remains `active | completed | cancelled` high-level Control truth and `present_goal_plan` remains read-only presentation. No Goal revision/completion or next AgentTask is inferred from Host state, ClientWindow, credential, Project, Workflow Session, or ambient activity. These boundaries follow the [September 11–13, 2026 MCP App continuation findings](../agent/mcp-app-continuation-experiments.md): one sparse card converges from authoritative state, Host acceptance is not model resumption, and background Host scheduling is not an immediate guarantee.

The View starts binding only after successful Host initialization. Visibility selects only the bounded poll cadence; both foreground and hidden Views may continue the exact acquire/prepare/dispatch path. A visible/hidden transition while acquire, prepare, or Host dispatch is in flight neither cancels that path nor creates uncertainty. State refresh follows the current unresolved Wake after exact consumption, and never copies an older Attempt's dispatch phase onto its successor. The previous claim remains available for late ACK reconciliation until acquire takes the next Wake. The View finishes any pending ACK retry before acquiring a successor and retains only one Attempt's retry markers.

### Goal-correlated terminal attention

The first asynchronous Goal orchestration loop is deliberately narrower than a generic event system. When and only when the exact current `AgentTaskAttempt` successfully terminalizes, the same authoritative SQLite transaction checks every caller-owned **active** Goal currently correlated to that Task. For each such Goal it resolves the routing target from that Goal's explicit `controller_agent_id`; when the field is absent, the existing Task-terminal attention rule uses the Task's explicit assignee. This Task-specific rule is not used for workflow-stall attention. The exact selected target Agent is re-checked as owned by the same principal before the transaction may persist exactly one `agent_task_terminal` attention Event and one pending `attention_event` Wake. `(kind, goal_id, task_attempt_id)` is unique, and the existing completion idempotency record is committed in the same transaction, so a crash cannot leave the Task terminal without a required attention fact and exact completion replay cannot fan out duplicates. A failed routing authorization or Event/Wake insert rolls the Task/Attempt terminal transition back too.

The Event is a durable semantic fact, not an authority snapshot or pub/sub payload. It stores only exact owner/selected target Agent identity, `goal_id`, `task_id`, `task_attempt_id`, terminal Task state, kind, and creation time. It deliberately omits Goal objective, Task instruction/result/error bodies, Session transcript, Project/Runner credentials, Attempt fence, Endpoint id/controller generation/private Host binding, and arbitrary JSON. The selected target is historical routing truth: changing `controller_agent_id` later does not retarget or duplicate an already-created Event/Wake, and the old Wake still grants no source-domain authority. Multiple active Goal correlations produce one bounded deterministic Event/Wake per Goal; new correlations are rejected once the same owner already has `MAX_GOAL_CORRELATIONS` (currently 64) active Goals pointing at that exact AgentTask, so a newly admitted Task cannot accumulate an unbounded terminal-attention fan-out. A terminal Goal is excluded at Event creation, while a Goal that becomes terminal afterward does not invalidate or retarget the historical Event.

The `attention_event` Wake is a third source beside `inbox_changed` and `agent_task_attempt`. It references only `source_event_id`; bootstrap derives bounded exact `event_id`, `goal_id`, `task_id`, and `task_attempt_id` after re-authorizing and validating the Event against authoritative Goal/Task/Attempt rows. Unlike `agent_task_attempt`, attention originates **after** the Attempt is terminal, so it never requires an active Attempt lease, Attempt heartbeat, or Attempt controller takeover. It still uses the normal Agent Endpoint/generation/binding/claim/prepare/dispatch fences and shares the global one-dispatched-Wake-per-Agent bound with every other Wake source. Missing Host/Endpoint capacity leaves the durable Wake pending and does not fail Task completion; `schedule_agent` is only a process-local best-effort hint.

The resumed automatic message is source-specific: it tells the model to bootstrap and exact-consume this Wake, then independently call normal `get_goal(goal_id)` and `read_agent_task(task_id)` before deciding what to do. Event/Wake identity grants no Goal, Task, Project, Runner, filesystem, Conversation, or Workflow Session authority. If another turn has already completed/cancelled the Goal, the resumed turn observes that terminal truth and no-ops rather than reopening it. If the Goal is still active, the model may explicitly update/complete/cancel it, create and associate a next AgentTask, associate a relevant Workflow Session, or report/wait on a real blocker. The Server never auto-completes the Goal and never auto-generates the next Task.

## Asynchronous Agent work

An **Agent Task** represents durable work that has been explicitly accepted or
created for later completion. It answers:

> What work remains to be completed?

It must survive model-turn completion, Endpoint detach, browser closure, Server
restart, and temporary absence of an execution carrier.

An Agent Task is not created merely because a Conversation Message exists. A Message
is communication; converting or accepting a request into work must be an explicit
durable transition. An Agent Task may retain stable origin references such as `conversation_id` and
`source_message_id`, but the Message body remains owned by the Conversation domain.

A3 does not define a global work-stealing queue. Before an Agent Task can create an
Attempt, it has an explicit current assignee Agent established by Task creation,
acceptance, or a separate authorized reassignment transition. An unassigned Task may
exist if a real UI/workflow needs it, but it is not claimable by arbitrary Agents.
Changing the assignee is an explicit durable mutation; lease expiry alone never
silently transfers work to a different Agent.

Likewise, an Agent Task that references a Project does not receive Project authority.
The Project reference identifies where work may need to occur; every actual execution
still passes the normal Project/Runner/filesystem/permission checks.

A first Agent Task model should stay deliberately small. Candidate durable fields
are:

```text
AgentTask
  task_id
  creator / owner principal attribution
  assignee_agent_id?  # must be explicit before creating an Attempt
  bounded title / instruction
  source_conversation_id?
  source_message_id?
  referenced_project_id?
  state
  created_at
  updated_at
  terminal result / reason references?
```

The exact state enum should be chosen by the implementing slice, not expanded in
advance for hypothetical graph execution. The first slice needs only enough states
to distinguish available/active work from terminal success/failure and any explicit
cancellation that is actually required.

## TaskAttempt is the execution ownership unit

An Agent Task does not record a mutable `claimed_by` field that is repeatedly reused
across retries. Each concrete attempt is a separate durable **Agent TaskAttempt**.

```text
AgentTask
   |
   +-- TaskAttempt 1 -> expired / failed
   +-- TaskAttempt 2 -> failed
   +-- TaskAttempt 3 -> completed
```

A TaskAttempt answers:

> Who owns this exact execution attempt, and which generation of that attempt is
> still allowed to make progress?

Candidate durable fields are:

```text
AgentTaskAttempt
  attempt_id
  task_id
  attempt_number
  assignee_agent_id
  state
  lease_expires_at
  attempt_fence
  attempt_controller_generation
  workflow_session_id?
  execution_kind?
  execution_ref?
  created_at
  started_at?
  terminal_at?
```

The Attempt belongs to the durable Agent named by the Agent Task's current explicit
assignment, not to a browser window. An Endpoint or other execution carrier is only
a current way of executing that Attempt.

### Two different stale-execution fences

Attempt retry and carrier replacement are different events and need different
fences.

**Attempt fence** separates one TaskAttempt from a later TaskAttempt:

```text
Attempt 1 (fence A) -> lease expires
Attempt 2 (fence B) -> becomes current

late heartbeat(A) -> stale
late complete(A)  -> stale
late fail(A)      -> stale
```

The same Agent reclaiming work after Attempt 1 expires creates Attempt 2. Matching
`agent_id` must never revive an expired Attempt.

**Attempt controller generation** separates execution carriers inside the same Attempt:

```text
Attempt 2
  Agent A
  attempt_fence = B
  attempt_controller_generation = 4
       |
       +-- old carrier binding / attempt generation 3 -> stale
       +-- replacement carrier binding / attempt generation 4 -> current
```

Carrier replacement therefore does not necessarily create a new Attempt, but the
old carrier must lose the ability to heartbeat, dispatch, or submit a terminal
result. `attempt_controller_generation` is Attempt-local freshness metadata, not a
credential or a grant of Project authority. It is distinct from the existing Agent
Endpoint controller generation. When the carrier is an Agent Endpoint, the binding
must also preserve and validate the exact `endpoint_id` plus that Endpoint's own
controller generation.

If an execution backend already has a stronger exact identity (for example a
CodingAgentRun `run_id` plus provider-instance fencing), bind and revalidate that
identity rather than weakening it into a generic controller token.

## Attempt authority and lease semantics

Possession of an Attempt id, fence, lease timestamp, Agent id, or Attempt controller
generation is never sufficient authority by itself.

The intended admission rule is conceptually:

```text
normal caller authorization
+ authorized Agent Task visibility / operation
+ exact active task_id + attempt_id
+ exact assignee_agent_id
+ unexpired Attempt lease
+ exact opaque attempt_fence
+ current attempt_controller_generation when a replaceable carrier is bound
+ normal Project / executor authority for consequential execution
```

`attempt_fence` is conflict-detection/freshness metadata, not a bearer credential.
A lease that is already expired cannot be renewed by the stale owner and cannot
submit completion. A new authorized start/claim by the current assignee after expiry
creates a new Attempt.

Planner or scheduler output, if one is added later, is advisory. In A3, assignment
is explicit rather than selected by a scheduler. The authoritative Attempt
start/claim and dispatch mutations must re-check current Agent Task state, current
assignee, Attempt state, lease, Project/executor authority, and any dependency rules
that eventually exist.

## Idempotency, replay, and uncertainty

Agent Task mutations should follow the existing WebCodex durable patterns:

- create-like operations use caller-generated idempotency keys plus canonical
  request fingerprints;
- exact uncertain retry returns the original Agent Task/TaskAttempt/result;
- reusing one key for changed intent fails closed;
- duplicate terminal completion is exact replay, not a second side effect;
- expired Attempt heartbeat/completion is rejected even when the Agent is the same;
- once a later Attempt exists, an older Attempt remains permanently stale;
- restart restores durable Task/Attempt truth rather than inferring ownership from
  process-local state.

Execution dispatch has a separate uncertainty boundary. If a backend dispatch may
have happened, the TaskAttempt must preserve that ambiguity and reconcile the exact
execution rather than minting a replacement execution blindly. Existing
CodingAgentRun `outcome_unknown` / provider-instance semantics are the preferred
first backend precedent.

## Communication, Wake, Task, and execution are separate

The expected relationship is:

```text
Human / Agent
    |
    v
Conversation Message
    |
    | explicit accept/create work
    v
Agent Task
    |
    | exact claim
    v
Agent TaskAttempt
    |
    +--> CodingAgentRun
    |       or
    +--> Agent Endpoint continuation
    |       or future concrete backend
    v
Workflow Session / Job / backend execution evidence
    |
    v
exact Attempt terminal transition
    |
    v
Agent Task result
    |
    v
Conversation reply / durable notification
```

The arrows are correlations, not authority inheritance.

A Wake Intent says that an Agent should receive another processing opportunity. It
is not an Agent Task claim and does not mean work completed. Consuming a Wake does
not consume Agent Task work. Consuming Inbox Deliveries does not complete an Agent
Task. An Agent Task may remain durable while the Agent has no wake-capable Endpoint.

A Workflow Session remains the execution/provenance/validation context for concrete
work. It does not become the Agent Task identity. A Session todo remains a useful
manual coordinator/worker assignment primitive, but it is not silently upgraded
into an Agent Task.

## Authority and privacy boundaries

Agent entity work crosses communication and execution domains, so the boundary must
remain explicit from the first Agent Task slice.

Standing rules:

- Agent identity or Agent Card metadata grants no execution authority.
- Conversation membership, authorship, mention, Delivery, or Wake possession grants
  no Agent Task execution authority.
- Agent Task assignment grants no Project, Runner, filesystem, Job, Workflow
  Session, Computer, or CodingAgent authority.
- An Agent Task's Project reference grants no access to that Project.
- Claim/dispatch/completion re-authorize their owning Agent Task domain and any
  actual Project/executor target at the consequential boundary.
- Agent Task visibility, Agent Task management, assignment/claim, and underlying execution
  authority are conceptually distinct even if the first implementation can safely
  reuse a smaller closed scope set.
- exact Task/Attempt lookups should follow the existing authorization-result privacy
  discipline: an unauthorized caller must not use guessed opaque ids as an
  existence oracle.
- generic audit/telemetry should keep ids, state, counts, generations, and bounded
  metadata, not duplicate full Agent Task instructions, Conversation bodies, secrets, or
  opaque fences/tokens.

Do not name new scopes merely to make the model look symmetric. Introduce a scope
only when the actual first Agent Task surface creates a distinct authority audience.

For the first A3 slice, Agent assignment must not manufacture a new bearer
principal. Task creation/assignment/start/heartbeat/completion run under the current
authenticated caller and must prove that caller may operate the exact Task and its
current assignee Agent in the Task domain. A simple first implementation may reuse
the Agent's existing owner communication principal when that is the actual product
audience, but doing so does not confer Project or executor authority and does not
make `agent_id`, `task_id`, `attempt_id`, or an opaque fence a credential.

Execution-carrier admission is then additive rather than implicit. A
CodingAgentRun-backed Attempt does not require any browser/Host Endpoint; it
re-authorizes the exact Project and CodingAgent backend under their normal rules. An
Endpoint-backed Attempt additionally requires the exact currently authorized
Endpoint plus its generation/binding fences. This keeps Task ownership independent
from window presence while preventing either Agent ownership or Endpoint possession
from silently broadening execution authority.

## Agent continuity across windows

The durable identity model is intentionally stronger than window continuity:

```text
Window / Host Endpoint E1 disappears
        |
        v
Durable Agent still exists
        |
        +-- Agent Card
        +-- Conversation / Inbox / Wake
        +-- future Agent Memory
        +-- Agent Tasks / TaskAttempts
        |
        v
Endpoint E2 attaches later
```

A replacement Endpoint may continue the same Agent, subject to current principal
ownership, Endpoint generation, TaskAttempt controller fencing, and the execution
backend's own rules. No model is assumed to remain resident between turns.

This also leaves a clean future boundary for Agent-scoped Memory and Skills. Current
Project Memory remains unchanged until a deliberate migration/namespace design is
implemented. Agent Skills are later additive capability/configuration, not part of
TaskAttempt authority.

## Execution backend sequence

Do not build a universal execution-provider framework in the Agent Task foundation.
Use concrete backends first.

### A3 — Agent Task + fenced TaskAttempt

A3 is implemented and establishes the durable work/ownership substrate:

- explicit work creation;
- explicit assignment/acceptance and atomic Attempt start/claim by that assignee;
- lease + exact Attempt fencing;
- carrier/controller-generation fencing where needed;
- exact heartbeat/completion/replay;
- restart recovery;
- authority/privacy boundaries;
- minimal observation/listing needed to dogfood the domain.

A3 does **not** automatically choose an assignee, spawn workers, operate a global
claimable queue, or choose execution capacity.

### A4a — TaskAttempt -> existing CodingAgentRun (implemented)

A4a is implemented through `start_agent_task_coding_run` and
`reconcile_agent_task_coding_run`. The runtime re-authorizes the exact TaskAttempt,
Project, and CodingAgent backend; persists the prepared binding in
`wc_agent_task_coding_runs`; durably claims dispatch before starting the backend;
preserves uncertain dispatch as `outcome_unknown`; and reconciles the authoritative
CodingAgentRun before terminalizing the exact TaskAttempt. The binding retains the
run/provider/authority/intent identities needed to reject a changed or stale backend
rather than weakening them into generic Task controller state.

This first concrete backend proves that Agent Task execution is independent from
ChatGPT browser windows and that TaskAttempt ownership can survive Server restart and
backend-response uncertainty.

### A4b — TaskAttempt -> Agent Endpoint continuation (implemented)

A4b is implemented through `start_agent_task_endpoint_continuation`. The model supplies
only the exact `task_id`, `attempt_id`, `assignee_agent_id`, `attempt_fence`, and
`attempt_controller_generation`; startup never selects an Endpoint. The Store records
one concrete `wc_agent_task_endpoint_executions` row plus one durable
`agent_task_attempt` Wake. Its Endpoint id/generation are nullable until an existing
wake-capable carrier later claims the Wake. Attempt controller generation remains
authoritative only in `wc_agent_task_attempts`; the backend row does not duplicate it.
A4a and A4b are mutually exclusive per Attempt rather than hidden behind a universal
execution-provider abstraction.

Task-origin Wakes are a second explicit Wake source, not fabricated Inbox activity.
They carry exact Task/Attempt references while Conversation Message/Delivery ids and
Inbox high-watermarks stay null. The Host continuation envelope re-reads durable Task
truth and carries the exact current Attempt fence/generation. Wake consumption proves
one reasoning takeover only; exact AgentTask completion remains a separate mutation.

A4b intentionally separates a short pre-takeover lease from renewable bounded active-turn reservations:

```text
TaskAttempt start
  -> short 60-second setup/pre-dispatch lease
Endpoint carrier claim / prepare
  -> still short pre-dispatch semantics
first durable Host dispatch outcome (dispatch_accepted | delivery_unknown)
  -> bounded 5-minute Host scheduling grace
exact model turn bootstrap + first exact consume during that grace
  -> bounded 30-minute active-turn reservation
ordinary coding work
  -> no periodic 60-second heartbeat ceremony
exact active-turn proof heartbeat before reservation expiry, when needed
  -> another bounded 30-minute active-turn reservation
  -> repeat only while the same exact Attempt/turn proof remains current
exact TaskAttempt completion
  -> terminal Task -> correlated attention_event Wake when applicable
abnormal/stalled model turn
  -> takeover lease eventually expires -> old Attempt permanently stale
  -> a new Attempt requires an explicit authorized start
```

Crossing the durable Host dispatch fence now creates a separate scheduling reservation only for an exact Task-origin Wake. The first `prepared -> dispatch_accepted | delivery_unknown` transition atomically rechecks the owned latest active/unexpired Attempt, exact assignee, exact Wake, and exact bound Endpoint/generation, then uses `max(existing_lease, now + 5 minutes)`. Delivery uncertainty gets the same reservation because the Host may already have accepted the message and exactly-once delivery forbids a blind resend. Same-generation process-local Host-binding loss or Server takeover that conservatively records an already-prepared Wake as `delivery_unknown` establishes that same one-shot grace before carrier capability is cleared; durable Endpoint replacement/expiry/detach fences the old generation instead and grants it no new grace. Dispatch replay, `delivery_unknown -> delivered` reconciliation, stale Endpoint generations, wrong Wakes, non-Task Wakes, polling, Endpoint heartbeat, and ClientWindow activity do not slide this reservation. A dispatch outcome arriving after the Attempt already expired or was superseded is still durable Wake evidence but never revives that Attempt.

The first successful exact consume of an `agent_task_attempt` Wake promotes only the same latest, active, unexpired, exact-assignee Attempt whose durable A4b Endpoint execution still matches that Wake and Endpoint generation. Promotion is atomic with Wake consumption and uses `max(existing_lease, now + 30 minutes)`. Consume replay does not slide the lease. An expired, terminal, superseded, or carrier-mismatched Attempt is never revived; its already-dispatched Wake can still be consumed/ACKed without lease promotion. `inbox_changed`, `attention_event`, and `agent_wait_events` consumption never changes a TaskAttempt lease.

`heartbeat_agent_task_attempt` has two intentionally distinct modes. Without active-turn
proof it preserves the ordinary A3/pre-takeover behavior: exact current Attempt fencing
plus `max(existing_lease, now + 60 seconds)`, but a freshly resumed A4b turn no longer
performs this as a mandatory preflight. It bootstraps the exact Wake, exact-consumes it
immediately, then reads the authoritative AgentTask and works only if the same Attempt is
still current. After actual A4b model takeover, the same
turn may additionally provide the exact consumed Task-origin `wake_id` and its
`consume_token`. In the same authoritative transaction the Server still rechecks normal
Task ownership, latest Attempt identity, assignee, fence, current Attempt controller
generation, active/unexpired state, the consumed Wake/token hash, and its durable A4b
Endpoint execution binding. Only then may the heartbeat use
`max(existing_lease, now + 30 minutes)`. Duration and absolute expiry remain
Server-authoritative; one renewal never grants a multi-hour lease, but repeated exact
renewals before expiry can support a multi-hour online turn through successive bounded
30-minute reservations. The proof is model-turn lineage evidence, not Task, Project,
Runner, Goal, Session, or Endpoint authority.

This renewal depends only on durable Store truth, so a Server restart does not break an
otherwise-current proof. Conversely, Endpoint loss/replacement advances the Attempt
controller generation and clears the old carrier binding, so a stale model turn cannot
renew even if it still knows the old Wake token. Expired, terminal, or superseded
Attempts are never revived. Renewal occurs only through this explicit heartbeat
mutation: consume replay, MCP App polling, Endpoint heartbeat, ClientWindow activity,
Goal soft liveness, and ordinary model tool traffic never renew a TaskAttempt. The Goal
/ ClientWindow five-minute liveness signal remains observation evidence; only the separate fenced live-card recheck can create workflow-stall attention
only and is not a correctness lease input.

A4b lifecycle v1 intentionally closes here. Automatic renewal would require a truthful,
exact model-turn lifetime signal that proves this specific resumed turn is still running
and eventually identifies its end. The current ChatGPT/MCP App carrier contract does not
provide one: `ui/message` acceptance or conservative delivery uncertainty may establish
only the one-shot bounded Host scheduling grace described above, not model-turn liveness
or active-turn renewal; View/App/Endpoint liveness proves only carrier coordination; and
exact `consume_agent_wake` proves a model turn took over at least once but not that it
remains alive minutes later. None of those carrier signals may become active-turn renewal authority. If a future Host contract
exposes exact turn identity plus trustworthy running/end lifecycle, a bounded
process-local controller may use the existing explicit active-turn heartbeat while that
proof remains live. Until then, long A4b turns explicitly renew before expiry; Server
restart safely restores durable lease truth without reconstructing process-local turn
ownership or persisting the raw consume token.

Claiming a Task-origin Wake atomically installs the actual Endpoint carrier and advances
`attempt_controller_generation`. Releasing or losing that carrier before the dispatch
fence clears the backend carrier; a replacement carrier must claim again and therefore
advances the Attempt generation again. Older carrier turns are permanently stale.
Endpoint lease and TaskAttempt lease remain independent correctness boundaries; MCP App
Endpoint heartbeats do not extend the TaskAttempt takeover lease.

If no wake-capable Endpoint exists, startup still succeeds with a pending durable Wake
and a null carrier. The existing event-driven continuation controller is scheduled once
and later Endpoint registration schedules the Agent again; there is no busy wait, fake
Message/Delivery, implicit Task failure, or lease extension. If the TaskAttempt expires
before exact model takeover, claim/other authoritative mutation paths retire the
pre-fence Task Wake when still possible, the old Attempt remains stale, and the existing
Task retry/Ready semantics require an explicit new Attempt. A post-fence
`delivery_unknown` observation remains exactly consumable and is never blindly
re-dispatched.

The same delivery closes repeated Endpoint-successor recovery in App protocol v15.
Every durable recovery edge remains exactly one generation (`E1 -> E2`, `E2 -> E3`,
...), and replay of E1 may reveal only its recorded E2 even when a later generation is
current. If that exact E2 is naturally expired, the Server returns a successful strict
intermediate proof without bootstrapping it; the App adopts E2 and requests the next
one-hop recovery. The App validates every edge and stops after eight hops. Ordinary
replacement, push takeover, detach, different ClientWindow/principal, stale binding,
and generic JSON-RPC `-32000` still fail closed.

Only after A4a and A4b reveal repeated common machinery should WebCodex consider a
minimal shared execution binding/adapter abstraction.

## Asynchronous events and scheduling are derived capabilities

A4b should not introduce a generic event bus, Goal scheduler, DAG engine, or autonomous
loop. The next architectural step after concrete Endpoint-backed work may be a small
durable event/attention layer driven by real domain transitions such as TaskAttempt
terminal/attention state, timers, or external completion signals.

Keep the meanings separate:

```text
Event = a durable fact that something happened
Wake  = a durable opportunity for one Agent to reason again
Wait  = one explicit durable one-shot interest in future source facts
Task  = durable work that may still be incomplete
Goal  = durable high-level intent/control truth
```

An Event is therefore not automatically a Wake, and a Wake is not an Event log. Many
durable events may coalesce into one reasoning opportunity; the resumed model reads
the authoritative source domains rather than treating copied event payloads as
execution truth. Event identity/reference must not transfer the source domain's
authority.

### Durable Agent Wait v1

`AgentWait` is the small model-facing rendezvous built on that distinction. It is not a
Task, Goal, Event log, Conversation, Workflow Session, or Host binding, and it never owns
or inherits authority over its sources. A Wait durably records only the caller-owned
target Agent, an optional exact `goal_id` correlation context, its bounded source
selectors, and bounded semantic match references. `goal_id` is an exact reference only:
it does not copy Goal objective/title/lifecycle/controller metadata and grants no Goal,
Task, Project, Session, or execution authority. The Endpoint/generation supplied at
creation is re-authorized only as the current Host presentation/carrier selector and is
not persisted as Wait execution ownership.

Wait v1 is deliberately one-shot with lifecycle
`waiting -> triggered -> resumed` or `waiting|triggered -> cancelled`. It supports only
1..8 exact `agent_task_terminal` selectors and one closed durable mode: `any` or `all`.
`any` is the default and omission is exactly equivalent to `any`; the first source match
triggers. `all` is only a bounded join/barrier: every registered exact source Task must be
terminal before the Wait triggers. Neither mode is a Task dependency, runnable-frontier
rule, successor-execution policy, or source-discovery query.

Generic Wait registration (`goal_id = null`) preserves the existing atomic Task snapshot:
already-terminal exact sources may match immediately in the SQLite IMMEDIATE registration
transaction. Goal-scoped registration is intentionally stricter. The exact Goal must be
owned by the same principal and `active`, must have an explicit `controller_agent_id`
equal to the Wait target Agent, every selected exact Task must be owned by the same
principal and already explicitly correlated to that exact Goal, and every selected Task
must still be non-terminal. The model supplies the exact 1..8 Task ids; WebCodex never
discovers them from Goal correlations. A late Goal-scoped registration fails closed with
`Goal-scoped rendezvous must be registered before selected source Tasks terminalize.`
rather than retracting prior attention, rewriting Event/Wake history, or guessing whether
a Host received an earlier continuation.

Registration and terminalization both use SQLite IMMEDIATE transactions, giving the race
a simple linearization point. If scoped registration commits first, later terminalization
sees the explicit rendezvous owner. If terminalization commits first, ordinary Goal
attention is already durable and the later scoped registration observes a terminal source
and fails. No timestamp ordering is used. Both explicit TaskAttempt completion and
CodingAgentRun terminal reconciliation write matching Wait facts in their same
source-terminal transaction. Admission still bounds active Waits per Agent and per source,
so accepted Waits cannot turn Task completion into unbounded fanout.

Terminal routing has one explicit precedence rule for each active exact Goal/Task
correlation: an active Goal-scoped Wait (`waiting` or `triggered`) whose `goal_id` and
registered source include that Task owns terminal attention for that correlation; otherwise
normal per-Task Goal terminal attention is created. A generic Wait never suppresses Goal
attention merely because the same Agent is also waiting on the same Task. Goal-scoped
`any` uses the ordinary ANY one-shot/coalescing state machine: the first selected terminal
fact triggers one Wait Wake and does not also create a duplicate Goal attention Wake.
Goal-scoped `all` records each partial terminal fact durably while remaining `waiting` and
creates neither a Wait Wake nor a per-Task Goal attention Event/Wake; the final selected
source records the complete match set, performs `waiting -> triggered`, and creates the
single Wait-origin reasoning opportunity.

Further ANY matches continue to obey the existing seal: they update the same Wake only
while it is `pending` or `claimed`; `prepared`, `delivered`, and `delivery_unknown` mean
the Host may already have received the envelope and no successor Wait turn is invented.
Exact Wait Wake consume atomically changes `triggered -> resumed`; consume replay is inert.
A resumed Goal-scoped continuation reads the bounded Wait (including only exact
`goal_id`), calls normal `get_goal(goal_id)`, independently re-reads every authoritative
source Task, and explicitly decides whether/how to update the Goal from current durable
truth. The compact automatic message carries those exact references and instructions but
never copies Goal objective or Task terminal bodies.

Cancellation is forward-looking. Cancelling a Goal-scoped Wait before any selected Task
terminalizes restores ordinary Goal terminal attention for later terminals. Cancelling an
ALL Wait after partial matches preserves those matches as durable history but does not
retroactively synthesize attention Events/Wakes for Task terminals the active Wait already
owned; later source terminals fall back to ordinary Goal attention. Cancellation still
fails closed after Host dispatch preparation. Server restart preserves `goal_id`, sources,
matches, state, and Wakes. A Goal controller change after registration does not retarget an
existing Wait or rewrite historical matches: the Wait target Agent remains the durable
historical subscription target validated at registration, and Window/Endpoint state is not
used to choose Goal routing.

Recommended bounded orchestration order is: make Coordinator/Workers continuation-ready;
create a Goal with explicit controller; create exact Tasks with explicit workers; explicitly
associate the selected Tasks to the Goal; register the Goal-scoped `any` or `all` Wait with
those exact 1..8 Task ids; **only then** start worker execution. On resume, consume the exact
Wake, read the Wait, read the Goal, re-read every source Task, and explicitly decide the
next Goal action. Do not dispatch workers first and try to add the rendezvous afterward.

Natural future source kinds include `deadline_reached`, Job terminal state,
Plugin/external completion, and human approval. They should be added only when each has an
authoritative source transition and bounded registration/fanout contract. Wait v1 does
not introduce a timer scheduler, generic event bus, public `publish_event`, recurring
subscription, generic predicate/expression language, nested boolean conditions, DAG,
reducer, automatic Goal progression, automatic successor Task creation, or automatic Task
discovery from Goal correlations.

Goal should remain the deliberately small `active | completed | cancelled` lifecycle.
States such as `implementing`, `waiting_ci`, `waiting_human`, `blocked`, or
`validating` should be derived presentation/attention from correlated durable work and
events unless a later product requirement proves they are independently authoritative
Goal truth. Goal orchestration should create/associate explicit work and react to
durable facts rather than becoming a second execution engine.

Later dogfood may show that many durable Agent Tasks benefit from a runnable-frontier
scheduler. If so, scheduling should derive from durable work and semantic state
transitions, not from browser tabs or UI idle state.

Useful future invariants include:

- planning is advisory; explicit assignment plus claim/dispatch remains authoritative;
- runnable work and live execution reservations drive capacity decisions;
- active execution reservations form a floor only for the carrier class they
  actually consume;
- semantic durable state transitions may create bounded wake pressure; visual UI
  state does not;
- stale workers/carriers cannot renew expired leases or submit late results.

Capacity is therefore potentially per execution class rather than simply
"number of active Agent Tasks = number of ChatGPT windows".

This is a possible later product slice, not an A4b requirement and not WebCodex's
north star.

## Dependencies and workflow graphs come later

Do not add dependency DAGs, fan-out/reducers, graph node kinds, conditional routing,
supersteps/barriers, or parent/child Agent orchestration merely because another
system demonstrates them.

First prove:

```text
Agent Task -> exact TaskAttempt -> concrete execution -> exact terminal result
```

Add dependency semantics only after real workflows require `A -> B` or fan-out/join.
Add an explicit workflow graph only after dependency plus conditional-routing use
cases justify a third abstraction layer. Superstep/BSP-style coordination has no
current roadmap commitment.

## Agent Task execution acceptance baseline

A3, A4a, and the implemented A4b establish the current baseline, including the
Endpoint-backed cases below:

| Case | Required result |
| --- | --- |
| two concurrent start/claim requests for one explicitly assigned Agent Task | exactly one authoritative Attempt is created/accepted |
| exact start/claim retry | returns the same Attempt without redispatch |
| heartbeat before lease expiry | succeeds only for exact current Attempt/controller |
| heartbeat after lease expiry | rejected as stale |
| completion after lease expiry | rejected as stale |
| same assigned Agent starts again after expiry | creates a new Attempt |
| different Agent tries after expiry without reassignment | rejected; lease expiry does not transfer assignment |
| authorized explicit reassignment, then new assignee starts | creates a new Attempt for the new current assignee |
| Attempt 1 responds after Attempt 2 exists | Attempt 1 remains permanently stale |
| Endpoint/carrier replacement inside one Attempt | old Attempt controller/binding is fenced |
| CodingAgentRun dispatch response is uncertain | exact durable binding reconciles the authoritative run; no blind second run |
| CodingAgentRun reaches terminal state | reconciliation terminalizes the exact current TaskAttempt once |
| A4b TaskAttempt has no wake-capable Endpoint | work remains durable/pending; no Task failure and no fabricated Conversation Message |
| A4b Wake is consumed | proves one resumed reasoning opportunity only; TaskAttempt remains active until exact completion |
| A4b Endpoint E1 is replaced by E2 inside one Attempt | same Attempt/fence, incremented Attempt controller generation; old E1 turn is stale |
| original continuation selector E1 has already advanced to expired E2 | same authorized Window can discover the authoritative chain and advance to E3 without reviving E1 |
| A4b Host outcome is `delivery_unknown` after dispatch fence | no blind redispatch and no duplicate TaskAttempt |
| Server restart | Agent Task/TaskAttempt truth and accepted replay identities survive; no second logical work item |
| Endpoint detach | Task/Attempt durable state does not disappear |
| duplicate completion | exact replay, no repeated terminal side effect |
| changed replay | idempotency conflict |
| Conversation participant references Agent Task | receives no implicit Agent Task execution authority |
| Agent Task references Project | receives no implicit Project authority |
| unauthorized exact Agent Task/Attempt id | does not disclose foreign-resource existence |

## Explicit non-goals for A4b

The Endpoint-backed execution slice must not expand into:

- a generic swarm scheduler or worker pool;
- automatic Agent spawning or autonomous delegation;
- runnable-frontier/capacity autoscaling;
- dependency DAG, fan-out/reducer, graph DSL, or superstep;
- a universal execution-provider framework;
- Agent parent/child hierarchy;
- a generic durable event bus or Goal scheduler;
- new Goal lifecycle states for execution/presentation phases;
- Agent-scoped Memory migration or Agent Skills;
- federation/A2A compatibility;
- PostgreSQL/distributed multi-Server scheduling;
- a generic Actor/Event/Entity ORM.

The design goal is narrower: make a durable Agent able to own and resume
asynchronous work correctly, with explicit identity, authority, replay, and stale
execution fencing. More automated coordination can grow from that foundation only
when real product use demonstrates the need.
