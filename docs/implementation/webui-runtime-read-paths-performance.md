# Work / Projects read-path scalability — phase one

Implemented in `special:/root/git/webcodex`, branch `perf/webui-runtime-read-paths`,
from clean `main` / fetched `origin/main` `025702f5bf0d2459436a1d98ca4603368dd7fd35`.
This is the WebUI inventory work, not model payload-size optimization. The older
#791 history-query work is recorded in [window-activity-query-performance.md](window-activity-query-performance.md).
No production database/service/Runner was deployed or restarted; all commits are local.
The requested Store/post-result work is a subsequent, independently committed phase.
Its completed implementation, review corrections and remaining limits are recorded in
[Store and post-result latency — phase two](store-post-result-performance.md).
The measurements and remaining-work section below describe the phase-one boundary.

## Authoritative paths

### Set-oriented Window inventory

`runtime_console_http/window_inventory.rs` reads compact derived Store cells,
not each Window's event history. ActionAudit and its workflow links remain
canonical. Cells partition each Window by exact principal, explicit Project and
the complete canonical workflow-Project anchor set. They retain first/last visible
time, latest activity/status/meaningfulness, last tool/meaningful time and gap
count. A separate compact relation projection preserves distinct Workflow Session
counts. No Project grant or authorization decision is persisted.

Explicit event Project still takes precedence over workflow links. A revoked
Project cannot reveal its Window/timestamp/activity through another link. Ordinary
management callers see genuinely unanchored events only under their own principal;
principal-scoped callers retain their exact filter. Anchors are reauthorized on
every request using one current Runner snapshot and the same visibility/TTL
predicate as exact Project observations, not a cross-request authorization cache.
Authorized live requests merge before total/order/pagination. Hidden rows cannot
inflate totals. Out-of-range empty pages may repeat the compact count projection.
Detailed history still uses the existing bounded event/link query path.

The normal global path uses two Store projection entry points (anchors and
inventory), zero per-Window history calls. The inventory aggregation is one SQL
statement. These are not claims that transaction/dirty-set checks add no SQLite
instructions. Query plans never open historical event/link tables in steady state.

### Active Job index

`JobRecords` owns canonical records plus active/dirty ID sets and has no mutable
map dereference. All insertion/mutation/removal routes through narrow methods.
`ReceiptRegistryGuard` reconciles touched IDs before unlock; same-lock consumers
also flush dirty IDs. Durable restoration uses the same insertion path.
Project reads refresh only authorized public active candidates for requested
Projects. Private Jobs do not inflate visible rows. Unregister still includes
hidden handoff/cleanup work. Existing recovery/disconnect/pruning/receipt tests
exercise the indexed container; removal also removes its index entries.

### Primary Overview and frontend ownership

Navigation explicitly requests `include_sessions=false, include_projects=false`.
It uses Runner aggregates, enabled Project/family counts, current active Jobs and
in-flight Windows. It does not call full Runtime status/list-runners helpers
(which materialize historical Jobs), build Project rows, or scan Workflow Sessions.
Projects owns its paged inventory; Work loads its bounded identity/name/lineage
catalog independently at 60-second cadence. Runtime/Session views retain explicit
progressive full hydration. Legacy overview requests keep their previous defaults.

Window history and live state have independent requests/state owners: initial
50 rows, explicit Load more in 50-row pages; Work liveness every 3 seconds and
selected-family Projects liveness every 5 seconds. History refreshes every 30
seconds and revalidates only the prefix actually opened. Prefixes exceeding one
2,000-row server response refresh in bounded chunks without dropping older pages.
New active Windows use a bounded 128-row recent overlay until inventory refresh.
Hidden tabs pause; existing 150 ms focus/visibility/online coalescing and abort/
ownership fences prevent storms and stale cross-client/scope responses.

Projects and Work family selection use exact server-side workspace-ID scopes.
Disabled selection never becomes an accidental global history request. Exact
Window navigation works outside the first page through primary detail lookup.
Work search remains local to loaded rows; the HTTP selector also supports literal
server-side search. Project/family/workspace/Window Maps replace repeated finds.
Historical grouping does not rebuild for every live counter tick; active ordering
can still update among loaded rows. Initial render does not create 2,000 Window rows.

## Complexity

J = retained Jobs, A = active Jobs, P = Projects, W = Windows, H = events/Window,
C = authority-partitioned cells, L = compact distinct Window/principal/Session links.

| Path | Before | After |
|---|---|---|
| Window inventory | summary + per-Window history, O(W × min(H,2000)) hydrated events | O(C + L + P + compact grouping/sort), zero event hydration |
| Project active count | O(P + J), refresh all Jobs under mutex | O(P + A), refresh eligible active candidates only |
| Primary Overview | duplicate Project projection and full Job scans | O(Runners + P + A + live authorization), no Project rows/history/Session scan |
| Browser Project lookup | O(loaded Windows × P) repeated find | O(P) index construction, O(1) per lookup |
| Fast refresh | full history every 3/5 seconds | liveness only, independent history cadence |

C can approach event cardinality under highly diverse principal/anchor sets.
General compact grouping/order legitimately uses TEMP B-TREE operations; exact
Window lookup uses the derived primary key without a temporary ordering B-tree.
No universal constant-time or zero-temp-B-tree claim is made.

## Reproducible local evidence

Disposable databases on special; Node 22.23.2, unoptimized Cargo test profile.
Fixture creation is outside inventory timings. These are Store measurements, not
production HTTP latency, measured browser first-paint, or an sf deployment acceptance.
No fragile wall-clock CI threshold is imposed.

Legacy measurement runs summary + per-Window event/link/session hydration.
New measurement returns the first 50 authorized rows and the full total; measured
fixtures have one authority cell per Window.

| Windows | Events/Window | Legacy ms | Derived ms |
|---:|---:|---:|---:|
| 100 | 10 | 47.175 | 4.823 |
| 100 | 200 | 274.595 | 5.058 |
| 500 | 10 | 241.659 | 15.073 |
| 500 | 200 | 1403.504 | 15.171 |
| 2000 | 10 | 1011.090 | 55.998 |
| 2000 | 200 | 5718.713 | 53.983 |

Benchmark legacy history-related queries: 3 × W; new history queries: 0.
Independent Console instrumentation with 25 Windows confirms two Store inventory
entry points and zero per-Window history methods. The retained detailed-history
benchmark (12 Windows × 2000 linked events) measured 348.126 / 344.863 / 343.850 ms;
this work remains available but no longer belongs to fast inventory polling.
Do not mix oe or other-machine timings into a controlled before/after comparison.

The Job complexity fixture has 10,000 terminal + five active Jobs: one Project
count refreshes exactly one eligible candidate and returns one public active Job;
unregister still sees its three relevant public/private active Jobs. A separate
4,096-Project fixture reports 4,095 enabled Projects, 1,024 families, zero cloned
Project bodies and zero full Job history scans. Batched authority is compared
with exact queries, including denied/missing targets.

Actual App request counts over 25 simulated seconds:

| Page | Primary nav | Project inventories | Window inventories | Liveness | Hidden further 60 sec |
|---|---:|---:|---:|---:|---:|
| Work | 3 | 1 | 1 | 8 | 0 requests |
| Projects | 3 | 1 | 1 | 5 | 0 requests |

Each primary nav excludes Sessions and Project rows. Projects Window requests
are selected-family scoped. Hook tests additionally cover 50+50 pagination,
historical-array identity across live ticks, stale-scope replies, single-flight
slow reads, and revalidation of 2,050 loaded rows. The lookup test performs 2,000
source Map gets for 2,000 Windows against 2,000 Projects, zero array scans.

## Schema, compatibility and startup cost

Four derived tables (cells, links, dirty set, version marker), three secondary
indexes and six event/link change triggers are additive. Old-database open creates
and backfills transactionally; the version marker commits only after success.
Canonical history is not deleted. Later raw updates/deletes/retention mark exact
Windows dirty and rebuild before inventory exposure. Normal event+link+summary
append commits atomically; failure/rollback and repeated open are tested.

A 400,000-event / 2,000-Window old database measured **6425.314 ms** first-open
backfill and **7.117 ms** next open. Cold cost scales with history/storage; warm
inventory latency does not imply free migration. Dirty repair after a large
retention/rewrite can also be substantial.

Additive existing Console HTTP contracts:

- `windows`: optional `projection` (`inventory` default / `liveness`), `offset`,
  exact `projects` set (1..2000; mutually exclusive with legacy `project`), exact
  `client_window_key`, bounded literal `query`; response optional `next_offset`.
  Existing defaults remain. Responses cap at 2000 rows; offsets reach later pages.
  Cross-request pagination is weakly consistent, not a durable snapshot cursor;
  the client deduplicates and revalidates its opened prefix.
- `overview`: optional `include_projects` defaults true; adds `projects_included`
  and `visible_project_families`. Explicit exclusion returns `projects=[]`.
  Conflicting full-Session/no-Project requests fail closed.

Deploy Server and WebUI together for new client request fields. No Runner wire/
generation change, no Runner upgrade requirement, and no MCP model-facing tool
name/schema or authority changes in phase one.

## Acceptance on the phase-one final source

| Scope | Result |
|---|---|
| Store full library | 246 passed; 3 manual benchmarks ignored in ordinary suite |
| Runner registry full library | 320 passed |
| Runtime Console | 59 passed |
| experimental-code-mode Window Console | 20 passed |
| Frontend admin / Runtime v2 | 2 / 186 passed (Runtime: 27 files) |
| Typecheck / build / check:dist | passed; Runtime bundle regenerated |
| Formatting / whitespace | passed |
| Three ignored Store benchmarks explicitly run | all passed |

Initial check:dist failed because local node_modules lacked lockfile-declared
markdown-it; npm ci --ignore-scripts restored the locked tree and verification
passed. An intermediate prefix-refresh regression automatically fetched a short
first page's successor; fixed by limiting automatic multi-page refresh to opened
prefixes over the single-response ceiling, without weakening denial handling.
Fake-clock request tests flush React effects between ticks instead of batching
25 seconds into one artificial turn. No flaky retry is presented as a fix.
Existing core schema-helper / experimental fixture dead-code warnings were not
suppressed. Local detailed logs: `/tmp/webcodex-perf-20261002/`.

```sh
cargo fmt --all -- --check
cargo test -p webcodex-store --lib window_activity
cargo test -p webcodex-store --lib
cargo test -p webcodex-runner-registry --lib
cargo test -p webcodex --lib runtime_console_http
cargo test -p webcodex --lib --features experimental-code-mode runtime_console_http::tests::window
cargo test -p webcodex-store --lib window_inventory_query_benchmark -- --ignored --nocapture
cargo test -p webcodex-store --lib window_inventory_migration_benchmark -- --ignored --nocapture
cargo test -p webcodex-store --lib window_history_query_benchmark -- --ignored --nocapture
npm --prefix frontend ci --ignore-scripts
npm --prefix frontend run typecheck
npm --prefix frontend test
npm --prefix frontend run build
npm --prefix frontend run check:dist
git diff --check
```

## Remaining work at the phase-one boundary

The shared Store connection can still serialize critical operations behind long
history/maintenance reads. Peer queries, optional post-result sidecars, Work
Result workspace observations and service config restart behavior belong to the
requested second phase. Full Runtime/Session diagnostic hydration and detailed
Window history remain potentially expensive explicit paths. Pathological summary
partition diversity and very large Project catalogs warrant further measurement.
