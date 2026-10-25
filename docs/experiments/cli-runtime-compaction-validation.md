# Runtime-backed CLI compaction validation

## Result and measurement boundary

PR3 reduces presentation overhead while restoring missing execution context. In the measured Runtime samples, project inventory is 284 → 200 UTF-8 bytes, contextual search 343 → 240, a Job start receipt 1,070 → 789, and ordinary default Shell success 61 → 6. Explicit Process cwd is restored for both success and exit-7 failure, adding 9 bytes to each receipt. Timeout, outcome-unknown, truncation and version-recovery evidence remain visible.

Across the series, the raw `run_job` start sample is **764 bytes (PR1 baseline) → 1,070 bytes (PR2 adds truthful budget/ownership evidence) → 789 bytes (PR3 presentation)**. The final receipt is 25 bytes larger than the original baseline. The 281-byte reduction is relative to PR2's complete safety facts, not a reduction across the whole series.

The [machine-readable ledger](cli-runtime-compaction-validation.json) contains all 22 raw before/after measurements, ten complete normalized snapshot pairs, exact test commands, source manifests and binary hashes. No Host token count, ChatGPT/Claude Code/Codex conversation cost, or end-to-end speed improvement was measured.

## Producer and reproducibility

Measurements ran on mini, `aarch64-apple-darwin`, Rust 1.96.0, using the default unoptimized test profile. The fixture dispatches through the actual Runtime and `cli_call_for_test`, then the real MCP/CLI projection. Its local Runner transport executes bounded native commands (`printf`, `pwd`, a controlled Python file), real file reads and search. Job state updates use the real outputs of those commands. Timeout/lost-completion cases intentionally inject the documented transport boundary; this stage does not claim a native Server/Runner service run.

Counts are taken from raw CLI text before normalization: UTF-8 bytes and Rust `chars()` Unicode scalar values. Serialized MCP response bytes are secondary diagnostics in the ledger. These are individual paired captures, not workload-weighted averages. Opaque revision digit widths can change in a later correctness run; no saving is attributed to that variation.

| Artifact | Before | After |
| --- | --- | --- |
| Source HEAD | `5c5a224f6e10d1ce68b93c1e1d9858477eb6f636` plus recorded foundation patch | `4cba98a56e9fdab142d16391cc8344d732c70105` plus recorded projection patch |
| Binary SHA-256 | `d6237e1bfd73d085c01b092e8ec6984faf0e4f72d3a1c271bc60797e7d50b4a2` | `158d85d332e4e9775803fe68ad153808924e400d7e750eecd530b17c44c7e16c` |
| Manifest SHA-256 | `2366a7d4c60685947d640598d9861e63d69b567ac1f6be5960f34a1df3026e49` | `9267f1387f81f7464b6bc76e707b6824f46efff6a073dbdf08b7c6f6e225ad20` |

The preserved binaries and logs are under `/tmp/webcodex-pr3-before-5c5a224f-w2bzu_1f` and `/tmp/webcodex-pr3-after-4cba98a5-lf4xhxsd`. The source manifest records the file hashes and patch hash used by each successful build. Temporary binaries are retained for the final same-producer native replay; they are not repository artifacts.

Both binaries were built with:

```sh
cargo test --locked -p webcodex --lib --no-run --message-format=json-render-diagnostics
```

The authoritative fixture is [`src/tool_runtime/tests/cli_runtime_projection.rs`](../../src/tool_runtime/tests/cli_runtime_projection.rs). To repeat its standard snapshot verification on the current tree:

```sh
cargo test --locked -p webcodex --lib cli_runtime_projection:: -- --nocapture --test-threads=1
```

`WEBCODEX_UPDATE_CLI_SNAPSHOTS=1` is only for an intentional refresh. The recorded final verification ran without it, using the same compiled binary.

## All Runtime samples: PR3 before/after

Negative byte deltas mean shorter output. The two positive deltas restore explicit cwd; they are correctness improvements.

| Sample | Raw UTF-8 bytes, before → after | Unicode scalars, before → after | Δ bytes |
| --- | ---: | ---: | ---: |
| `projects` | 284 → 200 | 284 → 200 | -84 |
| `files` | 17 → 17 | 17 → 17 | 0 |
| `tracked_folded` | 91 → 91 | 91 → 91 | 0 |
| `tracked_flat` | 92 → 92 | 92 → 92 | 0 |
| `search_context` | 343 → 240 | 339 → 236 | -103 |
| `search_file_context` | 327 → 289 | 323 → 285 | -38 |
| `shell_success` | 61 → 6 | 61 → 6 | -55 |
| `shell_failure` | 297 → 297 | 297 → 297 | 0 |
| `shell_nondefault_cwd` | 129 → 129 | 129 → 129 | 0 |
| `process_success` | 36 → 6 | 36 → 6 | -30 |
| `process_nondefault_cwd` | 102 → 111 | 102 → 111 | +9 |
| `process_failure_nondefault_cwd` | 335 → 344 | 335 → 344 | +9 |
| `job_started` | 1070 → 789 | 1066 → 785 | -281 |
| `job_running` | 109 → 109 | 109 → 109 | 0 |
| `job_completed_truncated` | 358 → 358 | 358 → 358 | 0 |
| `jobs_partial_unknown` | 1153 → 1116 | 1149 → 1112 | -37 |
| `read_versioned` | 101 → 101 | 97 → 97 | 0 |
| `edit_unknown_revision` | 469 → 469 | 469 → 469 | 0 |
| `read_truncated` | 3434 → 3434 | 3434 → 3434 | 0 |
| `read_recovered` | 3065 → 3065 | 3065 → 3065 | 0 |
| `shell_timeout` | 954 → 954 | 954 → 954 | 0 |
| `shell_outcome_unknown` | 787 → 787 | 787 → 787 | 0 |

## What changed and what remains

- [`src/mcp/cli/text.rs`](../../src/mcp/cli/text.rs) renders validated search contexts in before/hit/after order. Match lines use `path:line:text`; context uses `path-line-text`. Adjacent match groups remain separated, so overlapping context cannot consume a later match marker. Unknown context shapes fall back to field rendering.
- Compound search keeps the coalesced read body, read revision, match positions and meaningful read counts. Only proven `read_success: true` and `read_error: null` are omitted. Partial/error/truncated reads remain diagnostic.
- [`src/mcp/cli/text/compact.rs`](../../src/mcp/cli/text/compact.rs) keeps a queued Job's identity, `command_started: false`, requested/dispatched/effective budget, policy source/basis/notice, Runner ownership and exact callable continuation. The measured fixture has unknown Runner policy, so `effective_timeout_secs: null` and its warning remain. Only a redundant exact observation token/canonical continuation descriptor and empty routine stream fields are omitted.
- Batch Job totals are omitted only when `output_truncated` is exactly false and requested, returned and visible item counts agree. Classification counts, log totals, retained/returned lines, full diagnostic nulls and unknown fields remain; those log counters have different meanings.
- Inventory omits an empty description and the exact null-profile/not-configured pair. It retains identity, root path, connectivity, source and capability evidence, including `git_available: false` in this fixture.
- Workspace symbols retain file/range, container/index/loss evidence and every original `kind_code`, including unknown or inconsistent values. Extended or malformed range shapes fall back to generic rendering. Actual native symbol byte measurements are still pending.
- [`src/tool_runtime/result_projection/execution.rs`](../../src/tool_runtime/result_projection/execution.rs) captures original request context. Default cwd/shell can be omitted only after proven ordinary terminal success and only when both the request and resolved values prove them redundant. Session context and explicit cwd/shell/login settings stay visible. Process/Script failures retain cwd. The shared canonical result remains available to audit before projection.
- Exact repeated LSP code/message text is shown once; the code, status, other diagnostics and recovery information remain. This change does not bypass or alter index readiness.

The actual contextual-search output now includes:

```text
isError: false
==> query 0
src/sample.txt-1-before
src/sample.txt:2:needle 中文
src/sample.txt-3-between
--
src/sample.txt-3-between
src/sample.txt:4:needle again
src/sample.txt-5-after
context_after: 1
context_before: 1
path: src
pattern_mode: literal
```

The `isError` line above is the fixture snapshot wrapper; it is not included in the raw CLI byte counts. Full before/after texts, including the complete Job recovery JSON, are in the ledger.

## Regression and validation evidence

The effective before run saved all 22 samples and then failed one aggregate presentation assertion: unordered verbose context groups, redundant default Process fields, and omitted explicit cwd on real Process success/failure. `pwd` had produced the actual fixture `src` directory; the controlled Python file had produced that cwd, stderr `denied` and exit code 7. Runtime failure/unknown-state handling was not relaxed.

Two earlier attempts failed because the test responder did not handle `start_process_job` as native argv with typed terminal Job evidence. Those are recorded as harness faults, excluded from the product-red count and measurement set. The corrected helper preserves the actual async path and updates the same Job only after a real process completion.

| Validation | Result |
| --- | --- |
| Effective before: Process neighbors | 4 passed |
| Effective before: Runtime fixtures | 3 passed; 1 expected presentation failure, after 22 samples |
| Effective before: CLI neighbors | 30 passed; 1 opt-in replay ignored |
| After: Runtime snapshot refresh | 4 passed, 22 samples |
| After: CLI snapshot refresh | 22 passed; 1 opt-in replay ignored |
| After: ordinary-mode CLI/Runtime/projection snapshot verification | 72 passed; 1 opt-in replay ignored |
| After: exact execution context and timeout boundaries | 18 passed |

The two ordinary-mode after phases therefore cover **90 passing tests and one explicitly ignored native replay**. Refresh phases overlap and are not added as independent coverage. Test counts come from terminal libtest summaries, not example output embedded in a test. Boundary coverage includes default/normal/tool-cap/Runner-cap budgets, prestart failures, started timeout and unknown outcomes, same-Job recovery, Session cwd, output-loss notices and unknown projection fields. Formatting and diff checks passed; GitHub CI is a separate PR gate.

After the old binary was captured, the fixture normalizer was corrected to replace only its two known root spellings in descending length. This removes macOS `/private` residue from three normalized cwd snapshots and makes them portable to Linux. Raw byte counters, recovery parameters and the old renderer binary are unchanged; the normalized length difference is not counted as a saving. The native Python harness also gained explicit empty-env-file isolation and successful-symbol evidence checks. These post-build foundation changes are recorded in the evidence directory and foundation commit `4cba98a56e9fdab142d16391cc8344d732c70105`.

## Native validation still pending

The final isolated smoke will use one Server/Runner pair and the repository's `dev` binaries via `--bin-dir target/debug`. Its executable hashes and actual outcome will be recorded after execution; this is functional/output validation, without an optimized-production performance claim.

`scripts/e2e_cli_runtime_output.py` captures native inventory and workspace-symbol Runtime responses. `scripts/measure_cli_runtime_projection.py` replays the exact same captured producer response through the two preserved CLI implementations. The ignored replay test requires its capture/output environment and fails if they are missing; an ordinary ignored test is not successful native coverage. CLI and Adaptive LSP captures are separate bounded requests and may see different cold/warm readiness states. A successful symbol sample must contain the actual `fixture_symbol`, `src/lib.rs` and a valid position.

`work_on_project` receives no additional compaction here: the existing bootstrap projection already separates workflow guidance and keeps the necessary instructions, catalog, project identity and recovery. Its native bootstrap output will be captured for assessment, with no invented improvement figure. Native successful dry-run/edit, explicit Script/Shell context, Runner-policy Job cap/timeout and scope-denial checks remain part of that single later smoke.
