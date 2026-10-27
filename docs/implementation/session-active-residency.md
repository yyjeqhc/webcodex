# Workflow Session SQLite storage and residency

## Business contract

A Workflow Session is a durable task identity with lifecycle, current instructions,
execution defaults, collaboration state and evidence. Its identity is independent
of how much recent activity the Console displays or the server keeps expanded.
Active and Closed Sessions, events and messages have no automatic count/idle
expiry. Closing a Session denies ordinary business mutations; it does not delete
history or recycle its canonical id or principal-scoped `~sN` selector.

This domain uses the configured Server `webcodex.db`. It does not introduce a
new database service or a generic storage abstraction. Job, Agent, Conversation,
Wake, audit and transport identities retain their existing ownership contracts.
Durable history never grants permission to replay execution.

## Why the JSON design reached its limit

The original bounded task ledger made a full JSON snapshot a simple consistency
boundary: serialize one complete state, replace one file, restore the same graph.
A small corpus made that tradeoff reasonable. Later changes retained long-lived
Active identities, enlarged history and added replay/completion evidence. Moving
Cold Sessions to immutable serialized JSON reduced object expansion, but still
kept all encoded history in RAM. A mutation still rewrote the entire corpus.

The October 2026 sf audit reported roughly 382 MB per snapshot, repeated every
few seconds, with almost all sampled writes in the Session writer and heavy
swap pressure. This is consistent with the source-level cost: O(all retained
history) serialization/write for a small mutation, plus history-sized resident
memory. It does not establish that every byte of kernel I/O came from snapshots.
Increasing coalescing can reduce ordinary writes, but required durability barriers
still trigger full snapshots in that design.

The prior implementation and measurements are historical evidence in
[SessionStore access and write scheduling](session-store-access-and-write-scheduling.md).

## Tables and commit boundary

`persistence/database.rs` owns domain schema version 1:

| Table | Stored fact |
| --- | --- |
| `workflow_sessions` | Exact identity/owner/Project, lifecycle, latest instruction, current state and committed revision; no embedded event/message history |
| `workflow_session_events` | Individually indexed redacted events with stable ids and per-Session ordinals |
| `workflow_session_messages` | Message current state, observation revision, assignment history floor and completion fence fingerprint |
| `workflow_session_deliveries` | Session-scoped keyed delivery identity and payload fingerprint |
| `workflow_session_tombstones` | Imported proofs of deletions by the retired JSON policy |
| `workflow_session_storage` | Domain version and atomic legacy-import marker |

A mutation registers its generation under the same mutex as live state. One
background writer snapshots only dirty Sessions, pending event deltas and changed
messages. Events already on disk are untouched unless that exact event is updated
(for example, retained permission evidence). It releases the store mutex before
SQLite I/O. One transaction commits all rows in that batch with WAL and
`synchronous=FULL`; identity checks and expected previous revisions prevent stale
writers from overwriting a newer committed Session.

Ordinary notifications coalesce for 20 ms from the first pending notification.
A durability barrier bypasses that delay and asynchronously awaits its exact
successful commit generation. Observing an already committed generation does
not write. Later concurrent mutations remain dirty after an older snapshot is
acknowledged. Failed commits do not advance the watermark or evict dirty data;
the next request/barrier can retry. Cancellation is neither rollback nor replay
authority. Final owner drop makes a bounded final drain attempt.

Creation/resume responses confirm persistence before exposing successful new
Session state. Message delivery, completion, replacement/withdrawal and observation
tokens retain their explicit commit fences. A failure after mutation reports
uncertainty and preserves the exact identity for recovery. SQLite commit durability
still depends on the filesystem/storage honoring synchronization; tests cannot
promise survival of defective hardware.

## Memory and recent activity

Startup loads current-state metadata and committed revision locators, not event
or message histories. SQLite Cold entries hold that metadata. Exact reads load an
ephemeral record; mutations hydrate the selected identity. Only clean committed
records can leave the expanded cache. Closed clean records become Cold; Active
records use the existing access order and a default 16 MiB estimated byte target.
`open_database` accepts the cache byte target explicitly.

The recent event window is 2,000 by default; old event rows stay in SQLite. The
latest instruction is also current state, so a long task does not lose its task
boundary when that instruction leaves the window. Recovery projections label an
incomplete attempt `attempt_history_outside_window`; they do not treat missing
intervening evidence as a complete attempt. Console detail defaults to 100 recent
activities, at most 2,000; model summary limits remain unchanged. These are response
and working-set limits, not destructive history retention. This change does not
add a full-history browsing/export API.

The cache target is **not a hard process RSS ceiling**. The identity/current-state
index is proportional to Session count; process-local instructions, dirty batches,
in-flight snapshots and SQLite/allocator overhead are additional. A selected
Session's collaboration messages are still materialized together for the existing
atomic assignment/ACK/completion logic. A large message board can therefore cost
transient memory and query CPU, even though commits write changed messages only.

Runtime admission awaits a commit when 512 pending change units accumulate or a
prior persistence failure is outstanding. Already running work can still append
terminal evidence. This is backpressure on new runtime work, not a hard bound on
all concurrent effects or on direct synchronous library callers. Serialized bodies,
model responses, ACK references, assignment replies and wait durations retain their
own limits. A full disk produces an explicit failure, never silent history deletion.

## Identity, restore and import

Reference lookup uses identity metadata; it does not load historical payloads.
Principal, Project and incarnation checks still apply after selector resolution.
A known owned identity whose backing storage cannot be loaded is unavailable,
not newly unknown/expired. Failed storage never creates a replacement Session.

An existing JSON-v2 `sessions.json` is imported once in one transaction with its
completion marker. Structural corruption, unsupported formats and identity
conflicts abort the import; startup fails closed rather than starting an empty
store. Existing privacy sanitization applies to imported evidence. The source
file remains untouched and is never a second live persistence target. Imported
loss floors and tombstones describe already missing history; upgrading cannot
recover previously deleted evidence. No new count-based tombstones are generated
and imported proofs do not expire through cache churn.

Subsequent starts use SQLite and do not re-import even if the source JSON changes.
The retained JSON is a pre-cutover copy, **not** an up-to-date rollback database.
Returning to a JSON-only binary requires an explicit data recovery plan; otherwise
post-cutover changes would be absent. Normal backups must include a consistent
SQLite snapshot (including committed WAL state), not merely copy a live main file.

## Diagnostics and validation

`session_store` reports cache target, recent event limit and Hot/Cold counts,
without misleading Session/history/message count limits. Persistence diagnostics
separate ordinary notifications, durability barriers, successful/failed attempts,
pending generations, and snapshot-lock/write duration. `logical_payload_bytes`
counts changed JSON payload only; it is not physical I/O. `database_bytes` and
`wal_bytes` describe the shared database files, not Session-only attribution.

Focused regressions cover more than 10,000 events without rewriting old rows,
non-promoting reads, latest instruction retention, 500-message keyed replay,
exact references after 250 Closed Sessions and restart, interleaved snapshot
acknowledgement, multi-Session transaction rollback, failed-write retry,
nonblocking backpressure and cancellation, strict one-time import, indexed
identity mismatch and abrupt process exit with committed WAL state. Existing
privacy, assignment, completion, validation and authorization suites remain
applicable. These tests establish correctness and changed-payload scaling, not
production RSS, p95 latency or physical write throughput on sf.
