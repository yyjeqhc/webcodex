//! Canonical category projection, recommended flows, and task intents.

use super::tool_definition::{ToolManifestIntent, ToolRecommendedFlow};
use super::tool_policy::lookup_tool_definition;
use std::collections::BTreeMap;

/// Group an already-admitted selection by each ToolDefinition's sole category.
/// This is presentation, not admission: callers own visibility, protocol and
/// authority filtering. Unknown names are omitted; repeated names are deduplicated.
/// Both categories and members are sorted so registry order cannot cause drift.
pub fn group_tool_names_by_category<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<&'static str, Vec<&'a str>> {
    let mut categories: BTreeMap<&'static str, Vec<&'a str>> = BTreeMap::new();
    for name in names {
        if let Some(definition) = lookup_tool_definition(name) {
            categories
                .entry(definition.category)
                .or_default()
                .push(name);
        }
    }
    for members in categories.values_mut() {
        members.sort_unstable();
        members.dedup();
    }
    categories
}
/// Hidden editing specialists that remain available only through exact
/// tool_manifest lookup followed by the canonical gateway. They are excluded
/// from ordinary tools/list, intent ranking, and recommended edit routing.
pub const EXACT_MANIFEST_SPECIALIST_TOOL_NAMES: &[&str] =
    &["apply_patch", "apply_unified_diff", "write_project_file"];

/// Model-visible read specialists omitted from ordinary direct routing.
pub const EXACT_DISCOVERY_SPECIALIST_TOOL_NAMES: &[&str] =
    &["git_diff_hunks", "git_review_summary"];

/// Review specialists omitted from ordinary intent ranking/recommended flows.
/// Category and exact discovery retain them behind the canonical gateway.
pub const ORDINARY_DISCOVERY_DEMOTED_REVIEW_TOOL_NAMES: &[&str] =
    &["git_diff_hunks", "git_review_summary", "show_changes"];

/// Retain domain recipes without recommending an unavailable model operation.
/// This projection follows static ToolDefinition visibility, never workflow
/// guidance or environment settings. Re-admitting a descriptor restores its
/// complete recipes without a second continuation allowlist.
pub fn model_visible_recommended_flows() -> impl Iterator<Item = &'static ToolRecommendedFlow> {
    TOOL_RECOMMENDED_FLOWS.iter().filter(|flow| {
        flow.tools.iter().all(|name| {
            lookup_tool_definition(name)
                .is_some_and(|definition| definition.visibility.is_model_visible())
        })
    })
}

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
        name: "agent_continuation_setup",
        summary: "New durable Agent window setup: create identity -> create/rotate Endpoint -> present continuation card -> yield/end the current turn -> verify production_auto_resume_available.",
        manifest_purpose: "Use create_agent_identity, then rotate_agent_continuation_endpoint, then present_agent_continuation. Presentation success is not Host readiness: yield/end the current model turn promptly so the MCP App can mount/bind, then verify the exact Agent through list_agent_identities.production_auto_resume_available. Keep durable Agent identity independent from the Host window.",
        tools: &[
            "create_agent_identity",
            "rotate_agent_continuation_endpoint",
            "present_agent_continuation",
            "list_agent_identities",
        ],
    },
    ToolRecommendedFlow {
        name: "single_window_goal_workflow",
        summary: "Optional durable Goal workflow; current webcodex.workflow guidance owns selection. Reuse an exact active Goal, or explicitly admit Goal + Session. Present/checkpoint only for established Goal work, verify/review and explicitly complete. Host continuation is separate.",
        manifest_purpose: "Optional cross-repository workflow; consult current webcodex.workflow context for selection, not tool availability or task size alone. work_on_project may return sparse goal_context for active Goals explicitly correlated to the exact authorized Workflow Session. Reuse one exact candidate by calling get_goal and present_goal_plan; with multiple candidates, read candidate details through exact get_goal calls and explicitly choose one before present_goal_plan. Never auto-select or infer from Project, Window, title, or recency. Only after selecting durable Goal work, and without reusable active Goal context, call prepare_goal_workflow with the exact Workflow Session, bounded completion_conditions/steps and optional explicit controller Agent, then present_goal_plan. available=false is inconclusive evidence, not proof of zero active Goals. prepare_goal_workflow is durable admission only: Host carrier setup/readiness remains separate. The agent_continuation_setup flow is usable only while its dedicated presentation descriptor is advertised; when that descriptor is absent, do not repeat discovery for a Host carrier. Use checkpoint_goal at recovery-worthy boundaries; finish_coding_task returns the same sparse active Goal representation as goal_follow_up. After fresh validation/review explicitly complete all steps and update_goal. Low-level create_goal and associate_goal_workflow_session remain available for advanced composition. Tiny reads/trivial edits do not require Goal setup.",
        tools: &[
            "work_on_project", "get_goal", "prepare_goal_workflow", "present_goal_plan",
            "checkpoint_goal", "finish_coding_task", "update_goal",
        ],
    },
    ToolRecommendedFlow {
        name: "goal_agent_wait_orchestration",
        summary: "Goal-scoped AgentWait orchestration: ready Coordinator/Workers -> create controlled Goal -> create/associate exact Tasks -> register any|all Wait before workers terminalize -> start Attempt + Endpoint continuation -> on resume bootstrap/consume Wake and reconcile Wait/Goal/Tasks.",
        manifest_purpose: "For bounded Goal fan-in, keep identities explicit. First establish exact Coordinator/Worker Agents and continuation readiness. Create the Goal with an explicit controller, create exact AgentTasks with explicit workers, and associate each selected Task to that exact Goal. Then call wait_for_agent_events with goal_id plus an explicit 1..8 Task selector list before any selected worker can terminalize; use any for first-result continuation or all for fan-in. Only after registration start each worker with start_agent_task_attempt followed by start_agent_task_endpoint_continuation. On a fresh resumed Coordinator turn bootstrap the exact Wake, consume it immediately, read_agent_wait(wait_id), get_goal(goal_id), re-read every authoritative source Task, and explicitly decide/update Goal state from current durable truth. Never derive the Wait source list from Goal correlations and do not treat this flow as a scheduler, dependency DAG, auto-spawn rule, or automatic Goal progression.",
        tools: &[
            "create_agent_identity",
            "rotate_agent_continuation_endpoint",
            "present_agent_continuation",
            "list_agent_identities",
            "create_goal",
            "create_agent_task",
            "associate_goal_agent_task",
            "wait_for_agent_events",
            "start_agent_task_attempt",
            "start_agent_task_endpoint_continuation",
            "bootstrap_agent_conversation",
            "consume_agent_wake",
            "read_agent_wait",
            "get_goal",
            "read_agent_task",
            "update_goal",
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
            #[cfg(feature = "experimental-code-mode")]
            "code_mode_exec",
            "run_process",
            "run_script",
            "run_shell",
            "review_changes",
        ],
    },
    ToolRecommendedFlow {
        name: "edit",
        summary:
            "Edit: read_files → edit_project_files → structured validation → review_changes → finish_coding_task. Existing-file changes use read_revision fences; exact and deterministic range edits share one primary editor; legacy Git review specialists require exact-name discovery.",
        manifest_purpose:
            "Use read_files for the exact source/read_revision, edit_project_files for mutation, structured validation for execution evidence, then review_changes for one authoritative Git snapshot and bounded diff page before finish_coding_task. show_changes, git_review_summary, and git_diff_hunks remain exact-name specialists/internal building blocks rather than ordinary routing choices.",
        tools: &[
            "read_files",
            "edit_project_files",
            "cargo_check",
            "cargo_test",
            "review_changes",
            "finish_coding_task",
        ],
    },
    ToolRecommendedFlow {
        name: "file_transfer",
        summary: "File transfer: Host -> import_conversation_files_to_project -> Project; Project -> project_artifact -> Host/model; Project A -> transfer_project_artifact -> Project B. Use metadata for facts, inspect for one bounded segment, image for MCP image delivery, export for complete ResourceLink delivery.",
        manifest_purpose: "Keep directions explicit: import_conversation_files_to_project is the Host-to-Project write boundary. transfer_project_artifact is the direct Project-to-Project path and streams the exact source bytes/SHA snapshot through Control without Host attachments or model-facing base64. project_artifact is the preferred Project-to-model/host read facade: metadata observes artifact facts, inspect reads one bounded snapshot-fenced segment, image uses supported native MCP image delivery, and export uses an authenticated ResourceLink for complete transfer. Do not loop inspect chunks to transfer a whole file. save_project_artifact/artifact_upload_* remain low-level caller-held binary write primitives.",
        tools: &[
            "import_conversation_files_to_project",
            "transfer_project_artifact",
            "project_artifact",
            "save_project_artifact",
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
        ],
    },
    ToolRecommendedFlow {
        name: "validate",
        summary:
            "Validate: use structured validators when their canonical diagnostics, evidence/test-count, validation identity, or same-execution Job semantics help; native execution is first-class when the command is outside or awkward for that contract.",
        manifest_purpose:
            "For portable project validation, prefer project_validate. Use cargo_fmt/cargo_check/cargo_test/go_test for advanced ecosystem options when their canonical argv, parsed diagnostics, validation identity, test-count proof, min_tests/require_tests, bounded projection, or same-execution Job handoff materially helps. Native validation is first-class when the command is outside or awkward for that structured contract: prefer run_process for one literal-argv executable, run_shell when shell grammar/output shaping is required, and run_script for program-like supported scripts. Keep independent failure/permission boundaries separate. cargo_fmt check=false retains ensure-format mutation truth; check=true stays read-only.",
        tools: &[
            "project_validate",
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
        name: "browser",
        summary: "Browser/CDP runtime: discover Browser-capable Runners, launch an owned ephemeral Browser, use adaptive semantic snapshots and diagnostic deltas, act only through opaque identities, then re-observe after navigation or uncertain effects.",
        manifest_purpose: "Use browser_observe for targets/browsers/pages/snapshot/screenshot/diagnostics and browser_act for Browser effects. Use batch for 1..32 ordered input_text/select_option/set_value/click/upload_file operations admitted by one current snapshot on one Browser/page. Ordinary field effects preserve sibling element ids; navigation, document replacement and new snapshots stale prior authority. Batch checks freshness between effects, settles once, and stops on rejection, document change or uncertainty. Read completion counts and stopped certainty before observing recovery; never blindly retry outcome_unknown. Take a fresh verification snapshot after filling and after structural/page changes. Snapshot auto mode compacts large pages to admitted controls and semantic choices; diagnostics since_cursor returns only later events.",
        tools: &["browser_observe", "browser_act"],
    },
    ToolRecommendedFlow {
        name: "computer_observe",
        summary: "Computer observe: one guaranteed read-only gateway for Runner/desktop discovery, accessibility inspection, clipboard read, and window/display snapshots. Choose a closed action; no control effects are admitted.",
        manifest_purpose:
            "Use computer_observe with the smallest read-only action needed: targets/windows/displays/applications, accessibility_status/accessibility_tree/find_elements/element_state, snapshot_window/snapshot_display, or read_clipboard. Exact action scopes and Runner capabilities remain fenced.",
        tools: &["computer_observe"],
    },
    ToolRecommendedFlow {
        name: "computer_application_launch",
        summary: "Computer application launch: computer_observe(action=applications), computer_control(action=launch_application), then computer_observe(action=windows) and computer_control(action=activate_window) only if activation is needed.",
        manifest_purpose:
            "Discover a fresh opaque application_id with computer_observe, launch exactly that id through computer_control, then re-observe windows before any follow-up activation/control effect.",
        tools: &["computer_observe", "computer_control"],
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
        summary: "Review: use review_changes as the primary ordinary workspace or committed review surface; continue only with its same-snapshot token when needed. Old review tools remain specialists; hygiene stays authoritative and separate.",
        manifest_purpose: "Use review_changes for ordinary read-only Git review: one exact snapshot returns summary, signals, file metadata, and the first bounded diff page; follow only its returned continuation. show_changes, git_review_summary, and git_diff_hunks remain exact-discovery specialists/internal projections. workspace_hygiene_check remains separate and authoritative.",
        tools: &[
            "review_changes",
            "workspace_hygiene_check",
            "run_process",
        ],
    },
    ToolRecommendedFlow {
        name: "handoff",
        summary: "Handoff/recovery only: use session_summary for lightweight ledger reads. Use session_handoff_summary only for missing task context or explicit transfer, never routine progress polling. Coordinator posts a todo; worker reads it with get_session_assignment and completes with that fence.",
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

/// Ordered selection for ordinary coding discovery under Adaptive Runtime.
///
/// This ranks useful capabilities for `tool_manifest(intent="coding")`; it does
/// not define direct admission. ToolDefinition rank remains the direct SSOT.
pub const CODING_INTENT_TOOL_NAMES: &[&str] = &[
    "work_on_project",
    "project_overview",
    "search_and_read",
    "search_project_texts",
    "read_files",
    "project_artifact",
    #[cfg(feature = "experimental-code-mode")]
    "code_mode_exec",
    // Distinct semantic navigation capabilities remain useful even though they
    // are long-tail Adaptive gateway targets.
    "document_symbols",
    "document_diagnostics",
    "hover",
    "workspace_symbols",
    "goto_definition",
    "find_references",
    "call_hierarchy",
    // Canonical project editor. Patch/diff/whole-file specialists require
    // exact-name discovery and are intentionally absent from ordinary coding.
    "edit_project_files",
    #[cfg(feature = "experimental-code-mode")]
    "code_mode_exec_mutating",
    // Ordinary execution plus program-like multi-stage specialist.
    "run_process",
    "run_script",
    "run_shell",
    "observe_jobs",
    // Common structured validation with evidence semantics.
    "project_validate",
    "cargo_fmt",
    "cargo_check",
    "cargo_test",
    "go_test",
    #[cfg(feature = "experimental-code-mode")]
    "code_mode_exec_effectful",
    // Primary ordinary review plus authoritative hygiene/closeout.
    "review_changes",
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
            #[cfg(feature = "experimental-code-mode")]
            "code_mode_exec",
            "list_project_files",
            "git_status",
            "git_log",
            "review_changes",
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
            #[cfg(feature = "experimental-code-mode")]
            "code_mode_exec",
            "git_status",
            "git_log",
            "tool_manifest",
        ],
    },
    ToolManifestIntent {
        name: "file_transfer",
        purpose: "Move files across Host/Project and Project/Project boundaries without routing complete binary payloads through model text.",
        tools: &[
            "import_conversation_files_to_project",
            "transfer_project_artifact",
            "project_artifact",
            "save_project_artifact",
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
        ],
    },
    ToolManifestIntent {
        name: "release",
        purpose: "Release closeout checks: hygiene, validation, jobs status, changes, finish.",
        tools: &[
            "runtime_status",
            "git_status",
            "workspace_hygiene_check",
            "project_validate",
    "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "validation_summary",
            "observe_jobs",
            "list_jobs",
            "review_changes",
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
