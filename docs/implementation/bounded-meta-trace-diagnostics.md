# Bounded metadata trace diagnostics

## Scope and delivery

This continues the interrupted `feat/bounded-meta-trace-diagnostics` work preserved
by the operator as `41d1c23` on base `6524f559a85d2509b7379c472bd63ceb933cd57a`.
Completion took place on xa `/disk6/xie_rh/jialin/git/webcodex`, without rewriting
that temporary commit. The xa checkout's existing `origin` points to a fork;
`upstream` names `yyjeqhc/webcodex`, which owns the continued branch and PR.

The separately authorized xa Runner dogfood deployment installed the already-built
`0.4.3 / 6524f559a85d / dirty=false` binary, not the trace feature being developed.
Only `webcodex-runner-xa.service` was restarted. The existing config hash was
unchanged, `webcodex-runner-xrh.service` retained its original process, and the old
binary remains under `~/.local/state/webcodex/xa-upgrade-6524f559-20260929/previous`.
The deployment ran through a detached native launcher and separate systemd user
unit, outside the replaced Runner's control-group lifetime. A later observed
Runner replacement interrupted a frontend check; the stopped check was rerun,
not reported as success. No Server deployment, service-config edit or production
ledger/trace mutation was performed for feature testing.

## One evidence path, not another business ledger

ActionAudit remains the call/time/Project/Window index. The only store change is
an idempotent partial time index and a parameterized bounded reader. Detailed
metadata uses the existing trace directory, ownership checks, size/retention
accounting and fail-open background queue. Full raw payloads remain separate
compressed files. Existing safe audit allowlists are not replaced by these
operator diagnostics, and query contents are not recursively audited as bodies.

MCP and the hosted Console share `ToolRuntime::read_tool_trace_diagnostic`. Both
require canonical administrator scope; knowing a trace/Window ID grants no
permission. MCP additionally retains the existing Stateless diagnostic extension
admission; the Console retains same-origin JSON admission. The shared read does
not create a Workflow Session, select an implicit Session, append evidence, or
redispatch a tool. Query without a Window returns candidates; missing Window
identity stays null. Counts derived from returned calls explicitly say so.

## Capture boundaries

Capture uses borrowed input and bounded traversal, not full JSON clone/hash/
serialization before truncation. Per diagnostic: at most 8 KiB serialized,
128 nodes, depth 6, 32 object fields, eight array items and 768 bytes per text
preview (UTF-8 safe). Internal byte reserves can shorten text further; if the
final bounded object still exceeds the ceiling, an explicit budget omission is
returned. Arguments, context choices, paths, filters and bounded command/argv are
real data. Large body fields (including stdout/stderr tails and read/search text)
are omitted with sizes when available. Credential-labelled fields are omitted,
but arbitrary inline secrets in a command string are not detected. Operator
access and private storage remain necessary even without full mode.

`_wc` is projected once, ahead of ordinary arguments, including malformed envelope
shapes. Gateway outer entry and inner target are distinct. Unsupported tools get
only their explicit invocation sidecar, not a guessed per-tool body policy. The
trace-reader and continuation suppression rules remain independent and stronger.

`kernel_arguments` describes parser input, not proven Runner-effective arguments.
Producer-reported canonical normalization/effect/timing facts are captured before
model compaction. Final response receipts describe materials actually returned,
including budget loss, incomplete instruction scans, source scope/fingerprint,
returned content byte count and truncation/read continuation. Actual instruction
text is never copied into metadata. No stage claims downstream receipt or model
reading. The existing semantic telemetry now recognizes `not_observed` instead
of converting that valid status into null.

Metadata records Runner request identity/build routing, result acceptance and
terminal Job correlation only; it does not serialize each Runner log update.
The original authoritative ownership checks still precede accepted receipts.
Diagnostic readers/polling do not gain large request/result body capture.

## Reads and incomplete evidence

The trace index read is private-file and UUID checked, capped at 2 MiB / 4,096
events. Appends and index reads share the existing I/O lock, and bounded file reads
prevent an out-of-process growth race from allocating an unbounded index. It is
read on blocking workers, not the asynchronous HTTP request executor. A page
admits at most 64 events and 64 KiB of serialized event entries (small envelope
metadata is additional); a single event above that limit is rejected explicitly.
Payload entries expose indexes, not native payload paths. Explicit full reads
retain digest verification and the 256 KiB uncompressed read ceiling.

Capture mode controls new capture, not permission to read already-retained data.
Missing files are not treated as empty successful traces: `trace_not_retained`
returns possible not-captured/dropped/expired/evicted causes. Busy/corrupt indexes
have separate errors. Aggregate process-since-start loss counters are visible but
never claimed as per-request completeness. A present handler-return event proves
only framework handoff; absence is inconclusive. No historic journal backfill,
durable delivery proof or exact per-trace dropped-event accounting is introduced.

Query ranges default to 24 hours and cap at 31 days, with limit <=64 and offset
<=10,000. The effective range is returned for subsequent pages. Offset paging is
over the live retained ActionAudit set: late rows and pruning can change it. It is
not a frozen report or a stable cursor across concurrent mutations. No Project,
principal or time heuristic manufactures missing Host Window identities.

## UI lifecycle

The Work/Window sidebar exposes explicit time/Project/tool search; no hash is
required. Existing Window calls can expand the same diagnostic reader. The read
hook has no timer or retries. Expansion loads one metadata page, collapse reuses
that cached page, refresh is explicit, full payloads require another explicit
click, and the view stops at 240 displayed events. Superseded/aborted reads have
a distinct return state and cannot clear or append to newer evidence. Switching
trace identity remounts its cache. HTTP failures keep authorization and missing
data visible rather than appearing as empty successful queries. Existing Window
refresh, message delivery and Session selection lifetimes are not altered.

## Validation approach

Focused Rust tests cover bounded actual argument/sidecar projection, body and
credential omission, malformed envelopes before parsing, final context receipts,
UTF-8/size bounds, metadata read while capture is off, cross-trace corruption,
admin/invalid-selector checks, query paging/missing Window and the shared HTTP
permission boundary. An actual MCP adapter test covers direct, gateway and bad
`_wc` requests without raw payload files. Existing full capture/reader and Job
correlation regressions continue to protect the old forensic path.

Frontend tests cover explicit-only reads, cached re-expansion, paging, explicit
full-payload reads, stale exact-ID results, permission errors, hash-free search,
and reuse of the returned time interval. Final results and commands are recorded
in the PR; initial compile checks on intermediate source are not final evidence.

## Final validation

On xa, with the implementation frozen:

- `cargo test --locked -p webcodex --lib trace` passed, followed by the full
  default Server library: **3,124 passed, 3 ignored, 0 failed** (115.16 s test
  phase). The full suite includes the trace, authority, Console, schema budget,
  existing full capture and protocol-adapter regressions; counts overlap.
- Frontend TypeScript check and full runtime Vitest: **156 passed across 24
  files**. Build and `check:dist` passed; generated Runtime/Admin bundles are
  included. The four new explicit diagnostic-read tests are part of that count.
- Rust formatting, Git whitespace and the 21-package workspace dependency
  boundary checks passed.
- The retained query uses one indexed observed-time expression for both ordering
  and filtering, including legacy timestamp fallback. A regression gives coarse
  audit time a different value and verifies exact millisecond range selection.
- No full workspace/all-features suite, native Windows/macOS run, Server
  deployment, live full-vs-metadata cost benchmark or browser E2E was performed.

Initial failures were fixed without loosening production boundaries: the saved
WIP had a missing dispatcher arm; the new test module needed an explicit path;
query-mode discovery changed the old required-trace assertion; Bootstrap fixture
credentials were not suitable as a non-admin test identity; and the adapter body
fixture was reduced below the existing HTTP size ceiling while still exceeding
the metadata detail budget. The request-size limit and authority checks remain
unchanged. No flaky retry or ignored test was used to hide these failures.

## Deferred

No runtime toggle, per-Window capture lease, automatic escalation to full, generic
OpenTelemetry pipeline, new Direct tool, alternate trace database or snapshot
report subsystem is added. Per-Window/time/tool-scoped full arming is a separate
operator-control feature. This patch solves persistent metadata evidence and
on-demand discovery/inspection first; it cannot reconstruct unrecorded bodies.


## 2026-10-27: request finalization and capture loss

The preceding sections record the original delivery. The current implementation
adds explicit request-tail evidence without changing ActionAudit's response
handoff clock or Window continuity semantics.

The MCP adapter owns the lifecycle. `request_received` starts when that adapter
is entered (after upstream middleware), `request_parsed` follows ingress JSON and
protocol validation, and `dispatch_finished` ends tool dispatch, not a detached
Job's lifetime. `response_serialized` describes the constructed response and
`handler_returned` retains its existing framework-response handoff boundary.
The synchronous finalizer then persists ActionAudit and completes Window evidence;
`audit_finalize` reports completed/failed/unavailable and `request_finalize`
covers the whole finalizer. The lifecycle guard's final `handler_finished` records
leaving the adapter after these synchronous operations. Cancelled or unwinding
handlers instead retain `handler_incomplete_drop`. These are observations inside
the adapter, not client receipt, body-stream EOF, network flush, or model reading.
No await is permitted between response handoff and synchronous tail completion
without revisiting this guard contract.

Audit owns its explicit original `server_trace_id`; it does not consult the exited
Tokio dispatch task-local. A bounded synchronous Store observation context carries
that identity through audit persistence and restores nested/thread-local state on
exit or unwind. Store still owns its connection Mutex and wait/hold measurement.
After releasing the connection, a wait **or** hold of at least 100 ms emits an INFO
`store_connection_slow` event, plus metadata when capture is enabled. Slow phase
summaries also work at INFO. Fast calls retain the existing lightweight
observations. The new fields contain only fixed domain/phase/outcome labels,
durations and a bounded opaque trace ID; no SQL, parameters, tokens or bodies.
Other Store callers without explicit context report the domain and timings with
no invented request identity.

Long historical Window/ActionAudit reads could occupy a shared SQLite connection
Mutex, so even a cheap tool's required audit write waited behind unrelated history
work. Separating and bounding those readers in #861/#862 reduced that contention;
it did not make the remaining audit persistence instantaneous. The new tail span
makes such residual waits visible while preserving the response boundary used by
existing continuity consumers.

The writer queue remains **64**, with nonblocking `try_send`. A single background
writer serializes filesystem writes, compression, retention/budget maintenance
and occasional accounting reconciliation. Sustained producer traffic or slow disk
can exceed its service rate; increasing queue capacity alone only postpones loss.
This change diagnoses loss, not a throughput guarantee: queue/budget/write failure
counters remain process-wide, and a separate try-lock map coalesces known losses
for at most **128** bounded trace IDs. The writer drains those markers outside the
business request and before acknowledging a diagnostic flush. Its map and drained
batch are bounded, with no recursive retries if marker persistence fails.

Retained `tool_trace_capture_gap` events set reader/UI `coverage` to `incomplete`
and expose a lower bound `known_dropped_events`. `capture_health.loss_marker_drops`
also counts losses that could not be indexed or persisted. A missing marker,
including after crash, disk failure or map overflow, never proves completeness:
the alternative stays `observed_events_only`, even with `handler_finished` present.
Missing whole traces remain `trace_not_retained`. Capture is not a durable journal.
Queue saturation, bounded marker overflow and a blocked writer are tested without
blocking an actual successful MCP request. Deterministic channels and the actual
Store Mutex prove two concurrent audit waits keep separate trace identities,
response handoff precedes audit, and Handler finish includes the wait. Additional
regressions cover failed audits, cancellation, timeout, Runner disconnect and
normal fast responses.
