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

use super::session_context::{
    session_project_mismatch_warning, SessionProjectMismatch, SESSION_PROJECT_MISMATCH_KIND,
};
use super::sessions::{SessionDiscussionCounts, SessionDiscussionSummary, SessionMessage};
use super::types::ToolResult;
use super::validation_events::validation_summary_for_session;
use super::ToolRuntime;

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
        limit: Option<usize>,
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

        let mut output = json!({
            "session_id": summary.session_id,
            "project": summary.project,
            "title": summary.title,
            "mode": summary.mode,
            "guards": summary.guards,
            "created_at": summary.created_at,
            "updated_at": summary.updated_at,
            "counts": counts,
            "open_todos": open_todos,
            "open_risks": open_risks,
            "open_questions": open_questions,
            "open_guidance": open_guidance,
            "recent_progress": recent_progress,
            "recent_decisions": recent_decisions,
            "recent_failed_tools": recent_failed_tools,
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
        output["suggested_next_actions"] =
            json!(handoff_suggested_next_actions(&output, failed_tool_calls));

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

/// Build a bounded list of suggested next actions based on the handoff state.
fn handoff_suggested_next_actions(output: &Value, failed_tool_calls: usize) -> Vec<String> {
    let mut actions = Vec::new();
    let push = |actions: &mut Vec<String>, action: &str| {
        if !actions.iter().any(|existing| existing == action) {
            actions.push(action.to_string());
        }
    };

    if failed_tool_calls > 0 {
        push(
            &mut actions,
            "review recent failed tool calls before proceeding",
        );
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
    if actions.is_empty() {
        push(
            &mut actions,
            "session is ready for handoff; proceed with the next task step",
        );
    }
    actions
}
