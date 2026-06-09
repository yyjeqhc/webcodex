use crate::auth::get_db;
use crate::json_error;
use crate::{ActionEventRecord, ActionSessionRecord, Database};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const ACTION_SESSION_IDLE_TIMEOUT_SECS: i64 = 1800;
const MAX_SUMMARY_TEXT: usize = 500;
const MAX_PREVIEW_TEXT: usize = 120;
pub const ACTION_SESSION_GUIDANCE: &str =
    "Audited in rolling action sessions. Optional X-Action-Session-Id groups related calls.";
#[cfg(test)]
pub const AUDITED_ACTION_ROUTES: &[&str] = &[
    "/api/codex/projects",
    "/api/codex/context_batch",
    "/api/codex/edit",
    "/api/codex/artifact",
    "/api/codex/git",
    "/api/codex/command",
    "/api/codex/command_request_op",
    "/api/codex/job",
    "/api/codex/check",
    "/api/codex/report",
    "/api/desktop/task_op",
];

#[derive(Debug, Clone, Serialize)]
pub struct ActionSessionStats {
    pub by_endpoint: BTreeMap<String, i64>,
    pub by_project: BTreeMap<String, i64>,
    pub by_status: BTreeMap<String, i64>,
    pub edit_count: i64,
    pub context_count: i64,
    pub job_count: i64,
    pub command_count: i64,
    pub report_count: i64,
    pub artifact_count: i64,
    pub git_count: i64,
    pub desktop_count: i64,
    pub changed_files_distinct_count: usize,
    pub job_ids_distinct_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionSessionListItem {
    pub session: ActionSessionRecord,
    pub stats: ActionSessionStats,
    pub top_endpoints: Vec<String>,
    pub top_projects: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionEventView {
    pub event_id: String,
    pub session_id: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub endpoint: String,
    pub operation: Option<String>,
    pub action_name: String,
    pub project: Option<String>,
    pub status: String,
    pub http_status: Option<i64>,
    pub error_summary: Option<String>,
    pub warning_summary: Option<String>,
    pub changed_files: Vec<String>,
    pub ids: Value,
    pub summary: Value,
    pub request_bytes: Option<i64>,
    pub response_bytes: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ActionSessionOpRequest {
    pub op: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ActionSessionOpResponse {
    pub success: bool,
    pub op: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<ActionSessionListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<ActionSessionRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ActionSessionStats>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ActionEventView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActionAuditEventInput {
    pub explicit_session_id: Option<String>,
    pub session_title: Option<String>,
    pub endpoint: String,
    pub action_name: String,
    pub operation: Option<String>,
    pub project: Option<String>,
    pub status: String,
    pub http_status: Option<i64>,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
    pub error_summary: Option<String>,
    pub warning_summary: Option<String>,
    pub changed_files: Vec<String>,
    pub ids: Value,
    pub summary: Value,
    pub request_bytes: Option<i64>,
    pub response_bytes: Option<i64>,
}

pub fn trim_and_truncate(value: &str, max_len: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_len).collect::<String>()
}

pub fn secret_like_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "token",
        "api_key",
        "apikey",
        "authorization",
        "secret",
        "password",
        "ssh_key",
        "private_key",
        "pem",
        "id_rsa",
        "id_ed25519",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub fn secret_like_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("-----begin")
        || lower.contains("bearer ")
        || lower.contains("api_key")
        || lower.contains("token")
        || lower.contains("id_rsa")
        || lower.contains("id_ed25519")
}

pub fn redact_text(value: &str) -> String {
    let trimmed = trim_and_truncate(value, MAX_SUMMARY_TEXT);
    if secret_like_value(&trimmed) {
        return "[redacted]".to_string();
    }
    trimmed
}

pub fn sanitize_error_summary(value: Option<&str>) -> Option<String> {
    value
        .map(redact_text)
        .filter(|text| !text.is_empty())
        .map(|text| trim_and_truncate(&text, MAX_SUMMARY_TEXT))
}

pub fn summarize_command_text(kind: &str, text: &str) -> Value {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let first_line = text.lines().next().unwrap_or_default();
    json!({
        "command_kind": kind,
        "line_count": text.lines().count(),
        "char_count": text.chars().count(),
        "sha256": hash,
        "first_line_preview": trim_and_truncate(&redact_text(first_line), MAX_PREVIEW_TEXT),
    })
}

pub fn sanitize_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                if secret_like_key(key) {
                    out.insert(key.clone(), Value::String("[redacted]".to_string()));
                    continue;
                }
                if key == "stdout_tail"
                    || key == "stderr_tail"
                    || key == "stdout"
                    || key == "stderr"
                    || key == "diff"
                    || key == "openapi_json"
                    || key == "text"
                    || key == "base64_content"
                {
                    continue;
                }
                if key == "script_text" || key == "command_text" {
                    if let Some(text) = val.as_str() {
                        out.insert(key.clone(), summarize_command_text(key, text));
                    } else {
                        out.insert(key.clone(), sanitize_value(val));
                    }
                    continue;
                }
                out.insert(key.clone(), sanitize_value(val));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sanitize_value).collect()),
        Value::String(text) => Value::String(redact_text(text)),
        _ => value.clone(),
    }
}

pub fn get_or_create_active_session(
    db: &Database,
    explicit_session_id: Option<&str>,
    title: Option<&str>,
    now: i64,
) -> anyhow::Result<ActionSessionRecord> {
    if let Some(session_id) = explicit_session_id.filter(|id| !id.trim().is_empty()) {
        if let Some(existing) = db.get_action_session(session_id)? {
            return Ok(existing);
        }
        let record = ActionSessionRecord {
            session_id: session_id.to_string(),
            title: title
                .map(|v| trim_and_truncate(v, 120))
                .filter(|v| !v.is_empty()),
            note: None,
            status: "open".to_string(),
            created_at: now,
            updated_at: now,
            closed_at: None,
            first_event_at: None,
            last_event_at: None,
            total_actions: 0,
            success_count: 0,
            failed_count: 0,
            timeout_or_unknown_count: 0,
            warning_count: 0,
            total_duration_ms: 0,
            changed_files_count: 0,
            job_ids_count: 0,
        };
        db.insert_action_session(&record)?;
        return Ok(record);
    }
    if let Some(existing) =
        db.find_recent_open_action_session(now - ACTION_SESSION_IDLE_TIMEOUT_SECS)?
    {
        return Ok(existing);
    }
    let record = ActionSessionRecord {
        session_id: uuid::Uuid::new_v4().to_string(),
        title: title
            .map(|v| trim_and_truncate(v, 120))
            .filter(|v| !v.is_empty()),
        note: None,
        status: "open".to_string(),
        created_at: now,
        updated_at: now,
        closed_at: None,
        first_event_at: None,
        last_event_at: None,
        total_actions: 0,
        success_count: 0,
        failed_count: 0,
        timeout_or_unknown_count: 0,
        warning_count: 0,
        total_duration_ms: 0,
        changed_files_count: 0,
        job_ids_count: 0,
    };
    db.insert_action_session(&record)?;
    Ok(record)
}

pub fn request_action_session_id(req: &Request) -> Option<String> {
    req.headers()
        .get("X-Action-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
pub fn is_audited_action_route(path: &str) -> bool {
    AUDITED_ACTION_ROUTES.contains(&path)
}

pub fn record_action_event(db: &Arc<Database>, input: ActionAuditEventInput) {
    let session = match get_or_create_active_session(
        db,
        input.explicit_session_id.as_deref(),
        input.session_title.as_deref(),
        input.started_at,
    ) {
        Ok(session) => session,
        Err(err) => {
            tracing::warn!("action session lookup failed: {}", err);
            return;
        }
    };
    let warning_count = input
        .warning_summary
        .as_ref()
        .map(|summary| summary.split(" | ").count() as i64)
        .unwrap_or(0);
    let ids_json = sanitize_value(&input.ids);
    let summary_json = sanitize_value(&input.summary);
    let event = ActionEventRecord {
        event_id: uuid::Uuid::new_v4().to_string(),
        session_id: session.session_id.clone(),
        started_at: input.started_at,
        ended_at: input.ended_at,
        duration_ms: input.duration_ms,
        endpoint: input.endpoint,
        operation: input.operation,
        action_name: input.action_name,
        project: input.project,
        status: input.status.clone(),
        http_status: input.http_status,
        error_summary: sanitize_error_summary(input.error_summary.as_deref()),
        warning_summary: sanitize_error_summary(input.warning_summary.as_deref()),
        changed_files_json: serde_json::to_string(&input.changed_files)
            .unwrap_or_else(|_| "[]".to_string()),
        ids_json: serde_json::to_string(&ids_json).unwrap_or_else(|_| "{}".to_string()),
        summary_json: serde_json::to_string(&summary_json).unwrap_or_else(|_| "{}".to_string()),
        request_bytes: input.request_bytes,
        response_bytes: input.response_bytes,
    };
    let success_inc = if input.status == "success" { 1 } else { 0 };
    let failed_inc = if input.status == "failed" || input.status == "rejected" {
        1
    } else {
        0
    };
    let timeout_inc = if input.status == "timeout" || input.status == "unknown" {
        1
    } else {
        0
    };
    let changed_files_count = input.changed_files.len() as i64;
    let job_ids_count = extract_string_list(&ids_json, "job_ids").len() as i64
        + extract_string(&ids_json, "job_id").map(|_| 1).unwrap_or(0);
    if let Err(err) = db.append_action_event_and_update_session(
        &event,
        success_inc,
        failed_inc,
        timeout_inc,
        warning_count,
        input.duration_ms,
        changed_files_count,
        job_ids_count,
    ) {
        tracing::warn!("action session append failed: {}", err);
    }
}

fn extract_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn extract_string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn top_keys(map: &BTreeMap<String, i64>, limit: usize) -> Vec<String> {
    let mut pairs = map.iter().collect::<Vec<_>>();
    pairs.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    pairs
        .into_iter()
        .take(limit)
        .map(|(key, count)| format!("{} ({})", key, count))
        .collect()
}

pub fn decode_event(record: ActionEventRecord) -> ActionEventView {
    let changed_files =
        serde_json::from_str::<Vec<String>>(&record.changed_files_json).unwrap_or_default();
    let ids = serde_json::from_str::<Value>(&record.ids_json).unwrap_or_else(|_| json!({}));
    let summary = serde_json::from_str::<Value>(&record.summary_json).unwrap_or_else(|_| json!({}));
    ActionEventView {
        event_id: record.event_id,
        session_id: record.session_id,
        started_at: record.started_at,
        ended_at: record.ended_at,
        duration_ms: record.duration_ms,
        endpoint: record.endpoint,
        operation: record.operation,
        action_name: record.action_name,
        project: record.project,
        status: record.status,
        http_status: record.http_status,
        error_summary: record.error_summary,
        warning_summary: record.warning_summary,
        changed_files,
        ids,
        summary,
        request_bytes: record.request_bytes,
        response_bytes: record.response_bytes,
    }
}

pub fn compute_stats(events: &[ActionEventView]) -> ActionSessionStats {
    let mut by_endpoint = BTreeMap::new();
    let mut by_project = BTreeMap::new();
    let mut by_status = BTreeMap::new();
    let mut changed_files = BTreeSet::new();
    let mut job_ids = BTreeSet::new();
    let mut edit_count = 0;
    let mut context_count = 0;
    let mut job_count = 0;
    let mut command_count = 0;
    let mut report_count = 0;
    let mut artifact_count = 0;
    let mut git_count = 0;
    let mut desktop_count = 0;
    for event in events {
        *by_endpoint.entry(event.endpoint.clone()).or_insert(0) += 1;
        *by_status.entry(event.status.clone()).or_insert(0) += 1;
        if let Some(project) = &event.project {
            *by_project.entry(project.clone()).or_insert(0) += 1;
        }
        for path in &event.changed_files {
            changed_files.insert(path.clone());
        }
        if let Some(job_id) = extract_string(&event.ids, "job_id") {
            job_ids.insert(job_id);
        }
        for job_id in extract_string_list(&event.ids, "job_ids") {
            job_ids.insert(job_id);
        }
        match event.endpoint.as_str() {
            "/api/codex/edit" => edit_count += 1,
            "/api/codex/context_batch" | "/api/codex/context" => context_count += 1,
            "/api/codex/job" => job_count += 1,
            "/api/codex/command"
            | "/api/codex/command_request"
            | "/api/codex/command_request_op"
            | "/api/codex/command_request_raw"
            | "/api/codex/command_request_batch"
            | "/api/codex/command_approve"
            | "/api/codex/command_reject" => command_count += 1,
            "/api/codex/report" => report_count += 1,
            "/api/codex/artifact" => artifact_count += 1,
            "/api/codex/git" => git_count += 1,
            "/api/desktop/task_op" => desktop_count += 1,
            _ => {}
        }
    }
    ActionSessionStats {
        by_endpoint,
        by_project,
        by_status,
        edit_count,
        context_count,
        job_count,
        command_count,
        report_count,
        artifact_count,
        git_count,
        desktop_count,
        changed_files_distinct_count: changed_files.len(),
        job_ids_distinct_count: job_ids.len(),
    }
}

fn session_list_item(
    db: &Database,
    session: ActionSessionRecord,
) -> anyhow::Result<ActionSessionListItem> {
    let mut events = db
        .list_action_events(&session.session_id, 200)?
        .into_iter()
        .map(decode_event)
        .collect::<Vec<_>>();
    events.reverse();
    let stats = compute_stats(&events);
    let top_endpoints = top_keys(&stats.by_endpoint, 3);
    let top_projects = top_keys(&stats.by_project, 3);
    Ok(ActionSessionListItem {
        session,
        stats,
        top_endpoints,
        top_projects,
    })
}

#[handler]
pub async fn codex_action_sessions(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(db) = get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database not configured",
        ));
        return;
    };
    let body: ActionSessionOpRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ActionSessionOpResponse {
                success: false,
                op: "unknown".to_string(),
                sessions: Vec::new(),
                session: None,
                stats: None,
                events: Vec::new(),
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let limit = body.limit.unwrap_or(50);
    match body.op.as_str() {
        "list" | "stats" => match db.list_action_sessions(body.status.as_deref(), limit) {
            Ok(sessions) => {
                let items = sessions
                    .into_iter()
                    .filter_map(|session| session_list_item(&db, session).ok())
                    .collect::<Vec<_>>();
                res.render(Json(ActionSessionOpResponse {
                    success: true,
                    op: body.op,
                    sessions: items,
                    session: None,
                    stats: None,
                    events: Vec::new(),
                    error: None,
                }));
            }
            Err(e) => res.render(Json(ActionSessionOpResponse {
                success: false,
                op: body.op,
                sessions: Vec::new(),
                session: None,
                stats: None,
                events: Vec::new(),
                error: Some(format!("Failed to list action sessions: {}", e)),
            })),
        },
        "get" | "events" => {
            let Some(session_id) = body.session_id else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(ActionSessionOpResponse {
                    success: false,
                    op: body.op,
                    sessions: Vec::new(),
                    session: None,
                    stats: None,
                    events: Vec::new(),
                    error: Some("session_id is required".to_string()),
                }));
                return;
            };
            match db.get_action_session(&session_id) {
                Ok(Some(session)) => {
                    let mut events = match db.list_action_events(&session_id, limit.max(200)) {
                        Ok(events) => events.into_iter().map(decode_event).collect::<Vec<_>>(),
                        Err(e) => {
                            res.render(Json(ActionSessionOpResponse {
                                success: false,
                                op: body.op,
                                sessions: Vec::new(),
                                session: None,
                                stats: None,
                                events: Vec::new(),
                                error: Some(format!("Failed to load events: {}", e)),
                            }));
                            return;
                        }
                    };
                    events.reverse();
                    let stats = compute_stats(&events);
                    res.render(Json(ActionSessionOpResponse {
                        success: true,
                        op: body.op,
                        sessions: Vec::new(),
                        session: Some(session),
                        stats: Some(stats),
                        events,
                        error: None,
                    }));
                }
                Ok(None) => {
                    res.status_code(StatusCode::NOT_FOUND);
                    res.render(Json(ActionSessionOpResponse {
                        success: false,
                        op: body.op,
                        sessions: Vec::new(),
                        session: None,
                        stats: None,
                        events: Vec::new(),
                        error: Some("Session not found".to_string()),
                    }));
                }
                Err(e) => res.render(Json(ActionSessionOpResponse {
                    success: false,
                    op: body.op,
                    sessions: Vec::new(),
                    session: None,
                    stats: None,
                    events: Vec::new(),
                    error: Some(format!("Failed to load session: {}", e)),
                })),
            }
        }
        "close" => {
            let Some(session_id) = body.session_id else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(ActionSessionOpResponse {
                    success: false,
                    op: body.op,
                    sessions: Vec::new(),
                    session: None,
                    stats: None,
                    events: Vec::new(),
                    error: Some("session_id is required".to_string()),
                }));
                return;
            };
            match db.close_action_session(&session_id, chrono::Utc::now().timestamp()) {
                Ok(Some(session)) => res.render(Json(ActionSessionOpResponse {
                    success: true,
                    op: body.op,
                    sessions: Vec::new(),
                    session: Some(session),
                    stats: None,
                    events: Vec::new(),
                    error: None,
                })),
                Ok(None) => {
                    res.status_code(StatusCode::NOT_FOUND);
                    res.render(Json(ActionSessionOpResponse {
                        success: false,
                        op: body.op,
                        sessions: Vec::new(),
                        session: None,
                        stats: None,
                        events: Vec::new(),
                        error: Some("Session not found".to_string()),
                    }));
                }
                Err(e) => res.render(Json(ActionSessionOpResponse {
                    success: false,
                    op: body.op,
                    sessions: Vec::new(),
                    session: None,
                    stats: None,
                    events: Vec::new(),
                    error: Some(format!("Failed to close session: {}", e)),
                })),
            }
        }
        "rename" => {
            let Some(session_id) = body.session_id else {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(ActionSessionOpResponse {
                    success: false,
                    op: body.op,
                    sessions: Vec::new(),
                    session: None,
                    stats: None,
                    events: Vec::new(),
                    error: Some("session_id is required".to_string()),
                }));
                return;
            };
            match db.update_action_session_metadata(
                &session_id,
                body.title.as_deref(),
                body.note.as_deref(),
                chrono::Utc::now().timestamp(),
            ) {
                Ok(Some(session)) => res.render(Json(ActionSessionOpResponse {
                    success: true,
                    op: body.op,
                    sessions: Vec::new(),
                    session: Some(session),
                    stats: None,
                    events: Vec::new(),
                    error: None,
                })),
                Ok(None) => {
                    res.status_code(StatusCode::NOT_FOUND);
                    res.render(Json(ActionSessionOpResponse {
                        success: false,
                        op: body.op,
                        sessions: Vec::new(),
                        session: None,
                        stats: None,
                        events: Vec::new(),
                        error: Some("Session not found".to_string()),
                    }));
                }
                Err(e) => res.render(Json(ActionSessionOpResponse {
                    success: false,
                    op: body.op,
                    sessions: Vec::new(),
                    session: None,
                    stats: None,
                    events: Vec::new(),
                    error: Some(format!("Failed to update session: {}", e)),
                })),
            }
        }
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ActionSessionOpResponse {
                success: false,
                op: body.op,
                sessions: Vec::new(),
                session: None,
                stats: None,
                events: Vec::new(),
                error: Some("unsupported action session op".to_string()),
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, Database) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("drop.db")).unwrap();
        (tmp, db)
    }

    #[test]
    fn session_reuses_recent_open_session() {
        let (_tmp, db) = test_db();
        let now = 1000;
        let first = get_or_create_active_session(&db, None, Some("first"), now).unwrap();
        let second = get_or_create_active_session(&db, None, None, now + 60).unwrap();
        assert_eq!(first.session_id, second.session_id);
    }

    #[test]
    fn closed_session_is_not_reused() {
        let (_tmp, db) = test_db();
        let now = 1000;
        let first = get_or_create_active_session(&db, None, None, now).unwrap();
        db.close_action_session(&first.session_id, now + 10)
            .unwrap();
        let second = get_or_create_active_session(&db, None, None, now + 20).unwrap();
        assert_ne!(first.session_id, second.session_id);
    }

    #[test]
    fn sanitizer_redacts_secrets_and_logs() {
        let value = json!({
            "api_key": "sk-secret",
            "stdout_tail": "do not keep",
            "stderr_tail": "do not keep",
            "diff": "do not keep",
            "base64_content": "Zm9v",
            "command_text": "printf 'ok'\nexport TOKEN=abc",
            "script_text": "echo hello\nexport TOKEN=abc",
            "nested": {"authorization": "Bearer abc", "ok": "hello"}
        });
        let sanitized = sanitize_value(&value);
        assert_eq!(sanitized["api_key"], "[redacted]");
        assert!(sanitized.get("stdout_tail").is_none());
        assert!(sanitized.get("stderr_tail").is_none());
        assert!(sanitized.get("diff").is_none());
        assert!(sanitized.get("base64_content").is_none());
        assert_eq!(sanitized["command_text"]["command_kind"], "command_text");
        assert!(sanitized["command_text"]["sha256"].is_string());
        assert!(sanitized["script_text"]["sha256"].is_string());
        assert_eq!(sanitized["nested"]["authorization"], "[redacted]");
    }

    #[test]
    fn record_action_event_updates_counters() {
        let (_tmp, db_inner) = test_db();
        let db = Arc::new(db_inner);
        record_action_event(
            &db,
            ActionAuditEventInput {
                explicit_session_id: Some("session-1".to_string()),
                session_title: Some("test".to_string()),
                endpoint: "/api/codex/edit".to_string(),
                action_name: "applyProjectEdit".to_string(),
                operation: Some("replace_text".to_string()),
                project: Some("demo".to_string()),
                status: "success".to_string(),
                http_status: Some(200),
                started_at: 100,
                ended_at: 101,
                duration_ms: 1,
                error_summary: None,
                warning_summary: Some("diff truncated".to_string()),
                changed_files: vec!["a.txt".to_string()],
                ids: json!({"job_ids": ["job-1"]}),
                summary: json!({"response_mode": "summary"}),
                request_bytes: Some(10),
                response_bytes: Some(20),
            },
        );
        let session = db.get_action_session("session-1").unwrap().unwrap();
        assert_eq!(session.total_actions, 1);
        assert_eq!(session.success_count, 1);
        assert_eq!(session.warning_count, 1);
        assert_eq!(session.changed_files_count, 1);
        assert_eq!(session.job_ids_count, 1);
        let events = db.list_action_events("session-1", 10).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn audited_action_routes_cover_expected_business_endpoints() {
        for route in [
            "/api/codex/projects",
            "/api/codex/context_batch",
            "/api/codex/edit",
            "/api/codex/artifact",
            "/api/codex/git",
            "/api/codex/command",
            "/api/codex/command_request_op",
            "/api/codex/job",
            "/api/codex/check",
            "/api/codex/report",
            "/api/desktop/task_op",
        ] {
            assert!(
                is_audited_action_route(route),
                "missing audited route: {route}"
            );
        }
    }

    #[test]
    fn stats_smoke_test_tracks_multiple_endpoint_categories() {
        let (_tmp, db_inner) = test_db();
        let db = Arc::new(db_inner);
        record_action_event(
            &db,
            ActionAuditEventInput {
                explicit_session_id: Some("session-smoke".to_string()),
                session_title: None,
                endpoint: "/api/codex/context_batch".to_string(),
                action_name: "getProjectContextBatch".to_string(),
                operation: Some("read_file,git_status".to_string()),
                project: Some("demo".to_string()),
                status: "success".to_string(),
                http_status: Some(200),
                started_at: 10,
                ended_at: 11,
                duration_ms: 50,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({
                    "modes": ["read_file", "git_status"],
                    "count": 2
                }),
                request_bytes: None,
                response_bytes: None,
            },
        );
        record_action_event(
            &db,
            ActionAuditEventInput {
                explicit_session_id: Some("session-smoke".to_string()),
                session_title: None,
                endpoint: "/api/codex/command_request_op".to_string(),
                action_name: "runCommandRequestOp".to_string(),
                operation: Some("create_trusted_raw_and_approve".to_string()),
                project: Some("demo".to_string()),
                status: "success".to_string(),
                http_status: Some(200),
                started_at: 12,
                ended_at: 13,
                duration_ms: 60,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({"request_id": "req-1"}),
                summary: json!({
                    "command_text": "echo secret",
                    "script_text": "cat .env",
                    "stdout_tail": "do not store",
                    "stderr_tail": "do not store",
                }),
                request_bytes: None,
                response_bytes: None,
            },
        );
        let session = db.get_action_session("session-smoke").unwrap().unwrap();
        assert_eq!(session.total_actions, 2);
        let mut events = db
            .list_action_events("session-smoke", 10)
            .unwrap()
            .into_iter()
            .map(decode_event)
            .collect::<Vec<_>>();
        events.reverse();
        let stats = compute_stats(&events);
        assert_eq!(stats.by_endpoint["/api/codex/context_batch"], 1);
        assert_eq!(stats.by_endpoint["/api/codex/command_request_op"], 1);
        assert_eq!(stats.context_count, 1);
        assert_eq!(stats.command_count, 1);
        let command_event = events
            .iter()
            .find(|event| event.endpoint == "/api/codex/command_request_op")
            .unwrap();
        assert!(command_event.summary.get("stdout_tail").is_none());
        assert!(command_event.summary.get("stderr_tail").is_none());
        assert!(command_event.summary["command_text"]["sha256"].is_string());
        assert!(command_event.summary["script_text"]["sha256"].is_string());
        assert_ne!(
            command_event.summary["command_text"],
            Value::String("echo secret".to_string())
        );
    }
}
