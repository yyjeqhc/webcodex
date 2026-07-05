use super::super::input_schemas::{
    apply_patch_checked_input_schema, apply_patch_input_schema, apply_text_edits_input_schema,
    delete_line_range_input_schema, insert_after_pattern_input_schema, insert_at_line_input_schema,
    insert_before_pattern_input_schema, replace_exact_block_input_schema,
    replace_in_file_input_schema, replace_line_range_input_schema, validate_patch_input_schema,
    write_project_file_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "apply_patch",
            "Apply a unified diff patch to an agent-registered project.".to_string(),
            apply_patch_input_schema(),
        ),
        tool_spec(
            "apply_patch_checked",
            "Validated unified-diff edit tool for broad or multi-file patches. Returns a diff summary; for local line edits prefer replace_line_range, insert_at_line, delete_line_range, or apply_text_edits.",
            apply_patch_checked_input_schema(),
        ),
        tool_spec(
            "validate_patch",
            "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.",
            validate_patch_input_schema(),
        ),
        tool_spec(
            "replace_in_file",
            "Literal pattern compatibility path for short exact replacements. Prefer replace_line_range, insert_at_line, or delete_line_range when line numbers are available; fails without writing when old text is missing or ambiguous.",
            replace_in_file_input_schema(),
        ),
        tool_spec(
            "replace_exact_block",
            "Replace literal UTF-8 text that matches exactly once; no regex or auto-format. Use line edit tools when line numbers are known.",
            replace_exact_block_input_schema(),
        ),
        tool_spec(
            "insert_before_pattern",
            "Insert UTF-8 text before one literal pattern match; no regex, AST, auto-newline, or auto-format.",
            insert_before_pattern_input_schema(),
        ),
        tool_spec(
            "insert_after_pattern",
            "Insert UTF-8 text after one literal pattern match; no regex, AST, auto-newline, or auto-format.",
            insert_after_pattern_input_schema(),
        ),
        tool_spec(
            "write_project_file",
            "Whole-file write compatibility path for new files or deliberate small overwrites. Prefer structured line edits or apply_text_edits for source changes; requires overwrite/guards for existing files.",
            write_project_file_input_schema(),
        ),
        tool_spec(
            "replace_line_range",
            "Preferred source-code edit tool for local line changes with clear line numbers. Replaces a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
            replace_line_range_input_schema(),
        ),
        tool_spec(
            "insert_at_line",
            "Preferred source-code edit tool for local line changes with clear line numbers. Inserts before a specified 1-based line; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
            insert_at_line_input_schema(),
        ),
        tool_spec(
            "delete_line_range",
            "Preferred source-code edit tool for local line changes with clear line numbers. Deletes a 1-based inclusive range; better than write_project_file or run_shell for source edits. Supports sha256/prefix guards.",
            delete_line_range_input_schema(),
        ),
        tool_spec(
            "apply_text_edits",
            "Preferred batch text edit tool for coordinated source changes in one UTF-8 file. Applies bounded exact replace/insert/delete edits atomically only when all matches validate as unique/non-overlapping. Supports dry_run and sha256 guard.",
            apply_text_edits_input_schema(),
        ),
    ]
}
