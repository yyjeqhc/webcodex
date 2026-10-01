# Interactive pipe Jobs and coding workflow acceptance

## Scope

Ordinary `run_process` remains native argv execution with a bounded initial stdin
payload followed by EOF, or an explicit null/closed input when omitted. The new
`interactive: true` option holds a **pipe**, admits one normal public Runner Job,
and returns without waiting for input-dependent completion. This is not a PTY,
ConPTY, shell Session, REPL service, detached process, or automatic next-turn host
continuation. Programs must support pipes and flush their output; TTY-only
applications are outside this slice.

`write_job_input` is a model-visible Adaptive Runtime Direct capability paired with
`run_process(interactive=true)`, with `call_runtime_tool` retained as its gateway
fallback. It is not another execution launcher. Output observation and waiting
remain separate read-only operations (`observe_jobs`, `wait_for_job_readiness`);
stopping remains explicit
`stop_job(confirm=true)`. Existing batch read/edit/image and short-ref contracts
are exercised by the same opt-in real MCP/WS workflow fixture.

## Use

```json
{"project":"~p1","executable":"python3","args":["-u","-c","print(input())"],"interactive":true,"timeout_secs":120}
```

Keep the returned Job identity. Observe actual output when it informs the next
input; don't wait for terminal completion while a program is waiting for an
answer that the caller has not yet supplied.

```json
{"project":"~p1","job_id":"<same-job>","input_id":"answer-1","data":"hello\n"}
```

The input result's `state` is `pending`, `written`, `closed`, or `outcome_unknown`.
A successful receipt request can return **pending delivery**; the process is not
thereby complete. `bytes_written` proves only kernel pipe acceptance, not program
consumption, successful parsing or business completion. The final program result
continues to come from the original Job. Close explicitly when appropriate:

```json
{"project":"~p1","job_id":"<same-job>","input_id":"eof-1","close":true}
```

No empty-input polling, signal injection or terminal resize is offered. Initial
`stdin` and named Session SSH resources cannot be combined with interactive mode.
Use normal native SSH for a one-off explicit process if appropriate; retained
named-SSH shell semantics are not merged with this input channel.

## Exact identity and boundedness

Each caller-selected input ID identifies one exact UTF-8 byte payload and EOF
choice within the Job. A replay with the same ID/bytes/EOF returns the current
retained receipt without resending; changed data conflicts. A pending or uncertain
acknowledgement is not permission to invent another input ID. Reconcile only the
same known request or inspect the Job. No model-generated input is automatically
replayed during reconnect, Server restart, Runner restart or Job recovery.

One Job retains at most 128 data-write identities plus one reserved empty EOF.
Each request is limited to 65,536 UTF-8 bytes and a 128-byte ASCII input ID. The
writer has a single pending slot: a different input is rejected while delivery is
pending. Hashes and bounded receipts remain with the retained native Job; input
bodies are not persisted as a replay log. Closure is published after dropping the
owned pipe handle. Partial write failure remains unknown, records the accepted
byte count, and closes the channel rather than retrying the remainder.

The native request waiter is bounded to one second; Server receipt waiting is
bounded separately. This does not extend the process timeout or reset deadlines
on progress. A program that never reads may block its one pipe writer; original
Job cancellation/timeout uses the existing owned process tree to release it.
The stdout/stderr drains continue running independently of input writes.

A missing/queued input channel is a definite not-ready rejection, not an input
retry under a new key. After normal completion, the retained same-key receipt may
still be read; new data is rejected. Runner replacement invalidates the live input
channel and its process-local receipts, while durable Job records remain ordinary
history. Old input cannot be redirected to another process or incarnation. A
Server-only restart can reconcile the same still-live Runner Job; this does not
claim durability of pipe contents or application consumption.

## Authority and compatibility

`job_process_input_v1` is an **additive non-baseline** Runner capability. Old
protocol-generation-2 Runners remain compatible with the Server but cannot be
used for interactive starts/input. Normal argv execution remains available.
An implementing Runner advertises the feature; configuration defaults alone do
not infer it from async Jobs or persistent shells.

Admission independently verifies the current Project and `job:run` authority,
Job visibility, ownership, exact incarnation and capability. Before dequeue,
queued interactive starts and input requests recheck registration, owner/group,
Project presence and capability under the registry lock. Rejected queued input
bytes never cross to a replacement Runner; a rejected queued start is recorded
as not started on its original Job. The native receiver repeats incarnation,
Project and current raw-shell/allowed-root checks before accessing its channel.

Interactive execution has a distinct safe metadata source but uses the existing
public Job kind. It deliberately has no generated validation identity/count
attestation: future inputs can change behavior and are not covered by the initial
argv fingerprint. User-declared purpose remains a report, not proof that a test
recipe ran. Existing structured validation tools retain their original behavior.

Business audit stores input byte count and EOF intent, not data. Metadata trace
uses its existing body-omission rule for `data`; full forensic capture has its
existing privileged scope and should not be enabled casually for sensitive input.
No new credential kind or permission bypass is added.

## Short-reference Work Result

Work Result now accepts an already-issued principal-scoped `project_ref`, after
the same canonical Project/root-fingerprint resolution and authorization as other
project tools. Friendly/ambiguous aliases are still rejected. All mounted-card,
resource, audit and result identities remain canonical; a short ref never becomes
an authorization token or a resource's persistent identity. Unknown, foreign and
root-replaced refs fail closed.

## Verification

`job_manager/input/tests` uses deterministic fake pipes for exact-once retained
replay, conflicting data/EOF, pending delivery/backpressure, partial failure,
handle-close ordering, quota and reserved EOF. Server/registry tests check public
Job identity, permission/capability/Project fences, pre-dequeue revocation and
untrusted receipt validation. Tool schema tests keep default EOF behavior and one
canonical effectful input operation exposed Direct with the ordinary Adaptive
gateway fallback; Job/read observation interfaces remain read-only.

Run after building matching dogfood binaries:

```sh
python3 scripts/e2e_job_input_ws.py --bin-dir target/dogfood
```

This opt-in fixture composes the existing disposable MCP/WS Server/Runner harness.
It verifies short Project/Session refs; batch read, numeric revision fences, edit,
stale rejection and compound search; actual MCP PNG bytes; interactive output
without newline; keyed replay/conflict/EOF; foreign-principal and Project denial;
Server-only restart of the same live Job; a non-reading process, cancellation,
execution timeout, and Runner replacement. It uses a local generated binary
fixture, temporary credentials/files and no external model or paid endpoint.
Its readiness loops own one absolute deadline; they are acceptance-harness waits,
not a recommendation for model heartbeat polling.

### Final Linux verification

With runtime source frozen, the default Server library passed **3,156 tests**
(3 existing ignored), the Runner binary suite passed **973 tests** (4 existing
ignored), Runner Registry passed **311**, tool contracts with all features passed
**276**, and the new Core input-contract selection passed **3**. The native
input-channel cases and Server admission/receipt/short-reference tests are included
in those totals. Formatting, whitespace, Python syntax and all 21 workspace
dependency boundaries passed. No dependency or Cargo.lock update was needed.

One full Runner run on the same final source failed seven existing ACP/transport
timing tests (966 passed). All seven passed in a targeted run without changing
time thresholds or concurrency, then the original full suite passed 973/0/4.
The initial failure is retained as diagnostic evidence, not erased or attributed
to a proven cause. The relevant ACP/transport code and assertions were unchanged;
this is not a claim that those tests' intermittent timing sensitivity is fixed.

The opt-in real MCP/WS fixture passed all workflow and interactive scenarios on
the development build, including Direct input plus same-receipt gateway fallback
without duplicate delivery. Its initial failures revealed fixture assumptions rather
than permission exceptions: it needed explicit 2026-07-28 request metadata for
`_wc`, and the existing terminal timeout status is `timeout`, not `failed`.
Production protocol admission and Job lifecycle values were not loosened.
Deployment acceptance uses a rebuilt clean commit, not an unstated dirty artifact.

The optional `interactive` input/output contract and Direct input capability
modestly increase schema bytes; the administrator direct-inventory soft budget is
**74,000 bytes** with **30 direct tools**. The complete workflow guidance was not
expanded to carry per-tool interaction details; required-input-before-wait guidance
belongs to the process contract. No transport or payload safety ceiling was raised.
Native Windows/macOS execution, PTY/ConPTY, a paid coding agent, and a real Codex
model-performance comparison were not part of this local verification.

Final results and the separately authorized 0917 dogfood deployment are recorded
after source review. No PTY, package changes, global client-schema refresh, or
replacement of special's normal control Server/Runner is implied by this feature.
