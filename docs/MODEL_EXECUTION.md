# Model Execution and Durable Continuation

Status: design direction for the next WebCodex execution iterations. This is not a claim that every described tool or MCP extension is implemented today.

WebCodex should make model-driven execution predictable across Linux and Windows without requiring the model to become an expert in shell quoting, process lifetime, transport behavior, or UI orchestration. The core execution contract must also remain usable when one model turn ends before the underlying work does.

## 1. Why this is a product boundary

Recent dogfood exposed a consistent pattern: repository reads, structured edits, Git, validation Jobs, cross-machine Runner routing, and Server-restart Job reconciliation are no longer the dominant source of friction. The remaining friction clusters around execution boundaries:

- shell quoting and nested shell layers, especially Bash -> SSH -> PowerShell -> native executable;
- ambiguous command lifecycle after timeout, transport failure, or enqueue failure;
- long script bodies forced into command strings;
- a split between short synchronous execution and manually selected async Jobs;
- polling transports that can become unavailable while an ordinary long dispatch pins the poll loop;
- Windows output/encoding differences leaking into model-visible stderr;
- model turns spent repeatedly observing several independent Jobs.

These are not reasons to replace every shell backend. They are reasons to make shell text an escape hatch rather than the primary universal execution representation.

## 2. Current implementation facts to preserve or improve

Verify these facts against current code before implementation; the paths below are architectural landmarks, not frozen line references.

- `src/tool_runtime/shell.rs` can know that a command may have started while the shared rejection prose in `src/tool_runtime/helpers.rs` still says that no command started. Structured state and human-readable text must never contradict each other.
- `src/shell_client/validation.rs` limits command strings to 8,000 bytes while the same bridge already accepts a much larger bounded stdin payload. Long scripts should use a payload channel instead of a larger quoted command blob.
- structured Cargo validation already demonstrates the desired handoff pattern: start exactly one execution, wait for a bounded synchronous grace window, then expose the same execution as a Job if it is still running.
- Job observation already has opaque Job-bound `observation_token` values and bounded wait outcomes. New observation APIs should reuse that model instead of inventing another revision mechanism.
- the polling Runner currently backgrounds persistent-shell work specially, while ordinary polling dispatch waits for worker completion before the next poll. Long ordinary work can therefore starve transport progress even though execution itself is running correctly.
- the Runner defaults `max_concurrent_jobs` to four while retaining the existing deterministic, restart-required operator override and an effective range of 1 through 64. The upper normalization point is the unchanged hard active Job inventory bound, not a new scheduler policy.

## 3. Core design invariants

### 3.1 Execution truth has one owner

Every execution result must make these cases distinguishable without prose inference:

- definitely not started;
- accepted/running;
- completed with a terminal result;
- outcome unknown because the system cannot prove whether the side effect started or completed.

The exact public enum may evolve with implementation, but these semantic states must not be collapsed. Booleans such as `command_started` and `command_completed`, structured failure classification, Job status, and human-readable guidance must agree.

A caller must be able to decide whether retry is safe from structured state. WebCodex must never instruct a model to retry as a pre-start rejection when the command may already be running.

### 3.2 Structured execution first, shell second

The preferred general execution primitive should represent a native process directly:

```yaml
executable: git
args: [status, --porcelain]
cwd: .
stdin: null
timeout_secs: 600
purpose: diagnostic
```

The Runner should perform executable/argv execution directly on the target platform. Shell parsing is not needed for ordinary native command invocation.

A script-oriented primitive should carry the script through a bounded payload channel rather than force the complete body into one command string. The implementation may use stdin or a bounded temporary file, but the public contract should represent script content as data.

`run_shell` remains useful for shell semantics such as pipelines, shell functions, redirection, or interactive operator diagnostics. It should remain an explicit escape hatch, not the representation every model must use for every executable.

The first structured-exec version should stay small. Prefer `executable`, `args`, `cwd`, bounded `stdin`, total timeout, purpose, and existing project/Session context. Do not add arbitrary environment mutation, PTY semantics, a process-management framework, or a Windows service API until a concrete use case requires them.

### 3.3 One execution may outlive one model turn

A model should not need to predict whether a command is "short" or "long" before choosing a tool.

The desired behavior is:

```text
start one execution
    |
    +-- finishes within sync grace --> return terminal result
    |
    +-- still running -------------> expose the same execution as a durable Job
```

The handoff must not cancel and restart the command. The same process/execution continues and receives one durable `job_id` plus observation state. This generalizes the pattern already used by structured validation.

`timeout_secs` is the total execution budget. A small internal synchronous grace controls only how long the current tool call waits before returning a Job handle.

### 3.4 Job is the continuation contract

A Job is not a UI card and not a transport connection. It is the durable continuation handle for work that outlives the initiating call.

Core Job state should stay OS-neutral and presentation-neutral. At minimum the upper layers need:

- `job_id`;
- lifecycle status and terminal flag;
- exit code when terminal;
- bounded stdout/stderr observation;
- opaque `observation_token` or equivalent revision handle;
- cancellation/stop behavior;
- project/Session ownership and existing permission evidence;
- recovery/lost semantics when the Runner or transport changes.

Linux shell details, PowerShell parser details, Win32 process details, WebSocket connections, polling loops, and MCP App iframe state must not become part of the Job identity contract.

### 3.5 Observation is a first-class read primitive

Models and UIs should be able to observe multiple relevant Jobs without one model turn per Job.

A first product batch primitive should be read-only and bounded, for example:

```yaml
observe_jobs:
  items:
    - job_id: A
      after_observation_token: tokenA
    - job_id: B
      after_observation_token: tokenB
```

The first version should follow existing `read_files` / `search_project_texts` batch conventions where practical:

- bounded item count;
- request-order output;
- isolated per-item failure;
- bounded total response size;
- continuation metadata if the response budget is exceeded;
- no mutation and no implicit re-execution.

Do not start with a complex batch scheduler. Models may continue launching separate Jobs; batching observation is the lower-risk, higher-confidence optimization.

## 4. MCP 2026 / MCP Apps design posture

MCP Apps are an optional presentation/orchestration adapter, not the execution kernel.

Experiments on the `special` MCP probe workspace established useful design constraints on the ChatGPT host available during the experiment:

- an App can poll app-only tools while the initiating model turn is no longer active;
- App code can pass a terminal or partial snapshot back into model context and request a later model turn;
- real WebCodex Jobs can therefore continue without keeping one model tool call open;
- independently auto-resuming multiple Job Views can race and cross-wire context/message pairing;
- a single server-side/conversation-level resume authority can serialize handoff correctly;
- partial handoff is useful: completed Jobs can wake the model while slower Jobs remain represented by durable pending handles;
- one batch observation call can refresh multiple pending Jobs after the model resumes.

These are experimental product inputs, not promises that every ChatGPT workspace or future host exposes identical behavior. In the same experiment, optional MCP features such as Tasks/MRTR/elicitation/progress were not a dependable foundation for the product.

Therefore:

1. Core WebCodex execution and Job APIs must not depend on MCP Apps, `ui/message`, `updateModelContext`, MCP Tasks, or any other optional host feature.
2. Structured MCP `structuredContent` should remain sufficient for an App or model to consume Job snapshots without parsing human prose.
3. If an App UI is added, individual Job cards/views may display state, logs, and cancellation, but they do not independently own model-resume authority.
4. One conversation-level Orchestrator should own automatic resume decisions for a set of Jobs.
5. A handoff snapshot may contain both terminal results and pending Job handles; a batch is an observation/orchestration optimization, not an indivisible transaction.

Conceptually:

```text
Model turn 1
    |
    +--> start execution(s)
    |        |
    |        +--> durable Job handles
    |
    `--> model turn may end

Optional MCP App Orchestrator
    |
    +--> cheap/bounded Job observation
    +--> aggregate useful progress
    `--> one context handoff + one resume decision

Model turn 2
    |
    `--> continue from terminal results + pending handles
```

The UI may change. The Job/observation contract should not have to.

## 5. Transport reliability is part of execution UX

Transport choice must not change the execution semantics seen by the model.

For polling in particular, receiving one long request must not prevent the Runner from polling again for keepalive, cancellation, or unrelated work. Ordinary dispatch should be decoupled from the poll loop similarly to the existing persistent-shell special case, while result submission and project-cache invalidation remain correct.

This is required even if a deployment normally prefers QUIC or WebSocket, because polling remains a fallback transport. QUIC may improve behavior on lossy or high-jitter links, but it cannot compensate for a polling implementation that deterministically blocks its own next poll.

Validation should include the same long-work workload over polling, WebSocket, and QUIC/auto where available, checking selected transport, stale/reconnect behavior, Job state continuity, and non-duplication of execution.

## 6. Windows posture

Windows should benefit primarily from reducing shell dependence, not from adding a large Windows-only abstraction layer first.

Near-term goals:

- native executable/argv execution should bypass PowerShell quoting when shell semantics are unnecessary;
- script payloads should avoid nested one-line PowerShell quoting;
- Runner output presented to the model should be normalized to valid UTF-8 with deterministic decoding/failure semantics;
- Job lifecycle and observation schemas should remain the same on Windows and Unix.

Not required for this execution iteration:

- productized Windows SCM installation;
- self-upgrade/drain lifecycle;
- a generic process list/kill/service API;
- desktop/PTY terminal UX.

Those can be added later when they solve a current user need. A deployment may use an external/manual service wrapper without changing the execution API design.

## 7. Iteration order

### Phase A — truthful execution state

Remove contradictions between structured lifecycle fields and prose. Define and test the pre-start, running/may-have-started, completed, and unknown-outcome branches. This is the smallest high-value prerequisite because safe retry behavior depends on it.

### Phase B — structured process and script execution

Introduce the smallest direct-argv process primitive and a bounded script payload path. Reuse current project, permission, Session, timeout, output, and Runner boundaries. Keep shell as an escape hatch.

Phase B1 is complete. `run_process` carries a bounded executable plus
string-array argv to capable Runners as typed protocol data
(`structured_process_argv`). It never reconstructs shell text and fails before
start when that capability or true native argv transport is unavailable. On
Windows, `.exe`, `.com`, and extensionless PE images are native executables;
resolved `.cmd`/`.bat` files are rejected before spawn because they require
shell/script semantics. `run_shell` remains the explicit escape hatch, with no
automatic fallback.

Phase B2 provides `run_script`. Bounded script content, script argv, and stdin
are separate typed data carried only to Runners advertising
`structured_script_payload`; the script body never enters the legacy command
string. The Runner maps the explicit `sh`, `bash`, or `powershell` language to a
platform interpreter and invokes that interpreter directly with a private
temporary `.sh` or `.ps1` file. Script argv remain independent values. The
temporary file uses the Runner's inspect scratch when the inspect sandbox is
active and is removed after execution whenever possible. PowerShell files use
a UTF-8 BOM so Windows PowerShell 5.1 preserves Unicode without prepending
script statements.

Phase B2 deliberately does not add `cmd`/batch semantics or typed script
transfer over named SSH Session resources. `run_shell` remains the explicit
escape hatch for batch and SSH shell semantics.

### Phase C — general same-execution sync-to-Job handoff

Phase C is implemented for `run_process` and `run_script`. A capable Runner
advertises the additive `structured_execution_jobs` capability only when it can
execute both typed process Jobs and typed script Jobs. The Server creates one
initially hidden Job, dispatches it exactly once, and waits for a bounded
internal synchronous grace:

- a terminal Job within the grace is projected back into the ordinary
  `run_process` or `run_script` terminal result and its never-exposed Server Job
  record is removed;
- a still queued or running Job is made public with the same `job_id`,
  observation token, project, Session, authorization, cwd, purpose, and sandbox
  contract;
- the Runner never receives a replacement request at handoff, and neither
  process argv nor a script body is reconstructed as shell text.

The public `timeout_secs` contract is a total execution budget from 1 through
3,600 seconds, with a 60-second default. The internal synchronous grace is 10
seconds and the effective wait is the smaller of that grace and the total
budget. Handoff does not restart or extend the timeout clock. If the total
budget has no continuation headroom, the initiating call waits for the original
execution to terminate or time out instead of manufacturing a Job handoff.

Typed Job wire requests are explicit. `start_process_job` carries
`command=""`, a present `process` payload, an absent `script` payload, and
independent typed `stdin`. `start_script_job` carries `command=""`, a present
`script` payload, an absent `process` payload, and independent typed `stdin`.
Polling, WebSocket, and QUIC use this common representation. Process Jobs retain
direct `Command::new(executable).args(argv)` execution. Script Jobs retain the
Runner-owned temporary file and semantic interpreter policy; they never use
`sh -c`, `bash -c`, or PowerShell `-Command`. The Job task owns the script file
and inspect scratch for the complete child lifetime, including after handoff.

Job snapshots and terminal updates carry the existing
`ShellCommandExecutionState` additively for typed structured Jobs. This
preserves `not_started`, `outcome_unknown`, `timed_out`, and `completed`
lifecycle truth independently from Job status. The model-facing initiating
result uses `queued` or `running` only for an actual durable handoff; those are
not new variants of `ShellCommandExecutionState`. Same-instance Runner
reconciliation restores the same active structured Job without replaying its
typed input. Safe durable metadata records only execution source, language or
script byte count where applicable, argument count, stdin presence, and a
bounded summary—not raw process argv, script content, script args, or stdin.

Fast-terminal hidden handles are removed after their terminal result is
projected by the initiating Server process. That Server process keeps a bounded,
expiring suppression record for the exact runner instance, Job, and request it
discarded. A same-instance transport reconnect to the same Server therefore
suppresses a retained terminal inventory replay of that already-projected hidden
handle instead of resurrecting it as a public duplicate.

The suppression history is process-local only. After a Server restart, an
otherwise unknown retained terminal structured snapshot cannot be proven to
have been hidden, previously delivered, or already public, so it is recovered
conservatively as public execution evidence. In both cases reconciliation never
replays typed input or enqueues a replacement execution.

The capability is fail-closed and is never inferred from `async_jobs`,
`structured_process_argv`, `structured_script_payload`, or
`structured_validation_argv`. A Phase B2 Runner without
`structured_execution_jobs` retains the approved direct synchronous path for
timeouts through 120 seconds. Larger timeouts fail before execution with
`capability_unavailable`; there is no `run_shell` or `run_job` fallback. The
inactive server-local compatibility path follows the same direct-through-120
and reject-larger behavior. Named SSH Session resources remain unsupported for
typed process and script execution and fail before enqueue.

### Phase D — batch Job observation

Phase D is implemented as the model-visible, read-only `observe_jobs`
primitive. One call accepts 1 through 8 `items`, each containing a non-empty
`job_id` and an optional opaque `after_observation_token`. The input has no
project field: like `job_status` and `job_log`, authorization is resolved from
the Job id across local and Agent executors. Duplicate Job ids are rejected
before observation.

The global tail defaults to 40 lines per stream and is bounded from 1 through
200. `wait_secs`, when present, is bounded from 1 through 60 seconds and is one
batch wall-clock budget, never a per-item or subscription budget.

`observe_jobs` composes the existing `job_log_for_auth` implementation:

- an immediate concurrent pass applies the supplied token and bounded tail to
  every item in input order;
- a missing baseline, a changed token (including an old Server epoch), an
  already-terminal Job, or an item error is immediately actionable;
- otherwise the batch races the canonical per-Job waits and uses a shared
  read-only canonical observation heartbeat so a missed Agent notification
  cannot defer a visible token change until the deadline;
- the first meaningful change, terminal transition, or item error ends the
  shared wait; an unchanged batch ends at the one shared deadline;
- pending sibling wait futures are dropped without stopping, cancelling,
  retrying, or otherwise mutating their Jobs;
- after a wake, one final non-waiting canonical pass refreshes every item, so
  the response does not combine one fresh Job with stale sibling snapshots.

The deterministic outer `wake_reason` precedence is: no requested wait or any
missing baseline yields `immediate`; otherwise an item error wins over
`terminal`, which wins over `updated`, with `timeout` used only when every
returned observation remains successful, nonterminal, and unchanged.
Per-item `changed` and `terminal` fields remain the authoritative facts.

Local, legacy shell, validation, structured process/script, and Agent-backed
Jobs therefore retain the existing lifecycle, recovery/lost, validation,
structured metadata, log truncation, token binding, and authorization
semantics. Item failures are isolated; inaccessible and unknown Jobs share the
same external behavior. Results retain whole items in input order under the
existing model-result budget, expose `output_truncated` and the first omitted
`next_index`, and never persist opaque token bodies in audit/session summaries.

Phase D does not launch, retry, stop, schedule, or subscribe to Jobs. Batch
launch and concurrency/dispatch work remain out of scope.

### Phase E — polling dispatch reliability and practical concurrency

#### Phase E1 — bounded non-pinning polling dispatch (implemented)

Normal polling dispatch is process-local, bounded, and no longer pins the
polling control loop behind one ordinary long request. After the Server
atomically dequeues a request, the Runner hands it exactly once to a background
dispatch worker and may poll again while capacity remains. The fixed E1 bound
is two in-flight polling dispatches, the smallest value that permits one long
ordinary request plus a later Job/control or persistent-shell close request.
There is no Runner-side pending queue. When both slots are occupied, polling
waits for a worker completion before dequeuing more Server work.

Worker completions carry the correlated request id, dispatch outcome, and a
project-cache invalidation fact back to polling control. Fatal result submission
errors retain the existing structured `PollError::from_submit` classification
and stop that polling transport episode; permanent result rejection and
transient retry exhaustion remain non-replaying delivery outcomes. Every worker
is covered by the existing dispatch activity tracker through result submission
and its thread handle remains in the existing bounded shutdown accounting.
`--once` keeps its synchronous one-request and Job-drain contract.

This changes neither Server dequeue truth nor execution identity: a request is
not cancelled, reconstructed, requeued, or re-executed to make polling
progress. Results may complete out of dequeue order and remain correlated by
`request_id`. WebSocket and QUIC retain their existing background
`spawn_blocking` dispatch behavior.

#### Phase E2 — practical concurrency tuning (implemented)

The Runner's deterministic default Job execution concurrency is four. An
explicit `max_concurrent_jobs = N` remains supported, has an effective range of
1 through 64, and remains restart-required rather than hot-reloadable. Zero
normalizes to one, and values above 64 normalize to 64.
`JobManager` continues to own the existing per-client slot reservation,
bounded in-memory queue, terminal slot release, and eligible FIFO promotion.
Promotion starts the original queued Job and request exactly once with the
same `job_id`; no scheduler, priority, adaptive host tuning, or replacement
dispatch was introduced. The 64 ceiling is the existing
`JOB_INVENTORY_MAX_ACTIVE_JOBS` hard bound: Runner inventory must contain every
active Job or reject further starts. The active inventory constant remains
unchanged; using it to bound effective concurrency adds no smaller scheduler
policy.

Polling dispatch capacity remains independently fixed at two in-flight
requests. It is transport progress capacity, not Job execution concurrency,
and is not coupled to the effective `max_concurrent_jobs` range.

Runner registration additively reports the effective static limit as
`job_concurrency_limit`, so it contains only 1 through 64. Older Runner
registrations omit the field and remain unknown; the Server does not infer it
from capabilities, protocol version, inventory, or observed state. The field
is safe operational metadata and adds no capability bit, acknowledgement, or
request/response round trip.

The Server derives caller-visible concurrency state from canonical,
authorization-filtered Job records:

- running: `running` or `started`;
- queued: `queued` or `agent_queued`.

`stop_requested`, `recovering`, and terminal states retain their existing
lifecycle meaning. `list_agents` and full `runtime_status` client summaries
expose `job_concurrency { limit, running, queued }`; the top-level
`runtime_status.jobs` summary exposes `active_count`, `running_count`, and
`queued_count`; compact runtime status retains those three counts. Because the
dynamic counts are authorization-filtered while the static limit is
Runner-wide, WebCodex deliberately exposes neither `available_slots` nor a
derived `saturated` value. No command, argv, script, stdin, environment,
stdout, stderr, token, or credential content enters these fields.

### Phase F — Windows output normalization

Make shell/native stderr model-readable and UTF-8-stable. Do not make this a reason to delay structured exec.

MCP App UI work may proceed after the core Job/observation contracts are usable, but UI polish is not a prerequisite for Phases A-E.

## 8. Acceptance gates for execution changes

A model-facing execution change should answer all of these clearly:

- Can the caller tell whether retry is safe without reading prose heuristically?
- If work outlives the tool call, does the same execution continue exactly once?
- Can the work be rediscovered and observed from a durable handle?
- Can multiple relevant Jobs be observed without one model turn per Job?
- Is the upper-level state schema independent of Windows vs Unix shell details?
- Can an MCP App consume the structured result without becoming the source of execution truth?
- Does the design avoid giving independent Job Views uncoordinated model-resume authority?
- Are output, item count, wait time, and payload sizes bounded?
- Do transport failure and Runner replacement produce explicit recovery/lost/unknown semantics rather than silent retry?

## 9. Explicit non-goals for this cycle

Do not expand this execution cycle into:

- fleet dashboards or per-agent upgrade management;
- Runner drain/maintenance/self-upgrade;
- Windows SCM productization;
- a general process/service management framework;
- batch Job launch as the first batching feature;
- MCP Tasks/MRTR/elicitation/progress dependencies;
- polished MCP App UI;
- PTY/full terminal support;
- compatibility aliases for speculative consumers.

The goal is narrower: make execution truthful, structured, durable, observable, and cheap for the model to continue.
