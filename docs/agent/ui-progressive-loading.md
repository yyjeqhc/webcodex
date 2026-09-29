# Progressive WebUI inventories and lazy Work Result files

## Evidence and scope (2026-09-28)

The original caller-visible fleet contained 138 Projects, including 53 on special.
These are authorized inventory counts, not a global database count. Work was done
in `/root/git/webcodex-review` on `refactor/tool-surface-categories`, starting at
`ec0716c9246de502cb16b35610fd1c178c89a442`. No live deployment or service was changed.

A read-only SQLite aggregation on sf `/var/lib/webcodex/webcodex.db`,
`action_events`, ended at 2026-09-28 13:21:14 UTC:

| Window | All ActionAudit events | work_result_state | Share |
|---|---:|---:|---:|
| Retained history | 304,623 | 6,037 | 1.98% |
| Rolling last seven days | 31,253 | 6,037 | 19.32% |

This query grouped all retained `action_events` by canonical `operation`, not only
model `toolsCall` actions. It is not the earlier 37.6% sample, a model-efficiency
metric, or proof of any post-deployment savings. No production first-paint latency
benchmark or browser/Host end-to-end performance measurement was performed.

## First paint does not wait for all Session details

`overview` accepts `include_sessions=false` for registry-only primary data. The
omitted field retains the full historical REST behavior. Both paths apply the
same caller visibility. Primary results report `detail_level=primary`, zero
Projects scanned for Sessions, and explicit partial coverage; unscanned Sessions
are never presented as a proven empty inventory.

`useRuntimeOverview` paints primary data first. Only the Runtime and Work Session
surfaces hydrate the full retained-Session aggregation. Work Windows and Goals
continue using primary registry data; their own exact domain requests are not
blocked by a fleet-wide Session scan. The current bounded Project registry is
still available for Window/Goal pickers and exact names.

The Projects page requests 24 rows initially, not the entire 2,000-row ceiling.
Load more increases the bounded prefix by 24; Show less returns to 24. Search and
Runner filters restart at the first page. This is explicit prefix pagination,
not a silently auto-drained cursor or a background request for every Project.
Rows remain usable during ordinary refresh. Credential/filter changes clear the
old selection and stale responses cannot overwrite the current one.

`useVisibleRefresh` owns visibility, event coalescing and timer cleanup for
Overview, Projects, Project Sessions, Goal/Session workspaces, Agent inventory
and Window collaboration. Hidden tabs do not issue periodic observation reads;
focus/online/visibility events coalesce into one 150 ms resume. Window collaboration
also cancels an in-flight read when paused; an already-pending send is unaffected.

Goal and Session resources use `ObservationRequest` slots for cancellation,
null/error completion, stale-response fencing and single-flight release. List,
detail and message resources progress independently. Periodic refresh skips a
busy resource; an explicit refresh queues at most one follow-up behind it instead
of canceling slow work. A canceled old read cannot release a newer slot or revive
its queued callback. These slots are not used for mutations or lease renewal.
Agent Endpoint renewal retains its own schedule while the page is hidden; only
its inventory observation timer follows document visibility.

## Exact Session Job state

Work Result v13 retains an explicitly supplied business `session_id` on its normal
state refresh. With that exact authorized Session and Project, `jobs` projects up
to eight caller-visible Server registry snapshots, active Jobs first, then recent
terminals. `active` covers the selected Session even when items are truncated.
Only Job identity, tool, lifecycle, outcome and recovery state reach the card;
commands, logs, output, observation tokens and Runner payloads stay out.

Without an explicit Session, `jobs.available=false`; Window-linked Session
activity does not select Job inventory. Snapshot reads neither poll Runner nor
consume passive model attention, register wakes or create Session events. A
terminal transition changes `state_version` and appears on the existing next
refresh without a model turn. Unchanged terminal history is stable state, not a
new event. Active Jobs reuse the same visibility/backoff timer; hidden and
teardown fences remain unchanged.

## File lists and immutable source ownership

The Work Result passive producer now requests Git metadata only: no diff hunks
or untracked file bodies on each card refresh. Its ordinary workspace summary
retains the existing eight-file bound. Both live and final file rows use the same
folding component; the diff DOM and request are created only on expansion.
Initial display is five rows, with explicit More, Show fewer, Collapse all and
per-file toggles. Collapsing/re-expanding reuses the exact cached result.

The existing App-only `work_result_state` accepts an optional `files` request.
Omission keeps its state behavior. A first workspace request creates an immutable
snapshot; later metadata pages and diffs pin that snapshot id. Final Changes use
their existing sealed snapshot and exact Session. Both paths share the existing
Changes registry, bounded metadata parser and safe Git diff producer rather than
independent live/final implementations. The existing `changes_file_diff` remains
valid for final per-file reads.

Metadata pages contain at most 24 files. Initial final payloads are still bounded
to 24, while the registry retains up to 2,000 metadata entries under the existing
32 KiB producer budget. Producer truncation and sensitive-path exclusions remain
explicit; More cannot recover data omitted by a source bound or path policy.
Diffs retain the existing 48 KiB / 1,200-line limit. Every page/diff reauthorizes
its exact caller, Project, optional Session, snapshot and advertised path. A final
snapshot cannot be read through the unassociated live-workspace route.

A live inspection snapshot represents **net HEAD-to-working-tree changes**. It
is distinct from both sealed task changes and the status summary's separate
staging information. Staged-only intermediate content reverted in the working
tree may therefore have no net file diff. The UI labels this snapshot as pinned;
Refresh file snapshot explicitly observes newer edits. A newer live status must
not silently rewrite a diff the user is reading. Old page/diff completions cannot
retarget a refreshed view. Git capture uses the existing private index and safe
configuration path; it does not change the real index, refs or working tree,
though unreachable observation blobs/trees can be written to Git's object store.

### Independent snapshot retention quotas

Each Runtime's registry retains at most 32 sealed final results / 8 per caller,
and independently 8 live inspection snapshots / 2 per caller. Runtime clones
share these budgets; independently constructed runtimes do not share entries or
quotas. Both classes share identity checks and diff implementation, but an
insertion may only evict its own class. The combined hard cap is 40 snapshots
per Runtime, not a process-wide cap across independently constructed runtimes.
The existing 24-hour expiry and replay-without-TTL-extension remain; 24 hours is
a maximum age, not guaranteed retention under same-class pressure. Dropping the
last Runtime clone or restarting the Server drops these in-memory snapshots. This separation
changes no App request/schema/resource URI and requires no Host schema refresh.

## Mounted-card request budget

Changed/recent state refreshes at the existing 10-second base. Consecutive equal
states back off, capped at 60 seconds for an active but unchanged execution and
120 seconds for an idle card. Failures also back off to at most 120 seconds.
Fresh changed state or an explicit refresh restores responsiveness. Timers are
single-flight, hidden views pause, and teardown ignores late results. The existing
30-minute idle pause remains. Explicit file inspection has a longer bounded wait
than the lightweight periodic refresh and never auto-loads every file.

A deterministic fake-clock regression permits no more than eight state requests
in ten minutes of steady idle state. This is a test of scheduling behavior, not a
claim that production requests or latency have fallen by that ratio.

## Host refresh and regression ownership

This change preserves ordinary model Direct tools and adds no new tool name.
It deliberately changes one existing App-only request/output contract and ships
`ui://webcodex/work-result/v12`; v11 remains readable for previously shipped
resources. Deployment must coordinate **one Host schema/resource refresh**. A
Server restart alone does not replace already cached Host schemas or mounted HTML.
After that refresh, folding, pagination and backoff do not dynamically modify the
tool inventory. Batch this with the earlier branch's intentional surface change.

Regression ownership:

- `frontend/test-v2/progressive-loading.test.tsx`: primary/full separation, a
  138-Project fixture, explicit 24-row increments and shared visibility timers.
- `src/runtime_console_http.rs`: partial coverage and principal isolation before
  Session hydration; existing full overview behavior remains covered.
- `src/mcp_tests/work_result_app.test.mjs`: folding, cached diffs, actual subsequent
  pages, snapshot/principal consistency, late responses, backoff and teardown.
- `src/tool_runtime/tests/work_result/frozen_changes.rs`: real Git snapshot
  immutability, live/final pages beyond 24 files, secret/path fences and reauthorization.
- Existing ToolSpec, App-only admission, resource and serialization-budget tests
  guard the integration boundary instead of a second hand-written tool inventory.
