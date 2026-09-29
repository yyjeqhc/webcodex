# SessionStore access order and bounded write coalescing

## Scope

Follow-up to the #771 scale experiment, based on
`b7f56bad1b54283523928a3966724a4585ebcb16` (#772), developed in special
`/root/git/webcodex-review`. This change does not migrate storage or touch a
production ledger. It changes in-memory recency indexing and ordinary background
write scheduling only. Session ledger version 2, external tools, schemas,
authority, event/message retention, and explicit lifecycle remain unchanged.

## Ownership and invariants

`store/recency.rs` owns an internal ordered index. Exact touches/removals are
O(log n), rather than scanning the entire `VecDeque`; a separate set of ordered
Closed positions makes Closed-history counts and oldest-eligible selection
independent from the number of Active Sessions. It is not a second lifecycle
state machine. The store supplies lifecycle under the same existing mutex, and
rechecks authoritative lifecycle before any eviction. Access order is rebuilt
from restored identities, never accepted as user authority or persisted as a new
wire field. Rare counter rollover preserves ordering. The index uses extra
O(n) metadata; this is not a memory-reduction claim.

Creation no longer counts all Closed records on every insertion. Exact reads
still update access recency but do not append evidence, advance business
activity, reopen Sessions or schedule persistence. All Active identities remain
retained regardless of the hot target. Closed retention still evicts only the
least recently used Closed identities. Unknown lookups never create entries.

`store/writer.rs` owns the existing background writer, its dirty-generation
barriers, and shutdown drain. The production ordinary coalescing window is a
private constant of 20 ms, not a new deployment setting. The first pending dirty
mark owns one deadline. Later marks and spurious wakeups cannot extend it; marks
that arrive during I/O form the next pending generation. If that deadline has
already elapsed when I/O completes, the next cycle need not wait again.

Explicit `flush_through(generation)` and last-owner shutdown bypass coalescing.
A flush still waits for its exact generation, not an ever-moving latest target.
It cannot interrupt an in-progress write; lock, OS scheduling and I/O time are
outside the 20 ms coalescing budget. Writer and flush waiters share a condition
variable, so dirty and shutdown notification wake all waiters instead of risking
waking only another flusher. The shared snapshot/write mutex still orders
background writes against the synchronous fallback/test path.

Write completion remains an attempted-cycle marker. Failed persistence is not
reported as durable success: `last_persist_error` is checked by the existing
restart-safe operation boundary. New tests cover failure reporting and recovery,
multiple waiters, pending data on final-owner drop, and exact generation fences.

This intentionally allows up to 20 ms of additional scheduling delay for
ordinary **already asynchronous** Session event persistence. Operations that
promise a flushed result skip that delay. This is not fsync, not an atomic
cross-domain transaction, and not a power-loss guarantee. The same whole ledger
is still serialized/replaced for each actual write cycle. Reducing the number of
cycles does not eliminate per-cycle full-ledger write amplification.

## Measurements

All timings below are one local ordinary unoptimized test-profile observation on
special, not percentiles, production capacity, disk sectors or release-build
throughput. Compilation time is excluded. No benchmark changes the production
Server, Runner, environment variables or ledger. Fixtures use disposable files.

### Same pre-existing minimal-Session experiment, before and after

The before run used the unchanged baseline; the after run uses identical fixture
and recovery assertions. Exact IDs and Active/Closed state round-trip in every
case. Ledger sizes were identical before and after.

| Sessions | Create before ms | Create after ms | 200 exact reads before ms | 200 exact reads after ms | Ledger bytes |
|---:|---:|---:|---:|---:|---:|
| 100 | 1.460 | 1.446 | 2.250 | 1.150 | 71,416 |
| 1,000 | 37.435 | 13.034 | 16.310 | 1.174 | 714,916 |
| 10,000 | 3,183.259 | 126.597 | 169.467 | 1.568 | 7,158,916 |

The scaling improvement is concentrated in access order and retention-index
maintenance, not disk encoding. At 10,000 minimal Sessions, serialize/replace
was 375.375 ms before and 377.592 ms after; restore was 681.281 ms before and
712.540 ms after. One explicit close+flush was 402.555 ms before and 403.520 ms
after. Those paths are not claimed to be faster; restore now also builds the
ordered index. Representative large histories, memory overhead and concurrent
latency distributions require separate measurements.

### Populated-ledger scheduling comparison

One identical seed has 256 Active Sessions, 32 tool events and two synthetic
messages per Session. The seed ledger is 6,682,138 bytes. Each mode adds 128
synthetic read calls (one start and finish each), flushes, reopens and verifies
all exact identities, lifecycles and expected event totals.

Only the private ordinary write window changes between the two modes; both use
the same new implementation and source data. Zero window is a scheduling
comparison, not a separate checkout or a replacement persistence backend.

| Mode | Completed full-file write cycles | Mutations through final flush ms | Final ledger bytes |
|---|---:|---:|---:|
| Immediate (zero window) | 2 | 727.463 | 6,875,674 |
| Coalesced (20 ms window) | 1 | 384.763 | 6,875,674 |

Cycle counts are test-only observations. Timing/savings vary with producer
spacing, scheduler and I/O; no hard performance threshold is imposed in CI.
The experiment is ignored by default. Ordinary deterministic tests separately
prove the first-mark deadline, no automatic query writes, flush/shutdown bypass,
failure propagation, and burst identity preservation. A 60-second injected test
window plus a generous 10-second completion bound verifies bypass without
brittle millisecond assertions or sleep-based coordination.

## Reproduction

```sh
cargo test --locked -p webcodex-workflow-session --lib
cargo test --locked -p webcodex-workflow-session --lib session_store_scale_and_recovery -- --ignored --test-threads=1 --nocapture
cargo test --locked -p webcodex-workflow-session --lib populated_ledger_write_coalescing_experiment -- --ignored --test-threads=1 --nocapture
cargo test --locked -p webcodex --lib session
```

The default Session package suite covers existing authority, retention, replay,
cold state, ledger restore and lifecycle behavior alongside the new tests.
No live restart, abrupt power-loss test, full workspace/all-features run, or
native Windows/macOS verification is implied by these results.

## Validation result for this change

- Full Session-domain library: 218 passed, 2 ignored (the two manual experiments).
- Root Server `--lib session` integration selection: 355 passed, 0 failed;
  the other root-library tests were not rerun.
- Both ignored experiments were run explicitly and passed; new recency/writer
  cases are included in the ordinary Session-domain run, not extra test totals.
- Formatting, Git whitespace and workspace dependency-boundary checks passed.
- Final review checked recency-vs-lifecycle separation, restore behavior,
  condition-variable wakeups, fixed deadline, write ordering, failed writes and
  generation-scoped flush without changing production persistence encoding.

## Deferred work

Per-Session incremental persistence, full validation/closeout DTO conversion,
Active cold residency, and changes to durability guarantees are not part of this
patch. The measured next storage cost remains full-ledger serialization and
replacement. Any later format migration needs its own compatibility, recovery,
corrupt-data and rollback design; it must not discard Active identities merely
to satisfy a residency target.
