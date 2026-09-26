# Job Reliability, Observation, and Runner Concurrency

This note defines the current V1 engineering contract for Job continuation across
Control Server restarts, bounded Job observation, and shared Runner execution
capacity. It complements [Runner](../RUNNER.md), [Testing](../TESTING.md), and
the [architecture decisions](architecture-decisions.md).

The purpose is to keep implementation, operator diagnosis, and model-facing tool
descriptions aligned. It is not a new scheduler, persistence layer, or retry
framework.

## Continuation and control workflow

Retain the exact continuation returned when a long ordinary tool hands off to a
Job. Job observation never starts or replaces execution:

```text
long tool -> same durable Job -> exact continuation retained
  blocked on terminal -> wait_for_job_terminal
  independent work    -> continue read/search/review; optional jobs.attention
  need logs/details   -> observe_jobs on the retained Job
  need stop           -> stop_job(project="<project>", job_id="<job>", confirm=true)
  identity lost       -> list_jobs recovery before considering any retry
```

`wait_for_job_terminal(job_id="<job>", idempotency_key="<wait-key>")` registers
bounded one-shot terminal attention. Prefer it when progress genuinely depends
on completion and a real Host carrier is available. Check
`automatic_resume_available`; registration without a carrier does not promise
automatic model resumption. Do not poll the wait registration or repeatedly use
short `observe_jobs` waits just to keep a Job visible. Explicit `observe_jobs`
remains the fallback for details and recovery.

For MCP Apps, the canonical parser-ready `suggested_call` is projected only while
the exact wait is still `waiting/not_ready`. Use it only when no independent work remains:
present the continuation card as the final meaningful action, then yield/end the
current model turn promptly. If registration already returns terminal truth, handle
that result in the current turn instead of arming a redundant follow-up.

The current MCP App Host contract does not expose an authoritative "this model
turn is now idle/terminal" acknowledgement. The Job continuation App therefore
waits a bounded 10-second yield grace after the initial presentation tool result
before it may dispatch `ui/message`. This mitigates dispatch racing the invoking
turn; it is not a fabricated turn-generation fence. Until the Host exposes an
authoritative turn/supersession signal, the automatic message must reconcile the
terminal event against the current conversation and must not resume work superseded
by newer user instructions. This prevents stale work from being treated as current,
but it cannot prevent the extra best-effort wake itself. A successful `ui/message`
RPC proves only that the Host accepted the follow-up request, not that a fresh
model turn consumed it. Exactly-once delivery therefore still forbids blind
redispatch after an accepted or uncertain send.

A handoff failure after execution admission is `outcome_unknown`, not proof of
pre-start rejection. Recovery reuses the same canonical, atomic promotion:
caller authorization, cleanup ownership, terminal status, and an observation
token are checked under the registry lock. A nonterminal safely Public Job
retains machine-readable `job_id`, `job_status`, and the existing
`continuation` SuggestedToolCall to `observe_jobs`. A terminal hidden race is
projected through the initiating tool, not exposed as redundant active work.
If safe public visibility cannot be proved, hidden identity stays private and
the failure carries a parser-ready `suggested_call` to `list_jobs` for the exact
Project. CleanupPending cannot be adopted by observation. Neither recovery
branch dispatches a replacement or authorizes retrying the original call.

A sparse validation handoff failure remains incomplete `outcome_unknown`
evidence even without a log snapshot. The immutable Session ledger retains that
failure. Its bounded evidence view may replace the snapshot only when a known
terminal result reconciles the same Job, exact Project, Session, tool, and
validation target. A successful replacement execution with a matching target
must not erase an earlier unknown outcome.

An ordinary observation can request `context_request=["jobs.attention"]`.
This is an explicit post-tool context material, not another execution or a
business-tool argument. It reuses `active_jobs_summary` for the exact resolved
Project, requires canonical `runtime:read` independently of `project:read`,
filters caller visibility and Project before limiting recent summaries to eight,
and shares the existing context-projection byte budget. Zero active Jobs is an
available zero-count projection. Missing Project/scope or projection budget is
nonfatal to the main call. Recorder or prior Sessions never select a business
Session. The summary contains bounded counts and brief Job metadata, not logs,
command bodies, observation tokens, host details, or arbitrary payload.

Passive Job attention is a separate post-result projection on ordinary model-visible
coding tools. It requires a non-anonymous authenticated principal, a current
ClientWindow, exact resolved and currently visible Project, `runtime:read`, an
explicit business Workflow Session authorized for that Project, and a Job whose
durable Project and Session fields both match. A recorder Session, Window affinity,
or Project match alone never selects a Job. The ClientWindow scopes only the
process-local delivery cursor; it is not Job ownership, so another authorized
Window may independently observe the same changed Job state. Passive attention
reads only the Server's current Job records; unrelated read calls do not poll a
Runner. The payload is sparse: `job_id`, tool, durable status, active/terminal state,
recovery codes when present, and on terminal transitions bounded execution outcome,
exit/command truth, conservative validation/source freshness, plus an exact
`observe_jobs` details call. Successful validation stays sparse. Failed or unproven validation may include the
canonical safe `diagnostics` subset (at most three compiler diagnostics and three
`failed_test_details`, with an 8 KiB serialized detail ceiling). Item or byte
omissions set the corresponding truncation flag. Counts describe retained parser
evidence, not a complete log inventory. Exact `details -> observe_jobs` remains
available for truncated/absent evidence, full logs, and recovery. Evidence comes
from the same frozen Server record and retained bounded excerpts, never a Runner
poll. It never inlines logs, panic/assertion bodies, or command bodies. A bounded
process-local cursor keyed by principal, Window, Project, and business Session emits
decision-relevant states once per observed semantic revision. Routine queued,
agent_queued, started, running, stop-requested, progress, and timestamp changes
only advance the baseline. Terminal transitions and changes to the canonical
Server recovery phase/reason pair are deliverable; recovery completion is useful
because it resolves previously uncertain execution visibility. No status-text
heuristic classifies recovery. Server restart may repeat one bounded
active snapshot; historical terminal records establish a baseline rather than being
replayed as new completion. The cursor is not durable authority. Failure or
omission of attention leaves the main tool result intact. `current_window_activity`
is a nonmeaningful diagnostic observation and neither receives nor consumes passive
attention. Explicit `jobs.attention` retains its Project-level scope and zero-active
summary independently of this cursor.

Query may piggyback; control may not. `stop_job` is the explicit canonical
Mutate/JobRun/Standard/DesiredState primitive, with `confirm=true`, `job:run`,
and unchanged Project/Session ownership checks. MCP/Adaptive expose it directly;
GPT Actions uses the same definition through `call_runtime_tool` under its
GatewayOnly policy. There is no cancel alias, mixed observe/mutate manager, or
control sidecar. Direct exposure does not admit any Job lifecycle/control or
Host carrier tool into nested Code Mode E1/E2a/E2b.

A Server can preserve authoritative identity in its ToolResult, but cannot prove
that a remote Host/tool wrapper retained or delivered that result. Diagnose that
boundary separately; do not add replacement execution or invent authority to
hide delivery loss outside the Server.

## 1. Keep request lifetime, Job lifetime, and observation state separate

Three identities have different lifetimes:

| Thing | Meaning | Expected across Control Server restart |
|---|---|---|
| MCP / HTTP request | One transport request or bounded wait | No. The connection/request may fail immediately. |
| `job_id` | Identity of one already-dispatched execution | Active: when the same reconciliation-capable Runner process survives and reports inventory. Terminal public ordinary Jobs: also via the Server receipt within its original bounded retention window. |
| `after_observation_token` | Opaque lifecycle and bounded log-delta state for one observed Job snapshot | No. Its Server epoch is process-local; a surviving Job should return a reset baseline and fresh token immediately after restart. |

A dropped `observe_jobs`, `job_tail`, or other observation request therefore does **not**
mean that the underlying Job was lost. The caller should keep the original
`job_id` and observe authoritative Job state again before considering any retry.

Observation tokens are opaque. They must be returned unchanged by clients and
must never become business identity, retry identity, authorization, or evidence
that an execution no longer exists.

The first log observation returns a bounded current baseline. A later
cursor-aware token returns only newly observed stdout/stderr when continuity is
provable, or empty tails when only lifecycle metadata changed. `reset` means
continuity could not be proved (for example, retained logs advanced past the
token or the Server epoch changed), so the response contains a bounded recovery
tail and a new current token. One conservative repeat after a reset is expected;
silently assuming missing output is not.

## 2. Control Server restart recovery contract

When `job_state_reconciliation=true`, the intended recovery chain is:

1. A Job is accepted and dispatched to a Runner.
2. The Control Server stops or restarts while the Runner process and command keep
   running.
3. The same Runner process reconnects with the same `client_id` and
   `agent_instance_id` and supplies a complete active Job inventory.
4. The new Server registry validates that inventory and reconstructs the same
   `job_id` and ownership/project/session context.
5. An observation request carrying a token from the old Server epoch refreshes
   immediately rather than waiting for a new command-side event.
6. Later terminal state remains queryable through the same Job identity, subject
   to the normal bounded terminal-retention contract.

The Server-side reconstruction path records
`recovered_after_server_restart=true` and
`recovery_reason_code=server_restart_reconciliation`. Runner inventory is the
recovery authority; the Server must not guess a replacement execution or replay
the original command.

A command that finishes while the Server is down is also recoverable when its
terminal snapshot is still in the Runner's bounded retained inventory.

The production Server also hydrates accepted public ordinary terminal receipts
from `wc_job_receipts` before accepting traffic. Receipt writes happen after the
registry lock is released and cannot change a terminal verdict. The receipt
reuses the safe Job snapshot, excludes executable validation metadata, and fixes
`terminal_observed_at` / `expires_at` at the first accepted terminal observation.
SQLite retains at most 64 receipts per logical Runner for 24 hours. Expired
receipts are pruned on database open, writes, reads, and the existing recovery
sweep. Historical owner attribution is independent of replacement registration.
A new observation epoch resets old tokens without granting execution authority.

Only Server-admitted Jobs with proven public visibility are receipt candidates.
Inventory-only reconstruction retains its existing reconciliation behavior; it
cannot prove whether an unknown Job was previously a hidden synchronous result,
so it does not independently create a durable receipt. Receipt hydration never
creates a pending request, execution mapping, waiter, or stop/retry/adopt lease.

### What is expected and what is a bug

Expected:

- the in-flight MCP/HTTP request fails because the Server process restarted;
- an old observation token becomes stale and is replaced by a fresh token;
- a Job waits as `agent_queued` when Runner execution capacity is full;
- a legacy Runner without reconciliation support cannot provide this recovery
  guarantee;
- a new Runner process is outside this recovery contract.

A V1 correctness/reliability incident exists when all of the following are true:

- the Job had been dispatched and was active (or retained terminal) before the
  Server restart;
- the Runner process survived: the same `client_id` still reports the same
  process-scoped `agent_instance_id`; reconciliation logs may use
  `process_started_at` as a secondary cross-check;
- the Runner advertises `job_state_reconciliation=true`;
- the Job is present in the Runner inventory supplied after reconnect, or should
  have been present under the complete-active-inventory contract;
- after successful re-registration the same `job_id` is permanently reported as
  unknown or otherwise requires launching a replacement execution.

The important distinction is **request loss versus execution loss**. Treating
both as “retry the command” risks duplicate effects.

## 3. Diagnostic playbook for an `unknown job` after restart

Before retrying work, collect safe runtime facts:

1. Use `runtime_status` / `list_runners` to establish the current Server build,
   Runner connection state, `client_id`, process-scoped `agent_instance_id`,
   reconciliation capability, and Job concurrency state. If reconciliation logs
   are available, cross-check `process_started_at` there; it is not part of the
   current `runtime_status` / `list_runners` projection.
2. Determine whether the Runner process changed. If it changed, do not claim the
   same-process Server-restart recovery contract was violated.
3. If the Runner process is unchanged, inspect the registration/reconciliation
   path: was the original `job_id` present in `job_inventory`?
4. If it was present, verify that `reconcile_inventory_locked` reconstructed or
   updated the Server record instead of dropping it.
5. If it was absent, investigate Runner `JobManager` retention/inventory rather
   than creating a replacement Server Job.
6. Re-observe the original `job_id`. A stale observation epoch should cause an
   immediate token refresh when the Job exists.
7. Only after authoritative lifecycle evidence establishes a safe retry state
   should a caller create a new execution.

Useful reconciliation diagnostics should remain bounded and secret-free. A
summary such as runner instance, active/terminal inventory counts, reconstructed
count, updated count, and missing count is sufficient; command text, log bodies,
credentials, and private paths are not required.

## Long-running native process/script Jobs

Execution duration and lifetime ownership are separate policies. `run_process`,
`run_script`, and `run_detached_process` default to 60 seconds and accept a
total execution lifetime up to 604800 seconds (7 days). Values above that
ceiling clamp to 7 days. The Server-owned bounded synchronous handoff grace is
return-latency policy only; it never extends execution lifetime. Normal
model-facing discovery exposes no handoff-timing tuning input; compatibility
inputs, when present on older callers, can only be tightened by trusted Server
policy. `run_shell`, structured
validation, and trusted Skill resource execution retain the 3600-second
ceiling, and direct synchronous structured Runner requests retain the
120-second ceiling.

Use ordinary `run_process`/`run_script` for hours-to-days work on one Runner
host when the Runner process is expected to remain the lifetime owner. Use
`run_detached_process` only when the native payload must survive Runner process
restart, upgrade, stop, or replacement; duration alone is not a detach reason.
Detached recovery preserves the same logical Job/execution fence and does not
permit duplicate payload dispatch.

For normal sync-first execution, model-facing handoff is intentionally sparse:
`execution_state=pending` plus one exact fallback continuation. The canonical
registry and Session ledger retain the Job id, lifecycle, validation identity,
source fence, and structured execution metadata. A handoff in an exact
authenticated Window/Project/Workflow Session establishes the passive
`JobAttentionCursor` baseline without echoing a second active notification.
Subsequent ordinary coding calls in that same scope may attach `job_attention`
only when terminal truth or the canonical Server recovery overlay changes. Terminal attention contains
bounded outcome / exit truth, conservative validation/source-freshness truth
when applicable, and an explicit details call, but never stdout/stderr bodies.
The same terminal revision is delivered at most once per process-local cursor;
after Server restart a bounded active-state duplicate is allowed, while
historical terminal Jobs establish baseline and are not replayed as new
completions. Passive attention reads only the Server registry and never starts,
retries, stops, or polls a Runner execution.

This makes the normal same-turn path independent of Host automatic wake:
pending execution can overlap read/edit/search/review work and a later ordinary
result can carry its terminal truth. `observe_jobs` remains the explicit path
for logs, additional diagnostics, recovery, or cases where bounded terminal truth
is insufficient. `wait_for_job_terminal` and Host continuation carriers remain
optional blocked-on-terminal acceleration, not required lifecycle machinery.

Long-running Jobs still occupy the Runner's normal `max_concurrent_jobs`
execution quota. Detached Jobs remain excluded only from Runner shutdown drain
because shutdown is not allowed to kill their supervisor-owned payload; they
are not excluded from execution scheduling quota.

This facility is intentionally not a cluster scheduler. It supports one Runner
host, native processes/scripts, bounded observation, durable stop, and detached
Runner-process replacement recovery. It does not promise native process
survival across host OS reboot or power loss, multi-node scheduling, GPU
allocation, preemption/requeue, Slurm/Kubernetes replacement, arbitrary
model-provided secret environments, or detached named SSH resources. Training
programs should write checkpoints and complete logs to project files;
`observe_jobs` is a bounded tail and the 64 KiB Job snapshot tail is not a
training-log store. Secret/environment configuration should remain
Runner-owned rather than expanding model-authored inputs.

Detached running-output tails are durably checkpointed at a 5-second cadence,
rather than at the live Job update cadence, so multi-day chatty workloads do not
turn bounded presentation state into continuous fsync pressure. Stop/control
polling remains independent and fast, and terminalization performs a bounded
final drain and durable commit of the final retained tails.

## 4. Runner Job capacity is shared across windows and projects

`max_concurrent_jobs` is a Runner-process execution limit (default 4, valid
range 1..64; out-of-range configuration is rejected). It is not allocated per
ChatGPT window, Workflow Session, or Project.

Opening multiple model windows consumes no Job slot by itself. A slot is consumed
while a Job-backed execution owns Runner execution capacity. Therefore several
windows using different Projects on the same Runner can contend for the same
pool.

For example, with `max_concurrent_jobs = 4`:

- Window A running a build: 1 slot;
- Window B running tests: 1 slot;
- Window C running a long structured process: 1 slot;
- one additional Job may run immediately;
- later accepted Jobs remain the same Jobs and report `agent_queued` until a slot
  is available.

`agent_queued` is not a reason to create another Job. The queued record keeps its
original `job_id`, enters the Runner's complete active inventory, and should also
survive a Control Server restart under the same reconciliation contract.

A structured process or validation may consume a Runner Job slot while it runs
even when it finishes quickly enough for the initiating tool call to return a
terminal result instead of exposing a long-lived handoff to the model.

### Independent concurrency planes

Do not conflate the Job execution pool with other limits. In particular:

- Runner Job execution uses `max_concurrent_jobs`;
- polling request dispatch has its own in-flight bound;
- persistent shells have their own bounded population/lifecycle.

Changing one does not redefine the others. `runtime_status` / `list_runners`
should be used for current `job_concurrency { limit, running, queued }` facts
instead of inferring capacity from the number of browser/model windows. These
are bounded lifecycle-status counts, not an exact free-slot calculation:
`stop_requested` can still own a Runner slot until it becomes terminal, so do
not derive `available_slots` or saturation by subtracting `running` from `limit`.

## 5. Requirements for model-facing tool descriptions

Tool descriptions are part of the reliability contract because they influence
whether a model observes an existing execution or accidentally creates another
one. Keep descriptions concise, but preserve these semantic distinctions.

### Description density and discovery hygiene

The top-level tool description is primarily a **selection surface**, not a mini
reference manual. It should answer what the tool does, when it wins over nearby
choices, and any lifecycle fact that changes retry safety. Put detailed numeric
bounds, wire rules, and field-specific behavior on the relevant input/output
schema instead of repeating them in every top-level description.

For ordinary tools, keep the top-level description as short as its selection and
lifecycle semantics allow. There is no secondary numeric density limit below the
repository hard ceiling (`MODEL_TOOL_DESCRIPTION_MAX_CHARS`, currently 1024);
using more of that budget is appropriate when it preserves selection, authority,
retry, continuation, uncertainty, safety, or recovery semantics. Avoid naming
sibling tools merely to restate implementation or fallback details, because
exact-name discovery may otherwise retrieve unrelated tools whose descriptions
happen to mention the queried name. Prefer capability phrasing such as “shell
command tool”, “structured validation”, or “asynchronous execution” unless the
sibling tool name is itself needed to choose correctly.

Generic lifecycle words such as `Job` should be concentrated on actual Job
creation/observation tools. Structured validators and process adapters can say
that long work continues as the **same execution** and returns `job_id`, while
the timeout/output schema carries the detailed handoff contract. This keeps the
retry guarantee without turning every validation description into a Job search
hit.

### Job-producing execution tools

For structured validation/process tools (`cargo_*`, `go_test`, `run_process`,
`run_script`, `run_job`, and future equivalents), the combined model-facing tool
description plus lifecycle input/output schema should make clear that:

- a long operation continues as the **same execution / same Job**;
- handoff is not cancel-and-retry;
- queued execution keeps the same `job_id`;
- loss of the initiating request is not evidence that the Job did not start.

The top-level description normally carries only the selection-critical part of
that contract, such as “same execution” or a stable `job_id`; field descriptions
carry the detailed handoff and lifecycle rules. Avoid wording anywhere in the
model-facing schema that encourages “rerun if the call times out” without
consulting structured lifecycle state.

### Job observation tools

For `job_tail` and `observe_jobs`, the top-level description plus
observation-field schemas should make clear that:

- observation never launches or retries the Job;
- `wait_secs` is one bounded wait, not a subscription;
- `after_observation_token` is an opaque observation cursor, not Job identity;
- first log observation is a bounded baseline and cursor-aware follow-ups are
  delta-only when continuity is provable;
- an unterminated final line is not conclusively consumed; a follow-up may
  conservatively repeat that bounded partial line until a line boundary is observed;
- `reset` is a bounded recovery refresh, not proof that no intervening output
  existed;
- Control Server restart may invalidate the token while leaving `job_id` valid;
- a stale Server epoch should refresh immediately when the same Job has been
  reconciled;
- `unknown_job` after same-process reconciliation is a diagnostic signal, not an
  automatic instruction to create a replacement Job.

`observe_jobs` adds an optional `wake_on` policy: `change` is the compatible
wire default and wakes on any observable update. `terminal` coalesces ordinary
stdout/stderr/progress/activity changes until any watched Job is terminal, an
item errors, or one shared absolute deadline expires. It never returns an
`updated` wake reason: at the deadline `wait.outcome=timeout` can coexist with
`changed=true`. Item errors take precedence over terminal, then timeout.

Canonical execution handoffs expose a parser-ready `wake_on=terminal` observation
continuation whose bounded `wait_secs` is adapted by the Server-side MCP Host timing
profile. When blocked on terminal, prefer `wait_for_job_terminal` with a real Host
carrier; the bounded observation wait remains the details/recovery fallback, not a
polling subscription. When independent work remains, retain the exact
identity/continuation and continue that work, optionally requesting `jobs.attention`
on an ordinary observation. The transport-neutral Runtime still accepts explicit
waits up to 100 seconds and terminal completion wakes immediately; MCP transport
may clamp model-facing waits further to fit the configured Host budget. Any missing
token still gives an immediate baseline, and omitting `wait_secs` gives an immediate
observation.
Each Job waiter advances a private opaque cursor on non-terminal updates; final
bounded deltas always use the caller's original token. Waiters use canonical
Notify/revision rechecks, without a periodic polling heartbeat; updates neither
recreate other Jobs' waiters nor extend the batch deadline.

Workflow Session records retain every `observe_jobs` interaction for audit and
validation evidence. Runtime Console treats these calls as observation
transport: they are excluded from current/last Activity, the detail timeline,
and work run counts. `running_call` still reports an unfinished transport call.
Original Job handoff Activities remain historical snapshots; observing a
terminal Job does not rewrite them or join live Registry state into Activity.

### Runtime/operator observation tools

Descriptions for `runtime_status`, `list_runners`, and related operator surfaces
should distinguish connection health from execution capacity and expose safe
facts needed to diagnose recovery:

- Runner process identity/liveness;
- reconciliation capability;
- current Job concurrency limit/running/queued counts;
- Server build/version compatibility where relevant.

Do not imply that a healthy transport proves Job recovery succeeded, or that an
open model window reserves Runner capacity.

## 6. Acceptance coverage

The minimum real-process acceptance scenario for this contract is:

```text
running Runner Job
-> stop Control Server only
-> keep Runner process and command alive
-> restart Control Server
-> Runner re-registers complete inventory
-> same job_id is reconstructed
-> old observation token refreshes immediately
-> command runs exactly once
-> terminal result remains queryable
```

A second scenario should allow the command to become terminal while the Server is
down and verify terminal reconciliation after restart. A repeated-restart
scenario should preserve the same Job identity, monotonic sequence/log cursors,
and single execution.

These expectations already have dedicated coverage in
`docs/TESTING.md` (`e2e_job_reconciliation_ws.sh` and
`e2e_job_recovery_failures_ws.sh`). Unit coverage also verifies old-epoch
observation-token refresh behavior.

## 7. Runner-process restart boundary

Ordinary Jobs are still process-owned by their exact Runner instance. A Runner
process restart therefore makes those child processes unrecoverable and they
converge to `lost`; Server-side inventory reconciliation must never infer native
ownership for them.

`run_detached_process` is the explicit exception. Before payload start it makes
a one-shot durable ownership handoff to a narrow supervisor. The replacement
Runner may transfer the same logical Job to a new `agent_instance_id` only when
bounded durable state, request/context identity, supervisor native start
identity, and lifetime fencing all reconcile exactly. A lost initiating response
is replay-safe only while that logical Job remains in active or retained terminal
history; replay keys are not permanent tombstones after retention expires.

This contract still does not promise:

- survival of an in-flight MCP/HTTP connection across a Control Server process
  restart;
- survival of detached execution across a machine reboot;
- unbounded Job/log or idempotency-key retention;
- a generic distributed scheduler or general child-detach API;
- blind automatic retries for uncertain execution outcomes.

## 8. Implementation reference points

The current contract is implemented and tested primarily in:

- `src/shell_client/state.rs` — Server Job record, observation epoch, and Runner
  concurrency metadata;
- `src/shell_client/reconciliation.rs` — inventory validation and reconstruction;
- `src/tool_runtime/observe_jobs.rs` and `src/job_observation.rs` — bounded
  observation/token behavior;
- `crates/webcodex-runner/src/main.rs` — Runner `JobManager`, inventory, queue,
  and slot reservation;
- `docs/RUNNER.md` — public Job/concurrency behavior;
- `docs/TESTING.md` — real-process restart/reconciliation acceptance coverage.

## Server-only convergence measurement

The existing Action Audit `summary.model_ergonomics` (schema version 10) adds
optional `job_convergence`. It records per-invocation `pending_handoff_count`,
`passive_terminal_delivery_count`, `passive_failure_delivery_count`, and
`wait_for_job_terminal_count`, plus at most nine bounded events. Wait counts
include rejected invocations. Terminal failure means negative validation truth
or failed command truth; a separate event boolean identifies the validation
failure cohort for measuring actionable diagnostics.

Events are `pending_handoff`, `explicit_observe`, or `passive_terminal`. Their
64-hex `relation` hashes an unambiguous length-prefixed tuple: process-local
random salt, authenticated principal kind/id, exact Window, durable Project,
business Workflow Session, and Job. The salt is shared by runtime clones and
changes after restart. No native ids, command, argv, cwd, env, source, logs,
error text, or provider payload enter this projection. Reading durable Session
attribution for an exact authorized `observe_jobs` result is telemetry only;
it never supplies request/business/recorder context. Explicit mismatching
Project/Session context cannot correlate. Missing Window, scope, Job attribution,
or a busy registry yields `correlation_complete=false`, never a guessed match.
Failed or partially failed observation also leaves incomplete correlation; it
cannot be used as evidence that no explicit observation occurred.
The immutable telemetry snapshot uses a nonblocking read and cannot poll, refresh,
mutate, create receipts, or affect the ToolResult.

MCP's existing bounded principal/Window continuity registry also retains the
exact prior meaningful call's Server trace id. Action Audit stores that link as
`summary.previous_meaningful_call` only for a proven serial predecessor. Overlap,
cancellation, eviction and coverage gaps invalidate the link. There is no new
model-turn identity or claim that the Host/model consumed the result.

The existing offline `scripts/agent_loop_report.py summarize` report derives:

- `pending_followed_immediately_by_observe_count`: the next serial meaningful
  MCP call has an exact `explicit_observe` event matching the pending relation.
  Discovery/diagnostic calls are nonmeaningful under the existing policy. An
  intervening meaningful read/edit or different Job/Project/Session does not count.
- `passive_terminal_before_explicit_observe_count`: a complete chain back to the
  exact pending handoff contains no explicit observation of that relation.
- `terminal_failure_followed_by_observe_count`: the first later exact explicit
  observation after passive failure, within a proven serial chain. Reads/edits
  alone do not count. The report separately counts the validation failure cohort.
- `pending_to_terminal_ms`: first Server terminal observation minus the pending
  response's existing `response_handed_at_ms`. Terminal time comes from the
  registry's existing `terminal_observed_at`, at second resolution. Missing or
  inconsistent timestamps are unknown, never replaced by creation/start time,
  duration, or observation delivery time.

Correlations and distributions are computed offline because kernel completion
cannot establish adapter response handoff or exact serial adjacency. Missing
predecessor rows, non-MCP timing, restart/retention loss and overlap remain
unknown; they never become evidence of "no observe". The report consumes at most
100,000 rows and one million predecessor links for this analysis and exports
only aggregate counters/distributions, not relations or identities. Existing
Action Audit retention and failure isolation apply. Nothing is added to ordinary
`runtime_status`, model results, discovery, or nested Code Mode contracts.
