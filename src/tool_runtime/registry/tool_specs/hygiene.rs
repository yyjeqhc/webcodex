use super::super::input_schemas::{
    delete_project_files_input_schema, discard_untracked_input_schema,
    git_restore_paths_input_schema, workspace_hygiene_check_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "workspace_hygiene_check",
            "Default pre-final workspace hygiene review; read-only. Detects dirty worktree, untracked temp/smoke files, cache dirs, secret-like names, and large untracked files before validation or handoff. Never reads file contents.",
            workspace_hygiene_check_input_schema(),
        ),
        tool_spec(
            "delete_project_files",
            "Delete selected project-relative files only; safer than arbitrary rm for cleanup.",
            delete_project_files_input_schema(),
        ),
        tool_spec(
            "git_restore_paths",
            "Restore selected tracked paths with git restore; does not remove untracked files.",
            git_restore_paths_input_schema(),
        ),
        tool_spec(
            "discard_untracked",
            "Discard selected untracked files with git clean -f -- <paths>.",
            discard_untracked_input_schema(),
        ),
    ]
}
