# Store and post-result latency — phase two

This continues [Work / Projects read-path scalability](webui-runtime-read-paths-performance.md)
on `special:/root/git/webcodex`, branch `perf/webui-runtime-read-paths`.
Phase one ended clean at `e48c60717ebd5116285de5fc96ed8cee29861581`.
Phase-two source, including the independent review corrections below, is
`f16d858dec9dd8204c37599e2a30d4c84bf07fbc`.
All changes are local commits. No production database, installed service or
Runner was upgraded, deployed or restarted. No push, PR or merge was performed.

The motivating sf figures (about 319,000 ActionAudit rows, a 658 MB database,
seconds of post-execution latency, and repeated configuration-error restarts)
were supplied in the operator's Session guidance. They are not production
measurements reproduced by this implementation. The measurements below use
isolated fixtures on special; they are not sf HTTP or browser latency claims.

## Commits and authoritative owners

| Commit | Responsibility |
|---|---|
| `7591244d5409` | Isolate Console history reads from the critical Store writer |
| `8e4b09eb41f9` | Range-index recent Peer awareness against canonical ActionAudit |
| `9268e78e` | Budget optional post-result enrichment and instrument phases |
| `efc27b4153cb` | Reuse bounded presentation-only workspace observations |
| `7bcf7f8fe081` | Preflight Runner configuration and stop deterministic systemd restart loops |
| `7566004e6603` | Preserve event/link snapshot consistency under concurrent retention |
| `f16d858dec9d` | Keep the existing MCP schema budget and test-only wrapper boundary |

## A. Historical reads no longer hold the writer connection

`Database` owns the existing canonical writer and exactly one additional
read-only WAL connection. The latter is opened only after schema initialization,
auth cleanup and receipt pruning finish. It has `query_only=ON`, foreign keys,
the existing 5-second SQLite busy timeout, and a 2 MiB page-cache target.
There is no dynamic connection pool, alternate database or separate authority.
Fast reference/authority reads and durable mutations remain on the canonical
writer. Connections are owned and dropped with the Database.

Potentially long Console history calls use `runtime_console_http/store_read.rs`.
One semaphore permit is acquired asynchronously before `spawn_blocking` and is
held by the actual blocking closure, including after HTTP cancellation. Thus a
cancelled waiter does not submit work, and cancelling an already-started read
cannot release admission while that read still runs. This bounds admitted
blocking history work to one; it does not bound the number of external requests
waiting for admission or guarantee a deadline for every historical query.

The Window inventory's dirty check and derived read share one read transaction.
If repair is needed, the read transaction and reader lock are released before
acquiring the writer. Up to three repair/read attempts are allowed; continuing
invalidation returns unavailable, not knowingly stale derived evidence. Repairs
are still canonical writes, not a second concurrent writer owner. Audit append
repairs only its own Window rather than rebuilding unrelated dirty history.
A large retention repair may still hold the writer for significant time.

### Independent review correction: one page, one snapshot

Moving a read from the writer connection to a reader mutex is not enough for
multi-statement consistency. The event SELECT could finish before its batched
workflow-link SELECT, allowing another WAL connection to delete the event and
its links between the statements. That returned an event with missing links.

The regression `window_event_pages_keep_links_in_one_snapshot_during_retention`
uses a test-only SQLite authorizer to perform a real cascading deletion on a
separate connection exactly when the link query is prepared. Before correction,
the ordinary global query deterministically returned zero links instead of one.
The fixed ordinary, Code Mode composition, and Goal event-page readers hold an
explicit transaction for both statements. All three are tested with global and
principal-scoped reads, six cases in total. The page retains its original link,
a subsequent query sees the committed deletion, and the connection returns to
autocommit. The fix does not serialize the external writer behind the reader.

The isolated Store test also holds a read transaction open while a second
thread performs receipt persistence, Project reference creation/lookup and
ActionAudit append. Completion must occur before the reader is released. The
initial sample was 4.583 ms; the final concurrent Window test suite recorded
45.770 ms. These are scheduling-sensitive local samples, not a fixed performance
bound. The tested invariant is independence from the deliberately held reader.

## B. Peer awareness searches the recent indexed range

`idx_action_events_recent_peer` is a partial covering index with principal kind,
principal id, exact Project and completion time as its leading search fields.
`peer_recent.sql` materializes the matching recent interval before grouping by
Window and checking the exact discovery identity. Canonical ActionAudit remains
the source. There is no cached Project authorization, second recent-peer clock,
or limit applied to unrelated history that could hide a valid recent peer.
The empty result does not write or prune discovery records.

The new query depends on the matching recent rows, not all retained ActionAudit
rows. It may still group/sort a large recent interval. The query plan deliberately
retains temporary B-trees for GROUP BY and final ORDER BY on those candidates.
It uses the covering recent-range index and the existing exact discovery index:

```text
SEARCH e USING COVERING INDEX idx_action_events_recent_peer
  (principal_correlation_kind=? AND principal_correlation_id=? AND project=? AND window_ended_at_ms>?)
SEARCH d USING COVERING INDEX sqlite_autoindex_window_peer_discoveries_1
  (principal_kind=? AND principal_id=? AND observer_window_key=? AND peer_window_key=? AND project=?)
```

Same-fixture benchmark; eight recent rows, fixture insertion outside the timer:

| Old retained rows | Without new index ms | Indexed ms | Baseline VM steps | Indexed VM steps |
|---:|---:|---:|---:|---:|
| 10,000 | 11.816 | 0.094 | 140,630 | 611 |
| 100,000 | 152.127 | 0.113 | 1,400,630 | 611 |
| 400,000 | 765.503 | 0.086 | 5,600,630 | 611 |

The normal regression compares no old rows with 100,000 old rows. It allows a
small B-tree-boundary difference (610 versus 611 VM steps), not a fragile exact
clock or VM-count equality. It verifies principal, Project, cutoff, observer
exclusion, exact-once discovery and re-delivery after rollback.

## C. Optional enrichment has one shared budget

The normal Kernel post-record path and specialized MCP result decoration share
one 40 ms deadline across operator attention, Peer messages, Peer awareness and
passive Job attention. The deadline is not reset for each sidecar. This is a
best-effort enrichment budget, not a 40 ms guarantee for an entire tool response.

| Required semantics: still waited | Optional presentation: may be omitted |
|---|---|
| Canonical tool execution and Session recording | Unrequested operator/Peer message projection |
| Explicit Window reply and acknowledgement writes | Discovery of newly observed recent peers |
| Canonical ActionAudit finalization | Passive Job result attention |
| Existing permission, identity and outcome-uncertainty fences | Existing bounded asynchronous trace telemetry |

The Store optional path uses `try_lock` rather than queueing on the writer. With
the lock held, its SQLite busy timeout is temporarily zero, and a progress handler
checks the absolute deadline every 200 VM operations. The callback interrupts
at most once so transaction rollback remains possible. A scope guard restores
the previous busy timeout and removes the progress hook on every exit.
Explicit ACK inputs retain the required, unbudgeted transaction path.

Message/discovery projections are accepted against the response-size budget
before committing delivery state. Deadline, contention or rejected presentation
does not consume optional delivery state. Already committed explicit ACK effects
are not reclassified as an optional omission. No primary tool is replayed and
an omitted sidecar does not change the primary ToolResult success.

Passive Job snapshots use nonblocking registry access, periodic deadline checks
and bounded active/recent-terminal top-K storage instead of cloning and sorting
the full history. They may still inspect historical records until the deadline;
this is not the phase-one active-ID count algorithm. Failure to acquire/read a
snapshot does not advance the attention cursor. The final cursor uses try-lock.

Phase instrumentation covers `canonical_execution`, `explicit_window_reply`,
`operator_attention`, `peer_messages`, `peer_awareness`, `passive_attention`, and
`audit_finalize`. It records outcomes such as `completed`, `budget_exhausted`,
`skipped_due_to_contention` and `store_error`; Store domain lock wait/hold timing
remains separately available. Raw tool contents or credentials are not added.

The post-record contention regression deliberately holds the writer while a
successful read-result is finalized. Optional fields are absent; the primary
result is unchanged; after release the retained Peer message is delivered once.
The measured earlier sample was 0.084 ms before writer release. A separate
10 ms recursive-SQL deadline test verifies rollback and restoration of connection
policy, and an external writer-lock test verifies no wait or partial commit.

Mandatory durability can still wait on writer contention or storage I/O. SQLite
progress callbacks do not preempt every operating-system operation, serialization
or scheduler pause. There is intentionally no claim of a strict end-to-end SLA.

## D. Work Result observes expensive Git state only when needed

Automatic App polls opt in with `get_work_result_state.automatic=true`. Omission
or false forces a new observation. The bundled App passes true only for timer
polls; explicit refresh and presentation use fresh reads. Closed Sessions also
force re-observation. File/diff detail and sealed final Changes retain their own
existing exact authorization and snapshot contracts.

The presentation cache is bounded to 64 entries, 4 MiB total and 256 KiB per
entry. Keys contain principal kind/id and exact canonical Project. Reuse is
permitted for at most 30 seconds and only when all of the following still agree:

- Quiescent canonical source-mutation fence, including epoch/generation.
- Exact Runner registration observation epoch and complete Project inventory.
- Project registration revision, root fingerprint, path and observed Git identity.
- Current connection/liveness and fresh Project authorization on this call.

The source/target stamp is captured before and after the native probe. A raced
mutation is not cached; a slower old probe cannot replace a newer observation.
Missing revision, reconnect, stale/disconnected Runner, known mutation, expiry,
authorization loss or explicit refresh prevents reuse. Errors are not cached.
Jobs, validation and collaboration continue to project their own current state.
`workspace_observation` is diagnostic provenance, excluded from `state_version`
so a cache hit itself does not reset the App's idle clock.

This is not a filesystem watcher. Changes outside observed WebCodex mutations
may remain invisible until the 30-second lease expires or the user explicitly
refreshes. The result states `bounded_snapshot_not_filesystem_freshness`; cached
presentation is never proof of validation or sealed final task content.

The real local Git fixture observed one initial Git request, then zero additional
requests for eight unchanged polls, an independent validation update, and an
independent Job-state change. Further tests cover clean-to-dirty, dirty-to-another
change, explicit refresh, lease expiry, closed Session, reconnect, revision loss,
foreign principal, revoked Project and mutation during the original probe.

### Surface-budget review correction

The App-only boolean initially included a long generated implementation-note
description. The full MCP regression found anonymous/App compact tools/list at
87,117 bytes against the unchanged 87,000-byte limit. Keeping that note as an
ordinary source comment, without changing the boolean/default or raising the
test limit, restored 86,954 bytes. All credential/App combinations passed the
same test. A test-only passive-attention convenience wrapper is now compiled
only for tests, removing the new production dead-code warning.

## E. Deterministic configuration errors no longer hot-loop under systemd

The Runner has a separate `--check-config --config PATH` CLI action. It invokes
the same native configuration loader, exits zero with an exact success marker,
or exits two for invalid/unreadable configuration. It starts no Runner transport,
provider, project registry, or Agent loop. Preflight errors do not echo TOML
parser output, which can contain secrets. The retired `projects_dir` spelling
remains invalid; it is not silently migrated or accepted as a second contract.

The hosted CLI resolves the candidate and preflights it before stopping an
existing managed Runner, then verifies the config hash again. Missing candidate,
failed preflight or a changed config preserves the old process. Explicit local
restart has the same ordering. The helper bounds execution to 15 seconds and
captured output to 8 KiB per stream, with EOF stdin and existing process handling.

Generated native and CLI Runner systemd units add `RestartPreventExitStatus=2`.
Their existing restart policy and 5-second interval otherwise remain unchanged,
so transient non-configuration failures retain automatic restart. Server units
are not given the Runner-specific exception. Config correction subsequently
requires an explicit operator start/restart; no actual unit was installed here.

For existing managed native units, only the exact old text differing by omission
of this one guard is accepted. An explicit install can atomically reconcile that
policy without restarting the process. Other text changes, symlinks, wrong owner
or writable units remain rejected. The legacy ownership reader recognizes only
the exact old or guarded historical CLI template, not arbitrary normalized text.

Same-account native Linux start/restart/install perform preflight. Cross-account
privileged installations deliberately do not execute a user-owned candidate as
root merely to validate configuration; native startup validation and the restart
exception remain. This limitation is preferable to broadening execution authority.
macOS launchd and Windows SCM restart policies were not changed or accepted by
this Linux dogfood. The hosted helper is shared code, but the process acceptance
here ran on Linux only.

Tests invoke the actual candidate binary on valid, retired-key, malformed,
semantic-invalid and missing configs. They assert exit codes, no network accepts,
no new registry, unchanged files and no preflight secret leakage. The managed
process test confirms both invalid replacement and invalid restart leave the old
PID alive. Generated units pass `systemd-analyze verify`; no real service is
created or restarted by that check. Full Linux Runner and CLI suites also pass.

## Complexity and remaining cost

| Path | Before | After |
|---|---|---|
| History read versus critical Store write | Shared connection mutex serializes both | Independent fixed WAL reader; canonical writers still serialize |
| Peer awareness | Scan unrelated retained audit history | Indexed principal/Project/time range, then recent-row grouping and discovery lookup |
| Optional post-result enrichment | May queue behind Store/registry locks for seconds | Try-acquire and shared deadline; omitted work leaves optional delivery/cursor unchanged |
| Passive Job attention | Full cloned/sorted candidate history | Deadline-checked scan with bounded top-K output/state |
| Automatic Work Result Git observation | One native observation per timer poll | One observation per valid lease/stamp, none for unchanged hits |
| Deterministic config failure | Repeated 5-second systemd restarts | Exit two excluded from automatic restart; candidate preflight before managed replacement |

History readers still serialize with each other. WAL snapshots can delay
checkpoint progress and increase WAL retention. Cold schema/index creation and
large derived repairs remain potentially expensive. Full Session/Runtime details
and Work Result's non-Git projections still cost work. External filesystem changes
are bounded-stale rather than instantly detected. Required ACK/reply/audit writes
remain on the critical path. sf production remeasurement is still needed after
an explicitly authorized deployment; none is claimed in this source-only task.

## Schema, migration and upgrade boundary

Phase one adds its derived tables/triggers; phase two adds the recent-peer index.
Both are additive and established by normal Database open. The read connection
and caches add no durable authority or new migration owner. The old phase-one
400,000-event measurement (6.425 seconds cold, 7.117 ms warm) does not measure
the final extra index and is not a final upgrade-time estimate.

The existing Console HTTP fields from phase one remain additive. Phase two adds
one App-only `automatic` argument, default false, plus workspace observation
provenance; there is no new direct model-facing tool or widened authority.
Server and its bundled WebUI/MCP App should be upgraded together. Store, Peer,
post-result and Work Result changes require no Runner wire-generation change.
The configuration preflight and service policy require the new Runner/CLI/service
integration to be installed later; upgrading the Server alone does not rewrite
an already installed systemd unit. No install operation occurred during this work.

## Validation and reproduction

Validation on the final implementation (documentation-only changes afterward):

| Scope | Result and evidence |
|---|---|
| Store full library | 255 passed; 4 manual benchmarks ignored in the ordinary suite (`store-final.log`) |
| Runner Registry full library | 321 passed (`registry-final.log`) |
| Tool Contracts full library | 278 passed (`contracts-final.log`) |
| Root Runtime full library | 3,247 passed; 3 ignored (`runtime-final.log`); includes Console, Work Result, post-record, authority and MCP surface tests |
| Window Store focused suite | 17 passed; 4 manual benchmarks ignored (`snapshot-after.log`); includes the six-case retention snapshot regression |
| Experimental Code Mode Window Console | 20 passed (`feature-window-final.log`) |
| Environment full library | 111 passed in the retained unchanged service-source run (`environment-e.log`); final focused service tests also 14 passed (`service-focused.log`) |
| CLI full library | 450 passed (`cli-full.log`), including generated-unit verification and preserved-running-process tests |
| Runner binary suite | 1,002 passed; 4 ignored (`runner-cli.log`) |
| Native config preflight integration | 2 passed (`config-preflight.log`) |
| Runtime WebUI | 186 passed across 27 files; admin build tests 2 passed (`frontend-test.log`) |
| Work Result App | 135 passed (`work-result-app.log`) |
| Frontend typecheck / generated output checks | Passed (`frontend-typecheck.log`, `frontend-dist.log`); dist checking rebuilt and compared both bundles and Markdown output |
| Formatting / whitespace | `cargo fmt --all -- --check` and `git diff --check` passed |

Stage-two development runs also explicitly covered 4 post-record tests, 12 Peer
projection tests, 22 Job-attention tests, and 4 workspace-cache tests. Those are
subsets of the root suite, not additional unique tests to add to its total.
The Peer benchmark was explicitly run; phase one's three manual Store benchmarks
were also explicitly run and remain documented separately. The two zero-test
filter attempts seen in development logs are not counted above.

Final verification is recorded in the closeout log directory below; ordinary
ignored tests are not counted as executed. The large historical benchmarks are
manual runs, separate from normal suite totals. Reproduction uses the same source
and fixed fixtures, not a copied production database:

```sh
cargo fmt --all -- --check
cargo test -p webcodex-store --lib
cargo test -p webcodex-store --lib window_activity -- --nocapture
cargo test -p webcodex-store --lib optional_projection -- --nocapture
cargo test -p webcodex-store --lib recent_peer_query_benchmark -- --ignored --nocapture
cargo test -p webcodex-runner-registry --lib
cargo test -p webcodex-tool-contracts --lib
cargo test -p webcodex --lib
cargo test -p webcodex --lib --features experimental-code-mode runtime_console_http::tests::window
cargo test -p webcodex-environment --lib
cargo test -p webcodex-cli --lib
cargo test -p webcodex-runner --bin webcodex-runner
cargo test -p webcodex-runner --test config_preflight -- --nocapture
npm --prefix frontend run typecheck
npm --prefix frontend test
node --test src/mcp_tests/work_result_app.test.mjs
npm --prefix frontend run check:dist
git diff --check
```

Logs: `/tmp/webcodex-perf-closeout-20261003/` (final review/verification),
`/tmp/webcodex-perf-continuation/` (C/D/E development proofs), and
`/tmp/webcodex-perf-20261002/` (phase-one and initial A/B proofs).
The new snapshot test was observed failing before its production correction;
the schema overflow was fixed without increasing the bound. Empty test-filter
runs are not acceptance evidence. Existing webcodex-core schema-helper dead-code
warnings and the experimental Code Mode fixture's unused `login` field were not
suppressed or presented as new regressions.
