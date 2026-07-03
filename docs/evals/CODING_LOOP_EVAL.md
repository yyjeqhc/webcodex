# Coding Loop Eval

This document describes the minimal WebCodex coding-loop eval harness in
`scripts/eval_coding_loop.sh`.

## Purpose

The harness measures deterministic WebCodex runtime and tool-loop mechanics for
remote coding workflows. It does not evaluate LLM intelligence, prompt quality,
Codex CLI behavior, tree-sitter, LSP, or diagnostic parsing.

The current target is a low-noise local comparison between two scripted flows:

- `baseline`: a traditional manual runtime tool loop using explicit
  `start_session`.
- `guided`: the recommended coding task loop using `start_coding_task` and
  `finish_coding_task`.

Both flows pass explicit `session_id` values to subsequent tools and do not rely
on current-session binding.

## Scope

The harness is intentionally small:

- deterministic scripted tool calls only,
- local loopback WebCodex server and agent,
- disposable temporary Rust project,
- no external network dependency,
- bounded runtime with `EVAL_TIMEOUT_SECS`,
- no production repository mutation,
- final machine-readable JSON summary as the last stdout line.

Python is used only inside the script layer for JSON parsing and request
construction. Runtime paths do not depend on Python helpers.

## Flows

### `baseline`

Baseline is not a bad path. It represents a conventional manual runtime tool
loop:

1. `start_session`
2. `search_project_text` / `read_file` / `show_changes`
3. `replace_line_range` when a case edits source
4. `cargo_check` when a case validates source
5. manual closeout with `workspace_hygiene_check` and
   `session_handoff_summary`

Baseline never calls `start_coding_task` or `finish_coding_task`.

### `guided`

Guided keeps the Stage 3.0 coding-task flow:

1. `start_coding_task`
2. `search_project_text` / `read_file` / `show_changes`
3. `replace_line_range` when a case edits source
4. `cargo_check` when a case validates source
5. `finish_coding_task`

`start_coding_task` is called with `bind_current=false`; subsequent tools use
the returned explicit `session_id`.

## Cases

Each selected flow runs the same three cases. In `EVAL_MODE=compare`, the order
is interleaved:

1. `baseline.inspect_only`
2. `guided.inspect_only`
3. `baseline.small_structured_line_edit`
4. `guided.small_structured_line_edit`
5. `baseline.failed_call_recovery`
6. `guided.failed_call_recovery`

Every case starts by cleaning the disposable repository and ends by cleaning it
again so baseline and guided cases do not pollute each other.

### `inspect_only`

Exercises structured inspection with `search_project_text`, `read_file`, and
`show_changes`. Assertions include session id creation, structured
`search_project_text` fields (`backend`, `matches`, and `truncated`),
successful closeout handoff, clean disposable worktree cleanup, and zero
scripted raw shell runtime calls.

### `small_structured_line_edit`

Exercises a small source edit using line-number metadata. Assertions include
structured line-edit usage, no raw shell runtime editing, `show_changes`
reporting `src/lib.rs`, successful `cargo_check`, successful closeout handoff,
and clean worktree cleanup. Both baseline and guided assert that the closeout
validation summary reports at least one validation-like event after
`cargo_check`; guided also asserts that `finish_coding_task` reports the changed
file.

### `failed_call_recovery`

Exercises recovery after a controlled failed tool call. Assertions include a
controlled failed `replace_line_range`, unmodified workspace after the failed
edit, successful recovery edit, failed tool metadata in the closeout handoff,
and clean worktree cleanup. This case does not require validation summary
availability because it does not run `cargo_check`.

## Validation Summary

Validation metrics are ledger-derived. Baseline reads validation from
`session_handoff_summary.validation`; guided reads validation from
`finish_coding_task.validation`. Both sections use the same conservative
session-ledger summary for validation-like tool-call outcomes such as
`cargo_fmt`, `cargo_check`, `cargo_test`, `validate_patch`, and
`apply_patch_checked`.

The summary is observability data only. It records factual tool metadata such as
tool name, validation kind, success, exit code when available, timestamps when
available, and safe bounded input summaries. It does not include stdout/stderr
bodies, extract root causes, or provide semantic diagnosis.

The session ledger stores only a small, sanitized, bounded
`validation_output_summary` for cargo validation tools (`cargo_fmt`,
`cargo_check`, and `cargo_test`). The field is derived only from already-bounded
`stdout_tail`/`stderr_tail` tool output, caps each excerpt at 800 characters, and
filters suspicious token, secret, password, API key, authorization, bearer,
private key, and access key lines before persistence. Reloaded persisted ledgers
are sanitized again. Finish and handoff validation summaries never expose these
excerpts, `validation_output_summary`, or raw stdout/stderr fields.

The minimal bounded-tail parser reads that safe metadata when present. It
extracts only stable cargo facts: rustc severity/code/span for `cargo_fmt` and
`cargo_check`, and test summary counts plus the first stable failed test name
for `cargo_test`. It does not infer root causes, summarize compiler output,
offer fixes, use LSP, or use tree-sitter. `parser.available=true` means the
parser had safe bounded metadata to inspect. `diagnostics.available=true` means
stable facts were actually extracted. Successful `cargo_check` runs may set
`parser.available=true` while `diagnostics.available=false` because no stable
diagnostic facts are present. Session-ledger validation summaries report
`parser.available=false` only when no validation event contains safe bounded
output metadata.

Inspect-only cases may still report `validation.available=false` when no
validation-like tool calls exist in the session ledger.
Validation availability should match between baseline and guided when both
flows run the same validation-like tools.

## Metrics

The final stdout line is a JSON object with this top-level shape:

```json
{
  "mode": "compare",
  "skipped": false,
  "cases_total": 6,
  "cases_passed": 6,
  "cases_failed": 0,
  "baseline": {},
  "guided": {},
  "comparison": {}
}
```

`baseline` and `guided` flow summaries include:

```json
{
  "mode": "baseline",
  "skipped": false,
  "cases_total": 3,
  "cases_passed": 3,
  "cases_failed": 0,
  "tool_calls_total": 0,
  "raw_shell_calls": 0,
  "raw_shell_ratio": 0.0,
  "structured_edit_calls": 0,
  "failed_tool_calls": 0,
  "recovered_failed_tool_calls": 0,
  "workspace_clean_after_each_case": true,
  "handoff_available_rate": 1.0,
  "validation_available_rate": 0.3333333333,
  "validation_parser_available_rate": 0.3333333333,
  "validation_diagnostics_available_rate": 0.0,
  "validation_events_total": 1,
  "validation_successes": 1,
  "validation_failures": 0,
  "validation_diagnostics_total": 0,
  "finish_coding_task_success_rate": null,
  "cases": []
}
```

For guided summaries, `finish_coding_task_success_rate` is the successful
`finish_coding_task` calls divided by attempted `finish_coding_task` calls.
For baseline summaries, it is always `null`.

Each case summary includes `mode`, `case`, `passed`, `tool_calls`,
`raw_shell_calls`, `structured_edit_calls`, `failed_tool_calls`,
`recovered_failed_tool_calls`, `handoff_available`, `workspace_clean`,
`finish_coding_task_calls`, `finish_coding_task_successes`,
`validation_available`, `validation_events_total`, `validation_successes`,
`validation_failures`, `validation_parser_available`,
`validation_diagnostics_available`, `validation_diagnostics_total`, and
`warnings`.

`comparison` is present only in `EVAL_MODE=compare`:

```json
{
  "guided_minus_baseline_tool_calls": 0,
  "guided_minus_baseline_raw_shell_ratio": 0.0,
  "guided_minus_baseline_structured_edit_calls": 0,
  "guided_handoff_available_delta": 0.0,
  "guided_minus_baseline_validation_available_rate": 0.0,
  "guided_minus_baseline_validation_parser_available_rate": 0.0,
  "guided_minus_baseline_validation_diagnostics_available_rate": 0.0,
  "guided_minus_baseline_validation_events_total": 0,
  "guided_cleanup_delta": 0.0
}
```

The comparison does not require guided to use fewer tool calls. Guided may use
more calls because it includes `start_coding_task` and `finish_coding_task`
aggregators. The useful signal is whether guided keeps raw shell use,
structured edits, handoff availability, cleanup, and failed-call recovery at
least as stable as baseline.

## Running

Syntax-only / dry-run mode:

```bash
EVAL_SKIP_RUN=1 bash scripts/eval_coding_loop.sh
```

Run only guided cases:

```bash
EVAL_MODE=guided bash scripts/eval_coding_loop.sh
```

Run only baseline cases:

```bash
EVAL_MODE=baseline bash scripts/eval_coding_loop.sh
```

Run baseline and guided comparison, which is the default:

```bash
EVAL_MODE=compare bash scripts/eval_coding_loop.sh
bash scripts/eval_coding_loop.sh
```

Useful environment overrides:

```bash
EVAL_MODE=guided|baseline|compare
EVAL_PORT=18180
EVAL_TIMEOUT_SECS=240
EVAL_TRANSPORT=websocket
EVAL_KEEP_TMP=1
CARGO_BIN=cargo
```

On success, the temporary project is removed. On failure, the script leaves the
temporary root in place and prints server/agent log paths before the final JSON
summary.

## Limitations

- Eval remains scripted tool-call metrics, not full model behavior.
- Guided may use more tool calls because it includes start/finish aggregators.
- Minimal parser only extracts stable cargo fmt/check/test facts from bounded
  tails or safe metadata when available.
- Parser output is not semantic diagnosis.
- No root-cause extraction.
- No fix suggestions.
- No LSP or tree-sitter analysis.
- There is no semantic code understanding signal yet.
- The harness does not exercise the Codex CLI or any LLM delegation.
