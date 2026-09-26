use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::auth::AuthContext;
use crate::client_window::ClientWindow;
use crate::json_digest::update_sha256_with_json;

use super::handoff::review_evidence_summary_for_session;
use super::session_context::{
    session_project_mismatch_result, unknown_session_result, SessionProjectMismatch,
};
use super::validation_events::{
    current_validation_evidence_for_session, validation_summary_from_events,
};
use super::{ToolResult, ToolRuntime};
use webcodex_workflow_session::SessionSummary;

const WORK_RESULT_SESSION_EVENT_LIMIT: usize = 200;
const WORK_RESULT_VALIDATION_LIMIT: usize = 20;
const WORK_RESULT_ACTIVITY_LIMIT: usize = 24;
const WORK_RESULT_WINDOW_ACTIVITY_LIMIT: usize = 200;
pub(crate) const MAX_WORK_RESULT_FILES: usize = 8;
const MAX_WORK_RESULT_PATH_CHARS: usize = 512;
const MAX_WORK_RESULT_BRANCH_CHARS: usize = 160;
const MAX_WORK_RESULT_REVIEW_TOOLS: usize = 12;
const MAX_WORK_RESULT_TOOL_CHARS: usize = 64;
const MAX_WORK_RESULT_MESSAGES: usize = 6;

impl ToolRuntime {
    #[cfg(test)]
    pub(crate) async fn present_work_result(
        &self,
        project: String,
        session_id: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.present_work_result_for_window(project, Some(session_id), auth, None)
            .await
    }

    pub(crate) async fn present_work_result_for_window(
        &self,
        project: String,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
    ) -> ToolResult {
        self.exact_work_result(project, session_id, "present_work_result", auth, window)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn work_result_state(
        &self,
        project: String,
        session_id: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.work_result_state_for_window(project, Some(session_id), auth, None)
            .await
    }

    pub(crate) async fn work_result_state_for_window(
        &self,
        project: String,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
    ) -> ToolResult {
        self.exact_work_result(project, session_id, "work_result_state", auth, window)
            .await
    }

    pub(crate) async fn work_result_send_message(
        &self,
        project: String,
        session_id: Option<String>,
        message: String,
        delivery_key: String,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
    ) -> ToolResult {
        let project = match self.authorize_work_result_project(&project, auth).await {
            Ok(project) => project,
            Err(result) => return result,
        };
        let Some(window) = window else {
            return ToolResult::err("stable Window identity required");
        };
        self.post_window_operator_message(
            window.key(),
            session_id.as_deref(),
            Some(&project),
            message,
            delivery_key,
            auth,
        )
        .await
    }

    async fn authorize_work_result_project(
        &self,
        project: &str,
        auth: Option<&AuthContext>,
    ) -> Result<String, ToolResult> {
        let resolved = self
            .resolve_project_input_for_auth(project, auth)
            .await
            .map_err(|error| error.into_tool_result())?;
        if project.trim() != resolved.resolved_id {
            return Err(ToolResult::err_with_output(
                "Work Result requires the exact complete runtime project id",
                json!({
                    "error_kind": "work_result_project_not_exact",
                    "failure_kind": "invalid_arguments",
                    "state_changed": false,
                }),
            ));
        }
        Ok(resolved.resolved_id)
    }

    async fn authorize_work_result_target(
        &self,
        project: &str,
        session_id: &str,
        tool_name: &'static str,
        auth: Option<&AuthContext>,
    ) -> Result<(String, SessionSummary), ToolResult> {
        if let Err(result) = self
            .authorize_session_target(session_id, tool_name, auth)
            .await
        {
            return Err(result);
        }
        let resolved_project = self.authorize_work_result_project(project, auth).await?;
        let Some(summary) = self
            .sessions
            .summary(session_id, Some(WORK_RESULT_SESSION_EVENT_LIMIT))
        else {
            return Err(unknown_session_result(session_id));
        };
        if summary.project.as_deref() != Some(resolved_project.as_str()) {
            let mismatch = SessionProjectMismatch {
                session_project: summary
                    .project
                    .clone()
                    .unwrap_or_else(|| "<unscoped>".to_string()),
                request_project: resolved_project.clone(),
            };
            return Err(session_project_mismatch_result(
                session_id, tool_name, &mismatch,
            ));
        }
        Ok((resolved_project, summary))
    }

    /// The persistent card is Window-first. Project authorization is mandatory;
    /// Workflow Session evidence is optional and may appear later in the same Window.
    async fn exact_work_result(
        &self,
        project: String,
        session_id: Option<String>,
        tool_name: &'static str,
        auth: Option<&AuthContext>,
        window: Option<&ClientWindow>,
    ) -> ToolResult {
        let resolved_project = match self.authorize_work_result_project(&project, auth).await {
            Ok(project) => project,
            Err(result) => return result,
        };

        // Read the same Window ActionAudit truth used by Runtime WebUI. The App's own
        // hidden refresh tools are excluded from Window correlation at MCP ingress.
        let observed = match window {
            Some(window) => {
                self.current_window_activity(
                    Some(window),
                    auth,
                    Some(WORK_RESULT_WINDOW_ACTIVITY_LIMIT),
                    true,
                )
                .await
                .output
            }
            None => json!({"status":"unavailable","reason_code":"window_identity_unavailable"}),
        };

        let summary = if let Some(session_id) = session_id.as_deref() {
            match self
                .authorize_work_result_target(&resolved_project, session_id, tool_name, auth)
                .await
            {
                Ok((_, summary)) => Some(summary),
                Err(result) => return result,
            }
        } else if let Some(discovered) = work_result_linked_session_id(&observed, &resolved_project)
        {
            self.authorize_work_result_target(&resolved_project, &discovered, tool_name, auth)
                .await
                .ok()
                .map(|(_, summary)| summary)
        } else {
            None
        };

        // Keep legacy compact workspace/check/review fields when a Session is linked so
        // already-cached v6 cards remain readable. v7 does not use them as navigation.
        let workspace_result = self
            .show_changes_for_presentation(resolved_project.clone())
            .await;
        let mut projection = if let Some(summary) = summary.as_ref() {
            let projection_summary = self.refresh_validation_source_summary(summary);
            let validation = validation_summary_from_events(
                &projection_summary.events,
                WORK_RESULT_VALIDATION_LIMIT,
            );
            let current_validation = current_validation_evidence_for_session(
                &projection_summary,
                WORK_RESULT_VALIDATION_LIMIT,
            )
            .evidence;
            let review = review_evidence_summary_for_session(&projection_summary);
            let mut projection = build_work_result_projection(
                &resolved_project,
                &summary.session_id,
                workspace_result.success,
                &workspace_result.output,
                &validation,
                &current_validation,
                &review,
                summary.events_truncated,
            );
            projection["session"] = work_result_session(summary);
            projection["session_id"] = json!(summary.session_id);
            if let Some(detail) = self.workflow_session_console_detail(
                &resolved_project,
                &summary.session_id,
                Some(WORK_RESULT_ACTIVITY_LIMIT),
            ) {
                projection["workflow"] = json!({
                    "activity": detail.activity.iter().map(|item| {
                        json!({
                            "label": match item.kind.as_str() {
                                "Read" => "Read project files",
                                "Searched" => "Searched the project",
                                "Navigated" | "Explored" => "Explored the project",
                                "Edited" => "Edited code",
                                "Tested" => "Ran checks",
                                "Reviewed" => "Reviewed changes",
                                "Ran" => "Ran a command",
                                _ => "Task activity",
                            },
                            "stage": match item.kind.as_str() {
                                "Read" | "Searched" | "Navigated" | "Explored" => "explore",
                                "Edited" => "edit",
                                "Tested" => "check",
                                "Reviewed" => "review",
                                "Ran" => "run",
                                _ => "other",
                            },
                            "state": item.state,
                            "started_at": item.started_at,
                            "finished_at": item.finished_at,
                            "duration_ms": item.duration_ms,
                            "count": item.group_count.unwrap_or(1),
                        })
                    }).collect::<Vec<_>>(),
                    "history_partial": detail.activity_truncated || summary.retention_truncated,
                });
            }
            projection
        } else {
            json!({
                "version": 2,
                "project": resolved_project,
                "workspace": work_result_workspace(
                    workspace_result.success,
                    &workspace_result.output,
                ),
                "validation": empty_work_result_validation(),
                "review": empty_work_result_review(),
                "collaboration": {
                    "available": false,
                    "can_send": false,
                    "messages": [],
                },
            })
        };

        projection["collaboration"] = self.window_collaboration(
            window.map(ClientWindow::key),
            auth,
            MAX_WORK_RESULT_MESSAGES,
        );
        projection["window_activity"] = work_result_window_activity_projection(&observed);
        projection["activity"] = work_result_activity_projection(&observed, summary.as_ref());
        projection["state_version"] = json!(work_result_state_version(&projection));

        if let Some(summary) = summary.as_ref() {
            match self.sealed_work_result_changes(&resolved_project, summary, auth) {
                Ok(Some(changes)) => projection["final_changes"] = changes,
                Ok(None) => {}
                Err(result) => return result,
            }
        }
        ToolResult::ok(json!({"work_result": projection}))
    }
}

fn work_result_session(summary: &webcodex_workflow_session::SessionSummary) -> Value {
    let latest = summary.events.iter().rev().find_map(|event| {
        if !webcodex_tool_contracts::runtime_tool_activity_interaction(&event.tool_name)
            .is_meaningful()
        {
            return None;
        }
        let tool = bounded_token(&event.tool_name, MAX_WORK_RESULT_TOOL_CHARS)?;
        let kind = bounded_token(&event.kind, 64)?;
        let mut value = Map::new();
        value.insert("tool".to_string(), json!(tool));
        value.insert("kind".to_string(), json!(kind));
        value.insert("timestamp".to_string(), json!(event.timestamp));
        if let Some(status) = event
            .status
            .as_deref()
            .and_then(|value| bounded_token(value, 32))
        {
            value.insert("status".to_string(), json!(status));
        }
        if let Some(duration_ms) = event.duration_ms {
            value.insert("duration_ms".to_string(), json!(duration_ms));
        }
        Some(Value::Object(value))
    });
    let mut session = Map::new();
    session.insert("lifecycle".to_string(), json!(&summary.lifecycle));
    session.insert("events_total".to_string(), json!(summary.events_total));
    session.insert(
        "events_returned".to_string(),
        json!(summary.events_returned),
    );
    session.insert(
        "history_partial".to_string(),
        json!(summary.events_truncated),
    );
    session.insert("updated_at".to_string(), json!(summary.updated_at));
    if let Some(title) = summary
        .title
        .as_deref()
        .and_then(|value| bounded_plain_text(value, 160))
    {
        session.insert("title".to_string(), json!(title));
    }
    if let Some(latest) = latest {
        session.insert("latest_activity".to_string(), latest);
    }
    Value::Object(session)
}

pub(crate) fn work_result_state_version(projection: &Value) -> String {
    let mut hasher = Sha256::new();
    if update_sha256_with_json(&mut hasher, projection).is_err() {
        // Preserve the historical `to_vec(...).unwrap_or_default()` fallback:
        // serialization failure hashes an empty byte sequence, never a partial one.
        hasher = Sha256::new();
    }
    format!("wr2_{:x}", hasher.finalize())
}

pub(crate) fn build_work_result_projection(
    project: &str,
    session_id: &str,
    workspace_call_succeeded: bool,
    workspace_source: &Value,
    validation_source: &Value,
    current_validation_source: &Value,
    review_source: &Value,
    history_partial: bool,
) -> Value {
    json!({
        "version": 2,
        "project": project,
        "session_id": session_id,
        "workspace": work_result_workspace(workspace_call_succeeded, workspace_source),
        "validation": work_result_validation(validation_source, current_validation_source, history_partial),
        "review": work_result_review(review_source, history_partial),
    })
}

fn empty_work_result_validation() -> Value {
    json!({
        "status": "unknown",
        "latest_status": "unknown",
        "current_status": "unknown",
        "history_partial": false,
        "successes": 0,
        "failures": 0,
        "unresolved_failures": 0,
        "evidence_gaps": 0,
    })
}

fn empty_work_result_review() -> Value {
    json!({
        "available": false,
        "history_partial": false,
        "total": 0,
        "read_only_inspection_count": 0,
        "search_count": 0,
        "diff_review_count": 0,
        "workspace_review_count": 0,
        "hygiene_review_count": 0,
        "tools": [],
    })
}

fn work_result_linked_session_id(observed: &Value, project: &str) -> Option<String> {
    observed
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|event| {
            let at = event
                .get("ended_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            event
                .get("workflow_sessions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |link| {
                    let linked_project = link.get("project").and_then(Value::as_str);
                    if linked_project.is_some() && linked_project != Some(project) {
                        return None;
                    }
                    let session_id = link.get("workflow_session_id")?.as_str()?.to_string();
                    Some((at, session_id))
                })
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, session_id)| session_id)
}

fn work_result_observed_label(tool: &str, current: bool, meaningful: bool) -> &'static str {
    if meaningful {
        return semantic_activity_label(tool, current);
    }
    match tool {
        "observe_jobs" => "Observed job progress",
        "runtime_status" => "Observed Runtime status",
        "current_window_activity" => "Observed Window activity",
        "list_jobs" => "Observed Jobs",
        _ => "Observed WebCodex activity",
    }
}

fn work_result_window_activity_projection(observed: &Value) -> Value {
    if observed.get("status").and_then(Value::as_str) != Some("available") {
        return json!({
            "available": false,
            "active": false,
            "active_requests": [],
            "events": [],
            "events_returned": 0,
            "events_observed": 0,
            "truncated": false,
            "last_activity_at_ms": Value::Null,
        });
    }

    let active_requests = observed
        .get("active_requests")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|request| {
            let tool = request
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let started_at_ms = request.get("started_at_ms").and_then(Value::as_i64)?;
            let semantics = webcodex_tool_contracts::runtime_tool_activity_semantics(tool);
            Some(json!({
                "label": work_result_observed_label(
                    tool,
                    true,
                    semantics.interaction.is_meaningful(),
                ),
                "kind": semantics.kind.as_str(),
                "started_at_ms": started_at_ms,
            }))
        })
        .collect::<Vec<_>>();

    let events = observed
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|event| {
            let started_at_ms = event.get("started_at_ms").and_then(Value::as_i64)?;
            let ended_at_ms = event.get("ended_at_ms").and_then(Value::as_i64)?;
            let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
            let meaningful = event
                .get("meaningful")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let semantics = webcodex_tool_contracts::runtime_tool_activity_semantics(tool);
            Some(json!({
                "label": work_result_observed_label(tool, false, meaningful),
                "kind": semantics.kind.as_str(),
                "status": event.get("status").and_then(Value::as_str).unwrap_or("unknown"),
                "meaningful": meaningful,
                "started_at_ms": started_at_ms,
                "ended_at_ms": ended_at_ms,
                "duration_ms": event.get("duration_ms").and_then(Value::as_i64),
            }))
        })
        .collect::<Vec<_>>();

    let last_event = events
        .iter()
        .filter_map(|event| event.get("ended_at_ms").and_then(Value::as_i64))
        .max();
    let last_active = active_requests
        .iter()
        .filter_map(|request| request.get("started_at_ms").and_then(Value::as_i64))
        .max();
    let last_activity_at_ms = last_event.into_iter().chain(last_active).max();
    let events_observed = observed
        .pointer("/summary/events_scanned")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(events.len());

    json!({
        "available": true,
        "active": !active_requests.is_empty(),
        "active_requests": active_requests,
        "events_returned": events.len(),
        "events_observed": events_observed,
        "events": events,
        "truncated": observed.get("truncated").and_then(Value::as_bool).unwrap_or(false),
        "last_activity_at_ms": last_activity_at_ms,
    })
}

fn work_result_activity_projection(observed: &Value, summary: Option<&SessionSummary>) -> Value {
    if observed.get("status").and_then(Value::as_str) != Some("available") {
        return summary.map(session_activity_fallback).unwrap_or_else(|| {
            json!({
                "available": false,
                "scope": "window",
                "active": false,
                "current": Value::Null,
                "last": Value::Null,
                "last_activity_at_ms": Value::Null,
                "last_meaningful_activity_at_ms": Value::Null,
                "coverage_partial": false,
            })
        });
    }

    let current = observed
        .get("active_requests")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|request| {
            let tool = request
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let started = request.get("started_at_ms").and_then(Value::as_i64)?;
            let semantics = webcodex_tool_contracts::runtime_tool_activity_semantics(tool);
            Some((
                started,
                json!({
                    "label": work_result_observed_label(
                        tool,
                        true,
                        semantics.interaction.is_meaningful(),
                    ),
                    "kind": semantics.kind.as_str(),
                    "started_at_ms": started,
                }),
            ))
        })
        .max_by_key(|(started, _)| *started)
        .map(|(_, value)| value);

    let last = observed
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|event| {
            let at = event.get("ended_at_ms").and_then(Value::as_i64)?;
            let tool = event.get("tool_name").and_then(Value::as_str).unwrap_or("");
            let meaningful = event
                .get("meaningful")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some((
                at,
                json!({
                    "label": work_result_observed_label(tool, false, meaningful),
                    "kind": event.get("activity_kind").and_then(Value::as_str),
                    "at_ms": at,
                }),
            ))
        })
        .max_by_key(|(at, _)| *at);

    let last_active = current
        .as_ref()
        .and_then(|value| value.get("started_at_ms"))
        .and_then(Value::as_i64);
    let last_completed = last.as_ref().map(|(at, _)| *at);
    let last_activity_at_ms = last_active.into_iter().chain(last_completed).max();
    let last_value = last.map(|(_, value)| value);

    json!({
        "available": true,
        "scope": "window",
        "active": current.is_some(),
        "current": current,
        "last": last_value,
        "last_activity_at_ms": last_activity_at_ms,
        // Legacy v6 field name; use the same Window-wide timestamp so cached cards
        // no longer disagree with Runtime WebUI about the last observed activity.
        "last_meaningful_activity_at_ms": last_activity_at_ms,
        "coverage_partial": observed.get("truncated").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn semantic_activity_label(tool: &str, current: bool) -> &'static str {
    match webcodex_tool_contracts::runtime_tool_activity_semantics(tool)
        .kind
        .as_str()
    {
        Some("read") => {
            if current {
                "Reading project files"
            } else {
                "Read project files"
            }
        }
        Some("search") => {
            if current {
                "Searching the codebase"
            } else {
                "Searched the codebase"
            }
        }
        Some("navigate") => {
            if current {
                "Inspecting code"
            } else {
                "Inspected code"
            }
        }
        Some("edit") => {
            if current {
                "Editing code"
            } else {
                "Edited code"
            }
        }
        Some("run") => {
            if current {
                "Running a command"
            } else {
                "Ran a command"
            }
        }
        Some("test") => {
            if current {
                "Running checks"
            } else {
                "Ran checks"
            }
        }
        Some("review") => {
            if current {
                "Reviewing changes"
            } else {
                "Reviewed changes"
            }
        }
        _ => {
            if current {
                "Working"
            } else {
                "Work updated"
            }
        }
    }
}

fn session_activity_fallback(summary: &SessionSummary) -> Value {
    let latest = summary.events.iter().rev().find(|event| {
        webcodex_tool_contracts::runtime_tool_activity_interaction(&event.tool_name).is_meaningful()
    });
    let last_at = latest.map(|event| event.timestamp.saturating_mul(1000));
    let last = latest.map(|event| {
        let semantics = webcodex_tool_contracts::runtime_tool_activity_semantics(&event.tool_name);
        json!({
            "label": semantic_activity_label(&event.tool_name, false),
            "kind": semantics.kind.as_str(),
            "at_ms": event.timestamp.saturating_mul(1000),
        })
    });
    json!({
        "available": false,
        "scope": "session",
        "active": false,
        "current": Value::Null,
        "last": last,
        "last_meaningful_activity_at_ms": last_at,
        "coverage_partial": summary.events_truncated,
    })
}

fn work_result_workspace(call_succeeded: bool, source: &Value) -> Value {
    let git_available = call_succeeded
        && source
            .get("git_available")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let clean = git_available
        .then(|| source.get("clean").and_then(Value::as_bool))
        .flatten();
    let source_files = source
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let files_total = source
        .get("files_total")
        .and_then(Value::as_u64)
        .unwrap_or(source_files.len() as u64);
    let source_files_truncated = source
        .get("files_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || files_total > source_files.len() as u64;

    let mut files = Vec::new();
    let mut unsafe_file_omitted = false;
    for source_file in source_files.iter().take(MAX_WORK_RESULT_FILES) {
        match work_result_file(source_file) {
            Some(file) => files.push(file),
            None => unsafe_file_omitted = true,
        }
    }
    let projection_truncated =
        source_files_truncated || source_files.len() > MAX_WORK_RESULT_FILES || unsafe_file_omitted;

    let mut additions = 0_u64;
    let mut deletions = 0_u64;
    let mut line_stats_seen = 0_usize;
    let mut line_stats_missing = false;
    for source_file in source_files {
        match (
            source_file.get("additions").and_then(Value::as_u64),
            source_file.get("deletions").and_then(Value::as_u64),
        ) {
            (Some(add), Some(delete)) => {
                additions = additions.saturating_add(add);
                deletions = deletions.saturating_add(delete);
                line_stats_seen += 1;
            }
            _ => line_stats_missing = true,
        }
    }

    let mut workspace = Map::new();
    workspace.insert("git_available".to_string(), json!(git_available));
    if let Some(clean) = clean {
        workspace.insert("clean".to_string(), json!(clean));
    }
    if !call_succeeded {
        workspace.insert("reason_code".to_string(), json!("workspace_unavailable"));
    } else if source
        .get("non_git_project")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        workspace.insert("reason_code".to_string(), json!("non_git_project"));
    } else if !git_available {
        workspace.insert("reason_code".to_string(), json!("git_unavailable"));
    }
    if let Some(branch) = source
        .get("branch")
        .and_then(Value::as_str)
        .and_then(|value| bounded_plain_text(value, MAX_WORK_RESULT_BRANCH_CHARS))
    {
        workspace.insert("branch".to_string(), json!(branch));
    }
    if let Some(head) = work_result_head(source.get("head")) {
        workspace.insert("head".to_string(), head);
    }
    workspace.insert(
        "counts".to_string(),
        work_result_counts(source.get("counts")),
    );
    workspace.insert("files_total".to_string(), json!(files_total));
    workspace.insert("files".to_string(), Value::Array(files));
    workspace.insert("truncated".to_string(), json!(projection_truncated));

    if clean == Some(true) {
        workspace.insert("additions".to_string(), json!(0));
        workspace.insert("deletions".to_string(), json!(0));
        workspace.insert("line_stats_partial".to_string(), json!(false));
    } else if line_stats_seen > 0 {
        workspace.insert("additions".to_string(), json!(additions));
        workspace.insert("deletions".to_string(), json!(deletions));
        workspace.insert(
            "line_stats_partial".to_string(),
            json!(line_stats_missing || source_files_truncated),
        );
    } else if clean == Some(false) {
        workspace.insert("line_stats_partial".to_string(), json!(true));
    }

    Value::Object(workspace)
}

fn work_result_file(source: &Value) -> Option<Value> {
    let path = source
        .get("path")
        .and_then(Value::as_str)
        .and_then(safe_relative_path)?;
    let mut file = Map::new();
    file.insert("path".to_string(), json!(path));
    for field in ["status", "kind"] {
        if let Some(value) = source
            .get(field)
            .and_then(Value::as_str)
            .and_then(|value| bounded_token(value, 32))
        {
            file.insert(field.to_string(), json!(value));
        }
    }
    if let Some(old_path) = source
        .get("old_path")
        .and_then(Value::as_str)
        .and_then(safe_relative_path)
    {
        file.insert("old_path".to_string(), json!(old_path));
    }
    for field in ["staged", "unstaged"] {
        if let Some(value) = source.get(field).and_then(Value::as_bool) {
            file.insert(field.to_string(), json!(value));
        }
    }
    if let (Some(additions), Some(deletions)) = (
        source.get("additions").and_then(Value::as_u64),
        source.get("deletions").and_then(Value::as_u64),
    ) {
        file.insert("additions".to_string(), json!(additions));
        file.insert("deletions".to_string(), json!(deletions));
    }
    Some(Value::Object(file))
}

fn work_result_head(source: Option<&Value>) -> Option<Value> {
    let source = source?.as_object()?;
    let commit = source.get("commit")?.as_str()?;
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut head = Map::new();
    head.insert("commit".to_string(), json!(commit));
    if let Some(short) = source
        .get("short")
        .and_then(Value::as_str)
        .filter(|value| value.len() <= 16 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        head.insert("short".to_string(), json!(short));
    }
    Some(Value::Object(head))
}

fn work_result_counts(source: Option<&Value>) -> Value {
    let mut counts = Map::new();
    for field in [
        "modified",
        "added",
        "deleted",
        "renamed",
        "copied",
        "untracked",
        "conflicted",
        "staged",
        "unstaged",
    ] {
        counts.insert(
            field.to_string(),
            json!(source
                .and_then(|value| value.get(field))
                .and_then(Value::as_u64)
                .unwrap_or(0)),
        );
    }
    Value::Object(counts)
}

fn work_result_validation(historical: &Value, current: &Value, history_partial: bool) -> Value {
    let mut validation = Map::new();
    let historical_status = validation_status(historical.get("status").and_then(Value::as_str));
    let historical_latest =
        validation_latest_status(historical.get("latest_status").and_then(Value::as_str));
    validation.insert(
        "status".to_string(),
        json!(if history_partial && historical_status == "not_run" {
            "unknown"
        } else {
            historical_status
        }),
    );
    validation.insert(
        "latest_status".to_string(),
        json!(if history_partial && historical_latest == "not_run" {
            "unknown"
        } else {
            historical_latest
        }),
    );
    validation.insert("history_partial".to_string(), json!(history_partial));
    validation.insert(
        "current_status".to_string(),
        json!(current_validation_status(
            current.get("status").and_then(Value::as_str)
        )),
    );
    for field in ["successes", "failures"] {
        if let Some(value) = historical.get(field).and_then(Value::as_u64) {
            validation.insert(field.to_string(), json!(value));
        }
    }
    validation.insert(
        "unresolved_failures".to_string(),
        json!(current
            .get("unresolved_failure_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)),
    );
    validation.insert(
        "evidence_gaps".to_string(),
        json!(current
            .get("evidence_gap_event_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)),
    );
    if let Some(reason) = current
        .get("reason")
        .and_then(Value::as_str)
        .and_then(|value| bounded_token(value, 96))
    {
        validation.insert("reason".to_string(), json!(reason));
    }
    Value::Object(validation)
}

fn work_result_review(source: &Value, history_partial: bool) -> Value {
    let mut review = Map::new();
    review.insert("history_partial".to_string(), json!(history_partial));
    review.insert(
        "available".to_string(),
        json!(source
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    for field in [
        "total",
        "read_only_inspection_count",
        "search_count",
        "diff_review_count",
        "workspace_review_count",
        "hygiene_review_count",
    ] {
        review.insert(
            field.to_string(),
            json!(source.get(field).and_then(Value::as_u64).unwrap_or(0)),
        );
    }
    let tools = source
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|value| bounded_token(value, MAX_WORK_RESULT_TOOL_CHARS))
                .take(MAX_WORK_RESULT_REVIEW_TOOLS)
                .map(Value::String)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    review.insert("tools".to_string(), Value::Array(tools));
    Value::Object(review)
}

fn validation_status(status: Option<&str>) -> &'static str {
    match status {
        Some("not_run") => "not_run",
        Some("passed") => "passed",
        Some("failed") => "failed",
        Some("mixed") => "mixed",
        Some("expected") => "expected",
        Some("inconclusive") => "inconclusive",
        _ => "unknown",
    }
}

fn validation_latest_status(status: Option<&str>) -> &'static str {
    match status {
        Some("not_run") => "not_run",
        Some("passed") => "passed",
        Some("failed") => "failed",
        Some("expected") => "expected",
        Some("inconclusive") => "inconclusive",
        _ => "unknown",
    }
}

fn current_validation_status(status: Option<&str>) -> &'static str {
    match status {
        Some("not_run") => "not_run",
        Some("unproven") => "unproven",
        Some("failed") => "failed",
        Some("expected") => "expected",
        Some("inconclusive") => "inconclusive",
        Some("stale") => "stale",
        _ => "unknown",
    }
}

fn safe_relative_path(value: &str) -> Option<String> {
    if value.is_empty()
        || value.chars().count() > MAX_WORK_RESULT_PATH_CHARS
        || value.chars().any(char::is_control)
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.as_bytes().get(1) == Some(&b':')
        || value.split(['/', '\\']).any(|part| part == "..")
    {
        return None;
    }
    Some(value.to_string())
}

fn bounded_plain_text(value: &str, max_chars: usize) -> Option<String> {
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.to_string())
}

fn bounded_token(value: &str, max_chars: usize) -> Option<String> {
    if value.is_empty()
        || value.chars().count() > max_chars
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/'))
    {
        return None;
    }
    Some(value.to_string())
}
