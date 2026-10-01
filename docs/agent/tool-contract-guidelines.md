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

For repetitive mechanical changes in an explicit file, `edit_project_files` accepts
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
`edit_project_files(dry_run=true)`, inspect the bounded `match_count` and
`match_ranges`, then send an independent actual request with the still-valid
guard. The actual request resolves all matches and fences again. A simple,
obvious bulk edit may be applied directly. Dry-run creates no future mutation
authority. The compact success `change_summary` reports counts; use
`read_workspace_changes`, `read_git_diff_hunks`, or `read_git_review_summary` for semantic review.

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

Current ergonomic execution-input normalization is deliberately narrow:

| Model input | Canonical interpretation | Condition |
|---|---|---|
| `run_process.argv`, `run_detached_process.argv` | `args` | If `args` is also present, values must be identical. |
| `run_process` with exact `sh -c` or `bash -c` argv | `run_shell` with explicit `shell` | Runtime proves the request is lossless and the canonical shell path passes authority, policy, and capability gates. |
| `run_process` with exact `bash -lc` argv | `run_shell(shell=bash, login=true)` | Same proof and Bash-login capability gate. |

Input aliases save model turns; they are not an API compatibility promise.
An alias must be explicit, closed, lossless and unambiguous: alias-only input
canonicalizes; canonical plus alias with identical values canonicalizes; different
values fail closed. Canonical ToolCall serialization retains only the canonical
field. Advertise the known alias in Host input schema when necessary for it to
reach Server normalization, without opening `additionalProperties`.

Never fuzzy-correct unknown fields or tool names. Do not guess authority, target,
effect, retry, fence or idempotency fields. Keep the explicit process spelling
repair local; do not add a general alias registry without several concrete
normalizations needing one. Stable `input_normalization` codes, such as
`argv_to_args`, make normalization usage observable; counts do not prove how many
model turns or corrective calls would otherwise have occurred.

New aliases also need concrete return-on-cost evidence: a repeated mechanical
correction pattern in dogfood/ActionAudit, a known Host/provider that consistently
emits that spelling, or another explicit consumer requirement. When an alias must
appear in the Direct Host input schema, compare its descriptor byte increase with
observed corrective-call patterns and normalization usage. Do not populate every
plausible synonym merely because a model might misspell a field. More aliases are
not inherently more ergonomic: every extra spelling enlarges the model's selection
and context surface. An immediate same-tool success alone does not establish the
field that caused the failure; do not parse serde error prose to infer it.

`ToolInputNormalizationCode` owns the four existing wire spellings and model hints.
The parser, execution normalization, and result schema share that vocabulary.
ModelErgonomics v13 adds only the optional typed `input_normalization_code` to the
existing ActionAudit `summary.model_ergonomics` JSON. Capture canonical success
before late model projection; omit the field for failures and unknown codes.
Never persist the hint, command, arguments, path, or parser error as part of this
metric. One successful invocation contributes at most one reported code, retaining
existing result precedence when multiple normalization steps apply. Neither this
measurement nor its absence changes runtime admission or retry semantics.

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

The same typed durable-reference layer may issue a principal-scoped `session_ref` such as `~s1` for one exact Workflow Session incarnation. Canonical `wc_sess_*` remains authoritative for Session persistence, audit, diagnostics and internal joins. Business `session_id` and MCP envelope `_wc.record` remain separate semantic roles, but either may explicitly carry an already-issued Session ref. Runtime canonicalizes the selector before the role-specific authorization/dispatch path: business targeting still reruns Project visibility, Session authority, lifecycle and guards, while recorder provenance still reruns its independent recorder authorization and never supplies business authority. A Session ref never creates ambient or sticky recorder state. Bootstrap, discovery and handoff may expose both identities, while ordinary hot-path results should avoid redundant duplication.

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

Every server-generated parser-ready tool call has one Host execution posture:
`follow_up_kind=mechanically_followable` means the Server has already resolved the
semantic choice for that exact follow-up, while `fallback_recovery` means the
call is available only for explicit recovery, detail expansion, reconciliation,
or a blocked dependency. Hosts must not infer execution posture from field names
such as `next_call`, `suggested_call`, `recovery.*.next_call`, or
`continuation`.

This posture is an intentional exception to deletion-by-derivation because it
changes a Host decision: whether an exact generated call may continue without a
new model decision. Other classification vocabularies such as `kind`/`carrier`,
`safe_cursor`, `recommended_order`, or duplicate raw tokens should still stay
internal when they merely restate the same action. In all cases, generated
`arguments` must validate unchanged against the target tool's current registered
input schema; Rust deserialization alone is not a contract test.
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

- missing task context is recovered explicitly with `read_session_handoff_summary`;
- collaboration ACKs require request-scoped retained-message proof;
- a ClientWindow must not select a Workflow Session;
- support metadata must not become execution authority.

## 6. Follow-up actions use one parser-ready shape

When a domain already knows the next tool and exact bounded arguments, use the
shared conceptual shape:

```json
{
  "follow_up_kind": "mechanically_followable",
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

Contract consistency tests iterate `tool_definitions()` rather than maintaining a second
complete name/risk/capability table. Each ToolDefinition owns exactly one category;
`group_tool_names_by_category` derives sorted, non-overlapping category projections
from the caller's already-admitted tool selection. `list_tools` and `read_tool_manifest`
use the same taxonomy. Intent ranking and recommended flows remain deliberately
cross-category workflow views, not another category registry. Categories and Direct
rank never grant authority or determine execution/Activity semantics. Structured-validation
output family admission follows `ToolExecutionForm::StructuredValidation`; tool-specific
output fields remain explicit.
Input contract tests share `test_support::sample_tool_args_for_spec` in tool-contracts.
It chooses declared const/default/enum values, then bounded type/required-child samples;
unsupported constraints fail rather than silently inventing a fixture. Keep semantic
identity fixtures and cross-field overrides documented there, never in a second runtime
helper.

Runner wire capability facts live in core's `runner_protocol.rs` local
`runner_capabilities!` declaration. Each row owns the typed identity, public constant,
wire name, field/serde/default behavior and frozen V2 baseline membership. It generates
`RunnerCapabilityId`, inventories, `RunnerCapabilities` and typed get/set projections.
Declaration order preserves wire serialization order; golden tests protect legacy
required false fields, omission and defaults. Server `RunnerFeature` re-exports this
identity and validates baseline bits without inferring support. Computer admission
classification and capability prerequisites remain Server policy.

For a new `foo_v1` capability, add one protocol row (normally false-by-default,
omitted when false, and outside the frozen V2 baseline). Advertise it explicitly in
`runner_register_capabilities` only when that binary implements it; retain real
runtime/platform probes for dynamic support. Add capability-specific prerequisites or
Server classification only if needed. Catalog membership and baseline membership are
never permission to advertise implementation support. Only the legacy `shell`/`git`
implementation switches are copied from configuration; a new wire field cannot inherit
advertisement merely from config. All-enabled fixtures are tests only and iterate the
typed catalog.

### Canonical tool naming and model-surface exposure

Tool identity must describe the operation, not today's Host presentation policy.
Direct/Gateway placement is allowed to change as Host behavior, usage evidence and
schema budgets change without renaming the canonical tool. Do not encode
`direct`, `gateway`, `hidden`, `adaptive`, or a Host brand into an ordinary
canonical tool name.

For new model-visible tools, prefer a stable `verb_object` name and use these
verbs consistently:

| Form | Meaning |
|---|---|
| `list_*` | Bounded/filterable inventory of zero or more objects. |
| `read_*` | Read bounded content or evidence, commonly with paging, snapshots or revisions. |
| `get_*` | Exact durable object/state lookup when content-reading semantics are not the point. |
| `observe_*` | Observe changing state for an already-known identity; cursor/token semantics may apply. |
| `wait_for_*` | A real dependency wait/barrier with an owned deadline or continuation contract. |
| `present_*` | Host-visible presentation/App integration whose descriptor carries presentation semantics. |
| `create_*`, `update_*`, `assign_*`, `complete_*`, `stop_*` | Explicit lifecycle/effect verbs; keep target identity in the object name. |

Use plural objects when one ordinary call is natively batch-shaped
(`read_files`, `observe_jobs`, `search_project_texts`); use singular names
for exact-resource operations unless an established domain term says otherwise.
Provider-specific validation keeps `<provider>_<operation>`
(`cargo_check`, `cargo_test`, `go_test`), while a portable orchestration
entry may use a domain name such as `project_validate`.

Reserve `*_tool` for a genuine gateway into a separately named or dynamic tool
namespace, such as `plugin_tool` or `mcp_tool`. The generic
`call_runtime_tool` is the explicit Adaptive Runtime dispatch gateway. Do not
introduce `git_tool`, `job_tool`, `session_tool`, or similar mega-tools merely
to reduce the Direct inventory.

Model-visible names describe the business operation, not Host routing. Moving
between Direct, Gateway and Hidden is not a rename reason; do not add routing
prefixes such as `direct_`, `gateway_`, `hidden_`, `host_` or `adaptive_`.

Ordinary ChatGPT tool names acquire no compatibility promise from a past schema.
After a deliberate rename, the refreshed Host schema is the current contract.
Update ToolDefinition, ToolCall, schemas, discovery, generated follow-ups, tests
and documentation atomically. Do not retain duplicate model-visible tool names
just because an earlier schema exposed them. v0.5 has no legacy tool-name adapter;
ergonomic parameter spelling normalization is a different concern, described in
section 2.

Keep these four concerns independent:

1. **canonical name** — stable operation identity used by parsing, audit and
   generated follow-ups;
2. **category** — complete non-overlapping domain taxonomy owned by
   `ToolDefinition.category`;
3. **semantic contract** — effect/risk/authority/execution and result meaning;
4. **model-surface policy** — how the currently integrated Host should discover
   or invoke that same canonical tool.

Current Adaptive Runtime owns one optional `ToolAdaptiveDirectPolicy { rank,
reason }` in `ToolDefinition.adaptive_runtime_direct`. Each dedicated descriptor
has exactly one `ToolDirectReason` and a unique rank; derived rank/reason methods
keep callers independent of the representation. `None` grants no admission: an
ordinary admitted model-visible tool uses exact `read_tool_manifest` discovery plus
`call_runtime_tool`, while ModelHidden and operator extensions keep their existing
visibility/admission boundaries. Rank or reason changes are presentation/routing
changes, not renames, authority changes or new ToolCalls. Neither the policy nor
its reason is serialized into model-facing schemas/results.
`tool_manifest.route.primary`/fallback reports the current callable posture.

The Direct reasons are:

- **CoreWorkflow** — high-frequency primitive needed in the ordinary coding loop;
- **HostIntegration** — the dedicated descriptor carries Host-native input or
  another integration contract that the generic gateway cannot reproduce;
- **Presentation** — the descriptor carries MCP App/resource presentation
  metadata; while that integration is enabled it must not be replaced by a
  generic gateway call;
- **Continuation** — fresh-turn/continuation integration whose value depends on
  concrete Host lifecycle capability.

Gateway is the absence of a Direct policy plus ordinary model-visible admission,
not another Direct reason or a second tool taxonomy.

These labels explain exposure; they grant no authority and do not create a
second taxonomy. In particular, a Continuation tool may be down-admitted while a
Host cannot reliably resume a fresh turn and later promoted again without
changing its canonical name or domain contract. Presentation tools remain
dedicated when their Host resource association requires the direct descriptor.
CoreWorkflow tools should not be demoted merely to meet an arbitrary count
target.

Direct/Gateway decisions therefore belong to model-surface policy and measured
Host ergonomics. Tool names, categories, parser variants, persisted identities,
Runner protocol and domain authority must not churn when that policy changes.

### Current continuation surface

The current static Host surface does not advertise fresh-turn continuation:
`wait_for_job_terminal` and `wait_for_agent_events` keep their canonical names,
input/output contracts, keyed replay, authorization and durable state, but are
Gateway tools. `present_agent_continuation` and
`present_job_terminal_continuation` are ModelHidden, never generic Gateway
presentation targets. Their ToolCalls, handlers, resources and existing hidden
App protocol remain intact. A cached presentation descriptor follows existing
admission rules; no old-schema compatibility bypass is added.

`present_work_result` and `present_goal_plan` remain Direct with Presentation
reason. `import_conversation_files_to_project` remains Direct with HostIntegration
reason. No current Direct definition needs Continuation reason. Restore that
policy explicitly (and presentation visibility), then refresh Host schema if
fresh-turn support returns; do not couple registration to
`WEBCODEX_MCP_APP_RESUME_MODE` or `ModelWorkflowPolicy`.

Generated wait edges use the canonical Gateway wrapper; unavailable presentation
edges are omitted from schema and value. MCP-added Job carrier suggestions also
check the canonical Direct policy because they are outside the domain output
schema. Retained recommended recipes are projected through current static
visibility so dormant continuation recipes do not recommend unavailable tools.
App-only protocol descriptors are not ordinary model-tool savings.

### Stable schemas and optional workflow guidance

Host tool-schema refresh is an integration operation, not a workflow preference.
Do not change Direct ranks, tool names, schemas, descriptions or App associations
when an operator changes which workflow is recommended. Keep those contracts
static; deliver current recommendations through the existing bounded context
channel. An active conversation may retain older guidance: refresh context after
an announced policy change, not the Host tool registration, and do not poll.

Goal selection and Host interaction assumptions are separate, typed deployment
facts in `ModelWorkflowPolicy`. The baseline is on-demand; detailed Goal recipes
are an optional context chapter. Existing Goal/Wake/Job state is never retired,
completed or replayed by changing preference. A message API or accepted dispatch
is not proof that user confirmation is unnecessary. See
[`model-workflow-policy.md`](model-workflow-policy.md) for configuration and the
schema/data-refresh distinction. Extend this small boundary only for concrete
workflows, not a per-tool feature-flag or general rules engine.

## 10. Compatibility follows concrete consumers, not historical implementation

Compatibility belongs to a concrete durable/public consumer boundary, not to an
ordinary ChatGPT model-facing tool schema or name. Before retaining a legacy
name, dual shape, old argument or compatibility parser, name that real consumer.
Known ergonomic input aliases are turn-economy normalization, not this domain.

Valid reasons include, when actually present:

- durable persisted state that must still restore;
- mixed-version Server/Runner rolling operation;
- a named external client/workflow or published artifact contract;
- a required security/privacy migration boundary;
- an explicitly frozen legacy adapter contract.

"The old test expects it" and "a previous commit emitted it" are not consumers.
Historical persisted evidence should remain truthful about the past, but current
ToolDefinitions and model projections should not carry obsolete tool API baggage
solely to preserve old model behavior.

### Tool names are not persisted operation keys

`attach_agent_endpoint` has no ToolDefinition, ToolCall parser, exact discovery,
schema or dispatch entry. Use `rotate_agent_continuation_endpoint`. No feature
restores the retired name. v0.5 removes the Action adapter and its last tool-name
exception rather than introducing a compatibility registry.

The Store operation key named `attach_agent_endpoint` is a separate persisted
idempotency domain and remains unchanged. Tool naming does not migrate durable
endpoint state, replay keys, authorization or controller fencing.
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

### Completed result projections

After canonical validation, revision/fence handling, Session recording and capture
of audit/telemetry evidence, successful actual `edit_project_files` results omit
`dry_run=false`, completed execution state, the two top-level effect echoes and
applied/planned counts. `changed`, paths, summaries and per-file kind, destination,
changed/no-op facts and final `read_revision` remain. Dry-run and exceptional
results retain their existing detail.

Complete all-success `read_files` and `search_project_texts` batches omit each
item's `success=true` and `error=null`. Item indices still map to request order;
read items also retain their paths. Partial, mixed, truncated and search fallback
batches retain full item envelopes. Readiness observations omit request mode and
elapsed time only from the model result; telemetry retains both. `wait_state`,
ready Job ids/status/outcomes and pending ids remain, including terminal failures.
Passive `job_attention` presence with nonempty `items` represents changed delivery
without a second `changed=true` flag.

Execution/validation success compaction shares this late boundary: Session and
source consumers see canonical results, then definition-owned privacy projections
and generic telemetry capture bounded facts, then the model receipt is compacted.
Existing process/script/Skill and validation success shapes are unchanged.
`run_shell` removes lifecycle/Job/timing bookkeeping only for proven synchronous
exit-zero completion without Job/recovery identity or observation ambiguity. It
retains runtime-selected `command_summary`, `cwd`, `shell`, any `manage_ssh_resource`,
nonempty output, truncation/loss, expectations, normalization and sidecars. Failure,
timeout, uncertainty and exceptional handoff remain rich; normal pending receipts
still contain `execution_state=pending` and the exact continuation.

### Runtime status projections

Canonical `get_runtime_status` and HTTP/API omission retain full diagnostic output.
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
