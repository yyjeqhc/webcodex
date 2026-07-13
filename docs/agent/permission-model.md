# Permission Model — Decision Layer for Tool Execution

Design for a **default-frictionless**, **auditable**, and **progressively
enableable** permission model for WebCodex. This document is **Phase 0 only**:
design text. No Rust, config, database, OpenAPI, MCP, or test changes ship with
this file.

**Status:** design for documentation and implementation planning; not product
behavior today beyond the existing scaffold.

**Audience:** agents and maintainers working on self-hosted WebCodex.

**Related docs (link, do not duplicate):**

| Doc | Relationship |
|---|---|
| [`AGENTS.md`](../../AGENTS.md) | Executable safety rules (no push/release/secrets by default) |
| [`SECURITY.md`](../../SECURITY.md) | Security model and redaction expectations |
| [`session-model.md`](session-model.md) | Workflow Session vs Action Audit Session |
| [`session-correlation.md`](session-correlation.md) | Optional one-way audit → `wc_sess_*` link |
| [`architecture-decisions.md`](architecture-decisions.md) | Standing dual-session and API-evolution decisions |
| [`OPERATIONS.md`](../OPERATIONS.md) | Operator-facing `dev_auto_approve` notes |
| [`AUTH_MODEL.md`](../AUTH_MODEL.md) | Authn / tokens / scopes (orthogonal to this decision layer) |

---

## 1. Baseline facts (current scaffold)

### Module home

Permission scaffolding lives at:

- **`src/tool_runtime/permissions.rs`** (not a top-level `src/permissions.rs`)

Supporting pieces:

| Area | Location | Role today |
|---|---|---|
| Risk / “requires permission” metadata | `tool_definition` + `tool_policy.rs` | `requires_permission()`, `permission_risk()` from tool metadata |
| Annotations | `registry/annotations.rs` | MCP-style `readOnlyHint` / `destructiveHint` / … |
| Dispatch hooks | `dispatch.rs`, `kernel.rs` | Build decision, attach to result, optional ledger attach |
| Workflow ledger | `sessions/model.rs`, `sessions/store.rs` | `SessionEvent.permission`, `record_permission_decision` |
| Profile surface | `runtime_info`, coding-task / handoff summaries | `permission_profile_payload()`, `permission_summary_from_events` |
| Output schemas | `registry/output_schemas/common.rs` | Profile enums include permission mode values; keep public names minimal and avoid duplicate aliases |
| Hard guards | session guard, path safety, scopes, agent capability | Independent of permission status machine |
| Action Audit | `action_audit.rs`, `action_audit_sessions` | HTTP/operator facts; no permission fields today |
| Lifecycle trace | `tool_request_trace.rs` | Opt-in request path observation; no permission state |

### Existing capabilities

1. **Default policy string is hard-coded** `dev_auto_approve`
   (`DEFAULT_PERMISSION_POLICY`). There is no env-driven mode switch yet.
2. **`permission_decision_for_tool`** returns a decision only when
   `runtime_tool_requires_permission(tool_name)` is true (write / destructive /
   shell-like tools via metadata). Otherwise returns `None` (no permission
   object on the result).
3. **Every such decision is immediately `status: "auto_approved"`** with
   `reason: "dev_auto_approve"`. No waiting, no UI, no queue.
4. **Risk labels** come from tool metadata / policy overrides, not per-call
   argument inspection. Current labels include roughly:
   `write`, `patch`, `artifact_write`, `destructive`, `shell`, `job`,
   `validation`.
5. **Decision shape** (`PermissionDecision`) today:

   | Field | Meaning today |
   |---|---|
   | `required` | Always `true` when a decision is emitted |
   | `policy` | Always `dev_auto_approve` |
   | `request_id` | `wc_perm_*` UUID |
   | `status` | Always `auto_approved` (when emitted) |
   | `reason` | Policy name echo |
   | `risk` | Risk string from metadata |
   | `tool_name` | Tool name |
   | `project` | Optional project id from the call |

6. **Attachment timing (important):** today the decision is **created before**
   inner dispatch, but **attached after** execution, and **suppressed** when
   `is_hard_denied_output` detects hard-deny outcomes (session guard, path
   policy, sensitive path strings, etc.). Permission does **not** gate
   execution; hard safety runs inside / around the tool path independently.
7. **Workflow Session integration:** if a tool-call start was recorded,
   `record_permission_decision` copies the decision onto the in-flight start
   and the finished ledger event. Summaries
   (`permission_summary_from_events`) aggregate statuses including future
   `approved` / `denied` / `requested` / `hard_denied` buckets that the
   scaffold mostly does not produce yet.
8. **Release recommendation** is a string only:
   `RELEASE_RECOMMENDED_PERMISSION_POLICY = "require_approval"`. No state
   machine, no pending queue, no approve API.

### Missing capabilities

| Capability | Status |
|---|---|
| Configurable permission mode env | Missing |
| Pre-mutation allow / deny / pending gate | Missing (post-hoc annotation only) |
| Real `require_approval` pending queue | Missing |
| Approve / deny operators or API | Missing |
| `audit_only` recommended-vs-executed split | Missing |
| Stable reason codes separate from policy name | Minimal (`reason` ≈ policy) |
| Trace id / decision duration on permission records | Missing |
| Parameter-bound approval tokens | Missing |
| Persistence of pending approvals across restart | Missing |
| Permission fields on Action Audit rows | Missing |

### Existing hook points (implementation later)

Recommended reuse, not reimplementation:

1. **ToolRuntime dispatch** (`dispatch.rs` path after session/project guard and
   agent auth; before or around `dispatch_authorized_inner`) — single place to
   evaluate policy for concrete `ToolCall`s.
2. **Kernel wrapper** (`kernel.rs`) — keep permission semantics unified so
   generic `call_runtime_tool` and MCP do not diverge; avoid double-evaluate /
   double-attach when both layers touch the same call (today both can touch
   permission; Phase 1 should collapse to one evaluator call).
3. **`runtime_tool_requires_permission` / `permission_risk`** — classification
   source of truth.
4. **`is_hard_denied_output` + hard guards** — remain independent “hard safety”
   path; permission mode must not override them.
5. **Optional ledger attach** via existing `SessionEvent.permission` — keep
   format stable; do not invent a parallel ledger event kind in early phases.
6. **Action Audit / lifecycle trace** — correlate later by ids only; do not
   store full permission state machines there.

---

## 2. Design principles

1. **Permission is a decision layer**, not a Session manager.
2. **Workflow Session** provides optional task context and evidence storage.
3. **Permission Decision** answers: may this tool invocation proceed under the
   active mode?
4. **Action Audit** records that an HTTP/API action happened (facts).
5. **Lifecycle Trace** observes request handling (timing, success categories).
   It never owns approval state.
6. **Default mode must match today’s development experience**: no popups, no
   waits, no client changes, no blocked tool calls due to permission UX.
7. **Richer permission ability is off or auto-approve by default.**
8. **Permission must never create, switch, or close** a Workflow Session.
9. **Audit must never reverse-control** permission decisions.
10. **Do not over-design** for multi-party approval, RBAC, or distributed policy
    before a real need appears.

---

## 3. Layering model

```text
┌─────────────────────────────────────────────────────────────┐
│ AuthN / Scopes / Agent capability  (who may call tools)    │
├─────────────────────────────────────────────────────────────┤
│ Hard safety rules  (path, secrets, session guard, project) │
│   — not overridable by permission mode                      │
├─────────────────────────────────────────────────────────────┤
│ Permission Decision  (configurable: auto / audit / future) │
├─────────────────────────────────────────────────────────────┤
│ ToolRuntime mutation / shell / job execution                 │
├─────────────────────────────────────────────────────────────┤
│ Evidence sinks (optional): Workflow ledger, Action Audit,   │
│ lifecycle logs — record, do not decide                      │
└─────────────────────────────────────────────────────────────┘
```

**Auth** answers identity and coarse tool scopes.
**Hard safety** answers absolute product/agent safety.
**Permission** answers optional human/policy friction for high-risk work.
**Sessions / Audit / Trace** answer continuity and observability.

Do not collapse these into one mega-“permission” module.

---

## 4. Permission modes

### Recommended minimal set

| Mode | Default? | Execution | Human wait | Primary purpose |
|---|---|---|---|---|
| **`dev_auto_approve`** | **Yes** | Allow (after hard safety) | Never | Local/self-hosted development efficiency |
| **`audit_only`** | No | Allow (after hard safety) | Never | Shadow recommendations + richer logs |
| **`require_approval`** | No | High-risk may **pending** | Yes (when implemented) | Future real gate |


### Mode semantics

#### `dev_auto_approve` (default)

- All requests that pass hard safety continue to execute.
- No pending state is ever entered.
- Decisions for permission-bearing tools are **`auto_approved`**.
- Development efficiency equals current behavior.
- Decisions remain recordable for handoff summaries and audit correlation.

#### `audit_only`

- Execution path identical to `dev_auto_approve` after hard safety.
- Evaluator still computes a **recommendation** (e.g. “would require human
  confirmation if `require_approval` were on”) and a risk level.
- Recommendation **must not** block, delay, or alter tool success/failure.
- Outcome should be distinct from true approval, e.g.
  **`audit_only_allowed`**, with optional `recommended_outcome` metadata in
  logs/ledger (implementation may store recommendation as reason_code fields
  rather than inventing many statuses).

#### `require_approval` (future; not enabled in Phase 0–3)

- High-risk tools may enter **`pending`** until explicit approve / deny.
- Must not pretend to support this until a real state machine + consumption
  rules exist.
- Read-only / not-required tools still proceed without friction.
- Hard safety still applies **before** and **independently of** approval.

### Modes considered and **not** recommended for v1

| Mode | Why not in minimal set |
|---|---|
| `deny_by_default` | Too hostile for self-use default; folds into `require_approval` + risk rules later if needed |
| Separate “release mode” enum | Prefer env profile docs (`require_approval` when operators want gates), not a parallel enum |
| Per-tool mode overrides in env | Premature; use metadata + one global mode first |

**Recommendation:** implement configuration for the minimal three-mode set above,
starting with `dev_auto_approve`, then `audit_only`, and only add
`require_approval` when a real approval workflow is required.

---

## 5. Decision model

Conceptual structure (fields may evolve; semantics matter more than exact
Rust shape):

```rust
PermissionDecision {
    decision_id,       // e.g. wc_perm_* (today: request_id)
    mode,              // active PermissionMode
    outcome,           // see outcomes below
    policy,            // stable policy id (may equal mode name initially)
    reason_code,       // machine-stable code, not free prose
    risk_level,        // classification label (not the decision)
    tool_name,
    project,           // optional
    workflow_session_id, // optional wc_sess_*
    request_trace_id,  // optional lifecycle / server trace id
    created_at,
    // future (require_approval only):
    // recommended_outcome, param_digest, expires_at, consumed_at, …
}
```

### Field semantics

| Field | Semantics |
|---|---|
| `decision_id` | Unique id for this evaluation (`wc_perm_*`). Not a session id. |
| `mode` | Effective permission mode for this evaluation. |
| `outcome` | What the permission layer decided for **execution eligibility**. |
| `policy` | Named policy profile that produced the outcome (start with mode name). |
| `reason_code` | Stable code (`dev_auto_approve`, `hard_safety_denied`, `approval_required`, `invalid_arguments_skipped`, …). |
| `risk_level` | Risk classification of the tool/call class. Independent of outcome. |
| `tool_name` | Runtime tool name. |
| `project` | Optional resolved/requested project id when known. |
| `workflow_session_id` | Optional `wc_sess_*` **already resolved** by the call path; never invented. |
| `request_trace_id` | Optional lifecycle trace id when tracing is enabled. |
| `created_at` | Decision time (for expiry / audit). |

### Outcomes (do not conflate with protocol success)

| Outcome | Meaning | Execution |
|---|---|---|
| **`not_required`** | Tool class does not enter the permission-bearing set | Proceed (subject to hard safety) |
| **`auto_approved`** | Mode auto-approved a permission-bearing tool | Proceed |
| **`audit_only_allowed`** | Executed under `audit_only`; recommendation recorded | Proceed |
| **`approved`** | Explicit human/operator approval consumed | Proceed once |
| **`denied`** | Permission layer denied (or approval denied) | **Do not** mutate |
| **`pending`** | Waiting for approve/deny (`require_approval` only) | **Do not** mutate |
| **`hard_denied`** (internal/category only, not a permission outcome) | Hard safety blocked; not a soft permission deny | **Do not** mutate |

Notes:

- **HTTP/MCP protocol success** (200, JSON-RPC result envelope) is not
  `approved`. A tool may return a structured error after `auto_approved`.
- Today’s scaffold uses `status` instead of `outcome` and mostly only emits
  `auto_approved`. Renames can stay compatible (`status` as wire alias) if
  implementation chooses gradual migration; prefer one canonical name in new
  design: **`outcome`**.
- Summary counters already anticipate `approved`, `denied`, `requested`
  (pending), `hard_denied`. Align future names: prefer **`pending`** over
  `requested` on the wire when implementing, with a one-time mapping if ledger
  history used `requested`.

### Risk vs outcome

- **High risk ≠ deny.** Under `dev_auto_approve`, high-risk tools still
  `auto_approved` after hard safety.
- **Low risk ≠ skip hard safety.** Path checks and scopes still apply.

---

## 6. Risk classification

### Goals

- Stable, coarse labels for summaries and future gates.
- Prefer **existing tool metadata / annotations / capability**, not ad-hoc
  per-tool switchboards.
- Keep risk labels independent of permission outcomes.

### Current labels (preserve / map, do not thrash)

From `tool_definition` / `tool_policy` today:

| Label | Typical source |
|---|---|
| `write` | Project write tools |
| `patch` | Patch path tools |
| `artifact_write` | Artifact path writes |
| `destructive` | `destructive` metadata |
| `shell` | Shell-like tools |
| `job` | Job lifecycle tools |
| `validation` | Validation capture tools |

### Conceptual risk classes (design map)

These are **product classes**. Implementation may continue to emit the current
string labels, mapping into this taxonomy in docs and future gates:

| Conceptual class | Approx. current label / signal | Notes |
|---|---|---|
| read-only | `readOnlyHint` / no permission required | No permission object today |
| workspace mutation | `write`, `patch` | Edits inside allowed project root |
| process execution | `shell`, `job` | Commands / background jobs |
| destructive filesystem | `destructive` | Deletes / overwrites with higher impact |
| git history mutation | (future finer label if needed) | Prefer hard agent rules over soft gate |
| network access | open-world / shell profiles | Often inherits shell risk |
| release / deployment | (policy + agent rules) | Hard safety: no deploy/publish unless explicit |
| credential-sensitive | sensitive path / secret rules | Hard deny / redaction, not soft approve |

### Rules

1. Do **not** hard-code large temporary per-tool matrices in the evaluator.
2. Extend `ToolDefinition` / metadata when a new class is needed.
3. Annotations (`readOnlyHint`, `destructiveHint`, `openWorldHint`) remain
   discovery hints; permission risk remains the authoritative runtime label.
4. Default `dev_auto_approve` still obeys **hard safety** for high-risk ops.

---

## 7. Configurable policy vs hard safety

### Configurable permission policy (soft)

Examples:

- Whether humans must confirm high-risk tools (`require_approval`).
- Whether to shadow-recommend confirmations (`audit_only`).
- Whether to attach auto-approval metadata (`dev_auto_approve` vs `disabled`).

### Hard safety rules (not overridable by mode)

Aligned with `AGENTS.md` / `SECURITY.md` / existing runtime guards:

| Hard rule | Enforcement home (examples) |
|---|---|
| No push / release / deploy / npm publish without explicit request | Agent contract + operational practice; not a soft approve checkbox |
| No secret / token / `.env` exfiltration | Path denial, redaction, logging rules |
| Stay inside resolved project / allowed roots | Path policy, project resolution |
| `read_only` Workflow Session denies write/shell/job-like tools | Session guard **before** mutation |
| Unknown explicit `session_id` → `unknown_session_id` | Session resolution |
| OAuth / scope insufficient | Scope check |
| Agent capability missing | `agent_authorization` |
| Sensitive / traversal path rejection | File tools + `policy_rejected` / hard deny detection |

**Permission mode must never weaken these.** Auto-approval only means “no
extra human gate after hard safety passes.”

---

## 8. Request processing order

### Recommended pipeline

```text
Tool Request
    ↓
Request validation / parse ToolCall
    ↓
AuthN + scope / agent capability (as today)
    ↓
Resolve explicit Workflow Session context (if any)
    · unknown id → fail; no silent current-session invent
    · session guard (read_only / closed rules) before mutation
    ↓
Classify risk (metadata)
    ↓
Evaluate Permission Policy (mode + risk + optional session context)
    ↓
Decision:
    allow (auto_approved | approved | audit_only_allowed | not_required)
    deny
    pending   (require_approval only; unimplemented until Phase 4+)
    ↓
Tool execution (ToolRuntime mutation)  — only on allow
    ↓
Record result in Workflow evidence / Action Audit / optional trace
```

### Ordering requirements

| Rule | Rationale |
|---|---|
| Permission check **before** mutation | Prevent “execute then pretend to ask” |
| Parse/validation failure → **no misleading approval record** | Invalid args are not `auto_approved` successes |
| `deny` / `pending` → **no** ToolRuntime mutation | Side-effect free wait/deny |
| Hard safety may run before or as part of “allow” | Absolute blocks stay independent |
| Auto-approve path stays **cheap** | Default mode is hot path for every write tool |
| Evaluator **never executes** tools | Pure decision function + persistence of decision only |

### Alignment with today’s code

Today: decision is pre-built as `auto_approved`, tool runs, then decision is
dropped on hard deny and otherwise attached. Phases 1–2 should move to
**evaluate → (optional hard pre-checks) → execute only if allowed**, while
keeping default outcomes identical for successful write tools.

---

## 9. Workflow Session integration

| Topic | Decision |
|---|---|
| `workflow_session_id` | Optional context on the decision when the call path already resolved a `wc_sess_*` |
| Auto-create session | **Forbidden** for permission reasons |
| Implicit current-session fallback | **Forbidden** inside the evaluator; only use session ids the outer call path already resolved under existing explicit/current-session rules |
| Closed / missing session | Permission still evaluates for the tool call; ledger attach is skipped if there is no valid start event |
| `read_only` session | Session **guard** denies write-like tools **before** soft permission; recommend recording guard failure as hard deny / guard error, not as `auto_approved` |
| Permission as Session event | **Recommendation:** keep attaching to existing `tool_call_finished` (and start metadata) via `permission` field; **do not** add a separate ledger event kind in early phases |

### Should Permission Decision become its own Session event?

| Approach | Pros | Cons |
|---|---|---|
| **Embed on tool_call events (recommended)** | Stable ledger format; already implemented; summaries work | Less ideal for long-lived pending spanning many events |
| Separate `permission_decision` event kind | Clear pending lifecycle history | Schema change; risk of dual sources of truth |

For pending approvals (Phase 4+), store **pending records outside the ledger**
(or in a dedicated store), and only mirror final outcome onto the tool-call
event when execution proceeds or is denied. Prefer **not** expanding ledger
schema until required.

---

## 10. Action Audit integration

| Topic | Decision |
|---|---|
| Concepts | **Permission Decision ≠ Action Audit Record** |
| Permission | Why allowed / denied / pending (policy rationale) |
| Action Audit | What HTTP/API action occurred (endpoint, status, duration, redacted ids) |
| Correlation | Optional via `request_trace_id`, tool `request_id` / decision id, and/or `workflow_session_id` (see session-correlation) |
| Transactions | **No** strong transaction across SQLite audit and JSON ledger |
| Cascading deletes | **None** — independent retention |
| Audit write failure | See below |

### Audit write failure vs tool execution

| Mode | If Action Audit write fails | If permission decision persist fails |
|---|---|---|
| `dev_auto_approve` | **Do not** block tool execution | Prefer not to block; log warning |
| `audit_only` | Do not block execution; log | Do not block; log (recommendation lost is acceptable) |
| `require_approval` | Do not treat audit failure as approve | **Must** not execute if approval state cannot be durably recorded when that mode requires durability |

Conservative default for this self-hosted project: **observability failures
must not invent denials in default modes**, and must not invent approvals in
`require_approval`.

---

## 11. Lifecycle Trace integration

- Module: `tool_request_trace.rs`, env `WEBCODEX_TOOL_REQUEST_TRACE` (default
  **false**).
- Trace records operational lifecycle only (trace id, method, tool name,
  duration, sizes, success categories).
- **Must not** hold pending approval state or become a permission store.
- Future permission logs may **include** `request_trace_id` when present for
  joinability; enabling permission modes does not require enabling trace.

---

## 12. Pending / approval state machine (design only)

Minimal states for a future `require_approval` implementation:

```text
pending ──approve──► approved ──consume──► consumed
   │                    │
   ├──deny──────────► denied
   ├──timeout───────► expired
   └──cancel────────► cancelled
```

### Recommended answers (minimal, safe, deferrable)

| Question | Recommendation |
|---|---|
| Is approval one-shot? | **Yes.** One approved decision authorizes one execution consumption. |
| Bound to tool + param digest? | **Yes.** Store tool name + stable hash of **normalized, redacted** argument summary. Never store full secrets/contents. |
| Params change after approve? | **Old approval invalid** (digest mismatch → new pending or deny). |
| Reuse / double consume? | **Forbidden.** Second attempt needs a new decision. |
| Timeout | Configurable later; safe default e.g. short TTL (minutes). Expired ≠ approved. |
| Restart while pending | **Pending does not auto-approve.** Prefer durable store if mode is on; if store lost, treat as expired/cancelled and require re-request. |
| Who can approve/deny? | Local operator / authenticated admin surface only. No multi-party quorum in v1. No model self-approve. |
| Client timeout while pending | Tool call returns structured `pending` / not-executed error; does not mutate. |

### Explicit non-implementation now

Do not ship half a state machine that always returns `auto_approved` while
advertising `require_approval` as active. Until Phase 4 is complete, the mode
must either be **unavailable** (config reject / fall back with clear log) or
**documented as not implemented** and refused at config validation.

---

## 13. Default configuration

Recommended form:

```text
WEBCODEX_PERMISSION_MODE=dev_auto_approve
```

| Rule | Detail |
|---|---|
| Default when unset | **`dev_auto_approve`** |
| Unset must not enter pending | Absolute |
| Illegal value | Fail closed at startup **or** fall back to `dev_auto_approve` with a loud error log — pick one in Phase 1; prefer **startup fail** for unknown modes once the env is introduced, to avoid silent policy drift. Until then, hard-coded default remains. |
| Independent trace switch | Keep `WEBCODEX_TOOL_REQUEST_TRACE` separate (default false). Do not couple. |
| Independent audit DB | Action Audit remains its existing enablement path; no new required switch for default permission mode. |
| Client changes | **None** for default mode. |

`runtime_status` / coding-task permission profile payloads should continue to
advertise the effective policy for operators (already do so for
`dev_auto_approve`).

---

## 14. Observability

### Safe fields to record (future structured logs / summaries)

- decision id  
- mode  
- outcome  
- policy  
- reason code  
- risk level  
- tool name  
- workflow session id (if any)  
- trace id (if any)  
- evaluation duration  
- project id (if any)  
- param **digest** only (never raw args) when approvals exist  

### Forbidden to record

- Full tool parameters  
- File contents / patches / diffs  
- Secrets, tokens, credentials  
- User prompts / chat content  
- Large tool results / stdout / stderr  

This matches existing ledger and `SECURITY.md` redaction expectations.

---

## 15. Compatibility guarantees (for future implementation)

| Surface | Guarantee under default mode |
|---|---|
| Tool behavior | Unchanged vs today |
| `tools/list` | Unchanged |
| `tools/call` result | Unchanged unless optional metadata is explicitly designed; today’s `output.permission` for high-risk tools remains acceptable |
| Clients | No required changes |
| Workflow Session ID / ledger format | Unchanged in Phases 0–3 |
| Action Audit routes | Unchanged |
| Hard safety | Never bypassed by any mode |

---

## 16. Non-goals (this phase and near-term)

- No approval UI  
- No notification system  
- No multi-person approval  
- No RBAC / multi-tenant policy engine  
- No distributed policy service  
- No modifications to current permission Rust code in Phase 0  
- No database migration in Phase 0  
- No Session correlation implementation tied to this doc  
- No MCP / OpenAPI changes in Phase 0  
- No enabling of real `require_approval`  
- **No code in this design round**

---

## 17. Phased implementation plan

### Phase 0 — Design only (this document)

- Deliverables: this file + index/architecture pointers.
- Risk: design drift from code — mitigated by baseline section above.
- Validate: markdown links, `git diff --check`, human review.

### Phase 1 — Types and decision model cleanup

- Clarify `PermissionDecision` fields/outcomes; keep default `auto_approved`.
- Optional: introduce mode enum wired to **hard-coded or env default**
  `dev_auto_approve` without behavior change.
- Collapse double-evaluate paths (kernel vs dispatch) carefully.
- Risk: accidental rename breaks handoff summary tests.
- Validate: `cargo test --bin webcodex session`, `metadata`, focused
  permission/handoff tests; `cargo fmt` / `cargo check`.

### Phase 2 — Unified pre-exec evaluator call (still auto-approve)

- Call evaluator at a single ToolRuntime entry **before** mutation.
- Still always allow under default mode after hard safety.
- Record decision the same way (or better) without changing client UX.
- Risk: ordering bugs (approve recorded on parse failure); double ledger
  writes.
- Validate: write-tool tests still see `auto_approved`; invalid args tests
  never claim approval; guard denial tests never claim approval.

### Phase 3 — `audit_only`

- Add mode + recommendation fields / safe logs.
- Execution unchanged.
- Risk: log volume; accidental blocking if recommendation wired wrong.
- Validate: mode matrix tests; prove execution success independent of
  recommendation.

### Phase 4 — Durable pending / approve / deny (default off)

- Persist pending; approve/deny API or local operator tool; one-shot consume;
  param digest.
- Config default remains `dev_auto_approve`.
- Risk: stuck pendings; restart consistency; security of approve endpoint.
- Validate: never auto-approve when mode is `require_approval`; restart tests;
  digest mismatch tests; no mutation on pending/deny.

### Phase 5 — UI / client entry only if real demand

- Minimal operator UI or CLI.
- Risk: scope creep into multi-tenant product.
- Validate: usability on self-hosted single-operator setup only.

### Phase ordering assessment

The proposed order is **sound** for a self-use project: document → type
cleanup → pre-gate without behavior change → shadow mode → real gate → UI.
Each phase is independently reviewable and revertible. **Do not skip to
Phase 4** without Phase 2 ordering fixed, or pending will race mutations.

---

## 18. Summary for implementers

1. Keep **`dev_auto_approve`** as the default forever until an operator opts in.
2. Treat permission as a **pure decision + optional record**, not session/audit
   ownership.
3. Reuse **tool metadata** for risk; keep **hard safety** supreme.
4. Prefer **embedding** decisions on existing tool-call ledger events.
5. Implement **pending** only with one-shot, digest-bound, durable, default-off
   semantics.
6. Measure success by: **zero friction in default mode**, honest audit when
   enabled, and no bypass of project/session/secret rules.
