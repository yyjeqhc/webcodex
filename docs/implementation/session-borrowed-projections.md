# Borrowed Session evidence projections

## Scope

Based on main@46c8fe0d (Active residency #883). Neither retention, residency,
permissions, Session identity, persistence format nor public tool schemas change.
Two projections previously deep-cloned every retained event simply to supply a
slice: summary counts and exact retained changed-path evidence. The event graph
already lives in an Arc-backed deque. They now borrow it directly; only the
requested public summary tail and projected paths are copied into owned output.
The exported slice API remains and delegates to the same borrowed implementation.
Recorder/business de-duplication, conservative malformed/reused-id behavior, source
order, retention totals, truncation flags, rule summaries and message summaries
are unchanged. No persistent cache, secondary index or new dependency is added.

Hot summary work still runs under the existing store lock, but no longer clones
its entire event graph there. Cold queries still use their existing temporary
materialization without promotion; this does not remove Cold deserialization.

## Parity and cost evidence

A frozen test-only copy of the parent summary is the full-JSON oracle. Tests cover
empty/large/wrapped event deques, zero/default/oversized limits, evicted history,
ambiguous correlation, original Arc pointer identity, Hot/Cold equivalence, and
path-attribution uncertainty. Selection policy itself was not rewritten.

On special, optimized dogfood, 2,000 synthetic retained events and 200 summaries
per case (same immutable record; parent first, borrowed second):

| Returned event limit | Parent total ms | Borrowed total ms |
| --- | ---: | ---: |
| 0 | 644.59 | 95.41 |
| 50 | 659.19 | 112.14 |
| 200 | 690.85 | 152.86 |

Every summary eliminates 2,000 intermediate event deep clones in this fixture;
the returned tail remains identical. These single-process synthetic timings are
not a prediction of network/tool latency or sf whole-service memory reduction.
No deployment, allocator tuning or history deletion is needed for the change.

```sh
cargo test --locked --profile dogfood -p webcodex-workflow-session --lib \
  borrowed_projection_cost_comparison -- --ignored --nocapture
```
