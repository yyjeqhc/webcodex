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
            "Advanced/raw unified-diff apply. Prefer apply_patch_checked for new workflows; this lower-level path does not provide the full checked preflight and diff_summary package.".to_string(),
            apply_patch_input_schema(),
        ),
        tool_spec(
            "apply_patch_checked",
            "Canonical checked patch tool for complex multi-file unified diffs. Runs patch preflight first and applies only when validation passes. Prefer over raw apply_patch; for ordinary local edits prefer apply_text_edits.",
            apply_patch_checked_input_schema(),
        ),
        tool_spec(
            "validate_patch",
            "Dry-run a unified diff with git apply --check/--stat through the owning agent; never writes files.",
            validate_patch_input_schema(),
        ),
        tool_spec(
            "replace_in_file",
            "Compatibility tool. Prefer apply_text_edits for new workflows. Short exact replacements; fails without writing when old text is missing or ambiguous.",
            replace_in_file_input_schema(),
        ),
        tool_spec(
            "replace_exact_block",
            "Compatibility tool. Prefer apply_text_edits for new workflows. Replace literal UTF-8 text that matches exactly once; no regex or auto-format.",
            replace_exact_block_input_schema(),
        ),
        tool_spec(
            "insert_before_pattern",
            "Compatibility tool. Prefer apply_text_edits for new workflows. Insert UTF-8 text before one literal pattern match; no regex, AST, auto-newline, or auto-format.",
            insert_before_pattern_input_schema(),
        ),
        tool_spec(
            "insert_after_pattern",
            "Compatibility tool. Prefer apply_text_edits for new workflows. Insert UTF-8 text after one literal pattern match; no regex, AST, auto-newline, or auto-format.",
            insert_after_pattern_input_schema(),
        ),
        tool_spec(
            "write_project_file",
            "Create new files or intentional whole-file rewrites. Not preferred for ordinary local edits—prefer apply_text_edits. Inspect current content and worktree changes before overwriting; do not silently clobber user edits.",
            write_project_file_input_schema(),
        ),
        tool_spec(
            "replace_line_range",
            "Compatibility tool. Prefer apply_text_edits for new workflows. Replaces a 1-based inclusive line range with sha256/prefix guards when available.",
            replace_line_range_input_schema(),
        ),
        tool_spec(
            "insert_at_line",
            "Compatibility tool. Prefer apply_text_edits for new workflows. Inserts before a 1-based line with sha256/prefix guards when available.",
            insert_at_line_input_schema(),
        ),
        tool_spec(
            "delete_line_range",
            "Compatibility tool. Prefer apply_text_edits for new workflows. Deletes a 1-based inclusive line range with sha256/prefix guards when available.",
            delete_line_range_input_schema(),
        ),
        tool_spec(
            "apply_text_edits",
            "Canonical precise edit tool for one file. Preferred for ordinary local changes: ordered exact replace/insert/delete against current worktree content (not HEAD). Use hash/prefix/anchor guards when available. Prefer over whole-file write and compatibility edit tools. Supports dry_run.",
            apply_text_edits_input_schema(),
        ),
    ]
}
