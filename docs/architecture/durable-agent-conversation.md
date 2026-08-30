# Durable Agent identity, Conversation, and wake foundation

A1 adds a concrete communication domain without changing the meaning of Workflow Sessions or project Memory. A2 extends that domain with durable wake intent, Endpoint lifecycle/generation fencing, dispatch uncertainty, and exact continuation consume semantics without making model execution part of Conversation or Inbox state.

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
| Task | Reserved future bridge from communication to requested work | implemented by A1 or A2 |
| Workflow Session | Existing execution, provenance, validation, Job, workspace, todo, and guidance context | a chat room |

An Agent Card contains a mutable non-unique handle, display name, description, bounded specialty labels, profile revision, and timestamps. These fields are self-description metadata. Canonical identity is only the Server-generated `agent_id`, and neither identity nor metadata grants Project, filesystem, Runner, Task, or Workflow Session authority.

## Authoritative persistence

The Control-owned SQLite database remains the standalone authoritative transactional store. A1 adds independent tables for:

- Agent identities and owner communication principals;
- Agent Endpoints and their attachment principals;
- Conversations and participants;
- append-only ordered Messages;
- recipient-specific Agent Deliveries;
- operation-scoped idempotency records;
- coalesced Agent Wake Intents and Endpoint/generation-bound Wake Delivery Attempts.

This reuses the existing durable database lifecycle instead of adding another JSON truth or process-local registry. Generic `Database::open` is storage-only: it may run schema migration and owner-independent housekeeping, but it never declares a Wake worker dead or performs takeover recovery. The standalone Control Server holds an exclusive instance guard bound to its exact database state for its full lifetime; crash/takeover Wake reconciliation runs only after a successor acquires that ownership proof. This is deliberately standalone coordination, not a distributed lease or cluster protocol. The schema is concrete to the current communication/wake use case rather than a generic actor/event framework. It does not assume SQLite is process memory, and the domain can later be mapped to another transactional backend without changing its IDs or public semantics.

A Message append, sequence allocation, all requested Agent Deliveries, required Wake Intent coalescing/creation, Conversation update, and idempotency record commit in one immediate transaction. A forced Wake insertion failure therefore leaves no Message, no partial Inbox state, no Wake, no consumed sequence, and no stale idempotency outcome.

## Identity and authorization

`communication:read` and `communication:manage` are independent scopes. Communication principals are stable hashes of an authenticated account/shared-key subject; secrets and hashes are never returned. Runner transport credentials, Project credentials, and project-scoped OAuth subjects cannot become communication principals.

Creating or updating an Agent, attaching an Endpoint, and adding an Agent to a new Conversation require ownership by the same communication principal. An Agent-authored Message additionally requires an active Endpoint bound to that principal and Agent. Human-authored Messages require the current principal to be the Conversation's Human participant.

Conversation participation authorizes only communication. Existing Project, Workflow Session, Runner, filesystem, Session fence, and Memory authority checks are not consulted, inherited, or relaxed.

## Ordering, delivery, and replay

Each Conversation owns a monotonic `next_seq`; Message rows are unique by `(conversation_id, seq)`. Transcript reads use an exclusive `after_seq` cursor and never consume Inbox state.

Each Delivery is unique by `(message_id, recipient_agent_id)` and has its own monotonic `delivery_order`, queued/consumed state, and timestamps. Inbox reads use an exclusive `after_delivery_order` cursor. An offline Agent needs no Endpoint for Delivery creation; a later principal-bound Endpoint can read and consume the queued Delivery.

Message/Delivery cardinality is deliberately independent from model-turn cardinality. Multiple queued Deliveries for one Agent coalesce into a bounded pending Wake Intent while every Message and Delivery remains independently durable. A later Message can update the pending Wake high-water mark/count without replacing its logical identity.

Wake delivery uses a separate Attempt state machine. Claim is Endpoint/generation fenced and leased. Failure before the durable dispatch fence revokes the Attempt and returns the same Wake Intent to `pending`; after the fence, an unconfirmed Host outcome becomes `delivery_unknown` and is never unconditionally redispatched. Endpoint detach, replacement, expiry materialization, and authoritative standalone Server takeover reconcile stale claims or ambiguous dispatched work conservatively. Exact Wake consume is bound to `wake_id`, target Agent, current Endpoint, controller generation, and an opaque per-Attempt consume token. Wake consume never consumes Agent Deliveries, and Delivery consume never consumes a Wake.

Create-Agent, attach-Endpoint, create-Conversation, and post-Message mutations require caller-generated idempotency keys. The store hashes both key and canonical request. Exact replay returns the original durable resource with `replayed=true` and `state_changed=false`; changed reuse is rejected. Exact Message replay is checked before current Endpoint/lifecycle validation so a caller can recover a committed response after the original Endpoint detached without duplicating the Message or Deliveries. Detach and consume are desired-state idempotent.

Request and response payloads, list pages, participant counts, profile fields, Message bodies, active Endpoints, Conversations, and Messages per Conversation have explicit bounds. Audit and Workflow Session context projections retain IDs, counts, cursors, and mutation state but omit Agent description contents, specialty-label contents, Message bodies, and idempotency keys.

## Runtime Console and tool surfaces

The model-visible Control surface provides explicit tools to create/list/update Agents, attach/detach Endpoints, create/list/read Conversations, post Messages, list an Agent Inbox, consume Deliveries, and consume one exact Agent Wake after its durable dispatch fence. Server-owned output schemas remain complete even when a Compact model-facing `tools/list` projection omits `outputSchema`. Endpoint heartbeat/renewal remains Host infrastructure rather than model bookkeeping.

Runtime Console exposes the same domain through hidden same-origin POST routes. Its minimal Chat panel can:

- create and select an Agent;
- inspect its Agent Card and queued count;
- attach/detach the current browser as a non-wake-capable Endpoint;
- create/open a Conversation;
- render participants and the ordered transcript with Human/Agent provenance;
- post Human-authored Messages;
- inspect and consume an attached Agent's queued Deliveries.

The page performs bounded polling on the existing eight-second Runtime Console cadence and renews its own non-wake-capable Endpoint lease through a hidden same-origin route. Polling, refreshing the page, attaching/renewing an Endpoint, and receiving a Delivery do not invoke or wake a model. Endpoint detach on page exit is best effort. Lease validity and generation fencing are checked synchronously at operation admission and are the immediate correctness boundary. Replacement or authoritative Server takeover lazily materializes durable `expired` Endpoint state, while claim recovery lazily releases expired pre-dispatch claims; A2 does not run a periodic Endpoint/Wake reaper and does not promise a database write exactly when wall clock crosses a lease deadline. The durable Agent, transcript, Inbox, and Wake state never depend on browser lifetime.

A2 also defines a narrow Host-neutral `ContinuationAdapter`: preflight occurs before the dispatch fence and must not resume a model, while dispatch happens only after the Attempt is durably prepared. The checked-in deterministic adapter is test-only; A2 does not claim a production ChatGPT/MCP-App auto-resume path.

## Future boundaries

A later slice may add a production Host continuation adapter and dogfood, an explicit operator/Host reconciliation policy for long-lived `delivery_unknown` ambiguity, a concrete bridge such as `Message -> accepted Task -> Workflow Session -> Job -> Conversation reply`, richer participant administration, and cluster storage. ChatGPT MCP Apps remain one possible Endpoint/continuation adapter rather than a core dependency.

Current project-scoped Memory behavior is unchanged. Agent identity is stable enough for a future Memory principal or namespace keyed by `agent_id`; A1/A2 do not migrate Memory, add Agent Skills, schedule work, spawn autonomous workers, implement Task/DAG orchestration, federation, A2A compatibility, PostgreSQL, or distributed multi-Server leases.
