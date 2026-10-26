# Window polling consistency

This implementation note describes the Runtime V2 Window workspace's observation
and refresh contract. It applies to the inventory, selected Window detail, and
Project family navigation. Window keys remain selectors rather than authority;
the Server authorizes every request independently.

## Observation time and ordering

The navigation timestamp is `last_seen_at_ms`, the latest Server-observed request
activity for that Window. Browser refresh time never substitutes for it.
`last_meaningful_activity_at_ms` is a separate clock: a newer request can update
the Window's last-seen time without representing new meaningful work. The UI
labels both facts separately.

Inventory and exact detail observations merge by Server activity time. A newer
timestamp wins; a local request sequence breaks equal-timestamp ties. A delayed
inventory or full-history response can add retained history without restoring
older running state. First-seen time takes the minimum known value, and tool-call
and meaningful-activity clocks take the maximum. Missing optional facts do not
erase facts from a more recent observation.

A primary or full detail read updates an existing summary row. It cannot insert a
Window omitted from a filtered inventory. New active Windows enter through the
authorized liveness response and are retained in a bounded recent list until the
next inventory refresh. Recent rows use the same timestamp and sequence rules as
ordinary inventory rows. A truncated linked-Session page never replaces a known
Session total with its returned-page count.

## Independent bounded refreshes

| Read | Normal visible cadence | Bound and purpose |
| --- | --- | --- |
| Window inventory | 30 seconds | Refresh the prefix the user has opened; explicit load-more opens one additional page. |
| Window liveness | 3 seconds | Read the Server's bounded in-flight registry independently of retained history. |
| Selected Window primary detail | 3 seconds | Read up to 80 recent activities and current active requests. |
| Selected Window full detail | 30 seconds | Hydrate up to 2,000 activities, linked Sessions, and retained detail. |
| Non-selected completion catch-up | Observed active-to-idle transition | At most four exact inventory reads in one serial batch; the selected Window already has primary detail polling. |

Each read class admits at most one in-flight request. A slow request is allowed to
finish across polling ticks. Full-history attempts, including failures and burst
catch-up, share the 30-second cadence. A failed full read therefore cannot turn
each primary tick into another 2,000-row request. If a truncated primary head no
longer overlaps the previously observed head, the catch-up need remains pending
until a full read is allowed. This bounds load while retaining eventual history
completion.

When hidden, the document stops timers and aborts outstanding requests. Returning
to visibility, focus, or online state coalesces refresh into one short delay.
Responses from an aborted or replaced request are ignored even if the transport
later resolves successfully.

Polling ownership is local to each mounted workspace. This implementation does
not coordinate requests between browser tabs; adding such coordination would
require an explicit principal and target ownership contract.

## Authority changes and failure state

A detail is visible only while its owning client, opaque local authority
revision, Project selection, selected Window, and enabled/detail settings still
match. The render path checks this ownership before effects run, so switching
credentials or clients with the same selected Window cannot expose one frame of
the previous principal's detail. The reset aborts old requests and clears detail,
inventory, paging metadata, and full-history retry state.

`RuntimeV2Client.authorityRevision` changes when its credential changes or is
cleared. It contains no credential value. This matters because the application
reuses one client instance across authentication changes.

HTTP 401, 403, and 404 clear affected retained data. A 401 also invokes the
credential boundary. A missing exact Window is removed from the inventory while
its selected key remains explicit; omission from a bounded page does not silently
retarget the user's selection. Temporary failures retain previously authorized
data and report it as stale. In particular, successful primary polling does not
make failed full-history hydration appear fresh.

The Server remains the authority for resource visibility. Client retention,
request sequencing, and cancellation are presentation fences, not authorization
substitutes. The collaboration target semantics are defined in
[Manual Multi-Window Collaboration](../agent/manual-window-collaboration.md).

## Reproduction and measured scope

Run the focused hook regressions and reproducible load measurement from
`frontend`:

```sh
npm run test:runtime -- test-v2/window-polling-consistency.test.tsx test-v2/runtime-state.test.tsx
npm run test:runtime -- test-v2/window-load-benchmark.test.tsx
npm run typecheck
```

The tests exercise actual hooks with deferred responses, controlled visibility,
and clocks. They cover late responses, same-key client/credential/scope changes,
401/403/404 clearing, 503 retention, full-history retry throttling, single-flight
polling, and bounded completion reads.

The synthetic load fixture contains 2,000 completed calls, 100 Projects, and 40
referenced Sessions. The initial jsdom measurement rendered 32,126 DOM nodes;
React Profiler reported a 1,216 ms mount and ten metadata-only rerenders had a
377 ms median and 645 ms p95. These are measurements of the test environment,
not browser frame times or production latency guarantees.

For two independent hook trees over 27 simulated seconds, both the initial
baseline and the consistency fix issued two inventory, eighteen liveness, twenty
primary, and two full reads. When the next 30-second interval returned 503 for
full history, the initial baseline issued twenty failed full reads and reported
the detail as available. With the consistency fix, the same interval issued two
failed full reads and correctly retained the stale state. Unchanged activity
arrays still changed identity on nine of nine primary updates in this measurement;
that is independent of the polling correctness result.

Both hook trees run in the same jsdom document. They establish deterministic
request-count behavior for separate consumers, not real-browser multi-tab
behavior, cross-tab coordination, paint timing, or layout cost. The load test
prints DOM counts, React Profiler samples, request counts, and activity-array
identity changes so later rendering changes can be compared using the same
fixture.
