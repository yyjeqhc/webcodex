//! `session_handoff_summary` — read-only structured handoff for degraded or
//! contaminated execution context recovery (GPT long-task window routed to a
//! degraded/contaminated context, context pollution, or continuing in a fresh
//! window), multi-agent, and multi-window scenarios.
//!
//! Aggregates session info, message-board state, recent progress/decisions,
//! open todos/risks/questions/guidance, recent failed tool calls, and optional
//! workspace + checkpoint metadata. Never calls an LLM; never generates
//! natural-language summaries. Output is always bounded and never includes
//! full diffs, file contents, stdout/stderr bodies, validation commands,
//! secrets, tokens, or raw session input payloads.

use serde_json::{json, Value};

use super::permissions::permission_summary_from_events;
use super::session_context::{
    session_project_mismatch_warning, SessionProjectMismatch, SESSION_PROJECT_MISMATCH_KIND,
};
use super::sessions::{tool_failure_summary_from_events, SessionEvent, SessionSummary};
use super::sessions::{SessionDiscussionCounts, SessionDiscussionSummary, SessionMessage};
use super::tool_result::ToolResult;
use super::validation_events::validation_summary_for_session;
use super::ToolRuntime;
use crate::auth::AuthContext;

const DEFAULT_HANDOFF_LIMIT: usize = 20;
const MAX_HANDOFF_LIMIT: usize = 100;
const HANDOFF_VALIDATION_SESSION_EVENT_LIMIT: usize = 200;
const MAX_RECENT_FAILED_TOOLS: usize = 10;
const MAX_RECENT_PROGRESS: usize = 10;
const MAX_RECENT_DECISIONS: usize = 10;
const MAX_OPEN_ITEMS: usize = 20;
const MAX_RECENT_CHECKPOINTS: usize = 10;
const HANDOFF_MESSAGE_CHARS: usize = 240;

impl ToolRuntime {
    pub(crate) async fn session_handoff_summary(
        &self,
        session_id: String,
        project: Option<String>,
        include_workspace: Option<bool>,
        include_checkpoints: Option<bool>,
        include_validation: Option<bool>,
        summary_only: bool,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let limit = limit
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_HANDOFF_LIMIT)
            .min(MAX_HANDOFF_LIMIT);
        let include_workspace = include_workspace.unwrap_or(true);
        let include_checkpoints = include_checkpoints.unwrap_or(true);
        let include_validation = include_validation.unwrap_or(true);

        // --- session basic info + events ---
        let summary = match self.sessions.summary(&session_id, Some(limit)) {
            Some(summary) => summary,
            None => return super::unknown_session_result(&session_id),
        };

        // --- message board state ---
        let discussion = match self.sessions.discussion_summary(&session_id, Some(limit)) {
            Ok(discussion) => discussion,
            Err(_) => {
                // UnknownSession is already caught by summary() above; any
                // other error is treated as an empty board.
                SessionDiscussionSummary {
                    counts: SessionDiscussionCounts {
                        total: 0,
                        open: 0,
                        resolved: 0,
                        guidance: 0,
                        progress: 0,
                        risk: 0,
                        todo: 0,
                        question: 0,
                        decision: 0,
                    },
                    open_guidance: Vec::new(),
                    open_questions: Vec::new(),
                    open_risks: Vec::new(),
                    open_todos: Vec::new(),
                    recent_progress: Vec::new(),
                    recent_decisions: Vec::new(),
                }
            }
        };

        // --- recent failed tool calls (from finished events) ---
        let recent_failed_tools: Vec<Value> = summary
            .events
            .iter()
            .filter(|event| {
                event.kind == "tool_call_finished" && event.status.as_deref() == Some("failed")
            })
            .rev()
            .take(MAX_RECENT_FAILED_TOOLS)
            .map(|event| {
                json!({
                    "tool_name": event.tool_name,
                    "error_kind": event.error_kind,
                    "failure_kind": event.failure_kind,
                    "created_at": event.timestamp,
                    "write_like": event.write_like,
                    "job_like": event.shell_like,
                })
            })
            .collect();

        let failed_tool_calls = recent_failed_tools.len();
        let tool_failures =
            tool_failure_summary_from_events(&summary.events, MAX_RECENT_FAILED_TOOLS);
        let expected_failed_tool_calls = output_recent(&tool_failures, "recent_expected");
        let unexpected_failed_tool_calls = output_recent(&tool_failures, "recent_unexpected");
        let expectation_mismatches = output_recent(&tool_failures, "recent_mismatches");
        let unexpected_success_tool_calls =
            output_recent(&tool_failures, "recent_unexpected_successes");

        let open_todos = bound_messages(&discussion.open_todos, MAX_OPEN_ITEMS);
        let open_risks = bound_messages(&discussion.open_risks, MAX_OPEN_ITEMS);
        let open_questions = bound_messages(&discussion.open_questions, MAX_OPEN_ITEMS);
        let open_guidance = bound_messages(&discussion.open_guidance, MAX_OPEN_ITEMS);
        let recent_progress = bound_messages(&discussion.recent_progress, MAX_RECENT_PROGRESS);
        let recent_decisions = bound_messages(&discussion.recent_decisions, MAX_RECENT_DECISIONS);

        let counts = json!({
            "events": summary.events.len(),
            "failed_tool_calls": failed_tool_calls,
            "messages": discussion.counts.total,
            "open_todos": discussion.counts.todo,
            "open_risks": discussion.counts.risk,
            "open_questions": discussion.counts.question,
            "open_guidance": discussion.counts.guidance,
        });

        let session_project_mismatch = match (summary.project.as_ref(), project.as_ref()) {
            (Some(session_project), Some(request_project))
                if !request_project.trim().is_empty() && session_project != request_project =>
            {
                Some(SessionProjectMismatch {
                    session_project: session_project.clone(),
                    request_project: request_project.trim().to_string(),
                })
            }
            _ => None,
        };
        let mut warnings = Vec::new();
        if let Some(mismatch) = session_project_mismatch.as_ref() {
            warnings.push(session_project_mismatch_warning(mismatch, false));
        }
        let jobs_project = match project
            .as_deref()
            .map(str::trim)
            .filter(|project| !project.is_empty())
        {
            Some(project) => self
                .resolve_project_input_for_auth(project, auth)
                .await
                .map(|resolved| resolved.resolved_id)
                .unwrap_or_else(|_| project.to_string()),
            None => summary.project.clone().unwrap_or_default(),
        };
        let jobs_project = (!jobs_project.is_empty()).then_some(jobs_project);
        let jobs = self
            .active_jobs_summary(jobs_project.as_deref(), auth, 10)
            .await;
        if let Some(job_warnings) = jobs.get("warnings").and_then(Value::as_array) {
            warnings.extend(job_warnings.iter().cloned());
        }

        let mut output = json!({
            "session_id": summary.session_id,
            "project": summary.project,
            "title": summary.title,
            "mode": summary.mode,
            "guards": summary.guards,
            "created_at": summary.created_at,
            "updated_at": summary.updated_at,
            "counts": counts,
            "permissions": permission_summary_from_events(&summary.events, DEFAULT_HANDOFF_LIMIT),
            "open_todos": open_todos,
            "open_risks": open_risks,
            "open_questions": open_questions,
            "open_guidance": open_guidance,
            "recent_progress": recent_progress,
            "recent_decisions": recent_decisions,
            "recent_failed_tools": recent_failed_tools,
            "tool_failures": tool_failures,
            "expected_failed_tool_calls": expected_failed_tool_calls,
            "unexpected_failed_tool_calls": unexpected_failed_tool_calls,
            "expectation_mismatches": expectation_mismatches,
            "unexpected_success_tool_calls": unexpected_success_tool_calls,
            "review_evidence": review_evidence_summary_for_session(&summary),
            "jobs": jobs,
            "warnings": warnings,
        });
        if let Some(mismatch) = session_project_mismatch.as_ref() {
            output["warning_kind"] = json!(SESSION_PROJECT_MISMATCH_KIND);
            output["session_project"] = json!(mismatch.session_project);
            output["request_project"] = json!(mismatch.request_project);
            output["allow_cross_project_session_required"] = json!(true);
            output["allow_cross_project_session"] = json!(false);
        }

        // --- optional workspace summary ---
        let has_project = project
            .as_deref()
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false);
        if has_project && include_workspace {
            let project = project.clone().unwrap_or_default();
            let workspace = self.handoff_workspace_summary(&project).await;
            output["workspace"] = workspace;
        }

        // --- optional checkpoint candidates ---
        if has_project && include_checkpoints {
            let project = project.clone().unwrap_or_default();
            let checkpoints = self.handoff_checkpoint_summary(&project, limit).await;
            output["checkpoints"] = checkpoints;
        }

        // --- optional ledger-derived validation summary ---
        if include_validation {
            let validation_summary = self
                .sessions
                .summary(&session_id, Some(HANDOFF_VALIDATION_SESSION_EVENT_LIMIT))
                .map(|summary| validation_summary_for_session(&summary))
                .unwrap_or_else(|| validation_summary_for_session(&summary));
            output["validation"] = validation_summary;
        }

        // --- bounded suggested next actions ---
        output["suggested_next_actions"] = json!(handoff_suggested_next_actions(&output));

        if summary_only {
            return ToolResult::ok(compact_handoff_output(&output));
        }

        ToolResult::ok(output)
    }

    /// Build a bounded workspace summary reusing the read-only `show_changes`
    /// git inspection path. Returns only clean/branch/head/counts/warnings/
    /// suggested_next_actions — never hunks, full diffs, or file contents.
    async fn handoff_workspace_summary(&self, project: &str) -> Value {
        let show_result = self
            .show_changes(project.to_string(), None, Some(false), None, None, None)
            .await;
        if !show_result.success {
            // Non-git project or git failure: do not fail the whole handoff.
            // Surface a structured warning instead.
            let mut warnings: Vec<Value> = show_result
                .output
                .get("warnings")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            warnings.push(json!({
                "kind": "git_unavailable",
                "message": "git-backed workspace inspection unavailable; project may not be a git repository",
            }));
            return json!({
                "project": project,
                "git_available": false,
                "non_git_project": show_result.output.get("non_git_project").cloned().unwrap_or(json!(false)),
                "clean": true,
                "branch": null,
                "head": null,
                "changed_files_count": 0,
                "warnings": json!(warnings),
                "suggested_next_actions": [],
            });
        }
        let counts = show_result
            .output
            .get("counts")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let changed_files_count = counts
            .as_object()
            .and_then(|obj| {
                let modified = obj.get("modified").and_then(Value::as_u64).unwrap_or(0);
                let added = obj.get("added").and_then(Value::as_u64).unwrap_or(0);
                let deleted = obj.get("deleted").and_then(Value::as_u64).unwrap_or(0);
                let renamed = obj.get("renamed").and_then(Value::as_u64).unwrap_or(0);
                let copied = obj.get("copied").and_then(Value::as_u64).unwrap_or(0);
                let untracked = obj.get("untracked").and_then(Value::as_u64).unwrap_or(0);
                Some(modified + added + deleted + renamed + copied + untracked)
            })
            .unwrap_or(0);

        // Carry warnings from show_changes and add a handoff-specific warning
        // when git is unavailable so the receiver immediately sees the gap.
        let mut warnings: Vec<Value> = show_result
            .output
            .get("warnings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let git_available = show_result
            .output
            .get("git_available")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !git_available {
            warnings.push(json!({
                "kind": "git_unavailable",
                "message": "git-backed workspace inspection unavailable; project may not be a git repository",
            }));
        }

        json!({
            "project": project,
            "git_available": json!(git_available),
            "non_git_project": show_result.output.get("non_git_project").cloned().unwrap_or(json!(false)),
            "clean": show_result.output.get("clean").cloned().unwrap_or(json!(true)),
            "branch": show_result.output.get("branch").cloned().unwrap_or(Value::Null),
            "head": show_result.output.get("head").cloned().unwrap_or(Value::Null),
            "changed_files_count": changed_files_count,
            "warnings": json!(warnings),
            "suggested_next_actions": show_result.output.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
        })
    }

    /// Build a bounded checkpoint summary using the read-only
    /// `workspace_checkpoint_list` path. Returns the latest
    /// `last_known_good` checkpoint (preferring `validation_status == passed`)
    /// and a bounded recent list. Never returns validation.commands or diffs.
    async fn handoff_checkpoint_summary(&self, project: &str, limit: usize) -> Value {
        let list_result = self
            .workspace_checkpoint_list(project.to_string(), Some(limit))
            .await;
        if !list_result.success {
            return json!({
                "latest_last_known_good": Value::Null,
                "recent": [],
            });
        }
        let checkpoints = list_result.output["checkpoints"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        // Find the latest last_known_good, preferring validation_status == passed.
        let mut latest_lkg: Option<&Value> = None;
        for checkpoint in &checkpoints {
            let kind = checkpoint.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind != "last_known_good" {
                continue;
            }
            let candidate_status = checkpoint
                .get("validation_status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let is_passed = candidate_status == "passed";
            match &latest_lkg {
                None => {
                    latest_lkg = Some(checkpoint);
                }
                Some(current) => {
                    let current_status = current
                        .get("validation_status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let current_passed = current_status == "passed";
                    let candidate_time = checkpoint
                        .get("created_at")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let current_time = current
                        .get("created_at")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    // Prefer passed; among same pass status prefer newer.
                    if (is_passed && !current_passed)
                        || (is_passed == current_passed && candidate_time > current_time)
                    {
                        latest_lkg = Some(checkpoint);
                    }
                }
            }
        }

        let latest_lkg_value = latest_lkg
            .map(|checkpoint| {
                json!({
                    "checkpoint_id": checkpoint.get("checkpoint_id").cloned().unwrap_or(Value::Null),
                    "kind": checkpoint.get("kind").cloned().unwrap_or(Value::Null),
                    "labels": checkpoint.get("labels").cloned().unwrap_or_else(|| json!([])),
                    "validation_status": checkpoint.get("validation_status").cloned().unwrap_or(json!("unknown")),
                    "created_at": checkpoint.get("created_at").cloned().unwrap_or(Value::Null),
                    "title": checkpoint.get("title").cloned().unwrap_or(Value::Null),
                })
            })
            .unwrap_or(Value::Null);

        let recent: Vec<Value> = checkpoints
            .iter()
            .take(MAX_RECENT_CHECKPOINTS)
            .map(|checkpoint| {
                json!({
                    "checkpoint_id": checkpoint.get("checkpoint_id").cloned().unwrap_or(Value::Null),
                    "kind": checkpoint.get("kind").cloned().unwrap_or(Value::Null),
                    "validation_status": checkpoint.get("validation_status").cloned().unwrap_or(json!("unknown")),
                    "created_at": checkpoint.get("created_at").cloned().unwrap_or(Value::Null),
                    "title": checkpoint.get("title").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();

        json!({
            "latest_last_known_good": latest_lkg_value,
            "recent": recent,
        })
    }
}

/// Bound a list of session messages for handoff output: limit count and
/// truncate message bodies. Never includes raw full bodies.
fn bound_messages(messages: &[SessionMessage], max_items: usize) -> Vec<Value> {
    messages
        .iter()
        .take(max_items)
        .map(|message| {
            json!({
                "message_id": message.message_id,
                "created_at": message.created_at,
                "kind": message.kind,
                "status": message.status,
                "priority": message.priority,
                "message": bound_chars(&message.message, HANDOFF_MESSAGE_CHARS),
                "tags": message.tags,
                "resolved_at": message.resolved_at,
            })
        })
        .collect()
}

fn bound_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn output_recent(tool_failures: &Value, key: &str) -> Value {
    tool_failures.get(key).cloned().unwrap_or_else(|| json!([]))
}

fn compact_handoff_output(output: &Value) -> Value {
    let workspace_checked = output.get("workspace").is_some();
    let workspace_clean = output
        .get("workspace")
        .and_then(|workspace| workspace.get("clean"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut compact = json!({
        "summary_only": true,
        "project": output.get("project").cloned().unwrap_or(Value::Null),
        "session_id": output.get("session_id").cloned().unwrap_or(Value::Null),
        "workspace_clean": workspace_clean,
        "hygiene_clean": true,
        "jobs": compact_jobs(output.get("jobs").unwrap_or(&Value::Null)),
        "permissions": compact_permissions(output.get("permissions").unwrap_or(&Value::Null)),
        "tool_failures": compact_tool_failures(output.get("tool_failures").unwrap_or(&Value::Null)),
        "validation": compact_validation(output.get("validation").unwrap_or(&Value::Null)),
        "review_evidence": compact_review_evidence(output.get("review_evidence").unwrap_or(&Value::Null)),
        "warnings": output.get("warnings").cloned().unwrap_or_else(|| json!([])),
        "suggested_next_actions": output.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
    });
    compact["verdict"] = compact_workflow_verdict(&compact, workspace_checked, None);
    compact
}

pub(crate) fn compact_jobs(jobs: &Value) -> Value {
    json!({
        "active_count": jobs.get("active_count").and_then(Value::as_u64).unwrap_or(0),
        "blocking_active_count": jobs.get("blocking_active_count").and_then(Value::as_u64).unwrap_or(0),
        "nonblocking_active_count": jobs.get("nonblocking_active_count").and_then(Value::as_u64).unwrap_or(0),
        "terminal_pending_count": jobs.get("terminal_pending_count").and_then(Value::as_u64).unwrap_or(0),
        "warnings": jobs.get("warnings").cloned().unwrap_or_else(|| json!([])),
    })
}

pub(crate) fn compact_permissions(permissions: &Value) -> Value {
    json!({
        "required_count": permissions.get("required_count").and_then(Value::as_u64).unwrap_or(0),
        "manual_approved_count": permissions.get("manual_approved_count").and_then(Value::as_u64).unwrap_or(0),
        "auto_approved_count": permissions.get("auto_approved_count").and_then(Value::as_u64).unwrap_or(0),
        "total_approved_count": permissions.get("total_approved_count").and_then(Value::as_u64).unwrap_or(0),
        "hard_denied_count": permissions.get("hard_denied_count").and_then(Value::as_u64).unwrap_or(0),
    })
}

pub(crate) fn compact_tool_failures(tool_failures: &Value) -> Value {
    json!({
        "expected_count": tool_failures.get("expected_count").and_then(Value::as_u64).unwrap_or(0),
        "unexpected_count": tool_failures.get("unexpected_count").and_then(Value::as_u64).unwrap_or(0),
        "expectation_mismatch_count": tool_failures.get("expectation_mismatch_count").and_then(Value::as_u64).unwrap_or(0),
        "unexpected_success_count": tool_failures.get("unexpected_success_count").and_then(Value::as_u64).unwrap_or(0),
    })
}

pub(crate) fn compact_validation(validation: &Value) -> Value {
    json!({
        "status": validation.get("status").cloned().unwrap_or_else(|| json!("not_run")),
        "reason": validation.get("reason").cloned().unwrap_or_else(|| json!("no_validation_tool_invoked")),
        "latest_status": validation
            .get("latest_status")
            .cloned()
            .unwrap_or_else(|| compact_validation_latest_status_fallback(validation)),
        "historical_failures": validation
            .get("historical_failures")
            .cloned()
            .unwrap_or_else(compact_validation_historical_failures_fallback),
        "cargo_test_zero_tests_run": validation_has_cargo_test_zero_tests(validation),
    })
}

pub(crate) fn validation_has_cargo_test_zero_tests(validation: &Value) -> bool {
    validation
        .get("cargo_test_zero_tests_run")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn review_evidence_summary_for_session(summary: &SessionSummary) -> Value {
    review_evidence_summary_from_events(&summary.events)
}

fn review_evidence_summary_from_events(events: &[SessionEvent]) -> Value {
    let mut read_only_inspection_count = 0_u64;
    let mut search_count = 0_u64;
    let mut diff_review_count = 0_u64;
    let mut workspace_review_count = 0_u64;
    let mut hygiene_review_count = 0_u64;
    let mut total = 0_u64;
    let mut tools: Vec<String> = Vec::new();

    for event in events {
        if event.kind != "tool_call_finished" || event.status.as_deref() != Some("succeeded") {
            continue;
        }
        let Some(kind) = review_evidence_kind(event.tool_name.as_str()) else {
            continue;
        };
        match kind {
            ReviewEvidenceKind::ReadOnlyInspection => read_only_inspection_count += 1,
            ReviewEvidenceKind::Search => search_count += 1,
            ReviewEvidenceKind::DiffReview => diff_review_count += 1,
            ReviewEvidenceKind::WorkspaceReview => workspace_review_count += 1,
            ReviewEvidenceKind::HygieneReview => {
                workspace_review_count += 1;
                hygiene_review_count += 1;
            }
        }
        total += 1;
        push_unique_tool(&mut tools, &event.tool_name);
    }

    json!({
        "available": true,
        "source": "session_ledger",
        "read_only_inspection_count": read_only_inspection_count,
        "search_count": search_count,
        "diff_review_count": diff_review_count,
        "workspace_review_count": workspace_review_count,
        "hygiene_review_count": hygiene_review_count,
        "total": total,
        "tools": tools,
    })
}

pub(crate) fn compact_review_evidence(review_evidence: &Value) -> Value {
    let tools: Vec<Value> = review_evidence
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(Value::as_str)
                .take(20)
                .map(|tool| json!(tool))
                .collect()
        })
        .unwrap_or_default();

    json!({
        "available": review_evidence.get("available").and_then(Value::as_bool).unwrap_or(false),
        "total": review_evidence.get("total").and_then(Value::as_u64).unwrap_or(0),
        "read_only_inspection_count": review_evidence
            .get("read_only_inspection_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "search_count": review_evidence
            .get("search_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "diff_review_count": review_evidence
            .get("diff_review_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "workspace_review_count": review_evidence
            .get("workspace_review_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "hygiene_review_count": review_evidence
            .get("hygiene_review_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "tools": tools,
    })
}

#[derive(Debug, Clone, Copy)]
enum ReviewEvidenceKind {
    ReadOnlyInspection,
    Search,
    DiffReview,
    WorkspaceReview,
    HygieneReview,
}

fn review_evidence_kind(tool_name: &str) -> Option<ReviewEvidenceKind> {
    match tool_name {
        "read_file" | "list_project_files" => Some(ReviewEvidenceKind::ReadOnlyInspection),
        "search_project_text" => Some(ReviewEvidenceKind::Search),
        "git_diff" | "git_diff_summary" | "git_diff_hunks" => Some(ReviewEvidenceKind::DiffReview),
        "show_changes" | "git_status" => Some(ReviewEvidenceKind::WorkspaceReview),
        "workspace_hygiene_check" => Some(ReviewEvidenceKind::HygieneReview),
        _ => None,
    }
}

fn push_unique_tool(tools: &mut Vec<String>, tool_name: &str) {
    if !tools.iter().any(|tool| tool == tool_name) {
        tools.push(tool_name.to_string());
    }
}

fn compact_validation_latest_status_fallback(validation: &Value) -> Value {
    let latest_status = match validation.get("status").and_then(Value::as_str) {
        Some("passed") => "passed",
        Some("failed") => "failed",
        Some("not_run") => "not_run",
        _ => "unknown",
    };
    json!(latest_status)
}

fn compact_validation_historical_failures_fallback() -> Value {
    json!({
        "count": 0,
        "resolved": false,
        "unresolved": false,
    })
}

pub(crate) fn compact_workflow_verdict(
    output: &Value,
    workspace_checked: bool,
    hygiene_checked: Option<bool>,
) -> Value {
    let mut blocking_reasons: Vec<&'static str> = Vec::new();
    let mut warning_reasons: Vec<&'static str> = Vec::new();
    let mut actions = string_array(output.get("suggested_next_actions"));

    if !workspace_checked {
        push_unique(&mut warning_reasons, "workspace_not_checked");
        push_unique_action(&mut actions, "run show_changes before final handoff");
    }
    if output
        .get("workspace_clean")
        .and_then(Value::as_bool)
        .is_some_and(|clean| !clean)
    {
        push_unique(&mut blocking_reasons, "workspace_dirty");
        push_unique_action(&mut actions, "review workspace changes with show_changes");
    }

    if let Some(false) = hygiene_checked {
        push_unique(&mut warning_reasons, "hygiene_not_checked");
        push_unique_action(&mut actions, "run workspace_hygiene_check before closeout");
    }
    if output
        .get("hygiene_clean")
        .and_then(Value::as_bool)
        .is_some_and(|clean| !clean)
    {
        push_unique(&mut blocking_reasons, "hygiene_failed");
        push_unique_action(&mut actions, "review workspace hygiene before closeout");
    }

    let jobs = output.get("jobs").unwrap_or(&Value::Null);
    if jobs
        .get("blocking_active_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        push_unique(&mut blocking_reasons, "blocking_active_jobs");
        push_unique_action(&mut actions, "stop or await blocking active jobs");
    }
    if jobs
        .get("terminal_pending_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        push_unique(&mut warning_reasons, "jobs_terminal_pending");
    }

    let validation = output.get("validation").unwrap_or(&Value::Null);
    let tool_failures = output.get("tool_failures").unwrap_or(&Value::Null);
    let expected_count = count_field(tool_failures, "expected_count");
    let unexpected_count = count_field(tool_failures, "unexpected_count");
    let expectation_mismatch_count = count_field(tool_failures, "expectation_mismatch_count");
    let unexpected_success_count = count_field(tool_failures, "unexpected_success_count");
    let resolved_validation_like_unexpected_count =
        resolved_validation_like_unexpected_tool_failure_count(
            tool_failures,
            output,
            validation,
            workspace_checked,
            hygiene_checked,
        );
    let blocking_unexpected_count =
        unexpected_count.saturating_sub(resolved_validation_like_unexpected_count);
    if blocking_unexpected_count > 0 {
        push_unique(&mut blocking_reasons, "unexpected_tool_failures");
        push_unique_action(
            &mut actions,
            "review unexpected failed tool calls before proceeding",
        );
    } else if resolved_validation_like_unexpected_count > 0 {
        push_unique(
            &mut warning_reasons,
            "resolved_validation_like_tool_failures",
        );
    }
    if expectation_mismatch_count > 0 {
        push_unique(&mut blocking_reasons, "expectation_mismatches");
        push_unique_action(
            &mut actions,
            "review expected failure mismatches before proceeding",
        );
    }
    if unexpected_success_count > 0 {
        push_unique(&mut blocking_reasons, "unexpected_successes");
        push_unique_action(
            &mut actions,
            "review expected-failure assertions that unexpectedly succeeded",
        );
    }
    if expected_count > 0
        && unexpected_count == 0
        && expectation_mismatch_count == 0
        && unexpected_success_count == 0
    {
        push_unique(&mut warning_reasons, "expected_failures_matched");
        push_unique_action(&mut actions, "expected failure assertions matched");
    }

    match validation.get("status").and_then(Value::as_str) {
        Some("not_run") => {
            let review_evidence_total = output
                .get("review_evidence")
                .and_then(|review_evidence| review_evidence.get("total"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if review_evidence_total > 0 {
                push_unique(
                    &mut warning_reasons,
                    "validation_not_run_with_review_evidence",
                );
                push_unique_action(
                    &mut actions,
                    "no structured validation was run; review evidence is available for task-appropriate closeout",
                );
            } else {
                push_unique(&mut warning_reasons, "validation_not_run");
                push_unique_action(
                    &mut actions,
                    "run validation or review before closeout when applicable",
                );
            }
        }
        Some("failed") => {
            push_unique(&mut blocking_reasons, "validation_failed");
            push_unique_action(&mut actions, "review validation failures before closeout");
        }
        Some("mixed") => {
            if validation_historical_failures_resolved(validation) {
                push_unique(
                    &mut warning_reasons,
                    "validation_historical_failures_resolved",
                );
                push_unique_action(
                    &mut actions,
                    "historical validation failures were resolved by later successful validation",
                );
            } else {
                push_unique(&mut blocking_reasons, "validation_mixed");
                push_unique_action(
                    &mut actions,
                    "review mixed validation results before closeout",
                );
            }
        }
        Some("unknown") | None => {
            push_unique(&mut warning_reasons, "validation_unknown");
        }
        Some(_) => {}
    }
    if validation_has_cargo_test_zero_tests(validation) {
        push_unique(&mut warning_reasons, "cargo_test_zero_tests");
        push_unique_action(
            &mut actions,
            "cargo_test ran zero tests; verify the test filter or command",
        );
    }

    if actions.is_empty() {
        actions.push("proceed with handoff or closeout".to_string());
    }
    let status = if blocking_reasons.is_empty() {
        if warning_reasons.is_empty() {
            "pass"
        } else {
            "warn"
        }
    } else {
        "fail"
    };

    json!({
        "status": status,
        "blocking": !blocking_reasons.is_empty(),
        "blocking_reasons": blocking_reasons,
        "warning_reasons": warning_reasons,
        "suggested_next_actions": actions,
    })
}

fn validation_historical_failures_resolved(validation: &Value) -> bool {
    validation.get("latest_status").and_then(Value::as_str) == Some("passed")
        && validation
            .pointer("/historical_failures/resolved")
            .and_then(Value::as_bool)
            == Some(true)
        && validation
            .pointer("/historical_failures/unresolved")
            .and_then(Value::as_bool)
            == Some(false)
}

fn resolved_validation_like_unexpected_tool_failure_count(
    tool_failures: &Value,
    output: &Value,
    validation: &Value,
    workspace_checked: bool,
    hygiene_checked: Option<bool>,
) -> u64 {
    if !workspace_checked
        || hygiene_checked != Some(true)
        || output.get("workspace_clean").and_then(Value::as_bool) != Some(true)
        || output.get("hygiene_clean").and_then(Value::as_bool) != Some(true)
        || !validation_historical_failures_resolved(validation)
    {
        return 0;
    }

    tool_failures
        .get("recent_unexpected")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|failure| {
            failure
                .get("tool_name")
                .and_then(Value::as_str)
                .is_some_and(is_validation_like_closeout_tool)
        })
        .count() as u64
}

fn is_validation_like_closeout_tool(tool_name: &str) -> bool {
    matches!(tool_name, "cargo_fmt" | "cargo_check" | "cargo_test")
}

fn count_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn push_unique<T>(values: &mut Vec<T>, value: T)
where
    T: PartialEq,
{
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn push_unique_action(actions: &mut Vec<String>, action: &str) {
    if !actions.iter().any(|existing| existing == action) {
        actions.push(action.to_string());
    }
}

/// Build a bounded list of suggested next actions based on the handoff state.
fn handoff_suggested_next_actions(output: &Value) -> Vec<String> {
    let mut actions = Vec::new();
    let push = |actions: &mut Vec<String>, action: &str| {
        if !actions.iter().any(|existing| existing == action) {
            actions.push(action.to_string());
        }
    };

    let tool_failures = output.get("tool_failures").unwrap_or(&Value::Null);
    let unexpected_count = tool_failures
        .get("unexpected_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let expectation_mismatch_count = tool_failures
        .get("expectation_mismatch_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unexpected_success_count = tool_failures
        .get("unexpected_success_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let expected_count = tool_failures
        .get("expected_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if unexpected_count > 0 {
        push(
            &mut actions,
            "review unexpected failed tool calls before proceeding",
        );
    }
    if expectation_mismatch_count > 0 {
        push(
            &mut actions,
            "review expected failure mismatches before proceeding",
        );
    }
    if unexpected_success_count > 0 {
        push(
            &mut actions,
            "review expected-failure assertions that unexpectedly succeeded",
        );
    }
    if expected_count > 0
        && unexpected_count == 0
        && expectation_mismatch_count == 0
        && unexpected_success_count == 0
    {
        push(&mut actions, "expected failure assertions matched");
    }
    let open_todos = output["counts"]["open_todos"].as_u64().unwrap_or(0);
    if open_todos > 0 {
        push(&mut actions, "address open todos");
    }
    let open_risks = output["counts"]["open_risks"].as_u64().unwrap_or(0);
    if open_risks > 0 {
        push(&mut actions, "mitigate open risks");
    }
    let open_questions = output["counts"]["open_questions"].as_u64().unwrap_or(0);
    if open_questions > 0 {
        push(&mut actions, "answer open questions");
    }
    if let Some(workspace) = output.get("workspace") {
        if workspace.get("git_available").and_then(Value::as_bool) == Some(true) {
            let clean = workspace
                .get("clean")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if !clean {
                push(&mut actions, "review workspace changes with show_changes");
            }
        }
    }
    if let Some(checkpoints) = output.get("checkpoints") {
        let lkg_is_null = checkpoints
            .get("latest_last_known_good")
            .map_or(true, Value::is_null);
        if lkg_is_null {
            push(
                &mut actions,
                "consider creating a last_known_good checkpoint",
            );
        }
    }
    if validation_has_cargo_test_zero_tests(output.get("validation").unwrap_or(&Value::Null)) {
        push(
            &mut actions,
            "cargo_test ran zero tests; verify the test filter or command",
        );
    }
    if actions.is_empty() {
        push(
            &mut actions,
            "session is ready for handoff; proceed with the next task step",
        );
    }
    actions
}
