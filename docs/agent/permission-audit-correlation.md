# Permission Decision ↔ Action Audit Correlation

Design for an **optional, explicit, id-only** link between the **Permission
Decision** layer and **Action Audit** (and, secondarily, lifecycle **trace** and
**Workflow Session**). This document is **design only**: no code, schema,
OpenAPI, MCP, or migration ships with this file.

**Status:** design for review; implementation is future work.

**Baseline (HEAD context for this design):**

| Area | State |
|---|---|
| Workflow Session lifecycle | Implemented / designed (`session-lifecycle.md`) |
| Session correlation (audit → `wc_sess_*`) | Design only (`session-correlation.md`) |
| Permission System Phase 1/2 | **Implemented** (types, env mode, single pre-exec gate) |
| MCP / API lifecycle trace | Implemented, default **off** (`tool_request_trace`) |
| Action Audit Session boundary | Clarified dual model (`session-model.md`) |

**HEAD reference at design time:** `30ccc22` (`feat: add workflow session close lifecycle`).

**Related docs (link, do not duplicate):**

| Doc | Relationship |
|---|---|
| [`permission-model.md`](permission-model.md) | Decision layer, modes, wire shape, Phase 1/2 status |
| [`session-model.md`](session-model.md) | Workflow Session vs Action Audit Session |
| [`session-correlation.md`](session-correlation.md) | Optional audit → `workflow_session_id` |
| [`session-lifecycle.md`](session-lifecycle.md) | Lifecycle independence; Permission never owns sessions |
| [`architecture-decisions.md`](architecture-decisions.md) | Standing dual-session + permission summary |
| [`AGENTS.md`](../../AGENTS.md) | Executable session / safety invariants |

---

## 1. Why correlate

Permission and Action Audit already answer different questions:

| System | Answers |
|---|---|
| **Permission Decision** | May this **tool invocation** proceed under the active soft policy? |
| **Action Audit** | What **HTTP/API action** happened in this audit window? |
| **Workflow Session ledger** | What happened in this **coding task** (`wc_sess_*`)? |
| **Lifecycle trace** | How long did this **handler invocation** take (optional logs)? |

Without an explicit join key, operators fall back to timestamp windows and
concurrent-request guessing when they need:

> “Which permission decision governed this audited `/api/tools/call`?”  
> “Which audit event (if any) recorded the HTTP envelope around this `wc_perm_*`?”

That becomes brittle as concurrent tools increase. Correlation must stay **cheap
and id-only** — not a second copy of the decision object inside SQLite.

### Primary uses

| Use | How correlation helps |
|---|---|
| Incident triage | From a failed Action event → locate `wc_perm_*` / ledger permission attach |
| Policy debugging | Confirm denied vs auto-approved for a specific HTTP call |
| Log join | Align audit event, structured logs, and optional trace by shared ids |
| Avoid timestamp archaeology | Explicit keys, not “around the same second” |

### Explicit non-drivers

Do **not** treat the following as requirements for this design:

- Approval product UI
- Multi-tenant RBAC
- Distributed tracing as a hard dependency
- Cross-store ACID transactions
- Merging Permission ownership into Audit (or the reverse)

---

## 2. Responsibility boundaries

### 2.1 Permission Decision owns

| Owns | Does **not** own |
|---|---|
| Soft allow / deny / auto-approve / future pending for **one tool request** | HTTP Action Audit Session lifecycle |
| Wire `PermissionDecision` (`status`, `policy`, `reason`, `risk`, …) | Action Audit create / close / idle reuse |
| One `wc_perm_*` request id per decision that reaches the gate | Workflow Session create / close / mode / guards |
| Optional attach to tool output and Workflow ledger tool-call events | Authoritative “list of audit events for this decision” |
| Mode config (`WEBCODEX_PERMISSION_MODE`) | Trace enablement or OTel exporters |

**Invariants (must):**

1. **One tool request → at most one Permission Decision** at the dispatch gate
   (kernel reuses; does not re-evaluate).
2. **Permission never creates, switches, or closes** a Workflow Session.
3. **Permission never creates or closes** an Action Audit Session.
4. **Hard safety remains independent** of soft permission mode.

### 2.2 Action Audit owns

| Owns | Does **not** own |
|---|---|
| HTTP / API action facts (endpoint, status, duration, redacted summaries) | Soft policy evaluation |
| Action Audit Session grouping (UUID, idle open/close) | Allow / deny of tool mutation |
| SQLite rows (`action_sessions`, `action_events`) | Full `PermissionDecision` objects as source of truth |
| Optional **correlation ids** (pointers only) | Pending approval state machine |

**Invariants (must):**

1. **Audit records observation; it does not decide.**
2. **Audit must never reverse-control** permission outcomes (no “audit said
   success → treat as approved”).
3. **Audit write failure must not invent denials** in default / frictionless
   modes (existing rule in `permission-model.md` §9).
4. **Missing correlation is normal** — same as unlinked Workflow Session
   correlation.

### 2.3 Shared “evidence sinks” (record only)

```text
┌──────────────────────────────────────────────────────────────┐
│ AuthN / scopes / agent capability                            │
├──────────────────────────────────────────────────────────────┤
│ Hard safety (session guard, path, secrets, project)          │
├──────────────────────────────────────────────────────────────┤
│ Permission Decision  ←── single evaluate per tool request    │
├──────────────────────────────────────────────────────────────┤
│ Tool execution (only if decision allows)                     │
├──────────────────────────────────────────────────────────────┤
│ Evidence sinks (optional, non-authoritative for policy):     │
│   · Workflow ledger  (permission object on tool-call event)  │
│   · Action Audit     (ids only; no full decision copy)       │
│   · Lifecycle trace  (joinable ids in logs; no policy state) │
└──────────────────────────────────────────────────────────────┘
```

### 2.4 What “association” is **not**

| Forbidden coupling | Why |
|---|---|
| Audit controls Permission | Observation must not become policy authority |
| Permission creates Session | Session identity is explicit product action |
| Copy full `PermissionDecision` into every audit row | Duplicates wire object; redaction / drift risk; larger SQLite |
| Strong SQLite ↔ JSON ledger transaction | Independent stores; best-effort join only |
| Infer permission id from timestamps | Same failure mode as forbidden session inference |

---

## 3. Correlation direction

### Recommended direction (one-way)

```text
Action Audit event  (observation)
    └── permission_request_id: Option<String>   # wc_perm_*
    └── workflow_session_id:   Option<String>   # wc_sess_*  (see session-correlation)
    └── server_trace_id:       Option<String>   # handler invocation (optional)

Permission Decision  (policy)
    └── request_id = wc_perm_*                  # stable join key
    └── (no authoritative audit event id list)

Workflow Session ledger  (task evidence)
    └── SessionEvent.permission: Option<PermissionDecision>
    └── (may already hold the full decision for the tool-call)

Lifecycle trace logs  (optional)
    └── server_trace_id + optional permission_request_id in structured fields
```

### Why this direction

| Option | Verdict |
|---|---|
| **A. Audit holds optional permission id** (recommended) | Matches session-correlation; audit is the HTTP envelope; triage starts from “what API call happened?” |
| B. Permission / ledger holds audit event id list | Couples decision store to HTTP transport; Permission becomes a session/audit index |
| C. Bidirectional authoritative lists | Dual-write, drift, and “which is truth?” fights — reject |
| D. Full decision embedded in audit | Violates “ids only”; see §5 |

**Authority rule:** the **Permission Decision object** remains authoritative for
policy fields (`status`, `policy`, `reason`, `risk`, …). Audit may store
**join keys and at most a tiny denormalized subset** for filter UX later — never
a second full decision struct as source of truth.

**Reverse lookup** (permission id → audit events) is a **query-time** concern
only (scan / index later). It is never a required reciprocal field on
`PermissionDecision`.

---

## 4. Correlation key design

### 4.1 Canonical keys

| Key | Format | Namespace | Role |
|---|---|---|---|
| **`permission_request_id`** | `wc_perm_` + UUID (simple form, same as today) | Permission Decision | **Primary** Permission ↔ Audit join |
| **`workflow_session_id`** | `wc_sess_*` | Workflow Session | Task correlation (existing design) |
| **`server_trace_id`** | UUID string (today’s `new_trace_id()`) | Lifecycle trace | Request-handler join when trace enabled |
| **Action Audit `event_id`** | UUID | Action Audit | Audit-native event identity |
| **Action Audit `session_id`** | UUID (not `wc_sess_*`) | Action Audit Session | HTTP grouping only |
| **Workflow ledger `event_id`** | existing ledger event id | Workflow Session | Ledger-native tool-call identity |

### 4.2 Primary join for this design

```text
permission_request_id  ==  PermissionDecision.request_id  ==  wc_perm_*
```

All other ids are **orthogonal optional** joins:

| From | To | Key |
|---|---|---|
| Action Audit event | Permission Decision | `permission_request_id` |
| Action Audit event | Workflow Session | `workflow_session_id` |
| Action Audit event | Trace log lines | `server_trace_id` |
| Workflow ledger event | Permission Decision | embedded `permission.request_id` |
| Trace log | either side | same ids when emitters include them |

### 4.3 What to store on the audit side

**Store (ids / thin pointers only):**

| Field (logical) | Required? | Notes |
|---|---|---|
| `permission_request_id` | Optional | Present only when a decision was produced for this tool request |
| `workflow_session_id` | Optional | Per [`session-correlation.md`](session-correlation.md); independent of permission |
| `server_trace_id` | Optional | Only when the handler already has a trace id |

**Do not store as audit source of truth:**

- Full `PermissionDecision` JSON clone
- Tool arguments / digests (except future approval product — out of scope)
- Pending approval state
- OAuth tokens, secrets, path contents

### 4.4 Optional thin denormalization (later only)

If operator filters need “denied vs auto-approved” without joining the ledger,
a **later** phase may add **one** thin field such as:

```text
permission_status: Option<String>  # wire status only: auto_approved | denied | …
```

Rules if ever added:

1. Must remain a **non-authoritative convenience** for filter/display.
2. Must not invent statuses the decision layer did not emit.
3. Must not replace reading the ledger / tool output for full policy fields.
4. Prefer **not** shipping this in Phase 1 (ids alone are enough).

### 4.5 Naming

| Context | Preferred name |
|---|---|
| Design / logs / future structured ids bag | `permission_request_id` |
| Wire on `PermissionDecision` | existing field `request_id` (value is already `wc_perm_*`) |
| Do not introduce | dual aliases (`decision_id` **and** `request_id` on the same wire) without a named migration |

Internally, docs may say “decision id”; **canonical external correlation name**
for the audit join is **`permission_request_id`** so it is not confused with
shell/agent `request_id` or HTTP request ids.

### 4.6 Cardinality

| Relation | Cardinality |
|---|---|
| Tool request → Permission Decision | **0 or 1** (0 for non-permission tools / hard-deny suppress paths that never attach) |
| Permission Decision → tool request | **1** (one evaluate at dispatch) |
| HTTP audited call → Action Audit event | **0 or 1** (when audit path runs) |
| Permission Decision ↔ Action Audit event | **0..1** soft link via `permission_request_id` |
| Permission Decision → Workflow ledger attach | **0 or 1** when a resolvable workflow start exists |

One HTTP call that dispatches one runtime tool yields **at most one** of each
when all sinks are active — not a fan-out of decisions.

---

## 5. Which outcomes record audit correlation?

### 5.1 Decision outcomes vs Action Audit

Action Audit records the **HTTP/API envelope** (did the handler run, status,
duration). Permission records **soft policy eligibility**. They are independent
enough that:

| Soft permission outcome (wire `status`) | Tool executes? | Emit `PermissionDecision`? | Action Audit may still record HTTP fact? | Put `permission_request_id` on audit when available? |
|---|---|---|---|---|
| *(no decision — not permission-bearing)* | Yes (hard safety still applies) | **No** | **Yes** | **No** (nothing to join) |
| `auto_approved` | Yes | Yes | Yes | **Yes** |
| `audit_only_allowed` | Yes | Yes | Yes | **Yes** |
| `denied` (e.g. `require_approval_not_implemented`, invalid mode) | **No** | Yes | **Yes** (failed / non-success envelope) | **Yes** — denial is high value for triage |
| Future `approved` / `requested` | Yes / No | Yes | Yes | **Yes** |
| Hard safety deny **before** soft evaluate | No | Usually **no** soft object | Yes | **No** soft permission id |
| Hard-denied tool output **after** soft allow | Ran then hard-denied | Soft decision may be **suppressed** on attach | Yes | Prefer **yes if id already known at request end**; do not claim soft success semantics in audit thin fields |

### 5.2 Rules

1. **Denied soft decisions are first-class correlation targets.**  
   Operators need “this API call was blocked by permission mode” without
   hunting only in the ledger.

2. **`auto_approved` / `audit_only_allowed` also correlate.**  
   Frictionless mode is not “invisible.” Correlation still helps policy and
   incident work; it must not add client friction.

3. **Non-permission tools:** no fake `wc_perm_*`, no empty decision object, no
   synthetic audit permission field.

4. **Action Audit existence is orthogonal to soft allow.**  
   A call can be audited and denied; audited and allowed; or (if audit
   middleware not on that route) have a decision with no audit row.

5. **Do not skip audit solely because permission denied.**  
   That would hide the failure envelope operators need.

6. **Do not treat HTTP 200 as “approved.”**  
   Protocol success ≠ soft permission approval (existing permission-model rule).

### 5.3 Failure independence

| Failure | Effect on the other system |
|---|---|
| Permission evaluate fails closed (deny) | Tool does not run; audit may still record the denial response |
| Audit write fails | Default / audit_only modes: **do not** block execution; log warn |
| Ledger permission attach fails | Prefer not to block execution in default modes |
| Trace disabled / missing | Correlation via audit ↔ permission ids still valid without trace |

---

## 6. Do we persist Permission Decision?

### 6.1 What already persists (today)

| Sink | What is stored | Durability |
|---|---|---|
| Tool result `output.permission` | Full wire `PermissionDecision` | Response lifetime (+ client) |
| Workflow Session ledger | Optional `SessionEvent.permission` | In-memory + JSON ledger (bounded) |
| Action Audit SQLite | **No** permission fields today | N/A |
| Dedicated permission SQLite table | **None** | N/A |
| Lifecycle trace | Log lines only; no durable decision store | Process logs |

### 6.2 Standing decision for this design

| Question | Decision |
|---|---|
| Persist full decision in Action Audit? | **No** |
| Persist only `permission_request_id` (and optional thin status later) on audit? | **Yes** (progressive; see §8–§9) |
| New SQLite table / migration for permissions in early phases? | **No** |
| Authoritative durable home for full decision? | Tool response + **Workflow ledger attach when session exists**; not audit |
| Decision without Workflow Session? | Still valid in-process and on tool output; may have **no** long-lived full copy — correlation id on audit (when present) still helps join logs |
| Pending approval durability? | **Out of scope** until real approval (permission-model Phase 5); would need its own store design |

### 6.3 Retention semantics

- `wc_perm_*` is an **identifier**, not a secret.
- Audit may outlive in-memory ledgers: a stored `permission_request_id` may
  become a **dangling pointer** if the ledger was GC’d — acceptable for
  soft correlation (same spirit as unknown-but-well-formed `workflow_session_id`).
- Do **not** recreate a Permission Decision or Workflow Session from a dangling
  audit pointer.

---

## 7. Relationship to `workflow_session_id`

Permission correlation and Workflow Session correlation are **independent soft
pointers** that may appear together on the same Action Audit event.

```text
Action Audit event
  ├── workflow_session_id?     # “which coding task?”
  └── permission_request_id?   # “which soft decision?”
```

| Rule | Detail |
|---|---|
| Independence | Either field may be present without the other |
| No create | Neither Permission nor Audit may invent a `wc_sess_*` to fill correlation |
| No inference | Do not derive `permission_request_id` from `workflow_session_id` or the reverse |
| Ledger attach | When a workflow start exists, ledger may hold the **full** decision; audit still only needs the id |
| Closed / missing session | Permission may still evaluate; ledger attach may skip; audit may still store ids from the request path |
| Lifecycle | Workflow close / archive / eviction does **not** cascade-delete audit rows or permission ids |

Full Workflow ↔ Audit rules remain in
[`session-correlation.md`](session-correlation.md). This document does not
replace that design; it only states that **permission join is a separate key**.

---

## 8. Relationship to trace id

### 8.1 Trace role

| Aspect | Contract |
|---|---|
| Module | `tool_request_trace` / `ToolRequestLifecycle` |
| Enablement | `WEBCODEX_TOOL_REQUEST_TRACE` (default **false**) |
| Scope | One inbound handler invocation (MCP or API tools/call path) |
| Payload | Operational metadata only (timing, sizes, categories) — no tool args/results |
| Field | `server_trace_id` (UUID) |

### 8.2 Correlation rules

1. **Trace never owns approval state** (pending, approved, denied policy).
2. **Enabling permission modes does not require enabling trace.**
3. **Enabling trace does not require permission correlation.**
4. When both are active on the same handler, emitters **may** include:

   ```text
   server_trace_id
   permission_request_id   # if a decision exists
   workflow_session_id     # if known for this request
   action_event_id         # only if already allocated (often after write — optional)
   ```

5. Trace is **not** a durable store and **not** a substitute for Action Audit.
6. Do **not** introduce OpenTelemetry as a hard dependency for this correlation
   model (optional future exporter must map the same logical ids).

### 8.3 Join patterns

| Question | Join path |
|---|---|
| Log lines for one HTTP call | `server_trace_id` |
| Soft decision for that call | `permission_request_id` (from logs and/or audit ids) |
| Coding-task ledger | `workflow_session_id` then ledger `permission.request_id` |
| Operator audit UI row | Action Audit `event_id` → optional ids bag |

---

## 9. Progressive implementation **without** SQLite / OpenAPI / MCP changes

Constraint for early phases: **no database migration**, **no OpenAPI surface
change**, **no MCP schema change**, **no approval UI**.

### 9.1 Existing extension points (use these first)

| Extension point | How it helps |
|---|---|
| Action Audit `ids_json` (JSON object bag) | Already free-form sanitized ids; can hold `permission_request_id` / `workflow_session_id` / `server_trace_id` **without** a new column |
| Action Audit `summary_json` | Prefer **not** for correlation ids (summaries are narrative); keep ids in `ids` |
| Tool result `output.permission` | Already carries full decision + `request_id` |
| Workflow ledger `SessionEvent.permission` | Already persists full decision when session start exists |
| Structured `tracing` fields | Can emit join keys without schema migrations |
| Request-scoped internal context | Thread ids through one handler without new public API |

### 9.2 Phase map (no forced client changes)

```text
Phase 0  Design only (this document)
   ↓
Phase 1  Id propagation on internal paths only
         · populate ids_json / logs when decision exists
         · no migration, no OpenAPI/MCP, no UI
   ↓
Phase 2  Consistent multi-sink emission (audit + ledger + optional trace)
         · still additive ids only
   ↓
Phase 3  Query / operator convenience (optional)
         · filter helpers over ids_json or later columns
         · externalize fields only with product review
   ↓
Phase 4+ Named migration / thin columns / OpenAPI  only if real need
```

### 9.3 What “no SQLite migration” means in practice

| Allowed without migration | Not allowed in early phases |
|---|---|
| Write additional keys into existing `ids_json` | New required columns |
| Read keys opportunistically in internal helpers | GPT Action / OpenAPI new required properties |
| Structured logs | MCP tool input/output schema churn for correlation alone |
| Tests asserting ids bag contents | Dual alias fields for the same key |

### 9.4 Validation without new external contracts

| Input | Behavior |
|---|---|
| No permission decision on path | Omit `permission_request_id` |
| Decision present | Copy `decision.request_id` into audit `ids` when the call is audited |
| Malformed id (should not happen if server-generated) | Do not invent a replacement; log and omit |
| Client-supplied fake permission id | **Do not accept** as authority; correlation ids are **server-emitted** from the evaluate path, not client trust input |
| `workflow_session_id` on audit | Still follows session-correlation rules (explicit / same-request); independent of permission id |

**Trust boundary:** `permission_request_id` on audit is a **server-side
observation** of the decision just produced. It is not a client claim that
changes allow/deny.

### 9.5 Compatibility with session-correlation phases

Session correlation may later add a nullable column
`action_events.workflow_session_id`. Permission correlation **must not** block
on that migration:

| Order | Acceptable |
|---|---|
| Permission ids in `ids_json` first | Yes |
| Workflow id column migration later | Yes |
| Promote `permission_request_id` to a column later | Optional, only if query load needs it |
| Require both columns in one migration | **No** — keep independently revertable |

---

## 10. Phase 1 — minimal implementation plan

**Goal:** make Permission ↔ Action Audit **joinable in practice** for the
primary tool call path, with **zero** schema / OpenAPI / MCP / UI work.

### 10.1 In scope

1. **Server-side only** propagation on audited tool-call routes that already
   write Action Audit events (especially `/api/tools/call` and any path that
   both evaluates permission and records audit).
2. When a `PermissionDecision` exists for the request, include in Action Audit
   `ids`:

   ```json
   {
     "permission_request_id": "wc_perm_…"
   }
   ```

3. Optionally, when already known on the same request without new APIs:
   - `workflow_session_id` (only if session-correlation Phase 2 rules allow —
     otherwise skip until that work lands)
   - `server_trace_id` if `ToolRequestLifecycle` is active **or** the handler
     already allocated a trace id for logging
4. Structured log fields on the decision / deny / complete paths:
   - `permission_request_id`
   - `permission_status` (wire status string) — logs only, not a new audit
     column
   - existing tool name / project where already logged
5. Unit / integration tests:
   - permission-bearing allow → audit `ids` contains `permission_request_id`
   - soft deny → still audited (when audit path runs) with same id
   - non-permission tool → no `permission_request_id`
   - audit write failure still does not change allow path in default mode
6. Docs cross-link only as needed; no release process changes.

### 10.2 Out of scope for Phase 1

| Item | Why |
|---|---|
| SQLite migration / new columns | Constraint |
| OpenAPI / GPT Actions / MCP schema changes | Constraint |
| Approval UI / pending queue | Non-goal |
| Full `PermissionDecision` in `summary_json` | Forbidden by design |
| Query API filters by permission id | Phase 3 |
| Trace required on | Keep default-off |
| Changing `wc_perm_*` format | Unchanged |
| Inferring ids from timestamps | Forbidden |

### 10.3 Suggested implementation sketch (non-normative)

```text
HTTP tools/call (or MCP equivalent with audit)
  → lifecycle / audit start (existing)
  → dispatch
       → PermissionEvaluator.evaluate  (0 or 1 decision)
       → execute or structured deny
       → attach decision to tool output (existing)
       → optional ledger record_permission_decision (existing)
  → audit.record(...)
       → ids = existing ids ∪ { permission_request_id? }
  → trace.log(...)  // optional fields if enabled
```

**Plumbing preference:** pass the optional `PermissionDecision` (or only its
`request_id`) through the **existing request-local path** into the audit
`ids` builder. Avoid process-global maps.

### 10.4 Success criteria

| Criterion | Measure |
|---|---|
| Joinable | Given an Action Audit event from a permission-bearing call, `ids.permission_request_id` matches `output.permission.request_id` |
| Denied visible | Soft-denied calls still produce audit rows with the id when the route audits |
| No friction | Default `dev_auto_approve` behavior and client contracts unchanged |
| No migration | Schema version / SQL migrations untouched |
| No control inversion | Audit contents never feed back into `allows_execution` |

### 10.5 Rollback

- Stop writing the new `ids` keys; old rows simply lack them.
- No migration reverse required.
- Feature can be landed behind a small internal flag **only if** needed; default
  should be “always write when decision exists” once validated (ids are cheap).

### 10.6 Later phases (preview only)

| Phase | Work |
|---|---|
| 2 | Align MCP + API + ledger + optional trace emission; same-request workflow id if session-correlation Phase 2 is ready |
| 3 | Operator query helpers; optional response fields after product review |
| 4 | Optional SQLite column / index if `ids_json` scans hurt; still no full decision blob |
| 5+ | Real approval durability remains a **separate** design (permission-model); correlation keys stay id-only |

---

## 11. Non-goals

This design explicitly does **not**:

1. **Copy the full `PermissionDecision` into Action Audit** as source of truth
2. **Let Action Audit control** allow / deny / pending / auto-approve
3. **Let Permission create, switch, or close** Workflow or Action Audit Sessions
4. **Allow more than one soft decision per tool request** at the dispatch gate
5. **Introduce a database migration** for early correlation phases
6. **Implement approval UI**, notifications, or multi-person approval
7. **Implement real pending approval durability** (separate future design)
8. **Require OpenAPI / MCP / GPT Actions schema changes** for Phase 1
9. **Require lifecycle trace** to be enabled
10. **Merge** Permission, Workflow Session, Action Audit, or Trace into one store
    or state machine
11. **Strong cross-store transactions** or cascading deletes
12. **Infer** permission or workflow ids from time, thread, TCP connection, or
    current Action Audit Session
13. **Accept client-supplied `permission_request_id`** as an authority that
    changes policy outcomes
14. **Pretend `require_approval` is a working human gate** (remains honest deny
    until a dedicated approval design ships)
15. **Ship code with this document** — Phase 0 is docs only

---

## 12. Decision tables (quick reference)

### 12.1 Ownership

| Concern | Owner |
|---|---|
| Soft policy outcome | Permission Decision |
| HTTP action fact | Action Audit |
| Coding-task evidence | Workflow Session ledger |
| Handler timing logs | Lifecycle trace |
| Hard path/secret/session guards | Hard safety (not soft permission) |

### 12.2 Correlation keys

| Key | Written by | Read for |
|---|---|---|
| `permission_request_id` | Server after evaluate | Audit ↔ decision ↔ logs |
| `workflow_session_id` | Explicit / same-request rules | Audit ↔ coding task |
| `server_trace_id` | Handler lifecycle | Log join only |
| Full `PermissionDecision` | Dispatch / ledger / tool output | Policy detail (not audit blob) |

### 12.3 Record matrix (Phase 1 intent)

| Case | Decision object | Ledger attach | Audit HTTP row | Audit `permission_request_id` |
|---|---|---|---|---|
| Read-only tool | No | No | Maybe | No |
| `auto_approved` + session | Yes | Yes | Yes (if audited route) | Yes |
| `auto_approved` + no session | Yes | No | Yes | Yes |
| Soft `denied` | Yes | If start exists | Yes | Yes |
| Hard deny pre-permission | No soft object | Per hard path | Yes | No |

---

## 13. Relationship to existing docs

| Doc | Update expectation after acceptance |
|---|---|
| [`permission-model.md`](permission-model.md) | Phase 4 “audit correlation” points here for detail; modes/status remain authoritative there |
| [`session-correlation.md`](session-correlation.md) | Unchanged authority for `workflow_session_id`; this file adds a **sibling** key |
| [`session-model.md`](session-model.md) / [`session-lifecycle.md`](session-lifecycle.md) | Dual model + “Permission never owns lifecycle” remain standing |
| [`architecture-decisions.md`](architecture-decisions.md) | Optional short pointer later; not required for Phase 0 |
| This file | Full Permission ↔ Audit correlation design |

---

## 14. Summary decisions

| Topic | Decision |
|---|---|
| Responsibility | Permission decides; Audit observes; no control inversion |
| Direction | Audit (and logs) hold optional `permission_request_id` → decision |
| Primary key | `PermissionDecision.request_id` / `wc_perm_*` as `permission_request_id` |
| Full decision on audit | **Forbidden** as source of truth |
| Persist decision | Tool output + optional Workflow ledger; not new permission SQLite |
| Denied / auto_approved | **Both** correlate when a decision exists |
| Non-permission tools | No synthetic decision or id |
| `workflow_session_id` | Orthogonal soft pointer |
| `server_trace_id` | Optional log join; default-off trace; never policy authority |
| Progressive path | Use `ids_json` + logs first; **no** migration / OpenAPI / MCP in Phase 1 |
| Phase 1 | Propagate server-emitted permission id into audit ids + tests |
| This phase | **Docs only** |

---

## 15. Open questions (intentionally deferred)

These do **not** block Phase 0 acceptance or Phase 1 id propagation:

1. Whether operator audit **API responses** should surface `permission_request_id`
   as a first-class field (vs remaining inside `ids`).
2. Whether a thin `permission_status` denormalized field ever justifies a column.
3. Exact interaction order if a future lifecycle **Closed** deny happens before
   soft evaluate (lifecycle doc already prefers lifecycle deny for mutations).
4. Whether MCP-only paths without Action Audit should write a parallel
   lightweight correlation sink (recommendation: **no** — ledger + logs suffice).

Resolve only when a concrete operator or product need appears.
