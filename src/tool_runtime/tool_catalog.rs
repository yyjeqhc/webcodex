//! Model-facing runtime tool discovery groups, recommended flows, and intents.

use super::tool_definition::{ToolDiscoveryGroup, ToolManifestIntent, ToolRecommendedFlow};

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
            "work_on_project",
            "start_coding_task",
            "project_overview",
            "list_project_tracked_files",
            "read_file",
            "read_files",
            "run_process",
            "run_script",
            "run_shell",
            "search_project_text",
            "search_project_texts",
            "document_symbols",
            "document_diagnostics",
            "hover",
            "workspace_symbols",
            "goto_definition",
            "find_references",
            "call_hierarchy",
            "lsp_status",
            "list_project_files",
            "show_changes",
            "git_status",
            "git_diff",
            "git_diff_summary",
            "git_diff_hunks",
            "git_log",
            "workspace_checkpoint_list",
            "workspace_checkpoint_show",
            "computer_list_targets",
            "computer_list_windows",
            "computer_list_displays",
            "computer_list_applications",
            "computer_launch_application",
            "computer_accessibility_status",
            "computer_accessibility_tree",
            "computer_find_elements",
            "computer_element_state",
            "computer_activate_window",
            "computer_control",
            "computer_scroll_to_element",
            "computer_key_input",
            "computer_input_text",
            "computer_snapshot",
            "computer_snapshot_display",
            "computer_save_snapshot",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_PROJECTS,
        tools: &[
            "list_projects",
            "register_project",
            "unregister_project",
            "create_project",
        ],
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
            "go_test",
            "validation_summary",
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
            "apply_text_edits",
            "apply_patch_checked",
            "write_project_file",
            "save_project_artifact",
            "read_project_artifact_metadata",
            "read_project_artifact",
            "import_conversation_files_to_project",
            "export_project_artifact",
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
            "run_process",
            "run_script",
            "run_shell",
            "open_session_shell",
            "session_shell_exec",
            "session_shell_status",
            "close_session_shell",
            "run_job",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_JOBS,
        tools: &[
            "open_session_shell",
            "session_shell_exec",
            "session_shell_status",
            "close_session_shell",
            "run_job",
            "stop_job",
            "job_status",
            "job_log",
            "observe_jobs",
            "list_jobs",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_RUNTIME,
        tools: &[
            "list_tools",
            "work_on_project",
            "start_coding_task",
            "finish_coding_task",
            "session_summary",
            "update_session_context",
            "close_session",
            "post_session_message",
            "list_session_messages",
            "resolve_session_message",
            "session_discussion_summary",
            "session_handoff_summary",
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
        summary: "Discovery: use search_project_text for bounded code search after list_projects/project_overview. Prefer run_process for native argv and run_script for typed scripts; run_shell with rg or git grep remains the diagnostic escape hatch.",
        manifest_purpose:
            "Resolve the project, inspect bounded structure, then search code with search_project_text or search_project_texts.",
        tools: &[
            "list_projects",
            "project_overview",
            "read_file",
            "read_files",
            "search_project_text",
            "search_project_texts",
            "run_process",
            "run_script",
            "run_shell",
        ],
    },
    ToolRecommendedFlow {
        name: "inspect",
        summary: "Inspect: use search_project_text and read_file before editing. Prefer run_process for native argv and run_script for typed scripts; run_shell with rg or git grep is the diagnostic escape hatch; show_changes reviews.",
        manifest_purpose:
            "Use bounded structured search and file reads for code inspection, then review the worktree.",
        tools: &[
            "search_project_text",
            "search_project_texts",
            "read_file",
            "read_files",
            "run_process",
            "run_script",
            "run_shell",
            "show_changes",
        ],
    },
    ToolRecommendedFlow {
        name: "edit",
        summary:
            "Edit: prefer apply_text_edits for transactional guarded file changes; apply_patch_checked for complex unified diffs; write_project_file only for intentional full rewrites.",
        manifest_purpose:
            "Prefer guarded transactional changes and checked complex diffs for source changes; whole-file write only for an intentional full rewrite.",
        tools: &[
            "apply_text_edits",
            "apply_patch_checked",
            "write_project_file",
        ],
    },
    ToolRecommendedFlow {
        name: "validate",
        summary:
            "Validate: use cargo_check / cargo_test / go_test / validate_patch; long validation continues as a Job. Prefer structured validation tools when available; raw run_shell is a bounded escape hatch, not the primary validation path.",
        manifest_purpose:
            "Use structured Rust or Go validation; long checks become Jobs. Prefer structured validation tools when available; run_shell remains an explicit escape hatch, not the primary validation path.",
        tools: &[
            "cargo_check",
            "cargo_test",
            "go_test",
            "observe_jobs",
            "job_status",
            "validation_summary",
            "validate_patch",
            "run_process",
            "run_script",
            "run_shell",
        ],
    },
    ToolRecommendedFlow {
        name: "computer_observe",
        summary: "Computer observe: discover a caller-visible capable Runner, list its exact windows, then inspect accessibility or capture one exact surface. Read-only; no control actions.",
        manifest_purpose:
            "Discover a Computer-capable Runner, list its windows, then inspect accessibility or capture one exact surface.",
        tools: &[
            "computer_list_targets",
            "computer_list_windows",
            "computer_accessibility_status",
            "computer_accessibility_tree",
            "computer_find_elements",
            "computer_element_state",
            "computer_snapshot",
        ],
    },
    ToolRecommendedFlow {
        name: "computer_application_launch",
        summary: "Computer application launch: discover fresh bounded opaque application IDs, launch exactly one ID, then re-list windows and activate an exact surface only if needed.",
        manifest_purpose:
            "Discover a Windows application, submit its exact native launch request, then re-observe windows before any follow-up UI effect.",
        tools: &[
            "computer_list_applications",
            "computer_launch_application",
            "computer_list_windows",
            "computer_activate_window",
        ],
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
            "validation_summary",
        ],
    },
];

/// Single ordered, unique source of truth for the `local_coding` MCP surface
/// and `tool_manifest(intent="coding")`. The order is both the MCP tools/list
/// order and the coding manifest ranking.
pub(crate) const LOCAL_CODING_TOOL_NAMES: &[&str] = &[
    // entry
    "work_on_project",
    "list_projects",
    // project discovery + read
    "project_overview",
    "list_project_tracked_files",
    "list_project_files",
    "search_project_text",
    "search_project_texts",
    "read_file",
    "read_files",
    // LSP navigation
    "lsp_status",
    "document_symbols",
    "document_diagnostics",
    "hover",
    "workspace_symbols",
    "goto_definition",
    "find_references",
    "call_hierarchy",
    // guarded edits
    "apply_text_edits",
    "apply_patch_checked",
    // structured process, shell escape hatch, and jobs
    "run_process",
    "run_script",
    "run_shell",
    "run_job",
    "observe_jobs",
    "job_status",
    "job_log",
    "list_jobs",
    "stop_job",
    // validation
    "cargo_fmt",
    "cargo_check",
    "cargo_test",
    "go_test",
    "validation_summary",
    // git review
    "git_status",
    "git_log",
    "git_diff",
    "git_diff_hunks",
    "show_changes",
    "workspace_hygiene_check",
    // finish
    "finish_coding_task",
];

/// Stable task-intent views for `tool_manifest(intent=...)`.
/// Ordered lists are ranked for model selection; not a substitute for category.
/// Intent views only filter and rank discovery output; they do not change tool
/// behavior, policy, permissions, execution, or finish verdict semantics.
pub(crate) const TOOL_MANIFEST_INTENTS: &[ToolManifestIntent] = &[
    ToolManifestIntent {
        name: "coding",
        purpose: "Default coding loop: start, inspect, structured edit, validate, review, report.",
        tools: LOCAL_CODING_TOOL_NAMES,
    },
    ToolManifestIntent {
        name: "audit",
        purpose: "Read-only review/audit: inspect, git history/diff, hygiene, finish or handoff.",
        tools: &[
            "start_coding_task",
            "project_overview",
            "list_project_tracked_files",
            "read_file",
            "read_files",
            "search_project_text",
            "search_project_texts",
            "list_project_files",
            "git_status",
            "git_log",
            "git_diff_summary",
            "git_diff_hunks",
            "show_changes",
            "workspace_hygiene_check",
            "finish_coding_task",
            "session_handoff_summary",
            "validation_summary",
            "tool_manifest",
        ],
    },
    ToolManifestIntent {
        name: "exploration",
        purpose: "Light repository exploration without shell/jobs or default write paths.",
        tools: &[
            "list_projects",
            "runtime_status",
            "project_overview",
            "list_project_tracked_files",
            "list_project_files",
            "search_project_text",
            "search_project_texts",
            "read_file",
            "read_files",
            "git_status",
            "git_log",
            "tool_manifest",
        ],
    },
    ToolManifestIntent {
        name: "release",
        purpose: "Release closeout checks: hygiene, validation, jobs status, changes, finish.",
        tools: &[
            "runtime_status",
            "git_status",
            "git_diff_summary",
            "workspace_hygiene_check",
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "validation_summary",
            "observe_jobs",
            "list_jobs",
            "show_changes",
            "finish_coding_task",
        ],
    },
    ToolManifestIntent {
        name: "discovery",
        purpose: "Runtime and project discovery before choosing a work intent.",
        tools: &[
            "tool_manifest",
            "list_tools",
            "runtime_status",
            "list_agents",
            "list_projects",
            "project_overview",
        ],
    },
];

pub(crate) fn available_tool_manifest_intent_names() -> Vec<&'static str> {
    TOOL_MANIFEST_INTENTS
        .iter()
        .map(|intent| intent.name)
        .collect()
}

/// Resolve a caller-supplied intent name.
///
/// Returns `Ok(None)` for empty/whitespace input (treated as no intent).
/// Returns `Err(raw)` when a non-empty name does not match a known intent.
pub(crate) fn resolve_tool_manifest_intent(
    name: &str,
) -> Result<Option<&'static ToolManifestIntent>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let normalized = trimmed.to_ascii_lowercase().replace('-', "_");
    match TOOL_MANIFEST_INTENTS
        .iter()
        .find(|intent| intent.name == normalized)
    {
        Some(intent) => Ok(Some(intent)),
        None => Err(trimmed.to_string()),
    }
}
