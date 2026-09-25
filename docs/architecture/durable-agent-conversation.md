# Durable Agent identity, Conversation, and wake foundation

A1 adds a concrete communication domain without changing the meaning of Workflow Sessions or project Memory. A2 extends that domain with durable wake intent, Endpoint lifecycle/generation fencing, dispatch uncertainty, and exact continuation consume semantics without making model execution part of Conversation or Inbox state. A2.5 connects those facts through a production-shaped, process-local Host binding/controller boundary, a bounded natural-conversation bootstrap, and explicit Agent selection in Runtime Console. The standing direction beyond this implemented substrate is defined in [`durable-agent-runtime.md`](durable-agent-runtime.md).

## Domain boundaries

| Concept | Durable meaning | Explicitly not |
| --- | --- | --- |
| Agent | Server-generated `wc_dagent_*` identity plus a mutable Agent Card | a browser window, MCP connection, credential, Project, Workflow Session, or authority grant |
| Agent Endpoint | A principal-bound Host/Client attachment with a Server-assigned controller generation, bounded lease, lifecycle, and wake capability metadata | the Agent identity itself or execution authority |
| Conversation | Durable communication space containing Human and Agent participants | a Workflow Session, Task scheduler, or execution context |
| Conversation Message | Append-only transcript fact with a stable per-Conversation sequence and author provenance | recipient read state or a current-state delta |
| Agent Delivery | Recipient-specific queued/consumed Inbox state pointing to one Message | a duplicate Message or model invocation |
| Wake Intent | Durable logical continuation saying an Agent should receive another processing opportunity | a Message, Inbox Delivery, or Host delivery attempt |
| Wake Delivery Attempt | One Endpoint/generation-bound attempt to deliver a Wake Intent through a continuation adapter | the durable communication fact or a grant of execution authority |
| Agent Task | Planned durable asynchronous work accepted/created for an Agent | Conversation Message, Workflow Session todo, Job, or execution authority |
| Agent Wait | Caller-owned durable one-shot rendezvous over 1..8 exact AgentTask-terminal sources with closed `any|all` mode | a Task dependency graph, Goal attention policy, Host binding, scheduler, or source-authority grant |
| Goal | High-level durable workflow/progress truth: fixed completion intent, bounded plan/checkpoints, lifecycle and controller routing | Task execution ownership or inherited authority |
| Workflow Session | Concrete current-work context: existing execution, provenance, validation, Job, workspace, todo, and guidance | a chat room or a Goal |

An Agent Card contains a mutable non-unique handle, display name, description, bounded specialty labels, profile revision, and timestamps. These fields are self-description metadata. Canonical identity is only the Server-generated `agent_id`, and neither identity nor metadata grants Project, filesystem, Runner, Agent Task, or Workflow Session authority.

## Authoritative persistence

The Control-owned SQLite database remains the standalone authoritative transactional store. A1 adds independent tables for:

- Agent identities and owner communication principals;
- Agent Endpoints and their attachment principals;
- Conversations and participants;
- append-only ordered Messages;
- recipient-specific Agent Deliveries;
- operation-scoped idempotency records;
- coalesced Agent Wake Intents and Endpoint/generation-bound Wake Delivery Attempts.
- bounded Agent Waits, exact source registrations, and semantic match references used to derive one-shot Wait-origin Wake opportunities.

This reuses the existing durable database lifecycle instead of adding another JSON truth or process-local registry. Generic `Database::open` is storage-only: it may run schema migration and owner-independent housekeeping, but it never declares a Wake worker dead or performs takeover recovery. The standalone Control Server holds an exclusive instance guard bound to its exact database state for its full lifetime; crash/takeover Wake reconciliation runs only after a successor acquires that ownership proof. This is deliberately standalone coordination, not a distributed lease or cluster protocol. The schema is concrete to the current communication/wake use case rather than a generic actor/event framework. It does not assume SQLite is process memory, and the domain can later be mapped to another transactional backend without changing its IDs or public semantics.

Agent Endpoint lifecycle, Conversation lifecycle, and Agent Delivery state are closed typed Store contracts. Their existing SQLite strings remain storage encodings and public serialization values; unknown persisted lifecycle values fail closed before authorization or delivery authority is evaluated. This does not change the independently typed Wake or Agent Task/TaskAttempt state machines.

A Message append, sequence allocation, all requested Agent Deliveries, required Wake Intent coalescing/creation, Conversation update, and idempotency record commit in one immediate transaction. A forced Wake insertion failure therefore leaves no Message, no partial Inbox state, no Wake, no consumed sequence, and no stale idempotency outcome.

## Identity and authorization

`communication:read` and `communication:manage` are independent scopes. Communication principals are stable hashes of an authenticated account/shared-key subject; secrets and hashes are never returned. Runner transport credentials, Project credentials, and project-scoped OAuth subjects cannot become communication principals.

Creating or updating an Agent, attaching an Endpoint, and adding an Agent to a new Conversation require ownership by the same communication principal. An Agent-authored Message, Agent-scoped Conversation read, Inbox read/consume, and Wake operation additionally require the exact current Endpoint id and controller generation bound to that principal and Agent. Human-authored Messages require the current principal to be the Conversation's Human participant.

Conversation participation authorizes only communication. Existing Project, Workflow Session, Runner, filesystem, Session fence, and Memory authority checks are not consulted, inherited, or relaxed.

Exact communication-resource authorization is existence-hiding. Agent ownership, Endpoint attachment authority, Conversation participation, reply targets, and Delivery recipients are scoped in the authoritative lookup before resource state is inspected. A well-formed resource id outside the caller's communication principal or authorized parent therefore has the same public not-found contract as a nonexistent id; foreign owner, host, lifecycle, Message presence, and Inbox recipient metadata are never used as an error distinction. Once access is proven, own-resource diagnostics such as profile revision conflicts, detached/stale Endpoints, closed Conversations, and desired-state consume retries remain explicit.

## Ordering, delivery, and replay

Each Conversation owns a monotonic `next_seq`; Message rows are unique by `(conversation_id, seq)`. Transcript reads use an exclusive `after_seq` cursor and never consume Inbox state.

Each Delivery is unique by `(message_id, recipient_agent_id)` and has its own monotonic `delivery_order`, queued/consumed state, and timestamps. Inbox reads use an exclusive `after_delivery_order` cursor. An offline Agent needs no Endpoint for Delivery creation; a later principal-bound Endpoint can read and consume the queued Delivery.

Message/Delivery cardinality is deliberately independent from model-turn cardinality. Multiple queued Deliveries for one Agent coalesce into a bounded pending Wake Intent while every Message and Delivery remains independently durable. A later Message can update the pending Wake high-water mark/count without replacing its logical identity.

Wake delivery uses a separate Attempt state machine. Claim is Endpoint/generation fenced and leased. Failure before the durable dispatch fence revokes the Attempt and returns the same Wake Intent to `pending`; after the fence, an unconfirmed Host outcome becomes `delivery_unknown` and is never unconditionally redispatched. Endpoint detach, replacement, expiry materialization, and authoritative standalone Server takeover reconcile stale claims or ambiguous dispatched work conservatively. Exact Wake consume is bound to `wake_id`, target Agent, current Endpoint, controller generation, and an opaque per-Attempt consume token. Wake consume never consumes Agent Deliveries, and Delivery consume never consumes a Wake.

Create-Agent, attach-Endpoint, create-Conversation, and ordinary post-Message mutations require caller-generated idempotency keys. The store hashes both key and canonical request. Idempotency lookup is communication-principal-scoped. Exact replay returns the original durable resource with `replayed=true` and `state_changed=false`; changed reuse is rejected. Exact Message replay is checked before current Endpoint/lifecycle validation so a caller can recover a committed response after the original Endpoint detached without duplicating the Message or Deliveries. After a Wake has crossed either the Host dispatch fence or the explicit-activation fence, its resumed reply may instead use the exact `wake_id` plus a bounded per-Wake operation index. A still-`pending` Wake cannot mint this reply identity; an already-active manual turn must explicitly activate it first. That pair derives a stable internal replay identity without making the body or replaceable Endpoint an identity or exposing a raw idempotency key; different indexes permit multiple intentional replies from one Wake. The first commit still requires the exact current Endpoint generation, while an exact retry can recover across a replacement carrier. A changed retry at an occupied index conflicts and must re-read the authoritative Conversation. Reply targets are resolved only inside the already-authorized Conversation, and Delivery consume resolves only inside the authorized Agent Inbox. Detach and consume are desired-state idempotent.

Request and response payloads, list pages, participant counts, profile fields, Message bodies, active Endpoints, Conversations, and Messages per Conversation have explicit bounds. Audit and Workflow Session context projections retain IDs, counts, cursors, and mutation state but omit Agent description contents, specialty-label contents, Message bodies, and idempotency keys.

## Runtime Console and tool surfaces

The model-visible Control surface provides explicit tools to create/list/update Agents, attach/detach Endpoints, bootstrap a current Agent activation, create/list/read Conversations, post Messages, list an Agent Inbox, consume Deliveries, and consume one exact Agent Wake after its durable dispatch fence. Server-owned output schemas remain complete even when a Compact model-facing `tools/list` projection omits `outputSchema`. Endpoint heartbeat/renewal remains Host infrastructure rather than model bookkeeping.

`list_agent_identities` also projects one bounded coordinator-facing readiness fact per listed Agent: `production_auto_resume_available`. It is true only when that listing snapshot has a current, unexpired, generation-matching, durably wake-capable Endpoint and the current Server process holds a production Host carrier for that exact Agent/generation. The existing `active_endpoint_count` therefore distinguishes “current Endpoint but no production auto-resume carrier” from “no current Endpoint” without exposing ClientWindow continuity, binding fences, consume tokens, claim fences, or private transport data. This readiness means only “a Host carrier currently exists for automatic continuation”; it is not worker idleness or model-turn liveness, not presence, not a capacity lease/reservation, not execution authority, and not a guarantee that Host scheduling will run immediately. Server restart drops the process-local half of the conjunction, so readiness remains false until a View or other production carrier rebinds or recovers through its ordinary fenced path.

`bootstrap_agent_conversation` requires `communication:manage` plus the exact acting Agent, Endpoint, and controller generation. An optional explicit Conversation and Wake select durable records; no HTTP/MCP connection, `Mcp-Session-Id`, credential, Project, Workflow Session, or previous request supplies hidden continuity. By default the bounded result contains the Agent Card, Endpoint, selected Conversation summary, queued Inbox count/high-watermark, safe Wake identity/state/count metadata, reply-operation bounds, and current process adapter capability. It contains no transcript, Inbox Message body, claim fence, principal digest, Host secret, or raw replay key.

When the caller is already in an explicitly started model turn and supplies `activation_idempotency_key`, the same bootstrap can accept one exact `pending` Wake through a durable `explicit_activation` Wake Delivery Attempt. This does not request another turn and does not make the Endpoint generally wake-capable. The response includes the exact consume token; exact retry with the same activation key recovers the same Attempt and token after response loss, while changed reuse fails closed. Audit and Session projections omit both the activation key and consume token. The caller must still read authoritative Inbox/Conversation state and exact-consume only the Wake and Deliveries it processed.

Runtime Console exposes the same domain through hidden same-origin POST routes. Its minimal Chat panel can:

- create and select an Agent;
- inspect and update its Agent Card and queued count;
- explicitly “Continue as” the selected Agent by attaching/detaching the current browser as a non-wake-capable Endpoint;
- create/open a Conversation;
- render participants and the ordered transcript with Human/Agent provenance;
- post Human-authored Messages or Agent-authored Messages through the selected exact Endpoint/generation;
- inspect and consume an attached Agent's queued Deliveries.

The page performs bounded polling on the existing eight-second Runtime Console cadence and renews its own non-wake-capable Endpoint lease through a hidden same-origin route. Polling, refreshing the page, attaching/renewing an Endpoint, and receiving a Delivery do not invoke or wake a model. Endpoint detach on page exit is best effort. Lease validity and generation fencing are checked synchronously at operation admission and are the immediate correctness boundary. Replacement or authoritative Server takeover lazily materializes durable `expired` Endpoint state, while claim recovery lazily releases expired pre-dispatch claims; A2 does not run a periodic Endpoint/Wake reaper and does not promise a database write exactly when wall clock crosses a lease deadline. The durable Agent, transcript, Inbox, and Wake state never depend on browser lifetime.

A2.5 adds an event-driven `AgentContinuationController` around the narrow Host-neutral continuation boundary. Its process-local registry now supports two carrier kinds without changing durable truth: a push `ContinuationAdapter` callable by the Server, and an MCP App pull binding whose live View calls the Host. Message commit, exact Wake consume, and push-adapter registration still schedule only a bounded deduplicated dispatch opportunity; the queue capacity equals the durable Agent capacity, so one deduplicated notification per possible Agent cannot overflow a smaller process-local queue. The Server worker never pretends an MCP App pull binding is a callable callback. No permanent polling loop is introduced, and both carrier kinds reuse the same durable Wake / Wake Delivery Attempt claim, dispatch-fence, uncertainty, and consume state machine. Repeated events therefore cannot turn a 50-Message burst into 50 simultaneous model turns: every Message and Delivery remains durable while unresolved Wake state stays bounded.

Host registration is the only path that projects an attached Endpoint as `wake_capable=true`; model and Runtime Console attach input cannot self-declare that capability. Push registration is accepted only for an exact Endpoint freshly attached through the current Server process, so a successor cannot reuse a pre-restart callback. MCP App identity is deliberately two-layered: `binding_id` is the process-local iframe fence, while an optional canonical hashed ClientWindow is Host-window continuity. A successful bind persists the v11 binding fingerprint and, when available, that Window key. Server takeover clears the Host registry and `wake_capable` but preserves both bounded recovery values. Exact-fingerprint recovery remains the no-Window fallback. A refreshed iframe in the same Window may use a new binding fence after restart or exact unbind; a different Window may not. Exact unbind clears the binding fingerprint but preserves matching Window continuity while the Endpoint remains current. Natural lease expiry also clears `wake_capable` and the exact binding fingerprint, but v14 deliberately retains only the already-hashed ClientWindow continuity key on the expired Endpoint. That retained hash is not Host authority and cannot revive the expired generation: it is usable only by the dedicated app-only expired-Endpoint replacement transition described below. Explicit detach, ordinary Endpoint replacement, and transition to a push carrier still clear both recovery values. Detach remains effective after lease expiry and after expiry materialization; it withdraws only the exact principal-owned Endpoint, never its successor. Existing Store reconciliation still revokes a pre-fence claim back to the same pending Wake or preserves a post-fence Attempt as `delivery_unknown`. Restart/refresh recovery never recreates the process-local fresh-attachment marker; only a newly committed expired-Endpoint replacement is registered as the fresh current-process attachment for its new Endpoint.

G3 installs the production ChatGPT MCP App pull carrier without making the App a continuation owner. `present_agent_continuation` creates the only persistent controller card at `ui://webcodex/agent-continuation/v16`; it is an explicit bounded read and never infers an Agent or Endpoint. Callers pass either a server-issued `agent_continuation_ref` pinned to one Agent, Endpoint, and controller generation, or that explicit tuple. The ref is not a credential; presentation still rechecks owner, lifecycle, and generation, and a newer Endpoint never rewrites an older ref. On an eligible Stateless MCP 2026 Apps surface, ModelHidden app-only bind/recover/state/acquire/prepare/finish/unbind operations re-authorize normal communication authority on every call. Acquire delegates to `claim_next_agent_wake`; prepare crosses `prepare_agent_wake_dispatch` and revalidates the exact binding immediately before the View may call `ui/message`; finish records only Host `dispatch_accepted` or conservative `delivery_unknown`. Claim fences remain Server-side. The consume token appears only in the bounded App-private automatic continuation envelope and is omitted from model-visible projection, generic audit/session projection, and request-trace payload capture.

New-window setup is deliberately front-loaded and short: `create_agent_identity -> rotate_agent_continuation_endpoint -> present_agent_continuation -> yield/end the current turn`. Presentation success is not a Host-binding ACK, so a coordinator must not keep doing long business work in that same turn and assume the card is already wake-ready. A later `list_agent_identities` read exposes `production_auto_resume_available` for the exact current generation; that readiness remains carrier availability only, not model-turn liveness or execution authority.

Each View creates a stable `wc_host_binding_<32 lowercase hex>` fence from 128 browser-secure random bits before bind; unavailable secure randomness fails closed. Bind includes that exact fence. A retry for the current same View only renews the Endpoint and preserves its claim and dispatch observation. A same-Window refreshed iframe may present a different fence, but another Window cannot replace the current carrier. After Server takeover, the exact fingerprint or matching canonical Window can yield the same successful `host_binding.bound=false` / `recovery.kind=host_binding_missing_in_process` observation for the same exact Endpoint/generation. Expired-Endpoint recovery is intentionally separate. The app-only `agent_continuation_recover_endpoint` receives the exact stale Agent/Endpoint/generation plus iframe fence, while canonical ClientWindow identity is injected only by the Host/runtime sideband. Under a SQLite `BEGIN IMMEDIATE` transaction the Store re-checks owner, old Endpoint ownership/lifecycle/lease, matching durable Window continuity, and current generation. A live Endpoint returns a successful no-op. Only natural expiry may atomically mark E1/g1 expired and create E2/g2, with a generation compare-and-swap and a durable old-selector-to-successor idempotency record. Exact response-loss retry returns the same E2/g2 only while it remains the authoritative successor and still retains the matching Window key; if E3 already exists the old replay fails stale. Replay never recreates the current-process fresh-attachment marker, and a push transition that retired Window continuity cannot be undone through a historical replay record. A different Window conflicts/fails closed. Generic Host JSON-RPC `-32000` is never classified as expiry: after bounded bind/state failure the App may ask this dedicated operation to decide authoritatively, and the Server either returns live/no-op, performs the fenced replacement, or rejects it.

The v14 controller state machine is therefore `Bound(E1/g1) -> Stale(E1/g1) -> Replacing(E1/g1 -> E2/g2) -> Bound(E2/g2)`. `Stale` is durable Endpoint truth (expired lifecycle or elapsed lease), while the active iframe binding and Host carrier remain process-local. There is no durable `Replacing` row: the replacement transaction either commits E2/g2 plus its idempotency record together or commits nothing. The App may update its card identity only from the strict successful replacement envelope naming the exact old selector and exactly `generation+1`. Ordinary bind/state/Wake projections remain unable to retarget the card. The App increments a local identity epoch when accepting replacement; outstanding old bind/state Promises and scheduled timers capture the prior epoch and become inert, while any delayed ordinary old-generation projection is ignored rather than allowed to tear down or revert the new generation. The same iframe/card then clears only local controller coordination state, renders `Reconnecting…`, binds E2/g2, and resumes normal Wake reconciliation.

Successful prepare carries the exact bounded envelope in standard `CallToolResult.structuredContent.output.app_protocol.automatic_message`. This is confined to the ModelHidden app-only prepare tool, not `present_agent_continuation` or ordinary model tools. No continuation custom ToolResult `_meta` channel remains. A successful response with a missing, malformed, mismatched, or oversized envelope is treated conservatively like a prepare timeout: heartbeat/acquire reconcile the durable fence and finish as `delivery_unknown`, without another prepare or `ui/message` for that Attempt. Typed audit/Session projections and tool-name-based forensic trace suppression also cover the standard App result payload. Agent v1-v15 remain thin hidden read aliases; discovery advertises only v16. The published continuation projection schema still requires the nullable restart `recovery` field, while the dedicated replacement tool has a separate strict `endpoint_recovery` envelope; the two recovery paths are not inferred from each other. Goal Plan uses the sole v3 resource and wire version 2; its old pre-production resource aliases are removed independently of Agent carrier compatibility.

A successful `ui/message` call proves only Host dispatch acceptance, not model resumption. The App never automatically retries an Attempt after the durable dispatch fence; timeout, reload, View loss, or an outcome that cannot prove non-delivery remains `delivery_unknown`. The only production proof that the continuation model turn actually ran is a later exact `consume_agent_wake`, which remains valid for an already-dispatched `mcp_app` Attempt even if View teardown has withdrawn transient wake capability. A late Host ACK after exact consume is idempotent and cannot regress `consumed`. v16 allows hidden/background Views to acquire, prepare, and dispatch while retaining their slower bounded heartbeat cadence; visibility changes alone never imply delivery uncertainty. Host scheduling remains eventually available/best effort rather than a real-time guarantee. Runtime Console itself remains polling-only and non-wake-capable. The empirical Host behavior and remaining manual dogfood boundary are recorded in [`../agent/mcp-app-continuation-experiments.md`](../agent/mcp-app-continuation-experiments.md).

Goal-correlated terminal attention reuses that carrier without reusing A4b's active-Attempt semantics. A Goal may name one explicit durable `controller_agent_id` independently from the worker Agent assigned to a concrete Task/Attempt. Without an explicit Goal-scoped AgentWait, exact TaskAttempt terminalization atomically creates the narrow `agent_task_terminal` attention Event plus `attention_event` Wake for each currently active owned Goal correlation, targeting that Goal controller when present; a controller-less legacy Goal still falls back to the Task assignee. A generic AgentWait does not change that routing. An explicitly registered Goal-scoped AgentWait is the only precedence override: while its one-shot Wait remains `waiting` or `triggered`, its exact registered Goal/Task sources route terminal facts through the Wait and suppress the duplicate per-Task Goal attention Event/Wake. Scoped `all` partials therefore create neither kind of Wake; the final required terminal fact creates the single Wait-origin Wake. Scoped registration must happen before every selected Task terminalizes, under the same IMMEDIATE-transaction ordering used by terminalization, so no old attention is retracted or rewritten. Cancellation restores ordinary attention only for future terminal facts; it never backfills suppressed historical attention. The Wait retains the controller Agent validated at registration and is not retargeted if Goal controller metadata later changes. On scoped resume the compact automatic message names only exact `wait_id`/`goal_id` and tells the model to bootstrap and consume the exact Wake, `read_agent_wait`, `get_goal`, re-read every source AgentTask, and explicitly decide Goal state from current durable truth. Neither attention nor Wait continuation copies Goal/Task bodies or transfers Goal/Task/Project/Session/execution authority.

## Goal workflow continuity — G4

The WebCodex-owned startup workflow and `single_window_goal_workflow` recommended
flow now use durable Goals for substantial multi-step/cross-turn work in any
repository, independently of `AGENTS.md`. Explicitly correlate the current Workflow
Session, establish fixed completion intent and bounded steps, checkpoint recoverable
milestones with the single revision-fenced/idempotent `checkpoint_goal`, then
freshly verify/review and explicitly complete. `finish_coding_task` surfaces sparse
owned active Goal follow-up but never completes a Goal automatically.

One durable Agent may be both a callable Worker/Task assignee and a Goal controller,
in either setup order and for more than one Goal. Reuse an already-callable exact
Agent established by durable setup or exact Wake context; never create a second
Goal-only identity or infer identity from Window co-location. Task/Attempt owns
execution, while controller identity routes the next reasoning opportunity. Neither
identity grants Project/Runner/Session/Job authority.

The second and only new attention kind is `goal_workflow_stalled`. Goal Plan remains
a separate sparse progress card and detector. Its ordinary polling and App-only
selector-only recheck are NonMeaningful. The Server, not browser timestamps,
recomputes active owned Goal/controller, explicit authorized Session and current
Window relation, complete observation coverage, no active meaningful request,
five-minute quiet and fresh exact-Goal card observation. Window/card heartbeat is
inactivity evidence, not authority; **stalled is not offline**. The existing Window
registry and action ledger supply evidence; there is no second liveness database,
Goal timer, periodic scanner or scheduler.

The transaction binds exact Goal/controller/Session and the last meaningful-work
epoch, atomically creating one immutable fact and one logical Wake. Repeated polls,
View refreshes or Goal metadata revisions cannot mint more turns. Only fresh
meaningful work after that attention can permit a later independent epoch. Historical
Event routing is immutable and is not an assertion that the Goal remains stalled.

The existing Agent Continuation card alone crosses the Host dispatch fence and calls
`ui/message`. Goal Plan never carries Host turn dispatch. Without a Host carrier,
durable Wake truth does not claim model resumption; readiness remains carrier-only,
accepted dispatch is not resume, delivery-unknown cannot be blindly resent, and
only exact consume proves execution of a new turn. The compact stall message
requires bootstrap and immediate exact consume, `get_goal`, exact correlated
`session_handoff_summary`, then continued work from the latest checkpoint/current
step using current authorized Job/Project truth. Recovery is a **fresh reasoning
opportunity**, not retry of the preceding uncertain tool effect.

## Next boundaries

The attention domain has exactly `agent_task_terminal` and `goal_workflow_stalled`;
it is not a generic Event/Actor framework, timer/cron service, webhook bus, dependency
DAG, worker pool or Goal scheduler. Existing Task-terminal attention and Goal-scoped
ANY/ALL Wait precedence are unchanged. `agent_task_attempt` still requests execution
of an exact active fenced Attempt; `attention_event` requests re-reading current
authorized truth before deciding the next step. Neither attention source changes
TaskAttempt ownership or leases. Agent-scoped Memory, arbitrary predicates,
automatic successor tasks and scheduling remain outside this workflow contract.

Current project-scoped Memory behavior is unchanged. Agent identity is stable enough for a future Memory principal or namespace keyed by `agent_id`; A1/A2 do not migrate Memory, add Agent Skills, spawn autonomous workers, implement DAG/swarm orchestration, federation, A2A compatibility, PostgreSQL, or distributed multi-Server leases.
