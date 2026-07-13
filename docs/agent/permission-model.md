# Permission Model — Decision Layer for Tool Execution

Design and **implementation status** for a **default-frictionless**, **auditable**,
and **progressively enableable** permission model for WebCodex.

**Status (as of Phase 1/2 on `refactor/session-boundaries`):**

| Phase | State |
|---|---|
| Phase 0 — design | Done |
| Phase 1 — types / mode config / module split | **Implemented** |
| Phase 2 — single pre-exec evaluator gate | **Implemented** |
| Phase 3+ — richer risk, audit correlation, real approval | Not implemented |

This document is the single place for both the standing design and the current
runtime facts. **When design text and code disagree, code wins.** Design
sections below that describe future behavior are marked as such.

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

## Implementation Status

### Module layout (implemented)

Permission lives under **`src/tool_runtime/permissions/`** (not a top-level
`src/permissions.rs`):

| File | Role |
|---|---|
| `mod.rs` | Attach/suppress helpers, hard-deny detection, profile/summary, deny results |
| `model.rs` | `PermissionMode`, `PermissionOutcome`, `PermissionDecision`, env/constant names |
| `evaluator.rs` | **`PermissionEvaluator`** — single evaluation entry |
| `policy.rs` | Mode → decision mapping, `WEBCODEX_PERMISSION_MODE` resolution |
| `risk.rs` | **Classification facade only** (delegates to tool metadata / `tool_policy`) |
| `tests.rs` | Unit tests for modes, outcomes, hard-deny suppress |

Supporting (outside the module, unchanged ownership):

| Area | Location | Role |
|---|---|---|
| Risk / requires-permission metadata | `tool_definition` + `tool_policy.rs` | Source of labels for the risk facade |
| Annotations | `registry/annotations.rs` | MCP-style discovery hints (not the gate) |
| Authoritative gate | `dispatch.rs` | Evaluate once → allow/deny → execute → attach |
| Kernel reuse | `kernel.rs` | Reuses attached decision from output; **does not re-evaluate** |
| Workflow ledger | `sessions/*` | Optional `SessionEvent.permission` attach |
| Runtime wiring | `runtime.rs` | Holds `PermissionEvaluator::from_env()` |
| Hard safety | session guard, path policy, scopes, agent auth | Independent of permission mode |

### Capability matrix

#### Implemented

| Capability | Code fact |
|---|---|
| Module split (model / evaluator / policy / risk) | `permissions/*` |
| Env-driven mode | `WEBCODEX_PERMISSION_MODE` via `EffectivePermissionConfig::from_env` |
| Default when unset / empty | `dev_auto_approve` |
| `PermissionEvaluator` single entry | `evaluator.rs` |
| Authoritative gate at ToolRuntime dispatch | `dispatch.rs` after session/auth guards, **before** `dispatch_authorized_inner` |
| One evaluation per tool request | Dispatch evaluates; kernel only `permission_decision_from_output` |
| Pre-mutation allow/deny | Denied decisions return before inner dispatch |
| Mode: `dev_auto_approve` | Executes; wire `status=auto_approved`, `reason=dev_auto_approve` |
| Mode: `audit_only` | Executes; wire `status=audit_only_allowed`, `reason=audit_only` |
| Mode: `require_approval` | **Does not execute**; `status=denied`, `reason=require_approval_not_implemented` |
| Invalid mode | **Does not execute**; fail-closed deny (`policy=invalid`, `reason=invalid_permission_mode:…`) |
| `wc_perm_*` request id per decision | `PermissionDecision::new` |
| Attach decision to tool output | `add_permission_to_result` |
| Suppress attach on hard-deny tool output | `is_hard_denied_output` after execution |
| Profile payload for operators | `permission_profile_payload` / `runtime_status` |
| Ledger attach when session start exists | `record_permission_decision` |
| Read-only / non-permission tools | Evaluator returns `None` — **no** fake approval object |

#### Partially implemented

| Capability | What exists | What does not |
|---|---|---|
| Risk classification | Coarse labels from tool metadata via `risk.rs` facade (`write`, `patch`, `shell`, …) | Argument-bound risk, full risk engine, git/network/release subclasses as gates |
| `audit_only` semantics | Distinct outcome + allow execution | Rich `recommended_outcome` / shadow recommendation fields beyond status/reason |
| `require_approval` as a mode name | Accepted config; honest pre-exec **deny** | Pending queue, approve/deny, consumption |
| Decision field model | Wire-stable `status` / `policy` / `reason` / `risk` | Design-doc names like `outcome` / `decision_id` / `reason_code` as separate wire fields |
| Summary counters | Count `approved` / `requested` / `hard_denied` if present on ledger | Evaluator does not emit real pending approvals |

#### Designed but not implemented

| Capability | Notes |
|---|---|
| Real pending state machine | `pending` → approve/deny/timeout/consume |
| Parameter-bound approval tokens / digests | One-shot consume rules |
| Durable pending across restart | Store + expiry |
| Approve / deny HTTP or operator API | |
| Approval UI / notifications | |
| Permission fields on Action Audit rows | Optional correlation only (Phase 4 plan) |
| Lifecycle trace permission fields | Optional join by ids later |
| RBAC / multi-tenant policy | Explicit non-goal near-term |
| SQLite migration for permissions | None today |
| Startup hard-fail on invalid mode | Invalid mode is **request-time fail-closed deny**, not necessarily process abort |

#### Explicitly deferred

- Multi-person approval / quorum
- Distributed policy service
- Model self-approve
- Merging Permission with Workflow Session or Action Audit ownership
- Moving all path/sensitive checks into the evaluator (they remain tool-internal hard safety)

---

### Current execution chain (code order)

Authoritative path is **ToolRuntime dispatch** (`dispatch.rs`), not the kernel
wrapper. Approximate order for a normal tool call that reaches the permission
gate:

```text
request / project resolution / current-session binding
  → session existence + project-mismatch guards (as applicable)
  → tool-disabled check
  → session guard (e.g. read_only) before mutation
  → record tool_call_started (when a workflow session is in play)
  → agent authorization
  → PermissionEvaluator.evaluate  (exactly once when this path is taken)
  → if decision present and !allows_execution:
        structured permission_denied result
        attach the same PermissionDecision
        optional ledger record
        return  (no tool execution / no agent enqueue)
  → tool execution (dispatch_authorized_inner)
  → hard tool-internal safety checks remain effective
        (path policy, sensitive paths, etc. still run inside tools —
         they were not all moved in front of the evaluator)
  → if hard-denied output: suppress permission attach
  → else attach the same PermissionDecision + optional ledger record
  → finish ledger / result recording
```

**Kernel** (`kernel.rs`): builds the `ToolCall`, may record an outer wrapper
session, then calls dispatch. After dispatch returns it **reuses**
`permission` from the result output for outer ledger attach. It does **not**
call `PermissionEvaluator` again (no second `wc_perm_*` id).

### Current mode behavior matrix

| Mode | Executes permission-bearing tools? | Wire `status` | Typical `reason` |
|---|---|---|---|
| `dev_auto_approve` | **yes** | `auto_approved` | `dev_auto_approve` |
| `audit_only` | **yes** | `audit_only_allowed` | `audit_only` |
| `require_approval` | **no** | `denied` | `require_approval_not_implemented` |
| invalid mode | **no** | `denied` | `invalid_permission_mode:{value}` |

Config:

| Rule | Behavior |
|---|---|
| Env var | `WEBCODEX_PERMISSION_MODE` |
| Unset / empty / whitespace | `dev_auto_approve` |
| Known values | `dev_auto_approve`, `audit_only`, `require_approval` |
| Unknown non-empty value | `EffectivePermissionConfig::InvalidMode` → deny before execution |
| Default development experience | No human approval, no client changes, no extra wait, no UI required |

Non-permission tools (`tool_requires_permission` false): evaluator returns
`None`; no `output.permission` object; tools still subject to hard safety.

### Invariants (must hold)

1. **One permission decision per request** that reaches the gate for a
   permission-bearing tool (one evaluate call on the dispatch path).
2. **One `wc_perm_*` request id** per that decision.
3. **Kernel does not re-evaluate** — only reuses the attached decision.
4. **`require_approval` must not silently fall back to auto-approve.**
5. **Invalid configuration must not fall back to default allow** — fail closed
   (deny execution) for permission-bearing tools.
6. **Hard safety must not be bypassed by permission mode** (including
   `dev_auto_approve` / `audit_only`).
7. **Read-only / not-required tools must not invent fake approval records.**
8. **Hard-denied tool outcomes suppress soft permission attach** so auto-approve
   metadata is not claimed on policy/session/path hard denies.

### Default development experience

With no env set (or explicit `dev_auto_approve`):

- No human approval step
- No client protocol changes required
- No pending wait
- No approval UI
- Existing self-hosted write/shell/job flows continue as before
- Permission metadata still appears on permission-bearing successes for handoff /
  summaries when not suppressed by hard deny

---

## 1. Design principles

1. **Permission is a decision layer**, not a Session manager.
2. **Workflow Session** provides optional task context and evidence storage.
3. **Permission Decision** answers: may this tool invocation proceed under the
   active mode?
4. **Action Audit** records that an HTTP/API action happened (facts).
5. **Lifecycle Trace** observes request handling (timing, success categories).
   It never owns approval state.
6. **Default mode must match the frictionless development experience**: no
   popups, no waits, no client changes, no blocked tool calls due to permission
   UX (under `dev_auto_approve`).
7. **Richer permission ability is opt-in** (`audit_only`, future real approval).
8. **Permission must never create, switch, or close** a Workflow Session.
9. **Audit must never reverse-control** permission decisions.
10. **Do not over-design** for multi-party approval, RBAC, or distributed policy
    before a real need appears.

---

## 2. Layering model

```text
┌─────────────────────────────────────────────────────────────┐
│ AuthN / Scopes / Agent capability  (who may call tools)    │
├─────────────────────────────────────────────────────────────┤
│ Hard safety rules  (path, secrets, session guard, project) │
│   — not overridable by permission mode                      │
│   — session/project guards often before permission;         │
│     path/sensitive checks often still inside tools          │
├─────────────────────────────────────────────────────────────┤
│ Permission Decision  (configurable soft policy)             │
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

## 3. Permission modes

### Mode set

| Mode | Default? | Execution today | Human wait | Primary purpose |
|---|---|---|---|---|
| **`dev_auto_approve`** | **Yes** | Allow (after pre-exec permission allow + hard safety) | Never | Local/self-hosted development efficiency |
| **`audit_only`** | No | Allow; distinct decision status | Never | Shadow / audit-style labeling without blocking |
| **`require_approval`** | No | **Denied** until real approval is implemented | N/A today | Future human gate (name reserved; no fake approve) |

### Mode semantics

#### `dev_auto_approve` (default)

- Permission-bearing tools that pass the gate execute.
- No pending state.
- Decision: **`status: auto_approved`**, **`reason: dev_auto_approve`**.
- Development efficiency equals the pre-permission-gate product behavior for
  default mode.

#### `audit_only`

- Execution allowed like `dev_auto_approve` for permission-bearing tools.
- Decision: **`status: audit_only_allowed`**, **`reason: audit_only`**.
- Must not block, delay, or alter success/failure relative to hard safety and
  tool logic.
- Richer “would have required approval” recommendation metadata is still
  design-level (see partially implemented).

#### `require_approval` (name reserved; **not** a working approval workflow)

- **Current implementation:** always **deny before mutation** with
  `reason: require_approval_not_implemented`.
- Must **not** emit `auto_approved` while this mode is selected.
- Read-only / not-required tools still proceed without a permission object.
- Hard safety still applies independently.
- Real `pending` / approve / deny is **deferred** (see later phases).

### Modes not in the minimal set

| Mode | Why not |
|---|---|
| `deny_by_default` | Hostile for self-use default; fold into risk rules later if needed |
| Separate “release mode” enum | Prefer documenting `require_approval` when operators want gates |
| Per-tool mode overrides in env | Premature; metadata + one global mode first |

---

## 4. Decision model

### Wire shape today (`PermissionDecision`)

Serialized field names are **wire-stable** for ledger / handoff clients:

| Field | Meaning |
|---|---|
| `required` | `true` when a decision is emitted |
| `policy` | Mode / policy name (`dev_auto_approve`, `audit_only`, `require_approval`, or `invalid`) |
| `request_id` | `wc_perm_*` UUID (design name: decision id) |
| `status` | Wire name for outcome (`auto_approved`, `audit_only_allowed`, `denied`, …) |
| `reason` | Stable-ish code or policy echo (`dev_auto_approve`, `require_approval_not_implemented`, `invalid_permission_mode:…`) |
| `risk` | Coarse risk label from metadata facade |
| `tool_name` | Runtime tool name |
| `project` | Optional project id from the call |

Internal typed enums (`PermissionMode`, `PermissionOutcome`) map to these
strings. Design docs may say **outcome**; the **serialized field remains
`status`**.

### Outcomes (execution eligibility)

| Outcome (wire `status`) | Meaning | Execution |
|---|---|---|
| *(no object — not required)* | Tool class not permission-bearing | Proceed (hard safety still applies) |
| **`auto_approved`** | Default mode allowed | Proceed |
| **`audit_only_allowed`** | Audit mode allowed | Proceed |
| **`approved`** | Explicit human approval consumed | Proceed once (**not produced today**) |
| **`denied`** | Permission layer denied | **Do not** mutate |
| **`requested`** / pending | Waiting for approve/deny | **Do not** mutate (**not produced today**) |
| **`hard_denied`** | Category for hard safety in summaries | Not a soft permission outcome; hard-deny path **suppresses** soft attach |

Notes:

- HTTP/MCP protocol success is not “approved.” A tool may still fail after
  `auto_approved`.
- Summary counters historically use **`requested`** for pending; design may
  prefer `pending` later with a one-time mapping if needed.
- Unknown / unparsable `status` **fails closed** (`allows_execution` = false).

### Risk vs outcome

- **High risk ≠ deny** under `dev_auto_approve` / `audit_only`.
- **Low risk ≠ skip hard safety.**

---

## 5. Risk classification

### Goals

- Stable, coarse labels for summaries and future gates.
- Prefer **existing tool metadata / annotations / capability**, not ad-hoc
  per-tool switchboards.
- Keep risk labels independent of permission outcomes.

### Current implementation

`risk.rs` is a **thin facade**:

- `tool_requires_permission` → `runtime_tool_requires_permission`
- `classify_tool_risk` → `runtime_tool_permission_risk`

It is **not** a completed argument-aware risk engine.

### Current labels (from tool metadata / policy)

| Label | Typical source |
|---|---|
| `write` | Project write tools |
| `patch` | Patch path tools |
| `artifact_write` | Artifact path writes |
| `destructive` | `destructive` metadata |
| `shell` | Shell-like tools |
| `job` | Job lifecycle tools |
| `validation` | Validation capture tools |

### Conceptual classes (design map — not all gated today)

| Conceptual class | Approx. current label / signal | Notes |
|---|---|---|
| read-only | `readOnlyHint` / no permission required | No permission object |
| workspace mutation | `write`, `patch` | Inside allowed project root |
| process execution | `shell`, `job` | Commands / background jobs |
| destructive filesystem | `destructive` | Higher impact deletes/overwrites |
| git history mutation | (future finer label if needed) | Prefer hard agent rules |
| network access | open-world / shell profiles | Often inherits shell risk |
| release / deployment | policy + agent rules | Hard safety: no deploy/publish unless explicit |
| credential-sensitive | sensitive path / secret rules | Hard deny / redaction, not soft approve |

### Rules

1. Do **not** hard-code large temporary per-tool matrices in the evaluator.
2. Extend `ToolDefinition` / metadata when a new class is needed.
3. Annotations remain discovery hints; runtime risk labels remain authoritative
   for the permission facade.
4. Default `dev_auto_approve` still obeys **hard safety** for high-risk ops.

---

## 6. Configurable policy vs hard safety

### Configurable permission policy (soft)

- Whether to auto-approve (`dev_auto_approve`).
- Whether to label audit-style decisions (`audit_only`).
- Whether to require a human gate (`require_approval` — currently hard-denies
  as not implemented rather than pending).

### Hard safety rules (not overridable by mode)

| Hard rule | Enforcement home (examples) |
|---|---|
| No push / release / deploy / npm publish without explicit request | Agent contract + operational practice |
| No secret / token / `.env` exfiltration | Path denial, redaction, logging rules |
| Stay inside resolved project / allowed roots | Path policy, project resolution |
| `read_only` Workflow Session denies write/shell/job-like tools | Session guard **before** mutation (and typically before soft permission) |
| Unknown explicit `session_id` → `unknown_session_id` | Session resolution |
| OAuth / scope insufficient | Scope check |
| Agent capability missing | `agent_authorization` |
| Sensitive / traversal path rejection | File tools + `policy_rejected` / hard-deny detection |

**Permission mode must never weaken these.** Auto-approval only means “no
extra human gate after the soft policy allows execution.”

Hard-deny detection used when attaching permission after execution includes
structured kinds such as `policy_rejected`, `session_guard_denied`,
`unknown_session_id`, and error-string markers for sensitive/traversal paths
(`is_hard_denied_output`).

---

## 7. Request processing order

### Implemented pipeline (summary)

```text
Tool Request
    ↓
Parse / validation (kernel or surface) — invalid args: no fake approval
    ↓
AuthN + scope / agent capability (as today)
    ↓
Resolve Workflow Session context (if any)
    · unknown id → fail; no silent invent
    · session guard before mutation
    ↓
PermissionEvaluator (once at dispatch)
    ↓
Decision:
    allow (auto_approved | audit_only_allowed)
    deny  (require_approval_not_implemented | invalid mode | …)
    (pending — designed only)
    ↓
Tool execution — only on allow
    ↓
Hard tool-internal checks still apply
    ↓
Attach same decision unless hard-denied; record optional ledger / audit / trace
```

### Ordering requirements

| Rule | Rationale |
|---|---|
| Permission check **before** mutation | Prevent “execute then pretend to ask” |
| Parse/validation failure → **no misleading approval record** | Invalid args are not `auto_approved` successes |
| `deny` → **no** ToolRuntime mutation / agent enqueue | Side-effect free deny |
| Hard safety remains effective | Absolute blocks stay independent |
| Auto-approve path stays **cheap** | Default mode is hot path for every write tool |
| Evaluator **never executes** tools | Pure decision function |

---

## 8. Workflow Session integration

| Topic | Decision |
|---|---|
| `workflow_session_id` | Optional context when the call path already resolved a `wc_sess_*` (not a field on today’s `PermissionDecision` wire struct) |
| Auto-create session | **Forbidden** for permission reasons |
| Implicit current-session fallback | **Forbidden** inside the evaluator |
| Closed / missing session | Permission still evaluates for the tool call; ledger attach skipped without a valid start |
| `read_only` session | Session **guard** denies write-like tools; not soft `auto_approved` |
| Permission on Session events | Attach to existing tool-call start/finish via `permission` field; no separate ledger event kind in early phases |

For future pending approvals, store **pending records outside the ledger** (or
in a dedicated store), and only mirror final outcome onto the tool-call event.
Prefer **not** expanding ledger schema until required.

---

## 9. Action Audit integration

| Topic | Decision |
|---|---|
| Concepts | **Permission Decision ≠ Action Audit Record** |
| Permission | Why allowed / denied / pending (policy rationale) |
| Action Audit | What HTTP/API action occurred |
| Correlation | Optional later via ids only — **not implemented** |
| Transactions | **No** strong transaction across SQLite audit and JSON ledger |
| Cascading deletes | **None** |

| Mode | If Action Audit write fails | If permission decision attach fails |
|---|---|---|
| `dev_auto_approve` | **Do not** block tool execution | Prefer not to block; log |
| `audit_only` | Do not block execution | Do not block |
| `require_approval` (future real gate) | Do not treat audit failure as approve | **Must** not execute if approval state cannot be durably recorded when durability is required |

Observability failures must not invent denials in default modes, and must not
invent approvals when a real approval mode exists.

---

## 10. Lifecycle Trace integration

- Module: `tool_request_trace.rs`, env `WEBCODEX_TOOL_REQUEST_TRACE` (default
  **false**).
- Operational lifecycle only; **must not** hold pending approval state.
- Future logs may include joinable ids; enabling permission modes does not
  require enabling trace.

---

## 11. Pending / approval state machine (design only)

Minimal states for a **future** real `require_approval` implementation:

```text
pending ──approve──► approved ──consume──► consumed
   │                    │
   ├──deny──────────► denied
   ├──timeout───────► expired
   └──cancel────────► cancelled
```

| Question | Recommendation |
|---|---|
| Is approval one-shot? | **Yes.** |
| Bound to tool + param digest? | **Yes.** Redacted normalized summary hash only. |
| Params change after approve? | Old approval invalid. |
| Reuse / double consume? | **Forbidden.** |
| Timeout | Configurable later; expired ≠ approved. |
| Restart while pending | Pending does **not** auto-approve. |
| Who can approve/deny? | Local operator / authenticated admin only. No model self-approve. |
| Client timeout while pending | Structured not-executed error; no mutation. |

**Today:** selecting `require_approval` does **not** enter this machine; it
denies with `require_approval_not_implemented` so the product never pretends
approval works.

---

## 12. Default configuration

```text
WEBCODEX_PERMISSION_MODE=dev_auto_approve
```

| Rule | Detail |
|---|---|
| Default when unset | **`dev_auto_approve`** |
| Unset must not enter pending | Absolute |
| Illegal value | **Fail closed on permission-bearing tools** (deny; do not fall back to allow). Process startup is not required to abort. |
| Independent trace switch | `WEBCODEX_TOOL_REQUEST_TRACE` (default false) |
| Independent Action Audit | Existing enablement path only |
| Client changes | **None** for default mode |

`runtime_status` / coding-task permission profile payloads advertise effective
`policy`, `auto_approve`, `human_approval_required`, and
`release_recommended_policy` (`require_approval` as a recommendation string only).

---

## 13. Observability

### Safe fields

- decision / request id (`wc_perm_*`)
- mode / policy
- outcome / status
- reason
- risk level
- tool name
- workflow session id (if any, via ledger context)
- trace id (if any)
- project id (if any)
- param **digest** only when approvals exist later

### Forbidden

- Full tool parameters
- File contents / patches / diffs
- Secrets, tokens, credentials
- User prompts / chat content
- Large tool results / stdout / stderr

Matches ledger and `SECURITY.md` redaction expectations.

---

## 14. Compatibility guarantees (default mode)

| Surface | Guarantee under default mode |
|---|---|
| Tool behavior | Unchanged vs frictionless self-hosted development |
| `tools/list` | Unchanged by permission modes |
| `tools/call` result | Optional `output.permission` on permission-bearing tools when attach is not suppressed |
| Clients | No required changes for `dev_auto_approve` |
| Workflow Session ID / ledger format | Stable attach field; no new event kind |
| Action Audit routes | Unchanged |
| Hard safety | Never bypassed by any mode |

---

## 15. Non-goals (near-term)

- No approval UI
- No notification system
- No multi-person approval
- No RBAC / multi-tenant policy engine
- No distributed policy service
- No SQLite permission migration until a real store is required
- No formal Action Audit ↔ Permission coupling until Phase 4
- No pretending `require_approval` is a working human gate

---

## 16. Phased plan

### Phase 0 — Design only — **done**

Design document and architecture pointers.

### Phase 1 — Types, mode config, module split — **done**

- `PermissionMode` / `PermissionOutcome` / `PermissionDecision`
- `WEBCODEX_PERMISSION_MODE` resolution
- Module layout under `permissions/`
- Default remains `dev_auto_approve`

### Phase 2 — Unified pre-exec evaluator — **done**

- Single `PermissionEvaluator` call at dispatch before mutation
- Kernel reuses decision; no second evaluation
- Mode matrix: allow for auto/audit; deny for require_approval (not implemented)
  and invalid config
- Hard-deny suppress on attach retained

### Phase 3 — Risk classification improvement (default execution unchanged)

- Deepen metadata-driven risk labels / facade clarity
- Still no default-mode friction
- Must not claim a full risk engine until argument-bound rules exist

### Phase 4 — Optional Permission ↔ Action Audit / trace correlation

- Optional ids only; no ownership merge
- No strong transactions
- Default mode still frictionless

### Phase 5 — Real pending / approval only if genuine demand

- Durable pending; approve/deny; one-shot consume; param digest
- Config default remains `dev_auto_approve`
- Until this ships, `require_approval` stays **honest deny**
  (`require_approval_not_implemented`), never fake auto-approve

### Ordering assessment

Document → types → pre-gate without default UX change → richer risk/audit
correlation → real gate only with demand. **Do not** implement pending without
the single pre-exec evaluation invariant.

---

## 17. Summary for implementers

1. Keep **`dev_auto_approve`** as the default until an operator opts in.
2. Treat permission as a **pure decision + optional record**, not session/audit
   ownership.
3. Reuse **tool metadata** for risk via the facade; keep **hard safety** supreme.
4. Evaluate **once** at dispatch; kernel **reuses** only.
5. Prefer **embedding** decisions on existing tool-call ledger events.
6. Implement **pending** only with one-shot, digest-bound, durable, default-off
   semantics — and only when there is a real product need.
7. Measure success by: **zero friction in default mode**, honest behavior when
   modes are set, and no bypass of project/session/secret rules.
