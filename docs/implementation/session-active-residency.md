# Active Session residency

## Problem and scope

Active Workflow Session identity is durable, but that does not require keeping its
expanded event/message graph resident indefinitely. The previous count target did
not compact any Active record, and streamed restore kept every Active row Hot.
This change preserves the non-destructive identity/retention contract from #693.

The store now bounds expanded records by `hot_session_capacity_target` (minimum
one working mutation target). It uses its existing access order on admission,
not a second CLOCK/SLRU index or a periodic full-history scan. An older record is
serialized into one immutable `Arc<RawValue>` without removing its canonical id.
Ordinary history queries use an ephemeral materialization and do not promote rows.
The streaming loader still validates/sanitizes every row; it now retains each
validated row Cold rather than accumulating all Active expanded trees.

## Boundaries and exact round trips

Cold here means **serialized payload in RAM**, not SQLite or disk demand paging.
Consequently pending/dirty changes are preserved independently of persistence
success, and no Job, Agent, lease, or controller has to pin an expanded ledger.
The store mutex prevents a transition during any record borrow; in-flight callers
own identities/evidence, not an evictable raw pointer. Their later completion,
permission, ACK and validation events hydrate the same exact Active identity.
The existing generation barriers and atomic JSON-v2 full-ledger writer stay intact.

Disk restore is a trust boundary: it sanitizes untrusted/legacy fields and may
shorten historical metadata. Live residency is not a second restore boundary.
Active materialization therefore exhaustively transfers the already-canonical
persisted fields without another sanitize pass. Process-local instruction bodies
and historical no-fence completion entries (neither belongs in durable payloads)
are retained separately and restored losslessly. Cached metadata must match the
payload's identity, owner, Project, lifecycle, guards, mode and update timestamp.
A parse/mismatch failure rejects materialization without inventing a replacement.
Closed history keeps its previous query/rewrite/retention behavior.

Normal message, exact resume, execution-context, event, permission and validation
mutation paths prepare an Active target before accessing mutable records. Query,
lifecycle, authority, Closed retention and persistent-shell teardown semantics do
not infer a new Session or broaden access. A failed compaction retains Hot data;
a memory target never authorizes data loss.

## Observability and cost

`SessionStoreStatus.active_cold_sessions` separates durable Active identities from
materialized ones. `cold_payload_bytes` counts only encoded Cold JSON, not heap
live bytes, metadata, instructions, allocator overhead or concurrent snapshots.
`capacity_evictions` continues to count Closed retention deletions, not compaction.

Compaction/admission does serialize or deserialize one record under the store
mutex and can add latency on a cold mutation. Workload-sized cost must be measured;
no performance speedup is claimed. Repeated history reads intentionally trade CPU
for not retaining a scan's full working set. Existing per-record history caps,
JSON schemas, durable identity and event retention are unchanged.

## Validation and next decision

Tests cover bounded Hot counts without Active deletion, non-promoting queries,
lossless live state, explicit restart and keyed replay/ACK, in-flight completion,
parallel writes with churn, zero target, failed persistence and corrupt payloads.
Existing lifecycle, assignment, tombstone, sanitation and writer suites are kept;
only tests which asserted the old Active-implies-Hot policy change residency counts.

A manual Linux benchmark compares the previous **streaming** Active/Hot restore,
Cold restore, and Cold plus 100 materialized records in separate child processes
using the same generated 512-row/128-event ledger. It reports RSS/HWM and isolated
materialization cost, not production heap profiling or end-to-end tool latency.

Remaining total memory is still proportional to encoded retained history. A true
byte-weighted/disk-backed cache requires a separate design for dirty generations,
atomic per-row commits, migration, recovery and corruption. Do not silently replace
this RAM backing with a stale `sessions.json` offset. Do not shrink the canonical
2,000-event tail to 128, introduce a permanent interner, or switch allocators merely
to fit a target; first measure actual working-set bytes and read/write costs.
