# Bounded tracked-file Glob filtering — 2026-10-25

## Problem and scope

Base: main `2371ec6730f16cee137a94835f7ac3a71364925b`.
The previous iterative matcher bounded stack frames but still enumerated
combinatorial star allocations when a later single star stopped at a slash.
A 23-byte pattern and 22-byte path already took about 14.6 ms per match on mini.
The previous all-double-star/EOF regression did not exercise this case.

A separate output defect used `Some(usize::MAX)` to disable rollup for literal
queries. It leaked an implementation sentinel into `depth`; on 64-bit systems
this integer is not exactly representable by JavaScript. The existing schema
already documents `null` as individual-file mode.

Changes are confined to the existing pure `project_listing.rs` implementation
and tests. There is no new glob library, Git cache, enumeration protocol, option,
filesystem traversal, or production service change.

## Implementation and preserved semantics

The existing matcher now uses one-row dynamic programming: bounded
O(pattern-byte-length × path-byte-length) work and O(path-byte-length) memory.
Literal and one-contiguous-star-run patterns avoid the allocation. A basename
equal to the full path is not evaluated twice.

The language is unchanged: `*` cannot consume `/`, `**` can, and `?`
consumes one non-slash **byte**. Slashes stay literal: `**/a` does not match
`a`. UTF-8 strings remain byte-matched; no character-class/brace semantics were
introduced. The independent recursive reference consumes one star or one pair,
rather than sharing the production star-run simplification.

Literal queries now select the existing `None` depth mode directly. Query
results remain individual files even with an explicit depth or pagination.
`depth_auto=false`, sorting, exact path identities, counts and offsets stay
unchanged. JSON contains an explicit `"depth": null`; no fake directory rollup
or missing field substitutes for it.

Sensitive/bulk path filtering, scope/glob/query order, source truncation,
pagination limits, Git acquisition, path authorization and symlink policy are
unchanged. Existing tests still cover those listing boundaries.

## Measurements

Measurements ran through WebCodex on the same mini checkout, before and after
the implementation, using the optimized `dogfood` profile. Compilation was
excluded. The ignored measurement test used one test thread, three samples per
case, and `black_box`. The test process had a 30-second external limit; old
inputs were capped below 32 bytes and escalation would stop after a 250 ms
median. No performance threshold is part of ordinary CI.

Baseline-only commit: `9a858de8`. Pattern family:
`"**" + "a*".repeat(k) + "b"`; path: `"a".repeat(n) + "/b"`.
Every result is false. Both n and k grow, so the sample exposes combinatorial
backtracking instead of merely timing a larger fixed-pattern scan.

| n / k | Pattern / path bytes | Old median ns | New median ns |
| --- | --- | ---: | ---: |
| 4 / 2 | 7 / 6 | 917 | 834 |
| 8 / 4 | 11 / 10 | 4,791 | 916 |
| 12 / 6 | 15 / 14 | 60,666 | 1,375 |
| 16 / 8 | 19 / 18 | 933,458 | 2,083 |
| 20 / 10 | 23 / 22 | 14,607,500 | 2,750 |

The largest measured case improves by about **5,312×**. Its observed old range
was 14.496–14.639 ms and new range 2.458–3.000 µs. Tiny-input timings are near
timer/allocation noise and are not a claim of general speedup.

For a fixed eight-path mixed matching/nonmatching `*.rs` set, 400,000 public
`glob_matches` calls per sample took:

| | Old | New |
| --- | ---: | ---: |
| Median | 98.023 ms | 42.657 ms |
| Minimum | 89.565 ms | 42.545 ms |
| Maximum | 104.288 ms | 42.690 ms |

That microbenchmark improves about **2.30×**. The mini was not CPU-pinned or
reserved exclusively, and three samples are diagnostic evidence, not a
population latency estimate.

### Layer boundaries

The earlier [tool experience baseline](https://github.com/yyjeqhc/webcodex/pull/1293)
recorded separate diagnostic samples on the deployed `d45f11fe` runtime:

| Layer | Observed evidence | What remains unknown |
| --- | --- | --- |
| Tool call from the orchestration cell | Three tracked-file query calls: 2,513 / 2,177 / 1,468 ms | Includes transport, Server, Runner and all processing; excludes model reasoning and is not a three-client study. |
| Git acquisition on mini | Five `git ls-files -z` subprocesses: 62.581–71.777 ms; median 69.268 ms; 133,649 output bytes | Separate subprocess samples, not a correlated breakdown of those RPCs. |
| Pure Glob filtering | Same-function before/after measurements above | Does not measure sorting, rollup or every query shape. |
| Server queue, Runner transport, serialization duration, network | Not isolated in this PR | No attribution of the residual time to any one layer. |

Those layers cannot be subtracted into a trustworthy residual breakdown. This
PR fixes proven local unbounded work; it does **not** claim a 2.30× or 5,312×
end-to-end listing speedup. The evidence does not justify a Git cache or LSP
readiness bypass. Search-count and LSP timing remain separate research.

## Verification and reproduction

- **368 core tests passed**, one manual measurement test ignored in the normal
  run. This includes the 22 focused listing tests.
- **165,165** short pattern/text pairs agree with an independent bounded recursive
  reference (pattern alphabet `a/*?`, lengths 0–5; text alphabet `ab/`, lengths 0–4).
- Explicit UTF-8-byte, slash, overlapping-prefix/suffix and long negative cases
  preserve semantics. The long adversarial case is only run against the bounded
  new implementation.
- Existing query test now checks omitted/depth-1/depth-2 requests, exact deep
  filenames, offsets and explicit nullable JSON depth. Empty-query core behavior
  and non-query finite-depth rollup remain covered.
- The manual measurement test passed before and after the change.
- Formatting and `git diff --check` passed.

```sh
cargo test --locked --profile dogfood -p webcodex-core --lib
cargo test --locked --profile dogfood -p webcodex-core --lib project_listing::tests::glob_matcher_measurements -- --ignored --exact --nocapture --test-threads=1
```

For the old commit, keep the bounded inputs in the measurement test; do not
increase the adversarial path length to the long CI regression. There is no
durable-data migration. Clients should already accept nullable depth according
to the published output schema.
