# Runtime maintenance boundaries and Session scale evidence

Follow-up: [SessionStore access order and bounded write coalescing](session-store-access-and-write-scheduling.md)
implements and measures the recency/scheduling work identified here. The numbers
below remain the original #771 baseline, not a current deployment benchmark.

## Scope (2026-09-29)

Worktree: special `/root/git/webcodex-review`; baseline
`ced97978cb250c1d3b01434c2585d27c9ecda931` (merged #770).
This change groups local ownership/projection cleanup with UI observation
lifecycle reuse and a disposable Session scale/recovery experiment. No running
Server or Runner, production ledger, permission policy or deployment was changed.

## Changes and invariants

| Concern | Single owner | Preserved boundary |
|---|---|---|
| Model result shaping | `tool_runtime/result_projection.rs` | Canonical evidence before one-shot projection; no redispatch, authorization or Session selection |
| Git review / Changes retention | Runtime-owned registries | Clones share; independent runtimes isolate; existing caller/source fences and distinct TTL/quota semantics |
| Current validation computation | Typed `ValidationSummary` in `webcodex-validation` | Historical execution success is not current-source proof; no success inferred from absent metadata |
| UI observation scheduling | `useVisibleRefresh` | Only observation timers pause when hidden; resume events coalesce |
| Goal/Session read incarnations | `ObservationRequest` | Slow independent reads do not cancel each other; canceled responses cannot affect a replacement |
| Session persistence | Existing SessionStore and writer | Exact durable identity and explicit close; no automatic Active eviction or storage-engine migration |

The result projection relocation preserves the existing policy expressions,
including exact pending continuations, effect uncertainty, permission denials,
read budgets and validation evidence. Execution binding and pre-record result
expectations remain in dispatch. Existing projection regression tests move with
their implementation rather than being rewritten to approve a different output.

The two Git registries no longer live in `static OnceLock` instances. Replacing a
Runtime starts a new in-memory registry, and dropping its final clone releases
that registry. This is not a guarantee of persistence across process restart;
it does not change content-derived snapshot identities or relax reauthorization.
Live and sealed Changes retain their independent numerical quotas from #769,
now scoped per Runtime: 32 sealed / 8 per caller, and 8 live / 2 per caller.
Clones share those budgets; distinct runtimes have independent budgets, so this
is not a process-wide total cap when multiple runtimes are deliberately created.

Validation calculation now builds a typed summary once, reads its fields,
performs current-source classification, then serializes the existing public
shape. The private current-evidence enum has Unknown/NotRun/Failed/Unproven/
Expected/Inconclusive/Stale but no Passed variant. Public JSON ports consumed by
handoff and continuation are intentionally not all redesigned in this patch;
this removes the self-serialization round trip without a new cross-crate DTO
hierarchy or duplicated validator.

UI explicit refresh queues at most one further read per occupied slot. Periodic
refresh instead skips occupied slots. Selection/disable/unmount cancel obsolete
reads and queued follow-ups. Window collaboration resumes through the same
150 ms coalescing path as inventories; its hidden read cancellation and exact
uncertain send identity remain independent. Agent renewal does not pause when
its inventory becomes hidden. No mutation is automatically retried.

No new external dependency, workspace crate, user configuration, model tool,
MCP schema, direct inventory or App resource URI is introduced. Server/UI
implementation changes do not inherently require a Host schema change. Where a
future public change is intentional, new windows use the new schema; operations
already launched must still preserve exact Job, snapshot and recovery identity.

## Session scale and recovery experiment

Run the actual SessionStore and production snapshot writer on disposable,
synthetic, minimally populated Sessions. For each size: create in memory, clone
the retained ledger under its store lock, serialize/replace a temporary file,
restore with a deliberately small hot-capacity target of 16, query 200 exact
Sessions, explicitly close one Session and flush, then reopen and verify every
identity and the one Closed state. Active identities are not discarded to meet
the hot target. Each case has its own temporary file.

```sh
cargo test --locked -p webcodex-workflow-session --lib session_store_scale
cargo test --locked -p webcodex-workflow-session --lib session_store_scale_and_recovery -- --ignored --test-threads=1 --nocapture
```

Observed once on special using the ordinary unoptimized test profile:

| Retained minimal Sessions | Ledger bytes | Snapshot under lock (ms) | Serialize + replace (ms) | Restore (ms) | 200 exact queries (ms) | One close + flush (ms) |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 71,416 | 0.185 | 3.999 | 6.631 | 2.263 | 4.363 |
| 1,000 | 714,916 | 1.814 | 37.800 | 67.549 | 16.405 | 46.946 |
| 10,000 | 7,158,916 | 20.484 | 374.932 | 680.137 | 171.395 | 398.021 |

All exact identities and lifecycle checks passed. After closing one of 10,000
Sessions, the resulting full ledger contained 7,159,516 bytes; this is file size,
not a measured physical disk-write count or number of writer cycles. The regular
small smoke and corrupt-copy tests verify explicit recovery errors without
replacing the damaged file merely by observing it.

These are a diagnostic baseline, not a production capacity limit, optimized
benchmark or percentile SLA. Minimal Sessions do not model full 2,000-event tails,
message bodies, concurrent traffic, native Windows/macOS storage or large cold
histories. RSS, concurrent lock contention, abrupt machine power loss, and fsync
latency were not measured. Successful flush/reopen is not a power-loss durability
claim. The existing temp-file/flush/rename writer is unchanged.

The measured growth justifies investigating full-snapshot write amplification
and linear LRU touch cost with representative retained data before selecting an
incremental storage design. It does not justify deleting Active identities,
blindly raising limits or migrating every domain to another storage engine.

## Maintenance checks

For each future refactor, identify one authoritative fact producer, its current
consumers, and its reset/expiry behavior. Keep implementation-only cleanup separate
from deliberate wire changes. Use source- and authority-bound fixtures for cache
reuse; keep queries that refresh/materialize state distinct from passive reads.

A useful review asks whether a presentation-only change touches execution,
whether a query must parse another consumer's JSON, whether an independently
constructed Runtime silently shares state, and whether automatic UI work can
cancel or replay a user operation. Preserve existing behavioral tests and add
small independent tests at the ownership boundary, rather than pinning module
names or duplicating complete tool registries in assertions.
