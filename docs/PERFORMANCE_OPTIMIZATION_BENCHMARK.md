# WebPi performance optimization benchmark

Baseline commit: `2466a3e80d7c90c4b6c80a58978281995a8f8b45`

Test host: Windows 11. Baseline and optimized builds used the same source baseline, machine, test project, and local loopback transport. Benchmark fixtures were read-only or used `apply_text_edits(dry_run=true)` so the test project was not mutated.

## Changes under test

1. `read_files` request planning
   - deduplicate identical ranges
   - merge overlapping and nearby ranges (`merge_gap=20`)
   - cap a merged range at 400 lines
   - preserve `expected_read_revision` fences
   - reconstruct original read results when the ordinary `read_files` API requires per-request output

2. Windows single-file native search fast-path
   - directly execute native `rg.exe` for eligible single-file searches
   - retain the existing generated Bash search as the fallback
   - fall back for directories, include/exclude globs, unavailable `rg`, unsafe/out-of-project targets, or truncated native output
   - literal patterns preserve canonical semantics because Runtime escapes literal text before producing the Runner search payload

3. `search_and_read`
   - one model-visible call performs bounded search and immediate source inspection
   - search matches are converted into read ranges
   - duplicate/overlapping/nearby ranges are coalesced before compound output
   - source reads keep canonical SHA/read-revision semantics and the existing read result budget
   - coding discovery recommends this tool when search is predictably followed by source inspection

## Microbenchmarks

### `read_files`

| Scenario | Baseline | Optimized | Improvement |
| --- | ---: | ---: | ---: |
| 8 duplicate ranges | 11.92 ms | 6.05 ms | 49.2% |
| 8 adjacent ranges | 9.09 ms | 6.66 ms | 26.8% |
| mixed two-file ranges | 8.64 ms | 6.74 ms | 22.0% |

Planner request-count examples:

- duplicate ranges: `8 -> 1`
- adjacent/nearby ranges: `8 -> 1`
- mixed ranges: `8 -> 5`

### Windows single-file search

20 runs per scenario, same project and query shape:

| Scenario | Baseline mean | Optimized mean | Improvement |
| --- | ---: | ---: | ---: |
| literal | 406.29 ms | 106.95 ms | 73.7% |
| regex | 393.17 ms | 106.86 ms | 72.8% |
| context ±20 | 392.13 ms | 108.81 ms | 72.3% |

Directory and glob searches remained on the fallback path and showed baseline-equivalent latency.

## Task-level benchmark

Fixture: real Vue source (`index.vue`) in the test project.

Task:

1. locate four occurrences of `进入商店`
2. inspect source around those matches
3. prepare the same exact edit for the first occurrence
4. run the edit as `dry_run=true`

20 alternating baseline/optimized runs were used to reduce ordering bias.

### Final result after compound-output coalescing

| Metric | Baseline | Optimized | Improvement |
| --- | ---: | ---: | ---: |
| total mean | 464.63 ms | 129.11 ms | 72.2% |
| total P50 | 468.63 ms | 119.46 ms | 74.5% |
| total P95 | 529.71 ms | 176.44 ms | 66.7% |
| inspect mean | 457.03 ms | 122.32 ms | 73.2% |
| edit dry-run mean | 6.02 ms | 5.77 ms | approximately unchanged |
| outer model-visible calls | 3 | 2 | 33.3% fewer |
| average returned bytes | 23.9 KB | 10.7 KB | 55.3% fewer |

All benchmark runs located the expected four matches and produced valid inspection/edit results.

For the four nearby matches, compound inspection reduced four requested source ranges to one unique returned range (`index.vue:6-136`), avoiding repeated source text while preserving all match locations.

## Interpretation

The largest local latency gain comes from avoiding the Windows Bash process chain for eligible single-file searches. `read_files` coalescing mainly reduces redundant Runner work. `search_and_read` is primarily a model-round-trip optimization: its local execution time is similar to performing search plus read sequentially, but it removes one outer model/tool turn and, after coalescing, significantly reduces duplicated source bytes sent to the model.

A cross-call read cache was intentionally not added. After the other optimizations, bounded local reads are already on the order of a few milliseconds, while cache invalidation must remain correct when editors, Git, scripts, or other agents modify files outside WebPi.

## Validation performed

- formatting with `cargo fmt --all`
- focused `read_files` planner/coalescing tests
- focused `search_and_read` tests
- ToolDefinition/audit contract tests
- Adaptive MCP direct-surface tests
- coding-intent discovery test
- `cargo check -p webcodex`
- optimized dogfood builds
- real loopback A/B calls for search, compound inspection, fallback behavior, output budgeting, and task-level dry-run edit workflow

Warnings observed during validation were pre-existing unrelated warnings in the baseline tree; the optimization work introduced no known compile errors.

## Follow-up review: compound output and recovery

The Windows timings above are the contributor's original local measurements,
not new timings measured during the Linux review. They are not a claim that
end-to-end model response latency, directory searches, or Linux/macOS workflows
improve by the same percentage.

The follow-up review found that preserving member ranges for byte-ceiling
fallback had inadvertently re-expanded successful compound reads into duplicate
source blocks. The read core now retains the original member fallback while
returning each successful union once for `search_and_read`. Ordinary `read_files`
still preserves the caller's per-item ranges and order.

The compound read phase now also applies canonical actionable continuation and
sparse projection. `reads.suggested_call` carries the exact resolved Project,
explicit business Session, and observed read revision for a continued range.
Output indexes and `coalesced_read_count` follow the actual ranges, including
member ranges used after a merged read exceeds the byte ceiling, rather than
reporting an optimistic pre-execution plan.

Compound inspection also has an explicit ToolDefinition-owned exploration
projection for its nested search result. Session records retain sanitized,
deduplicated matched paths for handoff instead of treating the compound output
as an ordinary `items`-based search batch and silently losing those paths.

Runner-boundary regression fixtures exercise:

- Four overlapping matches produce one `6..136` source block, with all four
  match positions preserved.
- Read-budget truncation provides a parser-ready continuation; changing the file
  before that continuation rejects the stale revision without returning new text.
- A merged range that exceeds the raw-byte ceiling falls back to the original
  independently valid ranges and retains correct continuation indexes.
- Removing duplicate members does not drop a later deferred coalesced group.
- No matches finish without a file read.

Run the focused cross-platform checks with:

```bash
cargo test --locked -p webcodex --lib search_and_read
cargo test --locked -p webcodex --lib read_files
```

These fixtures validate correctness and the `4 -> 1` source-block reduction,
not native Windows process execution or the original latency percentages.
Native Windows execution and an exact-head Windows A/B benchmark remain separate
validation evidence.

## Native Windows verification during PR #526 review

The follow-up was exercised on Windows 11 (build 26100), using the
`x86_64-pc-windows-msvc` toolchain and an installed native `rg.exe`. The isolated
real-process fixture invokes the production native search helper, rather than
mocking its stdout. It covers a Unicode filename containing spaces, CRLF input,
match/file/count result modes, no-match exit status, and fallback selection for
directories, missing files, out-of-project paths, and include globs.

This exposed a single-file count defect: `rg --count --null` emits only the count
when given one file. The canonical parser requires a filename-bearing record.
Both native Windows and generated fallback commands now request
`--with-filename`; the regression verifies the exact `path + NUL + count` record.
The fixture failed before the fix and passed afterward. An ordinary argv test
also protects the flag without requiring real-process execution.

Run native verification explicitly:

```text
cargo test --locked -p webcodex-runner --features runner-real-process-tests runner_real_process_native_single_file_search -- --ignored --test-threads=1 --nocapture
```

This closes the native correctness gap for the scenarios listed above. It does
not remeasure the contributor's 20-run A/B workload, validate every timeout or
cancellation path, or establish an end-to-end model latency improvement. The
original timing percentages remain separately attributed measurements.
