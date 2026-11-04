# Inventory observation performance: controlled comparison

Date: 2026-11-04. Baseline: `7e308d9edcb6d091c2a161e7bd03d253c71ebe2b`.
The baseline is the existing PR implementation before this continuation. Both
implementations were actually executed with the same
`frontend/test-v2/inventory-load.test.tsx` harness.

## Changes and freshness contract

Overview, Projects, and Work's Project catalog previously polled every 5/10/15
seconds respectively (full Overview: 20 seconds). An explicit refresh arriving
during an in-flight read was discarded. A full Overview response that still
identified itself as primary could start another immediate hydration indefinitely.

The new inventory scheduler reuses `ObservationRequest` and the shared
`RuntimeV2Client` observation boundary:

- Primary Overview polls every 10 seconds; full Overview every 30 seconds.
  Projects polls every 30 seconds; Work's Project catalog every 60 seconds.
- Periodic ticks skip an occupied slot. Explicit refreshes coalesce into one
  follow-up, rather than canceling slow work or silently discarding the refresh.
- Focus/online/visibility events still coalesce for 150 ms. A queued resume
  rechecks failure/denial state after the preceding request settles; it cannot
  replace an already queued deliberate refresh.
- Transient failures preserve previously loaded rows as stale. Automatic retry
  eligibility backs off exponentially, capped at 120 seconds; the next visible
  timer tick or resume event performs the eligible read. A deliberate refresh
  bypasses this automatic gate. Denied reads stop automatic retry; a deliberate
  refresh or scope change starts a new attempt, whose result determines the
  subsequent retry state.
- A primary response can initiate one full hydration. A still-partial full
  response waits for the next normal refresh instead of causing a zero-delay loop.
- Authority/filter changes fence both pending responses and the first replacement
  render. The old authority's overview, or old filter's Project rows, is not
  temporarily returned before effects run.

Longer fallback polling does not require waiting 30/60 seconds after an observed
Project change. `projectInventoryKey` uses the primary overview's visible Project
count plus each sorted Runner's identity and connected/status. The Server exposes
existing Registry facts through new optional HTTP fields:
`project_inventory_generation`, `project_inventory_sync_state`, and
`project_inventory_updated_at`. A changed digest immediately invalidates an
eligible mounted Project inventory, or queues one follow-up if it is busy.
Inventory invalidation obeys the same synchronous failure/denial gate as a resume,
including when denial arrives after invalidation was queued; it does not become
a manual retry. Heartbeat age, job counters, and Session activity do not invalidate
the catalog. The first Overview arrival does not cause an unnecessary duplicate
Project fetch.
Successful local Project registration also keeps its explicit list refresh.

This is bounded polling, not an event subscription. A remote Project change is
noticed on the next primary Overview observation (normally within its 10-second
visible cadence, plus response time), then triggers the Project read. Older
servers without generation facts still have count/connection invalidation and
the periodic fallback. Hidden documents issue no periodic reads; work already
in flight may finish.

The client boundary shares only simultaneous identical observations within one
exact client and credential generation, has a 30-second completion boundary,
and does not cache completed results. It does not share across independently
constructed clients or browser tabs. Mutations keep their separate contract.

## Benchmark definition

Each consumer is an independent React hook tree in **one jsdom document**,
containing `useRuntimeOverview(..., includeSessions=false)` and `useProjects`.
The fixture starts 1, 5, or 10 trees simultaneously. Shared-client cases use one
`RuntimeV2Client`; independent-client cases use one client per tree. These are
controlled load scenarios, not real browser-tab measurements.

All time values below are virtual timer values. The harness flushes React between
browser-like timer turns, counts actual mocked fetch invocations, and records
the maximum simultaneous requests:

- Healthy: fixed 100 ms successful response, 25,000 ms visible observation.
- Slow failure: fixed 7,000 ms response with HTTP 503, 55,000 ms visible observation.
- Every case then hides the document for 60,000 ms and asserts zero extra requests.
- First Project ready means the hook first exposes available Project data.
  It is neither browser first paint nor production request latency.

## Measured results

Counts include only Overview plus Projects, excluding Window/Session views.

| Profile | Client scope | Consumers | Baseline requests | Patch requests | Baseline peak in flight | Patch peak in flight |
|---|---|---:|---:|---:|---:|---:|
| Healthy 25 s | Shared | 1 | 9 | 4 | 2 | 2 |
| Healthy 25 s | Shared | 5 | 45 | 4 | 10 | 2 |
| Healthy 25 s | Shared | 10 | 90 | 4 | 20 | 2 |
| Healthy 25 s | Independent | 1 | 9 | 4 | 2 | 2 |
| Healthy 25 s | Independent | 5 | 45 | 20 | 10 | 10 |
| Healthy 25 s | Independent | 10 | 90 | 40 | 20 | 20 |
| Slow 503, 55 s | Shared | 1 | 12 | 3 | 2 | 2 |
| Slow 503, 55 s | Shared | 5 | 60 | 3 | 10 | 2 |
| Slow 503, 55 s | Shared | 10 | 120 | 3 | 20 | 2 |
| Slow 503, 55 s | Independent | 1 | 12 | 3 | 2 | 2 |
| Slow 503, 55 s | Independent | 5 | 60 | 15 | 10 | 10 |
| Slow 503, 55 s | Independent | 10 | 120 | 30 | 20 | 20 |

Per unshared consumer, healthy requests change from 6 Overview + 3 Projects to
3 Overview + 1 Projects. Slow failures change from 6 + 6 to 2 + 1.
Initial Project readiness is **100 ms for both implementations**, for every
healthy case. Slow-failure cases never claim data readiness. All 24 baseline
and patch cases issue **zero** additional requests while hidden.

The separate `console-nav-requests.test.tsx` mounts the complete App and confirms
the following 25-second request counts:

| App surface | Observation | Baseline | Patch |
|---|---|---:|---:|
| Work | Overview | 6 | 3 |
| Work | Project catalog | 2 | 1 |
| Work | Window inventory | 1 | 1 |
| Work | Window liveness | 8 | 8 |
| Projects | Overview | 6 | 3 |
| Projects | Project inventory | 3 | 1 |
| Projects | Window inventory | 1 | 1 |
| Projects | Window liveness | 5 | 5 |

These results establish reduced idle/failing request load and bounded
multi-consumer behavior. They do not measure production CPU, SQLite lock
contention, network throughput, browser rendering latency, or real multi-tab
scheduling. Server projection benchmarks and actual browser validation are
separate evidence.

## Reproduction

Run from the current repository with frontend dependencies installed:

```sh
cd frontend
npm run test:runtime -- test-v2/inventory-load.test.tsx test-v2/inventory-scheduling.test.tsx test-v2/console-nav-requests.test.tsx
```

Every load case prints one `INVENTORY_LOAD` JSON record. For the actual old
implementation, run the same harness in a temporary copy of the baseline frontend:

```sh
repo_root=$(git rev-parse --show-toplevel)
baseline_dir=$(mktemp -d /tmp/webcodex-inventory-baseline-XXXXXX)
git archive 7e308d9edcb6d091c2a161e7bd03d253c71ebe2b frontend | tar -x -C "$baseline_dir"
cp "$repo_root/frontend/test-v2/inventory-load.test.tsx" "$baseline_dir/frontend/test-v2/"
ln -s "$repo_root/frontend/node_modules" "$baseline_dir/frontend/node_modules"
cd "$baseline_dir/frontend"
INVENTORY_BASELINE=1 npm run test:runtime -- test-v2/inventory-load.test.tsx
```

The baseline flag selects the expected measured budgets and report label; the
baseline copy supplies the actual old scheduler/client implementation. The
working checkout is not rewritten for the comparison.

`inventory-scheduling.test.tsx` separately covers explicit refresh during a slow
read, queued resume followed by 503 or 403, explicit priority over resume,
failure backoff and stale data, deliberate recovery, primary/full loop bounds,
first-render authority/filter fences, generation invalidation, and cancellation
of a queued refresh belonging to an obsolete filter.
