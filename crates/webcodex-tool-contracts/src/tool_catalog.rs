//! Model-facing runtime tool discovery groups, recommended flows, and intents.

use super::tool_definition::{ToolDiscoveryGroup, ToolManifestIntent, ToolRecommendedFlow};

pub const TOOL_DISCOVERY_GROUP_CHECKPOINT: &str = "checkpoint";
pub const TOOL_DISCOVERY_GROUP_CLEANUP: &str = "cleanup";
pub const TOOL_DISCOVERY_GROUP_CODING_AGENT: &str = "coding_agent";
pub const TOOL_DISCOVERY_GROUP_COMMUNICATION: &str = "communication";
pub const TOOL_DISCOVERY_GROUP_AGENT_TASK: &str = "agent_task";
pub const TOOL_DISCOVERY_GROUP_AGENT_WAIT: &str = "agent_wait";
pub const TOOL_DISCOVERY_GROUP_EDIT: &str = "edit";
pub const TOOL_DISCOVERY_GROUP_FILE_TRANSFER: &str = "file_transfer";
pub const TOOL_DISCOVERY_GROUP_GIT: &str = "git";
pub const TOOL_DISCOVERY_GROUP_GOAL: &str = "goal";
pub const TOOL_DISCOVERY_GROUP_INSPECT: &str = "inspect";
pub const TOOL_DISCOVERY_GROUP_JOBS: &str = "jobs";
pub const TOOL_DISCOVERY_GROUP_PATCH: &str = "patch";
pub const TOOL_DISCOVERY_GROUP_PROJECTS: &str = "projects";
pub const TOOL_DISCOVERY_GROUP_REVIEW: &str = "review";
pub const TOOL_DISCOVERY_GROUP_RUNTIME: &str = "runtime";
pub const TOOL_DISCOVERY_GROUP_SHELL: &str = "shell";
pub const TOOL_DISCOVERY_GROUP_VALIDATION: &str = "validation";

pub const TOOL_DISCOVERY_GROUPS: &[ToolDiscoveryGroup] = &[
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_INSPECT,
        tools: &[
            "list_tools",
            "list_projects",
            "list_runners",
            "runtime_status",
            "work_on_project",
            "project_overview",
            "list_project_tracked_files",
            "read_files",
            "run_process",
            "run_script",
            "run_shell",
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
            "git_review_summary",
            "git_diff_hunks",
            "git_log",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_list",
            #[cfg(feature = "workspace-checkpoints")]
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
            "computer_read_clipboard",
            "computer_write_clipboard",
            "computer_pointer_move",
            "computer_pointer_click",
            "computer_input_text",
            "computer_snapshot",
            "computer_snapshot_display",
            "computer_save_snapshot",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_AGENT_TASK,
        tools: &[
            "create_agent_task",
            "list_agent_tasks",
            "read_agent_task",
            "assign_agent_task",
            "start_agent_task_attempt",
            "start_agent_task_endpoint_continuation",
            "start_agent_task_coding_run",
            "reconcile_agent_task_coding_run",
            "heartbeat_agent_task_attempt",
            "complete_agent_task_attempt",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_AGENT_WAIT,
        tools: &[
            "wait_for_agent_events",
            "read_agent_wait",
            "cancel_agent_wait",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_GOAL,
        tools: &[
            "create_goal",
            "get_goal",
            "present_goal_plan",
            "list_goals",
            "update_goal",
            "associate_goal_agent_task",
            "associate_goal_workflow_session",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_COMMUNICATION,
        tools: &[
            "create_agent_identity",
            "list_agent_identities",
            "update_agent_identity",
            "rotate_agent_continuation_endpoint",
            "present_agent_continuation",
            "bootstrap_agent_conversation",
            "detach_agent_endpoint",
            "create_conversation",
            "list_conversations",
            "read_conversation",
            "post_conversation_message",
            "list_agent_inbox",
            "consume_agent_deliveries",
            "consume_agent_wake",
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
            "git_commit_paths",
            "git_status",
            "git_review_summary",
            "git_diff_hunks",
            "git_log",
            "show_changes",
            "git_restore_paths",
            "discard_untracked",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_create",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_restore",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_REVIEW,
        tools: &[
            "finish_coding_task",
            "present_work_result",
            "show_changes",
            "git_review_summary",
            "git_diff_hunks",
            "workspace_hygiene_check",
            "git_log",
            "git_status",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_show",
            #[cfg(feature = "workspace-checkpoints")]
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
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_PATCH,
        tools: &["apply_patch", "apply_unified_diff"],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_EDIT,
        tools: &[
            "apply_text_edits",
            "apply_patch",
            "apply_unified_diff",
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
        name: TOOL_DISCOVERY_GROUP_FILE_TRANSFER,
        tools: &[
            "import_conversation_files_to_project",
            "export_project_artifact",
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
            "run_process",
            "run_detached_process",
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
            "run_detached_process",
            "run_job",
            "stop_job",
            "observe_jobs",
            "list_jobs",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_RUNTIME,
        tools: &[
            "list_tools",
            "work_on_project",
            "finish_coding_task",
            "session_summary",
            "update_session_context",
            "close_session",
            "post_session_message",
            "list_session_messages",
            "get_session_assignment",
            "observe_session_messages",
            "resolve_session_message",
            "complete_session_message",
            "session_discussion_summary",
            "session_handoff_summary",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_create",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_list",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_show",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_restore",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_delete",
            "list_projects",
            "list_runners",
            "runtime_status",
            "runner_config_check",
            "runner_config_reload",
            "tool_manifest",
            "plugin_tool",
            "ssh_resource",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_CODING_AGENT,
        tools: &[
            "coding_agent_start",
            "coding_agent_observe",
            "coding_agent_cancel",
        ],
    },
    ToolDiscoveryGroup {
        name: TOOL_DISCOVERY_GROUP_CLEANUP,
        tools: &[
            "delete_project_files",
            "git_restore_paths",
            "discard_untracked",
            #[cfg(feature = "workspace-checkpoints")]
            "workspace_checkpoint_delete",
        ],
    },
    #[cfg(feature = "workspace-checkpoints")]
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

pub const TOOL_RECOMMENDED_FLOWS: &[ToolRecommendedFlow] = &[
    ToolRecommendedFlow {
        name: "discovery",
        summary: "Discovery: if the user gives an exact Runner client_id, use runtime_status/list_projects for that Runner before treating it as absent. Otherwise use bounded runtime/project discovery, then batch-capable structured search/read.",
        manifest_purpose:
            "Exact Runner targeting: with client_id use runtime_status(client_id=...) or list_projects(client_id=...); use list_runners only for broad fleet discovery, then inspect/search the resolved project.",
        tools: &[
            "runtime_status",
            "list_runners",
            "list_projects",
            "project_overview",
            "read_files",
            "search_project_texts",
            "run_process",
            "run_script",
            "run_shell",
        ],
    },
    ToolRecommendedFlow {
        name: "persistent_shell",
        summary: "Persistent shell: primarily reuse one shell for repeated commands on an active named SSH resource and keep remote shell state. New target: ssh_resource list/register -> restart -> list -> bind -> open/reuse. Local persistent shell is only for true same-process state; one-shot SSH uses run_process.",
        manifest_purpose:
            "Persistent shell is SSH-resource-primary: use ssh_resource list to discover safe logical names. If a new SSH target should persist, ssh_resource register it and stop for Runner restart; after restart list again, update_session_context binds the active Runner-local named SSH resource, open_session_shell once, and session_shell_exec repeatedly preserves remote cwd/env/exports/functions/umask. A managed resource is not an arbitrary host; the SSH target does not run WebCodex Runner. Local persistent shell remains supported only when same local-process state is required; several ordinary local commands are not enough. Use session_shell_status only when needed, close_session_shell for cleanup, and run_process for explicit one-shot/no-persistence SSH.",
        tools: &[
            "ssh_resource",
            "update_session_context",
            "open_session_shell",
            "session_shell_exec",
            "session_shell_status",
            "close_session_shell",
            "run_process",
        ],
    },
    ToolRecommendedFlow {
        name: "execution_lifetime",
        summary: "Execution selection: run_process/run_script/run_shell and structured validation are Runner-owned sync-first; run_job is Runner-owned immediate async; run_detached_process is supervisor-owned immediate async; session_shell_exec continues an existing Session shell.",
        manifest_purpose:
            "Choose execution by form, lifetime, start mode, and continuation rather than duration. Runner-owned sync-first run_process/run_script/run_shell and structured validation keep the same execution when handed off and continue with observe_jobs. run_job is Runner-owned immediate async. run_detached_process is supervisor-owned immediate async and is only for a native child that must survive Runner restart/upgrade/stop/replacement; duration alone is not a reason to detach. session_shell_exec continues an existing persistent Session shell instead of creating a Job.",
        tools: &[
            "run_process",
            "run_script",
            "run_shell",
            "run_job",
            "run_detached_process",
            "session_shell_exec",
            "observe_jobs",
            "stop_job",
        ],
    },
    ToolRecommendedFlow {
        name: "inspect",
        summary: "Inspect: choose the simplest sufficient primitive. Native commands are first-class for small bounded observations; use search_project_texts/read_files when batching, path policy, bounded structured results, snapshot/continuation, or portable Runtime semantics help.",
        manifest_purpose:
            "For small bounded observations, native commands are first-class: run_process for one literal-argv executable, run_shell for shell grammar or a short related chain, and run_script for program-like supported scripts. Use search_project_texts/read_files when their batching, path policy, bounded structured result, read_revision, snapshot continuation, or portable Runtime semantics materially help.",
        tools: &[
            "search_project_texts",
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
            "Edit by mutation shape: apply_text_edits for small/local exact edits, write_project_file for intentional whole-file replacement, apply_patch for contextual patch-shaped work, bounded deterministic transforms for repetitive mechanical changes, and apply_unified_diff for external diffs.",
        manifest_purpose:
            "Choose the simplest reliable mutation for the edit shape. apply_text_edits is the strong transactional path for small/local exact edits; read_files first when read_revision, positional scope, or stale-context protection materially helps, but do not add a ritual read for globally unique exact edits that do not need it. Use write_project_file for intentional whole-file replacement. Bounded deterministic programmatic transforms through run_shell are first-class for repetitive mechanical rewrites; respect Project/path/permission policy, avoid unauthorized network, inspect the resulting diff, and validate final source. Use apply_patch only when naturally contextual or multi-hunk patch form is materially clearer. Repetitive patch targets need stable unique containing function/impl/type/test/module context. On matching_mode_rejected, never weaken the guard or switch to first_match; if patch form remains clearer, consume bounded read_files recovery and preserve unique/exact_unique. context_mismatch requires bounded reread and regeneration from current source, never blind retry. External raw diffs use apply_unified_diff.",
        tools: &[
            "read_files",
            "apply_text_edits",
            "apply_patch",
            "apply_unified_diff",
            "write_project_file",
            "run_shell",
        ],
    },
    ToolRecommendedFlow {
        name: "file_transfer",
        summary: "File transfer: host/conversation attachment -> import_conversation_files_to_project; project artifact -> export_project_artifact; caller-held bounded binary -> save_project_artifact/artifact_upload_*; bounded inspection -> read_project_artifact.",
        manifest_purpose: "Use host-native transfer at the boundary: import_conversation_files_to_project moves current host attachments into a Project without model Base64; export_project_artifact returns an authenticated ResourceLink for complete project-to-host/user transfer. Use save_project_artifact or artifact_upload_* only when bounded binary data is already held by the caller, and read_project_artifact only for bounded inspection.",
        tools: &[
            "import_conversation_files_to_project",
            "export_project_artifact",
            "save_project_artifact",
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
            "read_project_artifact_metadata",
            "read_project_artifact",
        ],
    },
    ToolRecommendedFlow {
        name: "validate",
        summary:
            "Validate: use structured validators when their canonical diagnostics, evidence/test-count, validation identity, or same-execution Job semantics help; native execution is first-class when the command is outside or awkward for that contract.",
        manifest_purpose:
            "For common supported validation, use cargo_fmt/cargo_check/cargo_test/go_test when their canonical argv, parsed diagnostics, validation identity, test-count proof, min_tests/require_tests, bounded projection, or same-execution Job handoff materially helps. Native validation is first-class when the command is outside or awkward for that structured contract: prefer run_process for one literal-argv executable, run_shell when shell grammar/output shaping is required, and run_script for program-like supported scripts. Keep independent failure/permission boundaries separate. cargo_fmt check=false retains ensure-format mutation truth; check=true stays read-only.",
        tools: &[
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "go_test",
            "observe_jobs",
            "validation_summary",
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
            "Discover a macOS or Windows application, submit its exact native launch request, then re-observe windows before any follow-up UI effect.",
        tools: &[
            "computer_list_applications",
            "computer_launch_application",
            "computer_list_windows",
            "computer_activate_window",
        ],
    },
    ToolRecommendedFlow {
        name: "commit",
        summary: "Commit: inspect git_status/show_changes, copy show_changes.head.commit into git_commit_paths.expected_head, and pass explicit changed file paths. It rejects pre-existing staged state and never pushes; keep run_process for unusual Git operations outside this narrow contract.",
        manifest_purpose:
            "Commit route: inspect with show_changes, copy head.commit to expected_head, then call git_commit_paths with explicit paths. Requires project:write + job:run because clean filters may run; isolated exact-tree commit bypasses hooks, rejects staged state, never pushes.",
        tools: &["git_status", "show_changes", "git_commit_paths"],
    },
    ToolRecommendedFlow {
        name: "review",
        summary: "Review: small bounded Git observations may use native Git. Use show_changes for workspace-wide overview/Session signals, git_review_summary to map broad or unknown committed ranges, and git_diff_hunks for fenced, paged, or continued review; check hygiene before final response.",
        manifest_purpose: "Small predictable Git observations may use native git through run_process. Use structured review when its independent semantics materially help: show_changes for bounded workspace-wide review and Session signals, git_review_summary for broad/unknown committed-range mapping, and git_diff_hunks for scope/fence-bound paging, safe continuation, and long-hunk fragmentation. Review the resulting diff before closeout and check workspace hygiene.",
        tools: &[
            "git_review_summary",
            "show_changes",
            "git_diff_hunks",
            "workspace_hygiene_check",
            "run_process",
        ],
    },
    ToolRecommendedFlow {
        name: "handoff",
        summary: "Handoff: use session_summary / session_handoff_summary; coordinator posts a todo, worker reads it once with get_session_assignment, then passes its fence to complete_session_message. Use observe_session_messages only for later generic deltas.",
        manifest_purpose: "Coordinate independent Workflow Sessions through atomic assignment snapshots, required assignment-fenced completions, and explicit generic message-state delta observation without sharing execution history, authority, subscriptions, or automatic wake-up.",
        tools: &[
            "session_summary",
            "post_session_message",
            "session_handoff_summary",
            "list_session_messages",
            "get_session_assignment",
            "observe_session_messages",
            "complete_session_message",
            "session_discussion_summary",
            "validation_summary",
            "finish_coding_task",
        ],
    },
];

/// Single ordered, unique source of truth for the fixed `local_coding` MCP
/// compatibility surface. This list intentionally does not drive Adaptive
/// Runtime intent discovery.
pub const LOCAL_CODING_TOOL_NAMES: &[&str] = &[
    // entry
    "work_on_project",
    "list_projects",
    "plugin_tool",
    // exact coordinator assignment read + atomic completion
    "get_session_assignment",
    "complete_session_message",
    // delegated ACP coding-agent Runs (explicit coding_agent:run authority)
    "coding_agent_start",
    "coding_agent_observe",
    "coding_agent_cancel",
    // project discovery + read
    "project_overview",
    "list_project_tracked_files",
    "list_project_files",
    "search_project_texts",
    "read_files",
    "read_project_artifact",
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
    "apply_patch",
    "apply_unified_diff",
    // structured process, shell semantics/scripts, and jobs
    "run_process",
    "run_script",
    "run_shell",
    "run_job",
    "observe_jobs",
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
    "git_review_summary",
    "git_diff_hunks",
    "show_changes",
    "workspace_hygiene_check",
    // finish
    "finish_coding_task",
];

/// Ordered selection surface for ordinary coding work under Adaptive Runtime.
///
/// This is intentionally smaller and more canonical than the fixed Local Coding
/// compatibility surface. It may include distinct gateway-routed specialists
/// that are worth explicit discovery, but excludes singular/legacy peers when a
/// preferred batch, structured review, or Job-continuation path exists.
pub const CODING_INTENT_TOOL_NAMES: &[&str] = &[
    "work_on_project",
    "project_overview",
    "search_project_texts",
    "read_files",
    // Distinct semantic navigation capabilities remain useful even though they
    // are long-tail Adaptive gateway targets.
    "document_symbols",
    "document_diagnostics",
    "hover",
    "workspace_symbols",
    "goto_definition",
    "find_references",
    "call_hierarchy",
    // Canonical edit plus contextual/multi-hunk specialist.
    "apply_text_edits",
    "apply_patch",
    // Ordinary execution plus program-like multi-stage specialist.
    "run_process",
    "run_script",
    "run_shell",
    "observe_jobs",
    // Common structured validation with evidence semantics.
    "cargo_fmt",
    "cargo_check",
    "cargo_test",
    "go_test",
    // Worktree and committed-range review.
    "git_review_summary",
    "git_diff_hunks",
    "show_changes",
    "workspace_hygiene_check",
    "finish_coding_task",
];

/// Stable task-intent views for `tool_manifest(intent=...)`.
/// Ordered lists are ranked for model selection; not a substitute for category.
/// Intent views only filter and rank discovery output; they do not change tool
/// behavior, policy, permissions, execution, or finish verdict semantics.
pub const TOOL_MANIFEST_INTENTS: &[ToolManifestIntent] = &[
    ToolManifestIntent {
        name: "coding",
        purpose: "Default coding loop: start, inspect, make reliable scoped changes, validate, review, report.",
        tools: CODING_INTENT_TOOL_NAMES,
    },
    ToolManifestIntent {
        name: "audit",
        purpose: "Review/audit without Project mutation or command execution: establish bounded Workflow context, inspect, read git history/diff, check hygiene, finish or handoff.",
        tools: &[
            "work_on_project",
            "project_overview",
            "list_project_tracked_files",
            "read_files",
            "search_project_texts",
            "list_project_files",
            "git_status",
            "git_log",
            "git_review_summary",
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
            "search_project_texts",
            "read_files",
            "git_status",
            "git_log",
            "tool_manifest",
        ],
    },
    ToolManifestIntent {
        name: "file_transfer",
        purpose: "Move files across the host/Project boundary without routing complete binary payloads through model text.",
        tools: &[
            "import_conversation_files_to_project",
            "export_project_artifact",
            "save_project_artifact",
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
            "read_project_artifact_metadata",
            "read_project_artifact",
        ],
    },
    ToolManifestIntent {
        name: "release",
        purpose: "Release closeout checks: hygiene, validation, jobs status, changes, finish.",
        tools: &[
            "runtime_status",
            "git_status",
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
            "list_runners",
            "list_projects",
            "project_overview",
        ],
    },
];

pub fn available_tool_manifest_intent_names() -> Vec<&'static str> {
    TOOL_MANIFEST_INTENTS
        .iter()
        .map(|intent| intent.name)
        .collect()
}

/// Resolve a caller-supplied intent name.
///
/// Returns `Ok(None)` for empty/whitespace input (treated as no intent).
/// Returns `Err(raw)` when a non-empty name does not match a known intent.
pub fn resolve_tool_manifest_intent(
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
