# Coding Loop Eval

This document describes the minimal WebCodex coding-loop eval harness in
`scripts/eval_coding_loop.sh`.

## Purpose

The harness measures WebCodex runtime and tool-loop mechanics for remote coding
workflows. It does not evaluate LLM intelligence, prompt quality, Codex CLI
behavior, tree-sitter, LSP, or diagnostic parsing.

The current target is a deterministic, low-noise, local signal for whether the
recommended coding loop is mechanically stable:

- start a coding task with an explicit session id,
- inspect with structured project tools,
- edit with structured line-edit tools,
- validate through dedicated validation tools when applicable,
- finish the coding task with a deterministic handoff summary,
- leave the disposable workspace clean after every case.

## Scope

The harness is intentionally small:

- deterministic scripted tool calls only,
- local loopback WebCodex server and agent,
- disposable temporary Rust project,
- no external network dependency,
- bounded runtime with `EVAL_TIMEOUT_SECS`,
- no production repository mutation,
- final machine-readable JSON summary.

Python is used only inside the script layer for JSON parsing and request
construction. Runtime paths do not depend on Python helpers.

## Cases

### `inspect_only`

Exercises the recommended inspect loop:

1. `start_coding_task`
2. `search_project_text`
3. `read_file`
4. `show_changes`
5. `finish_coding_task`

Assertions include session id creation, structured
`search_project_text` fields (`backend`, `matches`, and `truncated`),
successful `show_changes`, successful `finish_coding_task`, clean disposable
worktree cleanup, and zero scripted raw shell runtime calls.

### `small_structured_line_edit`

Exercises a small source edit using line-number metadata:

1. `start_coding_task`
2. `read_file` with line numbers
3. `replace_line_range`
4. `show_changes`
5. `cargo_check`
6. `finish_coding_task`
7. local cleanup of the disposable project

Assertions include structured line-edit usage, no `write_project_file`, no raw
shell runtime editing, `show_changes`/`finish_coding_task` reporting
`src/lib.rs`, successful `cargo_check`, and clean worktree cleanup.

### `failed_call_recovery`

Exercises recovery after a controlled failed tool call:

1. `start_coding_task`
2. `replace_line_range` with a deliberately wrong prefix guard
3. `read_file` to confirm the failed edit did not corrupt the file
4. corrected `replace_line_range`
5. `show_changes`
6. `finish_coding_task`
7. local cleanup of the disposable project

Assertions include a controlled failed call, unmodified workspace after the
failed call, successful recovery edit, failed tool metadata in the
`finish_coding_task` handoff, and clean worktree cleanup.

## Metrics

The final stdout line is a JSON object. Top-level fields include:

- `cases_total`, `cases_passed`, `cases_failed`
- `tool_calls_total`
- `raw_shell_calls`
- `raw_shell_ratio`
- `structured_edit_calls`
- `failed_tool_calls`
- `recovered_failed_tool_calls`
- `workspace_clean_after_each_case`
- `finish_coding_task_success_rate`
- `cases`

Each case summary includes:

- `case`
- `passed`
- `tool_calls`
- `raw_shell_calls`
- `structured_edit_calls`
- `failed_tool_calls`
- `recovered_failed_tool_calls`
- `workspace_clean`
- `finish_coding_task_calls`
- `finish_coding_task_successes`
- `warnings`

The first implementation counts scripted runtime tool calls from the harness.
It does not yet parse the full session ledger for hidden model behavior.

## Running

Syntax-only / dry-run mode:

```bash
EVAL_SKIP_RUN=1 bash scripts/eval_coding_loop.sh
```

Full local run:

```bash
bash scripts/eval_coding_loop.sh
```

Useful environment overrides:

```bash
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

- Metrics initially count scripted tool calls, not full model behavior.
- Validation summaries are not parsed yet.
- There is no baseline/guided comparison yet.
- There is no LSP or tree-sitter signal yet.
- The harness does not parse cargo/test stderr beyond runtime tool success.
- The harness does not exercise the Codex CLI or any LLM delegation.
