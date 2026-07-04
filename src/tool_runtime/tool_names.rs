//! Runtime tool name compatibility catalog shared by parsers, metadata checks,
//! and surfaces.

use super::tool_definition::{lookup_tool_definition, TOOL_DEFINITIONS};

/// Compatibility snapshot of runtime tool names accepted by
/// `ToolCall::from_tool_name`. `ToolDefinition` is the runtime source for
/// known-tool checks; schema tests keep this snapshot synchronized for callers
/// that still consume it directly.
pub const KNOWN_TOOL_NAMES: &[&str] = &[
    "list_tools",
    "start_session",
    "start_coding_task",
    "finish_coding_task",
    "session_summary",
    "post_session_message",
    "list_session_messages",
    "resolve_session_message",
    "session_discussion_summary",
    "session_handoff_summary",
    "workspace_hygiene_check",
    "bind_current_session",
    "current_session",
    "unbind_current_session",
    "workspace_checkpoint_create",
    "workspace_checkpoint_list",
    "workspace_checkpoint_show",
    "workspace_checkpoint_restore",
    "workspace_checkpoint_delete",
    "list_projects",
    "register_project",
    "create_project",
    "list_agents",
    "runtime_status",
    "tool_manifest",
    "run_shell",
    "run_job",
    "stop_job",
    "run_codex",
    "job_status",
    "job_log",
    "list_project_files",
    "search_project_text",
    "git_diff_summary",
    "show_changes",
    "list_jobs",
    "job_tail",
    "read_file",
    "git_status",
    "git_diff",
    "git_diff_hunks",
    "git_log",
    "cargo_fmt",
    "cargo_check",
    "cargo_test",
    "apply_patch",
    "apply_patch_checked",
    "delete_project_files",
    "git_restore_paths",
    "discard_untracked",
    "validate_patch",
    "replace_in_file",
    "replace_exact_block",
    "insert_before_pattern",
    "insert_after_pattern",
    "write_project_file",
    "save_project_artifact",
    "read_project_artifact_metadata",
    "read_project_artifact",
    "artifact_upload_begin",
    "artifact_upload_chunk",
    "artifact_upload_finish",
    "artifact_upload_abort",
    "replace_line_range",
    "insert_at_line",
    "delete_line_range",
    "apply_text_edits",
];

/// Compatibility snapshot of runtime tools that remain implemented/parser-
/// recognized but are deliberately hidden from model-facing discovery surfaces.
/// ToolDefinition visibility is the runtime source for hidden checks.
#[allow(dead_code)]
pub const MODEL_HIDDEN_TOOL_NAMES: &[&str] = &["run_codex"];

/// Returns `true` if `name` is a recognized runtime tool name. Public so the
/// HTTP/MCP adapters can decide whether to emit the rich "unknown tool" error.
pub fn is_known_tool_name(name: &str) -> bool {
    let known = lookup_tool_definition(name).is_some();
    debug_assert_eq!(
        known,
        KNOWN_TOOL_NAMES
            .iter()
            .any(|known_name| *known_name == name),
        "{name} known-tool compatibility list drifted from ToolDefinition"
    );
    known
}

#[allow(dead_code)]
pub(crate) fn is_model_hidden_tool_name(name: &str) -> bool {
    let hidden = lookup_tool_definition(name)
        .is_some_and(|definition| definition.visibility.is_model_hidden());
    debug_assert_eq!(
        hidden,
        MODEL_HIDDEN_TOOL_NAMES
            .iter()
            .any(|hidden_name| *hidden_name == name),
        "{name} hidden-tool compatibility list drifted from ToolDefinition visibility"
    );
    hidden
}

pub(super) fn model_visible_tool_names_csv() -> String {
    TOOL_DEFINITIONS
        .iter()
        .filter(|definition| definition.visibility.is_model_visible())
        .map(|definition| definition.name)
        .collect::<Vec<_>>()
        .join(", ")
}
