# Runtime Console Project selector performance

Measured on 2026-11-04. This change removes model-facing discovery work from
the browser Project selector while preserving current authorization, filtering,
canonical identity, optional short references, sort order and response bounds.

The largest fixture has 5,000 registered Projects. With a 2,000-row page and ten
concurrent consumers, measured Server query-plus-serialization P95 fell from
**1,574.308 ms to 278.288 ms** (82.3% lower, 5.66× faster). With a 24-row page,
the same fixture's ten-consumer P95 fell from **102.509 ms to 20.460 ms**.
All 18 cases retained the same returned counts and serialized payload byte
lengths. These are controlled Server measurements, not deployed-browser latency
or a production service-level guarantee.

## Observed hot path and changes

The former HTTP path called
`ToolRuntime::query_project_inventory` through
`query_projects_for_runtime_console`. That path:

1. Entered the Runner Registry mutation/lifecycle guard and cloned the complete
   selected Runner inventories into semantic views, before Project filtering
   and limiting.
2. Built and sorted candidates, computed active-Job summaries, resolved shell
   profiles and built model-facing capability facts for the selected page.
3. Called `get_or_create_project_reference` synchronously for each selected
   Project. Even a reference that already existed performed its own
   `BEGIN IMMEDIATE`, lookup and `COMMIT`.
4. Reduced those records to the browser allowlist: canonical id, client id,
   name, path, connected/status and optional short ref.

The HTTP selector now calls
`RunnerRegistry::select_projects_for_auth` for minimal typed facts. It applies
the existing access/expired-shared-key fence before counting candidates and
retains only the first ID-sorted page in a bounded heap. It clones only that
page's selector fields. Search still evaluates the existing canonical search
semantics, including metadata that may be searched but is deliberately absent
from the response.

The new path uses the Registry's **immutable observation guard**. This is still
the same asynchronous mutex, not a concurrent RwLock or a lock-free cache.
The immutable guard avoids mutation-guard Drop work such as unrelated receipt
retry consumption and active lifecycle reconciliation. No new permission cache,
Runner RPC, inventory TTL, transport generation or partial snapshot publication
was introduced.

Short references have a real browser display consumer and remain supported.
`Database::get_or_create_project_references` uses one indexed batch lookup on
the warm path, with no SQL write transaction. If mappings are missing, it
rechecks under one immediate transaction and allocates them together. The
identity remains **principal + canonical Project id + root fingerprint**.
Duplicate identities share one allocation; changing a root creates a new
mapping, and old mappings never retarget. The existing single-reference method
delegates to this implementation, so model-facing consumers use the same
allocation and durability rules.

The Console batch runs in `spawn_blocking` with one admitted worker. Its permit
stays in the worker until completion even if the HTTP future is cancelled.
Admission and awaiting the worker share **one absolute one-second budget**.
If that optional display lookup expires, the already-authorized canonical page,
total and pagination result still return, with absent short refs. There is no
automatic write replay; an admitted worker may finish the same durable mapping
after the caller stops waiting. Required MCP reference resolution and command
execution do not acquire this Console presentation deadline.

The deadline addresses a concrete failure mode: with a five-second SQLite busy
timeout, ten requests queued behind a cold-reference writer could otherwise
wait serially for up to roughly 50 seconds. An isolated test holds a real
external SQLite write transaction while ten callers wait, verifies that all
optional waits expire, only one worker starts, and the worker retains admission
until the writer is released. A separate cancellation test proves that aborting
the caller cannot admit a second blocking worker.

## Work bounds and database operations

Let `P` be the authorized Projects considered by a query, `K` the returned
page size (at most 2,000), and `M` its distinct missing stable identities.

| Work | Previous HTTP path | Updated HTTP path |
|---|---|---|
| Retained candidate ordering | All matches, sort by id | At most K candidates, bounded heap |
| Complete Project summary clones | All selected Runner inventories, then selected rows | 0 |
| Selector field clones | Part of complete summary clones | K bounded rows |
| Candidate scan | O(P), plus full-match sorting | O(P log K), including exact total count |
| Retained selection memory | O(P) full metadata and candidates | O(K) borrowed candidates, then selector fields |
| Active-Job/shell/capability projection | Present | 0 |
| Runner RPCs | 0 | 0 |
| Warm reference connection acquisitions | K | 1 |
| Warm reference SQL statements | 3K: BEGIN, SELECT, COMMIT per row | 1 indexed SELECT |
| Warm reference SQL write transactions | K | 0 |
| Missing-reference transactions | One per selected row in the old implementation | One for the batch |
| Missing-reference statements | Existing rows still use individual transactions | At most three reads, M inserts, one BEGIN/COMMIT |
| Optional Console reference wait | No overall queue/result deadline | One second total; canonical page can still return |

The SQL counts are based on the implementation and directly checked by the
Store regression's SQLite statement trace. The warm regression asserts exactly
one SELECT and no BEGIN/INSERT. The cold regression verifies one transaction
and exactly one insert per distinct new identity. Query/serialization timings
include any Store wait, but this harness does **not** claim a separately
measured database lock-wait P50/P95 distribution.

Formatting a canonical candidate id and evaluating a supplied text query still
requires visiting matching inventories. The patch does not claim constant-time
search. Disabled registrations retain their existing discovery/list semantics;
enabled-only Project-family counts remain a separate aggregate.

## Inventory freshness and consistent family counts

Primary and full overview now expose the same existing path-free inventory
facts on each Runner summary:

- `project_inventory_generation`: the admitted inventory generation.
- `project_inventory_sync_state`: pending/complete/degraded state.
- `project_inventory_updated_at`: the existing `last_sync_at` value.

Ordinary heartbeat age is not one of these fields. Dynamic registration already
clears the generation and updates `last_sync_at`; completed inventory
publications replace the generation. The frontend can invalidate a Project
catalogue from these facts without refetching it on every last-seen, Job or
Session update. Existing periodic refresh remains necessary, including for
same-second dynamic changes or older Servers that lack the added fields.

Full overview previously counted families from sparse selector DTOs that
deliberately omitted lineage. Consequently, managed worktrees could become
separate families, and results were limited to the returned page. Full and
primary overview now use the same Registry lineage and disabled-registration
rules across all authorized inventories. A regression includes a managed
worktree, a disabled registration, a foreign credential and more than 2,000
Projects, and verifies equal family counts and real JSON inventory fields.

Project selector liveness follows the existing canonical RunnerView heartbeat
rule for both polling and streaming Runners. A polling Runner does not need a
stream notifier to be online. Tests compare both recent and expired heartbeat
states directly with the canonical view.

## Controlled measurement

The opt-in harness is
`src/runtime_console_http/tests/project_inventory_benchmark.rs`:

- Same `special` Runner, same Cargo unoptimized test profile, same fixture
  workload and a four-worker Tokio runtime before and after the backend patch.
- 500, 2,000 and 5,000 Projects spread across five cached polling Runner
  inventories. No fixture serves Runner RPCs. Stale/expired Runner behavior is
  checked separately by the Registry regressions.
- Metadata uses valid bounded descriptions, hooks, shell profiles and stable
  root fingerprints, so cloning work is represented rather than measuring
  id-only records. Inventory cardinalities are asserted after protocol admission.
- Page limits 24 and 2,000; one, five and ten concurrent request consumers.
- Twenty one-consumer samples, twenty-five five-consumer samples and fifty
  ten-consumer samples per size/page combination: **570 measured warm requests
  across 18 cases**, plus the first pass for each size/page.
- Each warm sample invokes the authenticated typed Server selector, constructs
  the browser DTO and serializes JSON. It excludes HTTP/TLS transport, credential
  token verification, browser rendering, Runner synchronization and compilation.
- Percentiles use nearest rank. The whole opt-in test ran in 38.79 seconds before
  and 9.69 seconds after; compilation time is excluded.

The baseline uses the selector/reference implementation at the branch's
`7e308d9ed` starting point with this harness added. Frontend files were being
integrated in the same checkout, but the measured backend implementation had
not yet been changed. The patched measurement ran after the backend corrections
and focused tests. This is one before/after run, not an isolated-machine
statistical study. Scheduler and filesystem noise can affect individual
percentiles; notably the 2,000-Project/24-row baseline has a higher P95 than its
median. No timing threshold is enforced in CI.

### Warm query plus serialization

All latency columns are milliseconds. The final column is before-P95 divided
by after-P95.

| Projects | Page limit | Consumers | Samples | Before P50 | After P50 | Before P95 | After P95 | P95 ratio |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 500 | 24 | 1 | 20 | 2.790 | 1.021 | 2.931 | 1.067 | 2.75× |
| 500 | 24 | 5 | 25 | 11.211 | 1.701 | 14.931 | 2.398 | 6.23× |
| 500 | 24 | 10 | 50 | 19.949 | 2.466 | 30.489 | 4.156 | 7.34× |
| 500 | 2000 | 1 | 20 | 44.676 | 14.645 | 53.239 | 14.974 | 3.56× |
| 500 | 2000 | 5 | 25 | 144.395 | 26.440 | 188.765 | 38.364 | 4.92× |
| 500 | 2000 | 10 | 50 | 229.400 | 38.437 | 490.763 | 67.698 | 7.25× |
| 2000 | 24 | 1 | 20 | 5.277 | 1.415 | 38.821 | 1.452 | 26.74× |
| 2000 | 24 | 5 | 25 | 23.631 | 2.785 | 31.208 | 4.534 | 6.88× |
| 2000 | 24 | 10 | 50 | 40.867 | 4.563 | 57.801 | 7.900 | 7.32× |
| 2000 | 2000 | 1 | 20 | 174.199 | 59.768 | 187.557 | 60.637 | 3.09× |
| 2000 | 2000 | 5 | 25 | 575.036 | 108.086 | 759.652 | 166.823 | 4.55× |
| 2000 | 2000 | 10 | 50 | 765.698 | 160.835 | 1486.836 | 279.677 | 5.32× |
| 5000 | 24 | 1 | 20 | 7.871 | 2.643 | 8.158 | 3.143 | 2.60× |
| 5000 | 24 | 5 | 25 | 42.678 | 6.207 | 49.480 | 10.169 | 4.87× |
| 5000 | 24 | 10 | 50 | 83.565 | 11.477 | 102.509 | 20.460 | 5.01× |
| 5000 | 2000 | 1 | 20 | 175.859 | 62.899 | 176.523 | 63.227 | 2.79× |
| 5000 | 2000 | 5 | 25 | 593.607 | 117.026 | 816.099 | 160.236 | 5.09× |
| 5000 | 2000 | 10 | 50 | 858.475 | 159.477 | 1574.308 | 278.288 | 5.66× |

### Response size and first pass

The payload byte count below is the same before and after for every consumer
count. The harness checks counts and payload lengths; it does not capture a
cross-run response-body hash.

“First pass” measures the initial selector query for that page, before JSON
serialization. Each size uses a fresh temporary database, but **limit 24 runs
first and allocates those references before limit 2,000**. The latter therefore
has 24 existing refs and is not a completely cold database measurement. The
baseline log named this diagnostic field `cold_ms`; the checked-in harness
calls it `first_pass_ms` to make that distinction clear. Renaming that label
and correcting the fixture comment did not change the measured workload.

| Projects | Page limit | Returned rows | Payload bytes (both) | Before first pass, ms | After first pass, ms |
|---:|---:|---:|---:|---:|---:|
| 500 | 24 | 24 | 4,282 | 5.600 | 2.191 |
| 500 | 2000 | 500 | 88,936 | 94.223 | 21.457 |
| 2000 | 24 | 24 | 4,283 | 45.677 | 2.156 |
| 2000 | 2000 | 2000 | 356,938 | 337.696 | 87.387 |
| 5000 | 24 | 24 | 4,283 | 12.846 | 3.726 |
| 5000 | 2000 | 2000 | 356,937 | 339.089 | 91.310 |

## Validation and reproduction

Focused validation passed:

- Runtime Console tests: 71 passed, including authorization, source-window
  visibility, Project filters, sparse allowlists and overview projection.
- Runner Registry selector tests: 2 passed, covering bounded selection, exact
  query, ownership, expired shared-key visibility, receipt-candidate preservation,
  zero Job scans and canonical polling/stale liveness.
- Store project-reference tests: 4 passed, including trace-based SQL counts,
  durability, isolation, concurrent allocation and root incarnation.
- Console reference budget/cancellation tests: 2 passed with isolated lanes.
- Existing ToolRuntime/MCP project-reference tests: 8 passed, including real
  scoped dispatch, late-page resolution, credential reauthorization and
  replaced-root rejection.
- Both complete profiling runs: 18 cases and 570 requests each, assertions passed.
- Stable rustfmt and the diff whitespace check passed during integration.

Reproduce from the PR checkout:

```bash
cargo test -p webcodex --lib project_selector_load_profile -- --ignored --nocapture
cargo test -p webcodex-runner-registry project_selector
cargo test -p webcodex-store project_reference
cargo test -p webcodex --lib console_refs
cargo test -p webcodex --lib project_ref
cargo test -p webcodex --lib runtime_console_http::tests::
```

For a before/after rerun, add the same opt-in harness and its module declaration
to a separate checkout of `7e308d9ed`, keeping the Rust toolchain, Cargo profile
and fixture workload the same. Do not compare this debug-profile result with a
release-profile number. Run CPU-heavy builds separately from measurement.

Recorded validation Jobs:

- Baseline profile: `wc_job_UeJqrOdi4pFj9lxC`.
- Updated profile: `wc_job_GRmOKuWZHlbnv_67`.
- Store batch regressions: `wc_job_J4jpz5Ekn6dT3ZqS`.
- Final Registry/budget/MCP regressions: `wc_job_cLrFLe6F1S2xHdW1`.

Earlier fixture/compiler failures produced no accepted performance data. This
report uses only the successful complete runs.

## Production boundary

No production Server or Runner was restarted or deployed for this measurement.
The live sf Server observed during this task runs an older commit and has
separate resource/I/O and persistence observations. This patch reduces a proven
read-path cost and bounds optional Console reference waiting; it does not prove
that those production resource or writer failures are resolved.

The end-to-end browser, request-scheduling and Session-navigation evidence is
reported separately in
[the WebUI end-to-end report](webui-end-to-end-performance.md).
