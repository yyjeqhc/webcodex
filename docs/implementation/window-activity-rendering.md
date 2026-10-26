# Window activity rendering

Measured on 2026-10-26. This rendering change builds on the polling-consistency
commit `96bd80cc6759654ac811b7d4c99a09745ff85c99`. The refresh, authority,
cancellation, stale-state, and history-attempt contracts remain those documented
in [Window polling consistency](window-polling-consistency.md).

## Observed cost and implementation

The original activity feed mounted every loaded call. With 2,000 completed calls,
100 Projects, and 40 referenced Sessions, the synthetic fixture produced 32,126
DOM nodes. Each metadata-only update reconstructed the call/session projections,
looked up Projects repeatedly, and rendered the entire list. Healthy primary
polling also replaced the retained activity array on nine of nine measured
updates even when all activity facts were identical.

The feed now initially renders 80 calls. **Show more calls** adds another 80
matching calls without changing the loaded dataset or issuing a network request.
The UI reports both the number shown out of matching calls and the full loaded
count.

Session segmentation runs over the complete chronological history first. Text
and status filters then run over that complete selected dataset. Ordering and
the presentation bound come last. Search therefore finds a retained call beyond
the initial slice, and untagged Session continuation calls keep their original
segment membership. The visible **Failed calls** and **Running calls** controls
include counts from all matches, so an old failure or a running call outside the
slice remains directly discoverable. Failure matching includes expectation
mismatches as well as failed, cancelled, and timed-out statuses. No tool-name
allowlist hides returned observations.

Completed-call projections, Session catalogs, Project indexes, and options are
memoized from their actual source arrays. Live elapsed-time changes do not
rebuild completed-call projections. Call cards use ordinary React memoization.
The Workbench's own Session catalog also depends on activity and linked-Session
arrays instead of the whole detail object. Existing Project-family navigation
memoization remains in place.

## Structural sharing with complete facts

The Window activity merge is centralized in
[windowActivity.ts](../../frontend/src/runtime-v2/model/windowActivity.ts).
An unchanged primary head compares only the returned head and returns the
retained array immediately, avoiding a full-history map, sort, and merged array.
An unchanged full snapshot also reuses the retained array. Changed snapshots
reuse untouched row objects and keep the 2,000-row retention cap.

Equality compares every JSON-shaped response fact, recursively, including
optional or newly added fields. It does not treat matching timestamps, trace
IDs, statuses, or row counts as proof that content is unchanged. Changes to
duration, Project, Session links, Job observations, timing, expectation results,
or additional response fields remain visible.

Exact trace IDs identify calls. Older untraced observations use their start time,
method, tool, and occurrence position; repeated invocations at the same
millisecond remain represented. Result/status, duration, Project, and Session
fields are mutable facts rather than identity. New trace identity is retained as
new evidence.

Full refreshes also share unchanged linked-Session, Job, and active-request
arrays. This is local structural sharing inside the existing workspace state;
it adds no cross-tab cache, persistent data store, or dependency.

### Primary history while full hydration is pending

A primary refresh now uses the same retaining merge before the first full read
finishes. In the existing burst regression, one previously observed call plus
80 new primary calls produces **81 retained rows**, rather than discarding the
earlier call and temporarily showing 80. Full catch-up subsequently produces all
161 fixture rows.

This changes client retention, not request size: primary reads still request at
most 80 activities, full history stays bounded at 2,000, and full attempts retain
their 30-second cadence. The regression explicitly checks both the intermediate
retained call and eventual complete catch-up.

## Expansion and diagnostics

Expansion choices belong to the exact Window and stable call keys. Metadata
updates, text/Session/status filters, and progressive loading do not clear them;
changing the Window resets them. The selected Window and collaboration target
remain controlled by the existing Workbench selection logic.

A collapsed call renders a small diagnostics disclosure. Its request-owning
panel mounts only after expansion. Closing and reopening an already mounted
panel retains its cached page. Filtering a row out unmounts that row and cancels
its pending read; returning to it preserves the expansion choice and may perform
a new authorized diagnostic read. No shared trace cache is introduced.

Closed event and capture-health disclosures do not render or stringify their
JSON bodies. Diagnostic events still open by default when the call diagnostics
are opened. Full captured payloads require their existing explicit read action.
The 240-event diagnostic bound, permission errors, and exact-trace response fence
remain covered by existing tests.

## Measurements

The same `window-load-benchmark.test.tsx` fixture and ten metadata rerenders were
used before and after this change. The latest pre-rendering baseline was captured
with the focused B validation batch; the C benchmark ran alone during a
coordinated quiet CPU interval.

| Measurement | Before rendering change | After rendering change |
| --- | ---: | ---: |
| Loaded completed calls | 2,000 | 2,000 |
| Initially rendered call cards | 2,000 | 80 |
| DOM nodes in feed container | 32,126 | 1,415 |
| React Profiler mount | 1,351.76 ms | 178.18 ms |
| Mount wall time | 2,110.29 ms | 446.35 ms |
| Profiler update median | 424.32 ms | 14.35 ms |
| Profiler update p95 | 652.62 ms | 16.11 ms |
| Profiler update samples | 11 | 11 |
| Unchanged activity-array replacements across nine primary ticks | 9 | 0 |

Initial DOM size fell by approximately 95.6%. Profiler samples include updates
caused by the test UI's own effects, alongside the ten requested metadata
rerenders. Host scheduling and concurrent test work affect wall-clock and
Profiler measurements; these values are observations rather than timing gates.

Polling counts are unchanged from the consistency fix. Two independent hook
trees over 27 simulated seconds issued two inventory, eighteen liveness, twenty
primary, and two full reads. During the following 30-second interval with
full-history 503 responses, they issued two failed full reads and retained the
stale state.

These measurements use jsdom. They do not measure browser layout, paint,
scrolling, frame timing, production latency, or real multi-tab coordination.
Progressive loading can intentionally grow the rendered list to the complete
loaded history; the 80-row bound applies to the initial view and each additional
user-requested slice.

## Validation and reproduction

From `frontend`:

```sh
npm run test:runtime -- test-v2/window-activity-sharing.test.tsx test-v2/window-activity-rendering.test.tsx test-v2/window-polling-consistency.test.tsx test-v2/runtime-state.test.tsx test-v2/relations.test.tsx test-v2/trace-diagnostics.test.tsx
npm run test:runtime -- test-v2/window-load-benchmark.test.tsx
npm run typecheck
```

The six focused files passed 70 tests. Both reproducible benchmark tests passed,
and TypeScript validation passed. Deterministic assertions cover the initial
80-row DOM bound, full-data search and counts, old failures, running calls,
Session segments, progressive reachability of all returned calls, expansion
continuity, semantic updates, structural sharing through actual primary/full
polling, repeated untraced calls, and the retention cap. Existing authority,
stale-state, single-flight, diagnostic-permission, and explicit-payload tests
remain green. No wall-clock threshold is used.
