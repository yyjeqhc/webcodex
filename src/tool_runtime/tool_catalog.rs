//! Model-facing runtime tool discovery groups and recommended flows.

use super::tool_definition::{ToolDiscoveryGroup, ToolRecommendedFlow};

pub(crate) const TOOL_DISCOVERY_GROUP_CHECKPOINT: &str = "checkpoint";
pub(crate) const TOOL_DISCOVERY_GROUP_CLEANUP: &str = "cleanup";
pub(crate) const TOOL_DISCOVERY_GROUP_EDIT: &str = "edit";
pub(crate) const TOOL_DISCOVERY_GROUP_GIT: &str = "git";
pub(crate) const TOOL_DISCOVERY_GROUP_INSPECT: &str = "inspect";
pub(crate) const TOOL_DISCOVERY_GROUP_JOBS: &str = "jobs";
pub(crate) const TOOL_DISCOVERY_GROUP_PATCH: &str = "patch";
pub(crate) const TOOL_DISCOVERY_GROUP_PROJECTS: &str = "projects";
pub(crate) const TOOL_DISCOVERY_GROUP_REVIEW: &str = "review";
pub(crate) const TOOL_DISCOVERY_GROUP_RUNTIME: &str = "runtime";
pub(crate) const TOOL_DISCOVERY_GROUP_SHELL: &str = "shell";
pub(crate) const TOOL_DISCOVERY_GROUP_VALIDATION: &str = "validation";

pub(crate) const TOOL_DISCOVERY_GROUPS: &[ToolDiscoveryGroup] = &[
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_INSPECT,
        tools: &[
            "list_tools",
            "list_projects",
            "list_agents",
            "runtime_status",
            "start_coding_task",
            "read_file",
            "search_project_text",
            "show_changes",
            "list_project_files",
            "git_status",
            "git_diff",
            "git_diff_summary",
            "git_diff_hunks",
            "git_log",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_PROJECTS,
        tools: &["list_projects", "register_project", "create_project"],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_GIT,
        tools: &[
            "git_status",
            "git_diff",
            "git_diff_summary",
            "git_diff_hunks",
            "git_log",
            "show_changes",
            "git_restore_paths",
            "discard_untracked",
            "workspace_checkpoint_create",
            "workspace_checkpoint_restore",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_REVIEW,
        tools: &[
            "finish_coding_task",
            "show_changes",
            "git_diff_hunks",
            "workspace_hygiene_check",
            "git_diff_summary",
            "git_log",
            "git_status",
            "git_diff",
            "workspace_checkpoint_show",
            "workspace_checkpoint_list",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_VALIDATION,
        tools: &[
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "validate_patch",
            "apply_patch_checked",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_PATCH,
        tools: &["apply_patch", "apply_patch_checked", "validate_patch"],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_EDIT,
        tools: &[
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "apply_patch_checked",
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
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_SHELL,
        tools: &[
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "run_shell",
            "run_job",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_JOBS,
        tools: &[
            "run_job",
            "stop_job",
            "job_status",
            "job_log",
            "list_jobs",
            "job_tail",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_RUNTIME,
        tools: &[
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
            "bind_current_session",
            "current_session",
            "unbind_current_session",
            "workspace_checkpoint_create",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
            "list_projects",
            "list_agents",
            "runtime_status",
            "tool_manifest",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_CLEANUP,
        tools: &[
            "delete_project_files",
            "git_restore_paths",
            "discard_untracked",
            "workspace_checkpoint_delete",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_CHECKPOINT,
        tools: &[
            "workspace_checkpoint_create",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
            "workspace_checkpoint_restore",
            "workspace_checkpoint_delete",
        ],
    },
];

pub(crate) const TOOL_RECOMMENDED_FLOWS: &[ToolRecommendedFlow] = &[
    ToolRecommendedFlow {
        name: "discovery",
        summary:
            "Discovery: resolve project with list_projects/runtime_status, then load rules/context with read_file before editing.",
        manifest_purpose: "Resolve the project and load rules/context before editing.",
        tools: &[
            "start_coding_task",
            "list_projects",
            "runtime_status",
            "read_file",
        ],
    },
    ToolRecommendedFlow {
        name: "inspect",
        summary: "Inspect: use read_file, search_project_text, and show_changes before editing.",
        manifest_purpose: "Use the default inspect tools before editing.",
        tools: &["read_file", "search_project_text", "show_changes"],
    },
    ToolRecommendedFlow {
        name: "edit",
        summary:
            "Edit: prefer replace_line_range / insert_at_line / delete_line_range for local line edits; use apply_text_edits for batches; use apply_patch_checked for broad diffs.",
        manifest_purpose:
            "Prefer structured line edits, batch text edits, or checked patches for source changes.",
        tools: &[
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
            "apply_text_edits",
            "apply_patch_checked",
        ],
    },
    ToolRecommendedFlow {
        name: "validate",
        summary:
            "Validate: use cargo_check / cargo_test / validate_patch when applicable. raw run_shell is a bounded escape hatch, not the primary editing or validation path.",
        manifest_purpose:
            "Use structured validation; run_shell is a bounded diagnostics escape hatch, not the primary validation path.",
        tools: &["cargo_check", "cargo_test", "validate_patch", "run_shell"],
    },
    ToolRecommendedFlow {
        name: "review",
        summary: "Review: use show_changes / git_diff_hunks / workspace_hygiene_check before final response.",
        manifest_purpose: "Review diffs and workspace hygiene before the final response.",
        tools: &["show_changes", "git_diff_hunks", "workspace_hygiene_check"],
    },
    ToolRecommendedFlow {
        name: "handoff",
        summary: "Handoff: use session_summary / session_handoff_summary when a task spans multiple steps.",
        manifest_purpose: "Summarize or hand off multi-step session state.",
        tools: &[
            "finish_coding_task",
            "session_summary",
            "session_handoff_summary",
        ],
    },
];
