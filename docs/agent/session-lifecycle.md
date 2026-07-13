# Workflow Session Lifecycle

Design for a **minimal, explicit lifecycle** for Workflow Sessions
(`wc_sess_*`). This document is **Phase 0 only**: design text. No Rust, ledger
schema, database, API, or state-machine code ships with this file.

**Status:** design for review; implementation is future work.

**Baseline (HEAD context for this design):**

- Session module split under `tool_runtime::sessions`
- State mutation converged through `SessionStore` / `SessionStoreInner`
- Dual-session model: [`session-model.md`](session-model.md)
- Optional correlation: [`session-correlation.md`](session-correlation.md)
- Permission decision layer: [`permission-model.md`](permission-model.md)

**Related standing docs:**

| Doc | Relationship |
|---|---|
| [`session-model.md`](session-model.md) | What a Workflow Session is; dual model vs Action Audit || [`session-lifecycle.md`](session-lifecycle.md) | Lifecycle states, close/archive semantics, and future transitions |
| [`session-correlation.md`](session-correlation.md) | Optional audit → `wc_sess_*` link (independent lifecycle) |
| [`permission-model.md`](permission-model.md) | Allow/deny decision layer (not session ownership) |
| [`architecture-decisions.md`](architecture-decisions.md) | Standing dual-session summary |
| [`AGENTS.md`](../../AGENTS.md) §6 | Executable session invariants |

---

## 1. Current Session lifecycle facts

This section describes **what the code does today**. It is not the target model.

### 1.1 What exists as data fields

`SessionRecord` / `PersistedSessionRecord` carry:

| Field | Role today |
|---|---|
| `session_id` | `wc_sess_*` identity |
| `project`, `title` | Optional association / label |
| `mode` | `normal` \| `read_only` (mutation policy, **not** lifecycle) |
| `guards` | Effective write/shell denials |
| `created_at`, `updated_at` | Timestamps |
| `events` | Bounded tool-call ledger (`VecDeque`) |
| `messages` | Bounded session message board |
| `project_instructions` | Optional snapshot at create |

There is **no** field such as `status`, `lifecycle`, `closed_at`, or `archived`.

### 1.2 Operational phases (product path, not a state machine)

Operators and agents experience the following phases. They are **product
operations** over a single in-memory (optionally JSON-persisted) record that
remains fully mutable until eviction.

```text
create  →  active use  →  message/event append  →  checkpoint (optional)
        →  handoff (optional)  →  finish (optional)
        →  [still present until LRU eviction]
```

| Phase | How it happens in code | Lifecycle effect |
|---|---|---|
| **Create** | `start_session` / `start_session_with_*` / `start_coding_task` → `SessionStore::start_session_with_options` → `insert_session` | New `wc_sess_*`; empty events/messages |
| **Active** | Session exists in the map; tools may resolve explicit `session_id` or (where allowed) current-session binding | Implicit: “present” = usable for guards, append, query |
| **Message / event write** | `push_event` (`tool_call_started` / `tool_call_finished`, …); `post_message` / resolve | Updates `updated_at`; bounded queues may drop oldest |
| **Checkpoint** | Workspace checkpoint tools (`workspace_checkpoint_*`) under project state dir | **Not** a session lifecycle transition; optional continuity artifact linked by product use, not by a session status |
| **Handoff** | `session_handoff_summary` | **Read-only aggregate**; does not close, archive, or change mode |
| **Finish** | `finish_coding_task` | **Closeout report** (workspace, validation, hygiene, optional handoff, canonical task/evidence outcomes); still **appends** ledger events for nested calls (e.g. `show_changes`); does **not** mark the session closed |

### 1.3 Explicit close / archive today

| Capability | Present? | Notes |
|---|---|---|
| Explicit **close** API / tool | **No** | No `close_session`, no `status=closed` on Workflow Session |
| Explicit **archive** | **No** | No archive state, cold store, or retention worker |
| Product “finish” | **Yes** as summary only | `finish_coding_task` produces task/evidence outcomes; session remains in the store and remains usable for further tools if the client keeps the id |
| Process-local unbind | **Yes** | Current-session **binding** may be unbound; that does not close the Workflow Session record |
| Capacity eviction | **Yes** | When `sessions.len() > max_sessions`, oldest LRU entry is **removed** from the map (and no longer queryable). Eviction is capacity management, **not** a named lifecycle state |
| Persistence restore | **Yes** | JSON ledger may restore sessions on restart; restored sessions are again fully “active” for append/query |

### 1.4 Modes vs lifecycle (do not confuse)## 1.5 Eviction is not a lifecycle transition

LRU eviction or process-level removal is capacity management, not a Workflow Session lifecycle state change.

An evicted session:

- is not implicitly `Closed`;
- does not emit a lifecycle transition event;
- does not become `Archived`;
- must not be treated as a completed task.

Future lifecycle implementation should keep retention/eviction policy separate from explicit lifecycle transitions.



| Concept | Values / behavior | Is lifecycle? |
|---|---|---|
| `SessionMode` | `normal`, `read_only` | **No** — policy for write/shell tools |
| `SessionGuards` | `deny_write_tools`, `deny_shell_tools` | **No** — effective policy |
| Message `status` | `open` / `resolved` | **No** — per-message board state |
| Tool event `status` | `succeeded` / `failed` (on finished events) | **No** — per-call outcome |
| Session **presence** | In map vs missing / evicted | Only coarse “exists” signal today |

### 1.5 Failure and identity facts (unchanged baseline)

These remain true and constrain any future lifecycle field:

1. Explicit `session_id` always wins over current session.
2. Unknown explicit id → `unknown_session_id` (no silent fallback).
3. Guard denial before mutation / agent enqueue when the session id is valid.
4. Event/message append on unknown or already-evicted id does **not** recreate
   the session.
5. Action Audit Sessions have their own open/close/idle semantics and must not
   be treated as Workflow lifecycle.

---

## 2. Target lifecycle model

Minimal linear model. Prefer **four states** only. Do not add intermediate
“paused”, “suspended”, “failed”, or multi-region replicas in this design.

```text
Created
   ↓
 Active
   ↓
 Closed
   ↓
Archived   (optional later; may be deferred indefinitely)
```

### 2.1 Transition rules (target)

| From | To | Trigger (conceptual) | Notes |
|---|---|---|---|
| — | **Created** | Explicit create (`start_session` / `start_coding_task` family) | Immediate; creation is the only entry |
| **Created** | **Active** | First accepted use **or** implicit on create | Implementation may collapse Created+Active into one stored value initially (see Phase 1) |
| **Active** | **Closed** | Explicit **close** (new tool/API in a later phase) | Not Audit, not Permission, not finish-by-side-effect alone unless product later defines finish→close |
| **Closed** | **Archived** | Explicit archive or retention policy (Phase 3+) | Cold / reduced mutability; not required for v1 close semantics |
| Any → remove | — | Capacity eviction / process loss (existing) | Orthogonal capacity behavior; must remain documented as non-lifecycle |

### 2.2 Design choices

1. **Minimal.** Four names are enough for product language and future fields.
2. **Explicit transitions only** for close/archive (see §4).
3. **Created may be ephemeral.** If create always yields a usable session in one
   call, wire representation may start as `active` with `created_at` only.
   The **conceptual** Created state still exists for documentation and for
   “never used after create” diagnostics.
4. **Finish ≠ Close by default.** Today’s `finish_coding_task` remains a
   **closeout report**. Mapping finish → Closed is an explicit product decision
   for Phase 2, not assumed here. Recommendation for Phase 2:

   | Option | Recommendation |
   |---|---|
   | A. Finish stays report-only; separate `close_session` | **Preferred initially** — least surprise vs current finish behavior |
   | B. Optional `close: true` on finish | Acceptable if default is false |
   | C. Finish always closes | Only after explicit product sign-off; breaks “finish then keep querying / posting notes” workflows |

5. **Handoff never transitions.** Always observation/summary over existing state.
6. **Checkpoint never transitions.** Workspace artifact only.

### 2.3 Suggested wire name (for later implementation; not shipped here)

When a field is added (Phase 1+), prefer a single field:

```text
lifecycle: "created" | "active" | "closed" | "archived"
```

Do **not** overload `mode` (`normal` / `read_only`) with lifecycle values.

---

## 3. State definitions

Capability matrix for the **target** model. “Today” column reflects current
code when no lifecycle field exists (everything present behaves like Active).

| Capability | Created | Active | Closed | Archived |
|---|---|---|---|---|
| Meaning | Allocated identity; may be unused | Normal coding-task context | Task intentionally ended; evidence retained hot | Cold retention; not for ongoing work |
| Tool execution (write/shell/job-like) | Allowed if policy/guards allow¹ | **Yes** (subject to mode/guards/permission) | **No** (deny with clear lifecycle error) | **No** |
| Tool execution (pure read tools) | Allowed¹ | **Yes** | **Yes** (recommended) or deny-all (stricter alt) | Query-only tools **Yes**; execution tools **No** |
| Message / event append | Yes (bootstrap events) | **Yes** | **Limited:** optional audit-style append of “closed” marker; **no** normal tool evidence growth after close² | **No** (except system retention metadata if ever needed) |
| Checkpoint create/restore | Yes if Active-equivalent | **Yes** (project tool; session may record) | **No** new checkpoints as part of this session’s work | **No** |
| Query (`session_summary`, handoff, validation_summary, list messages) | **Yes** | **Yes** | **Yes** | **Yes** (may be slower / off hot path later) |
| Permission correlation / ledger attach of decisions | Allowed when session is resolved for a call | **Yes** | Attach on denied-for-lifecycle calls **Yes** (evidence); do not invent session | Historical read only |
| Permission **create** of session | **Never** | **Never** | **Never** | **Never** |
| current-session bind | Allowed to this id | **Yes** | **Discouraged / no** — bind only Active (recommended) | **No** |
| Correlation from Action Audit (`workflow_session_id`) | Allowed (soft pointer) | Allowed | Allowed (post-hoc triage) | Allowed (historical) |

¹ If Created is not stored separately, treat as Active.

² Phase 2 should pick one append policy and test it:

| Policy | Pros | Cons |
|---|---|---|
| **Hard stop:** no event/message append after Closed | Simple | Cannot record a final “session_closed” event after the fact |
| **Soft stop:** allow a small set of system events (close, lifecycle_denied) + message board read; deny tool_call evidence | Better audit trail | Slightly more rules |

**Recommendation:** Soft stop for system lifecycle events only; deny normal
tool_call append and user message post after Closed unless a future design
explicitly allows “post-close notes.”

### 3.1 Error semantics (target)

| Situation | Behavior |
|---|---|
| Unknown `session_id` | `unknown_session_id` (unchanged) |
| Known + Closed + write-like tool | Deny with a **lifecycle** error kind (name TBD at implement time, e.g. `session_closed`) — **not** silent success |
| Known + Archived + mutation | Deny similarly (`session_archived` or reuse closed) |
| Explicit id wins | Still wins; lifecycle checked **after** resolution |
| `read_only` vs Closed | Orthogonal: `read_only` is mode; Closed is lifecycle. Both can deny writes |

---

## 4. Lifecycle trigger authority

### 4.1 Who may create

| Actor | May create Workflow Session? |
|---|---|
| Explicit tools: `start_session`, `start_coding_task` (and equivalent runtime entry points) | **Yes** |
| Operator/admin tooling that deliberately wraps those entry points | **Yes** |
| Action Audit middleware / SQLite audit path | **No** |
| Permission evaluator / modes | **No** |
| current-session bind / get-or-create patterns | **No** create; bind only to an existing id |
| Correlation field `workflow_session_id` on audit | **No** create (store soft pointer only; see correlation doc) |
| Trace / `WEBCODEX_TOOL_REQUEST_TRACE` | **No** |

### 4.2 Who may close (target Phase 2)

| Actor | May close? |
|---|---|
| Explicit close tool or documented finish-with-close option | **Yes** |
| Human/operator via that explicit API | **Yes** |
| Action Audit idle timeout / audit session close | **No** |
| Permission deny / approve / timeout | **No** |
| Handoff / summary / validation_summary | **No** |
| current-session unbind | **No** (binding only) |
| LRU eviction | **Not** a close; removal without Closed transition |
| Automatic “no traffic for N minutes” | **No** in this design (non-goal) |

### 4.3 Who may archive (target Phase 3+)

| Actor | May archive? |
|---|---|
| Explicit archive API or retention policy job (future) | **Yes**, when designed |
| Close itself | **No** automatic archive |
| Audit / Permission / Trace | **No** |

### 4.4 Hard prohibitions

1. **Audit must not automatically close** a Workflow Session.
2. **Permission must not automatically create** a Workflow Session.
3. **current-session must not implicitly change lifecycle** (no bind→Active
   resurrection of Closed; no unbind→Closed).
4. **Trace must not own or mutate** session lifecycle.
5. **Correlation must not transition** lifecycle (link only).
6. **Do not infer close** from `finish_coding_task` success, clean git tree, or
   empty job list unless a named product rule is implemented later.

---

## 5. Relationship to existing systems

```text
                    ┌─────────────────────────────┐
                    │     Workflow Session        │
                    │  identity: wc_sess_*        │
                    │  lifecycle: this document   │
                    │  ledger / messages / mode   │
                    └─────────────┬───────────────┘
                                  │
          ┌───────────────────────┼───────────────────────┐
          │                       │                       │
          ▼                       ▼                       ▼
   Checkpoint (files)      Permission (decision)    Action Audit (HTTP)
   project state dir         wc_perm_*                  UUID sessions
   optional continuity       allow/deny/audit           observation only
          │                       │                       │
          └───────────────────────┴───────────────────────┘
                                  │
                                  ▼
                         Trace (request path)
                    WEBCODEX_TOOL_REQUEST_TRACE
                         observation only
```

### 5.1 Workflow Session internals

| Concern | Ownership | Lifecycle interaction |
|---|---|---|
| **Ledger** (events) | `tool_runtime::sessions` | Append rules depend on Active vs Closed (target); identity stable across states until eviction/archive delete |
| **Messages** | Same module | Same as ledger for post rules |
| **Checkpoint** | Workspace checkpoint store + tools | Independent files; session id may appear in tool args/events but checkpoint does not close sessions |
| **Handoff** | `session_handoff_summary` | Read model over Active or Closed; never transitions |
| **Finish** | `finish_coding_task` | Closeout report; optional future link to close (Phase 2 decision) |
| **Mode / guards** | `SessionMode`, `SessionGuards` | Orthogonal to lifecycle |
| **current-session** | In-memory bindings by principal/transport/project | Not ledger; not lifecycle authority |

### 5.2 Permission

| Rule | Detail |
|---|---|
| Role | Decision layer: may this **tool invocation** proceed under the mode? |
| Session role | Optional context and evidence sink (`SessionEvent.permission`) |
| Create session? | **Never** |
| Close session? | **Never** |
| Order (conceptual) | Resolve session → lifecycle allow? → guards → permission evaluate → execute |
| Closed + permission | Lifecycle deny should win for mutations; permission metadata may still attach on the denial path for auditability |

Full design: [`permission-model.md`](permission-model.md).

### 5.3 Action Audit

| Rule | Detail |
|---|---|
| Role | HTTP/API **observation** and operator grouping (SQLite) |
| Lifecycle | Own open/close/idle for **audit** UUIDs only |
| Workflow close | **Forbidden** as a side effect |
| Correlation | Optional `workflow_session_id` soft pointer; independent of Workflow lifecycle |

Full design: [`session-correlation.md`](session-correlation.md),
[`session-model.md`](session-model.md).

### 5.4 Trace

| Rule | Detail |
|---|---|
| Role | Optional request-handler lifecycle logs (`tool_request_trace`) |
| Scope | One inbound handler invocation (timing, sizes, categories) |
| Session | May mention ids as metadata when already known; **must not** create/close/archive |
| Not a store | No durable session state |

### 5.5 What “association” means

| Pair | Association type |
|---|---|
| Workflow ↔ Checkpoint | Same coding task **may** create checkpoints; no shared state machine |
| Workflow ↔ Permission | Decision attached to a call that may be ledgered under a session |
| Workflow ↔ Action Audit | Optional explicit correlation id only |
| Workflow ↔ Trace | Incidental request observation |
| Action Audit ↔ Permission | Not lifecycle-coupled (future optional fields only) |

---

## 6. Compatibility principles

Any implementation of this design **must** preserve:

| Principle | Requirement |
|---|---|
| **ID format** | `wc_sess_*` (`SESSION_ID_PREFIX`) unchanged |
| **JSON ledger evolution** | Additive fields preferred; `SESSION_LEDGER_VERSION` bump only with a defined load path for old rows (missing lifecycle → treat as `active`) |
| **Unknown session** | Explicit unknown id → `unknown_session_id`; never invent or remap |
| **Explicit session id** | Explicit always wins over current-session |
| **read_only / guards** | Unchanged meaning; not replaced by lifecycle |
| **No silent fallback** | Closed/Archived must not fall back to another session |
| **Dual model** | Do not merge Action Audit close with Workflow close |
| **Internal API stance** | No dual alias fields for the same lifecycle concept; one canonical field |

### 6.1 Default for existing persisted sessions

When Phase 1 introduces a lifecycle field:

```text
missing lifecycle on load  →  active
```

Rationale: today’s sessions are fully usable; treating them as Closed would
break restore-and-continue workflows.

### 6.2 Eviction vs Closed

| Mechanism | Semantic |
|---|---|
| Closed | Intentional end; id may still resolve for query |
| Evicted / missing | Id no longer in store → `unknown_session_id` |
| Archived (future) | May leave hot store; resolution rules defined in Phase 3 |

Do not pretend eviction is close. Optional later: on eviction of an Active
session, only capacity metrics — no fake Closed transition required.

---

## 7. Non-goals

This design **does not** include:

| Non-goal | Why deferred |
|---|---|
| Automatic idle cleanup of Workflow Sessions | Avoid surprise data loss; eviction already bounds memory |
| Retention worker / archive daemon | Phase 3+ product decision |
| Database migration for Workflow Sessions | Workflow store is not Action Audit SQLite |
| UI for session browser / archive | Out of scope |
| Multi-tenant session isolation model | Project is self-hosted / internal; use existing auth boundaries |
| Distributed / multi-node shared session store | Single-process store remains the model |
| Merging Workflow and Action Audit state machines | Forbidden by dual-model rules |
| Pausing, branching, forking, or hierarchical sessions | Over-design |
| Automatic close on `finish_coding_task` without product decision | Compatibility with current finish semantics |
| Permission-driven session provisioning | Permission is not a session manager |
| Changing ledger event schema as part of Phase 0 | Docs only |
| Real-time subscription / session websockets for lifecycle | Not required |

---

## 8. Implementation phases

### Phase 0 — Documentation (this change)

| Deliverable | Status |
|---|---|
| `docs/agent/session-lifecycle.md` | This file |
| Code / ledger / API / DB | **Unchanged** |

**Validation:** Markdown review only.

### Phase 1 — Minimal state field

Goals:

- Add a single lifecycle representation on `SessionRecord` /
  `PersistedSessionRecord` (name TBD at implement time; e.g. `lifecycle`).
- Default / migrate-missing → `active`.
- Surface in `session_summary` (and similar) for observability.
- **No** enforcement of Closed yet (or enforce only if trivial and tested).
- Do **not** change `wc_sess_*`, mode/guards, or dual-model boundaries.

Exit criteria:

- Create sets lifecycle to `active` (or `created`→`active` as chosen).
- Old JSON ledgers load without error.
- Tests for defaulting and serialization.

### Phase 2 — Close semantics

Goals:

- Explicit close path (dedicated tool and/or opt-in finish flag — pick one
  product shape; prefer dedicated close or opt-in).
- Deny write/shell/job-like tools on Closed with a stable error kind.
- Query, handoff, validation_summary remain available.
- current-session: do not bind Closed (recommended); unbind does not close.
- Permission still never creates/closes sessions.
- Audit still never closes Workflow Sessions.
- Document finish vs close interaction in tool descriptions.

Exit criteria:

- Matrix tests: Active vs Closed × read/write tools × unknown id.
- No regression on `unknown_session_id` and `read_only` guards.
- Domain tests: `cargo test --bin webcodex session` (and related) green.

### Phase 3 — Archive / retention (future)

Goals (sketch only):

- Optional `archived` state or off-hot-path storage.
- Explicit archive API or policy job.
- Query may remain; mutations remain denied.
- No automatic production daemon required for self-use unless operators ask.

Exit criteria: separate design amendment when retention is actually needed.

### Phase ordering rules

1. Do not implement Phase 2 enforcement without Phase 1 field (or equivalent).
2. Do not implement Phase 3 before close semantics are clear.
3. Each phase keeps PR scope small; no drive-by OpenAPI bloat.
4. Named migration required before any external JSON/OpenAPI lifecycle field
   becomes a hard client dependency.

---

## 9. Decision summary (for reviewers)

| Decision | Choice |
|---|---|
| Model shape | Created → Active → Closed → Archived (minimal) |
| Today’s gap | No explicit close/archive; finish is report-only; presence = active |
| Finish vs Close | Keep separate unless product opts in (Phase 2) |
| Field vs mode | New lifecycle field; do not overload `SessionMode` |
| Create authority | Explicit session/coding-task tools only |
| Close authority | Explicit close only; never Audit/Permission/current-session/trace |
| Archive | Future; not required for close value |
| Old ledgers | Missing field → `active` |
| Non-goals | Auto cleanup, DB migration, UI, multi-tenant, distributed store |

---

## 10. Open questions for human review

These are intentionally unresolved in Phase 0:

1. Should Phase 2 add `close_session` as a standalone tool, an opt-in on
   `finish_coding_task`, or both?
2. After Closed, are **all** tools denied, or only write/shell/job-like (with
   reads allowed)?
3. May operators post message-board notes after Closed?
4. Should Created be a distinct stored value or only a conceptual state?
5. When max-session eviction removes an Active session, is any metric/event
   required, or is silent drop acceptable (today’s behavior)?

---

## Related docs

- [`session-model.md`](session-model.md) — dual session concepts
- [`session-correlation.md`](session-correlation.md) — audit → workflow link
- [`permission-model.md`](permission-model.md) — decision layer
- [`architecture-decisions.md`](architecture-decisions.md) — standing summary
- [`AGENTS.md`](../../AGENTS.md) — executable invariants
- [`../CONCEPTS.md`](../CONCEPTS.md) — product vocabulary
- [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — module map
