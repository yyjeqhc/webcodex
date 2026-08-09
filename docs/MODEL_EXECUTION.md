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
- the Runner default `max_concurrent_jobs` remains conservative. Concurrency tuning is useful, but it is secondary to correct dispatch and observation semantics.

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
  jobs:
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

### Phase C — general same-execution sync-to-Job handoff

Apply the structured-validation pattern to general model execution: one start, short synchronous grace, same process continues as a Job, no retry-based promotion.

### Phase D — batch Job observation

Add a bounded read-only `observe_jobs`-style primitive using existing Job observation tokens and existing batch response conventions. Keep batch launch out of scope initially.

### Phase E — polling dispatch reliability and practical concurrency

Ensure ordinary long dispatch cannot pin the polling loop. Add focused observability for running/queued/limit state and tune deployment concurrency only after dispatch semantics are correct.

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