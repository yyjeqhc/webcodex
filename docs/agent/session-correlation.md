# Session Correlation — Workflow Session ↔ Action Audit Session

Design for an **optional, explicit, one-way** link between the two session
systems. This document is **Phase 0 only**: design text. No code, schema,
OpenAPI, or migration ships with this file.

**Status:** design accepted for documentation; implementation is future work.

**Baseline context:** dual-model naming and non-merge rules live in
[`session-model.md`](session-model.md). Standing summary:
[`architecture-decisions.md`](architecture-decisions.md) §1.

---

## 1. Why correlate

Today the two systems are independent. That is correct for ownership and
lifecycle. Operators and agents still need a **cheap way to answer**:

> “Which coding task (`wc_sess_*`) did this HTTP Action audit event belong to?”

Without an explicit link, people fall back to timestamp windows, concurrent
request guessing, or log archaeology. That is brittle for a self-hosted
internal tool and becomes worse as concurrent tasks increase.

### Primary uses (now and near-term)

| Use | How correlation helps |
|---|---|
| Map HTTP Action audits to a coding task | One optional field on the audit side |
| Incident / failure triage | From a failed Action event → open the Workflow Session ledger |
| Log and audit search | Filter Action Audit rows by `workflow_session_id` |
| Avoid timestamp guessing | Explicit id, not “around the same second” |

### Optional later (not a current requirement)

- Tighter permission or approval policies that *consult* a known Workflow
  Session id when present
- Operator UIs that deep-link from audit detail to session summary

Do **not** treat approval workflows, multi-tenant isolation, or distributed
tracing as drivers of this design. The project is primarily self-use; keep the
model simple.

---

## 2. Core principles

1. **Workflow Session** is the task-execution and evidence context
   (`tool_runtime::sessions`, `wc_sess_*`, JSON ledger).
2. **Action Audit Session** is the HTTP Action grouping and external-call audit
   context (`action_audit_sessions`, SQLite).
3. **Lifecycles are independent.** Creating, closing, or timing out one system
   must not create, close, or transition the other.
4. **Action Audit must not control** Workflow Session create / close / mode /
   guards / tool dispatch.
5. **No inference** of `wc_sess_*` from current Action Audit Session, wall
   clock, thread id, TCP connection, idle-open audit reuse, or global
   current-session bindings used only for tool dispatch.
6. **Correlation is explicit and optional.** Absence is normal and preserves
   today’s unlinked behavior.
7. **Recommended direction is one-way, audit → workflow:**

   ```text
   Action Audit Session / Audit Record
       └── workflow_session_id: Option<String>
   ```

8. **Do not** make Workflow Session the authority for a list of Action Audit
   Session ids. Reverse indexes, if any, are derived query results only.
9. **Missing correlation field** → keep existing behavior (no error, no
   implicit bind).
10. **Unknown or malformed `workflow_session_id` must never silently fall back**
    to another Workflow Session (including current session).

These principles extend [`session-model.md`](session-model.md) §5–§6; they do
not replace Workflow Session invariants in `AGENTS.md` §6.

---

## 3. Field semantics

### Canonical field

```text
workflow_session_id: Option<String>
```

| Property | Contract |
|---|---|
| Type | Optional string |
| When `Some` | Must be a **format-valid** Workflow Session id (`wc_sess_*` prefix and existing format rules under `SESSION_ID_PREFIX`) |
| When `None` / absent | This Action call is **not claimed** to belong to a known Workflow Session |
| Meaning | **Correlation only** — not ownership, not a foreign key with cascade, not a guard input |
| Lifecycle effect | **None** on either system |
| Authority | Stored on the Action Audit side; Workflow Session store does not need a reciprocal list |

### What it is not

- Not a second name for Action Audit `session_id` (UUID / client audit id)
- Not a replacement for tool business input `session_id` or wrapper
  `recording_session_id`
- Not permission to invent or open a Workflow Session
- Not proof that the Workflow Session still exists in the in-memory store

---

## 4. Where to store the field

Three options were considered.

| Placement | Pros | Cons |
|---|---|---|
| **A. Action Audit Session only** (one id per audit window) | Single column; tiny migration | One audit window often spans many HTTP calls and can touch **zero or many** coding tasks; loses per-call accuracy |
| **B. Action Audit Record (event) only** | Per-call precision; matches how failures are diagnosed | Slightly more rows carry the column |
| **C. Both** | Session-level default + per-event override | Extra rules, dual writes, more API surface |

### Recommendation (self-use, low migration cost)

**Phase 1 implement B only: record / event level.**

- Put nullable `workflow_session_id` on the **Action Audit event** path
  (`action_events` / `ActionEventRecord` / write input), not as the sole truth
  on the session aggregate row.
- An Action Audit Session remains a pure **HTTP grouping** window; it does not
  need a single “owning” `wc_sess_*`.
- Skip session-level storage until a concrete operator UI needs a denormalized
  “last seen / primary” workflow id. If added later, it must stay a
  **non-authoritative convenience** derived from events, never the reverse
  source of truth.

**Rationale:** simplest mental model, one migration surface that answers the
primary triage question, no dual-write protocol, no false implication that an
entire audit session belongs to one coding task.

---

## 5. Propagation (explicit only)

Correlation enters the server only through **explicit request data** for that
call. Design-time recommendations (final wire names can be reviewed at
implementation):

### Recommended carriers

| Carrier | Recommended name | Role |
|---|---|---|
| JSON body (especially `/api/tools/call` and audited project routes that already take structured JSON) | `workflow_session_id` | Primary, self-documenting, distinct from tool `session_id` |
| Optional HTTP header for routes without a body field | `x-workflow-session-id` | Explicit affinity for middleware; **not** the same as `x-action-session-id` |
| Server-side request context | Internal only for the duration of one request | Holds the resolved optional value after parse; never a process-global map keyed by connection |

### Explicit vs forbidden

| Allowed | Forbidden |
|---|---|
| Client sets `workflow_session_id` / `x-workflow-session-id` on this request | Infer from open Action Audit Session id |
| For `/api/tools/call`, copy from the **same request’s explicit tool** `session_id` when the dedicated audit field is absent **and** that tool field is present and format-valid (same-request explicit business id) | Copy from principal/transport **current-session** binding |
| Header or body supplied by the caller for this HTTP exchange | Infer from timestamps, job ids alone, thread locals across requests, or long-lived client connection state |
| | Treat audit UUID as a Workflow Session id |

**Same-request tool session propagation (narrow):** only when the request payload
already carries an explicit workflow session identifier intended for this
request. The value must come from explicit request data; it must not be obtained
from current-session fallback when the request omits the field. Prefer using a
dedicated `workflow_session_id` field as the long-term external name so audit
correlation is not confused with tool business input.

### Logging and privacy

- `wc_sess_*` is an **identifier**, not a secret credential.
- Still treat it as ordinary structured metadata: prefer dedicated fields /
  columns over stuffing ids into free-form error text.
- Do not log correlation in the same places secrets, tokens, or authorization
  headers appear.
- Control log volume: do not dump full session ledgers when only the id is
  needed.

---

## 6. Validation behavior

Correlation validation is **not** the same as Workflow Session tool-guard
validation. Correlation never mutates the workflow store and never drives tool
allow/deny.

### Decision table

| Input | Behavior | Rationale |
|---|---|---|
| Field **absent** or JSON `null` | Accept; store `NULL`; existing unlinked behavior | Optional by design |
| Empty string after trim | Treat as absent (`NULL`), or reject as bad parameter — **recommend reject** if the client sent the key with `""` (explicit but invalid) | Avoid ambiguous “I tried to set it” |
| **Malformed** (wrong prefix / fails `wc_sess_*` format rules) | **Reject the request** with a clear parameter error | See comparison below |
| Format-valid, session **not found** in Workflow store | **Accept and store** the id; do **not** create a Workflow Session; do **not** fall back | Audit may outlive in-memory ledgers; correlation is a soft pointer |
| Format-valid, session **read_only** | Allow correlation | Audit write is not a workflow mutation |
| Format-valid, session **closed / finished** | Allow correlation | Post-hoc and late audit events remain useful |
| Must session be **active**? | **No** | Active checks belong to tool dispatch, not audit linkage |

### Malformed id: reject vs ignore+warn

| Approach | Pros | Cons |
|---|---|---|
| **Reject with clear parameter error** (recommended) | Matches “no silent wrong binding”; fails fast; easy to test | Client must fix typo before the Action succeeds |
| Ignore field + log warning | Higher availability if a buggy client sends junk | Hides client bugs; stores “no link” when the client believed there was one; softer than Workflow’s unknown-id discipline |

**Recommendation: reject on malformed format.**

Unknown-but-well-formed ids are different: the server **does not** invent a
session and **does not** substitute another id. Storing a dangling pointer is
acceptable for historical correlation; inventing or remapping is not.

### Interaction with tool dispatch

If the same HTTP request both (a) dispatches a runtime tool with business
`session_id` and (b) records Action Audit:

- Tool guard / `unknown_session_id` rules apply only to **dispatch**.
- Audit correlation uses the rules in this section.
- A tool rejection must not auto-create audit correlation to a different
  session.
- Audit correlation success must not relax tool guards.

---

## 7. Persistence and compatibility

### Future SQLite shape (illustrative; not implemented here)

- Add a **nullable** column on the event table, e.g.
  `action_events.workflow_session_id TEXT NULL`.
- Provide a normal migration in the existing DB migration path.
- Existing rows remain `NULL` (unlinked) — no backfill required.
- Optional index for diagnostics, e.g. partial or plain index on
  `workflow_session_id` where not null — only if query load justifies it.

### What does not change for correlation alone

| Surface | Expectation |
|---|---|
| Workflow JSON ledger | **No** required schema change for this link |
| `wc_sess_*` format | **Unchanged** |
| Action Audit Session id (UUID) | **Unchanged** |
| Default HTTP routes and response shapes | **Unbroken** by a nullable internal column |
| OpenAPI / GPT Actions | Do **not** auto-expose the field; **separate review** if externalized |
| MCP tool schemas | Unchanged unless a later phase deliberately adds a carrier |
| Cross-store transactions | **Not required** |
| Cascading deletes | **Not required** — deleting or expiring one side leaves the other intact |

### Compatibility stance

WebCodex is internal/self-use ([`architecture-decisions.md`](architecture-decisions.md)
§2). Still:

- Prefer additive, nullable columns over renames.
- Do not emit dual alias fields for the same correlation concept.
- Whether operator audit JSON **responses** include `workflow_session_id` is a
  separate product decision at Phase 3; storage can exist before any public
  field is advertised.

---

## 8. Query and observability

### Minimal future queries

1. **By workflow id:** list Action Audit events where
   `workflow_session_id = ?` (optional time range / project filters).
2. **From one audit event:** read its `workflow_session_id` (if any) and, if the
   Workflow store still has that id, open ledger summary tools separately.
3. **No join transaction:** two independent reads; tolerate missing workflow
   rows (dangling correlation) and missing audit rows.

### Semantics

This is **correlation for humans and diagnostics**, not relational ownership:

- No FK enforcement to the JSON ledger.
- No cascade delete when a Workflow Session finishes or is GC’d from memory.
- No requirement that SQLite and the ledger agree at every instant.
- Derived reverse maps (workflow id → audit session ids) are query-time only.

### Observability notes

- Structured logs may include `workflow_session_id` next to Action Audit
  `session_id` and `event_id` when present.
- Metrics: optional counter of correlated vs uncorrelated audited calls — only
  if useful; not a Phase 1 requirement.

---

## 9. Non-goals

This design explicitly does **not**:

1. Merge Workflow Session and Action Audit Session into one type or store
2. Build a unified session state machine
3. Introduce OpenTelemetry / distributed tracing as a dependency for linkage
4. Introduce an approval workflow product
5. Change the permission, OAuth scope, or guard systems
6. Require cross-store (SQLite ↔ JSON ledger) transactions
7. Implement cascading deletes or “session close closes audit”
8. Change existing Session ID formats (`wc_sess_*` or audit UUIDs)
9. Make Workflow Session authoritative for Action Audit id lists
10. Infer `wc_sess_*` from current Action Session, time, thread, or connection
11. **Implement any code, migration, OpenAPI, MCP, CI, or test change in this
    documentation phase**

---

## 10. Phased implementation (each phase independently revertable)

### Phase 0 — Design only (this document)

- Write and link design docs.
- No runtime behavior change.
- **Rollback:** revert doc commits.

### Phase 1 — Data model only

- Nullable `workflow_session_id` on Action Audit **events** (SQLite + internal
  write structs).
- Migration; old rows `NULL`.
- Writers still leave the field unset (no external API requirement yet).
- **Rollback:** stop writing the column; column may remain unused (or reverse
  migration if required by policy).

### Phase 2 — Explicit propagation on internal call path

- Accept optional body/header on audited routes, starting with
  `/api/tools/call`.
- Apply validation rules in §6.
- Optional same-request copy from explicit tool `session_id` only as defined in
  §5.
- Still no requirement that every client send the field.
- **Rollback:** ignore the new input field; leave column nullable.

### Phase 3 — Query and diagnostics

- Filter/list helpers or audit API fields for correlated search.
- Operator-facing display only after a small product review of response shape.
- **Rollback:** hide response fields; keep storage.

### Phase 4 — Stricter policy (only if a real need appears)

- Examples: require correlation for certain consequential routes; use presence
  of a valid workflow id in a future approval design.
- Must not land “just in case.”
- **Rollback:** feature-flag or config off; default remains optional.

Each phase should be a small, reviewable change set. Prefer landing Phase 1
without forcing clients to change.

---

## 11. Relationship to existing docs

| Doc | Role after this design |
|---|---|
| [`session-model.md`](session-model.md) | Dual model, forbidden inference, pointer to this file for correlation detail |
| [`architecture-decisions.md`](architecture-decisions.md) | Short standing decision: one-way optional link, audit → workflow |
| [`AGENTS.md`](../../AGENTS.md) | Executable Workflow Session invariants; no expansion required for Phase 0 |
| This file | Full correlation design, validation, phases, non-goals |

---

## 12. Summary decisions (quick reference)

| Topic | Decision |
|---|---|
| Direction | Action Audit → optional `workflow_session_id` |
| Authority | Audit-side storage; no reciprocal authoritative list on Workflow |
| Placement | Prefer **event/record** level first |
| Lifecycle | Fully independent |
| Missing field | Accept; unlinked |
| Bad format | Reject with parameter error |
| Unknown well-formed id | Store; do not create; do not fall back |
| Active / read_only / closed | Do not block correlation |
| Cross-store consistency | Best-effort correlation queries only |
| This PR / phase | **Docs only** |
