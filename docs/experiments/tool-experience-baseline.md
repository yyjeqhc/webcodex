# Tool experience baseline (2026-10-25)

## Scope and measurement conditions

Source baseline: `2371ec6730f16cee137a94835f7ac3a71364925b` on
`origin/main`, inspected in the existing mini checkout. The checkout was clean.
The open pull requests were unrelated older drafts (#1010, #1047 and #1087).

The connected Server and mini Runner reported exact/aligned build
`d45f11fee169`. They do **not** include #1291. Connected-runtime observations
must not be attributed to the newer source baseline. No service is deployed or
restarted by this work.

Bytes below are UTF-8 or serialized JSON bytes, **not tokens**. Deterministic
fixtures measure response projections and declared scenario calls, not a live
ChatGPT, Claude Code or Codex conversation, client error rate or network latency.
HTTP tests use the real in-process MCP handler with controlled fixtures; they do
not launch a real Host.

## Current-state matrix

| Feedback | Current implementation and existing evidence | Remaining work / decision |
| --- | --- | --- |
| Verbose successful execution/read results | #1289 and #1291: `src/mcp/cli/text.rs`, `src/mcp/cli/text/compact.rs`, `src/tool_runtime/result_projection.rs`; CLI text snapshots and execution projection tests | Reuse these projections. Do not repeat their compression work. |
| First Job observation looks like lost logs | #1291: `crates/webcodex-runner-registry/src/job_updates.rs`; matching cursorless tokens establish an initial baseline, actual resets remain explicit | Already fixed on main. The older deployed build still exposes the old behavior. |
| Timeout request differs from effective Runner ceiling | #1291: `src/tool_runtime/structured_execution.rs`, process/script/shell/detached admission and focused process tests | Audit other execution forms and descriptions; do not add another timeout policy. |
| Reads mechanically continue beyond an explicitly requested range | #1291: `src/tool_runtime/read_files.rs` and read-files tests | Already fixed. Genuine truncation still has snapshot-fenced continuation. |
| LSP available before the workspace is indexed | #1288/#1291: `crates/webcodex-lsp/src/supervisor.rs` and readiness tests, including document symbols | Preserve readiness semantics. No speculative index bypass or shorter timeout. |
| Manifest route/Schema does not match the callable surface | #1290: `src/mcp/tool_exposure.rs`, `src/tool_runtime/surface.rs`, `src/mcp/cli.rs`, tool-exposure/CLI tests | Already repaired for the inspected cases. Preserve full exact schemas and admission; measure discovery cost separately. |
| Nested argument errors and redundant search choices | #1290: tool-contracts parsing/request schemas and argument-path tests | Already repaired; do not add broad aliases or open schemas. |
| No comparable final CLI/Adaptive response metric | `src/tool_runtime/model_ergonomics_telemetry.rs` measures structured results; `src/mcp.rs` captures it before CLI projection | Add payload-free final response measurement to the existing ActionAudit. Preserve the meaning of the old metric. |
| Repeated execution/Host advice in descriptions | Canonical `ToolDefinition`, `src/mcp/discovery.rs`, CLI overrides, Guidance Contributors and Context Projection all exist | Audit ownership and use the existing projection layers; avoid a parallel prompt framework. |
| Bootstrap includes optional Goal/continuation/collaboration detail | `src/tool_runtime/guidance/contributors.rs`, `context_projection.rs`, `model_workflow.rs` | Measure the current default chapter and move genuinely optional guidance only with a discoverable, tested loading path. |
| Editing requires copying a revision | `read_files` returns explicit evidence; `edit_project_files` and Runner preflight/recheck preserve source/version/transaction boundaries | No Session-latest revision inference. A shared Session is not a caller-bound edit snapshot. Any simplification needs independent provenance/concurrency proof. |
| Git/glob enumeration, count search and cold LSP may be slow | Existing bounded Runner implementations and LSP readiness tests | No bottleneck claim without separated measurements. Avoid cache/index/protocol changes based on end-to-end intuition. |

## Existing infrastructure to reuse

- `src/tool_runtime/model_ergonomics_telemetry.rs`: canonical outcomes, input
  normalization, batch cardinality, Job convergence and wait facts without payloads.
- `src/mcp_tests/model_ergonomics.rs`: real HTTP-handler ActionAudit checks.
- `src/mcp_tests/cli/text_projection.rs` and its snapshots: compact text and
  preserved failure/recovery evidence.
- `src/mcp_tests/cli.rs`, `tool_exposure.rs` and tool-contracts tests:
  callable surfaces, canonical schemas, permission filtering and nested errors.
- Existing `json_measurement::serialized_json_len`: counts serialized bytes
  without allocating a second complete payload.

## Final-response measurement

The MCP adapter records `summary.response_projection` only after its final
projection, for ordinary audited tool results. It records a version, surface,
serialized result bytes, UTF-8 bytes in text content blocks, structured-content
bytes when present, and text/non-text block counts. It stores no arguments,
commands, paths, text, resource bodies or schema contents.

The existing `model_ergonomics.serialized_result_bytes` retains its original
canonical/structured-result meaning. A CLI result has no final structured content;
that measurement is null, not zero and not copied from the pre-projection metric.
The final JSON size includes metadata/native blocks; text size only counts text
content blocks. Neither is a tokenizer or proof of what a particular Host retains.
Streaming artifact exports and pre-result transport failures are not represented
as successfully measured tool results.

## Observed baseline costs

On the frozen starting source, `cargo test --locked -p webcodex --lib
mcp::tests::cli -- --nocapture` passed all 28 tests. The existing fixtures report:

| Fixture / discovery surface | UTF-8 bytes |
| --- | ---: |
| CLI tools/list, 17 descriptors | 34,800 |
| Adaptive tools/list without Apps, 41 descriptors | 69,189 |
| Adaptive tools/list with Apps, 74 descriptors | 105,055 |
| CLI find_definition result | 38 |
| CLI 17-file directory listing | 289 |
| CLI terminal Job observation | 139 |
| CLI command nonzero result | 283 |
| CLI clean workspace | 113 |
| CLI edit dry run | 188 |
| CLI partial read with fenced continuation | 304 |

These are existing main behavior, not savings introduced by this PR. The
discovery surfaces expose different capabilities and must not be treated as
identical tasks with interchangeable authorization.

A connected-runtime probe of `list_project_tracked_files` (literal query
`tool_definition`, limit 40) returned 31 paths in 2,513 / 2,177 / 1,468 ms,
measured around the connected tool call in Host Code Mode. This includes
unseparated adapter, Server, Runner, transport and scheduling costs. A preliminary
five-sample subprocess probe on mini measured `git ls-files -z` at
62.6–71.8 ms (133,649 output bytes), `rg --files src` at 76.3–136.8 ms,
and `rg --count read_revision src` at 72.0–173.7 ms. The probes used the warm
existing checkout while a focused Rust compilation could be active. They are
diagnostic samples, not a quiet-machine benchmark or a network timing estimate.
They do not support a claim that Git enumeration dominates end-to-end cost.

The tracked-file probe also exposed an internal `usize::MAX` rollup sentinel as
`depth` in literal-query output; JavaScript cannot represent that integer exactly.
The same source path remains on main in
`crates/webcodex-core/src/project_listing.rs`. Investigate this projection and
the multi-star glob matcher independently of the baseline PR.

## Representative fixture ledger

`src/mcp_tests/experience_baseline.rs` validates canonical input schemas and
parser admission, exact continuation arguments, read-to-edit revision wiring and
preserved decision evidence. It also fetches exact manifests through the HTTP
handler for both surfaces and validates their inner arguments and gateway shell.

| Scenario | Declared calls | Adaptive result JSON B | CLI result JSON B | CLI text UTF-8 B |
| --- | ---: | ---: | ---: | ---: |
| Search/read/edit/test failure analysis | 5 | 1,388 | 639 | 353 |
| Locate code in an unfamiliar repository | 2 | 727 | 275 | 156 |
| Pending Job through successful observation | 3 | 938 | 604 | 409 |
| Pending Job through timeout observation | 2 | 705 | 492 | 355 |
| Unknown execution through reconciliation observation | 2 | 752 | 547 | 410 |
| Discover, obtain Schema, construct execution arguments | 3 | 4,511 | 4,351 | 3,967 |

The two surfaces project identical controlled facts. These differences reflect
existing compression, not a new performance optimization. The exact-manifest
scenario is dominated by the complete callable Schema, which is intentionally
retained. Declared calls count fixture steps; no child command or source edit is
executed by this ledger. Runtime concurrency/replay safety remains covered by
the existing Job and guarded-edit suites, not by these synthetic revisions.

CLI structured-content bytes are null for each result. Their scenario aggregate
adds zero for absence; this does not describe an existing zero-byte JSON document.
The report explicitly leaves Host tokens, end-to-end latency and client parameter
error rate unknown. No fixed byte ceiling is raised to make a regression pass.

## Validation and results

Validated on mini with the final Rust source frozen:

- `cargo test --locked -p webcodex --lib mcp::tests`: **432 passed,
  0 failed, 1 existing ignored**. This includes the new eight-case HTTP audit
  matrix, exact-discovery/argument checks and scenario projections.
- The targeted final-response and scenario suites each passed independently
  before the complete MCP run.
- `cargo fmt --all`, `git diff --check` and
  `python3 scripts/check_markdown_links.py` passed (1,166 links, no missing target).

This PR improves measurement, not execution latency or model success rate.
No input Schema, admission, permission, Job protocol, revision handling or replay
behavior changes. Audit remains best effort; a missing measurement never changes
the tool outcome. The new summary field is additive and needs no database migration.
