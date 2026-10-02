# Window activity query performance

> This note records the September 29 / #791 history-query optimization. The
> subsequent [Work / Projects read-path scalability work](webui-runtime-read-paths-performance.md)
> removes the inventory's per-Window history scans, separates live polling,
> and adds an authority-free derived summary. The historical measurements and
> detail-query guarantees below remain applicable; the inventory limitation is
> no longer the current read path.

The WebUI already loads a selected Window's recent activity before its full
history. That does not remove work on the preceding Window inventory request:
its authorized summaries can scan up to 2,000 events for each candidate Window.
Previously every event separately prepared and executed a Workflow Session link
query while holding the Store connection lock. The event ordering also used
`COALESCE(request_observed_at_ms, window_started_at_ms)`, which did not match the
existing started-time or completed-time indexes.

The Store now reads the event page and its links in two queries, using only the
returned event IDs for the second query. Empty pages need no link query. Both
reads hold the existing connection lock; principal selection, Project visibility,
page limits, per-event link ordering and exact trace lookup remain unchanged.
No authorization cache or additional persisted summary is introduced. Console
summary reads omit Code Mode composition JSON, which is still loaded for Window
activity details.

An additive partial expression index matches the actual observed-time ordering
and event-ID tie break. Opening an older database creates this index through the
existing idempotent schema setup; this adds index storage and initial build work,
but does not alter or delete audit history.

## Local evidence

On 2026-09-29, the same synthetic fixture and unoptimized Cargo test profile read
12 Windows with 2,000 linked events each. Three samples before the production
change (based on `cdfc89ba`) took 1.015, 1.012 and 1.008 seconds; after batching and
indexing they took 0.345, 0.339 and 0.339 seconds. Fixture creation is outside the
timed section. These are local Store query measurements, not production HTTP or
browser page-load latency, and no wall-clock threshold is imposed in CI.

Reproduce the explicit benchmark with:

```sh
cargo test --locked -p webcodex-store --lib window_history_query_benchmark -- --ignored --nocapture
```

Focused regressions cover exact Window/principal/page link selection, empty link
sets, multiple relations, Code Mode reads, exact trace reads, observed-time
ordering, ties, index installation on an existing database, and query plans
without a temporary ordering B-tree:

```sh
cargo test --locked -p webcodex-store --lib window_activity
cargo test --locked -p webcodex --lib --features experimental-code-mode runtime_console_http::tests::window
npm --prefix frontend run test:runtime -- test-v2/window-first-work.test.tsx
```

The Console tests retain current-Project permission checks, revoked-grant
behavior, active request projection and primary/full hydration coverage. The
frontend tests verify that inventory remains usable while selected Window
requests are pending. No frontend polling interval, API response shape, deployed
service or production database is changed by this source-only optimization.
