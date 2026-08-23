# Manual Multi-Window Collaboration

This guide defines bounded collaboration between independent WebCodex Workflow Sessions. It reuses the Session handoff and message board; it is not a scheduler, worker pool, task queue, claim service, shared transcript, or filesystem lock.

## Core model

Assume coordinator Session `C` and worker Session `W`.

`C` owns the collaboration todo and bounded answers. `W` owns the worker's tool calls, validation, review evidence, and workspace activity. They are always independent Sessions: the worker does not resume `C`, and WebCodex does not copy `W` execution history into `C`.

Knowing a `session_id`, `message_id`, worker Session id, Job id, checkpoint id, artifact ref, commit SHA, or PR number is not authority. Every read or mutation still passes the normal caller/project/owner authorization checks. A `recording_session_id` is authorized before it can affect ledger recording, lifecycle/guard inheritance, provenance, or project-mismatch logic. Project-scoped Session targets require both current authorization to the stored project and an immutable creation-time canonical authority-group fingerprint; project-less Sessions use the same internal durable fence. Direct shared-key access and its OAuth shared-key bridge normalize to the same authority group. A legacy project-scoped record that predates the fingerprint remains fail-closed to ordinary Session tools; a coding continuation may upgrade it only when the current caller's unchanged historical `CurrentSessionKey` exactly matches a pre-existing durable binding to that Session, and the fingerprint upgrade commits atomically with the continuation. Project authorization, `session_id` knowledge, or a process-local binding alone is never migration proof. Authenticated legacy records without that proof remain denied, while the trusted local/dev path retains its compatibility behavior. Collaboration never grants filesystem, shell, Computer, artifact, credential, or other project authority.

## Canonical coordinator -> worker flow

1. **Coordinator posts one bounded todo to `C`.** Use `post_session_message(kind="todo")`. Include the objective, scope, prohibitions, exact stable identifiers, and expected answer shape.
2. **Worker starts its own Session `W`.** Do not resume `C` just to accept the assignment.
3. **Worker reads bounded coordinator state.** Use `session_handoff_summary(session_id=C)` and then `list_session_messages(session_id=C, message_id=<todo_id>)`. Exact lookup does not depend on the todo being in a recent-message window.
4. **Worker performs the task under `W`.** Reads, edits, shell/process calls, validation, review evidence, Jobs, checkpoints, and other authoritative activity stay attached to `W`.
5. **Worker completes the exact todo atomically.** Use `complete_session_message(session_id=C, message_id=<todo_id>, answer=<bounded answer>, completion_key=<caller key>)`. On stateless MCP 2026, the `tools/call` arguments additionally carry wrapper metadata `recording_session_id=W`; this is distinct from the concrete business `session_id=C` and is stripped before concrete tool parsing. One Session-store mutation creates exactly one `kind=answer` reply, resolves the todo, and records the todo -> answer correlation.
6. **Coordinator reads the result from `C`.** Use exact todo lookup, `list_session_messages(reply_to=<todo_id>)`, `session_discussion_summary`, or `session_handoff_summary`.
7. **Coordinator re-observes authoritative state.** A message may reference `W`, a Job, checkpoint, artifact, commit, or PR, but the coordinator explicitly reads/revalidates that source before consequential follow-up.

```text
coordinator C
  -> post todo(C)

worker W (independent Session)
  -> handoff(C)
  -> exact todo(C, message_id)
  -> inspect / edit / validate / review under W
  -> complete(C, todo, answer, completion_key)

coordinator C
  -> exact todo(C) + replies(todo)
  -> discussion/handoff(C)
  -> revalidate authoritative source/state
```

## Atomic completion and retries

`complete_session_message` is preferred over separately posting an answer and then resolving a todo.

A successful completion records:

- one answer whose `reply_to` is the exact todo id;
- the todo as resolved;
- `resolved_by_message_id` pointing to that exact answer;
- a bounded completion identity derived from `completion_key`;
- `author_session_id` from the trusted recording Session when the tool call is explicitly recorded under one; otherwise from the existing trusted current-window Session binding when available.

The recording Session wins when it differs from the current-window binding, so provenance follows the Session that actually records the completion evidence. If neither trusted source exists, `author_session_id` is `null`; callers cannot supply a trusted author identity themselves. Stateless MCP 2026 has no reliable transport/window Session identity, so callers that want worker provenance must pass the explicit `recording_session_id` wrapper metadata returned by WebCodex's 2026 `tools/list` schema. WebCodex does not infer it from `mcp-session-id`, HTTP connection state, credentials, project identity, or a recent Workflow Session.

When a collaboration call has both a recording Session and a target Session, WebCodex authorizes both independently and then requires their stored project scopes to match exactly. `project/project` is allowed only for the same project, `project/project` with different projects is denied, both `project/project-less` directions are denied, and `project-less/project-less` is allowed only after both owner authorities have independently matched. The generic cross-project escape flag never widens this collaboration relationship.

For uncertain responses, retry the same `session_id + message_id + completion_key` with the same answer metadata. WebCodex returns the original completion without creating another answer. Reusing the same key with different answer content/metadata fails with `idempotency_conflict`. A different completion after another completion already resolved the todo returns `already_completed` and bounded existing completion identity.

A successful completion is fenced through the Session-ledger writer generation that contains that completion before success is returned. If durable persistence cannot be confirmed, the tool returns `completion_persistence_uncertain` with `failure_kind=outcome_unknown`; retry the same completion key and payload to reconcile the already-possible in-memory mutation instead of posting a second answer. Correlation and idempotency metadata survive restart/persistence restoration; old records without the new fields load with absent provenance. Malformed partial completion structures fail closed and never silently reopen a resolved todo.

## Exact assignment and reply lookup

`list_session_messages` supports narrow exact filters:

```text
message_id=<wc_msg_*>
reply_to=<wc_msg_*>
kind=<optional>
status=<optional>
```

All supplied filters use deterministic AND semantics. `message_id` therefore gives an exact 0/1 lookup even when the Session contains many messages; `reply_to` finds bounded replies to one exact todo.

## Provenance is metadata, not authority

A completed answer can identify the independent worker with `author_session_id`. That value is derived first from the trusted recording Session that owns the completion tool evidence, then from the trusted current-Session binding only when no recording Session exists. It is not a caller-authored claim. In stateless MCP 2026, `recording_session_id` is explicit wrapper provenance metadata, not a transport Session and not an authority grant; the legacy `mcp-session-id` header remains irrelevant.

The coordinator may then explicitly inspect `session_handoff_summary(worker_session_id)` if it has authority to that Session. WebCodex does not copy the worker's transcript, validation, diff review, Job logs, or workspace evidence into the coordinator Session merely because the answer references `W`.

Session message bodies are explicit bounded collaboration payloads. Ordinary tool audit stores metadata such as target Session/message ids, body byte counts, tag counts, correlation ids, completion identity, and safe author provenance; it does not persist a second copy of the full todo/answer body or raw completion key.

## Common collaboration patterns

### Coordinator -> implementation worker

`C` posts an implementation todo with exact scope. Worker `W` uses an isolated worktree/Project when writing, performs implementation and validation under `W`, then atomically completes the todo with the commit SHA and concise result. `C` reads the answer and independently re-observes the branch/commit before merge or further delegation.

### Implementation -> independent reviewer

The implementation Session posts a review todo containing the exact commit/range and review constraints. Reviewer Session `W` stays independent, performs its own source reads/tests/review evidence, then returns findings with `complete_session_message`. A no-findings answer is still bounded collaboration metadata, not an automatic acceptance or merge decision.

### Two independent worktrees in parallel

If two workers may write concurrently, give them separate Git worktrees and separate WebCodex Projects/Sessions. The message board coordinates intent and results only. It does not claim a branch, lease a path, serialize edits, or prevent conflicts.

### Cross-host conceptual example

A coordinator on host A may post a todo in `C`; an authorized worker on host B can read the bounded handoff and exact todo, work under its own Session/Project authority, and atomically answer `C`. The same authorization rules still apply. Session/message ids do not delegate owner or project authority, and cross-owner delegation is outside this workflow.

## Message board is not a lock

Do not treat todo state, `reply_to`, `completion_key`, `author_session_id`, or `resolved_by_message_id` as:

- a filesystem/worktree/branch mutex;
- an automatic task claim or lease;
- ownership transfer;
- proof that only one worker inspected the source;
- authority to mutate another Project.

When multiple workers operate on the same source, use normal Git/WebCodex Project isolation and revalidate current state before acting on collaboration messages.

## Bounded payload guidance

Keep todos and answers small enough to be useful as handoff state. Prefer stable references over copied authoritative objects:

- worker Session id;
- Job id;
- checkpoint id;
- artifact ref;
- commit SHA;
- PR number.

Do not put bearer tokens, OAuth secrets, private keys, credentials, sensitive connection details, full worker transcripts, or hidden reasoning in Session messages.

## Explicit non-goals

This workflow does not add automatic worker spawning, scheduler/worker pool behavior, generic task queues, automatic claims, work leases, filesystem locks, branch locks, shared transcripts, hidden chain-of-thought transfer, cross-owner delegation, webhook/model callbacks, automatic Job-terminal continuation, or implicit authority inheritance.

The human or coordinator still chooses workers and isolated worktrees/Projects. WebCodex supplies bounded durable collaboration state and deterministic completion correlation, not a multi-agent execution scheduler.

## Future extensibility boundary

The current message board is intentionally scoped to a Workflow Session because
that is sufficient for short coordinator/worker handoffs. That storage choice
does not make the Workflow Session the future long-lived conversation object.

If real usage later needs a durable multi-participant chat or discussion space,
introduce an additive Room/Discussion container with its own collaboration
lifecycle. Independent Workflow Sessions may participate in that container, but
they continue to own their own execution/evidence histories and may be created,
closed, or replaced independently of the room.

Room/Discussion state is durable application collaboration state, not model-context
continuity or delivery proof. Membership, presence, a transport/window identity,
or a referenced Workflow Session never proves that a model still retains or has
ever read any room message. Models must explicitly observe the bounded messages
or deltas they need; Room identity must not become a hidden current-session or
context-retention fallback.

Room/Discussion identifiers and participant claims are not bearer capabilities:
every Room read or post still requires the authenticated caller to satisfy that
container's own visibility/posting authorization.

Keep the future layers separate:

```text
collaboration substrate
  bounded messages / replies / provenance / observation
            |
            v
optional orchestrator
  wake-up / routing / worker spawning / scheduling
            |
            v
Workflow Sessions / Jobs / Projects
  authoritative execution and evidence
```

A message or room may reference a Workflow Session, Job, Artifact, checkpoint,
commit, PR, or another collaboration object. Such references only locate the
authoritative object; they never delegate authority, and consumers revalidate
the referenced source before consequential use. Participant roles and presence
are likewise collaboration metadata, not execution permissions.

An optional orchestrator may decide when to wake, route, or spawn work, but it
must not mint, inherit, or cache execution authority from Room membership,
participant role, message authorship, or object references. Every consequential
execution still goes through the normal explicit Project/Workflow Session target,
caller authorization, guards, capability checks, and dispatch-time revalidation.
Collaboration state may be scheduling input; it is not execution-effect truth or
a lease over a Project, Session, Job, or worktree.

Do not introduce Room membership, participant registries, presence, typing
state, generic task leases, or automatic agent routing until a concrete product
need exists. When a second real collaboration container does exist, shared
message semantics may be extracted from the existing model rather than creating
parallel `ChatMessage`, `WorkItem`, and `Todo` systems. Preserve historical
`wc_sess_*` and Session-message semantics through additive fields/objects rather
than reinterpretation migrations.
