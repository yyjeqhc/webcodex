use crate::action_audit_sessions::{
    record_action_event, request_action_session_id, ActionAuditEventInput,
};
use crate::get_db;
use salvo::prelude::*;
use serde_json::{json, Value};

pub struct ActionAudit {
    db: Option<std::sync::Arc<crate::Database>>,
    explicit_session_id: Option<String>,
    endpoint: &'static str,
    action_name: &'static str,
    started_at: i64,
}

impl ActionAudit {
    pub fn start(
        req: &Request,
        depot: &Depot,
        endpoint: &'static str,
        action_name: &'static str,
    ) -> Self {
        Self {
            db: get_db(depot),
            explicit_session_id: request_action_session_id(req),
            endpoint,
            action_name,
            started_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn record(&self, event: ActionAuditRecord) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        let ended_at = chrono::Utc::now().timestamp();
        record_action_event(
            db,
            ActionAuditEventInput {
                explicit_session_id: self.explicit_session_id.clone(),
                session_title: None,
                endpoint: self.endpoint.to_string(),
                action_name: self.action_name.to_string(),
                operation: event.operation,
                project: event.project,
                status: event.status,
                http_status: Some(event.http_status.as_u16() as i64),
                started_at: self.started_at,
                ended_at,
                duration_ms: (ended_at - self.started_at).max(0) * 1000,
                error_summary: event.error_summary,
                warning_summary: event.warning_summary,
                changed_files: event.changed_files,
                ids: event.ids,
                summary: event.summary,
                request_bytes: None,
                response_bytes: None,
            },
        );
    }
}

pub struct ActionAuditRecord {
    pub operation: Option<String>,
    pub project: Option<String>,
    pub status: String,
    pub http_status: StatusCode,
    pub error_summary: Option<String>,
    pub warning_summary: Option<String>,
    pub changed_files: Vec<String>,
    pub ids: Value,
    pub summary: Value,
}

impl ActionAuditRecord {
    pub fn new(operation: impl Into<String>, success: bool, http_status: StatusCode) -> Self {
        Self {
            operation: Some(operation.into()),
            project: None,
            status: action_status(success, http_status),
            http_status,
            error_summary: None,
            warning_summary: None,
            changed_files: Vec::new(),
            ids: json!({}),
            summary: json!({}),
        }
    }

    pub fn error(mut self, error: Option<String>) -> Self {
        self.error_summary = error;
        self
    }

    pub fn ids(mut self, ids: Value) -> Self {
        self.ids = ids;
        self
    }

    pub fn summary(mut self, summary: Value) -> Self {
        self.summary = summary;
        self
    }
}

pub fn action_status(success: bool, http_status: StatusCode) -> String {
    if success {
        return "success".to_string();
    }
    if http_status == StatusCode::REQUEST_TIMEOUT {
        "timeout".to_string()
    } else {
        "failed".to_string()
    }
}
