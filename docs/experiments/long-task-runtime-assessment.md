# Long-task execution and retention assessment

Date: 2026-10-25. Scope: current main plus the run_job budget correction in this
series. This is a source and targeted-regression assessment, not a day-long
training experiment or a production rollout.

## Findings

Execution lifetime, process ownership, and retained evidence are independent.
A successful admission receipt proves that the request has a Job identity; a
queued receipt does not prove that the payload has started or that it will
survive until its maximum allowed duration.

| Entry point | Default execution budget | Tool ceiling | Lifetime owner |
|---|---:|---:|---|
| `run_shell` | 60 seconds | 3600 seconds | Exact Runner instance |
| `run_job` raw Shell Job | 3600 seconds | 86400 seconds | Exact Runner instance |
| `run_process`, `run_script` | 60 seconds | 604800 seconds | Exact Runner instance |
| `run_detached_process` | 60 seconds | 604800 seconds | Detached Supervisor |

These are tool ceilings, not permission to bypass the Runner's
`policy.max_timeout_secs`. Its default is 3600 seconds; the current mini Runner
also advertised 3600 during this investigation. No Runner configuration was
changed. A higher tool ceiling alone therefore does not enable an hour-plus
training run on this Runner.

The shared server resolver uses the last registered Runner policy, then the
Runner enforces its own policy again. The effective budget is an admission-time
limit, not a promise of future availability or survival through policy changes,
process loss, host restart, cancellation, or execution failure. Runner hot reload
exists, so the registration snapshot and the policy captured when the Runner
receives the request can differ. Once accepted into the Runner queue, the pending
Job retains that captured policy and configuration generation. Current reload
metadata does not refresh the complete Server policy summary. The receipt must
make this evidence boundary explicit through its policy basis and notice.
Strict confirmation of the final Worker budget would require a Runner budget
acknowledgement or equivalent execution metadata; this series does not add that
wire-protocol extension.

A legacy Runner can omit its policy summary. In that case the actual effective
budget is unknown: the receipt reports `effective_timeout_secs: null`, an explicit
`runner_policy_unknown` source, and `dispatched_timeout_secs` for the known
Server budget. That dispatched value must not be presented as a confirmed Runner
limit. The warning and policy basis remain visible in compact output.

The run_job correction reports requested and effective seconds, the limiting
source, Runner lifetime ownership, and observed command-start truth. It resolves
the previous mismatch in which the Runtime accepted up to seven days, Registry
raw Shell admission rejected values above one day, and the Runner could silently
apply its one-hour policy.

Sources:
[execution budgets](../../src/tool_runtime/structured_execution.rs),
[run_job admission and receipt](../../src/tool_runtime/jobs.rs),
[protocol ceilings and native argv validation](../../crates/webcodex-core/src/runner_protocol.rs),
[Registry validation](../../crates/webcodex-runner-registry/src/validation.rs),
[Runner Shell deadline](../../crates/webcodex-runner/src/webcodex_runner/job_manager/shell_worker.rs),
[Runner policy defaults and reload](../../crates/webcodex-runner/src/webcodex_runner/config.rs),
[request-time policy snapshot](../../crates/webcodex-runner/src/webcodex_runner/transport/poll_dispatch.rs),
[queued policy ownership](../../crates/webcodex-runner/src/webcodex_runner/job_manager.rs),
[registered policy summary](../../crates/webcodex-runner/src/webcodex_runner/transport/registration.rs).

## What survives each boundary

| Event | Ordinary Runner-owned Job | Supervisor-owned detached Job |
|---|---|---|
| One MCP request or observation wait ends | The same Job can continue; retain its identity and exact continuation. | Same logical Job; observation is separate from payload ownership. |
| Control Server restarts | Can reconcile when the same capable Runner instance survives and reports its authoritative inventory. Observation tokens may reset. | Reconciliation still requires exact durable identity and ownership evidence. |
| Runner process is replaced | Ordinary Job is not adopted by a new process. Registry converges to `lost`; it must not replay the command. | May attach to the same execution only after durable state, native process birth identity, and lifetime fences reconcile. |
| Host OS reboot or power loss | No native-process survival guarantee. | No native-process survival guarantee. |
| Result retention expires | Observation/replay evidence can disappear; project artifacts have a separate lifetime. | A retained idempotency key is not a permanent tombstone after reclamation. |

A disconnected Runner normally enters a bounded recovery window. The canonical
default is 120 seconds, with configured bounds of 5 to 3600 seconds. Recovery
does not grant a new Runner ownership of an old ordinary child process.
`outcome_unknown`, timeout, or loss must not trigger automatic re-execution of
possibly side-effecting work. A terminal timeout is not, by itself, proof that
every external side effect has ended.

Sources:
[recovery constants](../../crates/webcodex-runner-registry/src/registry.rs),
[instance replacement and detached transfer regressions](../../crates/webcodex-runner-registry/src/tests/instance_lease.rs),
[detached identity and phases](../../crates/webcodex-runner/src/webcodex_runner/detached_job/model.rs),
[current reliability contract](../agent/job-reliability-and-concurrency.md).

## The 24-hour retention window

The terminal-record retention constant is 24 hours. It is applied after a
terminal observation/completion, not as a universal process runtime limit.
Runner inventory also bounds terminal snapshots to 64; count eviction can
remove older terminal observations before the time window ends. Active execution
is not reclaimed merely because its age exceeds a terminal retention period.

Detached record reclamation requires a terminal record and its elapsed terminal
retention window, with ownership checks under the store lock. Idempotent replay
depends on active or retained terminal history. A caller must not assume an old
idempotency key prevents all future duplicate execution after that history is
gone.

Sources:
[shared inventory and retention constants](../../crates/webcodex-core/src/runner_protocol/job.rs),
[Runner retained inventory](../../crates/webcodex-runner/src/webcodex_runner/job_manager/retained.rs),
[Server terminal receipts](../../crates/webcodex-runner-registry/src/receipts.rs),
[detached terminal reclamation](../../crates/webcodex-runner/src/webcodex_runner/detached_job/store.rs),
[detached reclamation regressions](../../crates/webcodex-runner/src/webcodex_runner/detached_job/tests.rs).

The durable terminal-wait horizon must use the correct kind-specific execution
ceiling. This series also corrects the raw Shell horizon from one hour to one
day. Its upper bound includes execution, maximum recovery grace, and terminal
retention: 86400 + 3600 + 86400 = 176400 seconds. This is a bound on retained wait
registration, not an extension of any execution budget.

## Logs and training artifacts

Server live Job tails are bounded to 256 KiB per stream. Runner reconciliation
snapshots are bounded to 64 KiB per stream. Detached running tails are
checkpointed on a bounded cadence, documented as five seconds, rather than
persisting every output update. None of these surfaces is a complete training
log archive.

Training should write complete logs, checkpoints, metrics, and model artifacts
to stable project files with an application-defined recovery/checkpoint policy.
Job-record retention does not imply deletion of those files, and an observed
tail is not evidence that a full log or final checkpoint was saved. Ordinary
filesystem durability and available storage remain separate concerns.

Sources:
[Server stream bounds](../../crates/webcodex-runner-registry/src/registry.rs),
[Runner snapshot bounds](../../crates/webcodex-core/src/runner_protocol/job.rs),
[training and checkpointing contract](../agent/job-reliability-and-concurrency.md#long-running-native-processscript-jobs).

## Practical duration cases

- **Up to one hour on current mini:** the request must still specify enough
  time; process/script defaults remain 60 seconds. Queued acceptance does not
  prove launch or completion.
- **Longer than one hour:** current mini policy limits the execution to one
  hour, even for a tool that can represent a longer budget. A deliberate,
  separately authorized Runner policy decision would be necessary; this series
  makes the cap visible and does not raise it.
- **Longer than 24 hours:** raw `run_job` remains unsuitable because its tool
  ceiling is one day. Structured native process/script execution can represent
  up to seven days only if Runner policy permits it. Terminal retention is a
  separate clock.
- **Must survive Runner replacement:** duration alone is not enough to select
  detached execution. Use its explicit, authorized Supervisor ownership
  contract and a controlled native executable with literal argv.
- **Must survive machine loss or resume across machines:** application
  checkpoints and a scheduler/restart design are required. That architecture
  is outside this patch series.

The existing detached validator rejects shell command modes. This boundary
remains intact; arbitrary detached Shell input is not a training workaround.
The shared argv validator and its
`process_argv_validation_is_bounded_and_rejects_shell_command_modes` regression
cover both transport sides.

## Decisions and remaining design work

No new scheduler, detached Shell facility, policy bypass, retry framework, or
unbounded evidence store is introduced. The bounded fixes are transparent
run_job budgets, truthful admission state, and the matching durable-wait
horizon.

If long neural-network training becomes a supported product workflow, evaluate
a Runner-owned allowlist of native launchers, explicit argv, stable Supervisor
state, checkpoint destinations, complete log retention, storage quotas, and
human-authorized policy changes. Specify exactly which restart boundaries are
supported before claiming reliability. A future validation should include
controlled multi-hour execution and Runner replacement using disposable
payloads and isolated instances; this investigation did not disturb production
training or restart production services.
