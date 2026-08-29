use super::super::input_schemas::{
    apply_text_edits_input_schema, apply_unified_diff_input_schema, write_project_file_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "apply_unified_diff",
            "Canonical complex/multi-file raw unified-diff mutation. Prefer apply_text_edits for ordinary guarded local edits. This tool performs its own bounded preflight, applies only after it passes, and never needs a separate validation call. Input must be a standard unified diff; shell heredocs and Codex *** Begin Patch wrappers are rejected with recovery metadata.",
            apply_unified_diff_input_schema(),
        ),
        tool_spec(
            "write_project_file",
            "Create new files or intentional whole-file rewrites. Not preferred for ordinary local edits—prefer apply_text_edits. Inspect current content and worktree changes before overwriting; do not silently clobber user edits.",
            write_project_file_input_schema(),
        ),
        tool_spec(
            "apply_text_edits",
            "Canonical transactional file-change preferred for ordinary local edit/create/delete/rename on current worktree, not HEAD. Whole batch uses per-file hashes, dry_run; prefer over whole-file. Unique exact by default; bounded conflicts may advertise 1-based occurrence. SHA conflict requires reread.",
            apply_text_edits_input_schema(),
        ),
    ]
}
