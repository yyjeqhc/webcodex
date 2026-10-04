//! Shared model-facing projection for canonical coding workflow startup.
//!
//! The runtime builds this once and every transport carries the same core
//! value. The projection is deterministic, bounded, path-safe, and contains
//! only the facts a coding model needs to start or continue work.

#[cfg(test)]
use super::instruction_projection::{
    changed_instruction_sources, project_instructions_context_projection,
};
use super::instruction_projection::{instructions_projection, trim_instruction_source};
use super::projection_text::{bounded_json_string, json_string_payload_len, serialized_len};
use super::startup_catalog::{
    StartupExtensions, StartupPluginsCatalog, StartupSkillsCatalog, PLUGIN_DISCOVERY_HINT,
    SKILL_DISCOVERY_HINT, STARTUP_EXTENSION_CATALOG_HARD_MAX_BYTES,
};
#[cfg(test)]
use super::startup_catalog::{StartupPluginEntry, StartupSkillEntry};
use serde_json::{json, Value};
#[cfg(test)]
use std::collections::BTreeSet;

use super::continuation_feedback::EXPLORATION_CONTINUITY_ACTION;
use super::guidance::{core_guidance, tool_strategy_guidance};
#[cfg(test)]
use super::project_instructions::INSTRUCTION_CANDIDATE_PATHS;
use super::project_instructions::{
    ProjectInstructionsSnapshot, ProjectInstructionsSummarySnapshot,
};
use super::project_resolution::ResolvedProject;
use super::session_context::canonical_repository_key;
use super::sessions::SessionSummary;
use super::tool_inputs::{CodingGuidanceProfile, StartupDetail};

// Reserve transport-envelope headroom so the rendered ToolResult stays
// below the documented 32 KiB startup ceiling.
pub(crate) const STANDARD_STARTUP_HARD_MAX_BYTES: usize = 30 * 1024;
pub(crate) const REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON: &str =
    "not_requested_by_work_on_project";
const MIN_INSTRUCTION_CONTENT_JSON_BYTES: usize = 512;
const MAX_CHANGED_PATHS: usize = 20;
const MAX_MINIMAL_EXPLORATION_PATHS: usize = 3;
const MAX_STANDARD_EXPLORATION_PATHS: usize = 12;
const MAX_FAILURES: usize = 10;
const MAX_SUGGESTED_ACTIONS: usize = 5;
const MAX_PATH_JSON_BYTES: usize = 192;
const MAX_FAILURE_NAME_JSON_BYTES: usize = 160;
const MAX_FAILURE_FILE_JSON_BYTES: usize = 160;
const MAX_ACTION_JSON_BYTES: usize = 384;
const MAX_INSTRUCTION_EXCERPT_JSON_BYTES: usize = 768;

pub(crate) use webcodex_core::runtime_contract::{
    BUILTIN_CODING_WORKFLOW_CONTRACT, BUILTIN_CODING_WORKFLOW_VERSION,
};
#[cfg(test)]
pub(crate) use webcodex_core::runtime_contract::{
    BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS,
    BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEM_CHARS,
};

/// Stable model-facing coding/review semantics owned by WebCodex itself.
///
/// This is intentionally not a project instruction source and is never stored
/// as Session mode, capability, permission, or execution authority. Ordinary
/// implementation uses the default guidance; task text may explicitly request
/// the independent review role, whose name only selects review behavior.
pub(crate) fn builtin_coding_workflow_projection(profile: CodingGuidanceProfile) -> Value {
    builtin_coding_workflow_projection_with_policy(profile, Default::default())
}

/// Request-context guidance may select a deployment preference without changing
/// canonical startup parsing, ToolSpecs or previously established workflow state.
pub(crate) fn builtin_coding_workflow_projection_with_policy(
    profile: CodingGuidanceProfile,
    policy: crate::model_workflow::ModelWorkflowPolicy,
) -> Value {
    json!({
        "contract": BUILTIN_CODING_WORKFLOW_CONTRACT,
        "version": BUILTIN_CODING_WORKFLOW_VERSION,
        "authority": "model_guidance_only",
        "role_selection": "Ordinary implementation uses default guidance. Use independent_review only for an explicit independent review pass. Roles never grant authority.",
        "guidance": core_guidance(),
        "tool_strategy": tool_strategy_projection(profile),
        "model_protocol": {
            "goal_workflow": policy.goal_selection_guidance(),
            "goal_continuation": policy.mcp_app_resume.guidance(),
            "goal_checkpoint": policy.goal_checkpoint_guidance(),
            "handoff_recovery": "Use handoff recovery only after task-context loss/compaction/restart, explicit transfer or user-requested recovery. Never use it for routine progress/baselines; timeout alone is not Session loss. Missing exact session_id: original Window uses _wc.context=[\"workflow.resume\"]; otherwise list_sessions(project). Choose an authorized candidate, read read_session_handoff, check basis completeness/truncation, then resume Active work with work_on_project(session_id=..., instruction=...). Checkout derives and reauthorizes its bound Project; reuse project_ref. Closed is historical only. Notes are reports; recheck stale facts. Dirty files alone do not prove external changes; recover exact Session evidence first.",
            "session_recording": "When work_on_project creates or resumes, prefer its returned session_ref as _wc.record on the next ordinary model-visible call for recorder provenance only; canonical wc_sess_* remains valid. business session_id may target another Session; _wc.record grants no authority. At recovery-worthy boundaries save agreed decisions and current progress/remaining work with post_session_message(kind=decision/progress) on the explicit business Session; use a stable delivery_key for uncertain same-payload retries. Host conversation/private reasoning is not automatically recorded. Before switching save context; never put credentials in notes. Keep delegated findings as explicit reports, not native validation proof.",
            "session_message_ack": "For any retained collaboration message with requires_ack=true projected in session_attention, peer_messages, or operator_messages, attach _wc.ack=[<wc_msg_*>] on the next ordinary model-visible call. Compact _wc.ack_ref remains Session-only. ACK proves model-context retention only; it never resolves messages, grants authority, or gates execution.",
            "window_reply": "After handling an operator_messages item, reply textually by attaching _wc.reply={reply_to:<wc_msg_*>,message:<text>} to the next ordinary model-visible WebCodex call in the same Window. No _wc.record or business session_id is required. If requires_ack=true, also include the same id in _wc.ack. The reply is a post-result collaboration sidecar: it never authorizes, predicts, or changes the main tool effect.",
            "session_message_resolution": "For a handled non-todo, attach _wc.resolve={message_id:<wc_msg_*>,resolution:<text>} on the next ordinary call together with _wc.record; if requires_ack, also include the id in _wc.ack. Resolve cannot predict or change the main call. Todos use complete_session_message.",
            "control_sidecars": "_wc.control is optional and only for an established workflow: piggyback a known transition on an already-needed call; otherwise omit it or use its standalone tool. Never create a Goal/Agent just to use a sidecar. Follow the canonical control schema and load webcodex.goal_workflow for Goal transitions. One mutation per phase, independently authorized; no automatic transitions. Post failure preserves main success. session_context_update remains fail-closed pending CAS/replay support.",
            "context_sidecar": "_wc.context is an optional post-result context request; it never authorizes. jobs.attention is Project-level, not Session/control; workflow.resume is Window/principal-scoped recovery evidence only and never selects or resumes a Session. Recover project.instructions by observation call before dependent mutation.",
            "bootstrap_reuse": "Fresh model context: first suitable work_on_project requests _wc.context=[\"project.instructions\",\"webcodex.workflow\"]. First entry to another Project: request project.instructions; reuse retained webcodex.workflow. With complete/current context retained, omit both context keys on later tasks/Sessions and return visits. Session/ClientWindow continuity does not prove retention. Refresh after compaction/context loss, changed fingerprints/revisions, incomplete/truncated/missing bodies, stale workflow, or user request. status=loaded means observed only; content_included=true and complete means read. Follow read_more for missing bodies; read_files for exact ranges. Context refresh never authorizes.",
            "bootstrap_observations": "work_on_project.workspace is the initial branch/HEAD/status observation; refresh after mutation/stale state and for mutation fences. semantic_navigation available=true works without get_lsp_status; diagnose probe_timeout/probe_failed/unavailable. Fresh/first Project: include_extension_catalog=true/default. Set false only while that Project's complete/sufficient catalog is retained; refresh for changed catalog revision/runtime, truncated discovery, missing relevant metadata, context loss, or user request. If one Skill description materially matches the task, load_skill; Do not load unrelated Skills. Reuse retained same-Project Skill bodies/revisions; catalog metadata alone is not a loaded Skill. Plugin catalog is selection metadata: reuse its applicable retained schema/binding; Never carry a projectBound binding to another Project or recover by replaying uncertain effects. Skill guidance grants no authority.",
            "runner_targeting": "For exact Runner client_id, use get_runtime_status(client_id=...) or list_projects(client_id=...) before treating it as absent.",
            "persistent_shell": "Local: run_process=literal argv; run_shell=shell grammar/short chains; run_script=program-like scripts; specialize for added semantics. Persistent shell only for repeated named-SSH state or local same-process state.",
            "work_result_presentation": "For substantial Project work in a stable client Window, call present_work_result(project) exactly once immediately after the first successful project-scoped WebCodex action. Do not wait for work_on_project, a Workflow Session, mutation, validation, or closeout. The mounted card refreshes the same Window ActionAudit activity as WebUI, including observe/diagnostic actions; optional linked Session collaboration and final changes may appear later. Never repeat presentation or model-poll it in the same Window. Tiny one-step/read-only lookups may skip it; finish may suggest it only as a fallback if no card was presented.",
            "normal_closeout": "Source/validation/open evidence: finish_coding_task(summary_only=true). Artifact tasks: pass outputs after task-appropriate count/content/format checks; read-only tasks without deliverables omit outputs. No established Goal means no Goal closeout. For goal_follow_up, load webcodex.goal_workflow and preserve unresolved obligations; preference changes never complete a Goal. Read/planning/artifact: finalize directly; close established Goals only after verified completion."
        },
        "roles": {
            "independent_review": {
                "purpose": "Review independently within task scope.",
                "guidance": [
                    "Challenge authority, bounds, malformed data, privacy, replay/races, timeouts, fail-closed behavior.",
                    "For review-only tasks, report concrete file/line findings and impact; do not edit. Fix only when the task authorizes corrections, with focused regression validation."
                ]
            }
        }
    })
}

fn tool_strategy_projection(profile: CodingGuidanceProfile) -> Value {
    let mut strategy = json!({
        "profile": profile,
        "guidance": tool_strategy_guidance(profile),
    });
    if profile == CodingGuidanceProfile::HostCodeMode {
        strategy["host_orchestration"] = host_orchestration_catalog();
    }
    strategy
}

fn host_orchestration_catalog() -> Value {
    let mut native_batch_first = Vec::new();
    let mut independent_parallel_reads = Vec::new();
    let mut compound_preferred = Vec::new();
    let mut sequential = Vec::new();

    for definition in super::tool_definition::model_visible_tool_definitions() {
        let hint = definition.host_orchestration;
        if hint.native_batch_field.is_some() {
            native_batch_first.push(definition.name);
        }
        match hint.concurrency {
            super::tool_definition::ToolHostConcurrencyHint::IndependentParallelRead => {
                independent_parallel_reads.push(definition.name);
            }
            super::tool_definition::ToolHostConcurrencyHint::Sequential => {
                sequential.push(definition.name);
            }
            super::tool_definition::ToolHostConcurrencyHint::Unspecified => {}
        }
        if hint.compound_preferred {
            compound_preferred.push(definition.name);
        }
    }

    for list in [
        &mut native_batch_first,
        &mut independent_parallel_reads,
        &mut compound_preferred,
        &mut sequential,
    ] {
        list.sort_unstable();
    }

    json!({
        "guidance_only": true,
        "native_batch_first": native_batch_first,
        "independent_parallel_reads": independent_parallel_reads,
        "compound_preferred": compound_preferred,
        "sequential": sequential,
    })
}

// Model-side caps for the deterministic repository overview projected into the
// shared startup brief. Lists are stable-sorted and record truncation; a list
// that exceeds its cap is never silently presented as complete.
pub(crate) const REPOSITORY_MAX_PROJECT_TYPES: usize = 8;
pub(crate) const REPOSITORY_MAX_MANIFESTS: usize = 12;
pub(crate) const REPOSITORY_MAX_KEY_FILES: usize = 16;
pub(crate) const REPOSITORY_MAX_TOP_LEVEL: usize = 24;
pub(crate) const REPOSITORY_MAX_SUGGESTED_READS: usize = 8;
pub(crate) const REPOSITORY_MAX_ROOTS_PER_CLASS: usize = 8;
pub(crate) const REPOSITORY_MAX_WARNINGS: usize = 8;
pub(crate) const REPOSITORY_MAX_PROJECT_TYPE_EVIDENCE: usize = 4;

pub(crate) struct StartupBriefInput<'a> {
    pub(crate) guidance_profile: CodingGuidanceProfile,
    pub(crate) detail: StartupDetail,
    pub(crate) requested_project: &'a str,
    pub(crate) project_resolution: &'a Value,
    pub(crate) resolved: &'a ResolvedProject,
    pub(crate) project_ref: Option<&'a str>,
    pub(crate) knowledge_association: Option<&'a Value>,
    pub(crate) session: &'a SessionSummary,
    pub(crate) continuation_kind: &'a str,
    pub(crate) reused: bool,
    pub(crate) resume_requested: bool,
    pub(crate) instructions: &'a ProjectInstructionsSnapshot,
    pub(crate) previous_instructions: Option<&'a ProjectInstructionsSummarySnapshot>,
    pub(crate) force_instruction_load: bool,
    pub(crate) include_instruction_content: bool,
    pub(crate) extensions: Option<&'a StartupExtensions>,
    pub(crate) coding_agent_providers:
        &'a [webcodex_core::coding_agent::CodingAgentProviderSummary],
    pub(crate) git: &'a Value,
    pub(crate) semantic_navigation: &'a Value,
    pub(crate) repository: &'a Value,
    pub(crate) continuation_feedback: &'a Value,
    pub(crate) active_jobs: &'a Value,
    pub(crate) owning_runner_available: Option<bool>,
    pub(crate) canonical_repository_root_matches: Option<bool>,
    pub(crate) runtime_status_call_failed: bool,
}

/// Build the one startup contract used by MCP and REST.
pub(crate) fn build_startup_brief(input: StartupBriefInput<'_>) -> Value {
    let minimal = input.detail == StartupDetail::Minimal;
    let instruction_projection = instructions_projection(
        input.instructions,
        input.previous_instructions,
        input.force_instruction_load,
        !minimal && input.include_instruction_content,
        minimal,
    );
    let continuation = continuation_projection(
        input.continuation_feedback,
        input.active_jobs,
        minimal,
        input.continuation_kind,
    );
    let workspace = workspace_projection(input.git);
    let semantic_navigation = semantic_navigation_projection(input.semantic_navigation);
    let repository = repository_projection(input.repository);
    let (blockers, warnings) = startup_issues(
        &input,
        &instruction_projection,
        &workspace,
        &semantic_navigation,
    );
    let startup_verdict = startup_verdict(&blockers, &warnings, &continuation, minimal);
    let repository_identity = format!(
        "repository:v1:{}",
        canonical_repository_key(&input.resolved.config.path)
    );
    let mut brief = json!({
        "detail": input.detail.as_str(),
        "session": {
            "session_id": input.session.session_id,
            "mode": input.session.mode,
            "execution_context": input.session.execution_context,
            "continuation": input.continuation_kind,
            "reused": input.reused,
            "resume_requested": input.resume_requested,
            "explicit_resume_required_for_continuation": true,
        },
        "project": {
            "requested": input.requested_project,
            "resolved_id": input.resolved.resolved_id,
            "repository_identity": repository_identity,
            "canonical_repository_root_matches": input.canonical_repository_root_matches,
        },
        "project_resolution": input.project_resolution,
        "workspace": workspace,
        "workflow": builtin_coding_workflow_projection(input.guidance_profile),
        "instructions": instruction_projection,
        "continuation": continuation,
        "semantic_navigation": semantic_navigation,
        "repository": repository,
        "blockers": blockers,
        "warnings": warnings,
        "startup_verdict": startup_verdict,
        "deterministic": true,
        "llm_summary": false,
    });
    if let Some(association) = input.knowledge_association {
        brief["project"]["knowledge_association"] = association.clone();
    }
    if let Some(project_ref) = input.project_ref {
        brief["project"]["project_ref"] = json!(project_ref);
    }
    if let Some(extensions) = input.extensions {
        debug_assert!(extensions.serialized_len() <= STARTUP_EXTENSION_CATALOG_HARD_MAX_BYTES);
        brief["extensions"] = serde_json::to_value(extensions).unwrap_or_else(|_| {
            json!({
                "skills": StartupSkillsCatalog::unavailable("skills_catalog_unavailable"),
                "plugins": StartupPluginsCatalog::unavailable("plugin_runtime_unavailable"),
            })
        });
    }
    if !input.coding_agent_providers.is_empty() {
        brief["coding_agent_providers"] = json!(input.coding_agent_providers);
    }
    enforce_hard_size_limit(&mut brief);
    brief
}

/// Return the already-built shared core from any successful startup detail.
/// Standard/minimal are the core itself; full embeds the same core alongside
/// diagnostics. No transport is allowed to reassemble these fields.
pub(crate) fn startup_brief_from_output(output: &Value) -> Option<&Value> {
    if output.get("detail").and_then(Value::as_str) == Some("full") {
        output
            .get("startup_brief")
            .filter(|value| value.is_object())
    } else if output.is_object() {
        Some(output)
    } else {
        None
    }
}

fn workspace_projection(git: &Value) -> Value {
    let counts = git.get("counts").unwrap_or(&Value::Null);
    let git_available = git.get("available").and_then(Value::as_bool);
    let non_git_project = git
        .get("non_git_project")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let clean = git.get("clean").and_then(Value::as_bool);
    let conflicts = count(counts, "conflicted");
    let git_status = if non_git_project {
        "not_applicable"
    } else if git_available == Some(false) || clean.is_none() {
        "unavailable"
    } else if conflicts > 0 {
        "conflicted"
    } else if clean == Some(true) {
        "clean"
    } else {
        "dirty"
    };
    let git_reason_code = match git_status {
        "not_applicable" => Some("non_git_project"),
        "unavailable" => Some("git_unavailable"),
        _ => None,
    };
    let status = if conflicts > 0 {
        "blocked"
    } else if non_git_project {
        "available"
    } else if git_available == Some(false) || clean.is_none() {
        "unavailable"
    } else if clean == Some(true) {
        "clean"
    } else {
        "dirty"
    };
    let head = git
        .pointer("/head/commit")
        .cloned()
        .or_else(|| git.get("head").filter(|value| value.is_string()).cloned())
        .unwrap_or(Value::Null);
    let show_changes = git.get("show_changes").unwrap_or(&Value::Null);
    let mut changed_paths = Vec::new();
    for path in show_changes
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| file.get("path").and_then(Value::as_str))
    {
        if path.len() > MAX_PATH_JSON_BYTES || changed_paths.iter().any(|seen| seen == path) {
            continue;
        }
        changed_paths.push(path.to_string());
        if changed_paths.len() == MAX_CHANGED_PATHS {
            break;
        }
    }
    let changed_paths_total = show_changes
        .get("files_total")
        .and_then(Value::as_u64)
        .unwrap_or(changed_paths.len() as u64);
    let changed_paths_truncated = show_changes
        .get("files_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || changed_paths_total > changed_paths.len() as u64;
    json!({
        "status": status,
        "git_available": git_available,
        "git": {
            "status": git_status,
            "reason_code": git_reason_code,
        },
        "branch": git.get("branch").cloned().unwrap_or(Value::Null),
        "head": head,
        "clean": clean,
        "conflicts": conflicts,
        "modified": count(counts, "modified"),
        "untracked": count(counts, "untracked"),
        "staged": count(counts, "staged"),
        "upstream_status": git.get("upstream_status").cloned().unwrap_or_else(|| json!("unobserved")),
        "upstream_reason_code": git.get("upstream_reason_code").cloned().unwrap_or(Value::Null),
        "upstream": git.get("upstream").cloned().unwrap_or(Value::Null),
        "ahead": git.get("ahead").cloned().unwrap_or(Value::Null),
        "behind": git.get("behind").cloned().unwrap_or(Value::Null),
        "changed_paths": changed_paths,
        "changed_paths_total": changed_paths_total,
        "changed_paths_truncated": changed_paths_truncated,
    })
}

fn count(value: &Value, field: &str) -> u64 {
    value.get(field).and_then(Value::as_u64).unwrap_or(0)
}

fn semantic_navigation_projection(value: &Value) -> Value {
    json!({
        "supported": value
            .get("supported")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "status": value
            .get("status")
            .cloned()
            .unwrap_or_else(|| json!("probe_failed")),
        // Preserve an indeterminate startup observation as null. Coercing a
        // timed-out status probe to false would turn "not observed" into a
        // false semantic-navigation unavailability claim.
        "available": value
            .get("available")
            .filter(|available| available.is_boolean() || available.is_null())
            .cloned()
            .unwrap_or(Value::Null),
        "provider": value.get("server").cloned().unwrap_or(Value::Null),
        "capability": if value
            .get("supported")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            json!("lsp_read_only_navigation")
        } else {
            Value::Null
        },
        "reason_code": value.get("reason_code").cloned().unwrap_or(Value::Null),
    })
}

/// Deterministic repository overview for the startup brief. `source` is the
/// already-built overview (available) or an unavailable marker. Lists are
/// stable-sorted and bounded; a list that exceeds its cap records the original
/// total and flips truncated instead of silently presenting as complete.
pub(crate) fn repository_projection(source: &Value) -> Value {
    if source.get("status").and_then(Value::as_str) != Some("available") {
        let reason_code = if source.get("reason_code").and_then(Value::as_str)
            == Some(REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON)
        {
            REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON
        } else {
            "unsupported_or_unavailable"
        };
        return json!({
            "status": "unavailable",
            "reason_code": reason_code,
        });
    }
    json!({
        "status": "available",
        "reason_code": Value::Null,
        "project_types": bounded_list(
            source.get("project_types"),
            source
                .get("project_types")
                .and_then(Value::as_array)
                .map(Vec::len),
            false,
            REPOSITORY_MAX_PROJECT_TYPES,
            project_type_item,
        ),
        "manifests": bounded_list(
            source.get("manifests"),
            source.get("manifests").and_then(Value::as_array).map(Vec::len),
            false,
            REPOSITORY_MAX_MANIFESTS,
            manifest_item,
        ),
        "key_files": bounded_list(
            source.get("key_files"),
            source.get("key_files").and_then(Value::as_array).map(Vec::len),
            false,
            REPOSITORY_MAX_KEY_FILES,
            key_file_item,
        ),
        "roots": roots_projection(source.get("roots").unwrap_or(&Value::Null)),
        "top_level": bounded_list(
            source.get("top_level"),
            source.get("top_level").and_then(Value::as_array).map(Vec::len),
            false,
            REPOSITORY_MAX_TOP_LEVEL,
            top_level_item,
        ),
        "suggested_next_reads": bounded_list(
            source.get("suggested_next_reads"),
            source
                .get("suggested_next_reads")
                .and_then(Value::as_array)
                .map(Vec::len),
            false,
            REPOSITORY_MAX_SUGGESTED_READS,
            suggested_read_item,
        ),
        "scan": scan_projection(source.get("scan").unwrap_or(&Value::Null)),
        "warnings": bounded_string_items(source.get("warnings"), REPOSITORY_MAX_WARNINGS, 96)
            .into_iter()
            .map(Value::String)
            .collect::<Vec<_>>(),
    })
}

/// Project the fixed `scan` fields only. Never clone an arbitrary Runner
/// `scan` object: a malformed payload cannot smuggle extra fields into the
/// startup brief through this subobject. Missing or non-conforming fields
/// fall back to safe defaults rather than transparently echoing the source.
fn scan_projection(source: &Value) -> Value {
    let empty = serde_json::Map::new();
    let scan = source.as_object().unwrap_or(&empty);
    json!({
        "max_depth": scan.get("max_depth").and_then(Value::as_u64).unwrap_or(0),
        "limit": scan.get("limit").and_then(Value::as_u64).unwrap_or(0),
        "returned_entry_count": scan
            .get("returned_entry_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "truncated": scan.get("truncated").and_then(Value::as_bool).unwrap_or(false),
        "truncation_reason": scan
            .get("truncation_reason")
            .cloned()
            .filter(|value| value.is_string() || value.is_null())
            .unwrap_or(Value::Null),
    })
}

fn project_type_item(value: &Value) -> Option<Value> {
    let kind = value.get("kind").and_then(Value::as_str)?;
    let evidence = value.get("evidence").and_then(Value::as_array);
    let evidence_total = evidence.map(Vec::len).unwrap_or(0);
    let items = evidence
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(REPOSITORY_MAX_PROJECT_TYPE_EVIDENCE)
        .map(|path| json!(bounded_json_string(path, MAX_PATH_JSON_BYTES).0))
        .collect::<Vec<_>>();
    Some(json!({
        "kind": kind,
        "evidence": items,
        "evidence_total": evidence_total,
        "evidence_truncated": evidence_total > items.len(),
    }))
}

fn manifest_item(value: &Value) -> Option<Value> {
    let path = value.get("path").and_then(Value::as_str)?;
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("manifest");
    Some(json!({
        "path": bounded_json_string(path, MAX_PATH_JSON_BYTES).0,
        "kind": kind,
    }))
}

fn key_file_item(value: &Value) -> Option<Value> {
    let path = value.get("path").and_then(Value::as_str)?;
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("key_file");
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(json!({
        "path": bounded_json_string(path, MAX_PATH_JSON_BYTES).0,
        "kind": kind,
        "reason": bounded_json_string(reason, 160).0,
    }))
}

fn top_level_item(value: &Value) -> Option<Value> {
    let path = value.get("path").and_then(Value::as_str)?;
    let kind = value.get("kind").and_then(Value::as_str).unwrap_or("file");
    Some(json!({
        "path": bounded_json_string(path, MAX_PATH_JSON_BYTES).0,
        "kind": kind,
    }))
}

fn suggested_read_item(value: &Value) -> Option<Value> {
    let path = value.get("path").and_then(Value::as_str)?;
    let reason = value
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(json!({
        "path": bounded_json_string(path, MAX_PATH_JSON_BYTES).0,
        "reason": bounded_json_string(reason, 160).0,
    }))
}

fn roots_projection(source: &Value) -> Value {
    let mut projected = serde_json::Map::new();
    for class in ["source", "tests", "docs", "examples", "scripts", "ci"] {
        projected.insert(
            class.to_string(),
            bounded_list(
                source.get(class),
                source.get(class).and_then(Value::as_array).map(Vec::len),
                false,
                REPOSITORY_MAX_ROOTS_PER_CLASS,
                project_path_item,
            ),
        );
    }
    projected.insert(
        "classification_basis".to_string(),
        source
            .get("classification_basis")
            .cloned()
            .unwrap_or_else(|| json!("conventional_directory_name")),
    );
    Value::Object(projected)
}

fn continuation_projection(
    feedback: &Value,
    active_jobs: &Value,
    minimal: bool,
    continuation_kind: &str,
) -> Value {
    let status = feedback.get("status").and_then(Value::as_str).unwrap_or(
        if continuation_kind == "created" {
            "not_applicable"
        } else {
            "unknown"
        },
    );
    let attempt = feedback.get("attempt").unwrap_or(&Value::Null);
    let validation = attempt.get("validation").unwrap_or(&Value::Null);
    let delta = feedback.get("validation_delta").unwrap_or(&Value::Null);
    let delta_failures = delta.get("failures").unwrap_or(&Value::Null);
    let changes = attempt.get("changes").unwrap_or(&Value::Null);
    let exploration = attempt.get("exploration").unwrap_or(&Value::Null);
    let actions = attempt
        .get("suggested_next_actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut projected = json!({
        "status": status,
        "reason_code": feedback.get("reason_code").cloned().unwrap_or(Value::Null),
        "instruction": {
            "status": attempt
                .pointer("/instruction/status")
                .cloned()
                .unwrap_or_else(|| json!("not_observed")),
            "excerpt": attempt
                .pointer("/instruction/excerpt")
                .and_then(Value::as_str)
                .map(|value| bounded_json_string(value, MAX_INSTRUCTION_EXCERPT_JSON_BYTES).0),
            "truncated": attempt
                .pointer("/instruction/truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "outcome": {
            "status": attempt
                .pointer("/outcome/status")
                .cloned()
                .unwrap_or_else(|| json!("unknown")),
            "reason_codes": bounded_string_items(
                attempt.pointer("/outcome/reason_codes"),
                8,
                96,
            ),
        },
        "changed_paths": bounded_list(
            changes.get("changed_paths"),
            changes
                .get("total_changed_paths")
                .and_then(Value::as_u64)
                .map(|value| value as usize),
            changes
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            if minimal { 0 } else { MAX_CHANGED_PATHS },
            project_path_item,
        ),
        "exploration": {
            "paths": bounded_list(
                exploration.get("observed_paths"),
                exploration
                    .get("total_observed_paths")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                exploration
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                if minimal {
                    MAX_MINIMAL_EXPLORATION_PATHS
                } else {
                    MAX_STANDARD_EXPLORATION_PATHS
                },
                exploration_path_item,
            ),
            "read_count": exploration
                .get("read_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "search_count": exploration
                .get("search_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "navigation_count": exploration
                .get("navigation_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "latest_tool": exploration
                .get("latest_tool")
                .cloned()
                .unwrap_or(Value::Null),
            "complete": exploration
                .get("complete")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "validation": {
            "latest_status": normalized_validation_status(
                validation.get("latest_status").and_then(Value::as_str),
            ),
            "open_failures": bounded_list(
                validation.get("open_failures"),
                validation
                    .get("total_open_failures")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                validation
                    .get("failures_truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                if minimal { 0 } else { MAX_FAILURES },
                failure_item,
            ),
            "delta": {
                "status": delta
                    .pointer("/comparison/status")
                    .cloned()
                    .unwrap_or_else(|| json!("unavailable")),
                "reason_code": delta
                    .pointer("/comparison/reason_code")
                    .cloned()
                    .unwrap_or(Value::Null),
                "new_failures": bounded_list(
                    delta_failures.get("newly_failed"),
                    delta_failures
                        .get("total_newly_failed")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize),
                    false,
                    if minimal { 0 } else { MAX_FAILURES },
                    failure_item,
                ),
                "resolved_failures": bounded_list(
                    delta_failures.get("resolved"),
                    delta_failures
                        .get("total_resolved")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize),
                    false,
                    if minimal { 0 } else { MAX_FAILURES },
                    failure_item,
                ),
                "still_failing": bounded_list(
                    delta_failures.get("still_failing"),
                    delta_failures
                        .get("total_still_failing")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize),
                    false,
                    if minimal { 0 } else { MAX_FAILURES },
                    failure_item,
                ),
            },
        },
        "jobs": {
            "active_count": active_jobs
                .get("active_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "blocking_active_count": active_jobs
                .get("blocking_active_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "nonblocking_active_count": active_jobs
                .get("nonblocking_active_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "recovering_count": active_jobs
                .get("recovering_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "terminal_pending_count": active_jobs
                .get("terminal_pending_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "latest_status": active_jobs
                .pointer("/recent/0/status")
                .cloned()
                .unwrap_or_else(|| json!("not_observed")),
        },
        "open_guidance": {
            "count": attempt
                .pointer("/guidance/open_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "risk_count": attempt
                .pointer("/guidance/open_risk_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "todo_count": attempt
                .pointer("/guidance/open_todo_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "latest_kind": attempt
                .pointer("/guidance/latest_open_kind")
                .cloned()
                .unwrap_or(Value::Null),
        },
        "suggested_next_actions": bounded_list(
            Some(&Value::Array(actions)),
            None,
            false,
            if minimal { 1 } else { MAX_SUGGESTED_ACTIONS },
            action_item,
        ),
    });

    if active_jobs.get("active_job").is_some_and(Value::is_object) {
        projected["jobs"]["active_job"] = active_jobs["active_job"].clone();
    }

    if minimal {
        // The first action remains concrete, while bulk evidence lists are
        // represented only by their total/returned/truncated metadata.
        projected["instruction"]["excerpt"] = Value::Null;
    }
    projected
}

fn normalized_validation_status(status: Option<&str>) -> &'static str {
    match status {
        Some("passed") => "passed",
        Some("failed") => "failed",
        Some("expected") => "expected",
        Some("inconclusive") => "inconclusive",
        Some("not_run") => "not_run",
        Some("unavailable") => "unavailable",
        _ => "unknown",
    }
}

fn bounded_list(
    source: Option<&Value>,
    total: Option<usize>,
    source_truncated: bool,
    limit: usize,
    project: fn(&Value) -> Option<Value>,
) -> Value {
    let source_items = source.and_then(Value::as_array);
    let observed = source_items.map(Vec::len).unwrap_or(0);
    let total = total.unwrap_or(observed).max(observed);
    let items: Vec<Value> = source_items
        .into_iter()
        .flatten()
        .filter_map(project)
        .take(limit)
        .collect();
    let returned = items.len();
    json!({
        "items": items,
        "total": total,
        "returned": returned,
        "truncated": source_truncated || total > returned,
    })
}

fn project_path_item(value: &Value) -> Option<Value> {
    value
        .as_str()
        .map(|value| json!(bounded_json_string(value, MAX_PATH_JSON_BYTES).0))
}

fn exploration_path_item(value: &Value) -> Option<Value> {
    value.as_str().and_then(|value| {
        super::sessions::normalize_observed_project_path(value).map(Value::String)
    })
}

fn action_item(value: &Value) -> Option<Value> {
    value
        .as_str()
        .map(|value| json!(bounded_json_string(value, MAX_ACTION_JSON_BYTES).0))
}

fn failure_item(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let kind = match object.get("kind").and_then(Value::as_str) {
        Some("test") => "test",
        Some("diagnostic") => "diagnostic",
        _ => "unknown",
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(|value| bounded_json_string(value, MAX_FAILURE_NAME_JSON_BYTES).0)
        .unwrap_or_else(|| "unknown failure".to_string());
    let file = object
        .get("file")
        .and_then(Value::as_str)
        .map(|value| bounded_json_string(value, MAX_FAILURE_FILE_JSON_BYTES).0);
    Some(json!({
        "kind": kind,
        "name": name,
        "file": file,
        "line": object.get("line").cloned().unwrap_or(Value::Null),
    }))
}

fn bounded_string_items(source: Option<&Value>, limit: usize, budget: usize) -> Vec<String> {
    source
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(limit)
        .map(|value| bounded_json_string(value, budget).0)
        .collect()
}

fn startup_issues(
    input: &StartupBriefInput<'_>,
    instructions: &Value,
    workspace: &Value,
    semantic_navigation: &Value,
) -> (Vec<String>, Vec<String>) {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    if workspace
        .get("conflicts")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        push_unique(&mut blockers, "workspace_conflicts");
    }
    let blocking_jobs = input
        .active_jobs
        .get("blocking_active_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if blocking_jobs > 0 {
        push_unique(&mut blockers, "active_jobs_blocking");
    }
    if input.owning_runner_available == Some(false) {
        push_unique(&mut blockers, "runner_unavailable");
    }
    if input.runtime_status_call_failed {
        push_unique(&mut warnings, "runtime_status_unavailable");
    }
    if workspace.get("status").and_then(Value::as_str) == Some("dirty") {
        push_unique(&mut warnings, "dirty_worktree");
    }
    if workspace.get("git_available").and_then(Value::as_bool) == Some(false)
        && workspace.pointer("/git/status").and_then(Value::as_str) != Some("not_applicable")
    {
        push_unique(&mut warnings, "git_unavailable");
    }
    if instructions.get("status").and_then(Value::as_str) == Some("unavailable") {
        push_unique(&mut warnings, "rules_unavailable");
    }
    if input.repository.get("status").and_then(Value::as_str) == Some("unavailable")
        && input.repository.get("reason_code").and_then(Value::as_str)
            != Some(REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON)
    {
        push_unique(&mut warnings, "repository_overview_unavailable");
    }
    let semantic_status = semantic_navigation.get("status").and_then(Value::as_str);
    if semantic_navigation
        .get("available")
        .and_then(Value::as_bool)
        == Some(false)
        && !matches!(semantic_status, Some("not_applicable"))
    {
        push_unique(&mut warnings, "semantic_navigation_unavailable");
    }
    let active_jobs = input
        .active_jobs
        .get("active_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if active_jobs > 0 && blocking_jobs == 0 {
        push_unique(&mut warnings, "active_jobs_present");
    }
    (blockers, warnings)
}

fn startup_verdict(
    blockers: &[String],
    warnings: &[String],
    continuation: &Value,
    minimal: bool,
) -> Value {
    let mut actions = Vec::new();
    let continuation_actions = continuation
        .pointer("/suggested_next_actions/items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if continuation
        .pointer("/validation/open_failures/total")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        if let Some(action) = continuation_actions.first() {
            push_unique(&mut actions, action);
        }
    }
    if blockers.iter().any(|item| item == "active_jobs_blocking") {
        push_unique(&mut actions, "inspect or await blocking active jobs");
    }
    if blockers.iter().any(|item| item == "runner_unavailable") {
        push_unique(
            &mut actions,
            "restore the project runner connection before attempting coding tools",
        );
    }
    if continuation
        .pointer("/open_guidance/risk_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        push_unique(
            &mut actions,
            "address open guidance on the session message board",
        );
    }
    if blockers.iter().any(|item| item == "workspace_conflicts") {
        push_unique(
            &mut actions,
            "resolve the existing workspace conflicts before editing unrelated code",
        );
    }
    for action in continuation_actions
        .iter()
        .copied()
        .filter(|action| *action != EXPLORATION_CONTINUITY_ACTION)
    {
        push_unique(&mut actions, action);
    }
    if warnings.iter().any(|item| item == "dirty_worktree") {
        push_unique(
            &mut actions,
            "inspect and preserve the existing worktree changes while continuing",
        );
    }
    if warnings.iter().any(|item| item == "rules_unavailable") {
        push_unique(
            &mut actions,
            "read the fixed repository instruction files before making changes",
        );
    }
    if continuation_actions.contains(&EXPLORATION_CONTINUITY_ACTION) {
        push_unique(&mut actions, EXPLORATION_CONTINUITY_ACTION);
    }
    if actions.is_empty() {
        actions.push("begin the requested coding task".to_string());
    }
    actions.truncate(if minimal { 1 } else { MAX_SUGGESTED_ACTIONS });
    let status = if !blockers.is_empty() {
        "fail"
    } else if !warnings.is_empty() {
        "warn"
    } else {
        "pass"
    };
    json!({
        "status": status,
        "blocking": !blockers.is_empty(),
        "suggested_next_actions": actions,
    })
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn enforce_hard_size_limit(brief: &mut Value) {
    // Reserve the same workflow allowance for either strategy. Otherwise a
    // larger guidance projection could trim unrelated instructions/evidence.
    let workflow_budget = serialized_len(&builtin_coding_workflow_projection(
        CodingGuidanceProfile::Direct,
    ))
    .max(serialized_len(&builtin_coding_workflow_projection(
        CodingGuidanceProfile::HostCodeMode,
    )));
    #[cfg(feature = "experimental-code-mode")]
    let workflow_budget = workflow_budget.max(serialized_len(&builtin_coding_workflow_projection(
        CodingGuidanceProfile::CodeMode,
    )));
    let max_bytes = STANDARD_STARTUP_HARD_MAX_BYTES
        .saturating_sub(workflow_budget.saturating_sub(serialized_len(&brief["workflow"])));
    if serialized_len(brief) <= max_bytes {
        return;
    }

    // Extension entries are optional selection metadata and can be recovered
    // through explicit discovery. Under aggregate startup pressure, trim them
    // before repository facts or project instruction prose.
    const EXTENSION_CATALOGS: &[(&str, &str)] = &[
        ("/extensions/skills", SKILL_DISCOVERY_HINT),
        ("/extensions/plugins", PLUGIN_DISCOVERY_HINT),
    ];
    loop {
        if serialized_len(brief) <= max_bytes {
            return;
        }
        let mut removed = false;
        for (pointer, discovery_hint) in EXTENSION_CATALOGS {
            let Some(catalog) = brief.pointer_mut(pointer).and_then(Value::as_object_mut) else {
                continue;
            };
            let Some(entries) = catalog.get_mut("entries").and_then(Value::as_array_mut) else {
                continue;
            };
            if !entries.is_empty() {
                entries.pop();
                let returned_count = entries.len();
                catalog.insert("returned_count".to_string(), json!(returned_count));
                catalog.insert("truncated".to_string(), json!(true));
                catalog.insert("discovery_hint".to_string(), json!(discovery_hint));
                removed = true;
                break;
            }
        }
        if !removed {
            break;
        }
    }

    // Repository metadata is lower priority than rule prose: drop optional
    // repository list items (top_level, key_files, manifests, project-type
    // evidence, suggested reads, and per-class roots) before touching rule
    // content. Every list keeps its required fields and truncation metadata.
    const REPOSITORY_LIST_POINTERS: &[&str] = &[
        "/repository/top_level",
        "/repository/key_files",
        "/repository/manifests",
        "/repository/suggested_next_reads",
        "/repository/roots/source",
        "/repository/roots/tests",
        "/repository/roots/docs",
        "/repository/roots/examples",
        "/repository/roots/scripts",
        "/repository/roots/ci",
    ];
    loop {
        if serialized_len(brief) <= max_bytes {
            return;
        }
        let mut removed = false;
        for pointer in REPOSITORY_LIST_POINTERS {
            let Some(list) = brief.pointer_mut(pointer) else {
                continue;
            };
            let Some(items) = list.get_mut("items").and_then(Value::as_array_mut) else {
                continue;
            };
            if items.len() > 1 {
                items.pop();
                list["returned"] = json!(items.len());
                list["truncated"] = json!(true);
                removed = true;
                break;
            }
        }
        // Shrink project-type evidence before its entries when all list
        // items are already at their floor.
        if !removed {
            let Some(types) = brief
                .pointer_mut("/repository/project_types/items")
                .and_then(Value::as_array_mut)
            else {
                break;
            };
            let mut shrank = false;
            for project_type in types.iter_mut().rev() {
                let Some(evidence) = project_type
                    .get_mut("evidence")
                    .and_then(Value::as_array_mut)
                else {
                    continue;
                };
                if evidence.len() > 1 {
                    evidence.pop();
                    project_type["evidence_truncated"] = json!(true);
                    shrank = true;
                    break;
                }
            }
            if !shrank {
                break;
            }
        }
    }

    // Rules content is the largest prose block. Shrink each source only to a
    // useful floor first, preserving content from every loaded source along
    // with source identity, headings, read_more, and truncation facts.
    loop {
        if serialized_len(brief) <= max_bytes {
            return;
        }
        let Some(sources) = brief
            .pointer_mut("/instructions/sources")
            .and_then(Value::as_array_mut)
        else {
            break;
        };
        let Some(source) = sources.iter_mut().rev().find(|source| {
            source
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|content| {
                    json_string_payload_len(content) > MIN_INSTRUCTION_CONTENT_JSON_BYTES
                })
        }) else {
            break;
        };
        let content = source
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current_budget = json_string_payload_len(content);
        let next_budget = current_budget
            .saturating_sub(512)
            .max(MIN_INSTRUCTION_CONTENT_JSON_BYTES);
        trim_instruction_source(source, next_budget);
        brief["instructions"]["truncated"] = json!(true);
    }

    // Extremely large continuation evidence is reduced item-by-item. Every
    // list retains the original total and flips truncated=true.
    const LIST_POINTERS: &[&str] = &[
        "/continuation/validation/delta/resolved_failures",
        "/continuation/validation/delta/new_failures",
        "/continuation/validation/delta/still_failing",
        "/continuation/validation/open_failures",
        "/continuation/exploration/paths",
        "/continuation/changed_paths",
    ];
    loop {
        if serialized_len(brief) <= max_bytes {
            return;
        }
        let mut removed = false;
        for pointer in LIST_POINTERS {
            let Some(list) = brief.pointer_mut(pointer) else {
                continue;
            };
            let Some(items) = list.get_mut("items").and_then(Value::as_array_mut) else {
                continue;
            };
            if items.len() > 1 {
                items.pop();
                list["returned"] = json!(items.len());
                list["truncated"] = json!(true);
                removed = true;
                break;
            }
        }
        if !removed {
            break;
        }
    }

    // Headings are optional navigation metadata; source path/fingerprint and
    // rule content/read_more remain authoritative.
    loop {
        if serialized_len(brief) <= max_bytes {
            return;
        }
        let Some(sources) = brief
            .pointer_mut("/instructions/sources")
            .and_then(Value::as_array_mut)
        else {
            break;
        };
        let Some(headings) = sources
            .iter_mut()
            .rev()
            .filter_map(|source| source.get_mut("headings"))
            .filter_map(Value::as_array_mut)
            .find(|headings| !headings.is_empty())
        else {
            break;
        };
        headings.pop();
    }

    // The bounded evidence above normally leaves the brief below the hard
    // limit. As a final defensive step, reduce rule excerpts below the useful
    // floor while retaining source metadata and a conservative read_more hint.
    loop {
        if serialized_len(brief) <= max_bytes {
            return;
        }
        let Some(sources) = brief
            .pointer_mut("/instructions/sources")
            .and_then(Value::as_array_mut)
        else {
            break;
        };
        let Some(source) = sources.iter_mut().rev().find(|source| {
            source
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|content| !content.is_empty())
        }) else {
            break;
        };
        let content = source
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current_budget = json_string_payload_len(content);
        let next_budget = current_budget.saturating_sub(128);
        trim_instruction_source(source, next_budget);
        brief["instructions"]["truncated"] = json!(true);
    }

    // The common workflow allowance is an internal evidence-alignment target,
    // not a tighter wire limit for smaller profiles. Synthetic or future
    // untrimmed fields may make that target unreachable after every canonical
    // degradable field is exhausted; in that case the real 30 KiB startup hard
    // bound remains authoritative. HostCodeMode itself has max_bytes equal to
    // the hard bound because its workflow envelope establishes the allowance.
    debug_assert!(
        serialized_len(brief) <= STANDARD_STARTUP_HARD_MAX_BYTES,
        "startup brief base contract exceeded its hard byte budget"
    );
}

#[cfg(test)]
pub(crate) fn startup_brief_size(value: &Value) -> usize {
    serialized_len(value)
}

#[cfg(test)]
pub(crate) fn instruction_source_paths(value: &Value) -> BTreeSet<String> {
    value
        .pointer("/instructions/sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|source| source.get("path").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
pub(crate) fn validate_schema_instance_for_test(
    instance: &Value,
    schema: &Value,
) -> Result<(), String> {
    webcodex_tool_contracts::test_support::validate_schema_instance(instance, schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::ProjectConfig;
    use crate::tool_runtime::project_instructions::LoadedInstructionCandidate;
    use crate::tool_runtime::sessions::SessionGuards;
    use crate::tool_runtime::{SessionMode, ToolRuntime};

    #[test]
    fn model_protocol_teaches_only_common_invocation_envelope_sidecars() {
        let workflow = builtin_coding_workflow_projection(CodingGuidanceProfile::Direct);
        let protocol = workflow["model_protocol"].to_string();
        for field in [
            "_wc.record",
            "_wc.ack",
            "_wc.ack_ref",
            "_wc.reply",
            "_wc.resolve",
            "_wc.context",
            "_wc.control",
        ] {
            assert!(
                protocol.contains(field),
                "missing model-facing envelope guidance for {field}"
            );
        }
        for legacy in [
            "recording_session_id",
            "ack_session_message_ids",
            "window_reply=",
            "session_message_resolution on",
            "context_request=",
            "_control",
        ] {
            assert!(
                !protocol.contains(legacy),
                "legacy root wrapper guidance leaked: {legacy}"
            );
        }
    }

    #[test]
    fn semantic_navigation_projection_preserves_probe_timeout_as_unknown() {
        let source = json!({
            "supported": true,
            "available": Value::Null,
            "status": "probe_timeout",
            "server": Value::Null,
            "reason_code": "status_probe_timed_out",
        });
        let projection = semantic_navigation_projection(&source);
        assert_eq!(projection["supported"], true);
        assert_eq!(projection["available"], Value::Null);
        assert_eq!(projection["status"], "probe_timeout");
        assert_eq!(projection["provider"], Value::Null);
        assert_eq!(projection["capability"], "lsp_read_only_navigation");
        assert_eq!(projection["reason_code"], "status_probe_timed_out");
    }

    #[test]
    fn repository_scan_projection_keeps_only_fixed_fields() {
        // A malicious available overview smuggles oversized extras inside the
        // `scan` object. The projection must keep only the five fixed fields.
        let source = json!({
            "status": "available",
            "reason_code": Value::Null,
            "scan": {
                "max_depth": 2,
                "limit": 120,
                "returned_entry_count": 7,
                "truncated": true,
                "truncation_reason": "max_depth",
                "padding": "X".repeat(20_000),
                "nested": {"deep": "Y".repeat(10_000)},
            },
            "runner_secret": "/absolute/leak",
        });
        let projection = repository_projection(&source);
        let scan = &projection["scan"];
        assert!(scan.is_object());
        let mut keys: Vec<&str> = scan
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "limit",
                "max_depth",
                "returned_entry_count",
                "truncated",
                "truncation_reason"
            ]
        );
        let serialized = projection.to_string();
        assert!(!serialized.contains("padding"));
        assert!(!serialized.contains("runner_secret"));
        assert!(!serialized.contains("/absolute/leak"));
        assert!(
            serialized.len() <= STANDARD_STARTUP_HARD_MAX_BYTES,
            "scan projection exceeded hard limit: {}",
            serialized.len()
        );
    }

    #[test]
    fn repository_scan_projection_falls_back_safely_on_malformed_scan() {
        // A scan that is not an object, or whose typed fields are wrong, must
        // still project to the fixed shape with safe defaults rather than echo
        // the malformed source.
        let source = json!({
            "status": "available",
            "reason_code": Value::Null,
            "scan": "not-an-object",
        });
        let scan = &repository_projection(&source)["scan"];
        assert!(scan.is_object());
        assert_eq!(scan["max_depth"], 0);
        assert_eq!(scan["limit"], 0);
        assert_eq!(scan["returned_entry_count"], 0);
        assert_eq!(scan["truncated"], false);
        assert_eq!(scan["truncation_reason"], Value::Null);
    }

    fn failure(index: usize) -> Value {
        json!({
            "kind": if index.is_multiple_of(2) { "test" } else { "diagnostic" },
            "name": format!("very_long_failure_name_{index}_{}", "x".repeat(180)),
            "file": format!("src/generated/{index}/{}.rs", "y".repeat(180)),
            "line": index + 1,
        })
    }

    fn large_feedback() -> Value {
        let paths = (0..120)
            .map(|index| json!(format!("src/generated/{index}/{}.rs", "p".repeat(220))))
            .collect::<Vec<_>>();
        let observed_paths = (0..100)
            .map(|index| json!(format!("src/explored/{index}/{}.rs", "e".repeat(220))))
            .collect::<Vec<_>>();
        let failures = (0..30).map(failure).collect::<Vec<_>>();
        json!({
            "status": "available",
            "reason_code": Value::Null,
            "attempt": {
                "instruction": {
                    "status": "available",
                    "excerpt": "continue the exact failing target ".repeat(80),
                    "truncated": true,
                },
                "outcome": {
                    "status": "blocked",
                    "reason_codes": ["validation_failed"],
                },
                "changes": {
                    "changed_paths": paths,
                    "total_changed_paths": 120,
                    "truncated": false,
                },
                "exploration": {
                    "observed_paths": observed_paths,
                    "total_observed_paths": 140,
                    "truncated": true,
                    "read_count": 80,
                    "search_count": 40,
                    "navigation_count": 20,
                    "latest_tool": "find_definition",
                    "complete": true,
                },
                "validation": {
                    "latest_status": "failed",
                    "open_failures": failures,
                    "total_open_failures": 30,
                    "failures_truncated": false,
                },
                "jobs": {
                    "active_count": 7,
                    "recovering_count": 2,
                    "terminal_pending_count": 1,
                    "latest_job_status": "recovering",
                },
                "guidance": {
                    "open_count": 9,
                    "open_risk_count": 4,
                    "open_todo_count": 5,
                    "latest_open_kind": "risk",
                },
                "suggested_next_actions": (0..12)
                    .map(|index| format!("rerun focused target {index} {}", "a".repeat(420)))
                    .collect::<Vec<_>>(),
            },
            "validation_delta": {
                "comparison": {
                    "status": "available",
                    "reason_code": Value::Null,
                },
                "failures": {
                    "newly_failed": (0..25).map(failure).collect::<Vec<_>>(),
                    "total_newly_failed": 25,
                    "resolved": (25..50).map(failure).collect::<Vec<_>>(),
                    "total_resolved": 25,
                    "still_failing": (50..75).map(failure).collect::<Vec<_>>(),
                    "total_still_failing": 25,
                    "list_truncated": false,
                },
            },
        })
    }

    fn instruction_snapshot() -> ProjectInstructionsSnapshot {
        ProjectInstructionsSnapshot::from_candidates(
            INSTRUCTION_CANDIDATE_PATHS
                .iter()
                .enumerate()
                .map(|(index, path)| LoadedInstructionCandidate {
                    source_scope:
                        webcodex_core::project_instructions::InstructionSourceScope::Project,
                    path: (*path).to_string(),
                    content: format!(
                        "# Rule source {index}\n## Required\n{}\n",
                        format!("bounded rule {index}; ").repeat(1_000)
                    ),
                    total_lines: 3,
                    full_sha256: None,
                })
                .collect(),
            true,
        )
    }

    #[test]
    fn instruction_delta_schema_accepts_replaced_runner_sources_and_project_changes() {
        use webcodex_core::project_instructions::InstructionSourceScope;
        let snapshot = |version: &str| {
            let mut candidates = (0..16)
                .map(|index| LoadedInstructionCandidate {
                    source_scope: InstructionSourceScope::Runner,
                    path: format!("runner/{index}/{version}.md"),
                    content: version.to_string(),
                    total_lines: 1,
                    full_sha256: None,
                })
                .collect::<Vec<_>>();
            candidates.extend(INSTRUCTION_CANDIDATE_PATHS.iter().map(|path| {
                LoadedInstructionCandidate {
                    source_scope: InstructionSourceScope::Project,
                    path: (*path).to_string(),
                    content: version.to_string(),
                    total_lines: 1,
                    full_sha256: None,
                }
            }));
            ProjectInstructionsSnapshot::from_candidates(candidates, true)
        };
        let previous = snapshot("old").to_summary();
        let current = snapshot("new");
        let changed = changed_instruction_sources(&current, Some(&previous));
        assert_eq!(changed.len(), 37);
        let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
        let changed_schema = &schema["properties"]["output"]["properties"]["instructions"]
            ["properties"]["changed_sources"];
        assert!(
            changed_schema.is_object(),
            "work_on_project instruction delta schema"
        );
        validate_schema_instance_for_test(&json!(changed), changed_schema).unwrap();
    }

    #[test]
    fn reused_instruction_status_never_reprojects_body() {
        let current = instruction_snapshot();
        let previous = current.to_summary();

        let projection = instructions_projection(&current, Some(&previous), false, true, false);
        assert_eq!(projection["status"], "reused");
        assert_eq!(projection["content_included"], false);
        assert!(projection["sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["content"].is_null()));
    }

    fn delta_feedback(new_total: usize, resolved_total: usize, still_total: usize) -> Value {
        let failures = |total: usize, offset: usize| {
            (0..total.min(20))
                .map(|index| failure(offset + index))
                .collect::<Vec<_>>()
        };
        json!({
            "status": "available",
            "attempt": {},
            "validation_delta": {
                "comparison": {"status": "available", "reason_code": Value::Null},
                "failures": {
                    "newly_failed": failures(new_total, 0),
                    "total_newly_failed": new_total,
                    "resolved": failures(resolved_total, 100),
                    "total_resolved": resolved_total,
                    "still_failing": failures(still_total, 200),
                    "total_still_failing": still_total,
                    "list_truncated": true,
                }
            }
        })
    }

    fn empty_active_jobs() -> Value {
        json!({
            "active_count": 0,
            "blocking_active_count": 0,
            "nonblocking_active_count": 0,
            "recovering_count": 0,
            "terminal_pending_count": 0,
            "recent": [],
        })
    }

    fn large_repository() -> Value {
        let types = (0..12)
            .map(|index| {
                json!({
                    "kind": format!("kind_{index}"),
                    "evidence": (0..6)
                        .map(|j| format!("src/generated/{index}/{j}.rs"))
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>();
        let manifests = (0..40)
            .map(|index| {
                json!({
                    "path": format!("crates/generated-{index}/Cargo.toml"),
                    "kind": "rust_manifest",
                })
            })
            .collect::<Vec<_>>();
        let key_files = (0..60)
            .map(|index| {
                json!({
                    "path": format!("generated/key_{index}.md"),
                    "kind": "documentation",
                    "reason": format!("generated key file {index} {}", "r".repeat(200)),
                })
            })
            .collect::<Vec<_>>();
        let top_level = (0..80)
            .map(|index| {
                json!({
                    "path": format!("generated/top_level_{index}.rs"),
                    "kind": "file",
                })
            })
            .collect::<Vec<_>>();
        let suggested = (0..20)
            .map(|index| {
                json!({
                    "path": format!("generated/read_{index}.rs"),
                    "reason": format!("generated read reason {index}"),
                })
            })
            .collect::<Vec<_>>();
        let roots = |prefix: &str| {
            (0..30)
                .map(|index| json!(format!("{prefix}/generated-{index}")))
                .collect::<Vec<_>>()
        };
        json!({
            "status": "available",
            "reason_code": Value::Null,
            "project_types": types,
            "manifests": manifests,
            "key_files": key_files,
            "roots": {
                "source": roots("src"),
                "tests": roots("tests"),
                "docs": roots("docs"),
                "examples": roots("examples"),
                "scripts": roots("scripts"),
                "ci": roots(".github"),
                "classification_basis": "conventional_directory_name",
            },
            "top_level": top_level,
            "suggested_next_reads": suggested,
            "scan": {
                "max_depth": 2,
                "limit": 120,
                "returned_entry_count": 200,
                "truncated": true,
                "truncation_reason": "limit",
            },
            "warnings": ["symlinks_skipped", "non_utf8_paths_skipped"],
        })
    }

    #[test]
    fn startup_exploration_projection_uses_minimal_and_standard_path_limits() {
        let feedback = json!({
            "status": "available",
            "attempt": {
                "exploration": {
                    "observed_paths": (0..20)
                        .map(|index| format!("src/explored-{index:02}.rs"))
                        .collect::<Vec<_>>(),
                    "total_observed_paths": 20,
                    "truncated": false,
                    "read_count": 3,
                    "search_count": 2,
                    "navigation_count": 4,
                    "latest_tool": "read_files",
                    "complete": true
                }
            },
            "validation_delta": {}
        });
        let minimal = continuation_projection(&feedback, &empty_active_jobs(), true, "continued");
        let standard = continuation_projection(&feedback, &empty_active_jobs(), false, "continued");

        assert_eq!(
            minimal["exploration"]["paths"],
            json!({
                "items": ["src/explored-00.rs", "src/explored-01.rs", "src/explored-02.rs"],
                "total": 20,
                "returned": 3,
                "truncated": true
            })
        );
        assert_eq!(standard["exploration"]["paths"]["returned"], 12);
        assert_eq!(standard["exploration"]["paths"]["total"], 20);
        assert_eq!(standard["exploration"]["paths"]["truncated"], true);
        assert_eq!(standard["exploration"]["read_count"], 3);
        assert_eq!(standard["exploration"]["search_count"], 2);
        assert_eq!(standard["exploration"]["navigation_count"], 4);
        assert_eq!(standard["exploration"]["latest_tool"], "read_files");
        assert_eq!(standard["exploration"]["complete"], true);
    }

    #[test]
    fn continuation_projection_preserves_only_explicit_active_job_handle() {
        let feedback = json!({"status": "available", "attempt": {}, "validation_delta": {}});
        let mut jobs = empty_active_jobs();
        assert!(
            continuation_projection(&feedback, &jobs, true, "continued")["jobs"]
                .get("active_job")
                .is_none()
        );

        jobs["active_job"] = json!({
            "job_id": "job-exact",
            "status": "running",
            "kind": "shell"
        });
        let projected = continuation_projection(&feedback, &jobs, true, "continued");
        assert_eq!(
            projected["jobs"]["active_job"],
            json!({"job_id": "job-exact", "status": "running", "kind": "shell"})
        );
        assert_eq!(projected["jobs"]["active_count"], 0);
    }

    #[test]
    fn validation_delta_truncates_only_still_failing_when_only_it_exceeds_limit() {
        let projected = continuation_projection(
            &delta_feedback(0, 1, 25),
            &empty_active_jobs(),
            false,
            "continued",
        );
        let delta = &projected["validation"]["delta"];
        assert_eq!(
            delta["new_failures"],
            json!({"items": [], "total": 0, "returned": 0, "truncated": false})
        );
        assert_eq!(delta["resolved_failures"]["total"], 1);
        assert_eq!(delta["resolved_failures"]["returned"], 1);
        assert_eq!(delta["resolved_failures"]["truncated"], false);
        assert_eq!(delta["still_failing"]["total"], 25);
        assert_eq!(delta["still_failing"]["returned"], 10);
        assert_eq!(delta["still_failing"]["truncated"], true);
    }

    #[test]
    fn validation_delta_truncates_only_new_failures_when_only_it_exceeds_limit() {
        let projected = continuation_projection(
            &delta_feedback(25, 1, 0),
            &empty_active_jobs(),
            false,
            "continued",
        );
        let delta = &projected["validation"]["delta"];
        assert_eq!(delta["new_failures"]["total"], 25);
        assert_eq!(delta["new_failures"]["returned"], 10);
        assert_eq!(delta["new_failures"]["truncated"], true);
        assert_eq!(delta["resolved_failures"]["total"], 1);
        assert_eq!(delta["resolved_failures"]["returned"], 1);
        assert_eq!(delta["resolved_failures"]["truncated"], false);
        assert_eq!(
            delta["still_failing"],
            json!({"items": [], "total": 0, "returned": 0, "truncated": false})
        );
    }

    #[test]
    fn standard_coding_task_startup_is_deterministic_and_hard_bounded() {
        let runtime = ToolRuntime::new_for_tests();
        let session = runtime.sessions.start_session_with_guards(
            Some("agent:size:demo".to_string()),
            Some("bounded startup".to_string()),
            SessionMode::Normal,
            SessionGuards::default(),
        );
        let resolved = ResolvedProject {
            input: "agent:size:demo".to_string(),
            resolved_id: "agent:size:demo".to_string(),
            config: ProjectConfig {
                path: "/tmp/model-facing-size-fixture".to_string(),
                client_id: "size".to_string(),
                allow_patch: true,
            },
            root_fingerprint: None,
            knowledge_association: None,
        };
        let instructions = instruction_snapshot();
        let git = json!({
            "available": true,
            "branch": "main",
            "head": {"commit": "a".repeat(40)},
            "clean": false,
            "counts": {
                "conflicted": 0,
                "modified": 120,
                "untracked": 4,
                "staged": 3,
            },
        });
        let semantic_navigation = json!({
            "supported": true,
            "available": true,
            "status": "running",
            "server": "rust-analyzer",
            "reason_code": Value::Null,
        });
        let feedback = large_feedback();
        let active_jobs = json!({
            "active_count": 7,
            "blocking_active_count": 6,
            "nonblocking_active_count": 1,
            "recovering_count": 2,
            "terminal_pending_count": 1,
            "recent": [{"status": "recovering"}],
        });
        let extensions = StartupExtensions {
            skills: StartupSkillsCatalog::available(
                "wc_skillcat_qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqo".to_string(),
                false,
                (0..64)
                    .map(|index| StartupSkillEntry {
                        skill_id: format!(
                            "wc_skill_{}",
                            webcodex_core::compact::encode(&(index as u128).to_be_bytes()[0..])
                        ),
                        name: format!("skill-{index:02}"),
                        description: format!("skill-{index:02}-{}", "s".repeat(500)),
                        source_scope: "project".to_string(),
                        trust: "project_content".to_string(),
                        name_conflict: index < 2,
                    })
                    .collect(),
            ),
            plugins: StartupPluginsCatalog::available(
                format!("wc_plugcat_{}", webcodex_core::compact::encode([0xbb; 32])),
                64,
                (0..64)
                    .map(|index| StartupPluginEntry {
                        plugin: format!("plugin-{index:02}"),
                        name: format!("Plugin {index:02}"),
                        tool: format!("tool_{index:02}"),
                        title: Some(format!("Tool {index:02}")),
                        description: Some(format!("plugin-{index:02}-{}", "p".repeat(500))),
                        annotations: webcodex_core::plugin::PluginSelectionAnnotations {
                            read_only_hint: Some(true),
                            destructive_hint: Some(false),
                            idempotent_hint: Some(true),
                            open_world_hint: Some(false),
                        },
                    })
                    .collect(),
            ),
        };
        assert!(extensions.serialized_len() <= STARTUP_EXTENSION_CATALOG_HARD_MAX_BYTES);
        assert!(extensions.skills.truncated);
        assert!(extensions.plugins.truncated);
        let project_resolution = json!({
            "source": "project",
            "outcome": "resolved_existing_project",
            "resolved_project": "agent:size:demo",
            "registered": false,
        });
        let build = |guidance_profile| {
            build_startup_brief(StartupBriefInput {
                guidance_profile,
                detail: StartupDetail::Standard,
                requested_project: "agent:size:demo",
                project_resolution: &project_resolution,
                resolved: &resolved,
                project_ref: None,
                knowledge_association: None,
                session: &session,
                continuation_kind: "continued",
                reused: true,
                resume_requested: false,
                instructions: &instructions,
                previous_instructions: None,
                force_instruction_load: true,
                include_instruction_content: true,
                extensions: Some(&extensions),
                coding_agent_providers: &[],
                git: &git,
                semantic_navigation: &semantic_navigation,
                repository: &large_repository(),
                continuation_feedback: &feedback,
                active_jobs: &active_jobs,
                owning_runner_available: Some(true),
                canonical_repository_root_matches: Some(true),
                runtime_status_call_failed: false,
            })
        };
        let first = build(CodingGuidanceProfile::Direct);
        let second = build(CodingGuidanceProfile::Direct);
        let host = build(CodingGuidanceProfile::HostCodeMode);
        assert!(startup_brief_size(&host) <= STANDARD_STARTUP_HARD_MAX_BYTES);
        let mut direct_facts = first.clone();
        let mut host_facts = host.clone();
        direct_facts.as_object_mut().unwrap().remove("workflow");
        host_facts.as_object_mut().unwrap().remove("workflow");
        assert_eq!(
            direct_facts, host_facts,
            "HostCodeMode guidance must not change which startup facts fit the byte budget"
        );
        #[cfg(feature = "experimental-code-mode")]
        {
            let composed = build(CodingGuidanceProfile::CodeMode);
            assert_eq!(composed, build(CodingGuidanceProfile::CodeMode));
            assert!(startup_brief_size(&composed) <= STANDARD_STARTUP_HARD_MAX_BYTES);
            let mut direct_facts = first.clone();
            let mut composed_facts = composed.clone();
            direct_facts.as_object_mut().unwrap().remove("workflow");
            composed_facts.as_object_mut().unwrap().remove("workflow");
            assert_eq!(
                direct_facts, composed_facts,
                "profile must not change which startup facts fit the byte budget"
            );
            assert!(serde_json::to_vec(&json!({"success": true, "output": {"compact": true, "startup_brief": composed}, "error": Value::Null})).unwrap().len() < 32 * 1024);
        }
        assert_eq!(first, second);
        let bytes = startup_brief_size(&first);
        eprintln!("worst_case_standard_startup_bytes={bytes}");
        assert!(bytes <= STANDARD_STARTUP_HARD_MAX_BYTES, "{bytes}");
        let action_wrapped_bytes = serde_json::to_vec(&json!({
            "success": true,
            "output": {
                "compact": true,
                "startup_brief": first.clone(),
            },
            "error": Value::Null,
        }))
        .unwrap()
        .len();
        eprintln!("worst_case_action_wrapped_startup_bytes={action_wrapped_bytes}");
        assert!(action_wrapped_bytes < 32 * 1024, "{action_wrapped_bytes}");
        assert_eq!(
            instruction_source_paths(&first),
            INSTRUCTION_CANDIDATE_PATHS
                .iter()
                .map(|path| (*path).to_string())
                .collect()
        );
        assert_eq!(first["instructions"]["content_included"], true);
        assert_eq!(first["instructions"]["truncated"], true);
        for source in first["instructions"]["sources"].as_array().unwrap() {
            assert!(
                source["content"]
                    .as_str()
                    .is_some_and(|content| !content.is_empty()),
                "each loaded rule source must retain a usable bounded excerpt: {source}"
            );
            assert!(source["read_more"].is_object());
        }
        assert_eq!(first["continuation"]["changed_paths"]["total"], 120);
        assert!(
            first["continuation"]["changed_paths"]["returned"]
                .as_u64()
                .unwrap()
                <= 20
        );
        assert_eq!(first["continuation"]["changed_paths"]["truncated"], true);
        assert_eq!(first["continuation"]["exploration"]["paths"]["total"], 140);
        assert!(
            first["continuation"]["exploration"]["paths"]["returned"]
                .as_u64()
                .unwrap()
                <= MAX_STANDARD_EXPLORATION_PATHS as u64
        );
        assert_eq!(
            first["continuation"]["exploration"]["paths"]["truncated"],
            true
        );
        for pointer in [
            "/continuation/validation/open_failures",
            "/continuation/validation/delta/new_failures",
            "/continuation/validation/delta/resolved_failures",
            "/continuation/validation/delta/still_failing",
        ] {
            let list = first.pointer(pointer).unwrap();
            assert!(list["returned"].as_u64().unwrap() <= 10, "{pointer}");
            assert_eq!(list["truncated"], true, "{pointer}");
        }
        assert!(
            first["continuation"]["suggested_next_actions"]["returned"]
                .as_u64()
                .unwrap()
                <= 5
        );
        assert!(first["continuation"]["suggested_next_actions"]["items"][0]
            .as_str()
            .unwrap()
            .starts_with("rerun focused target 0"));
        assert!(first["startup_verdict"]["suggested_next_actions"][0]
            .as_str()
            .unwrap()
            .starts_with("rerun focused target 0"));
        assert!(first["startup_verdict"]["suggested_next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .any(|action| action.starts_with("inspect or await blocking active jobs")));
        assert!(first["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "active_jobs_blocking"));
        assert!(first["startup_verdict"]["suggested_next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .any(|action| action.starts_with("rerun focused target 0")));
    }
}

#[cfg(test)]
#[path = "tests/startup_instructions_projection.rs"]
mod instruction_tests;
