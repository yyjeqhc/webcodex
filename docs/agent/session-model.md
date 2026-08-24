# Session Model — Two Non-Interchangeable Concepts

WebCodex uses the word **session** for two independent systems. They share
casual vocabulary only. They must not be merged, cross-wired, or inferred from
each other.

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

## Project Connector continuity is not a third session type

The ordinary project-bound product path uses existing durable Connector Tasks
and task events. Connector continuity is adapter-specific; it is never inferred
merely because two requests come from the same credential, connection, project,
or apparent chat.

An adapter/protocol that explicitly supplies a stable `ClientWindow` may use a
lightweight SQLite map from the domain-separated hashed window identity,
authenticated subject, exact Connector project, and canonical-root hash to one
current durable task. That mapping does not create another event ledger and must
never be cross-wired to either session system below. On such a stateful adapter,
`task_start` may continue the exact active mapping, repository switches remain
isolated, write upgrades recheck project-write authority, and a terminal task
advances only that exact mapping while preserving history.

**Stateless MCP 2026 deliberately supplies no stable `ClientWindow`.** Every
`task_start` therefore starts independent durable work; even a caller-supplied
legacy `Mcp-Session-Id` must not create hidden continuity. Existing work is
continued explicitly with its durable `task_id` through `task_resume` (and may be
discovered with `task_list`). This stateless path never falls back to a user,
credential, project identity, connection, or prior request.

Legacy/stateful MCP and first-party/hosted HTTP adapters may have their own
explicit window sources, such as the older server-minted MCP session header, a
conversation-scoped request header, or a first-party HttpOnly window cookie.
Those are adapter-local `ClientWindow` inputs, not a general property of HTTP or
MCP and never proof of Workflow Session identity, model-context retention, or
authority. Raw window values are not stored; only their domain-separated hash is
used where that adapter contract permits window binding.

Restart recovery follows the same boundary: durable task history always
survives; only adapters with an explicit stable window may restore an exact
window/repository mapping automatically. Stateless callers recover explicitly by
`task_id`. `task_resume` may rebind only when the current adapter actually
supplies a new stable `ClientWindow`; otherwise the durable task resumes without
manufacturing one.

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
| Coding resume field | `resume_session_id` on `start_coding_task`; distinct from ordinary project-tool `session_id` |
| Recorder field | `recording_session_id` on generic wrappers, including the stateless MCP 2026 tool-argument projection (metadata only; stripped before concrete tool dispatch) |

### Storage and ownership

| Aspect | Contract |
|---|---|
| Module | `tool_runtime::sessions` (model, store, events, JSON persistence) |
| Primary store | In-memory session store |
| Durability | JSON-oriented session ledger (bounded events/messages per session) |
| Current-session binding | In-memory exact-key cache plus a bounded durable projection in the same JSON ledger; isolated by client window, principal, transport, resolved project, and canonical repository-root hash |

Stateless MCP 2026 does not have a reliable Workflow Session or ChatGPT-window transport identity. Its `tools/list` schema therefore projects `recording_session_id` as explicit wrapper metadata for runtime tools. A call may carry `recording_session_id=W` while the concrete tool body carries business `session_id=C`; the MCP adapter removes the recorder field before concrete parsing and the kernel independently authorizes `W` before it can record evidence or supply trusted collaboration provenance. This does not revive legacy `mcp-session-id`, grant target authority, or infer a recorder from credentials, project identity, or connection state.

Stateless MCP 2026 also projects optional `ack_session_message_ids` wrapper metadata, bounded to eight opaque `wc_msg_*` ids. An ACK is request-scoped evidence that the current model context still remembers an unresolved message in the exact authorized recording Workflow Session. The adapter removes ACK metadata before concrete tool parsing; it never grants authority, resolves a message, or gates the concrete tool effect. In the first version only open high-priority Guidance can require ACK. Accepted ids suppress that Guidance body only in the current response; if a later request omits the id, the unresolved Guidance is eligible for bounded redelivery again. Historical ACK state is never used to infer current model-context retention.

A required Guidance message may persist `first_ack_observed_at` for observability. Only the first accepted ACK advances message-observation revision; repeated echoes do not create revision churn. This field means only that the Server once observed an explicit ACK echo. It is not a delivery/read receipt and does not change `status=open`. `resolve_session_message` remains the durable processed-state transition; resolved messages no longer participate in hints or urgent redelivery.

### Model-facing context continuity revision

Each Workflow Session also keeps a durable monotonic `context_revision` for finished model-facing tool results. Allocation is Session-local and atomic with annotating the finished event, so concurrent model-facing results receive unique revisions; generic background/system/Job bookkeeping does not advance this watermark. Retention may evict older annotated events without decreasing the durable high-water, and a capable caller whose ACK predates retained history receives `history_lost=true` rather than invented history.

The context ACK protocol is request-scoped and surface-capability-scoped. Stateless MCP 2026 Full Operator `tools/call` explicitly supports `ack_session_context_revision`; the adapter marks that request internally even when the ACK is omitted or malformed. Exact ACKs return only the newly allocated `session_context_revision`; missing, stale, malformed, or future ACKs never block the original tool effect and may add bounded `session_continuity` / `session_recovery`. If another model-facing call completes after request admission but before this result, that intervening result is included in recovery before the newer revision is exposed. When exact recovery events exceed the response cap, `current_handoff` supplies bounded current-state recovery so the newest revision never silently skips omitted consequences. The ACK is evidence about the caller's model view only: it grants no authority, is not a delivery/read receipt, and is not persisted as caller state.

Surfaces that do not expose this ACK protocol (including legacy MCP, generic REST/GPT Actions/OpenAPI, and Canonical Connector) still contribute their recorded model-facing results to the same durable `context_revision` history, so a later capable Stateless caller can recover intervening consequential work. Those non-capable responses preserve their existing contract and do not expose `session_context_revision`, `session_continuity`, or `session_recovery`; absence of an impossible ACK is not interpreted as model-context loss. Capability is supplied explicitly by the adapter, never inferred from host identity, elapsed time, transport heuristics, tool purpose, credentials, or Session age.

This watermark is independent of `ack_session_message_ids` and the message-observation revision: message ACKs mean only that specific unresolved guidance is still remembered for one request, while context ACKs describe the retained model-facing result watermark. Neither implicitly acknowledges or resolves the other.

### Message observation state

The Session-local message board has a separate durable monotonic **message-observation revision** used by `observe_session_messages`. The public observation token is bounded and opaque; it binds the exact Workflow Session plus durable cursor state without exposing the internal revision as the caller cursor. It is observation state only and grants no authority. Malformed, oversized, wrong-Session, and future-revision tokens fail closed.

A no-token call establishes the current baseline and returns no historical messages. Later token calls return retained messages whose latest observable state changed after that cursor, optionally with one bounded wait. Posts are observable mutations; resolve advances only on a real field/status change; a new `complete_session_message` observes both the todo resolution and answer creation; exact idempotent replay does not advance. This is current-state delta, not an audit/event log, so multiple changes to one retained message may collapse to its final state.

Retention correctness does not infer continuity from deque length or position. Retained messages keep their latest internal observation revisions and the Session persists a low-watermark for removed observation history, including non-FIFO completion retention holes. `history_lost=true` tells callers when a cursor predates state that can no longer be reconstructed. With pagination, a token advances only through the last returned change while `has_more=true`. Observation-token issuance fences the ledger generation containing the cursor revision, so tokens issued by the current implementation remain valid across Server restart when the Session itself restores.

Waiting uses process-local notification only as a wake signal; durable revision state remains the truth. No Session-store or persistence-writer mutex is held across the bounded await, unrelated Session mutation can only cause a spurious recheck, and timeout is a successful unchanged result rather than a tool failure.

Message observation is not a delivery receipt, not model-context retention, not a subscription/stream, and not an orchestrator wake-up. Room/Discussion, Participant, presence, typing, scheduler/worker-pool, automatic worker spawning, and routing remain future additive capabilities rather than reinterpretations of this cursor.

Authenticated project-scoped Workflow Sessions created by current code also persist an internal canonical authority-group fingerprint. A legacy project-scoped record from before that fence may legitimately have no fingerprint; current project authorization or knowledge of its `session_id` is not enough to claim it. The only automatic compatibility upgrade is a coding continuation/resume whose current caller can reconstruct an exact historical `CurrentSessionKey` and whose pre-existing **durable** binding already points to that exact active Session for the same resolved project. The proof ignores the process-local cache. After all continuation validation succeeds, the canonical fingerprint is written atomically with the instruction/capability/context/binding mutation and enters the same persistence generation. Without that durable proof the legacy Session remains fail-closed.

The historical `CurrentSessionKey` and durable binding hash format are unchanged, including their principal-presentation, transport, stable-window, project, and canonical-root components. Consequently a legacy binding created through one presentation (for example direct shared-key) is not rewritten to make a first migration attempt through another presentation match. Once migration succeeds, subsequent Session authorization uses the canonical authority-group fingerprint, so direct shared-key and its OAuth shared-key bridge share the migrated Session normally.

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

`start_session` can set the initial context. `start_coding_task` also sets it
when creating a Session; on automatic continuation or explicit resume,
omitting `execution_context` preserves the stored value, while an explicit
object commits with the instruction/capability/binding update under the
in-memory store lock. An explicit `{}` clears all execution defaults.
`update_session_context` requires a project input that the caller may access and
that resolves exactly to the explicit known active Session project. It never
uses current-session fallback, never creates an unknown Session, rejects closed,
archived, project-less, unauthorized, and mismatched Sessions, and has no
cross-project escape. The context and event commit together in memory; the JSON
ledger is then queued to the existing background writer. Persistence failures
are reported through existing status and logs, and success does not mean a
synchronous disk flush or promise rollback on a later writer failure.

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
channel. The Runner may reuse the authenticated transport for the same
Session/resource pair, but it does not preserve `cd`, exports, aliases,
functions, umask, or shell-process state between commands.

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

### Explicit persistent shell

The full operator runtime has a separate, command-oriented `PersistentShell`
model:

```text
open_session_shell
session_shell_exec
session_shell_status
close_session_shell
```

Opening creates one real long-lived local shell process, at most one active
shell per Workflow Session: `sh`/`bash` on Unix, or the configured PowerShell
program/profile on Windows. For an `agent:<client>:<project>` the Runner owns
and controls the shell. Without `execution_context.resource`, it runs against
the registered project host. With a named resource, the Runner opens a remote
persistent shell through that SSH resource; this requires the Runner's SSH and
SSH-persistent-shell capabilities and never silently falls back locally. Windows
Stage 1 does not advertise SSH persistent shells. The
process manager also has a Server-owned executor branch for a hosting surface
that supplies a Server-local project, although the current built-in public
project registry advertises Agent projects only. `inspect` and `read_only`
Sessions cannot open or execute a persistent shell.

Local open resolution is explicit `cwd`/`shell`, then the exact Session's
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

`start_coding_task` is the advanced/direct start-or-continue aggregate retained
for full-runtime compatibility. It is not an ordinary model-discovered MCP or
GPT Actions bootstrap: model-facing discovery and generic Action flattened fields
use `work_on_project` as the canonical coding entry. The advanced schema remains
available only to explicit direct/API compatibility callers. With a stable
transport window, `start_coding_task`'s default behavior is:

`work_on_project` deliberately does not use Workflow Session identity, transport
identity, a client-window key, credentials, project identity, or Server lifetime
as evidence that the current model still retains static bootstrap content. The
same `wc_sess_*` may be explicitly resumed by multiple independent ChatGPT
conversations. Its `include_workflow_guidance` and
`include_project_instructions` flags are caller-explicit model-facing projection
preferences only: their defaults are true, and false is appropriate only when
the caller's current model context already retains the corresponding content.
Repository instruction files are still re-observed and Session metadata/delta
status still update when instruction bodies are suppressed.

- no valid binding creates and binds one active Workflow Session;
- an exact repository binding reuses that Session and appends the accepted
  instruction as a `task_instruction` event;
- switching repositories preserves independent bindings, and switching back
  restores the earlier Session;
- `inspect`/`read_only` to `normal` rechecks project-write scope and changes
  mode/guards without changing Session identity;
- `new_session=true` explicitly creates an isolated Session without closing or
  rewriting the previous one.

Startup selection is strict and ordered:

1. `resume_session_id` resumes only that existing Workflow Session. A malformed
   or unknown id, non-active lifecycle, project mismatch, access denial, or
   unsafe capability change fails without consulting a current binding or
   creating a replacement.
2. Without an explicit resume, `new_session=true` creates an isolated Session.
   The two fields are mutually exclusive.
3. With neither field, the stable-window exact binding keeps the default
   get-or-create/continue behavior above.

Explicit resume preserves both the `wc_sess_*` id and root title, then appends
exactly one new `task_instruction`. With a stable window and
`bind_current=true`, it atomically replaces that exact window/repository
binding; a previously bound Session remains active and available by explicit
id. `bind_current=false` leaves both binding layers unchanged. Without a stable
window identity, explicit resume still succeeds but creates neither a
process-local nor durable binding. The result reports that state, and later
project tools must continue to pass the resumed `session_id` explicitly.

The first instruction remains the root title. Later instructions record their
timestamp, requested mode/guards, capability-change result, context-refresh
fact, explicit-resume fact, and whether the current binding was established,
without overwriting that root. Target validation, Session create/update,
instruction event, process-local cache replacement, and durable binding
replacement are decided under one store lock and enter one ledger snapshot
generation; a failed validation, permission check, or injected pre-commit
failure leaves them unchanged. Retrying after such a failure appends the
instruction only once. A title change is never an isolation signal.

The durable projection stores only:

```text
domain-separated SHA-256(exact CurrentSessionKey)
→ wc_sess_* session_id
→ updated_at
```

The canonical hash input is the principal kind/id, transport, already-hashed
stable window identity, resolved project, and already-hashed canonical
repository root. It uses fixed field order and length prefixes under
`webcodex.workflow-current-binding.v1`. Raw MCP session ids, hosted
conversation ids, cookies, credentials, authorization headers, and repository
paths never enter this projection, and neither binding hashes nor component
hashes are returned to the model.

After restart, a request with the same complete exact key may restore only an
existing active Workflow Session whose project still matches, repopulate the
process-local cache, and continue it. Missing stable window identity never
falls back to a principal-, credential-, project-, or repository-wide binding.
Changed principal, transport, window, resolved project, or canonical root
derives a different durable key. `new_session=true` atomically repoints only
that exact binding and preserves the previous Session for explicit-id access.
Explicit bind and unbind update both layers; close and LRU eviction remove all
bindings to the affected Session.

When the complete exact key cannot be formed, `resume_session_id` is the
intentional recovery path. It validates the Session against the currently
resolved project and derives any binding key from that project's current
canonical repository root, never a client-supplied path. It never derives a
Workflow Session from a Connector Task or Action Audit Session. Missing window
identity does not manufacture a key, create a credential-wide projection, or
write a durable binding.

The binding field is an additive, serde-defaulted field in ledger version 1, so
older ledgers load it as empty without migration and keep their existing
Session events/messages. Restore accepts only bounded, lowercase SHA-256 keys
that reference known active `wc_sess_*` records. Malformed, duplicate,
conflicting, missing, closed, project-mismatched-on-lookup, and excess entries
are discarded without rejecting valid Session data. Internal status exposes
only bounded counts (`durable_binding_count`, `restored_binding_count`,
`discarded_binding_count`) and never a binding key.

This remains intentionally separate storage and state from the Connector's
SQLite window/project/Task map, while presenting the same ordinary
window/repository continuity semantics. Connector Task continuation and resume
remain their own model and do not infer or mutate Workflow Sessions.

### Current lifecycle contract

- New Workflow Sessions are `active`; older ledgers without a lifecycle field
  are also read as `active`.
- `close_session` is the only explicit `active → closed` transition. It requires
  an explicit `session_id`, never uses current-session fallback, and returns
  `unknown_session_id` for malformed or unknown IDs without creating a session.
- Re-closing a closed session is idempotent. Only a real transition records one
  `session_closed` event.
- Closing removes process-local and durable current bindings that point at the
  Session; it does not delete the Workflow Session ledger.
- Closed sessions still allow queries and pure reads. They reject write-like
  tools, shell/job tools, session-message mutation, and session-scoped
  checkpoint create/restore/delete with `session_closed`.
- `finish_coding_task`, `session_handoff_summary`, and other summary/query tools
  produce closeout information but do not close the session.
- `archived` is a reserved wire state that current code does not produce. LRU
  eviction is capacity management, not a lifecycle transition, and removes
  bindings to the evicted Session.
- Session modes (`normal`, `inspect`, `read_only`) are execution policy, not
  lifecycle state.

This is **not** the same state machine as Action Audit Sessions. Lifecycle
tools and error kinds (`unknown_session_id`, `session_closed`, mode denials,
guard failures) apply only to Workflow Sessions.

### Continuation feedback (`continuation_feedback`)

`continuation_feedback` is a **deterministic, read-only projection** surfaced
by `start_coding_task`, `finish_coding_task`, `session_handoff_summary`, and
(as `validation_delta`) `validation_summary`. It is derived only from existing
persistent state — the Workflow Session ledger, validation evidence, bounded
Job metadata, and the session message board — and it is never a substitute for
a `finish_coding_task` verdict.

- **Read-only projection contract:** building it directly never executes shell,
  reads project files, enqueues Agent/Runner requests, mutates the ledger,
  refreshes activity, consumes or auto-resolves guidance, or calls an LLM.
  `start_coding_task` still appends its legitimate new `task_instruction`.
  Public MCP, REST, or runtime dispatch also records the enclosing tool's
  uniform `tool_call_started` / `tool_call_finished` telemetry, so
  `events_total`, `updated_at`, and activity telemetry are not required to stay
  unchanged across a public call. Those recorder facts are separate from the
  projection's business semantics.
- **Startup describes the previous attempt:** for reused, explicitly resumed,
  and restored-after-restart sessions, `start_coding_task` snapshots the
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
  structured evidence from focused `read_file`, `read_files`,
  `search_project_text`, `search_project_texts`, and
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
  resume, inspect/read-only to normal mode upgrades, and ledger restoration
  reuse the prior attempt's workset. Startup returns at most 3 paths in
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
  bounded minimum count. The effective minimum and the structured validation
  target identity are persisted in local and Runner Job metadata, so terminal
  projection and reconciliation apply the same contract after handoff or
  restart. An exit-zero Job remains raw lifecycle `completed`, but its
  validation projection and Session event are failed when the known count is
  below the minimum or when bounded output cannot prove a complete count.
  Omitted assertions preserve zero-test execution compatibility.
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

### Task handoff brief (`handoff_brief`)

`session_handoff_summary` and `finish_coding_task` return the same version-1
`handoff_brief`, built by one shared pure projection. It is the compact,
model-friendly view for a new window, a new Agent, or a human receiver;
`continuation_feedback` remains the more detailed diagnostic evidence. The
brief is not Session replay, does not reconstruct chat or hidden model
context, and does not decide that implementation work is complete.

The builder consumes only the bounded Session summary, continuation feedback,
workspace, validation, Job, guidance, exploration, and suggested-action
snapshots that its caller already obtained. It performs no shell, Git, file,
search, LSP, Agent, or Runner request; does not refresh activity, consume
guidance, append a ledger event, or call an LLM; and stores no new Session
data. `start_coding_task` intentionally does not return `handoff_brief`, so the
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
  identifies omitted or unavailable evidence, including an evicted attempt
  boundary. Internal error text is never a reason code.

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

### Invariants (must)

These are also summarized in `AGENTS.md` §7, **Sessions**:

1. **ID format:** Workflow Session IDs use `wc_sess_*`. Do not change the
   prefix, ledger event schema, or lifecycle semantics without an explicit
   design task.
2. **Explicit wins:** An explicit `session_id` always wins over current session.
3. **Unknown rejects:** Unknown explicit `session_id` → `unknown_session_id`.
   Never silently fall back to current session.
4. **Mode guards:** `inspect` denies structured write-like tools and permits
   shell/job-like tools only through the fail-closed Landlock inspect sandbox;
   `read_only` denies both write-like and shell/job-like tools.
5. **Guards first:** Guard denial happens before mutation or agent enqueue;
   record a failed session event when the session id is valid.
6. **Business vs recorder:** `session_summary` (and similar) required
   `session_id` is business input; do not replace it with current session or
   with `recording_session_id`.
7. **No inference from HTTP audit:** Never derive a `wc_sess_*` id from an
   Action Audit Session id (or from `x-action-session-id` / audit SQLite rows).
8. **Window isolation:** Current-session lookup and mutation require a stable
   transport window identity. Missing identity skips generic fallback and
   fails explicit current-binding operations; it never falls back to a
   credential-wide binding.

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

The version-1 generic record contains only:

- `schema_version = 1`;
- registry-owned `tool_name` and closed/bounded `tool_category`;
- `success` and non-negative `duration_ms` for the outer invocation up to its
  returned ToolResult/Job handoff;
- nullable `serialized_result_bytes`;
- nullable structured `error_kind`, `failure_kind`, `recovery_kind`, and
  authoritative closed `execution_state` when present.

`serialized_result_bytes` is the UTF-8 byte length of the exact final
model-facing ToolResult JSON object (`success`, `output`, and `error` only when
present), serialized with the normal serde JSON representation. It is not a
character count, Rust memory size, stdout/stderr estimate, database-row size, or
HTTP/JSON-RPC/MCP framing size. MCP finalizes the count from the final
`structuredContent` ToolResult after MCP-only image/resource framing, so bytes
removed from the model-facing ToolResult are not charged. If a registered
model-visible tool identity has already been established but the call is rejected
before any ToolResult exists (for example invalid arguments, insufficient scope,
or MCP wrapper validation), the invocation is still counted but
`serialized_result_bytes` is `null`; the transport-specific error envelope is
never substituted for a ToolResult. The MCP hard dispatch timeout is recorded the
same way with structured `error_kind = dispatch_hard_timeout` so stalled calls do
not disappear from failure/latency aggregates.
There is currently no
authoritative generic final-result truncation fact, so generic
`result_truncated` is deliberately **not** recorded; tool-specific truncation
fields keep their existing meanings.

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

The standing optional-correlation contract is:

- **Direction:** Action Audit side holds optional `workflow_session_id`
  (`wc_sess_*`); prefer **event/record** level first.
- **Optional & explicit:** absence is normal; no inference from current Action
  Audit Session, time, thread, connection, or current-session bindings.
- **Independent lifecycle:** audit must not create/close/transition Workflow
  Sessions; correlation is not ownership.
- **Validation sketch:** missing → unlinked; malformed → parameter error;
  well-formed but unknown → store without create or fallback.
- **Named migration** required before SQLite / OpenAPI / external JSON change.

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
  values such as `normal` / `inspect` / `read_only`
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
| Need a link later? | Optional explicit `workflow_session_id`; never infer it from transport or current bindings |

---

## Related docs

- [`AGENTS.md`](../../AGENTS.md) — executable Session invariants
- [`architecture-decisions.md`](architecture-decisions.md) — dual-model summary
- [`openapi-guidelines.md`](openapi-guidelines.md) — `session_id` vs
  `recording_session_id` on GPT Actions
- [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — module map and Workflow Session
  overview
