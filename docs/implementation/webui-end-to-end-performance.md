# WebUI performance and Window workflow continuation

Date: 2026-11-04. Starting source:
`7e308d9edcb6d091c2a161e7bd03d253c71ebe2b` on
`perf/webui-end-to-end-20261103`, continuing PR #1357.

## Problems addressed

The previous iteration made Overview and Project polling more frequent but did
not establish a request-load budget. It also left four expensive or fragile
boundaries: HTTP Project inventory reused the model-facing MCP projection,
existing short Project references still opened a write transaction per row,
an indefinitely stalled observation could occupy a hook's only request slot,
and opening a Window-associated Session replaced the Window workbench.

This continuation changes those boundaries together. It preserves the existing
Server/Runner transport, Project identity and authorization rules. Browser
freshness uses bounded HTTP observations of Server-owned registry facts.

## Request lifetime, freshness and authority

Read-only Runtime v2 adapters opt in to a common observation boundary. Identical
in-flight requests share one transport within the exact client/credential and
observation generation. Completed responses are not cached. Independently
constructed clients and browser tabs do not share requests.

The boundary allows at most 64 distinct pending observations and finishes each
within 30 seconds, including a response whose headers arrived but JSON body
never completes. Cancellation settles the local Promise independently of
whether the transport honors AbortSignal. Canceling one subscriber leaves the
others running; canceling the last one releases the slot and aborts the fetch.
Late responses cannot complete an already retired observation.

Credential changes retire existing observations. The replacement authority's
first render cannot expose old Overview, Project or Session facts. A late 401
from the old credential cannot lock the new workspace. A hung Window read is a
recoverable transport error, not evidence that the Window is forbidden.

Writes retain their existing outcome, delivery identity and explicit retry
rules. They are not coalesced, automatically retried or assigned the read-only
deadline. The observation generation advances before and after every write,
including uncertain failures and Project registration through a scoped API base.
An explicit post-write reconciliation therefore cannot join a pre-write or
during-write read retained by another subscriber.

Inventory scheduling coalesces pending refreshes, pauses while hidden and
backs off after failures. Primary/full Overview runs at 10/30 seconds, Projects
at 30 seconds and Work's Project catalog at 60 seconds. New optional Runner
summary fields expose the registry's existing inventory generation, sync state
and update time. While observations are healthy, a changed inventory invalidates
the mounted Project list without waiting for its fallback interval. Automatic
invalidations respect the same failure backoff and denial gate. Ordinary
heartbeats do not invalidate the catalog. Definitive denial stops automatic
refresh; deliberate user refresh remains possible.

## Project selector and reference storage

The HTTP Project selector reads a typed, authorized registry projection rather
than formatting MCP results and then discarding most fields. It applies caller
visibility and selector conditions before retaining the bounded page, and
clones only fields needed by the HTTP response. The immutable registry guard
still uses the existing mutex; this work reduces lock-held projection and avoids
mutation-guard reconciliation, rather than introducing concurrent read locking.
Disabled registrations keep their existing list visibility; family counts use
enabled Projects. Runner online state retains the canonical heartbeat-freshness
rule for both streaming and polling connections.

Short Project references remain available to existing consumers. A warm page
uses a bounded batch lookup rather than one write transaction per row; missing
references are allocated together while retaining principal, canonical identity
and root-fingerprint reincarnation checks. Blocking Store work runs outside
Tokio workers with one admitted worker. A single absolute one-second budget
covers both admission and the blocking result. On timeout, authorized canonical
Project rows and totals remain available without the optional short references.
The worker retains its permit until it actually finishes, even after timeout
or caller cancellation; there is no automatic replay or unbounded detached work.
The budget applies only to this optional Console presentation lookup, not MCP
reference resolution or ordinary command writes.

Primary and full Overview use the same canonical registry family aggregate.
Project lineage omitted by a compact response must not change navigation counts
when a user switches surfaces.

The [Server selector report](project-selector-performance.md) records exact
fixtures, all 18 latency distributions, payload sizes, storage operation bounds
and compatibility checks. With ten concurrent consumers, query-plus-JSON P95
was measured as follows on the same unoptimized test profile:

| Registered Projects | Returned rows | Before P95, ms | After P95, ms |
|---:|---:|---:|---:|
| 500 | 24 | 30.489 | 4.156 |
| 500 | 500 | 490.763 | 67.698 |
| 2,000 | 24 | 57.801 | 7.900 |
| 2,000 | 2,000 | 1,486.836 | 279.677 |
| 5,000 | 24 | 102.509 | 20.460 |
| 5,000 | 2,000 | 1,574.308 | 278.288 |

Each before/after profile measured 570 warm requests. Returned counts and
serialized payload byte lengths matched across all cases; no whole-page short
reference fallback occurred in these normal-load fixtures. This is not a
cross-run response-body hash comparison. HTTP/TLS, browser rendering, live
Runner synchronization and compilation are outside the measured interval.

## Window-associated Session records

A record opened from Window activity is a tab inside the same Window detail.
The Window list and exact selected Window remain mounted. Returning restores
activity filters, expanded content and actual scroll positions. The record
owns its exact client, credential generation, Window, Project and Session;
changing any owner retires the old view and requests.

Window and Session authorization remain separate. Opening a record does not
trigger full fleet Overview or Project Git hydration. Global Session navigation
remains available for entries outside a Window.

The [browser validation report](../ui/window-session-record-validation.md)
covers the same checked-in production renderer before and after at desktop and
phone widths, including a non-first Window and real DOM scroll restoration.

## Measured request-load comparison

The [inventory comparison](../ui/inventory-observation-performance.md) executes
the same harness against the actual starting commit and the patch. These are
virtual-clock request counts in independent React hook trees, not production
network timings or real multi-tab measurements.

| Scenario | Starting source | Patch |
|---|---:|---:|
| One consumer, healthy responses, 25 seconds | 9 requests | 4 requests |
| Ten consumers sharing a client, healthy, 25 seconds | 90 requests | 4 requests |
| Ten independent clients, healthy, 25 seconds | 90 requests | 40 requests |
| One consumer, slow 503 responses, 55 seconds | 12 requests | 3 requests |
| Ten independent clients, slow 503, 55 seconds | 120 requests | 30 requests |
| Hidden document, additional 60 seconds | 0 requests | 0 requests |

Counts include Overview and Projects. Window liveness keeps its separate cadence.
With the fixture's fixed 100 ms successful response, first Project availability
is 100 ms in both versions. This harness supports a reduction in request load;
it does not establish a browser first-paint speedup.

## Read-only production observations and limits

The user's existing Chrome session on mini reproduced the original navigation:
opening a Window's Session record removed the Window shell and switched to the
global Session layout. The sampled runtime requests returned 200 with no
reproduced 401/403. Browser diagnostics exposed bounded request metadata, not
request-duration measurements, so they cannot support a production P50/P95 claim.

The running sf Server reported version 0.5.0, clean build `7cf6442f9dc3`.
It was older than the starting source and was not changed during this work.
One host snapshot showed approximately 1 GiB physical RAM, swap use and high
I/O pressure. The shared database was approximately 2.20 GB and its WAL 617 MB.
Across two persistence snapshots, write attempts increased by 98 and failed
attempts by 31. A bounded 30-minute journal sample also contained 24
database-busy/locked matches.

Those observations justify investigating storage contention, but do not
attribute every failure to Project reference reads, prove a single root cause,
or establish that this patch resolves all production pressure. Controlled
selector measurements isolate the code change; they exclude network latency,
live Runner synchronization and production storage load. The local browser
fixture uses synthetic endpoint delays and does not run a native Server.

No live Server or Runner was deployed or restarted. Production acceptance
requires deploying the matching Server/WebUI and then repeating the user's
workflows and recording real HTTP timing and persistence behavior.

## Validation on the completed continuation

- Runtime v2: 398 tests across 43 files passed.
- Frontend Admin/document component tests, TypeScript checking, production build
  and generated-asset consistency checks passed.
- The production-renderer browser script passed at 1440 and 390 pixels, with
  exact selection, retained activity DOM, scroll restoration and readable tabs.
- Final-source Runtime Console: 71 passed, with three manual benchmarks ignored
  in the ordinary run. The Project selector benchmark was run explicitly.
- Focused Registry selector: 2 passed; Store project references: 4 passed;
  Console deadline/cancellation: 2 passed; existing ToolRuntime/MCP references:
  8 passed.
- Workspace dependency boundaries and stable Rust formatting passed.

## Regression and reproduction entry points

- `frontend/test-v2/runtime-observation-boundary.test.ts`: coalescing, bounds,
  stalled body, subscriber cancellation, credential changes and actual Window
  hook recovery after a never-completing read.
- `frontend/test-v2/runtime-observation-mutation.test.ts`: reads across
  successful, pending and uncertain writes.
- `frontend/test-v2/inventory-scheduling.test.tsx`: queued refresh, backoff,
  denial, authority/filter ownership and inventory invalidation.
- `frontend/test-v2/window-session-record.test.tsx` and
  `session-interaction-safety.test.tsx`: embedded navigation, owner changes
  and delayed read/mutation responses.
- `scripts/ui-smoke/window-session-record.mjs`: production-bundle browser
  checks and inspectable screenshots.
- The linked Server report contains focused Rust test and benchmark commands.

No wall-clock performance threshold is added to ordinary CI. Latency samples,
request counts and functional assertions are reported separately.
