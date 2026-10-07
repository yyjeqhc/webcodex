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
/// read_tool_manifest lookup followed by the canonical gateway. They are excluded
/// from ordinary tools/list, intent ranking, and recommended edit routing.
pub const EXACT_MANIFEST_SPECIALIST_TOOL_NAMES: &[&str] =
    &["apply_patch", "apply_unified_diff", "write_project_file"];

/// Model-visible read specialists omitted from ordinary direct routing.
pub const EXACT_DISCOVERY_SPECIALIST_TOOL_NAMES: &[&str] =
    &["read_git_diff_hunks", "read_git_review_summary"];

/// Review specialists omitted from ordinary intent ranking/recommended flows.
/// Category and exact discovery retain them behind the canonical gateway.
pub const ORDINARY_DISCOVERY_DEMOTED_REVIEW_TOOL_NAMES: &[&str] = &[
    "read_git_diff_hunks",
    "read_git_review_summary",
    "read_workspace_changes",
];

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
        summary: "Discovery: if the user gives an exact Runner client_id, resolve its registered path/query before treating it as absent. Use resolve_workspace without Session creation, get_runtime_status/list_projects for that Runner, Workbench for explicit context, and list_runners only for an unknown machine.",
        manifest_purpose:
            "Exact Runner targeting: resolve_workspace locates registered paths without Session creation; get_runtime_status(client_id=...) inspects health and list_projects(client_id=...) lists candidates. Use list_runners for an unknown machine. Ambiguity needs explicit choice, never automatic first-match selection.",
        tools: &[
            "resolve_workspace",
            "open_webcodex_workbench",
            "get_runtime_status",
            "list_runners",
            "list_projects",
            "read_project_overview",
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
        summary: "Persistent shell: primarily reuse one shell and keep remote shell state on an active named SSH resource. New target: manage_ssh_resource list/register -> restart -> list -> bind -> open/reuse. Local shell is only for same-process state; one-shot SSH uses run_process.",
        manifest_purpose:
            "Persistent shell is SSH-resource-primary: use manage_ssh_resource list to discover safe logical names. If a new SSH target should persist, manage_ssh_resource register it and stop for Runner restart; after restart list again, update_session_context binds the active Runner-local named SSH resource, open_session_shell once, and execute_session_shell repeatedly preserves remote cwd/env/exports/functions/umask. A managed resource is not an arbitrary host; the SSH target does not run WebCodex Runner. Local persistent shell remains supported only when same local-process state is required; several ordinary local commands are not enough. Use get_session_shell_status only when needed, close_session_shell for cleanup, and run_process for explicit one-shot/no-persistence SSH.",
        tools: &[
            "manage_ssh_resource",
            "update_session_context",
            "open_session_shell",
            "execute_session_shell",
            "get_session_shell_status",
            "close_session_shell",
            "run_process",
        ],
    },
    ToolRecommendedFlow {
        name: "execution_lifetime",
        summary: "Execution selection: run_process/run_script/run_shell and structured validation are Runner-owned sync-first; run_job is Runner-owned immediate async; run_detached_process is supervisor-owned immediate async; execute_session_shell continues an existing Session shell.",
        manifest_purpose:
            "Choose execution by form, lifetime, start mode, and continuation rather than duration. Runner-owned sync-first run_process/run_script/run_shell and structured validation keep the same execution when handed off and continue with observe_jobs. run_job is Runner-owned immediate async. run_detached_process is supervisor-owned immediate async and is only for a native child that must survive Runner restart/upgrade/stop/replacement; duration alone is not a reason to detach. execute_session_shell continues an existing persistent Session shell instead of creating a Job.",
        tools: &[
            "run_process",
            "run_script",
            "run_shell",
            "run_job",
            "run_detached_process",
            "execute_session_shell",
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
            "execute_code_mode",
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
            "Use read_files for the exact source/read_revision, edit_project_files for mutation, structured validation for execution evidence, then review_changes for one authoritative Git snapshot and bounded diff page before finish_coding_task. read_workspace_changes, read_git_review_summary, and read_git_diff_hunks remain exact-name specialists/internal building blocks rather than ordinary routing choices.",
        tools: &[
            "read_files",
            "edit_project_files",
            "project_validate",
            "review_changes",
            "finish_coding_task",
        ],
    },
    ToolRecommendedFlow {
        name: "file_transfer",
        summary: "File transfer: Host -> import_host_files -> Project; Project -> inspect_project_artifact -> Host/model; Project A -> transfer_project_artifact -> Project B. inspect for one bounded segment, image for MCP image delivery, export for complete ResourceLink delivery.",
        manifest_purpose: "Keep directions explicit: import_host_files is the Host-to-Project write boundary. transfer_project_artifact is the direct Project-to-Project path and streams the exact source bytes/SHA snapshot through Control without Host attachments or model-facing base64. inspect_project_artifact is the preferred Project-to-model/host read facade: metadata observes artifact facts, inspect reads one bounded snapshot-fenced segment, image uses supported native MCP image delivery, and export uses an authenticated ResourceLink for complete transfer. Do not loop inspect chunks to transfer a whole file. save_project_artifact/artifact_upload_* remain low-level caller-held binary write primitives.",
        tools: &[
            "import_host_files",
            "transfer_project_artifact",
            "inspect_project_artifact",
            "save_project_artifact",
            "begin_artifact_upload",
            "upload_artifact_chunk",
            "finish_artifact_upload",
            "abort_artifact_upload",
        ],
    },
    ToolRecommendedFlow {
        name: "build",
        summary:
            "Build: prefer project_build for portable Rust/Go builds; use native execution only outside the canonical gateway.",
        manifest_purpose:
            "Use project_build for canonical Rust cargo build and Go go build with portable bounded package or all-packages scope. dependency_policy.mode=locked prevents adapter-managed dependency selection updates without implying offline execution. It preserves Runner-owned recipe resolution, provenance, and same-execution Job admission. Use run_process only when the required build is outside this closed contract.",
        tools: &["project_build", "observe_jobs", "run_process"],
    },
    ToolRecommendedFlow {
        name: "validate",
        summary:
            "Validate: use structured validators when their canonical diagnostics, evidence/test-count, validation identity, or same-execution Job semantics help; native execution is first-class when the command is outside or awkward for that contract. Prefer project_validate for portable common validation.",
        manifest_purpose:
            "For portable project validation, prefer project_validate; dependency_policy.mode=locked prevents adapter-managed dependency selection updates for check/test without implying offline execution. Keep cargo_fmt for explicit formatting semantics; cargo_check/cargo_test/go_test remain exact-name advanced specialists rather than ordinary routing choices. Native validation is first-class when the command is outside or awkward for that structured contract: prefer run_process for one literal-argv executable, run_shell when shell grammar/output shaping is required, and run_script for program-like supported scripts. Keep independent failure/permission boundaries separate. cargo_fmt check=false retains ensure-format mutation truth; check=true stays read-only.",
        tools: &[
            "project_validate",
            "cargo_fmt",
            "observe_jobs",
            "read_validation_summary",
            "run_process",
            "run_script",
            "run_shell",
        ],
    },
    ToolRecommendedFlow {
        name: "browser",
        summary: "Browser/CDP: launch ephemeral/managed, or discover and attach an extension-shared tab. Use opaque ids and snapshot-admitted actions. surface resolves one exact Computer window. External close only detaches; re-observe after navigation or uncertainty.",
        manifest_purpose: "Use observe_browser for targets/browsers/pages/snapshot/screenshot/diagnostics and control_browser for Browser effects. Use batch for 1..32 ordered input_text/select_option/set_value/click/upload_file/select_choice/set_date operations admitted by one current snapshot on one Browser/page. select_choice(choice_path) chooses 1..4 exact semantic levels; set_date(value) fills a custom ISO date/month picker. Both open, choose and verify internally without intermediate model snapshots. Native controls keep select_option/set_value. Ordinary field effects preserve sibling element ids; navigation, document replacement and new snapshots stale prior authority. Batch checks freshness between effects, settles once, and stops on rejection, document change or uncertainty. Read completion counts and stopped certainty before observing recovery; never blindly retry outcome_unknown. Take a fresh verification snapshot after filling and after structural/page changes. Snapshot query filters fields/text/role/group/section before bounded pagination and returns one fresh generation for batch use. truncated makes absence inconclusive. Without query, auto compacts large pages. Diagnostics since_cursor returns only later events.",
        tools: &["observe_browser", "control_browser"],
    },
    ToolRecommendedFlow {
        name: "observe_computer",
        summary: "Computer observe: one guaranteed read-only gateway for Runner/desktop discovery, accessibility inspection, clipboard read, and window/display snapshots. Choose a closed action; no control effects are admitted.",
        manifest_purpose:
            "Use observe_computer with the smallest read-only action needed: targets/windows/displays/applications, accessibility_status/accessibility_tree/find_elements/element_state, snapshot_window/snapshot_display, or read_clipboard. Exact action scopes and Runner capabilities remain fenced.",
        tools: &["observe_computer"],
    },
    ToolRecommendedFlow {
        name: "computer_application_launch",
        summary: "Computer application launch: observe_computer(action=applications), control_computer(action=launch_application), then observe_computer(action=windows) and control_computer(action=activate_window) only if activation is needed.",
        manifest_purpose:
            "Discover a fresh opaque application_id with observe_computer, launch exactly that id through control_computer, then re-observe windows before any follow-up activation/control effect.",
        tools: &["observe_computer", "control_computer"],
    },
    ToolRecommendedFlow {
        name: "commit",
        summary: "Commit: inspect get_git_status/read_workspace_changes, copy read_workspace_changes.head.commit into commit_git_paths.expected_head, and pass explicit changed file paths. It rejects pre-existing staged state and never pushes; keep run_process for unusual Git operations outside this narrow contract.",
        manifest_purpose:
            "Commit route: inspect with read_workspace_changes, copy head.commit to expected_head, then call commit_git_paths with explicit paths. Requires project:write + job:run because clean filters may run; isolated exact-tree commit bypasses hooks, rejects staged state, never pushes.",
        tools: &["get_git_status", "read_workspace_changes", "commit_git_paths"],
    },
    ToolRecommendedFlow {
        name: "review",
        summary: "Review: use review_changes as the primary ordinary workspace or committed review surface; continue only with its same-snapshot token when needed. Old review tools remain specialists; hygiene stays authoritative and separate.",
        manifest_purpose: "Use review_changes for ordinary read-only Git review: one exact snapshot returns summary, signals, file metadata, and the first bounded diff page; follow only its returned continuation. read_workspace_changes, read_git_review_summary, and read_git_diff_hunks remain exact-discovery specialists/internal projections. check_workspace_hygiene remains separate and authoritative.",
        tools: &[
            "review_changes",
            "check_workspace_hygiene",
            "run_process",
        ],
    },
    ToolRecommendedFlow {
        name: "handoff",
        summary: "Handoff/recovery only: list_sessions(project) -> choose exact Session. read_session_summary reads ledger; read_session_handoff only for missing task context or transfer, never routine progress polling. Save decisions/progress. get_session_assignment -> complete_session_message with fence.",
        manifest_purpose: "Recover missing Session identity with caller-authorized list_sessions(project), explicitly choose a returned identity, then read read_session_handoff; discovery never resumes work. Save explicit decisions/progress with post_session_message. Coordinate independent Workflow Sessions through atomic assignment snapshots, required assignment-fenced completions, and explicit generic message-state delta observation without sharing execution history, authority, subscriptions, or automatic wake-up.",
        tools: &[
            "list_sessions",
            "read_session_summary",
            "post_session_message",
            "read_session_handoff",
            "list_session_messages",
            "get_session_assignment",
            "observe_session_messages",
            "complete_session_message",
            "read_session_discussion_summary",
            "read_validation_summary",
            "finish_coding_task",
        ],
    },
];

/// Ordered selection for ordinary coding discovery under Adaptive Runtime.
///
/// This ranks useful capabilities for `read_tool_manifest(intent="coding")`; it does
/// not define direct admission. ToolDefinition rank remains the direct SSOT.
pub const CODING_INTENT_TOOL_NAMES: &[&str] = &[
    "work_on_project",
    "read_project_overview",
    "search_file_context",
    "search_project_texts",
    "read_files",
    "inspect_project_artifact",
    #[cfg(feature = "experimental-code-mode")]
    "execute_code_mode",
    // Distinct semantic navigation capabilities remain useful even though they
    // are long-tail Adaptive gateway targets.
    "list_document_symbols",
    "read_document_diagnostics",
    "read_symbol_hover",
    "list_workspace_symbols",
    "find_definition",
    "find_references",
    "read_call_hierarchy",
    // Canonical project editor. Patch/diff/whole-file specialists require
    // exact-name discovery and are intentionally absent from ordinary coding.
    "edit_project_files",
    #[cfg(feature = "experimental-code-mode")]
    "execute_mutating_code_mode",
    // Portable project build plus ordinary execution specialists.
    "project_build",
    "run_process",
    "run_script",
    "run_shell",
    "observe_jobs",
    // Portable common validation plus explicit formatting semantics. Ecosystem
    // validators remain exact-name advanced specialists rather than ordinary coding choices.
    "project_validate",
    "cargo_fmt",
    #[cfg(feature = "experimental-code-mode")]
    "execute_effectful_code_mode",
    // Primary ordinary review plus authoritative hygiene/closeout.
    "review_changes",
    "check_workspace_hygiene",
    "finish_coding_task",
];

/// Stable task-intent views for `read_tool_manifest(intent=...)`.
/// Ordered lists are ranked for model selection; not a substitute for category.
/// Intent views only filter and rank discovery output; they do not change tool
/// behavior, policy, permissions, execution, or finish verdict semantics.
pub const TOOL_MANIFEST_INTENTS: &[ToolManifestIntent] = &[
    ToolManifestIntent {
        name: "maintenance",
        purpose: "Locate exact Runners/Projects, inspect health and cached Git, then explicitly preview or mutate registration/configuration. No automatic cleanup or Session creation.",
        tools: &["resolve_workspace", "list_runners", "list_projects", "get_runtime_status",
            "open_webcodex_workbench", "get_git_status", "check_runner_config",
            "unregister_projects", "unregister_project", "reload_runner_config"],
    },
    ToolManifestIntent {
        name: "resources",
        purpose: "Explicit Project/Session context and bounded file, Goal and artifact references; not execution or implicit authority.",
        tools: &["open_webcodex_workbench", "resolve_workspace", "search_webcodex_resources",
            "read_webcodex_resource", "list_sessions", "read_project_overview", "inspect_project_artifact"],
    },

    ToolManifestIntent {
        name: "coding",
        purpose: "Default coding loop: start, inspect, make reliable scoped changes, validate, review, report.",
        tools: CODING_INTENT_TOOL_NAMES,
    },
    ToolManifestIntent {
        name: "audit",
        purpose: "Read-only audit: locate existing work, inspect code/Git and hygiene without creating a Workflow Session. Native observations may run read-only commands.",
        tools: &[
            "resolve_workspace",
            "read_project_overview",
            "list_project_tracked_files",
            "read_files",
            "search_project_texts",
            #[cfg(feature = "experimental-code-mode")]
            "execute_code_mode",
            "list_project_files",
            "get_git_status",
            "read_git_log",
            "review_changes",
            "check_workspace_hygiene",
            "read_session_handoff",
            "read_validation_summary",
            "read_tool_manifest",
        ],
    },
    ToolManifestIntent {
        name: "exploration",
        purpose: "Light repository exploration without shell/jobs or default write paths.",
        tools: &[
            "list_projects",
            "get_runtime_status",
            "read_project_overview",
            "list_project_tracked_files",
            "list_project_files",
            "search_project_texts",
            "read_files",
            #[cfg(feature = "experimental-code-mode")]
            "execute_code_mode",
            "get_git_status",
            "read_git_log",
            "read_tool_manifest",
        ],
    },
    ToolManifestIntent {
        name: "file_transfer",
        purpose: "Move files across Host/Project and Project/Project boundaries without routing complete binary payloads through model text.",
        tools: &[
            "import_host_files",
            "transfer_project_artifact",
            "inspect_project_artifact",
            "save_project_artifact",
            "begin_artifact_upload",
            "upload_artifact_chunk",
            "finish_artifact_upload",
            "abort_artifact_upload",
        ],
    },
    ToolManifestIntent {
        name: "release",
        purpose: "Release closeout checks: hygiene, validation, jobs status, changes, finish.",
        tools: &[
            "get_runtime_status",
            "get_git_status",
            "check_workspace_hygiene",
            "project_validate",
            "cargo_fmt",
            "read_validation_summary",
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
            "read_tool_manifest",
            "list_tools",
            "resolve_workspace",
            "open_webcodex_workbench",
            "search_webcodex_resources",
            "get_runtime_status",
            "list_runners",
            "list_projects",
            "read_project_overview",
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
