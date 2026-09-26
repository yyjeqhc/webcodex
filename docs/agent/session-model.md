# Session Model — Two Non-Interchangeable Concepts

WebCodex uses the word **session** for two independent systems. They share
casual vocabulary only. They must not be merged, cross-wired, or inferred from
each other.

The default durable event tail retains up to 2,000 events per Session, while
one model-facing summary remains capped at 200 events (50 by default). These are
independent bounds: longer forensic/recovery retention does not enlarge one model
response, and the ledger remains a bounded tail rather than an archive. If
`retention_truncated` is true, event-derived summary counts describe the retained
ledger rather than claiming lifetime-complete history.
Executable constraints that agents must obey live in
[`AGENTS.md`](../../AGENTS.md); this document is the Workflow Sessions domain
source linked from §6. Standing architecture summary:
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

## ClientWindow is not a third session type

Adapters may derive a bounded, domain-separated `ClientWindow` from host-owned window metadata such as `_meta["openai/session"]`. The raw host value is never persisted or exposed. A principal-scoped `wc_peer_*` id may be derived from the hashed Window for lightweight collaboration routing, but neither identity is a Workflow Session selector, Project authority, credential, model-context-retention proof, implicit recorder, model-turn id, or liveness proof. Stateless MCP still never treats caller-supplied `Mcp-Session-Id` as hidden continuity.

Window correlation supports ActionAudit, agent-loop observations, bounded Peer awareness/messaging, and other explicitly designed non-authority features, while ordinary coding continuity remains the canonical Workflow Session lifecycle. `work_on_project(session_id=...)` resumes only that exact authorized Session; omission creates a fresh Session. Credentials, Project ids, windows, connections, Peer ids, and prior requests never select a Workflow Session implicitly.

The durable Agent/Conversation/Wake domain is also not a session type. A Server-minted Agent may participate in Conversations and own asynchronous Agent Tasks while concrete execution uses zero or more independent Workflow Sessions. **Agent Task** / **Agent TaskAttempt** remain a distinct work-ownership domain; similar names do not imply shared lifecycle, window binding, storage, or authority. See [`../architecture/durable-agent-runtime.md`](../architecture/durable-agent-runtime.md).

### Window Peer collaboration

Window Peer state is a small communication plane, not a third Session domain. Discovery requires the same authenticated principal, exact Project, and meaningful Window activity within a 10-minute recency window. Discovery produces a retained-state-deduplicated `peer_awareness` hint; the word "recent" is literal and does not imply online/presence state.

After discovery, a `wc_peer_*` route is principal-scoped but Project-independent. `post_peer_message` therefore does not become invalid merely because either collaborator later works in another Project/worktree; bounded Peer retention still applies. The route conveys only the explicit bounded message and safe message metadata. It grants no visibility into the peer's current Project, Workflow Session, files, branch, activity, assignment, or handoff. Session business tools retain their own target/project authorization, including the existing exact-project equality fence for recorder-to-target Workflow Session collaboration.

Peer delivery is ambient on model-facing ToolResults. Ordinary messages are persisted and marked after one projection attempt; ACK-required messages are eligible again whenever the current request omits their id. Persistent `first_projected_at_ms`, `last_projected_at_ms`, `projection_count`, and `first_ack_observed_at_ms` are analysis/observability facts only. They must not be described as delivery, reading, acceptance, current memory, or work completion.

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

Workflow Session lifecycle is independent from the durable `wc_goal_*` Goal domain. A Session may be explicitly correlated to a Goal, but that correlation grants no Session/Project authority and does not make the Session the Goal's lifecycle owner. In particular, `finish_coding_task` does not transition a Goal to `completed`; any Goal transition is a separate explicit Goal-domain mutation.

### Identity

| Aspect | Contract |
|---|---|
| ID form | Canonical `wc_sess_*` (`SESSION_ID_PREFIX`); model-facing bootstrap/discovery/handoff may additionally issue principal-scoped `session_ref` values such as `~s1` |
| Business field | `session_id` on tools that take a workflow session as input; model-facing business calls may pass either the canonical id or a Server-issued `session_ref` |
| Coding resume field | `session_id` on canonical external `work_on_project`; the internal startup primitive's `resume_session_id` is not a wire/API field |
| Recorder field | `recording_session_id` on generic wrappers, including the stateless MCP 2026 tool-argument projection (metadata only; accepts canonical `wc_sess_*` or an explicitly supplied principal-scoped `session_ref`, then canonicalizes before recorder authorization and concrete dispatch) |

### Storage and ownership

A `session_ref` is only a short model selector. The Server owns a durable mapping scoped to the authenticated principal and pins the ref to one exact canonical Workflow Session incarnation. Resolving it produces the canonical `wc_sess_*` before business authorization/dispatch; the ordinary Project visibility, Session authority, lifecycle, and guard checks then run unchanged. The ref is not a credential, bearer capability, recorder identity, or "current/recent Session" inference. If the pinned Session disappears or its exact incarnation cannot be proven, the old ref fails closed and is never recycled or silently retargeted. Canonical Session ids remain authoritative for persistence, audit, diagnostics, internal joins, and explicit API/CLI consumers.

Canonical Session identity/retention is separate from in-memory residency. `Active` and `Closed` are business lifecycle states; hot/cold residency and LRU ordering are implementation details and never lifecycle transitions. Active canonical Sessions currently remain materialized hot. The configured `hot_session_capacity_target` is therefore an observability target rather than destructive authority: when Active Session count exceeds it, the store retains those Active identities instead of deleting them or turning later exact resume into `unknown_session_id`. Restart restore follows the same rule and never trims Active rows merely to satisfy that target.

Closed historical rows use an independent bounded retention policy. Closed Sessions are coldified to compact durable JSON and remain queryable while retained; mutation remains denied and retention never reopens them. `historical_session_retention_limit` bounds retained Closed history only. When that explicit historical policy expires an old Closed row, the current v2 ledger has no tombstone shape, so a later lookup can no longer distinguish retention expiry from an identity that was never present. Adding explicit retention-expired tombstones is a separate follow-up and must not be approximated by deleting Active identities. The compatibility `max_sessions` status field now aliases the hot capacity target and must not be interpreted as permission to delete durable Active Sessions.

Per-Session event and message tails remain independently bounded (`DEFAULT_MAX_EVENTS_PER_SESSION` and `DEFAULT_MAX_MESSAGES_PER_SESSION`); preserving a canonical Active identity does not turn its event/message history into an unbounded archive. The persistence wire shape remains ledger version 2 because this change alters retention/restore policy, not the serialized Session row schema. Existing Session rows already deleted by an older Server cannot be reconstructed by upgrading: the fix prevents future destructive capacity loss from the first upgraded snapshot onward.

`recording_session_id` remains a separate provenance contract. It may explicitly carry the same principal-scoped `session_ref` returned for that exact Session; Runtime canonicalizes the ref before recorder authorization and durable provenance recording. This does not make recorder provenance a business Session target, and omission never selects a recorder implicitly or creates sticky recorder context.

Stateless MCP 2026 never derives a Workflow Session recorder identity from transport/window continuity. ChatGPT may supply `_meta["openai/session"]` as a hashed `ClientWindow`, but that identity is intentionally not a Workflow Session selector or trusted provenance source. Its `tools/list` schema therefore projects `recording_session_id` as explicit wrapper metadata for runtime tools. A call may carry `recording_session_id=W` while the concrete tool body carries business `session_id=C`; the MCP adapter removes the recorder field before concrete parsing and the kernel independently authorizes `W` before it can record evidence or supply trusted collaboration provenance. This does not revive legacy `mcp-session-id`, grant target authority, or infer a recorder from credentials, project identity, connection state, or `ClientWindow`.

Session collaboration attention has a narrower fallback that never becomes recorder identity. When an ordinary Project tool omits `recording_session_id`, the same strict Window/principal/exact-Project affinity used by the recorder-gap diagnostic may locate one latest authorized Active Workflow Session solely to project open ACK-required messages and accept request-scoped ACK ids. The result names that exact `session_id` and marks `source=window_affinity`. This fallback cannot append ToolCall events, supply business `session_id`, inherit execution context, resolve a message, complete a todo, change Goal/Task authority, or mutate Session lifecycle. An explicit recorder always selects attention instead.

Window activity correlation is observational and does not weaken that targeting
rule. Stateless MCP may persist the hashed `ClientWindow` on ActionAudit events
and attach a normalized Window↔Workflow Session relation only after the existing
Session/Project authority path has already established the fact: an authorized
outer `recording_session_id` yields a `recording` relation, while a successful
canonical `work_on_project` create/resume yields a `work_on_project` relation from
its typed projection. These relations are intentionally many-to-many and confer
no lease, ownership, authority, or implicit recorder selection.

Agent-loop timing uses the same observational boundary without adding a Session
identity. Adjacent meaningful MCP calls are eligible to pair only when the hashed
`ClientWindow` and canonical authenticated principal correlation both match.
Project remains an event dimension and current-visibility boundary, not the
continuity identity. A matching Window does not prove a matching model turn:
WebCodex receives no reliable turn/generation/response id and never infers one
from elapsed time. Missing or malformed host Window metadata therefore leaves
loop continuity unavailable rather than falling back to Workflow Session,
credential, Project, connection, trace, or MCP Session identity.

For a later successful meaningful Project tool call with no explicit recorder,
WebCodex may diagnose a **recorder continuity gap** when the same hashed Window,
canonical principal, and exact Project have a recent explicit Session affinity.
The candidate must still be Active, match the exact Project, and pass the current
caller through the ordinary Session authority check. The diagnostic may suggest
that exact Session to the model, but the missing call is not backfilled into the
Session ledger and the candidate never becomes a sticky recorder. Status and
discovery calls are not meaningful gap evidence. This preserves the original
Session event order, revisions, ACK state, validation evidence, and permission
facts while making an otherwise silent recorder discontinuity observable.

One real kernel tool request also receives one trusted runtime-generated logical invocation correlation id. The outer recorder event pair and any inner concrete business-execution event pair inherit that id while retaining independent pair-level `call_id` values. A small recorder/business role discriminator lets Session-local semantic projections deterministically prefer authoritative business execution facts when both pairs land in the same Workflow Session. Raw ledger facts remain intact. Correlation never grants authority and is not a permission identity, retry token, idempotency key, execution identity, lifecycle key, or model-supplied input. If recorder Session `W` and business Session `C` differ, each Session keeps its own one-invocation semantic evidence; correlation is never used for cross-Session global deduplication. Current-v2 ledger events without the additive correlation fields remain uncorrelated and are projected conservatively per event; restore never invents an id or rewrites persisted history.

Stateless MCP 2026 projects two request-scoped ACK evidence forms. `ack_session_message_ids` remains the bounded exact-ID form and can acknowledge both Session and Window Peer messages. For Session attention, the Server also returns an optional compact `ack_ref`. Each returned ref represents the exact Session ACK set explicitly retained across the current exchange: ACK evidence accepted on this request plus ACK-required messages newly projected in this response. As further bounded messages are projected, the next ref carries that retained set forward in one value. Echoing the ref on a later ordinary tool call expands only after the exact authorized attention Session has been established: the explicit recorder when present, otherwise the narrow authorized Window-affinity fallback above. The ref is Session-only, principal/Session-bound by the existing authority path, and fenced against the full current open ACK-required Session membership; adding or removing membership makes an older ref acknowledge nothing rather than absorb a different set. ACK bookkeeping itself does not change membership, so exact replay remains safe. Peer ACK continues to use explicit `ack_session_message_ids`.

The adapter removes both ACK forms before concrete tool parsing. Neither grants authority, resolves a message, accepts work, creates sticky Session/Window context, or gates the concrete tool effect. Any Session or Peer message kind/priority may request ACK. Accepted Session evidence suppresses only the represented body/bodies for the current response; later omission makes unresolved Session messages eligible for bounded re-projection with a current `ack_ref`. Retained Peer ACK messages follow the existing exact-ID behavior. Historical ACK state is never used to infer current model-context retention.

An ACK-required Session message may persist `first_ack_observed_at`; an ACK-required Peer message persists the analogous window-message timestamp. For Session messages only the first accepted ACK advances message-observation revision; repeated echoes do not create revision churn. These fields mean only that the Server once observed an explicit ACK echo. They are not delivery/read receipts and do not by themselves change business status. `resolve_session_message` remains the durable processed-state transition for Session messages.

Window Peer transport is bounded retained communication, not a durable task queue. Retention pruning may eventually remove old Peer messages or discovery edges, so ACK-required Peer re-projection lasts only while the message remains retained; durable work ownership and completion continue to use explicit Workflow Session or Agent Task primitives.


Workflow Session targeting is explicit in 0.4. Canonical external `work_on_project` creates a fresh Workflow Session when `session_id` is omitted and continues only the exact existing `wc_sess_*` when it is supplied. The retired `start_coding_task` wire/API name is not a second continuation path. Ordinary project tools do not infer a Workflow Session from caller identity, window identity, project identity, or prior calls. To record a call in a Workflow Session, pass an explicitly authorized `recording_session_id`; when a tool has its own Session business input, that explicit id is authorized independently.

Project scope is fail-closed. An explicit project-scoped business Session or recorder must match the canonical resolved request project before business execution or Session mutation. There is no cross-project warning/escape mode. `complete_session_message` records an answer author only from an explicitly authorized recorder; without one, author Session provenance is absent rather than inferred.

The JSON ledger restores only the current version-2 top-level shape and canonical current Session rows. Pre-current ledger versions are rejected rather than migrated. Within v2, fields explicitly declared optional/default may be absent and restore conservatively. The retired `context_revision` members are accepted only through explicit read-only compatibility sinks and are never restored into live Session state or re-emitted; other unknown row members still fail that row closed. General `ClientWindow` support remains available to explicitly designed non-Workflow observations; Workflow Sessions do not use it for selection or authority.
Optional explicit control mutations may also use the Stateless MCP 2026
[`_control` sidecar contract](control-sidecars.md). Each phase admits one mutation
with its canonical authority and replay fences; standalone tools remain valid.

### Assignment-fenced todo completion

Executable todo completion is assignment-fenced in 0.4. A worker first calls `get_session_assignment` for the exact coordinator `session_id + message_id`; one atomic store snapshot returns the open todo, every retained direct reply within the bound, and an opaque Session/todo-bound `assignment_fence`. Current `complete_session_message` requests require that exact token as `expected_assignment_fence` together with the independent caller `completion_key`. Assignment-local semantic changes stale the fence before mutation; unrelated Session traffic and ACK bookkeeping do not. A stale result has `state_changed=false` and includes the current assignment plus a fresh durable fence only when that exact current state remains provable. Retention loss or an oversized direct-reply set is non-completable from stale context.

The fence and completion key are different identity domains: the fence proves the semantic assignment snapshot, while the completion key correlates one accepted intent across uncertain retries. `ack_session_message_ids` and compact Session `ack_ref` are request-scoped ACK evidence; message-observation tokens are a separate cursor domain. `session_ref` is a principal-scoped exact-Session selector and never ACK evidence. The field carrying it determines the role: business `session_id` remains the independently authorized target, while `recording_session_id` remains provenance only and never supplies business guards or execution defaults. None substitutes for assignment/completion identity. Current persisted rows must carry the assignment-history and completion-fence tracking metadata required by the v2 format; restore never invents a missing fence, fingerprint, or tracking state, and a row that cannot prove the current shape is discarded rather than admitted for replay.

### Explicit handoff recovery and internal snapshot fencing

Workflow Sessions no longer maintain a model-context checkpoint revision, ACK
baseline, or ToolDefinition checkpoint classification. Finished tool calls use the
same canonical ledger append path regardless of whether their ToolResult is
model-facing. Durable consequence evidence remains in Session events, including
bounded `context_result_summary`, validation/effect evidence, observed/changed
paths, repository-edit sticky state, and Job evidence. `events_observed` remains
the cumulative Session event count, including events later removed by retention;
it is not a model-context token or cursor.

Normal continuous work has no context-revision ACK input, automatic recovery
delta, or handoff suggestion on unrelated tool results. When task context is
missing, the model explicitly calls `session_handoff_summary` with the exact
`session_id`. No identity, Project, window, transport, or recent-call state can
select a Session implicitly. The authorized business Session supplies its Project
when `project` is omitted; an explicit Project still passes the normal equality
and authority checks. An outer recorder never changes the recovery target.

The default result contains `session_id`, `project`, and `handoff_brief`. The
shared deterministic brief is hard-bounded at 8 KiB and carries root/latest task
instructions, workspace state, progress/changed paths/recent files, validation,
active/recovering Job attention, open collaboration counts, next actions, and
basis completeness. `diagnostic=true` explicitly adds detailed ledger and closeout
evidence. `include_workspace`, `include_validation`, `include_checkpoints`, and
`limit` continue to select bounded observations; omitted/unavailable evidence is
reported truthfully in the brief. There is no implicit handoff or ACK baseline.

Handoff assembly captures an internal Session snapshot fence before gathering
workspace/Job/Session evidence and compares it again afterwards. The Session fence
has exactly two independent mutation dimensions: `events_observed` for ledger event
mutations and `message_observation_revision` for collaboration/message mutations.
External reports remain a separate evidence plane, so the handoff also captures a
bounded external-report snapshot and compares that retained projection again after
the other recovery reads. A Session fence change reports
`session_changed_during_snapshot`; an accepted external-report change reports
`external_observations_changed_during_snapshot`. Either makes `basis.complete=false`
so the caller can explicitly re-observe before dependent work. This detects evidence
races without claiming atomicity across independent Runner workspace or Job reads,
and without promoting external reports into Session revision or native execution
truth. No handoff generation or replacement model-context revision is introduced or
returned.

Collaboration is independent: `ack_session_message_ids` still proves exact
ACK-required Session/Peer message ids, while `session_attention.ack_ref` is the
compact exact-set form carrying accepted Session ACK evidence plus messages newly
projected in the current exchange.
`session_attention` suppression/re-projection, message resolution, assignment
fences, completion keys, message-observation tokens, and their durable revision
keep their existing semantics. A handoff neither ACKs nor resolves a message and
grants no authority.

Stateless MCP 2026 tools also accept an explicit bounded `context_request` wrapper sidecar request. It is independent of collaboration ACKs and handoff recovery and is removed before concrete `ToolCall` parsing. A static canonical material registry authorizes every requested material before its provider is read: `webcodex.workflow` is public; `project.instructions` requires the resolved Project plus `project:read`; `jobs.attention` requires the exact resolved Project plus canonical `runtime:read` and reuses the authorized active-Job summary (at most eight recent Jobs) without selecting a business Session; `skills.catalog` additionally requires the admitted Skill runtime protocol capability; `plugins.catalog` requires the resolved Project plus both `project:read` and `plugin:inspect`; and `memory.bootstrap` requires the admitted Memory protocol capability plus both `project:read` and `memory:read`. Scope or material-capability denial is nonfatal to the main ToolResult and returns a bounded unavailable material without provider content. Unknown material keys are nonfatal and remain open-ended at the MCP schema layer. Sidecar material is projected only after the main tool effect or observation has completed, never grants authority, never retroactively makes requested guidance a precondition of that effect, never records caller-read state, and never infers a Project or model-memory state from a Workflow Session, connection, credential, `Mcp-Session-Id`, or hidden window identity. A model that has lost Project rules or durable Memory guidance must recover `project.instructions` and/or `memory.bootstrap` on an observation call, use `memory_read` when detailed Memory content is needed, reason over that context, and only then issue a later mutation that must obey it. Legacy MCP and generic REST/GPT Actions/OpenAPI do not expose this sidecar request contract.

Project Memory is a separate durable knowledge plane from Workflow Session continuity. `memory_search`/`memory_read` require both `project:read` and `memory:read`; `memory_set`/`memory_delete` require both `project:write` and `memory:manage`, with mutations still passing the independent permission evaluator. Direct shared-key runtime credentials explicitly carry both Memory scopes, while Open Anonymous, ProjectCredential, Project Share, and legacy/default OAuth client scope sets do not gain them from project scopes. A Memory `memory_key` is logical semantic identity, `memory_id` identifies the current incarnation, the internal `definition_hash` identifies canonical model-relevant content, and model-facing `revision` is a generation-bound state ETag/CAS identity; delete and identical recreate therefore produce a different `memory_id` and `revision`. Session events never create or consolidate Memory automatically. `ack_session_message_ids` and Session `ack_ref` are limited to ACK-required collaboration messages and never acknowledge Memory. Memory reads/searches may leave bounded metadata-only consequences in Session history, but Memory bodies, summaries, search results, and `memory.bootstrap` projections are not copied into durable Session recovery. Re-registering the same runtime Project id to a different authoritative registered root resolves to a distinct internal Memory scope rather than inheriting the old root's Memory.

### Message observation state

The Session-local message board has a separate durable monotonic **message-observation revision** used by `observe_session_messages`. The public observation token is bounded and opaque; it binds the exact Workflow Session plus durable cursor state without exposing the internal revision as the caller cursor. It is observation state only and grants no authority. Malformed, oversized, wrong-Session, and future-revision tokens fail closed.

A no-token call establishes the current baseline and returns no historical messages. Later token calls return retained messages whose latest observable state changed after that cursor, optionally with one bounded wait. Posts are observable mutations; resolve advances only on a real field/status change; a new `complete_session_message` observes both the todo resolution and answer creation; exact idempotent replay does not advance. This is current-state delta, not an audit/event log, so multiple changes to one retained message may collapse to its final state.

Retention correctness does not infer continuity from deque length or position. Retained messages keep their latest internal observation revisions and the Session persists a low-watermark for removed observation history, including non-FIFO completion retention holes. `history_lost=true` tells callers when a cursor predates state that can no longer be reconstructed. With pagination, a token advances only through the last returned change while `has_more=true`. Observation-token issuance fences the ledger generation containing the cursor revision, so tokens issued by the current implementation remain valid across Server restart when the Session itself restores.

`delivery_key` replay is likewise durable but bounded by message retention rather than an unbounded global idempotency ledger. While the keyed Session or Peer message remains retained, exact same-payload retries return the original `message_id` across Server restart and conflicting key reuse fails closed. Once the corresponding retained message/replay metadata is evicted by the existing bounded retention policy, that old key is no longer promised as replay authority.

Waiting uses process-local notification only as a wake signal; durable revision state remains the truth. No Session-store or persistence-writer mutex is held across the bounded await, unrelated Session mutation can only cause a spurious recheck, and timeout is a successful unchanged result rather than a tool failure.

Message observation is not a delivery receipt, not model-context retention, not a subscription/stream, and not an Agent Wake. Durable Agent/Conversation/Participant/Delivery/Wake state already exists as a separate Server-owned domain; this Workflow Session cursor neither observes nor mutates it. Presence, typing, Agent Task scheduling/worker-pool behavior, automatic worker spawning, and routing remain separate additive capabilities rather than reinterpretations of this cursor.

Every Workflow Session admitted to the in-memory store carries one canonical creation-time authority-group fingerprint. The ledger keeps only that domain-separated SHA-256 fingerprint under the historical `owner_authority_fingerprint` field; raw user, shared-key, project-grant, credential, or window identity material is never persisted as Session authority. Project authorization and creation-time Session authority are separate checks: access to a project does not authorize another authority group's Session, and a matching Session fingerprint does not bypass project authorization or project equality.

### Persistent execution defaults

An existing registered-project-bound Workflow Session may persist a closed set
of strongly typed execution defaults. The wire/type name remains
`SessionExecutionContext` for compatibility:

```text
SessionExecutionContext {
  default_cwd: project-relative path? | remote path?,
  default_shell: sh | bash?,
  resource: named Runner SSH resource?
}
```

These fields are execution defaults, not arbitrary model context. They cannot
contain environment variables, credentials, SSH host/configuration, keys,
passwords, SSH state, shell input, connection data, or custom options.
`resource` is only a safe named resource configured on the Runner that owns the
Session project. It is persisted as a name, never as an SSH transport or
authentication material.

Without `resource`, `default_cwd` is validated and normalized as a
project-relative path: absolute paths, URI forms, control characters, and
parent traversal fail without changing Session state. Filesystem existence,
canonicalization, symlink, and allowed-root checks remain in the normal Runner
execution path and fail closed there without retrying from the project root.
With `resource`, `default_cwd` is instead a bounded remote path; it is not
checked against the Runner project root, and the remote shell reports an
unenterable cwd explicitly.

Inheritance is intentionally closed and per field:

| Tool | `default_cwd` | `default_shell` | named `resource` |
|---|---|---|---|
| `run_process` | inherited when `cwd` is omitted | not applicable | unsupported; fails before process start |
| `run_script` | inherited when `cwd` is omitted | not applicable | unsupported; fails before script start |
| `run_shell` | inherited when `cwd` is omitted | inherited when `shell` is omitted | routes this call through the named SSH resource |
| `run_job` | inherited when `cwd` is omitted | inherited when `shell` is omitted | routes this Job through the named SSH resource |
| `open_session_shell` | inherited when `cwd` is omitted | inherited when `shell` is omitted | opens the persistent shell through the named SSH resource |

Structured Cargo/Go tools (`cargo_fmt`, `cargo_check`, `cargo_test`, `go_test`)
do not inherit `default_cwd` or `default_shell`; a named `resource` is also
rejected for those tools rather
than silently falling back to the Runner-host project. File, Git, LSP, and
checkpoint tools do not inherit any execution default.

`run_shell` and `run_job` remain independent-process tools. When an SSH
resource is selected, `run_shell`, `run_job`, and a newly opened
`open_session_shell` execute through that remote resource. Remote cwd
precedence for one-shot SSH commands is:

```text
per-call cwd
> exact active project-matched Workflow Session default_cwd
> SSH resource default_cwd
> remote login default directory
```

Each SSH `run_shell` / `run_job` command gets an independent remote exec
channel and requires the Runner's `ssh_shell` capability. Unix may reuse a
Runner-local authenticated ControlMaster transport for the same
Session/resource/generation; Windows starts one direct `ssh.exe` process for
each one-shot/background execution and creates no mux state. Neither transport
preserves `cd`, exports, aliases, functions, umask, or shell-process state between
commands. A generation change affects future preparation only: an already-spawned
command is never redirected, replayed, or blindly retried. This
one-shot/background capability is independent of named SSH persistent-shell
support.

Raw shell text has one shared model-authored ceiling of 16,000 UTF-8 bytes for
`run_shell`, raw `run_job`, and `session_shell_exec`. Larger shell program text
belongs in `run_script`; large literal data belongs in stdin, files, or
artifacts. Control may internally expand an explicit `sh`/`bash` command while
POSIX-quoting it for the existing Runner wire, so the internal raw-shell wire
envelope is separately capped at 64 KiB and revalidated by the Runner. That
transport headroom is not an additional model-facing command allowance.
A concrete OS/shell launch envelope may still be narrower (notably Windows
`CreateProcess` after shell-specific wrapping); such a request remains a
pre-start failure rather than weakening the authored bound or silently changing
execution semantics. Large or quote-dense program text should use `run_script`.

A missing Session leaves execution unchanged. A mismatched Session fails or,
on an explicitly authorized cross-project escape path, executes without
inheriting its context.

The cross-project escape is a low-level compatibility/debug control. It remains
auditable when explicitly used, but model-facing ToolSpecs and flattened Action
arguments do not advertise it. Read-only mismatches may continue with a factual
warning and without inheriting Session context; write/shell boundaries that
require the escape fail closed on ordinary model paths.

### Pre-declared execution result expectations

Execution intent (`purpose=diagnostic`, `test`, and so on) is evidence metadata, not
proof that a failed process is harmless. A started shell/process failure therefore
remains actionable by default. Model-facing execution and structured-validation tools
may instead declare a bounded `result_expectation` **before** execution: the omitted
`success` default requires ordinary success; `failure` expects a completed known
business failure; and `observe` accepts either completed known business result. The
legacy internal `expected_failure` / `expected_failure_kind` recorder fields remain a
separate compatibility/testing mechanism and are not exposed as the normal model
contract.

`run_process` additionally accepts a bounded `accepted_exit_codes` set for commands
whose exit status is itself a result (for example a boolean Git predicate). Matching
expectations change only ledger expectation/actionability classification. Validation
evidence keeps three facts separate: the real execution/validator outcome, whether the
pre-declared expectation was satisfied, and whether validation itself passed. A matched
negative/observation failure is neutral expected-result evidence: it does not become a
validation pass and cannot resolve an earlier real validation failure with the same
identity. ToolResult, exit code, effect evidence, authorization decision, and execution
state are never rewritten. Pre-start rejection, permission/guard denial, transport failure,
malformed result, timeout/cancellation, and unknown/lost outcomes remain fail-closed.
A Job admission is not a terminal match: structured validation Jobs inherit the
pre-declared expectation and classify it only when terminal evidence is materialized.
If the originating expectation evidence is no longer retained, terminal projection
falls back to the normal success-required behavior rather than guessing.

### Explicit persistent shell

The canonical runtime has a separate, command-oriented `PersistentShell` model:

```text
open_session_shell
session_shell_exec
session_shell_status
close_session_shell
```

Opening creates one real long-lived Runner-owned shell process, at most one
active shell per Workflow Session: `sh`/`bash` on Unix, or the configured
PowerShell program/profile on Windows. For an `agent:<client>:<project>` the Runner owns
and controls the shell. Without `execution_context.resource`, it runs against
the registered project host. With a named resource, the Runner opens a remote
persistent shell through that SSH resource; this requires `persistent_shell` +
`ssh_persistent_shell`, not the separate one-shot/background `ssh_shell`
capability, and never silently falls back to the registered Project host. Unix may reuse its OpenSSH mux;
Windows owns one direct long-lived `ssh.exe` channel while the remote shell remains
`sh`/`bash`. No PTY/ConPTY or terminal-control protocol is implied. Registered
Project execution is Runner-owned; the Server has no Project-local process or
persistent-shell fallback. `read_only` Sessions cannot open or execute a persistent
shell. The pre-0.4 `inspect` Session mode is retired.

Runner-project open resolution is explicit `cwd`/`shell`, then the exact Session's
`default_cwd`/`default_shell`, then the project/Runner defaults. The explicit
`sh`/`bash` override is Unix-only; Windows callers omit it and the Runner uses
the configured PowerShell program/profile, failing closed for incompatible
configuration. For an SSH persistent shell, cwd precedence is explicit open
`cwd`, Session `default_cwd`,
the named resource's default cwd, then the remote login default; the selected
Session `default_shell` is also inherited when `shell` is omitted. Profile
environment and initialization run once at open. Later commands retain the
same process's cwd, exports, unset state, umask, functions, and ordinary shell
variables. The shell record retains the execution location selected at open, so
updating Session execution defaults never redirects, moves, restarts, or
changes an already-open shell; close and reopen it to apply new defaults.
`run_shell` and `run_job` never reuse this process.

The random `shell_id` is bound to the exact Session, runtime project, executor,
Runner client when applicable, dialect/profile, initial cwd, and timestamps.
Every operation rechecks caller authorization and active Session/project
identity; before exec/status the Runner also rechecks its current project,
raw-shell, cwd, allowed-root, profile, and shell policy. Close remains available
for cleanup after an execution-policy change. A closed id remains terminal and
cannot address a subsequently opened shell. Explicit close is idempotent.
Session close, project disable/unregister, idle expiry, shell exit, Runner
disconnect/shutdown, and detected process/control-channel damage release the
process group and its pipes.

Commands are serialized; a concurrent command receives `shell_busy`. Output is
bounded independently for stdout and stderr. Completion uses transport-private
control framing with a high-entropy per-command token, never an ordinary output
marker: Unix uses inherited control descriptors; Windows uses a private control
file plus exact stdout/stderr drain boundaries. On timeout the owner performs
only the platform's safe bounded recovery attempt. If framing synchronization
cannot be proved, it terminates the owned process tree, marks the shell
poisoned/lost, returns `shell_reset_required`, and never writes another command
to that process.

Persistent shells are process-local and are not durable Job records. Neither a
Server nor Runner restart claims to recover or reattach one from ledger data.
There is no PTY, raw keystroke/input stream, terminal resize, WebSocket terminal
UI, or full-screen terminal application support.
The Session ledger stores bounded lifecycle and permission evidence
(`shell_id`, action, shell/execution state, error code, and completion flags),
never command text, stdout/stderr, the complete environment, internal shell
state, credentials, or unbounded command output.

The context is an additive serde-defaulted ledger-version-1 field, so older
ledgers load it as `{}` without a version bump. Startup and Session summaries
return the complete current context. Context changes record bounded structured
metadata only; no command, environment, token, or secret content is added to
the audit ledger.

### Full-runtime coding continuity

`work_on_project` is the canonical external full-runtime start-or-continue
aggregate. The former `start_coding_task` tool name and advanced direct/API
schema are retired and fail closed before dispatch. No internal `ToolCall`
variant remains for that retired name; `work_on_project` calls the shared coding
workflow engine directly, with diagnostic projection controls available only to
tests rather than as a Session-selection or compatibility surface.

`work_on_project` also owns the optional managed-worktree bootstrap without
creating a new authority or Session concept. On a fresh `client_id + path` call
with `mode=worktree`, `path` is a source checkout: the selected Runner validates
the source against its filesystem policy, resolves `base_ref` (or the source
`HEAD`) to an exact commit, chooses and creates an isolated detached worktree,
registers that worktree as an ordinary runtime Project, and only then creates the
Workflow Session on that final Project. The Server never constructs a Runner-host
worktree path or interprets the Git ref. `work_on_project` hides Project
registration and managed-worktree bootstrap from the ordinary model workflow;
Project authority itself is not removed.

An explicit `session_id` in worktree mode resumes only its already-authorized
final managed Project. The Runner re-observes that registered worktree and its
source provenance instead of creating another worktree; a provided `base_ref`
must still resolve to the stored exact base commit. `recording_session_id`,
`ClientWindow`, ACK metadata, source-path knowledge, and managed operation ids do
not select or authorize the Project. Finishing or closing the Workflow Session
does not remove the managed worktree or unregister its Project; later review,
commit, push, PR, handoff, and investigation remain possible until a future
explicit lifecycle operation says otherwise.

`work_on_project` deliberately does not use Workflow Session identity, transport
identity, a client-window key, credentials, project identity, or Server lifetime
as evidence that the current model still retains static bootstrap content. The
same `wc_sess_*` may be explicitly resumed by multiple independent ChatGPT
conversations. Its primary result therefore stays compact: static Project
instruction bodies and WebCodex workflow guidance are projected only when the
caller explicitly requests `project.instructions` and/or `webcodex.workflow`
through `context_request`. Omission means no static material, not an inferred
retention state. `include_extension_catalog` remains a separate caller-explicit
selection-metadata preference.

`guidance_profile` is a request-local presentation enum. Explicit selection wins;
when omitted on MCP, `McpHostRuntimePolicy.profile` (configured by
`WEBCODEX_MCP_HOST_PROFILE`) supplies the default, while non-MCP/internal omission
falls back to `direct`. Available explicit values are `direct`, `host_code_mode` for
Host-supplied native orchestration in every build, or `code_mode` for WebCodex nested
orchestration only in Experimental Code Mode builds. Host-native guidance favors canonical batches and `search_and_read`,
allows independent cross-tool observations and dependent branching within one
Host cell, keeps raw ToolResults in that cell, and warns against Job polling and
stale validation after covered source changes. It does not assert that WebCodex
verified Host capability or require nested Code Mode. Exact resume may choose any available profile
without a Session transition; omission is resolved from the current transport/Host policy,
never a remembered choice. It is not persisted in Session state or event arguments and
changes no admission, authority, effects, validation or Job semantics. An unavailable
profile fails parsing. `work_on_project` startup and later `webcodex.workflow` context
refreshes use the same effective-profile resolver, preventing Direct/HostCodeMode drift.

`current_window_activity` observes persisted ActionAudit activity for the exact
ClientWindow supplied by the current adapter request. Its input cannot select
another Window. It remains available through the canonical adaptive runtime
gateway and `tool_manifest` without expanding the default direct tool inventory.
It requires `runtime:read` and a non-anonymous authenticated
principal, fixes that principal,
and reapplies current Project visibility to every event and Workflow Session
link. Runtime Console and the model tool share the sanitized event and timing
projection. The tool scans at most 200 records and returns at most 50 events
within a smaller serialized byte ceiling. It exposes descriptive timing and
counts only, without arguments, outputs, paths, credentials, or principal IDs.
`response_handed_at_ms` means WebCodex constructed the response and handed it
to the HTTP framework or returned from the handler. It does not prove client
receipt, MCP Host receipt, ChatGPT ingestion, or model continuation. A
`next_call_gap_ms` exists only when a later canonical meaningful call was
actually observed; elapsed wall time alone never supplies that value. The
diagnostic call is nonmeaningful and excludes itself from active requests.

Repository instruction files are still re-observed and Session metadata/delta
status still update even though instruction bodies are absent from the primary
output. The default bounded
extension catalog contains selection metadata only: Skills are drawn from the
same canonical union as `skill_list` (project `.agents/skills`, Runner-configured
live `skills.roots`, and active Runner-managed Skill Store packages), while
Plugins are limited to ready committed providers whose configured `cwd` matches
the authoritative Project root. Skill bodies, Plugin schemas, bindings, native
paths, commands, environments, and provider identities are not projected; using
a selected Plugin still requires `plugin_tool describe` before `call`.

Startup selection is strict and ordered:

The durable projection stores only:

The canonical hash input is the principal kind/id, transport, already-hashed
stable window identity, resolved project, and already-hashed canonical
repository root. It uses fixed field order and length prefixes under
`webcodex.workflow-current-binding.v1`. Raw MCP session ids, hosted
conversation ids, cookies, credentials, authorization headers, and repository
paths never enter this projection, and neither binding hashes nor component
hashes are returned to the model.

The binding field is an additive, serde-defaulted field in ledger version 1, so
older ledgers load it as empty without migration and keep their existing
Session events/messages. Restore accepts only bounded, lowercase SHA-256 keys
that reference known active `wc_sess_*` records. Malformed, duplicate,
conflicting, missing, closed, project-mismatched-on-lookup, and excess entries
are discarded without rejecting valid Session data. Internal status exposes
only bounded counts (`durable_binding_count`, `restored_binding_count`,
`discarded_binding_count`) and never a binding key.

This durable binding remains Workflow Session state and is separate from ActionAudit/window observations. It does not create a second coding-task lifecycle, and window correlation never infers or mutates a Workflow Session.

### Current lifecycle contract

This is **not** the same state machine as Action Audit Sessions. Lifecycle
tools and error kinds (`unknown_session_id`, `session_closed`, mode denials,
guard failures) apply only to Workflow Sessions.

### Continuation feedback (`continuation_feedback`)

`continuation_feedback` is a **deterministic, read-only projection** surfaced
by `finish_coding_task`, `session_handoff_summary`, and (as `validation_delta`)
`validation_summary`. The internal coding-startup implementation also consumes
this projection while building canonical `work_on_project` startup state. It is derived only from existing
persistent state — the Workflow Session ledger, validation evidence, bounded
Job metadata, and the session message board — and it is never a substitute for
a `finish_coding_task` verdict.

- **Read-only projection contract:** building it directly never executes shell,
  reads project files, enqueues Agent/Runner requests, mutates the ledger,
  refreshes activity, consumes or auto-resolves guidance, or calls an LLM.
  Canonical coding startup still appends its legitimate new `task_instruction`.
  Public MCP, REST, or runtime dispatch also records the enclosing tool's
  uniform `tool_call_started` / `tool_call_finished` telemetry, so
  `events_total`, `updated_at`, and activity telemetry are not required to stay
  unchanged across a public call. Those recorder facts are separate from the
  projection's business semantics.
- **Startup describes the previous attempt:** for reused, explicitly resumed,
  and restored-after-restart sessions, canonical `work_on_project` startup snapshots the
  pre-instruction state *before* appending the new instruction, so
  `continuation_feedback.attempt` describes the *previous* attempt's bounded,
  redacted instruction excerpt, activity, changes, current unresolved failure
  identities, and validation — not the empty new attempt. When an unresolved
  identity is available, the first suggested action names that concrete target.
  A fresh session reports
  `status = not_applicable`, `reason_code = fresh_session`.
- **Attempt boundary:** the attempt window is segmented by the most recent
  `task_instruction` retained in the ledger window. When that instruction has
  been evicted by the bounded event limit, the boundary is reported as
  `source = unavailable`, `reason_code = attempt_boundary_evicted`, and
  `event_range.complete = false` — the projection never masquerades a truncated
  retained window as `session_start` with `complete = true`.
- **Exploration workset:** `attempt.exploration` projects only successful,
  structured evidence from focused `read_files`, `search_project_texts`, and
  typed LSP navigation calls. The existing ledger retains only a bounded set
  of validated project-relative paths; it never retains search patterns or
  previews, file contents, symbol/hover/diagnostic bodies, arbitrary result
  JSON, shell commands/output, or the absolute repository root for this
  workset. Paths are deduplicated newest successful observation first.
  Enumeration tools such as `project_overview`, `list_project_files`, and
  `list_project_tracked_files`, Git diff lists, failed calls, error text, and
  shell output are not exploration evidence. The workset is segmented by the
  same attempt boundary; when that boundary was evicted,
  `exploration.complete = false` as well.
- **Continuation reuse, not execution:** automatic continuation, explicit
  resume, read-only to normal mode upgrades, and ledger restoration reuse the
  prior attempt's workset. Startup returns at most 3 paths in
  `minimal` and 12 in `standard` (including the core embedded by `full`); full
  continuation feedback returns at most 100 with the real total and
  truncation state. This is a hint for model judgment only: startup never
  reads, searches, or navigates those paths automatically.
- **Handoff is independent of the display limit:** `session_handoff_summary`
  builds its display list from the caller-supplied `limit`, but
  `continuation_feedback` reads an independent bounded evidence snapshot (the
  maximum retained event window), so a small display limit cannot shrink the
  attempt boundary. `include_validation = false` does not fabricate validation;
  it reports `validation_not_requested` rather than `not_run`.
- **Validation delta comparability:** `validation_delta` is `available` only
  when the latest and prior runs are *proven* comparable — same validation
  kind/tool/cwd and structured scope (package, filter, features, targets,
  purpose), with complete evidence on both sides and a consistent parser
  identity. Otherwise it reports a stable reason code
  (`no_previous_validation`, `validation_scope_changed`,
  `previous_evidence_incomplete`, `current_evidence_incomplete`,
  `parser_changed`, `parser_identity_unavailable`, `test_identity_unavailable`,
  `insufficient_scope_identity`, `validation_not_requested`). Count deltas are
  signed integers (a decrease in passed tests yields a negative `passed_delta`);
  zero-test success never resolves a prior test failure.
- **Async terminal validation evidence:** structured validation Job metadata carries the same opaque `validation_target_id` as the originating validation attempt. When an authorized validation-summary or Runtime Console Session read observes a retained terminal Job, WebCodex idempotently materializes one bounded `validation_job_terminal` event in that exact Workflow Session before projecting validation state. Idempotence does not depend on that event remaining in the 200-event Session FIFO: the version-1 ledger also persists a serde-defaulted exact Job-id marker set bounded to the Runner authoritative terminal inventory limit (64), and a new materialization evicts only markers absent from the current terminal-candidate snapshot. The marker check, marker insertion, and event append commit under one Session-store mutation, so concurrent reconcilers append at most once and restart restoration keeps the same suppression identity. Terminal reconciliation also serializes authoritative candidate-snapshot acquisition through marker/event materialization within one runtime: a later snapshot cannot commit first, so an older snapshot never gains eviction authority over a marker established from newer inventory. Synthetic evidence uses the authoritative Job `finished_at`; reconciliation never advances Session activity to the wall-clock read time. This is recovery/materialization only: it never re-runs validation, never treats acceptance/handoff as terminal success, and never exposes raw Job output. A later terminal success for the same structured target can therefore resolve an older retained failure even after the acceptance event is gone; the materialized terminal evidence then follows normal Session persistence/retention across Server restart.
- **Cargo test-count postconditions:** a `cargo_test` caller may require a
  bounded minimum count with `require_tests` / `min_tests`. These are
  request-scoped evidence assertions: they are persisted with local and Runner
  Job metadata so the same invocation is evaluated consistently after handoff
  or restart, but they are deliberately excluded from the structured validation
  target identity and do not become durable Workflow Session requirements. If
  the validator exits successfully while the requested count is below the
  minimum, or bounded output cannot prove the count, the immutable raw ToolCall
  failure remains in history while validation projects the execution separately
  as an evidence gap rather than a code/test correctness failure. A later
  sufficient validation of the same execution target therefore need not inherit
  the earlier assertion threshold. Omitted assertions preserve zero-test
  execution compatibility.
- **Opaque scope identity:** `comparison.scope_identity` is a domain-separated,
  opaque stable identity (`validation_scope:v1:<sha256>`) over the normalized
  *structured* scope. It never returns a raw command, absolute path, or test
  filter — command text is not re-exposed through another field.
- **Jobs report only proven status:** the `attempt.jobs` block reports counts
  computed over the full bounded active Job aggregate, never the truncated
  `recent` list, so a hidden recovering job is never misreported as healthy.
  Fields that cannot be reliably proven are not reported.
- **No new persistence model:** continuation feedback introduces no new
  durable table and no second attempt state machine. Exploration adds only a
  serde-defaulted field to the existing version-1 event ledger, so older
  ledgers restore it as empty without a version bump; feedback remains a
  projection over that existing state.

### Optional external observations

`record_external_observation` and `list_external_observations` expose bounded,
explicitly authorized external reports for one exact Project and Workflow Session.
They do not append synthetic native execution/validation facts, consume the native
Session event tail, mutate Goal state, or derive a Session from a window or local
directory. The report's adapter/event IDs provide scoped replay correlation, not
authentication or execution proof. Unknown outcomes remain unknown. The first
adapter has no durable source sequence, so list results explicitly report incomplete
coverage and must not be interpreted as complete capture or source execution order.
See [`../../integrations/codex/README.md`](../../integrations/codex/README.md) for
the optional adapter, capacity/recovery contract and unverified Host boundaries.
The authorized `session_handoff_summary` handoff brief now includes a bounded
`external_observations` section, separate from native progress and validation.
It shows the last five retained reports in server timestamp and identity order,
with exact adapter/event IDs, tool,
reported status, and server receipt time, plus total/returned/truncated and
unknown counts. The section's `provenance` is always `external_report` and its
`coverage.complete` is always false: receipt ordering cannot prove source
execution ordering or complete capture. No Session Project, unavailable store,
or failed read produces `status=unavailable` with null observations and counts,
never an apparent empty result. The exact Project and Session association comes
from the surrounding handoff output and `handoff_brief.session.session_id`.

### Task handoff brief (`handoff_brief`)

`session_handoff_summary` and `finish_coding_task` return the same version-1
`handoff_brief`, built by one shared pure projection. It is the compact,
model-friendly view for a new window, a new Agent, or a human receiver;
`continuation_feedback` remains the more detailed evidence available from the
explicit diagnostic handoff view and closeout tools. The
brief is not Session replay, does not reconstruct chat or hidden model
context, and does not decide that implementation work is complete.

The builder consumes only the bounded Session summary, continuation feedback,
workspace, validation, Job, guidance, exploration, external-report, and suggested-action
snapshots that its caller already obtained. It performs no shell, Git, file,
search, LSP, Agent, or Runner request; does not refresh activity, consume
guidance, append a ledger event, or call an LLM; and stores no new Session
data. `work_on_project` intentionally does not return `handoff_brief`, so the
standard startup core's worst-case size does not grow.

A direct internal `session_handoff_summary(...)` call does not add business
events beyond those snapshots. Calls through MCP, REST, or runtime dispatch
remain subject to the uniform recorder and normally append exactly
`tool_call_started` and `tool_call_finished`. This telemetry is not guidance
consumption or a handoff-builder mutation, and `session_handoff_summary` must
not receive a recorder bypass.

The projection has these stable bounds and semantics:

- root and latest task instruction excerpts reuse the existing Session
  credential redaction (including token, Bearer, and client-secret styles) and
  are capped at 600 Unicode characters. Unix,
  Windows-drive, UNC, and `file://` locations; shell commands and parameters;
  fenced and inline code; and ordinary task prose remain available as useful
  handoff context. The latest excerpt is the latest retained
  `task_instruction`; when it equals the root, the same excerpt is returned.
  It is null when no such retained event exists. Its `truncated` flag is true
  only when credential redaction, the 600-character bound, or final
  byte-budget reduction changed the returned excerpt.
- changed paths reuse continuation changes (12 maximum); recent files reuse
  the attempt exploration workset (8 maximum, deduplicated newest-first);
  unresolved failure identities are capped at 5; deterministic next actions
  are capped at 5. Each bounded evidence list preserves
  `total`/`returned`/`truncated`. Recent files are only continuity hints, not
  complete history.
- external reports are an independent read-only claim section capped at five
  identities. A byte-budget reduction updates `returned` and `truncated` as it
  removes reports; `unknown_count` still counts all retained reports. Store
  unavailability is local to this section and does not change native closeout.
- `progress.state` is selected in order: a non-mutable lifecycle is `closed`;
  a workspace conflict, blocking/recovering Job, unresolved validation
  failure, or open risk is `blocked`; workspace changes without a proven
  latest validation pass are `needs_validation`; missing critical evidence is
  `insufficient_evidence`; otherwise an active Session is
  `ready_to_continue`. A dirty worktree alone is not a blocker, questions and
  todos are counts rather than blockers, and terminal-pending Jobs are
  nonblocking.
- workspace is `available`, `not_requested`, or `unavailable`.
  `include_workspace=false` never causes an implicit Git query. Validation is
  `passed`, `failed`, `not_run`, `not_requested`, or `unavailable`;
  `include_validation=false` never masquerades as `not_run`.
- `basis.complete` is false whenever a sorted fixed `reason_codes` entry
  identifies omitted, unavailable, or raced recovery evidence, including an evicted
  attempt boundary or an external-report change during snapshot assembly. Internal
  error text is never a reason code.

The complete object is checked against its actual serialized JSON size and
hard-capped at 8192 bytes. Stable reduction removes recent files, changed
paths, failure identities, next actions, and then instruction excerpt
characters while retaining lifecycle/mode, progress and validation status,
attention counts, basis, and deterministic/LLM flags.

A new window can start a new Session normally and then explicitly read the old
Session with `session_handoff_summary(session_id=...)`. Explicit
`resume_session_id` remains available when the caller truly intends to resume
the same active Session and continues to obey the existing identity,
lifecycle, project, guard, and binding rules.

For deliberate coordinator/worker delegation across separate windows, keep the
Sessions independent and use the existing handoff plus message-board primitives;
see [Manual Multi-Window Collaboration](manual-window-collaboration.md).

---

## 2. Action Audit Session

### Purpose

**HTTP Action call auditing** and operator-facing grouping of external API
requests. It answers “what HTTP/API actions happened in this audit window?” —
not “what is the coding task ledger for this repo work?”.

### Responsibilities

- Group HTTP Action / REST audit events under one audit session
- Persist action audit records (endpoints, status, durations, redacted summaries)
- Snapshot non-secret caller attribution at event write time: canonical credential
  kind, optional stable user id, and OAuth-only client id
- Idle open-session reuse and explicit close for operator audit views
- Aggregate stats for read-only audit APIs, with explicit bounded-scan coverage

### Generic model-ergonomics telemetry

Model-visible **runtime** tool calls reuse the existing Action Audit event as the
durable/queryable sink for low-cardinality ergonomics telemetry. No second
telemetry table or recorder is created. The shared ToolRuntime kernel owns the
normal timer for a registered model-visible tool; the transport that already owns
the outer Action Audit row then finalizes one `summary.model_ergonomics` object
from the final model-facing ToolResult projection. A transport may use a bounded
fallback timer only after a runtime tool identity is established when MCP-only
validation rejects the call before kernel entry or the MCP hard dispatch timeout
prevents kernel completion. Batch items do not create generic invocation records,
and hidden/internal helpers do not start this telemetry.

The current durable generic record uses `schema_version = 10`. Older telemetry rows
remain naturally queryable and are not migrated or backfilled. The current
record contains:

- base generic fields: `tool_name`, `tool_category`, `success`, `duration_ms`,
  nullable `serialized_result_bytes`, nullable structured `error_kind`,
  `failure_kind`, `recovery_kind`, and authoritative closed `execution_state`;
- optional `finish_summary_only` and bounded `work_on_project` request-shape facts;
- optional edit measurement fields: `edit_surface`, `edit_outcome`, and
  `edit_conflict_kind`.
- optional bounded Job convergence counts and salted exact-relation events;
  see [Server-only convergence measurement](job-reliability-and-concurrency.md#server-only-convergence-measurement).
  Exact serial predecessor links remain outer Action Audit metadata; they do
  not infer model turns or supply Workflow Session authority.

Context-ACK presence/status and recovery-delta byte/event metrics are retired.
Generic final-result byte size and latency remain available without retaining
revision values or recovery bodies.

`edit_surface` is the closed label `canonical | advanced`. `edit_outcome` is a
bounded structured classification derived from authoritative edit result fields.
`edit_conflict_kind` is limited to the closed allowlist `multiple_matches`,
`match_not_found`, `occurrence_out_of_range`, `overlapping_edits`, and
`sha256_mismatch`; unknown values are omitted and arbitrary error prose is never
promoted into the durable record.

`serialized_result_bytes` is the UTF-8 byte length of the exact final
model-facing ToolResult JSON object (`success`, `output`, and `error` only when
present), serialized with the normal serde JSON representation. It is not a
character count, Rust memory size, stdout/stderr estimate, database-row size, or
HTTP/JSON-RPC/MCP framing size. MCP finalizes the count from the final
`structuredContent` ToolResult after MCP-only image/resource framing, so bytes
removed from the model-facing ToolResult are not charged. If a registered
model-visible tool identity has already been established but a deterministic
pre-result rejection occurs (for example invalid arguments, insufficient scope,
or MCP wrapper validation), the invocation is still counted with
`serialized_result_bytes = null`; canonical edit tools classify those definite
rejections as `edit_outcome = rejected`.

The MCP outer hard dispatch timeout is different: dispatch may already have
started and the terminal edit result is unavailable. It is recorded with
`error_kind = dispatch_hard_timeout`, and a canonical edit remains
`edit_outcome = uncertain` rather than being interpreted as a definite rejection.
No retry authority, state-change claim, or rollback claim is derived from that
measurement. There is currently no authoritative generic final-result truncation
fact, so generic `result_truncated` is deliberately **not** recorded;
tool-specific truncation fields keep their existing meanings.

Classification consumes structured ToolResult fields only. It never derives a
kind from arbitrary English error prose. The generic record stores no tool
arguments, commands/argv/scripts/stdin/stdout/stderr, ToolResult body or error
prose, paths/cwd/project ids, query/file/clipboard/Computer contents, Session
message/prompt/answer bodies, credentials, native identities, or arbitrary user
text. Existing Action Audit correlation/attribution fields remain separate
pre-existing audit data; P1a does not copy them into `model_ergonomics`.

`edit_tool_telemetry` remains the edit-specific structured tracing enrichment.
An edit invocation therefore has one generic Action Audit invocation record plus
its existing edit-specific enrichment, not two generic counts. Workflow Session
`tool_call_started` / `tool_call_finished` events remain a separate workflow
ledger and are not the persistence source for this aggregate ergonomics data.

Telemetry is observation-only and failure-isolated. Failure to serialize the
bounded generic projection or to persist the Action Audit row is dropped/warned
without changing the tool's success, output, error, permission, execution state,
retry safety, Job lifecycle, or Computer authority.

#### Real MCP `tools/list` surface telemetry

The real MCP protocol method `tools/list` has a separate durable measurement;
it is not stored under `summary.model_ergonomics` and must not be confused with
the runtime `list_tools` tool. Each executed JSON-RPC request with an `id`
creates one metadata-only ActionAudit row:

```text
endpoint    = /mcp
action_name = toolsList
operation   = mcp_tools_list
```

A JSON-RPC notification has no executed ActionAudit row. On success the durable
summary shape is:

```json
{
  "transport": "mcp",
  "tool_surface": {
    "schema_version": 1,
    "protocol_era": "legacy | stateless_2026",
    "compact_schemas": false,
    "tool_count": 0,
    "serialized_tools_bytes": 0,
    "serialized_result_bytes": 0,
    "gateway_tool_included": false
  }
}
```

`tool_count` comes from the final caller-visible `tools` array.
`serialized_tools_bytes` is the exact JSON UTF-8 byte length of final
`result["tools"]`; `serialized_result_bytes` is the exact JSON UTF-8 byte length
of the final MCP result object, after stateless result metadata has been added
when applicable but excluding the JSON-RPC envelope. `gateway_tool_included`
reflects the actual final response rather than theoretical authorization. The
durable summary never stores the tool-schema body, tool-name array,
descriptions, arguments, or scopes. ActionAudit persistence failure remains
non-blocking and cannot change the MCP protocol result.

For dogfood analysis, read bounded Action Audit events through
`/api/audit/session` or query SQLite `action_events.summary_json`, select rows
with `summary.model_ergonomics`, and aggregate by `tool_name`. The raw bounded
records are sufficient for invocation/success counts, duration p50/p95,
serialized-result mean/high percentiles, and structured error/recovery-kind
distributions in later SQL/Python analysis; no dashboard or analytics service is
part of this contract.

### Identity

| Aspect | Contract |
|---|---|
| ID form | UUID string (or client-supplied id via headers/query), **not** `wc_sess_*` |
| Request affinity | Headers `x-action-session-id` / `x-webcodex-session-id`, or query `action_session_id` |
| Default creation | Server may create a new UUID when no open recent session is reused |
| Durable caller attribution | `principal_kind`, optional `principal_user_id`, OAuth-only `oauth_client_id`; legacy rows remain `NULL` and are never inferred from target project or session |
| Stats exposure | `/api/audit/stats` aggregates credential kinds and OAuth client usage; ordinary `/api/audit/session` event views do not expose principal/user/client attribution fields |

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
3. Events are written to SQLite; session aggregate counters update. Caller
   attribution is snapshotted from the authenticated request context before the
   write and never reconstructed later from execution targets.
4. Operator APIs list sessions, fetch one session with events, or compute stats.
   Stats report scanned/available event coverage and fail on database read errors
   instead of silently presenting a partial aggregate as complete.
5. Sessions may be closed (`status = closed`); idle open sessions time out for
   reuse purposes (`ACTION_SESSION_IDLE_TIMEOUT_SECS`).

This lifecycle is **orthogonal** to Workflow Session start/finish tools.

### What it is not

- Not a coding / workflow session
- Not a substitute for canonical coding-startup / `work_on_project` evidence
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

The standing optional-correlation contract is:

Until that design is implemented, code must treat the systems as unlinked.

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
- Audit event views (without principal attribution fields) and stats aggregates,
  including coverage plus credential-kind/OAuth-client usage summaries
- Workflow tool fields: `session_id`, `recording_session_id`, session mode
  values `normal` / `read_only`. A pre-0.4 persisted v2 row containing
  `mode = "inspect"` is malformed under the current enum and is discarded
  row-closed without reinterpreting it as `normal`.
- Error kinds such as `unknown_session_id`

### OpenAPI / MCP / runtime tool surface

- GPT Action OpenAPI operation ids and schemas that mention workflow
  `session_id` / `recording_session_id`
- MCP tool input schemas for session tools
- Runtime tool names (`start_session`, `work_on_project`,
  `session_summary`, …); retired wire names are not part of this external surface

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

---

## Related docs

- [`AGENTS.md`](../../AGENTS.md) — executable Session invariants
- [`architecture-decisions.md`](architecture-decisions.md) — dual-model summary
- [`openapi-guidelines.md`](openapi-guidelines.md) — `session_id` vs
  `recording_session_id` on GPT Actions
- [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — module map and Workflow Session
  overview
