# Session Model — Two Non-Interchangeable Concepts

WebCodex uses the word **session** for two independent systems. They share
casual vocabulary only. They must not be merged, cross-wired, or inferred from
each other.

Executable constraints that agents must obey live in
[`AGENTS.md`](../../AGENTS.md) §6 (Session). Standing architecture summary:
[`architecture-decisions.md`](architecture-decisions.md) §1.

---

## Formal names

| Formal name | Casual aliases (avoid in design) | Implementation home |
|---|---|---|
| **Workflow Session** | coding session, tool ledger session, `wc_sess_*` session | `tool_runtime::sessions` |
| **Action Audit Session** | HTTP action session, audit session, operator action trail | Internal module `action_audit_sessions` (SQLite table still named `action_sessions` for compatibility) |

When writing code, docs, or reviews, prefer the formal names above. If a
statement is true for only one kind, name that kind explicitly.

---

## 1. Workflow Session

### Purpose

Bounded **coding-task continuity and evidence** for MCP, GPT Actions, and
runtime tools. It records what happened in a task so review, validation,
handoff, and finish can reason about the same unit of work.

### Responsibilities

- Coding task start / finish lifecycle
- Tool-call evidence (bounded, redacted)
- Checkpoint-related task continuity
- Session-local message board
- Validation evidence and closeout summaries
- Handoff / finish tooling (`session_handoff_summary`, `finish_coding_task`, …)

### Identity

| Aspect | Contract |
|---|---|
| ID form | `wc_sess_*` (`SESSION_ID_PREFIX`) |
| Business field | `session_id` on tools that take a workflow session as input |
| Recorder field | `recording_session_id` on generic wrappers (metadata only; stripped before concrete tool dispatch) |

### Storage and ownership

| Aspect | Contract |
|---|---|
| Module | `tool_runtime::sessions` (model, store, events, JSON persistence) |
| Primary store | In-memory session store |
| Durability | JSON-oriented session ledger (bounded events/messages per session) |
| Current-session map | In-memory bindings isolated by principal, transport, and resolved project |

### Lifecycle (sketch)

Typical product path:

1. `start_coding_task` / `start_session` creates a Workflow Session and returns
   `wc_sess_*`.
2. Subsequent tool calls may pass explicit `session_id` or, where allowed, fall
   back to a current-session binding.
3. Guards, validation, messages, and checkpoints accumulate evidence on that
   ledger.
4. `finish_coding_task` or handoff tools close out or summarize the task.

This is **not** the same state machine as Action Audit Sessions. Lifecycle
tools and error kinds (`unknown_session_id`, `read_only` denials, guard
failures) apply only to Workflow Sessions.

### Invariants (must)

These are also summarized in `AGENTS.md` §6:

1. **ID format:** Workflow Session IDs use `wc_sess_*`. Do not change the
   prefix, ledger event schema, or lifecycle semantics without an explicit
   design task.
2. **Explicit wins:** An explicit `session_id` always wins over current session.
3. **Unknown rejects:** Unknown explicit `session_id` → `unknown_session_id`.
   Never silently fall back to current session.
4. **Read-only mode:** `read_only` sessions deny write-like and shell/job-like
   tools.
5. **Guards first:** Guard denial happens before mutation or agent enqueue;
   record a failed session event when the session id is valid.
6. **Business vs recorder:** `session_summary` (and similar) required
   `session_id` is business input; do not replace it with current session or
   with `recording_session_id`.
7. **No inference from HTTP audit:** Never derive a `wc_sess_*` id from an
   Action Audit Session id (or from `x-action-session-id` / audit SQLite rows).

---

## 2. Action Audit Session

### Purpose

**HTTP Action call auditing** and operator-facing grouping of external API
requests. It answers “what HTTP/API actions happened in this audit window?” —
not “what is the coding task ledger for this repo work?”.

### Responsibilities

- Group HTTP Action / REST audit events under one audit session
- Persist action audit records (endpoints, status, durations, redacted summaries)
- Idle open-session reuse and explicit close for operator audit views
- Aggregate stats for read-only audit APIs

### Identity

| Aspect | Contract |
|---|---|
| ID form | UUID string (or client-supplied id via headers/query), **not** `wc_sess_*` |
| Request affinity | Headers `x-action-session-id` / `x-webcodex-session-id`, or query `action_session_id` |
| Default creation | Server may create a new UUID when no open recent session is reused |

### Storage and ownership

| Aspect | Contract |
|---|---|
| Internal module | `action_audit_sessions` (crate-private; formerly the module path `action_sessions`) |
| HTTP handlers | `audit_http` under `/api/audit/*` |
| Persistence | SQLite tables `action_sessions` and `action_events` |
| Related types | `ActionSessionRecord`, `ActionEventRecord`, DB helpers in `db/audit.rs` |

### Lifecycle (sketch)

1. An audited HTTP request arrives; optional explicit audit session id is read
   from headers/query.
2. `get_or_create_active_session` attaches the event to an existing open session
   (explicit id, or recent idle-open session) or creates a new one.
3. Events are written to SQLite; session aggregate counters update.
4. Operator APIs list sessions, fetch one session with events, or compute stats.
5. Sessions may be closed (`status = closed`); idle open sessions time out for
   reuse purposes (`ACTION_SESSION_IDLE_TIMEOUT_SECS`).

This lifecycle is **orthogonal** to Workflow Session start/finish tools.

### What it is not

- Not a coding / workflow session
- Not a substitute for `start_coding_task` evidence
- Not an input to `session_summary`, message board, or `finish_coding_task`
- Not automatically correlated to any `wc_sess_*`

---

## 3. No unified state machine

The two systems:

- Use different ID namespaces
- Use different storage backends
- Expose different APIs (runtime tools / MCP vs `/api/audit/*`)
- Define different open/close and failure semantics

There is **no** shared session state machine, no shared store, and no
requirement that a request participate in both. A single HTTP call may
incidentally touch both only when a tool invocation both (a) records workflow
ledger evidence via `session_id` / `recording_session_id` and (b) is wrapped by
HTTP action audit middleware — those are still two separate writes.

---

## 4. Do not merge implementations

Do **not**:

- Fold Action Audit Sessions into `tool_runtime::sessions`
- Store workflow ledger events in SQLite `action_*` tables
- Reuse `wc_sess_*` as SQLite `action_sessions.session_id` by convention
- Drive workflow guards from audit session status, or audit close from
  `finish_coding_task`
- “Simplify” by making one ID type serve both products

Merge would couple coding-task continuity to HTTP transport audit, break
identity rules, and blur security/guard boundaries. Keep two implementations.

---

## 5. Future association (explicit only)

If product needs to relate the two, use **explicit optional** fields, for
example:

- `workflow_session_id` (a `wc_sess_*`) on an audit event or audit session row
- a generic `correlation_id` shared by both sides when a client supplies one

Rules for any future link:

1. Optional — absence is normal.
2. Explicit — client or tool must set it; server does not invent the link.
3. Validated independently — a bad `workflow_session_id` must not invent a
   workflow session from audit data.
4. Documented as a named migration if it changes SQLite schema, OpenAPI, or
   external JSON.

Until that design ships, code and docs must treat the systems as unlinked.

---

## 6. Forbidden inference

| Forbidden | Why |
|---|---|
| Infer `wc_sess_*` from current HTTP Action Audit Session | Wrong namespace; audit ids are not workflow ids |
| Fall back to Action Audit Session when Workflow Session is missing | Breaks `unknown_session_id` and explicit-wins |
| Treat `/api/audit/session` payload as coding-task summary | Different evidence model and redaction rules |
| Pass audit UUID as tool `session_id` expecting ledger semantics | Unknown or wrong session; not a supported bridge |

---

## 7. Compatibility surface (do not rename casually)

The following names are part of **storage, HTTP, or external API contracts**.
Internal Rust module renames for clarity are allowed; these surfaces are not
renamed without an explicit compatibility migration:

### SQLite

- Table: `action_sessions`
- Table: `action_events`
- Index: `idx_action_sessions_status_last_event`
- Column names and migration history in `db/schema.rs` / `db/audit.rs`

### HTTP routes

- `POST /api/audit/sessions`
- `POST /api/audit/session`
- `POST /api/audit/stats`
- Request affinity: `x-action-session-id`, `x-webcodex-session-id`,
  query `action_session_id`

### JSON / type shapes (illustrative)

- Audit session records (`session_id`, `status`, counters, timestamps, …)
- Audit event views and stats aggregates
- Workflow tool fields: `session_id`, `recording_session_id`, session mode
  values such as `normal` / `read_only`
- Error kinds such as `unknown_session_id`

### OpenAPI / MCP / runtime tool surface

- GPT Action OpenAPI operation ids and schemas that mention workflow
  `session_id` / `recording_session_id`
- MCP tool input schemas for session tools
- Runtime tool names (`start_session`, `start_coding_task`,
  `session_summary`, …)

### Internal vs external naming

| Layer | Current clarity practice |
|---|---|
| Docs / design | Prefer **Workflow Session** and **Action Audit Session** |
| Rust module path | `tool_runtime::sessions` vs `action_audit_sessions` |
| SQLite / HTTP / JSON | Keep existing `action_sessions` / `session_id` names for compatibility |

Renaming a **crate-private** module path does not change wire contracts.
Renaming tables, routes, or serialized field names does.

---

## 8. Quick decision guide

| Question | Answer with… |
|---|---|
| Coding task, guards, validation ledger, handoff? | **Workflow Session** (`wc_sess_*`) |
| HTTP Action audit trail, `/api/audit/*`, SQLite action events? | **Action Audit Session** |
| Tool argument `session_id` on runtime/MCP tools? | Workflow Session (business input) |
| Wrapper field `recording_session_id`? | Workflow Session (recorder metadata only) |
| Header `x-action-session-id`? | Action Audit Session |
| Should these share one store or state machine? | **No** |
| Need a link later? | Optional explicit `workflow_session_id` / correlation id |

---

## Related docs

- [`AGENTS.md`](../../AGENTS.md) — executable Session invariants
- [`architecture-decisions.md`](architecture-decisions.md) — dual-model summary
- [`openapi-guidelines.md`](openapi-guidelines.md) — `session_id` vs
  `recording_session_id` on GPT Actions
- [`../CONCEPTS.md`](../CONCEPTS.md) — product vocabulary (Workflow Session in
  client-facing language)
- [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — module map
