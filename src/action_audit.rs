use crate::action_audit_sessions::{
    record_action_event, request_action_session_id, ActionAuditEventInput,
};
use crate::auth::AuthContext;
use crate::get_db;
use salvo::prelude::*;
use serde_json::{json, Value};

pub struct ActionAudit {
    db: Option<std::sync::Arc<crate::Database>>,
    explicit_session_id: Option<String>,
    principal_kind: Option<String>,
    principal_user_id: Option<String>,
    oauth_client_id: Option<String>,
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
        let (principal_kind, principal_user_id, oauth_client_id) =
            action_principal_attribution(depot.obtain::<AuthContext>().ok());
        Self {
            db: get_db(depot),
            explicit_session_id: request_action_session_id(req),
            principal_kind,
            principal_user_id,
            oauth_client_id,
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
                principal_kind: self.principal_kind.clone(),
                principal_user_id: self.principal_user_id.clone(),
                oauth_client_id: self.oauth_client_id.clone(),
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

fn action_principal_attribution(
    auth: Option<&AuthContext>,
) -> (Option<String>, Option<String>, Option<String>) {
    let Some(auth) = auth else {
        return (None, None, None);
    };
    let oauth_client_id = auth
        .is_oauth_token()
        .then(|| auth.allowed_client_id.clone())
        .flatten();
    (
        Some(auth.principal_kind().to_string()),
        auth.user_id.clone(),
        oauth_client_id,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthContext, AuthKind};

    #[test]
    fn agent_allowed_client_id_is_not_oauth_client_attribution() {
        let mut auth = AuthContext::new(AuthKind::AgentToken);
        auth.user_id = Some("user-1".to_string());
        auth.allowed_client_id = Some("runner-1".to_string());

        let (principal_kind, principal_user_id, oauth_client_id) =
            action_principal_attribution(Some(&auth));

        assert_eq!(principal_kind.as_deref(), Some("agent_token"));
        assert_eq!(principal_user_id.as_deref(), Some("user-1"));
        assert_eq!(oauth_client_id, None);
    }
}
