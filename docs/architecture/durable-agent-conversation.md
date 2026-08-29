# Durable Agent identity and Conversation foundation

A1 adds a concrete communication domain without changing the meaning of Workflow Sessions or project Memory.

## Domain boundaries

| Concept | Durable meaning | Explicitly not |
| --- | --- | --- |
| Agent | Server-generated `wc_dagent_*` identity plus a mutable Agent Card | a browser window, MCP connection, credential, Project, Workflow Session, or authority grant |
| Agent Endpoint | A current principal-bound Host/Client attachment carrying one Agent | the Agent identity itself, presence truth, or a wake controller |
| Conversation | Durable communication space containing Human and Agent participants | a Workflow Session, Task scheduler, or execution context |
| Conversation Message | Append-only transcript fact with a stable per-Conversation sequence and author provenance | recipient read state or a current-state delta |
| Agent Delivery | Recipient-specific queued/consumed Inbox state pointing to one Message | a duplicate Message or model invocation |
| Task | Reserved future bridge from communication to requested work | implemented by A1 |
| Workflow Session | Existing execution, provenance, validation, Job, workspace, todo, and guidance context | a chat room |

An Agent Card contains a mutable non-unique handle, display name, description, bounded specialty labels, profile revision, and timestamps. These fields are self-description metadata. Canonical identity is only the Server-generated `agent_id`, and neither identity nor metadata grants Project, filesystem, Runner, Task, or Workflow Session authority.

## Authoritative persistence

The Control-owned SQLite database remains the standalone authoritative transactional store. A1 adds independent tables for:

- Agent identities and owner communication principals;
- Agent Endpoints and their attachment principals;
- Conversations and participants;
- append-only ordered Messages;
- recipient-specific Agent Deliveries;
- operation-scoped idempotency records.

This reuses the existing durable database lifecycle and restart recovery instead of adding another JSON truth or process-local registry. The schema is concrete to the first product use case rather than a generic actor/event framework. It does not assume SQLite is process memory, and the domain can later be mapped to another transactional backend without changing its IDs or public semantics.

A Message append, sequence allocation, all requested Agent Deliveries, Conversation update, and idempotency record commit in one immediate transaction. A forced Delivery insertion failure therefore leaves no Message, no partial Inbox state, and no consumed sequence.

## Identity and authorization

`communication:read` and `communication:manage` are independent scopes. Communication principals are stable hashes of an authenticated account/shared-key subject; secrets and hashes are never returned. Runner transport credentials, Project credentials, and project-scoped OAuth subjects cannot become communication principals.

Creating or updating an Agent, attaching an Endpoint, and adding an Agent to a new Conversation require ownership by the same communication principal. An Agent-authored Message additionally requires an active Endpoint bound to that principal and Agent. Human-authored Messages require the current principal to be the Conversation's Human participant.

Conversation participation authorizes only communication. Existing Project, Workflow Session, Runner, filesystem, Session fence, and Memory authority checks are not consulted, inherited, or relaxed.

## Ordering, delivery, and replay

Each Conversation owns a monotonic `next_seq`; Message rows are unique by `(conversation_id, seq)`. Transcript reads use an exclusive `after_seq` cursor and never consume Inbox state.

Each Delivery is unique by `(message_id, recipient_agent_id)` and has its own monotonic `delivery_order`, queued/consumed state, and timestamps. Inbox reads use an exclusive `after_delivery_order` cursor. An offline Agent needs no Endpoint for Delivery creation; a later principal-bound Endpoint can read and consume the queued Delivery.

Create-Agent, attach-Endpoint, create-Conversation, and post-Message mutations require caller-generated idempotency keys. The store hashes both key and canonical request. Exact replay returns the original durable resource with `replayed=true` and `state_changed=false`; changed reuse is rejected. Exact Message replay is checked before current Endpoint/lifecycle validation so a caller can recover a committed response after the original Endpoint detached without duplicating the Message or Deliveries. Detach and consume are desired-state idempotent.

Request and response payloads, list pages, participant counts, profile fields, Message bodies, active Endpoints, Conversations, and Messages per Conversation have explicit bounds. Audit and Workflow Session context projections retain IDs, counts, cursors, and mutation state but omit Agent description contents, specialty-label contents, Message bodies, and idempotency keys.

## Runtime Console and tool surfaces

The model-visible Control surface provides explicit tools to create/list/update Agents, attach/detach Endpoints, create/list/read Conversations, post Messages, list an Agent Inbox, and consume Deliveries. Server-owned output schemas remain complete even when a Compact model-facing `tools/list` projection omits `outputSchema`.

Runtime Console exposes the same domain through hidden same-origin POST routes. Its minimal Chat panel can:

- create and select an Agent;
- inspect its Agent Card and queued count;
- attach/detach the current browser as a non-wake-capable Endpoint;
- create/open a Conversation;
- render participants and the ordered transcript with Human/Agent provenance;
- post Human-authored Messages;
- inspect and consume an attached Agent's queued Deliveries.

The page performs bounded polling on the existing eight-second Runtime Console cadence. Polling, refreshing the page, attaching an Endpoint, and receiving a Delivery do not invoke or wake a model. Endpoint detach on page exit is best effort; the durable Agent, transcript, and Inbox never depend on browser lifetime.

## Future boundaries

A later slice may add a concrete bridge such as `Message -> accepted Task -> Workflow Session -> Job -> Conversation reply`, stale Endpoint/presence policy, continuation or wake adapters, richer participant administration, and cluster storage. ChatGPT MCP Apps can be one Endpoint/continuation adapter rather than a core dependency.

Current project-scoped Memory behavior is unchanged. Agent identity is stable enough for a future Memory principal or namespace keyed by `agent_id`; A1 does not migrate Memory, add Agent Skills, schedule work, spawn workers, implement automatic wake, federation, A2A compatibility, PostgreSQL, or distributed leases.
