# Model-facing tool contract guidelines

Status: standing design guidance for current WebCodex development.

This document defines the default style for model-facing WebCodex tools. It is
about **turn economy without semantic shortcuts**: a model/tool round trip should
pay for a real decision, effect, or observation that WebCodex cannot determine
mechanically. It should not be spent correcting a harmless parameter that the
runtime can normalize without changing meaning.

The short form is:

> **Strict on semantics and authority; tolerant on ergonomics; truthful about
> uncertainty; sparse about repetition.**

This guidance does not weaken Project, credential, permission, path, retry,
Session, Job, or durability boundaries. It tells tool contracts where strictness
belongs so those boundaries do not leak into unrelated mechanical friction.

## 1. Spend turns on meaning, not syntax

### Bounded bulk exact edits

For repetitive mechanical changes in an explicit file, `apply_text_edits` accepts
`replace_exact` with `expected_match_count=N` (1..=1024) and a current
`expected_read_revision`. The Runner replaces every fully contained exact match
only when the observed count equals N. `occurrence` selects one match and cannot
be combined with this field; `line_scope` may narrow the counted matches. The
Runner plans every source range against one original snapshot, checks overlaps
across the whole file change, and applies the transaction only after every file
has passed preflight. A count mismatch writes nothing.
The additive Runner capability is `apply_text_edit_expected_match_count`.
Servers reject bulk requests before dispatch to an older Runner that lacks it;
requests without the field keep their existing admission and unique-match behavior.

For nontrivial bulk changes, read the file and revision, optionally call
`apply_text_edits(dry_run=true)`, inspect the bounded `match_count` and
`match_ranges`, then send an independent actual request with the still-valid
guard. The actual request resolves all matches and fences again. A simple,
obvious bulk edit may be applied directly. Dry-run creates no future mutation
authority. The compact success `change_summary` reports counts; use
`show_changes`, `git_diff_hunks`, or `git_review_summary` for semantic review.

Use this exact cardinality contract for known repeated fixtures or struct
literals instead of an ad-hoc Python or sed global rewrite. It does not infer
the count, choose an occurrence, use regex, or expand across a glob.

A tool should reject an input when the model must make a new semantic decision.
If WebCodex already knows the only safe interpretation, prefer deterministic
normalization and continue the requested work.

The practical test is:

> If accepting, clamping, canonicalizing, or ignoring a **recognized** parameter
> cannot change target identity, authority, effect class, retry safety, evidence
> truth, result identity, or confidentiality, rejection needs a concrete reason.

Typical **hard** inputs remain exact and fail closed:

- tool identity and admission;
- Project, Runner, Workflow Session, Agent, Goal, Job, artifact, or other
  authority-bearing identities;
- OAuth scopes, permission/approval gates, destructive confirmations, and
  sensitive-path policy;
- commit/SHA/revision/lease/fence/idempotency identities;
- continuation and observation tokens whose exact scope is part of correctness;
- mode combinations that select a different target or effect;
- malformed structures or values whose intended meaning is ambiguous;
- unknown fields when silently ignoring them could hide a typo in a semantic or
  consequential parameter.

Typical **ergonomic** inputs should prefer bounded normalization:

- timeouts, wait budgets, page/result byte budgets, tail lengths, display limits,
  and other presentation/resource ceilings;
- explicit defaults that are semantically identical to omission, such as a
  harmless `false` on a non-selected optional mode;
- over-large bounded knobs where clamping cannot broaden authority or change the
  selected operation;
- redundant default spellings that can be canonicalized before business parsing.

When normalization matters to later reasoning, return the effective value. Do not
force the model to call again merely to discover a server-known clamp.

Unknown fields are different from recognized harmless fields. Closed schemas are
still the default because silently accepting an unknown name can hide a typo.
Low-friction design means making the **known contract** forgiving where semantics
are unchanged, not turning every input object into an open bag.

### Batch predetermined observations; keep adaptive work sequential

One model decision may request several observations when they are already known
to be needed, tightly related, and an existing primitive naturally supports the
batch. Examples include several related `read_files` ranges, independent search
queries, or a short bounded `run_shell` chain of predetermined observations.
Result-dependent follow-ups stay sequential so the next call can incorporate the
new evidence. In direct strategy the model chooses each follow-up across calls;
with explicitly selected Code Mode guidance, an admitted read-only cell can inspect
results and perform dependent follow-ups sequentially inside the same cell. Only
independent observations run concurrently. Keep intermediate child results inside
the cell and project compact decision evidence before `text(...)`; batching raw
results into one output does not save model context. Do not preload unrelated data or combine permission, mutation,
validation, commit, publish, deploy, or restart boundaries merely to reduce call
count.

Inspection should narrow before it expands. Known symbols/tests/regions favor
bounded targeted reads and related-range batching. Broad discovery should prefer
files/count/small low-context search projections followed by targeted reads.
`run_process` remains the natural path for one native executable with literal
argv; `run_shell` is first-class for shell grammar or a short tightly related
chain, while `run_script(language=python)` carries a program-like Python body as
typed data. A bounded Python heredoc remains possible for special shell
composition. None of these rules means “shell first” or weakens specialized semantics.

## 2. Mechanical repair should be server-owned

Current execution-input compatibility is deliberately narrow:

| Model input | Canonical interpretation | Condition |
|---|---|---|
| `run_process.argv`, `run_detached_process.argv` | `args` | If `args` is also present, values must be identical. |
| `run_process` with exact `sh -c` or `bash -c` argv | `run_shell` with explicit `shell` | Runtime proves the request is lossless and the canonical shell path passes authority, policy, and capability gates. |
| `run_process` with exact `bash -lc` argv | `run_shell(shell=bash, login=true)` | Same proof and Bash-login capability gate. |

`run_script(language=python)` is canonical; `python3` is not a language alias.
Unknown spellings such as `timeout`, `workdir`, `command_args`, and
`command` for `script` still fail closed. Successful normalization returns a
short `input_normalization` code and hint without replaying the raw payload.

Do not spend a model turn on a repair WebCodex can prove locally.

Prefer:

```text
requested timeout above ceiling
-> clamp
-> execute once
-> report effective timeout
```

rather than:

```text
requested timeout above ceiling
-> schema rejection
-> model edits one integer
-> same execution on the next turn
```

The same principle applies to safe default normalization, bounded list limits,
empty/no-op edits, and recovery metadata that already proves one exact direct
retry. If the runtime cannot prove the repair, it must ask for a new observation
or reject rather than guess.

This rule never authorizes WebCodex to infer missing authority, invent a Workflow
Session, auto-ACK model context, choose a destructive target, or reinterpret an
uncertain effect. Those are semantic decisions, not mechanical repair.

## 3. One fact has one canonical representation

Do not make the model reconcile duplicate machine truth.

- Do not emit a canonical field plus a legacy alias for the same fact without a
  named concrete consumer.
- Do not keep both a parser-local approximation and an authoritative source fact
  in the model projection when they can disagree.
- Do not retain args-only and `{tool, arguments}` variants of the same suggested
  action.
- Remove obsolete fields, schemas, tests, and docs together when one canonical
  representation replaces them.

A model-facing tool contract is not a public SDK compatibility promise by default.
During active development, a cleaner canonical tool shape is preferred over
preserving an unused historical shape. Durable persisted truth, mixed-version
Server/Runner protocol, published artifacts, and named external consumers are
separate compatibility domains and must be handled explicitly.

### Project selectors: canonical identity, short model reference

Runtime Project identity remains canonical as `agent:<client_id>:<project_id>`. Keep that form for authorization, persistence, audit, Runner routing, diagnostics and explicit API/CLI addressing. A Server-issued `project_ref` is a model-facing selector only: the Server owns a durable mapping scoped to the authenticated caller and pins it to one canonical Project incarnation, including stable root identity. The model may reuse the short ref across windows for the same principal, but no Workflow Session, ClientWindow, MCP session, transport connection, recent activity or Host rewrite participates.

Resolving a `project_ref` must always look up the pinned canonical identity and then run the ordinary current Project resolution/authorization path again. The ref is not a credential, bearer token or capability. If the canonical Project disappears, becomes invisible, loses stable identity, or the same canonical address is later registered for a different root, the old ref fails closed. Never recycle or silently retarget an issued ref. Discovery/bootstrap may expose both `project_ref` and canonical identity; ordinary hot-path results should not repeat them when no model decision depends on that duplication.

The same typed durable-reference layer may issue a principal-scoped `session_ref` such as `~s1` for one exact Workflow Session incarnation. Canonical `wc_sess_*` remains authoritative for Session persistence, audit, diagnostics and internal joins. Business `session_id` and wrapper `recording_session_id` remain separate semantic roles, but either may explicitly carry an already-issued Session ref. Runtime canonicalizes the selector before the role-specific authorization/dispatch path: business targeting still reruns Project visibility, Session authority, lifecycle and guards, while recorder provenance still reruns its independent recorder authorization and never supplies business authority. A Session ref never creates ambient or sticky recorder state. Bootstrap, discovery and handoff may expose both identities, while ordinary hot-path results should avoid redundant duplication.

### Agent continuation selectors

`present_agent_continuation` may take a server-issued `agent_continuation_ref` instead of the explicit `agent_id`, `endpoint_id`, and `expected_controller_generation` tuple. The ref is a durable mapping scoped to the communication principal and pinned to that exact Endpoint generation. It is not a bearer credential, Workflow Session, ClientWindow, or Host binding. Dereference expands the ref to the stored tuple and then runs the ordinary owner, lifecycle, and generation checks. A later rotation, expiry, or detach leaves the old ref stale; it must not be rewritten onto the successor. Canonical ids stay in the Endpoint record, audit, and continuation projection. App-only bind, recover, and wake tools keep the explicit tuple. Keep this mapping separate from Project and Session refs; do not generalize it to Goals, Tasks, or Conversations without a separate contract.

### AgentTask attempt selectors

`start_agent_task_endpoint_continuation` may take a server-issued `attempt_ref` instead of the explicit `task_id`, `attempt_id`, `assignee_agent_id`, `attempt_fence`, and `attempt_controller_generation` tuple. `start_agent_task_attempt` returns that ref for the Attempt it just created, including exact keyed replay. The ref is a durable mapping scoped to the communication principal and pinned to that exact fence and controller generation. It is not a bearer credential and does not weaken the fence. Dereference expands the ref to the stored tuple and then runs the ordinary owner, lease, fence, and generation checks. A later takeover, expiry, replacement, or controller generation change leaves the old ref stale; it must not be rewritten onto the successor. Canonical ids stay on the Attempt record and in the result audit. The request audit records the ref or the canonical ids, and records only whether a fence was supplied. Heartbeat, completion, coding-run, and reconcile keep the explicit tuple. This table is separate from Project, Session, and Agent continuation refs.

### Model-projection deletion test

A model-facing result field should normally survive only when it can change at
least one of these decisions:

- how the business result is interpreted;
- which target, authority, fence, retry, or continuation identity is safe;
- whether the observation is complete, partial, stale, or uncertain;
- which exact next action the model should take.

If deleting a field leaves all of those decisions unchanged, omit it from the
default model projection. Keep it in an internal typed contract, test invariant,
telemetry record, operator/Console view, or exceptional diagnostic when those
consumers still need it.

Internal proof contracts answer why the Runtime knows an operation, observation,
or follow-up is safe; the model projection answers what the model must know or do
next. Do not serialize scope digests, MAC/proof state, cursor/carrier classes, or
recovery bookkeeping merely to explain the Runtime's own safety proof when they do
not change a model decision.

One exception to deletion-by-derivation is an intentionally stable semantic
abstraction over a broader or evolving internal taxonomy. A derived field may stay
model-facing when its purpose is to let callers reason against a deliberately
smaller contract than the underlying status/state variants. Treat such a field as
an explicit semantic firewall with its own documented invariant, not as a
convenience duplicate.

In particular, a parser-ready `suggested_call` or continuation call should not
normally be accompanied by a second classification vocabulary such as
`kind`/`carrier`, `safe_cursor`, `recommended_order`, or a duplicate raw token
when those fields merely restate the same next action. Internal continuation and
recovery taxonomies may remain useful implementation SSOTs without becoming
per-result concepts the model must learn. Likewise, counts that are exactly an
array length and success booleans fully implied by one authoritative lifecycle
state should be omitted unless they carry independent meaning.

Prefer **progressive disclosure**: ordinary success returns sparse business truth
and one actionable follow-up; reset, truncation, reconciliation, malformed-source,
or other exceptional paths may expose the additional bounded forensic evidence
needed to explain or safely recover the anomaly.

## 4. Success is sparse; failure is decision-complete

Successful calls should foreground the business result and omit redundant derived
bookkeeping when the model does not need it.

A useful success projection usually contains:

- the requested observation/effect result;
- exact identity/fence information needed for a consequential next step;
- explicit partiality/completeness when relevant;
- one parser-ready follow-up only when more work is genuinely needed.

Do not repeat the same state through several fields such as `success=true`,
`status=ok`, `complete=true`, and duplicate returned counts unless each field has
an independent contract.

Failures should be more structured because the model must decide what to do next.
Prefer stable bounded fields such as:

```text
reason_code
failure_stage
detail_code
state_changed
outcome_unknown
```

plus the minimum safe recovery evidence. Backend implementation details are not a
substitute for domain semantics. For example, a proven missing search path is a
path-resolution fact, not an `rg` process failure merely because `rg` would also
exit non-zero.

## 5. Business result is primary; protocol maintenance is support

A successful compile, edit, review, or observation should still look successful
when a secondary protocol concern also needs attention.

Context recovery, recorder guidance, telemetry, and other maintenance metadata may
be important, but presentation should not make them look like a new business
failure. Keep them structured and actionable while visually and semantically
secondary to the tool result.

This is a presentation/projection rule, not permission to weaken the underlying
protocol. In particular:

- missing task context is recovered explicitly with `session_handoff_summary`;
- collaboration ACKs require request-scoped retained-message proof;
- a ClientWindow must not select a Workflow Session;
- support metadata must not become execution authority.

## 6. Follow-up actions use one parser-ready shape

When a domain already knows the next tool and exact bounded arguments, use the
shared conceptual shape:

```json
{
  "tool": "tool_name",
  "arguments": {}
}
```

Natural-language advice may explain *why*, but it should not be the only
machine-actionable representation.

Keep these concepts distinct:

- **continuation** — continue the same logical observation/execution identity or
  page with the domain's existing cursor/fence;
- **refine** — issue a new observation with changed bounded parameters, such as a
  larger result or hunk limit;
- **recovery** — repair a failed/lost/invalid state using domain-proven evidence;
- **collaboration ACK** — request-scoped retained-message proof; not a cursor or authority.

Do not advertise a continuation that cannot recover the omitted information. Do
not turn `outcome_unknown` into retry permission. Do not create a universal cursor
or `NextAction` state machine merely because several domains can express a
`{tool, arguments}` advisory call.

## 7. Unknown must stay unknown

Tool projections should distinguish:

```text
false / absent / empty
```

from:

```text
unknown / not observed / incomplete
```

when the distinction affects model decisions.

Examples:

- an unobserved path is not a proven missing path;
- a producer-truncated hunk is not complete merely because a downstream parser
  did not truncate it again;
- a transport timeout does not prove an effect never started;
- a stale or lost observation token does not prove the underlying Job failed.

Prefer explicit conservative completeness/provenance to a convenient Boolean that
can overclaim certainty.

## 8. Recovery should skip redundant re-observation when proof already exists

When a failed operation returns authoritative evidence for one exact safe retry,
make that retry directly actionable. Examples include an exact candidate
occurrence/range after a guarded edit conflict or a bounded parser-ready call that
recovers a known omitted page.

Require a reread/reobserve when current truth may have changed or the runtime
cannot prove a unique repair. A useful recovery contract makes this distinction
explicit rather than forcing the model to infer it from prose.

The default recovery hierarchy is:

```text
exact safe retry proven
-> direct structured retry

current truth required
-> exact re-observation

prior effect uncertain
-> inspect/reconcile; never blind retry
```

## 9. Static semantics belong in ToolDefinition; dynamic truth stays in its domain

`ToolDefinition` is the canonical owner for static tool facts such as model
visibility/admission, selection metadata, effect/risk, Activity presentation and
interaction semantics, audit policy, Session evidence policy, and other facts that
must not be re-created through scattered tool-name lists.

Dynamic request/result truth stays with the authoritative domain. A declaration
must not replace request parsing, path resolution, Job lifecycle, validation
parsing, Git scope/fence checks, Session state, or other runtime observations.

Presentation classes are not authority. For example, a Transport activity can
still be a meaningful model/environment interaction, and a ModelHidden tool can
still represent real work.

## 10. Compatibility follows concrete consumers, not historical implementation

For model-facing tool contracts, compatibility is opt-in rather than automatic.
Before retaining an alias, dual shape, legacy argument, or compatibility parser,
name the consumer or durable/public boundary that requires it.

Valid reasons include, when actually present:

- durable persisted state that must still restore;
- mixed-version Server/Runner rolling operation;
- a named external client/workflow or published artifact contract;
- a required security/privacy migration boundary.

"The old test expects it" and "a previous commit emitted it" are not consumers.
Historical persisted evidence should remain truthful about the past, but current
ToolDefinitions and model projections should not carry obsolete tool API baggage
solely to preserve old model behavior.

## 11. Measure friction before pruning tools

Low usage alone does not prove a tool lacks value. A tool may be avoided because
its schema rejects harmless inputs, discovery is expensive, recovery requires
extra turns, or a nearby generic tool is easier to invoke.

Before Direct/Gateway/Retire decisions, use dogfood telemetry and review traces to
look for:

- schema rejection followed by the same call with one mechanical correction;
- failure -> reread -> identical retry loops where direct recovery was provable;
- discovery calls whose result is much larger than the selected contract;
- success payloads dominated by duplicate metadata;
- recovery/support metadata that causes an unnecessary extra business turn;
- repeated tool sequences whose intermediate model decisions add no value.

The north-star measurement is fewer **non-business model/tool round trips** at the
same or better correctness, authority, and evidence quality.

## 12. Work sequence

Current tool work should proceed in this order:

1. **Input normalization and bounds** — remove harmless schema/parameter friction;
   keep semantic and authority fences exact.
2. **Recovery and follow-up shape** — converge parser-ready follow-ups and make
   safe direct retry vs required re-observation explicit.
3. **Result projection and common ergonomics** — sparse success, structured
   failure, exact discovery, common validation selectors, and secondary protocol
   metadata presentation.
4. **Surface pruning** — only after the same design standard applies across tools,
   use telemetry to decide Direct vs Gateway vs Retire.
5. **Composition** — only after primitive tool friction and surface shape are
   understood; composition must reduce outer turns without becoming a new
   authority, retry engine, or workflow runtime.

Do not skip directly to pruning or composition just because a trace contains many
tool calls. First determine whether the extra calls are real model decisions or
avoidable contract friction.

### Runtime status projections

Canonical `runtime_status` and HTTP/API omission retain full diagnostic output.
MCP supplies `compact=true` only when the argument is omitted, for both direct
and `call_runtime_tool` calls. Use `compact=false` (without `summary_only=true`)
for full diagnostics. `summary_only=true` is still an alias for sparse status.
Discovery schema compaction does not control result projection.

Sparse fleet status reports Server identity, MCP Host profile, Runner/Project
counts, active/running/queued/recovering/lost-after-reconcile Job counts,
protocol/build/source alignment, and connection states. Exact `client_id`
focus limits these observations to that caller-visible Runner, including its
protocol generation and shared Job concurrency. It does not return fleet rows,
capabilities, provider inventories, authority, auth configuration or timestamps.
Full mode retains those diagnostic facts. Both modes use the same canonical
Job counting and compatibility rules; sparse status branches before full
inventory/configuration JSON construction.

Measured costs and direct-surface decisions: [model-call economy audit](model-call-economy-audit.md).
