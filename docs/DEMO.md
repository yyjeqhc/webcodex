# Demo: A Safe Coding Task Through WebCodex

[English](DEMO.md) | [简体中文](DEMO.zh-CN.md)

This demo shows the intended WebCodex workflow. It is a product walkthrough, not a script you must copy exactly.

## Prompt

```text
Use WebCodex on project agent:local-dev:<project_id>.
Inspect this repo, find a small failing or stale documentation issue, fix it,
run appropriate validation, show changes, run workspace hygiene, and finish.
Prefer structured edit tools. Do not touch secrets. Do not use run_shell unless
the structured validation tools are not enough.
```

For the first run, make the task read-only:

```text
Use WebCodex on project agent:local-dev:<project_id>.
Inspect README.md and docs/QUICK_START.md, summarize how setup works, show
changes without a diff, run workspace hygiene, and finish. Do not edit files.
```

## Expected Tool Flow

1. `start_coding_task`
2. `list_project_files`, `search_project_text`, or `read_file`
3. `replace_line_range`, `insert_at_line`, `delete_line_range`, `apply_text_edits`, or `apply_patch_checked`
4. `validate_patch`, `cargo_fmt`, `cargo_check`, or `cargo_test`
5. `show_changes`
6. `git_diff_hunks` when targeted diff inspection is needed
7. `workspace_hygiene_check`
8. `finish_coding_task`
9. `session_handoff_summary` when another operator or client needs to continue

`run_shell` is available as a bounded escape hatch. It should not be the first choice for editing or validation.

## What The User Checks

- Changed files and whether they match the requested scope.
- Validation result and whether it is appropriate for the change type.
- `show_changes` output, including warnings and truncation flags.
- `workspace_hygiene_check` findings.
- Final `finish_coding_task` task/evidence outcomes.
- Any handoff notes, open risks, or rollback instructions.

## Safety Notes

- Do not expose secrets in prompts, tool output, docs, or examples.
- Keep the project id explicit: `agent:<client_id>:<project_id>`.
- Prefer structured edits over shell-based writes.
- Prefer structured validation over broad shell commands.
- The final workspace should be clean, or intentionally dirty for human inspection.
- If the task was only a smoke test, revert the small edit with normal Git review practices.

## Next

- First setup: [QUICK_START.md](QUICK_START.md)
- Concepts: [CONCEPTS.md](CONCEPTS.md)
- Security: [../SECURITY.md](../SECURITY.md)
