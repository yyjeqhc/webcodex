# Built-in result projection registry

## Purpose and boundary

The result projection layer is a bundled Rust composition point, not an external
result modifier API. Each tool family owns both capture of its request facts and
the ordered transformation of its model-facing result. The registry selects the
family without a central projection enum followed by another central match.

`ModelFacingProjectionPlan` keeps its existing capture/bind/project interface.
The Kernel continues to capture canonical telemetry, trace evidence and audit
output before consuming the plan. Session recording, canonical audit storage,
execution certainty and dispatch remain outside the projection registry. Optional
Window/peer/Job sidecars are still attached afterward by their existing owners.
Neither that ordering nor the Kernel postprocess implementation changes here.

## Ownership

`result_projection/registry.rs` contains an immutable list of six capture
functions. Its entries select Agent wait, Job readiness, guarded edit, execution,
read, and search projections. Each capture function returns either no match or
one owned, typed projection. Registry tests check all 17 previously projected
ToolCall variants have exactly one matching owner.

Family implementations live in `execution.rs`, `reads.rs`, `searches.rs`,
`edits.rs`, and `waits.rs`. Existing conservative formatting helpers remain
unchanged. Cargo test postconditions, edit count/dry-run, read snapshots, search
queries/timeouts/budgets and business Session identifiers retain their existing
request-local meanings. Read/search binding uses the exact Project resolved by
the original governance pass; projection cannot re-resolve a shorthand target.

The private projection trait exposes only bind-to-already-resolved-Project and
consume-once formatting. It receives no runtime, dispatcher, permission
evaluator, Session store, or canonical audit handle. A consumed or abandoned
plan releases its own state through ordinary Rust ownership. There is no global
mutable registration, lock, Arc-held service, or reload/unload state machine.

These are trusted bundled formatters with access to the model-facing ToolResult,
not a safe third-party callback boundary. External modifiers would require a
separate restricted output API and failure/budget isolation before admission.

## Cost and non-goals

Unmatched tools keep the allocation-free no-op plan. A matched stateful projector
uses one owned trait object rather than the previous inline enum; zero-sized
wait projectors carry no heap payload. This is a maintainability refactor, not a
claimed latency optimization. It adds no process/RPC boundary, registry lock,
background work, cloned runtime service graph, or extra tool execution.

The exhaustive authoritative ToolCall router is intentionally untouched. This
registry does not replace execution, validate permission, interpret retry safety,
select a Session, resume an Agent, or change durable Job state.

## Incremental adoption

This PR is independently based on main. It neither imports nor depends on the
context/guidance registry refactor. It works with old context/guidance/UI code,
and those layers may migrate separately. All ToolCall names, schemas, canonical
results, persisted formats, native plugins and Kernel entrypoints remain intact.
There is no dual execution path, migration flag, partial registration window,
Host schema refresh requirement, or database migration.

Rollback is an ordinary code revert; no plugin catalog or stored data conversion
is needed. Deployment and production smoke remain explicit separate operations.

## Regression coverage

Focused tests cover exclusive registration, unmatched results, consume/drop
ownership, Cargo policy capture, existing sparse execution results, uncertainty,
pending Jobs, canonical audit/telemetry ordering, guarded edits, read/search byte
budgets, snapshot fences, exact Project binding, and actionable continuations.
The registry does not weaken any of those existing assertions.
