# Window history read diagnostics and exact detail summaries

This change addresses the selected Window history path after the existing
[activity-query batching](window-activity-query-performance.md),
[derived inventory work](webui-runtime-read-paths-performance.md), and
[Store/read isolation work](store-post-result-performance.md).
Those earlier changes already provided the indexed event/link page, compact
inventory, one read-only WAL connection, one admitted Console history worker,
and per-read cancellation. They are not new optimizations in this change.

A reported approximately 13-second/503 historical Window request was not
reproduced against production traffic or a production database. The controlled
Console fixture below measures the current call path and admission saturation;
the new diagnostics distinguish possible causes on a subsequent observed slow
request. A 503 remains an unavailable read, never an empty successful page.

## Read flow and work removed

Global inventory keeps its two-phase authority boundary: read compact Project
anchors, authorize them against the current Runner snapshot, then read the
authorized and paginated inventory. It still merges authorized live requests
before ordering/counting/pagination and repairs its complete dirty selection.

Selected Window primary/full details now use that same derived inventory for
their summary. Previously primary detail scanned events once for the summary
and again for its activity page; full detail also repeated the relation query.
The new exact-key anchor query and inventory query use the existing Window
primary keys. Their dirty check/repair is restricted to the requested Window,
so a single detail request does not rebuild unrelated dirty Windows.
Summary fields can describe older authorized history outside the current
bounded activity page, without hydrating that history.

A successful detail read uses two Store admissions. The first reads exact
Project anchors; current authorization remains between that read and content.
The second worker reads the authorized inventory summary, the retained admin
first-seen value, activity, and (for full detail) relations under one existing
five-second budget and permit. Global inventory supplies no extra content work.

Primary detail hydrates one bounded event/link page and continues to defer Job
and Workflow Session detail. Full detail hydrates one event/link page and one
bounded relation page. Code Mode composition remains on the activity detail
path. Admin first-seen fallback and the existing principal/managed-OAuth,
projectless-event, explicit-Project precedence and Session-link authority
rules remain in effect.

Every historical and live Project contributing to a summary is retained in the
request-local visibility cache. The detail response reauthorizes all of them
after awaited activity/relation reads, immediately before final assembly.
Rechecking only the displayed last Project would leak an older revoked
Project's contribution to first-seen time or other summary fields.
No grant/visibility decision is persisted or reused across HTTP requests.

## Diagnostics

The debug targets are `webcodex::phase` and
`webcodex_store::connection`. Phase/outcome labels are fixed at call sites;
the new fields contain durations/counts only. They contain no Window keys,
principal identifiers, SQL text or SQL parameters. Blocking workers inherit
the existing tracing dispatch/span so their phases remain attached to the
request's existing tracing context.

Console decides whether to install SQL profiling from the current
`webcodex::phase` DEBUG target before offloading the worker. With that target
disabled, ordinary reads skip the SQLite profile callback. Explicit Store
`HistoryReadBudget` callers keep profiling enabled by default. Disabling
profiling never disables deadlines, progress interruption, lock observation,
cancellation or dirty-repair policy.

| Phase | Meaning |
|---|---|
| `console_window_primary`, `console_window_full`, `console_window_inventory` | Overall adapter read, including separate Store admissions and authorization |
| `console_history` | One Store admission/execution budget, including early queue timeout and dropped futures |
| `console_history_lane_queue` | Waiting for the single historical-read semaphore |
| `console_history_blocking_queue` | Time between admission and the blocking worker starting |
| `history_connection_wait` | Full polling wait for the read or repair connection, including exhausted attempts |
| `console_history_worker` | Actual blocking worker lifetime; permit remains held until it exits |
| `console_history_store` | Numeric SQL/connection/dirty-repair totals for that worker |
| `history_dirty_repair` | Exact/global derived repair work, outcome, elapsed time and rebuilt Window count |
| `console_window_authorization` | Current Project/principal visibility work and final reauthorization |
| `console_window_assembly` | Activity/summary response projection, including any nested visibility checks |

The worker totals include SQL statement count, SQLite profile time, connection
attempt count/full lock wait, dirty-repair attempts, rebuilt Windows and repair
time. SQL counts come from `SQLITE_TRACE_PROFILE`, not Store method-entry
counters. They include transaction control, dirty checks and repair statements;
progress-handler, busy-timeout and trace setup/restoration are excluded.
SQLite profile time has millisecond resolution and is not a precise sub-
millisecond CPU timer. Worker elapsed time also includes preparation, decoding
and Rust work. Dirty-repair time overlaps its SQL profile time, so these values
must not simply be summed as disjoint buckets. Activity assembly also includes
awaited link/Project visibility checks inside the canonical projection helper;
authorization and assembly are therefore inclusive stage measurements, not
mutually exclusive or additive attribution of the whole HTTP request.

The connection observer now retains the complete bounded polling wait rather
than reporting only the final successful `try_lock`. Admission and request
phase guards log on drop as well as normal completion. A request abandoned
while waiting therefore produces cancellation diagnostics without starting SQL.
An executing worker records its final work even if the HTTP future has already
timed out or been dropped.

## Resource and failure contracts

The budget is still **five seconds per `store_read::run` call**, including its
history-lane queue, blocking queue, connection wait and synchronous work.
It is not a five-second deadline for the entire multi-stage HTTP request.
The two detail Store calls and authorization/projection can therefore make an
HTTP request exceed five seconds. The phase breakdown is needed to distinguish that
case from a single saturated lane.

The semaphore capacity remains one and the Store still has exactly one
read-only WAL history connection. No budget, connection pool or retry policy
was enlarged. Dropping/timing out an HTTP future cancels only its own budget.
The blocking closure owns the permit until it finishes, and SQLite's bounded
progress interruption restores connection policy before another reader uses
it. Dirty repair uses the existing writer transaction and rolls back on error.
Concurrent invalidation still fails closed after the existing bounded repair
attempts.

## Controlled measurement

Environment: MSI Windows, AMD Ryzen 9 7940HX, 32 logical CPUs, Rust 1.97.
The explicit ignored Console benchmark uses a disposable database with 400
Windows, eight Projects, four principals plus bootstrap, 101,751 events, and a
target Window with 2,000 synthetic activity events plus its bootstrap seed and
one live request. Fixture creation is excluded from read timings. The fixture
calls the authorized Console query functions in process; it does not measure
HTTP transport, response serialization or browser rendering. Each primary/full/inventory operation uses
31 samples; the mixed workload has four rounds of 16 concurrent tasks.
A controlled five-second lane occupation tests 16 waiting requests and recovery.

The baseline was retained as a compiled executable from
`93d5911587afbb75b6ecf61051e79583ff59597e`, allowing adjacent runs of the
same fixture without reverting or recompiling the shared checkout. Its SHA-256
is `174ACF0BF24A2B9CA5CF1E1C1AF78C4458F7B0CE1BCC54E0883B5DB416ED872C`.
The final changed executable's SHA-256 is
`8DA4A45CACDFE745010DB0B97BB12524B8EB2F3C902FD78CED023D7A82A4C5A0`.
Both are debug test executables. Other task-owned build/test workloads were
paused for the final adjacent before/after run; Console debug profiling was
disabled in the timed processes.

| Operation | Before p50 ms | Before p95 ms | After p50 ms | After p95 ms |
|---|---:|---:|---:|---:|
| Primary detail, one live request | 5.205 | 7.158 | 5.360 | 8.277 |
| Full detail, 2,000 returned events and one live request | 52.670 | 63.593 | 27.825 | 29.170 |
| First 50 inventory rows | 42.575 | 53.031 | 37.824 | 48.404 |
| 16 mixed tasks, four rounds | 292.925 | 373.695 | 159.469 | 322.939 |
| Saturated admission, 16 waiting readers | 5001.095 | 5001.136 | 5001.733 | 5001.791 |

All ordinary requests succeeded. The saturated case deliberately returned
503 for all 16 readers on both versions, entered no Store queries, and recovered
on the same lane after the occupying worker exited. Initial repair plus inventory
measured 1241.449 ms before and 1173.595 ms after; it is excluded from warm samples.

Full-detail p50/p95 fell by about 47%/54%; mixed-task p50/p95 fell by about
46%/14% in this pair. Primary detail did **not** become faster: its median
remained close and p95 rose by 1.119 ms. Inventory and tail timings varied
between runs; this change does not claim an independent global-inventory
algorithmic speedup or a universal latency guarantee.

The deterministic test counters describe Store API entries, not SQL statements:

| Operation | Before history / inventory entries | After history / inventory entries |
|---|---:|---:|
| 31 primary requests | 62 / 0 | 31 / 62 |
| 31 full requests | 124 / 0 | 62 / 62 |
| 31 inventory requests | 0 / 62 | 0 / 62 |
| 64 mixed requests | 64 / 64 | 32 / 128 |

The derived summary replaces duplicate history reads with exact inventory reads;
the extra inventory entries must not be mistaken for free work. Separate
per-Database budget-entry assertions prove that primary and full details each
use exactly two admitted Store workers for both ordinary and admin callers.

### Changes rejected during measurement

An initial candidate added queue crossings: primary p50 rose from 4.242 to
6.927 ms and mixed p95 from 308.084 to 392.947 ms in an adjacent run. Combining
authorized content work into one worker addressed those extra admissions.

A later experimental first-seen `MIN` query passed small-fixture equivalence
and query-plan tests but regressed primary reads in a controlled
old/new/new/old sequence: old medians were 4.563 and 4.430 ms, while the candidate
measured 30.080 and 24.084 ms. That query and its new API were removed. The final
implementation retains the original admin summary query for first-seen and
the final pair above confirms removal of the 24–30 ms regression. No new index
or forced query plan was introduced.

These observations explain the final choices rather than asserting a production
13-second diagnosis. Host scheduling, cache state and debug builds limit how
far the measured percentages can be generalized.

Reproduce the controlled Console benchmark explicitly:

```sh
cargo test --locked -p webcodex --lib runtime_console_http::tests::window_history_benchmark::window_console_load_benchmark -- --ignored --exact --nocapture --test-threads=1
```

The benchmark is intentionally ignored in ordinary CI; there is no wall-clock
performance threshold.

## Regression evidence

The focused Store regressions verify four profiled statements for a populated
event/link page at both one and 40 returned events, and eight for the clean
exact anchors/inventory pair. Those totals include transaction control.
The dirty fixture proves that the exact request repairs one Window and leaves
an unrelated dirty Window pending; global inventory subsequently repairs it.

Console regressions verify one history entry for primary detail and two for
full detail, exactly two admitted budgets for ordinary/admin primary/full
reads, exact summary timestamps, all-anchor revalidation after an older
Project is revoked, and privacy-safe timeout/cancellation/recovery diagnostics.
The enabled DEBUG fixture checks an actual numeric SQL count; a separate
profiling-disabled fixture still verifies deadline interruption and recovery.
A separate two-Database fixture verifies that an unrelated repair on the same
blocking thread cannot contribute to the active budget's diagnostics.

During validation, the trace-capture fixture passed alone but lost events under
parallel tests. Tracing's single-dispatch call-site registration used another
test thread's empty default. The fixture now retains a second inert Dispatch
while capturing, without changing the process-global default or serializing
tests. The full parallel Console filter then passed all 77 active tests.
Existing coverage retains current authority, hidden events, Session links,
Code Mode composition, read-only WAL behavior, event/link snapshot consistency,
queued admission timeout, executing cancellation, rollback and reader recovery.

Focused commands:

```sh
cargo test --locked -p webcodex-store --lib window_activity
cargo test --locked -p webcodex-store --lib history_budget
cargo test --locked -p webcodex --lib runtime_console_http
cargo test --locked -p webcodex --lib --features experimental-code-mode runtime_console_http::tests::window
python scripts/check_markdown_links.py
```

This is source/test evidence. Production Server/Runner deployment, live
multi-tab traffic, browser first-paint and a production 13-second reproduction
remain separate validation work.
