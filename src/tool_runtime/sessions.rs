use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) const SESSION_ID_PREFIX: &str = "wc_sess_";
const EVENT_ID_PREFIX: &str = "evt_";
const DEFAULT_MAX_SESSIONS: usize = 100;
const DEFAULT_MAX_EVENTS_PER_SESSION: usize = 200;
const DEFAULT_SUMMARY_LIMIT: usize = 50;
const MAX_SUMMARY_LIMIT: usize = 200;
const MAX_SUMMARY_STRING_CHARS: usize = 240;
const MAX_INPUT_STRING_CHARS: usize = 120;
const MAX_INPUT_OBJECT_KEYS: usize = 16;
const MAX_INPUT_ARRAY_ITEMS: usize = 8;

#[derive(Debug, Clone)]
pub(crate) struct SessionStore {
    inner: Arc<Mutex<SessionStoreInner>>,
}

#[derive(Debug)]
struct SessionStoreInner {
    sessions: HashMap<String, SessionRecord>,
    lru: VecDeque<String>,
    max_sessions: usize,
    max_events_per_session: usize,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    session_id: String,
    project: Option<String>,
    title: Option<String>,
    created_at: i64,
    updated_at: i64,
    events: VecDeque<SessionEvent>,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolCallStart {
    pub(crate) session_id: String,
    pub(crate) transport: SessionTransport,
    pub(crate) tool_name: String,
    pub(crate) project: Option<String>,
    pub(crate) risk_class: String,
    pub(crate) changed_paths: Vec<String>,
    pub(crate) started_at: i64,
    pub(crate) started_instant: Instant,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SessionTransport {
    Api,
    Mcp,
}

impl SessionTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Mcp => "mcp",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionEvent {
    pub(crate) event_id: String,
    pub(crate) session_id: String,
    pub(crate) kind: String,
    pub(crate) transport: String,
    pub(crate) tool_name: String,
    pub(crate) project: Option<String>,
    pub(crate) risk_class: String,
    pub(crate) started_at: Option<i64>,
    pub(crate) finished_at: Option<i64>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) status: Option<String>,
    pub(crate) error_kind: Option<String>,
    pub(crate) error_message_summary: Option<String>,
    pub(crate) changed_paths: Vec<String>,
    pub(crate) job_id: Option<String>,
    pub(crate) input_summary: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionCounts {
    pub(crate) tool_calls: usize,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) write_like: usize,
    pub(crate) shell_like: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionSummary {
    pub(crate) session_id: String,
    pub(crate) project: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) counts: SessionCounts,
    pub(crate) events: Vec<SessionEvent>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SESSIONS, DEFAULT_MAX_EVENTS_PER_SESSION)
    }
}

impl SessionStore {
    pub(crate) fn new(max_sessions: usize, max_events_per_session: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SessionStoreInner {
                sessions: HashMap::new(),
                lru: VecDeque::new(),
                max_sessions,
                max_events_per_session,
            })),
        }
    }

    pub(crate) fn start_session(
        &self,
        project: Option<String>,
        title: Option<String>,
    ) -> SessionSummary {
        let session_id = format!("{SESSION_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let now = now_ts();
        let record = SessionRecord {
            session_id: session_id.clone(),
            project,
            title,
            created_at: now,
            updated_at: now,
            events: VecDeque::new(),
        };
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.sessions.insert(session_id.clone(), record);
        inner.touch(&session_id);
        inner.enforce_session_bound();
        inner
            .summary(&session_id, Some(DEFAULT_SUMMARY_LIMIT))
            .expect("newly inserted session must summarize")
    }

    pub(crate) fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        inner.touch(session_id);
        inner.summary(session_id, limit)
    }

    pub(crate) fn record_tool_call_started(
        &self,
        session_id: Option<&str>,
        transport: SessionTransport,
        tool_name: &str,
        arguments: &Value,
    ) -> Option<ToolCallStart> {
        let session_id = session_id?.trim();
        if !is_valid_session_id(session_id) {
            return None;
        }
        let now = now_ts();
        let event_id = format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let project = extract_project(arguments);
        let risk_class = risk_class_for_tool(tool_name).to_string();
        let changed_paths = changed_paths_for_tool(tool_name, arguments);
        let input_summary = Some(redact_and_bound_value(arguments));
        let start = ToolCallStart {
            session_id: session_id.to_string(),
            transport,
            tool_name: tool_name.to_string(),
            project: project.clone(),
            risk_class: risk_class.clone(),
            changed_paths: changed_paths.clone(),
            started_at: now,
            started_instant: Instant::now(),
        };
        self.push_event(SessionEvent {
            event_id,
            session_id: session_id.to_string(),
            kind: "tool_call_started".to_string(),
            transport: transport.as_str().to_string(),
            tool_name: tool_name.to_string(),
            project,
            risk_class,
            started_at: Some(now),
            finished_at: None,
            duration_ms: None,
            status: None,
            error_kind: None,
            error_message_summary: None,
            changed_paths,
            job_id: None,
            input_summary,
        });
        Some(start)
    }

    pub(crate) fn record_tool_call_finished(
        &self,
        start: Option<ToolCallStart>,
        success: bool,
        output: &Value,
        error: Option<&str>,
        error_kind: Option<&str>,
    ) {
        let Some(start) = start else {
            return;
        };
        let finished_at = now_ts();
        let duration_ms = start
            .started_instant
            .elapsed()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        self.push_event(SessionEvent {
            event_id: format!("{EVENT_ID_PREFIX}{}", uuid::Uuid::new_v4().simple()),
            session_id: start.session_id,
            kind: "tool_call_finished".to_string(),
            transport: start.transport.as_str().to_string(),
            tool_name: start.tool_name,
            project: start.project,
            risk_class: start.risk_class,
            started_at: Some(start.started_at),
            finished_at: Some(finished_at),
            duration_ms: Some(duration_ms),
            status: Some(if success { "success" } else { "error" }.to_string()),
            error_kind: error.map(|_| error_kind.unwrap_or("runtime_error").to_string()),
            error_message_summary: error.map(bound_summary_string),
            changed_paths: start.changed_paths,
            job_id: extract_job_id(output),
            input_summary: None,
        });
    }

    fn push_event(&self, event: SessionEvent) {
        let mut inner = self.inner.lock().expect("session store mutex poisoned");
        let max_events_per_session = inner.max_events_per_session;
        if let Some(record) = inner.sessions.get_mut(&event.session_id) {
            record.updated_at = now_ts();
            record.events.push_back(event);
            while record.events.len() > max_events_per_session {
                record.events.pop_front();
            }
            let session_id = record.session_id.clone();
            inner.touch(&session_id);
        }
    }
}

impl SessionStoreInner {
    fn touch(&mut self, session_id: &str) {
        self.lru.retain(|id| id != session_id);
        if self.sessions.contains_key(session_id) {
            self.lru.push_back(session_id.to_string());
        }
    }

    fn enforce_session_bound(&mut self) {
        while self.sessions.len() > self.max_sessions {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            self.sessions.remove(&oldest);
        }
    }

    fn summary(&self, session_id: &str, limit: Option<usize>) -> Option<SessionSummary> {
        let record = self.sessions.get(session_id)?;
        let limit = limit
            .unwrap_or(DEFAULT_SUMMARY_LIMIT)
            .clamp(0, MAX_SUMMARY_LIMIT);
        let finished_events: Vec<&SessionEvent> = record
            .events
            .iter()
            .filter(|event| event.kind == "tool_call_finished")
            .collect();
        let counts = SessionCounts {
            tool_calls: finished_events.len(),
            succeeded: finished_events
                .iter()
                .filter(|event| event.status.as_deref() == Some("success"))
                .count(),
            failed: finished_events
                .iter()
                .filter(|event| event.status.as_deref() == Some("error"))
                .count(),
            write_like: finished_events
                .iter()
                .filter(|event| event.risk_class == "project_write")
                .count(),
            shell_like: finished_events
                .iter()
                .filter(|event| event.risk_class == "job_run")
                .count(),
        };
        let skip = record.events.len().saturating_sub(limit);
        Some(SessionSummary {
            session_id: record.session_id.clone(),
            project: record.project.clone(),
            title: record.title.clone(),
            created_at: record.created_at,
            updated_at: record.updated_at,
            counts,
            events: record.events.iter().skip(skip).cloned().collect(),
        })
    }
}

pub(crate) fn is_valid_session_id(session_id: &str) -> bool {
    session_id.starts_with(SESSION_ID_PREFIX)
        && session_id.len() > SESSION_ID_PREFIX.len()
        && session_id
            .as_bytes()
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

pub(crate) fn extract_project(value: &Value) -> Option<String> {
    value
        .as_object()
        .and_then(|obj| obj.get("project"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn risk_class_for_tool(tool_name: &str) -> &'static str {
    match tool_name {
        "read_file"
        | "git_status"
        | "git_diff"
        | "git_diff_summary"
        | "git_diff_hunks"
        | "list_files"
        | "list_project_files"
        | "search"
        | "search_project_text"
        | "show_changes"
        | "validate_patch"
        | "list_tools"
        | "list_projects"
        | "list_agents"
        | "runtime_status"
        | "job_status"
        | "job_log"
        | "job_tail"
        | "list_jobs"
        | "read_project_artifact_metadata"
        | "read_project_artifact"
        | "start_session"
        | "session_summary" => "read_only",
        "write_project_file"
        | "replace_line_range"
        | "insert_at_line"
        | "delete_line_range"
        | "apply_patch"
        | "apply_patch_checked"
        | "delete_files"
        | "delete_project_files"
        | "replace_in_file"
        | "replace_exact_block"
        | "insert_before_pattern"
        | "insert_after_pattern"
        | "save_project_artifact"
        | "git_restore_paths"
        | "discard_untracked"
        | "register_project"
        | "create_project" => "project_write",
        "run_shell" | "run_job" | "run_codex" | "cargo_fmt" | "cargo_check" | "cargo_test" => {
            "job_run"
        }
        _ => "unknown",
    }
}

pub(crate) fn changed_paths_for_tool(tool_name: &str, arguments: &Value) -> Vec<String> {
    if risk_class_for_tool(tool_name) != "project_write" {
        return Vec::new();
    }
    let Some(obj) = arguments.as_object() else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    if let Some(path) = obj.get("path").and_then(Value::as_str) {
        push_path(&mut paths, path);
    }
    if let Some(values) = obj.get("paths").and_then(Value::as_array) {
        for path in values.iter().filter_map(Value::as_str) {
            push_path(&mut paths, path);
        }
    }
    paths
}

fn push_path(paths: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if path.is_empty() || paths.iter().any(|p| p == path) {
        return;
    }
    paths.push(path.to_string());
}

fn extract_job_id(output: &Value) -> Option<String> {
    output
        .as_object()
        .and_then(|obj| obj.get("job_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn redact_and_bound_value(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut redacted = serde_json::Map::new();
            for (key, value) in obj.iter().take(MAX_INPUT_OBJECT_KEYS) {
                if is_sensitive_key(key) {
                    redacted.insert(key.clone(), Value::String("[redacted]".to_string()));
                } else {
                    redacted.insert(key.clone(), redact_and_bound_value(value));
                }
            }
            if obj.len() > MAX_INPUT_OBJECT_KEYS {
                redacted.insert("_truncated".to_string(), json!(true));
            }
            Value::Object(redacted)
        }
        Value::Array(values) => {
            let mut redacted: Vec<Value> = values
                .iter()
                .take(MAX_INPUT_ARRAY_ITEMS)
                .map(redact_and_bound_value)
                .collect();
            if values.len() > MAX_INPUT_ARRAY_ITEMS {
                redacted.push(json!({"_truncated": true}));
            }
            Value::Array(redacted)
        }
        Value::String(s) if looks_like_secret_string(s) => Value::String("[redacted]".to_string()),
        Value::String(s) => Value::String(bound_chars(s, MAX_INPUT_STRING_CHARS)),
        _ => value.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key == "authorization"
        || key == "auth"
        || key == "client_secret"
        || key == "pat"
        || key == "bearer"
}

fn looks_like_secret_string(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("bearer ")
        || value.contains("wc_pat_")
        || value.contains("wc_oat_")
        || value.contains("wc_ort_")
        || value.contains("wc_agent_")
        || value.contains("wc_acct_")
        || value.contains("wc_pair_")
        || value.contains("wc_csec_")
        || value.contains("client_secret")
}

fn bound_summary_string(value: &str) -> String {
    bound_chars(value, MAX_SUMMARY_STRING_CHARS)
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

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_store_bounds_event_limit() {
        let store = SessionStore::new(10, 3);
        let summary = store.start_session(None, None);
        for idx in 0..5 {
            let args = json!({"project": "demo", "path": format!("file{idx}.rs")});
            let start = store.record_tool_call_started(
                Some(&summary.session_id),
                SessionTransport::Api,
                "write_project_file",
                &args,
            );
            store.record_tool_call_finished(start, true, &json!({}), None, None);
        }
        let summary = store.summary(&summary.session_id, Some(50)).unwrap();
        assert_eq!(summary.events.len(), 3);
        assert_eq!(summary.counts.tool_calls, 2);
    }

    #[test]
    fn input_summary_redacts_sensitive_keys() {
        let store = SessionStore::default();
        let summary = store.start_session(None, None);
        store.record_tool_call_started(
            Some(&summary.session_id),
            SessionTransport::Api,
            "read_file",
            &json!({
                "project": "demo",
                "token": "super-secret-token",
                "command": "curl -H 'Authorization: Bearer wc_pat_never_store'"
            }),
        );
        let summary = store.summary(&summary.session_id, Some(10)).unwrap();
        assert_eq!(
            summary.events[0].input_summary.as_ref().unwrap()["token"],
            "[redacted]"
        );
        assert_eq!(
            summary.events[0].input_summary.as_ref().unwrap()["command"],
            "[redacted]"
        );
    }
}
