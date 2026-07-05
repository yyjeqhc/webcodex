use super::super::input_schemas::{
    git_diff_hunks_input_schema, git_diff_input_schema, git_diff_summary_input_schema,
    git_log_input_schema, git_status_input_schema, show_changes_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "git_diff_summary",
            "Read-only git diff summary for a project: `git status --porcelain`, "
                .to_string()
                + "`git diff --stat`, and a parsed changed-file list. Does not modify the "
                + "worktree.",
            git_diff_summary_input_schema(),
        ),
        tool_spec(
            "show_changes",
            "Default inspect/review tool before final response. Read-only worktree plus optional session summary; reports status, warnings, next actions, and bounded hunks without modifying files.",
            show_changes_input_schema(),
        ),
        tool_spec(
            "git_status",
            "Run git status --porcelain for a project.",
            git_status_input_schema(),
        ),
        tool_spec(
            "git_diff",
            "Run git diff for a project, optionally scoped to paths.",
            git_diff_input_schema(),
        ),
        tool_spec(
            "git_diff_hunks",
            "Return bounded structured git diff hunks for review. Supports optional paths and cached diff; does not modify the worktree.",
            git_diff_hunks_input_schema(),
        ),
        tool_spec(
            "git_log",
            "Return bounded structured recent git commit history for a project. Does not return commit bodies or modify the worktree.",
            git_log_input_schema(),
        ),
    ]
}
